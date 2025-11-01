# Load Balancing Architecture & Test Guide

## Table of Contents
1. [Quick Start - Running the Test](#quick-start)
2. [Architecture Overview](#architecture-overview)
3. [Client Architecture](#client-architecture)
4. [Server Architecture](#server-architecture)
5. [Design Decisions](#design-decisions)
6. [Testing Guide](#testing-guide)

---

## Quick Start

### Prerequisites
```bash
# 1. Build the project (release mode for best performance)
cd raft-openraft-demo
cargo build --release

# 2. Ensure test image exists
ls input-image.png  # Should exist
```

### Step-by-Step Terminal Instructions

#### Terminal 1 - Node 1 (Leader Candidate)
```bash
cd raft-openraft-demo
cargo run --release -- \
  --id 1 \
  --http-addr 0.0.0.0:8001 \
  --rpc-addr 0.0.0.0:7001 \
  --peers "2=127.0.0.1:7002,3=127.0.0.1:7003"
```

**Wait for:** `🌐 HTTP API listening on 0.0.0.0:8001`

#### Terminal 2 - Node 2 (Follower)
```bash
cd raft-openraft-demo
cargo run --release -- \
  --id 2 \
  --http-addr 0.0.0.0:8002 \
  --rpc-addr 0.0.0.0:7002 \
  --peers "1=127.0.0.1:7001,3=127.0.0.1:7003"
```

**Wait for:** `🌐 HTTP API listening on 0.0.0.0:8002`

#### Terminal 3 - Node 3 (Follower)
```bash
cd raft-openraft-demo
cargo run --release -- \
  --id 3 \
  --http-addr 0.0.0.0:8003 \
  --rpc-addr 0.0.0.0:7003 \
  --peers "1=127.0.0.1:7001,2=127.0.0.1:7002"
```

**Wait for:** `🌐 HTTP API listening on 0.0.0.0:8003`

#### Terminal 4 - Initialize & Test
```bash
# Step 1: Wait 5-10 seconds for all nodes to start

# Step 2: Initialize the cluster
cd raft-openraft-demo
./setup.sh

# Step 3: Run multi-threaded load balancing test
python3 test_load_balancing_threaded.py
```

---

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│                    CLIENT (Multi-Threaded)                   │
│  ┌────────┐  ┌────────┐  ┌────────┐  ...  ┌────────┐      │
│  │Thread 1│  │Thread 2│  │Thread 3│       │Thread N│      │
│  │200 req │  │200 req │  │200 req │       │200 req │      │
│  └────┬───┘  └────┬───┘  └────┬───┘       └────┬───┘      │
│       │           │           │                 │           │
│       └───────────┴───────────┴─────────────────┘           │
│                  │                                           │
│                  │ Multicast to All 3 Nodes                  │
└──────────────────┼───────────────────────────────────────────┘
                    │
        ┌───────────┼───────────┐
        │           │           │
        ▼           ▼           ▼
┌───────────┐ ┌───────────┐ ┌───────────┐
│  Node 1   │ │  Node 2   │ │  Node 3   │
│ (Leader)  │ │(Follower) │ │(Follower) │
│  :8001    │ │  :8002    │ │  :8003    │
├───────────┤ ├───────────┤ ├───────────┤
│ HTTP API  │ │ HTTP API  │ │ HTTP API  │
│  503 ✗    │ │  503 ✗    │ │           │
│           │ │           │ │           │
│  ┌──────┐ │ │           │ │           │
│  │Load  │ │ │           │ │           │
│  │Bal.  │ │ │           │ │           │
│  └──┬───┘ │ │           │ │           │
│     │     │ │           │ │           │
│     ▼     │ │           │ │           │
│ Forward   │ │           │ │           │
│ Randomly  ─┼─┼───────────┼─┼───────────┤
│           │ │           │ │           │
├───────────┤ ├───────────┤ ├───────────┤
│ RPC :7001 │ │ RPC :7002 │ │ RPC :7003 │
│           │ │           │ │           │
│ Raft      │ │ Raft      │ │ Raft      │
│ Consensus │ │ Consensus │ │ Consensus │
└───────────┘ └───────────┘ └───────────┘
```

---

## Client Architecture

### Multi-Threaded Test Client (`test_load_balancing_threaded.py`)

**Design:**
- **20 concurrent threads** by default (configurable)
- Each thread sends `TOTAL_REQUESTS // NUM_THREADS` requests
- Thread-safe result aggregation using locks

**Request Flow:**
1. Each thread iterates through its assigned request range
2. For each request:
   - **Multicasts to all 3 nodes simultaneously** using `ThreadPoolExecutor`
   - Waits for all 3 responses in parallel
   - Accepts first successful response (HTTP 200)
   - If all fail, retries up to 4 times with progressive delays
3. Results aggregated thread-safely with locks

**Key Components:**

```python
# Configuration
TOTAL_REQUESTS = 10000    # Total requests to send
NUM_THREADS = 20          # Concurrent threads
MAX_RETRIES = 4           # Retry attempts per request
TIMEOUT = 10              # Request timeout (seconds)

# Thread-safe state
counters_lock = Lock()           # Protects shared counters
node_counts = defaultdict(int)   # Tracks requests per node
success_count = [0]              # Total successes
failure_count = [0]              # Total failures
```

**Multicast Implementation:**
```python
# Sends to all 3 nodes in parallel
with ThreadPoolExecutor(max_workers=3) as executor:
    futures = {
        executor.submit(post_to_node, port): port
        for port in [8001, 8002, 8003]
    }
    # Wait for all responses, accept first 200
```

**Retry Logic:**
- Progressive delays: 0.2s, 0.5s, 0.8s, 1.1s between retries
- Gives time for leader election or node recovery
- Client-side retry ensures resilience to transient failures

**Why This Design:**
- **Multicast pattern** ensures requests reach all nodes (followers reject, leader processes)
- **Threading** simulates realistic concurrent load
- **Client-side retry** handles temporary failures without server complexity

---

## Server Architecture

### Rust Async Runtime (Tokio)

**Runtime Configuration:**
```rust
#[tokio::main(flavor = "multi_thread", worker_threads = 8)]
```
- **8 worker threads** for true parallelism across CPU cores
- Multi-threaded scheduler allows concurrent task execution
- Ideal for I/O-bound and CPU-mixed workloads

### HTTP Request Handling (Axum)

**Concurrency Model:**
- Axum server handles requests concurrently
- Each request handler is `async` and non-blocking
- Multiple requests processed simultaneously on different worker threads
- No request blocks another (except for shared resources like locks)

**Request Flow:**
```
Client Request → Axum Router → Handler (async)
                             ↓
                    Check if Leader
                    ↓           ↓
                Yes            No
                 ↓              ↓
        Process or Forward    Return 503
                 ↓
        Get Random Healthy Node
                 ↓
        Forward or Process Locally
```

**Health Tracking:**
- Nodes marked healthy/unhealthy based on forwarding success/failure
- Only healthy nodes selected for load balancing
- Probation mechanism (20% chance) gives unhealthy nodes recovery opportunity

### Latency-Aware Load Balancing

**Strategy:**
- Leader tracks response latency (milliseconds) for each node
- Uses **inverse latency weighting** to prefer faster nodes
- Faster nodes naturally receive more load
- Enables automatic adaptation to cluster performance variations

**Implementation:**

```rust
// Track latency statistics per node
pub struct LatencyStats {
    pub total_ms: u64,   // Cumulative latency
    pub count: u64,      // Number of samples
}

impl LatencyStats {
    pub fn average_ms(&self) -> f64 {
        if self.count == 0 { 0.0 } else { self.total_ms as f64 / self.count as f64 }
    }
}

// Load balancing with latency weighting
async fn get_random_node(state: &AppState) -> Option<(NodeId, String)> {
    let latencies = state.node_latencies.read().await;
    
    // Calculate weighted selection based on inverse latency
    let mut weighted_nodes: Vec<(NodeId, f64)> = Vec::new();
    let mut total_weight: f64 = 0.0;
    
    for node_id in &healthy_non_self_nodes {
        let latency_ms = latencies
            .get(node_id)
            .map(|s| s.average_ms())
            .unwrap_or(50.0);  // Default 50ms if no data
        
        // Weight = 1 / latency, so faster nodes get higher weight
        let weight = 1.0 / (latency_ms + 1.0);
        weighted_nodes.push((*node_id, weight));
        total_weight += weight;
    }
    
    // Select node based on weighted probability
    let mut rand_val = (random::<f64>()) * total_weight;
    for (node_id, weight) in weighted_nodes {
        rand_val -= weight;
        if rand_val <= 0.0 {
            println!("🎯 LATENCY-WEIGHTED: Selected node {} (avg latency: {:.1}ms)", 
                     node_id, latencies.get(&node_id)?.average_ms());
            return Some((node_id, http_addrs.get(&node_id)?.clone()));
        }
    }
    
    Some((node_id, addr))
}

// Record latency on each request
let start_time = std::time::Instant::now();
match forward_request_to_node(target_id, &target_addr, "/image/steg", &body).await {
    Ok(response) => {
        let elapsed_ms = start_time.elapsed().as_millis() as u64;
        
        // Update latency statistics
        let mut latencies = state.node_latencies.write().await;
        let stats = latencies.entry(target_id).or_insert(LatencyStats {
            total_ms: 0,
            count: 0,
        });
        stats.total_ms += elapsed_ms;
        stats.count += 1;
    }
}
```

**Example Behavior:**

With 3 nodes and these latencies:
- Node 1: 50ms → weight = 1/51 ≈ 0.020
- Node 2: 100ms → weight = 1/101 ≈ 0.010  
- Node 3: 200ms → weight = 1/201 ≈ 0.005

**Selection probability:**
- Node 1 (fast): 51% of requests
- Node 2 (medium): 25% of requests
- Node 3 (slow): 13% of requests
- (Remaining 11% to leader for local processing)

**Benefits:**
- ✅ Automatic performance adaptation
- ✅ Faster nodes handle more load naturally
- ✅ Slow nodes gracefully receive fewer requests
- ✅ No manual configuration required
- ✅ Responds dynamically to network changes

### CPU-Intensive Operations


**Steganography Processing:**
- Image encryption and embedding is CPU-intensive
- Spawned in `tokio::task::spawn_blocking()` to avoid blocking async runtime
- Multiple stego operations can run in parallel on different threads
- Allows concurrent processing of multiple requests

```rust
// Before (blocking):
match embed_image_into_cover(&body[..]) { ... }

// After (non-blocking):
match tokio::task::spawn_blocking(move || {
    embed_image_into_cover(&body_clone[..])
}).await { ... }
```

### RPC (Server-to-Server) Communication

**Architecture:**
- Each incoming RPC connection spawns separate async task (`tokio::spawn`)
- Multiple Raft protocol messages handled concurrently
- Raft operations are async and non-blocking

**Message Types:**
1. **AppendEntries**: Log replication from leader to followers
2. **Vote**: Election requests during leader election
3. **InstallSnapshot**: State synchronization for catch-up

**Connection Handling:**
```rust
loop {
    let (socket, addr) = listener.accept().await?;
    tokio::spawn(async move {
        handle_rpc_connection(socket, raft).await
    });
}
```

### Raft Consensus Layer

**Leader Election:**
- Runs in separate async tasks (handled by OpenRaft)
- Non-blocking, managed by OpenRaft library
- Election timeouts and heartbeats managed concurrently

**Log Replication:**
- Forwarded to followers in parallel
- Each forward operation is async
- Health tracking happens concurrently with replication

**State Management:**
- `Arc<RaftNode>` - Shared reference to Raft state machine
- Thread-safe for concurrent access
- Metrics and state queries don't block operations

---

## Design Decisions

### 1. Multicast Request Pattern

**Decision:** Client sends every request to all 3 nodes simultaneously

**Rationale:**
- **Simplicity**: Client doesn't need to know which node is leader
- **Resilience**: If one node is down, others may still respond
- **Load Distribution**: Leader automatically distributes to healthy nodes

**Trade-offs:**
- ✅ No client-side leader discovery needed
- ✅ Natural failover if leader crashes
- ❌ 3x network traffic (but followers reject quickly with 503)
- ❌ Extra requests to followers (but they reject immediately)

### 2. Leader-Only Processing

**Decision:** Only leader accepts direct client requests; followers reject with 503

**Rationale:**
- **Single Point of Control**: Leader decides load distribution
- **Consistency**: All load balancing logic in one place
- **Simplified State**: Only leader maintains health tracking

**Trade-offs:**
- ✅ Centralized load balancing logic
- ✅ Consistent health tracking
- ❌ Leader becomes bottleneck (mitigated by forwarding)
- ❌ Followers waste CPU rejecting requests (minimal - quick 503)

### 3. Random Load Balancing

**Decision:** Leader randomly selects healthy nodes (including self) for processing

**Rationale:**
- **Simplicity**: No complex algorithms needed
- **Fair Distribution**: Random selection ensures roughly equal distribution
- **Flexibility**: Easy to exclude unhealthy nodes

**Implementation:**
```rust
// Filters to healthy nodes only
let healthy_nodes = http_addrs.iter()
    .filter(|(id, _)| healthy.get(id).unwrap_or(false))
    .collect();
let selected = random() % healthy_nodes.len();
```

### 4. Health Tracking with Probation

**Decision:** Track node health, but give unhealthy nodes 20% chance for recovery

**Rationale:**
- **Fail-Fast**: Don't waste requests on down nodes
- **Recovery**: Unhealthy nodes can prove they're back
- **Balance**: 80% healthy nodes, 20% probation for recovery

**Flow:**
1. Node forwarding fails → marked unhealthy
2. Unhealthy nodes excluded from normal selection
3. 20% chance to try unhealthy node (probation)
4. If successful → marked healthy again
5. If failed → remains unhealthy

**Why 20%?**
- Low enough to not significantly impact performance
- High enough for rapid recovery (within ~5 requests on average)
- Balances efficiency with resilience

### 5. Client-Side Retry

**Decision:** Client retries failed requests (multicast again) instead of server fallback

**Rationale:**
- **Separation of Concerns**: Client handles retries, server focuses on processing
- **Flexibility**: Client can implement custom retry strategies
- **Transparency**: Server returns errors; client decides what to do

**Alternative Considered:**
- Server-side fallback to alternative nodes
- **Rejected because**: Adds complexity, requires stateful retry tracking

### 6. Async Runtime with Blocking Tasks

**Decision:** Use `spawn_blocking` for CPU-intensive work (steganography)

**Rationale:**
- **Non-Blocking I/O**: HTTP handlers don't wait for CPU work
- **Parallelism**: Multiple blocking tasks can run on different threads
- **Throughput**: Can process multiple images simultaneously

**Thread Model:**
- 8 worker threads for async tasks (I/O, forwarding)
- Blocking pool (separate) for CPU-intensive work
- Optimal utilization of all CPU cores

### 7. Thread-Safe Shared State

**Decision:** Use `Arc<RwLock<>>` for shared mutable state

**Rationale:**
- **Concurrency**: Multiple requests can read simultaneously
- **Safety**: Writers lock exclusively
- **Performance**: Read locks don't block other readers

**State Protected:**
- `healthy_nodes: Arc<RwLock<BTreeMap<NodeId, bool>>>`
- `http_addresses: Arc<RwLock<BTreeMap<NodeId, String>>>`
- `raft: Arc<RaftNode>` (OpenRaft handles internal synchronization)

### 8. Timeouts

**Decision:** 5s node-to-node, 10s client-to-server timeouts

**Rationale:**
- **Fail-Fast**: Don't wait indefinitely for crashed nodes
- **Client Experience**: Reasonable timeout for user-facing requests
- **Node Communication**: Shorter timeout for internal calls (faster failure detection)

**Values:**
- Client timeout (10s): Allows for network latency and processing
- Forwarding timeout (5s): Internal calls should be faster

---

## Testing Guide

### Basic Load Balancing Test

```bash
python3 test_load_balancing_threaded.py
```

**What It Tests:**
- Request distribution across nodes
- Load balancing fairness
- Concurrent request handling
- Multicast pattern functionality

**Expected Results:**
- Roughly 33% distribution per node
- High success rate (>99%)
- Throughput: Significantly higher than single-threaded

### Failure Scenario Testing

**Test Node Crash Recovery:**

1. Start test: `python3 test_load_balancing_threaded.py`
2. While running, kill a node (Ctrl+C in Terminal 2 or 3)
3. Observe:
   - Failures spike briefly
   - Remaining nodes handle increased load
   - Distribution adjusts (e.g., 50% each if one node down)
4. Restart killed node
5. Observe:
   - Node marked unhealthy initially
   - Probation requests test recovery
   - Node regains load share once healthy

**Test Leader Crash:**

1. Kill the leader node
2. Observe:
   - Brief failure window during election
   - New leader elected (visible in logs)
   - Requests resume normally
   - Client retries handle transient failures

### Performance Benchmarks

**Expected Metrics:**

| Metric | Single-Threaded | Multi-Threaded (20 threads) |
|--------|----------------|----------------------------|
| **Throughput** | ~30 req/s | ~150-300 req/s |
| **Total Time (2000 req)** | ~66s | ~9s |
| **CPU Usage** | *** | **** |

**Throughput Calculation (Per-Node):**

The client calculates throughput for each node based on **actual request timing**, not total test duration:

```typescript
// Track when each request for a node completes
const nodeStat.requestTimestamps = [t1, t2, t3, ..., tn];

// Calculate time span from first to last request
const nodeStartTime = nodeStat.requestTimestamps[0];
const nodeEndTime = nodeStat.requestTimestamps[nodeStat.requestTimestamps.length - 1];
const nodeElapsedSeconds = (nodeEndTime - nodeStartTime) / 1000;

// True throughput = successful requests during that time window
const requestsPerSecond = nodeStat.success / nodeElapsedSeconds;
```

**Why This Matters:**

❌ **Wrong:** `(total_requests / total_test_time)` - Divides by entire test duration
- Example: If 39 requests hit Node 1, and total test took 120s → 0.32 req/s ❌

✅ **Correct:** `(node_requests / time_window_for_that_node)` - Divides by actual time window
- Example: If 39 requests hit Node 1 between 2s-4s → 19.5 req/s ✅

This shows Node 1's *actual processing rate*, not an artificially low number caused by requests arriving at different times.

**Factors Affecting Performance:**
- Network latency
- Image size (for steganography endpoint)
- Node health (unhealthy nodes slow down forwarding)
- System resources (CPU, memory, network bandwidth)

### Configuration Tuning


**Client (Python):**
```python
TOTAL_REQUESTS = 10000  # Adjust total load
NUM_THREADS = 20        # Increase for more concurrency
MAX_RETRIES = 4         # More retries = fewer failures, slower
TIMEOUT = 10            # Longer timeout = more resilient, slower
```

**Server (Rust):**
```rust
worker_threads = 8      // Match CPU cores (or slightly more)
heartbeat_interval = 300 // Raft heartbeat frequency (ms)
election_timeout_min = 1500 // Min election timeout (ms)
election_timeout_max = 2500 // Max election timeout (ms)
```

### Monitoring During Test

**Watch Server Logs:**
- Request forwarding messages
- Health status changes
- Probation attempts
- Error messages

**Watch Client Output:**
- Progress indicators
- Final distribution statistics
- Failure count and analysis

**System Monitoring:**
```bash
# CPU usage
top -p $(pgrep -f raft-openraft-demo)

# Network traffic
netstat -i

# Process threads
ps -eLf | grep raft-openraft-demo
```

---

## Summary

This architecture provides:
- ✅ **High Throughput**: Multi-threaded client and server
- ✅ **Resilience**: Health tracking, retries, probation
- ✅ **Fair Distribution**: Random selection with health filtering
- ✅ **Scalability**: Concurrent request handling, non-blocking I/O
- ✅ **Simplicity**: Clear separation of concerns, straightforward logic

The system demonstrates robust load balancing with automatic failure recovery and efficient concurrent processing.

