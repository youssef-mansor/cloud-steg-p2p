#!/usr/bin/env python3
"""
Advanced Multi-threaded Load Balancing Test with:
- Network latency tracking
- Automated server failure/recovery
- Steganography endpoint testing
"""
import concurrent.futures
import requests
import time
import json
import os
import sys
import subprocess
import signal
import shlex
import re
from pathlib import Path
from collections import defaultdict
from threading import Lock
from datetime import datetime

# Configuration
TOTAL_REQUESTS = 10000
NUM_THREADS = 20  # Number of concurrent threads
REQUESTS_PER_THREAD = TOTAL_REQUESTS // NUM_THREADS
MAX_RETRIES = 4
TIMEOUT = 30  # Increased for steganography processing

# Failure schedule (percentage-based, scales with TOTAL_REQUESTS)
NODE2_FAILURE_PERCENT = 0.25  # Node 2 fails at 25% of total requests
NODE1_FAILURE_PERCENT = 0.6  # Node 1 fails at 75% of total requests
RECOVERY_AFTER_SECONDS = 10  # Restart server after this many seconds

# Calculate absolute failure points based on percentages
NODE2_FAILURE_AT = int(TOTAL_REQUESTS * NODE2_FAILURE_PERCENT)
NODE1_FAILURE_AT = int(TOTAL_REQUESTS * NODE1_FAILURE_PERCENT)

PROJECT_DIR = os.path.dirname(os.path.abspath(__file__))

OUTPUT_DIR = f"/tmp/load_balance_test_{os.getpid()}"
RESULTS_FILE = f"{OUTPUT_DIR}/results.txt"
SUMMARY_FILE = f"{OUTPUT_DIR}/summary.txt"
FAILURE_LOG_FILE = f"{OUTPUT_DIR}/failures.txt"

# Thread-safe counters
counters_lock = Lock()
node_counts = defaultdict(int)
success_count = [0]
failure_count = [0]
response_times = []  # Track all response times
progress_counter = [0]
completed_requests = [0]  # Track actually completed requests (more accurate than request_num)

# Server management
server_processes = {}  # node_id -> subprocess.Popen
server_lock = Lock()
failure_events = []  # List of (timestamp, node_id, event_type, request_num)
request_counter = [0]  # Global request counter for failure timing

def get_server_command(node_id):
    """Get the command to start a server node"""
    peers = {
        1: "2=127.0.0.1:7002,3=127.0.0.1:7003",
        2: "1=127.0.0.1:7001,3=127.0.0.1:7003",
        3: "1=127.0.0.1:7001,2=127.0.0.1:7002"
    }
    
    http_port = 8000 + node_id
    rpc_port = 7000 + node_id
    
    cmd = [
        "cargo", "run", "--release", "--",
        "--id", str(node_id),
        "--http-addr", f"0.0.0.0:{http_port}",
        "--rpc-addr", f"0.0.0.0:{rpc_port}",
        "--peers", peers[node_id]
    ]
    
    return cmd

def find_server_pid(node_id):
    """Find PID of running server node by checking processes and verifying"""
    http_port = 8000 + node_id
    rpc_port = 7000 + node_id
    
    # Try lsof to find processes using the HTTP port
    try:
        result = subprocess.run(
            ['lsof', '-ti', f':{http_port}'],
            capture_output=True,
            text=True,
            timeout=2
        )
        if result.returncode == 0 and result.stdout.strip():
            pids = result.stdout.strip().split('\n')
            # Check each PID to verify it's actually using both ports
            for pid_str in pids:
                try:
                    pid = int(pid_str.strip())
                    # Verify it's also using the RPC port (more reliable check)
                    rpc_result = subprocess.run(
                        ['lsof', '-ti', f':{rpc_port}'],
                        capture_output=True,
                        text=True,
                        timeout=1
                    )
                    if rpc_result.returncode == 0 and pid_str.strip() in rpc_result.stdout:
                        # Also verify it's a cargo/rust process (not our test script)
                        try:
                            cmdline_result = subprocess.run(
                                ['ps', '-p', str(pid), '-o', 'command='],
                                capture_output=True,
                                text=True,
                                timeout=1
                            )
                            if cmdline_result.returncode == 0:
                                cmdline = cmdline_result.stdout.lower()
                                if 'cargo' in cmdline or 'raft-openraft' in cmdline or 'target/release' in cmdline:
                                    return pid
                        except:
                            pass
                        # If we can't verify command, but ports match, assume it's the server
                        return pid
                except ValueError:
                    continue
    except Exception as e:
        pass
    
    return None

def start_server(node_id, force=False):
    """Start a server node
    
    Args:
        node_id: Node ID to start
        force: If True, kill existing server first, then start new one
    """
    with server_lock:
        # Check if server is already running
        pid = find_server_pid(node_id)
        if pid and not force:
            print(f"✅ Node {node_id} already running (PID: {pid})")
            return
        
        # If force and server exists, kill it first
        if pid and force:
            print(f"🔄 Stopping existing Node {node_id} before restart...")
            stop_server(node_id)
            time.sleep(1)
        
        print(f"🚀 Starting Node {node_id}...")
        cmd = get_server_command(node_id)
        
        # Start process in background with output redirected
        log_file = open(f"{OUTPUT_DIR}/node_{node_id}.log", 'w')
        process = subprocess.Popen(
            cmd,
            cwd=PROJECT_DIR,
            stdout=log_file,
            stderr=subprocess.STDOUT,
            preexec_fn=os.setsid if hasattr(os, 'setsid') and os.name != 'nt' else None
        )
        # Note: We don't close log_file - let the process handle it
        
        server_processes[node_id] = process
        
        # Wait for server to be ready (check HTTP endpoint)
        max_wait = 30
        waited = 0
        while waited < max_wait:
            try:
                response = requests.get(f"http://127.0.0.1:{8000 + node_id}/metrics", timeout=2)
                if response.status_code == 200:
                    print(f"✅ Node {node_id} started successfully (PID: {process.pid})")
                    failure_events.append((
                        time.time(),
                        node_id,
                        "STARTED",
                        completed_requests[0]
                    ))
                    return
            except:
                pass
            time.sleep(0.5)
            waited += 0.5
        
        print(f"⚠️  Node {node_id} may not be fully ready, continuing anyway...")

def stop_server(node_id):
    """Stop a server node (either managed or external)"""
    with server_lock:
        # Never kill our own process or parent
        our_pid = os.getpid()
        parent_pid = os.getppid()
        
        pid = None
        
        # Check if we're managing this process
        if node_id in server_processes:
            process = server_processes[node_id]
            pid = process.pid
            del server_processes[node_id]
        else:
            # Try to find external process
            pid = find_server_pid(node_id)
        
        if not pid:
            # Already stopped - verify
            if not find_server_pid(node_id):
                print(f"✅ Node {node_id} is already stopped")
            return
        
        # Safety check: never kill our own process or parent
        if pid == our_pid or pid == parent_pid:
            print(f"⚠️  Safety check: PID {pid} is our own process or parent, skipping stop")
            return
        
        # Double-check this PID is actually using the right port
        http_port = 8000 + node_id
        try:
            verify_result = subprocess.run(
                ['lsof', '-ti', f':{http_port}'],
                capture_output=True,
                text=True,
                timeout=1
            )
            if str(pid) not in verify_result.stdout:
                print(f"⚠️  PID {pid} is not using port {http_port}, skipping stop")
                return
        except:
            pass
        
        print(f"🛑 Stopping Node {node_id} (PID: {pid})...")
        
        # Try graceful shutdown first
        killed = False
        try:
            # Use SIGTERM first
            try:
                os.kill(pid, signal.SIGTERM)
            except ProcessLookupError:
                # Already dead
                killed = True
            
            if not killed:
                # Wait for process to terminate
                for _ in range(10):
                    try:
                        os.kill(pid, 0)  # Check if process exists
                        time.sleep(0.5)
                    except ProcessLookupError:
                        killed = True
                        break
                
                # Force kill if still running
                if not killed:
                    try:
                        os.kill(pid, signal.SIGKILL)
                        time.sleep(0.5)
                        # Check again
                        try:
                            os.kill(pid, 0)
                        except ProcessLookupError:
                            killed = True
                    except ProcessLookupError:
                        killed = True
        except Exception as e:
            print(f"⚠️  Error stopping Node {node_id}: {e}")
            return
        
        # Verify it's stopped by checking port
        time.sleep(0.5)  # Give it a moment
        if not find_server_pid(node_id):
            print(f"✅ Node {node_id} stopped successfully")
            failure_events.append((
                time.time(),
                node_id,
                "STOPPED",
                completed_requests[0]
            ))
        else:
            print(f"⚠️  Node {node_id} may still be running - port still in use")

def server_manager_thread():
    """Background thread that manages server failures and recoveries"""
    global request_counter, completed_requests
    
    node_2_failed = False
    node_1_failed = False
    manager_running = True
    
    # Wait for initial requests to process
    time.sleep(5)
    
    while manager_running and completed_requests[0] < TOTAL_REQUESTS:
        current_completed = completed_requests[0]
        
        # Fail Node 2 at NODE2_FAILURE_PERCENT of total requests (only once)
        if not node_2_failed and current_completed >= NODE2_FAILURE_AT:
            node_2_failed = True
            print(f"\n💥 Automatically stopping Node 2 after {current_completed} completed requests ({current_completed*100//TOTAL_REQUESTS}% of total)...")
            stop_server(2)
            
            # Schedule restart
            restart_time = time.time() + RECOVERY_AFTER_SECONDS
            while time.time() < restart_time and completed_requests[0] < TOTAL_REQUESTS:
                time.sleep(0.5)
            
            if completed_requests[0] < TOTAL_REQUESTS:
                print(f"🔄 Automatically restarting Node 2 after {RECOVERY_AFTER_SECONDS}s downtime...")
                start_server(2, force=True)  # Force restart even if PID exists
                
                # Wait for server to actually be ready
                max_wait = 30
                waited = 0
                while waited < max_wait:
                    if find_server_pid(2):
                        try:
                            response = requests.get("http://127.0.0.1:8002/metrics", timeout=2)
                            if response.status_code == 200:
                                print(f"✅ Node 2 confirmed ready")
                                break
                        except:
                            pass
                    time.sleep(0.5)
                    waited += 0.5
                
                # Wait for cluster to stabilize
                time.sleep(3)
        
        # Fail Node 1 at NODE1_FAILURE_PERCENT of total requests (only once)
        if not node_1_failed and current_completed >= NODE1_FAILURE_AT:
            node_1_failed = True
            print(f"\n💥 Automatically stopping Node 1 after {current_completed} completed requests ({current_completed*100//TOTAL_REQUESTS}% of total)...")
            stop_server(1)
            
            restart_time = time.time() + RECOVERY_AFTER_SECONDS
            while time.time() < restart_time and completed_requests[0] < TOTAL_REQUESTS:
                time.sleep(0.5)
            
            if completed_requests[0] < TOTAL_REQUESTS:
                print(f"🔄 Automatically restarting Node 1 after {RECOVERY_AFTER_SECONDS}s downtime...")
                start_server(1, force=True)  # Force restart even if PID exists
                
                # Wait for server to actually be ready
                max_wait = 30
                waited = 0
                while waited < max_wait:
                    if find_server_pid(1):
                        try:
                            response = requests.get("http://127.0.0.1:8001/metrics", timeout=2)
                            if response.status_code == 200:
                                print(f"✅ Node 1 confirmed ready")
                                break
                        except:
                            pass
                    time.sleep(0.5)
                    waited += 0.5
                
                time.sleep(3)
        
        # Exit if test is done
        if current_completed >= TOTAL_REQUESTS:
            manager_running = False
            break
            
        time.sleep(0.5)  # Check more frequently
    
    # Wait for any in-progress restarts to complete
    print("\n⏳ Waiting for any server restarts to complete...")
    time.sleep(2)
    manager_running = False

def multicast_request(request_num):
    """Multicast request to all 3 nodes with retry logic and latency tracking"""
    ports = [8001, 8002, 8003]
    max_retries = MAX_RETRIES
    retry_count = 0
    
    # Check if input image exists
    image_path = "input-image.png"
    if not os.path.exists(image_path):
        print(f"❌ Error: {image_path} not found")
        return None, None, None, retry_count, []
    
    while retry_count <= max_retries:
        request_start_time = time.time()
        responses = []
        all_codes = []
        response_times_per_attempt = []
        
        # Multicast to all 3 nodes in parallel
        with concurrent.futures.ThreadPoolExecutor(max_workers=3) as executor:
            future_to_port = {}
            for port in ports:
                url = f"http://127.0.0.1:{port}/image/steg"  # Changed to steganography endpoint
                # Use a function factory to properly capture port and url
                def make_request_for_port(p, u, img):
                    return make_request(p, u, img)
                future = executor.submit(make_request_for_port, port, url, image_path)
                future_to_port[future] = port
            
            for future in concurrent.futures.as_completed(future_to_port):
                port = future_to_port[future]
                try:
                    result = future.result(timeout=TIMEOUT)
                    if result:
                        response_time, status_code, processed_by = result
                        responses.append((port, status_code, processed_by))
                        all_codes.append(status_code)
                        response_times_per_attempt.append(response_time)
                    else:
                        responses.append((port, 0, ''))
                        all_codes.append(0)
                        response_times_per_attempt.append(None)
                except Exception as e:
                    responses.append((port, 0, ''))
                    all_codes.append(0)
                    response_times_per_attempt.append(None)
        
        total_time = time.time() - request_start_time
        
        # Find first successful response (200)
        for (port, code, processed_by), rt in zip(responses, response_times_per_attempt):
            if code == 200:
                return code, processed_by, rt, retry_count, all_codes
        
        # If all failed and we have retries left, wait and retry
        if retry_count < max_retries:
            retry_count += 1
            time.sleep(0.2 + retry_count * 0.3)  # Progressive delay
        else:
            break
    
    return None, None, None, retry_count, all_codes

def make_request(port, url, image_path):
    """Make a single HTTP request and return timing info"""
    try:
        start = time.time()
        with open(image_path, 'rb') as f:
            response = requests.post(
                url,
                data=f,
                timeout=TIMEOUT,
                headers={'Content-Type': 'application/octet-stream'}
            )
        response_time = time.time() - start
        
        return (
            response_time,
            response.status_code,
            response.headers.get('X-Processed-By-Node', '')
        )
    except Exception:
        return None

def worker_thread(thread_id, start_request, num_requests):
    """Worker thread that sends a batch of requests"""
    thread_results = []
    thread_response_times = []
    
    for i in range(num_requests):
        request_num = start_request + i
        
        http_code, processed_by, response_time, retries, codes = multicast_request(request_num)
        
        # Thread-safe result logging
        with counters_lock:
            # Update counters AFTER request completes
            request_counter[0] = request_num
            completed_requests[0] += 1  # Increment completed count
            
            if http_code == 200:
                success_count[0] += 1
                if processed_by:
                    try:
                        node_id = int(processed_by)
                        node_counts[node_id] += 1
                    except ValueError:
                        # Debug: log if we can't parse the header
                        if request_num <= 5:  # Only log first few for debugging
                            print(f"\n⚠️  Request {request_num}: Could not parse processed_by='{processed_by}'", flush=True)
                else:
                    # Debug: log if header is missing
                    if request_num <= 5:  # Only log first few for debugging
                        print(f"\n⚠️  Request {request_num}: Missing X-Processed-By-Node header", flush=True)
                
                # Track response time
                if response_time is not None:
                    response_times.append(response_time)
                    thread_response_times.append(response_time)
                
                thread_results.append((request_num, http_code, processed_by, retries, response_time))
            else:
                failure_count[0] += 1
                codes_str = '/'.join(map(str, codes)) if codes else 'unknown'
                thread_results.append((request_num, 0, 'failed', retries, None, codes_str))
            
            # Progress indicator
            progress_counter[0] += 1
            completed = progress_counter[0]
            
            if completed % 100 == 0:
                print('.', end='', flush=True)
            if completed % 500 == 0:
                elapsed = time.time() - start_time[0]
                print(f" {completed}/{TOTAL_REQUESTS} ({int(elapsed)}s elapsed)")
    
    return thread_results, thread_response_times

def main():
    global start_time
    
    # Create output directory
    os.makedirs(OUTPUT_DIR, exist_ok=True)
    
    # Check if input image exists
    if not os.path.exists("input-image.png"):
        print("❌ Error: input-image.png not found")
        sys.exit(1)
    
    # Check if servers are already running
    print("🔍 Checking server status...")
    servers_running = {}
    for node_id in [1, 2, 3]:
        try:
            response = requests.get(f"http://127.0.0.1:{8000 + node_id}/metrics", timeout=2)
            if response.status_code == 200:
                servers_running[node_id] = True
                pid = find_server_pid(node_id)
                if pid:
                    print(f"  ✅ Node {node_id} is running (PID: {pid})")
                else:
                    print(f"  ✅ Node {node_id} is running")
            else:
                servers_running[node_id] = False
                print(f"  ⚠️  Node {node_id} is not responding")
        except:
            servers_running[node_id] = False
            print(f"  ⚠️  Node {node_id} is not responding")
    
    # Note: We'll manage failures automatically regardless of initial state
    
    # Find leader
    try:
        response = requests.get("http://127.0.0.1:8001/metrics", timeout=5)
        data = response.json()
        leader_node = data.get('data', {}).get('current_leader')
        if not leader_node:
            print("❌ Error: Could not find leader. Make sure all 3 nodes are running!")
            sys.exit(1)
        print(f"✅ Leader is Node {leader_node}")
    except Exception as e:
        print(f"⚠️  Warning: Could not connect to Node 1: {e}")
        print("   Assuming servers are managed externally, continuing...")
    
    print(f"\n📡 Testing steganography endpoint (/image/steg)")
    print(f"   - Requests multicast to all 3 nodes (port 8001, 8002, 8003)")
    print(f"   - Followers will reject (503)")
    print(f"   - Leader will forward to healthy nodes")
    print(f"\n🎯 Failure Schedule (percentage-based):")
    print(f"   - Node 2 will fail at {NODE2_FAILURE_PERCENT*100:.0f}% ({NODE2_FAILURE_AT} requests)")
    print(f"   - Node 2 will recover after {RECOVERY_AFTER_SECONDS} seconds")
    print(f"   - Node 1 will fail at {NODE1_FAILURE_PERCENT*100:.0f}% ({NODE1_FAILURE_AT} requests)")
    print(f"   - Node 1 will recover after {RECOVERY_AFTER_SECONDS} seconds")
    print(f"\n🚀 Starting {NUM_THREADS} threads, {REQUESTS_PER_THREAD} requests per thread")
    print(f"📤 Sending {TOTAL_REQUESTS} requests concurrently...\n")
    
    start_time = [time.time()]
    
    # Always start server manager thread for automated failure testing
    manager_executor = concurrent.futures.ThreadPoolExecutor(max_workers=1)
    manager_future = manager_executor.submit(server_manager_thread)
    
    # Start all worker threads
    with concurrent.futures.ThreadPoolExecutor(max_workers=NUM_THREADS) as executor:
        futures = []
        for thread_id in range(NUM_THREADS):
            start = thread_id * REQUESTS_PER_THREAD
            num = REQUESTS_PER_THREAD
            # Last thread gets any remainder
            if thread_id == NUM_THREADS - 1:
                num = TOTAL_REQUESTS - start
            future = executor.submit(worker_thread, thread_id, start, num)
            futures.append(future)
        
        # Wait for all threads and collect results
        all_results = []
        all_response_times = []
        for future in concurrent.futures.as_completed(futures):
            results, response_times = future.result()
            all_results.extend(results)
            all_response_times.extend(response_times)
    
    # Wait for manager thread to finish any ongoing operations (restarts)
    print("\n⏳ Waiting for server manager to complete...")
    # Give it time to finish, but don't wait forever
    try:
        manager_future.result(timeout=10)
    except concurrent.futures.TimeoutError:
        print("⚠️  Manager thread timed out, forcing shutdown...")
    except Exception as e:
        print(f"⚠️  Manager thread error: {e}")
    
    # Clean up server manager
    manager_executor.shutdown(wait=False)
    
    print("\n\n")
    
    # Write results file
    with open(RESULTS_FILE, 'w') as f:
        f.write("Request#\tHTTP_Code\tProcessed_By\tRetries\tResponse_Time_sec\n")
        for result in sorted(all_results, key=lambda x: x[0]):
            if len(result) == 5:
                req_num, code, processed_by, retries, rt = result
                rt_str = f"{rt:.4f}" if rt else "N/A"
                f.write(f"{req_num}\t{code}\t{processed_by}\t{retries}\t{rt_str}\n")
            else:
                req_num, code, status, retries, rt, codes = result
                rt_str = f"{rt:.4f}" if rt else "N/A"
                f.write(f"{req_num}\t{code}\t{status}\t{retries}\t{rt_str}\tcodes:{codes}\n")
    
    # Write failure log
    with open(FAILURE_LOG_FILE, 'w') as f:
        f.write("Timestamp\tRequest#\tNode_ID\tEvent\n")
        for timestamp, node_id, event_type, req_num in sorted(failure_events, key=lambda x: x[0]):
            dt = datetime.fromtimestamp(timestamp).strftime('%Y-%m-%d %H:%M:%S.%f')[:-3]
            f.write(f"{dt}\t{req_num}\t{node_id}\t{event_type}\n")
    
    # Calculate statistics
    total_processed = sum(node_counts.values())
    if total_processed == 0:
        total_processed = 1
    
    elapsed = time.time() - start_time[0]
    
    # Latency statistics
    avg_latency = sum(response_times) / len(response_times) if response_times else 0
    min_latency = min(response_times) if response_times else 0
    max_latency = max(response_times) if response_times else 0
    
    # Calculate percentiles
    sorted_times = sorted(response_times) if response_times else []
    p50 = sorted_times[len(sorted_times) // 2] if sorted_times else 0
    p95 = sorted_times[int(len(sorted_times) * 0.95)] if sorted_times else 0
    p99 = sorted_times[int(len(sorted_times) * 0.99)] if sorted_times else 0
    
    # Generate summary
    summary_lines = [
        "📊 Advanced Load Balancing Test Summary",
        "========================================",
        "",
        f"Total Requests Sent: {TOTAL_REQUESTS}",
        f"Successful Requests: {success_count[0]}",
        f"Failed Requests: {failure_count[0]}",
        "",
        "Network Latency Statistics:",
        "---------------------------",
        f"  Average Response Time: {avg_latency:.4f} seconds",
        f"  Minimum Response Time: {min_latency:.4f} seconds",
        f"  Maximum Response Time: {max_latency:.4f} seconds",
        f"  Median (P50): {p50:.4f} seconds",
        f"  95th Percentile (P95): {p95:.4f} seconds",
        f"  99th Percentile (P99): {p99:.4f} seconds",
        "",
        "Architecture:",
        f"  • {NUM_THREADS} concurrent threads",
        f"  • {REQUESTS_PER_THREAD} requests per thread",
        "  • Requests multicast to all 3 nodes (port 8001, 8002, 8003)",
        "  • Testing: /image/steg (steganography endpoint)",
        "  • Followers reject direct requests (503)",
        "  • Leader forwards to healthy nodes only",
        f"  • Client-side retry: multicast again on failure (up to {MAX_RETRIES} retries)",
        f"  • {TIMEOUT}-second timeout on client requests",
        "",
        "Automated Failure Testing:",
        "--------------------------",
    ]
    
    # Add failure events
    if failure_events:
        summary_lines.append("  Failure/Recovery Timeline:")
        for timestamp, node_id, event_type, req_num in sorted(failure_events, key=lambda x: x[0]):
            dt = datetime.fromtimestamp(timestamp).strftime('%H:%M:%S')
            summary_lines.append(f"    [{dt}] Request #{req_num}: Node {node_id} {event_type}")
    else:
        summary_lines.append("  No automated failures (servers managed externally)")
    
    summary_lines.extend([
        "",
        "Requests Processed by Each Node:",
        "---------------------------------"
    ])
    
    for node_id in [1, 2, 3]:
        count = node_counts[node_id]
        percentage = (count * 100.0 / total_processed) if total_processed > 0 else 0
        summary_lines.append(f"  Node {node_id}: {count} requests ({percentage:.2f}%)")
    
    summary_lines.extend([
        "",
        f"Total Time: {int(elapsed)} seconds",
        f"Average Throughput: {TOTAL_REQUESTS / elapsed:.2f} requests/second",
        "",
        f"Detailed results saved to: {RESULTS_FILE}",
        f"Failure log saved to: {FAILURE_LOG_FILE}"
    ])
    
    summary_text = '\n'.join(summary_lines)
    
    # Print and save summary
    print(summary_text)
    with open(SUMMARY_FILE, 'w') as f:
        f.write(summary_text)
    
    print(f"\n✅ Test Complete!")
    print(f"Summary saved to: {SUMMARY_FILE}")
    print(f"Detailed results saved to: {RESULTS_FILE}")
    print(f"Failure events saved to: {FAILURE_LOG_FILE}\n")

if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        print("\n\n⚠️  Test interrupted by user")
        # Clean up any managed servers
        with server_lock:
            for node_id, process in list(server_processes.items()):
                try:
                    os.killpg(os.getpgid(process.pid), signal.SIGTERM)
                except:
                    pass
        sys.exit(1)

