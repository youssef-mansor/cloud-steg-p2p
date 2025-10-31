# Quick Start Guide - Multi-Threaded Load Balancing Test

## Prerequisites

1. Build the project (release mode for best performance):
   ```bash
   cd /Users/kareemabdelrazek/projects/cloud-steg-p2p/raft-openraft-demo
   cargo build --release
   ```

2. Ensure `input-image.png` exists in the project directory

## Terminal Setup

### Terminal 1 - Node 1 (Leader Candidate)
```bash
cd /Users/kareemabdelrazek/projects/cloud-steg-p2p/raft-openraft-demo
cargo run --release -- \
  --id 1 \
  --http-addr 0.0.0.0:8001 \
  --rpc-addr 0.0.0.0:7001 \
  --peers "2=127.0.0.1:7002,3=127.0.0.1:7003"
```

**What you'll see:**
- Node 1 starting
- Raft node created
- HTTP API listening on 0.0.0.0:8001
- RPC server listening on 0.0.0.0:7001
- ⚡ Server configured with multi-threaded async runtime (8 worker threads)

---

### Terminal 2 - Node 2 (Follower)
```bash
cd /Users/kareemabdelrazek/projects/cloud-steg-p2p/raft-openraft-demo
cargo run --release -- \
  --id 2 \
  --http-addr 0.0.0.0:8002 \
  --rpc-addr 0.0.0.0:7002 \
  --peers "1=127.0.0.1:7001,3=127.0.0.1:7003"
```

**What you'll see:**
- Node 2 starting
- Raft node created
- HTTP API listening on 0.0.0.0:8002
- RPC server listening on 0.0.0.0:7002
- ⚡ Server configured with multi-threaded async runtime (8 worker threads)

---

### Terminal 3 - Node 3 (Follower)
```bash
cd /Users/kareemabdelrazek/projects/cloud-steg-p2p/raft-openraft-demo
cargo run --release -- \
  --id 3 \
  --http-addr 0.0.0.0:8003 \
  --rpc-addr 0.0.0.0:7003 \
  --peers "1=127.0.0.1:7001,2=127.0.0.1:7002"
```

**What you'll see:**
- Node 3 starting
- Raft node created
- HTTP API listening on 0.0.0.0:8003
- RPC server listening on 0.0.0.0:7003
- ⚡ Server configured with multi-threaded async runtime (8 worker threads)

---

### Terminal 4 - Setup & Run Test

**Step 1: Wait for all nodes to start (5-10 seconds)**

**Step 2: Initialize the cluster**
```bash
cd /Users/kareemabdelrazek/projects/cloud-steg-p2p/raft-openraft-demo
./setup.sh
```

**Expected output:**
```
⏳ Waiting for servers to start...
🎬 Initializing cluster...
✅ Initialized
📥 Adding nodes as learners...
✅ Learners added
🔄 Promoting to voters...
✅ Cluster ready

📊 Cluster Status:
  Node 1: Leader (or Learner/Follower initially)
  Node 2: Follower (or Learner initially)
  Node 3: Follower (or Learner initially)
```

**Step 3: Run the multi-threaded load balancing test**
```bash
python3 test_load_balancing_threaded.py
```

**Expected output:**
```
✅ Leader is Node 1
📡 Will multicast requests to all 3 nodes (port 8001, 8002, 8003)
   - Followers will reject (503)
   - Leader will forward to healthy nodes

🚀 Starting 20 threads, 100 requests per thread
📤 Sending 2000 requests concurrently...

[Progress dots every 100 requests]
.......... 1000/2000 (XXs elapsed)
.......... 2000/2000 (XXs elapsed)

📊 Load Balancing Test Summary
==============================

Total Requests Sent: 2000
Successful Requests: 199X
Failed Requests: X

Architecture:
  • 20 concurrent threads
  • 100 requests per thread
  • Requests multicast to all 3 nodes...
  ...

Requests Processed by Each Node:
---------------------------------
  Node 1: XXX requests (XX.XX%)
  Node 2: XXX requests (XX.XX%)
  Node 3: XXX requests (XX.XX%)

Total Time: XX seconds
Average Throughput: XX.XX requests/second

✅ Test Complete!
```

---

## Troubleshooting

### If nodes don't connect:
1. Make sure all 3 nodes are running
2. Wait 5-10 seconds after starting before running `setup.sh`
3. Check that ports 7001-7003 and 8001-8003 are not in use

### If test fails:
1. Verify all nodes show "HTTP API listening on..."
2. Check leader with: `curl http://127.0.0.1:8001/metrics | jq '.data.current_leader'`
3. Ensure `input-image.png` exists in project directory

### If Python dependencies missing:
```bash
pip3 install requests
```

### To monitor server logs:
- Watch all terminals for request processing messages
- You should see concurrent request handling across threads
- Look for "🔍 Probation" messages when nodes recover

---

## Optional: Test Failure Scenarios

While test is running:
1. Kill a follower (Ctrl+C in Terminal 2 or 3)
2. Watch how requests adapt (fewer failures due to health tracking)
3. Restart the node and see it recover via probation mechanism

---

## Performance Expectations

**With 20 threads:**
- Throughput: Should be much higher than single-threaded
- Server utilization: Should use multiple CPU cores
- Response times: Should remain low despite high concurrency

**Distribution:**
- Should see roughly 33% distribution across all 3 nodes
- If a node crashes and recovers, it should regain load share

