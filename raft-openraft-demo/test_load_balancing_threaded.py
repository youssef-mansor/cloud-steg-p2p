#!/usr/bin/env python3
"""
Multi-threaded load balancing test client
Each thread sends requests concurrently for maximum throughput
"""
import concurrent.futures
import requests
import time
import json
import os
import sys
from pathlib import Path
from collections import defaultdict
from threading import Lock

# Configuration
TOTAL_REQUESTS = 10000
NUM_THREADS = 20  # Number of concurrent threads
REQUESTS_PER_THREAD = TOTAL_REQUESTS // NUM_THREADS
MAX_RETRIES = 4
TIMEOUT = 10

OUTPUT_DIR = f"/tmp/load_balance_test_{os.getpid()}"
RESULTS_FILE = f"{OUTPUT_DIR}/results.txt"
SUMMARY_FILE = f"{OUTPUT_DIR}/summary.txt"

# Thread-safe counters
counters_lock = Lock()
node_counts = defaultdict(int)
success_count = [0]
failure_count = [0]

def multicast_request(request_num):
    """Multicast request to all 3 nodes with retry logic"""
    ports = [8001, 8002, 8003]
    max_retries = MAX_RETRIES
    retry_count = 0
    
    # Check if input image exists
    image_path = "input-image.png"
    if not os.path.exists(image_path):
        print(f"❌ Error: {image_path} not found")
        return None, None
    
    while retry_count <= max_retries:
        responses = []
        all_codes = []
        
        # Multicast to all 3 nodes in parallel
        with concurrent.futures.ThreadPoolExecutor(max_workers=3) as executor:
            future_to_port = {}
            for port in ports:
                url = f"http://127.0.0.1:{port}/image/echo"
                future = executor.submit(
                    requests.post,
                    url,
                    data=open(image_path, 'rb'),
                    timeout=TIMEOUT,
                    headers={'Content-Type': 'application/octet-stream'}
                )
                future_to_port[future] = port
            
            for future in concurrent.futures.as_completed(future_to_port):
                port = future_to_port[future]
                try:
                    response = future.result()
                    responses.append((port, response.status_code, response.headers.get('X-Processed-By-Node', '')))
                    all_codes.append(response.status_code)
                except Exception as e:
                    responses.append((port, 0, ''))
                    all_codes.append(0)
        
        # Find first successful response (200)
        for port, code, processed_by in responses:
            if code == 200:
                return code, processed_by, retry_count, all_codes
        
        # If all failed and we have retries left, wait and retry
        if retry_count < max_retries:
            retry_count += 1
            time.sleep(0.2 + retry_count * 0.3)  # Progressive delay
        else:
            break
    
    return None, None, retry_count, all_codes

def worker_thread(thread_id, start_request, num_requests):
    """Worker thread that sends a batch of requests"""
    thread_results = []
    
    for i in range(num_requests):
        request_num = start_request + i
        
        http_code, processed_by, retries, codes = multicast_request(request_num)
        
        # Thread-safe result logging
        with counters_lock:
            if http_code == 200:
                success_count[0] += 1
                if processed_by:
                    try:
                        node_id = int(processed_by)
                        node_counts[node_id] += 1
                    except ValueError:
                        pass
                thread_results.append((request_num, http_code, processed_by, retries))
            else:
                failure_count[0] += 1
                codes_str = '/'.join(map(str, codes)) if codes else 'unknown'
                thread_results.append((request_num, 0, 'failed', retries, codes_str))
        
        # Progress indicator
        if request_num % 100 == 0:
            print('.', end='', flush=True)
        if request_num % 1000 == 0:
            elapsed = time.time() - start_time[0]
            print(f" {request_num}/{TOTAL_REQUESTS} ({int(elapsed)}s elapsed)")
    
    return thread_results

def main():
    global start_time
    start_time = [time.time()]
    
    # Create output directory
    os.makedirs(OUTPUT_DIR, exist_ok=True)
    
    # Check if input image exists
    if not os.path.exists("input-image.png"):
        print("❌ Error: input-image.png not found")
        sys.exit(1)
    
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
        print(f"❌ Error connecting to nodes: {e}")
        sys.exit(1)
    
    print(f"📡 Will multicast requests to all 3 nodes (port 8001, 8002, 8003)")
    print(f"   - Followers will reject (503)")
    print(f"   - Leader will forward to healthy nodes")
    print(f"\n🚀 Starting {NUM_THREADS} threads, {REQUESTS_PER_THREAD} requests per thread")
    print(f"📤 Sending {TOTAL_REQUESTS} requests concurrently...\n")
    
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
        for future in concurrent.futures.as_completed(futures):
            results = future.result()
            all_results.extend(results)
    
    print("\n\n")
    
    # Write results file
    with open(RESULTS_FILE, 'w') as f:
        for result in sorted(all_results, key=lambda x: x[0]):
            if len(result) == 4:
                f.write(f"{result[0]}\t{result[1]}\t{result[2]}\tretries:{result[3]}\n")
            else:
                f.write(f"{result[0]}\t{result[1]}\t{result[2]}\tretries:{result[3]}\tcodes:{result[4]}\n")
    
    # Calculate statistics
    total_processed = sum(node_counts.values())
    if total_processed == 0:
        total_processed = 1
    
    elapsed = time.time() - start_time[0]
    
    # Generate summary
    summary_lines = [
        "📊 Load Balancing Test Summary",
        "==============================",
        "",
        f"Total Requests Sent: {TOTAL_REQUESTS}",
        f"Successful Requests: {success_count[0]}",
        f"Failed Requests: {failure_count[0]}",
        "",
        "Architecture:",
        f"  • {NUM_THREADS} concurrent threads",
        f"  • {REQUESTS_PER_THREAD} requests per thread",
        "  • Requests multicast to all 3 nodes (port 8001, 8002, 8003)",
        "  • Followers reject direct requests (503)",
        "  • Leader forwards to healthy nodes only",
        "  • Client-side retry: multicast again on failure (up to 2 retries)",
        "  • 5-second timeout on node-to-node forwarding",
        f"  • {TIMEOUT}-second timeout on client requests",
        "",
        "Requests Processed by Each Node:",
        "---------------------------------"
    ]
    
    for node_id in [1, 2, 3]:
        count = node_counts[node_id]
        percentage = (count * 100.0 / total_processed) if total_processed > 0 else 0
        summary_lines.append(f"  Node {node_id}: {count} requests ({percentage:.2f}%)")
    
    summary_lines.extend([
        "",
        f"Total Time: {int(elapsed)} seconds",
        f"Average Throughput: {TOTAL_REQUESTS / elapsed:.2f} requests/second",
        "",
        f"Detailed results saved to: {RESULTS_FILE}"
    ])
    
    summary_text = '\n'.join(summary_lines)
    
    # Print and save summary
    print(summary_text)
    with open(SUMMARY_FILE, 'w') as f:
        f.write(summary_text)
    
    print(f"\n✅ Test Complete!")
    print(f"Summary saved to: {SUMMARY_FILE}")
    print(f"Detailed results saved to: {RESULTS_FILE}\n")

if __name__ == "__main__":
    main()

