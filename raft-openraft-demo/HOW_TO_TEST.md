# How to Run the Advanced Load Balancing Test

## Quick Start

1. **Build the project:**
   ```bash
   cd /Users/kareemabdelrazek/projects/cloud-steg-p2p/raft-openraft-demo
   cargo build --release
   ```

2. **Start 3 servers in separate terminals:**

   **Terminal 1:**
   ```bash
   cargo run --release -- --id 1 --http-addr 0.0.0.0:8001 --rpc-addr 0.0.0.0:7001 --peers "2=127.0.0.1:7002,3=127.0.0.1:7003"
   ```

   **Terminal 2:**
   ```bash
   cargo run --release -- --id 2 --http-addr 0.0.0.0:8002 --rpc-addr 0.0.0.0:7002 --peers "1=127.0.0.1:7001,3=127.0.0.1:7003"
   ```

   **Terminal 3:**
   ```bash
   cargo run --release -- --id 3 --http-addr 0.0.0.0:8003 --rpc-addr 0.0.0.0:7003 --peers "1=127.0.0.1:7001,2=127.0.0.1:7002"
   ```

3. **Wait 5-10 seconds for servers to start, then initialize the cluster:**
   ```bash
   ./setup.sh
   ```

4. **Run the test:**
   ```bash
   python3 test_load_balancing_advanced.py
   ```

## What the Test Does

- Sends 2000 requests to `/image/steg` endpoint (steganography, CPU-intensive)
- Uses 20 concurrent threads
- **Automatically stops Node 2** after 500 requests
- **Automatically restarts Node 2** after 10 seconds
- **Automatically stops Node 1** after 1500 requests  
- **Automatically restarts Node 1** after 10 seconds
- Tracks network latency for each request
- Logs all failure/recovery events with timestamps

## Output Files

Results saved to `/tmp/load_balance_test_<PID>/`:
- `summary.txt` - Test summary with latency stats
- `results.txt` - Detailed results for each request
- `failures.txt` - Timeline of server failures and recoveries

## Configuration

Edit these variables in `test_load_balancing_advanced.py` if needed:
- `TOTAL_REQUESTS = 2000` - Total requests
- `NUM_THREADS = 20` - Concurrent threads
- `FAILURE_AFTER_REQUESTS = 500` - When to fail Node 2
- `RECOVERY_AFTER_SECONDS = 10` - Recovery delay

