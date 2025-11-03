#!/usr/bin/env python3
"""
Auto Failover Script - Automatically fails and restarts a Raft server node
with random intervals between failures.

Usage:
    python3 auto_failover.py --node-id <1|2|3> \
        [--up <min-max>] [--down <min-max>] \
        [--min <seconds>] [--max <seconds>] \
        [--min-up <seconds>] [--max-up <seconds>] \
        [--min-down <seconds>] [--max-down <seconds>]

This script runs on each server and:
1. Finds the running server process
2. Kills it
3. Waits a random down-time period (default: 60-120 seconds)
4. Restarts the server and keeps it UP for a random up-time period (default: 60-120 seconds)
5. Repeats indefinitely
"""

import argparse
import os
import random
import signal
import subprocess
import sys
import time
from pathlib import Path
from typing import Tuple

# Server configuration based on IPs
SERVER_CONFIG = {
    1: {
        'ip': '10.40.56.135',
        'http_port': 8001,
        'rpc_port': 7001,
        'peers': '2=10.40.39.217:7002,3=10.40.46.168:7003'
    },
    2: {
        'ip': '10.40.39.217',
        'http_port': 8002,
        'rpc_port': 7002,
        'peers': '1=10.40.56.135:7001,3=10.40.46.168:7003'
    },
    3: {
        'ip': '10.40.46.168',
        'http_port': 8003,
        'rpc_port': 7003,
        'peers': '1=10.40.56.135:7001,2=10.40.39.217:7002'
    }
}

# Get project directory (parent of script directory)
PROJECT_DIR = Path(__file__).parent.absolute()


def find_server_pid(node_id):
    """Find PID of running server node by checking the HTTP port"""
    config = SERVER_CONFIG[node_id]
    http_port = config['http_port']
    
    try:
        result = subprocess.run(
            ['lsof', '-ti', f':{http_port}'],
            capture_output=True,
            text=True,
            timeout=2
        )
        if result.returncode == 0 and result.stdout.strip():
            pid = int(result.stdout.strip().split('\n')[0])
            # Verify it's also using the RPC port
            rpc_port = config['rpc_port']
            rpc_result = subprocess.run(
                ['lsof', '-ti', f':{rpc_port}'],
                capture_output=True,
                text=True,
                timeout=1
            )
            if rpc_result.returncode == 0 and str(pid) in rpc_result.stdout:
                return pid
    except Exception as e:
        pass
    
    return None


def stop_server(node_id):
    """Stop the server node by killing its process"""
    pid = find_server_pid(node_id)
    
    if not pid:
        print(f"⚠️  Node {node_id}: No server process found")
        return False
    
    # Safety check: never kill our own process
    our_pid = os.getpid()
    if pid == our_pid:
        print(f"⚠️  Safety check: PID {pid} is our own process, skipping")
        return False
    
    print(f"🛑 Stopping Node {node_id} (PID: {pid})...")
    
    try:
        # Try graceful shutdown first (SIGTERM)
        os.kill(pid, signal.SIGTERM)
        time.sleep(2)
        
        # Check if still running
        if find_server_pid(node_id):
            # Force kill if still running
            print(f"⚠️  Process still running, force killing...")
            os.kill(pid, signal.SIGKILL)
            time.sleep(1)
        
        # Verify it's stopped
        if not find_server_pid(node_id):
            print(f"✅ Node {node_id} stopped successfully")
            return True
        else:
            print(f"⚠️  Node {node_id} may still be running")
            return False
    except ProcessLookupError:
        print(f"✅ Node {node_id} already stopped")
        return True
    except Exception as e:
        print(f"❌ Error stopping Node {node_id}: {e}")
        return False


def start_server(node_id):
    """Start the server node"""
    config = SERVER_CONFIG[node_id]
    
    # Check if already running
    if find_server_pid(node_id):
        print(f"⚠️  Node {node_id} is already running")
        return True
    
    print(f"🚀 Starting Node {node_id}...")
    
    cmd = [
        "cargo", "run", "--release", "--",
        "--id", str(node_id),
        "--http-addr", f"0.0.0.0:{config['http_port']}",
        "--rpc-addr", f"0.0.0.0:{config['rpc_port']}",
        "--peers", config['peers']
    ]
    
    try:
        # Start process in background
        log_file = open(f"/tmp/auto_failover_node_{node_id}.log", 'a')
        process = subprocess.Popen(
            cmd,
            cwd=PROJECT_DIR,
            stdout=log_file,
            stderr=subprocess.STDOUT,
            preexec_fn=os.setsid if hasattr(os, 'setsid') and os.name != 'nt' else None
        )
        
        # Wait for server to be ready (check HTTP endpoint)
        max_wait = 30
        waited = 0
        http_url = f"http://{config['ip']}:{config['http_port']}/metrics"
        
        while waited < max_wait:
            try:
                import requests
                response = requests.get(http_url, timeout=2)
                if response.status_code == 200:
                    print(f"✅ Node {node_id} started successfully (PID: {process.pid})")
                    return True
            except:
                pass
            time.sleep(0.5)
            waited += 0.5
        
        print(f"⚠️  Node {node_id} may not be fully ready, continuing anyway...")
        return True
    except Exception as e:
        print(f"❌ Error starting Node {node_id}: {e}")
        return False


def _countdown(seconds: float):
    remaining = int(seconds)
    while remaining > 0:
        if remaining % 10 == 0 or remaining <= 5:
            print(f"   ⏳ {remaining} seconds remaining...")
        time.sleep(1)
        remaining -= 1


def _parse_seconds(value: str) -> float:
    """Parse a duration string with optional unit 's' or 'm' (seconds/minutes)."""
    v = value.strip().lower()
    if v.endswith('m'):
        return float(v[:-1]) * 60.0
    if v.endswith('s'):
        return float(v[:-1])
    return float(v)


def _parse_range(range_str: str) -> Tuple[float, float]:
    """Parse a range string like '60-120' with optional units 's' or 'm'."""
    parts = [p for p in range_str.replace(' ', '').split('-') if p]
    if len(parts) != 2:
        raise ValueError("Range must be in the form MIN-MAX (e.g., 60-120 or 1m-2m)")
    min_v = _parse_seconds(parts[0])
    max_v = _parse_seconds(parts[1])
    return min_v, max_v


def run_failover_loop(node_id, min_down, max_down, min_up, max_up):
    """Main loop: keep server UP for random time, then DOWN for random time, repeat"""
    print(f"🔄 Starting auto-failover for Node {node_id}")
    print(f"   Up-time range:   {min_up}-{max_up} seconds")
    print(f"   Down-time range: {min_down}-{max_down} seconds")
    print(f"   Press Ctrl+C to stop")
    print()

    # Ensure server is running to begin the cycle with UP-time
    start_server(node_id)

    cycle = 0

    try:
        while True:
            cycle += 1
            print(f"\n{'='*60}")
            print(f"Cycle {cycle} - {time.strftime('%Y-%m-%d %H:%M:%S')}")
            print(f"{'='*60}")

            # STEP A: Keep server UP for random duration
            up_time = random.uniform(min_up, max_up)
            print(f"\n[UP] Keeping server running for {up_time:.1f} seconds ({up_time/60:.2f} minutes)...")
            _countdown(up_time)

            # STEP B: Stop server and keep it DOWN for random duration
            print(f"\n[DOWN] Stopping server now...")
            stop_server(node_id)
            time.sleep(2)

            down_time = random.uniform(min_down, max_down)
            print(f"[DOWN] Keeping server down for {down_time:.1f} seconds ({down_time/60:.2f} minutes)...")
            _countdown(down_time)

            # STEP C: Restart server to begin next UP period
            print(f"\n[UP] Restarting server...")
            start_server(node_id)

    except KeyboardInterrupt:
        print(f"\n\n🛑 Auto-failover stopped by user")
        print(f"   Node {node_id} will continue running if it was started")
        sys.exit(0)


def main():
    parser = argparse.ArgumentParser(
        description='Auto-failover script for Raft server nodes',
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
  # Run on Node 1 with default up/down intervals (60-120 seconds each)
  python3 auto_failover.py --node-id 1
  
  # Run on Node 2 with custom down/up intervals (30-90s down, 45-120s up)
  python3 auto_failover.py --node-id 2 --min-down 30 --max-down 90 --min-up 45 --max-up 120
  
  # Run on Node 3 with 1-2 minute intervals
  python3 auto_failover.py --node-id 3 --min-down 60 --max-down 120 --min-up 60 --max-up 120

  # Shorthand ranges with units:
  python3 auto_failover.py --node-id 1 --down 1m-2m --up 45s-90s
  # Or set both up & down at once:
  python3 auto_failover.py --node-id 3 --min 1m --max 2m
        """
    )
    
    parser.add_argument(
        '--node-id',
        type=int,
        choices=[1, 2, 3],
        required=True,
        help='Node ID to manage (1, 2, or 3)'
    )
    
    # Shorthand range flags (min-max with optional s/m)
    parser.add_argument('--down', type=str, help="Down-time range as 'MIN-MAX' (e.g., 60-120, 1m-2m)")
    parser.add_argument('--up', type=str, help="Up-time range as 'MIN-MAX' (e.g., 60-120, 45s-90s)")

    # Simple min/max applying to both up and down
    parser.add_argument('--min', dest='both_min', type=str, help="Minimum for both up & down (e.g., 60, 1m)")
    parser.add_argument('--max', dest='both_max', type=str, help="Maximum for both up & down (e.g., 120, 2m)")

    # Detailed flags: separate up/down intervals
    parser.add_argument('--min-down', type=float, default=60.0, help='Minimum DOWN time in seconds (default: 60)')
    parser.add_argument('--max-down', type=float, default=120.0, help='Maximum DOWN time in seconds (default: 120)')
    parser.add_argument('--min-up', type=float, default=60.0, help='Minimum UP time in seconds (default: 60)')
    parser.add_argument('--max-up', type=float, default=120.0, help='Maximum UP time in seconds (default: 120)')

    # Backward-compatible flags: if provided, map to both up and down
    parser.add_argument('--min-interval', type=float, help='Deprecated: use --min-down/--min-up')
    parser.add_argument('--max-interval', type=float, help='Deprecated: use --max-down/--max-up')
    
    args = parser.parse_args()
    
    # Apply shorthand flags if used
    if args.down:
        md, xd = _parse_range(args.down)
        args.min_down, args.max_down = md, xd
    if args.up:
        mu, xu = _parse_range(args.up)
        args.min_up, args.max_up = mu, xu

    if args.both_min:
        v = _parse_seconds(args.both_min)
        args.min_down = args.min_up = v
    if args.both_max:
        v = _parse_seconds(args.both_max)
        args.max_down = args.max_up = v

    # Apply deprecated flags if used
    if args.min_interval is not None:
        args.min_down = args.min_up = args.min_interval
    if args.max_interval is not None:
        args.max_down = args.max_up = args.max_interval

    # Validate ranges
    for name, value in [('min-down', args.min_down), ('max-down', args.max_down), ('min-up', args.min_up), ('max-up', args.max_up)]:
        if value <= 0:
            print(f"❌ Error: --{name} must be positive")
            sys.exit(1)

    if args.max_down <= args.min_down:
        print("❌ Error: --max-down must be greater than --min-down")
        sys.exit(1)
    if args.max_up <= args.min_up:
        print("❌ Error: --max-up must be greater than --min-up")
        sys.exit(1)
    
    # Check if requests library is available (optional, for health checks)
    try:
        import requests
    except ImportError:
        print("⚠️  Warning: 'requests' library not found. Health checks will be skipped.")
        print("   Install with: pip install requests")
    
    # Verify we're in the right directory
    if not (PROJECT_DIR / "Cargo.toml").exists():
        print(f"❌ Error: Cargo.toml not found in {PROJECT_DIR}")
        print("   Make sure you're running this script from the raft-openraft-demo directory")
        sys.exit(1)
    
    # Verify cargo is available
    try:
        subprocess.run(['cargo', '--version'], capture_output=True, check=True)
    except (subprocess.CalledProcessError, FileNotFoundError):
        print("❌ Error: 'cargo' command not found")
        print("   Make sure Rust/Cargo is installed and in your PATH")
        sys.exit(1)
    
    # Start the failover loop
    run_failover_loop(args.node_id, args.min_down, args.max_down, args.min_up, args.max_up)


if __name__ == '__main__':
    main()

