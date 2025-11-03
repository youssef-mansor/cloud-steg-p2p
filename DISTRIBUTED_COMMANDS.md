# Distributed Cluster Commands (10.7.57.x Network)

## Setup Overview
- **Node 1 (Leader)**: 10.7.57.143:8001 (HTTP), 10.7.57.143:7001 (RPC)
- **Node 2 (Follower)**: 10.7.57.109:8002 (HTTP), 10.7.57.109:7002 (RPC)
- **Node 3 (Follower)**: 10.7.57.155:8003 (HTTP), 10.7.57.155:7003 (RPC)

---

## Terminal 1 - Node 1 (Leader Candidate)

Run this on machine **10.7.57.143**:

```bash
cd raft-openraft-demo
cargo run --release -- \
  --id 1 \
  --http-addr 0.0.0.0:8001 \
  --rpc-addr 0.0.0.0:7001 \
  --peers "2=10.7.57.109:7002,3=10.7.57.155:7003"
```

**Wait for:** `🌐 HTTP API listening on 0.0.0.0:8001`

---

## Terminal 2 - Node 2 (Follower)

Run this on machine **10.7.57.109**:

```bash
cd raft-openraft-demo
cargo run --release -- \
  --id 2 \
  --http-addr 0.0.0.0:8002 \
  --rpc-addr 0.0.0.0:7002 \
  --peers "1=10.7.57.143:7001,3=10.7.57.155:7003"
```

**Wait for:** `🌐 HTTP API listening on 0.0.0.0:8002`

---

## Terminal 3 - Node 3 (Follower)

Run this on machine **10.7.57.155**:

```bash
cd raft-openraft-demo
cargo run --release -- \
  --id 3 \
  --http-addr 0.0.0.0:8003 \
  --rpc-addr 0.0.0.0:7003 \
  --peers "1=10.7.57.143:7001,2=10.7.57.109:7002"
```

**Wait for:** `🌐 HTTP API listening on 0.0.0.0:8003`

---

## Terminal 4 - Initialize & Test (From Any Machine)

```bash
# Step 1: Wait 5-10 seconds for all nodes to start

# Step 2: Initialize the cluster
cd raft-openraft-demo
./setup.sh

# Step 3: Run multi-threaded load balancing test
python3 test_load_balancing_threaded.py
```

---

## Quick Reference

### From 10.7.57.143:
```bash
cargo run --release -- --id 1 --http-addr 0.0.0.0:8001 --rpc-addr 0.0.0.0:7001 --peers "2=10.7.57.109:7002,3=10.7.57.155:7003"
```

### From 10.7.57.109:
```bash
cargo run --release -- --id 2 --http-addr 0.0.0.0:8002 --rpc-addr 0.0.0.0:7002 --peers "1=10.7.57.143:7001,3=10.7.57.155:7003"
```

### From 10.7.57.155:
```bash
cargo run --release -- --id 3 --http-addr 0.0.0.0:8003 --rpc-addr 0.0.0.0:7003 --peers "1=10.7.57.143:7001,2=10.7.57.109:7002"
```

---

## Network Diagram

```
┌──────────────────────────────────────────────────┐
│           Client Test Machine (Any)              │
│   python3 test_load_balancing_threaded.py        │
│   Multicasts to all 3 nodes simultaneously       │
└────────────┬─────────────────────────────────────┘
             │
        ┌────┼─────────────────┐
        │    │                 │
        ▼    ▼                 ▼
   ┌─────────┐          ┌──────────┐          ┌──────────┐
   │Node 1   │          │Node 2    │          │Node 3    │
   │10.7.57. │          │10.7.57.  │          │10.7.57.  │
   │143:8001 │          │109:8002  │          │155:8003  │
   └────┬────┘          └────┬─────┘          └────┬─────┘
        │                    │                     │
        │ RPC Heartbeats & Consensus               │
        └────────────┬───────┴─────────────────────┘
                     │
          Raft Cluster Agreement
```
