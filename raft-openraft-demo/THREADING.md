# Threading Implementation

This document describes the multi-threading architecture for both client and server.

## Client-Side Threading

### Multi-Threaded Test Script (`test_load_balancing_threaded.py`)

**Architecture:**
- **20 concurrent threads** by default
- Each thread sends ~200 requests (configurable via `NUM_THREADS` and `TOTAL_REQUESTS`)
- Each request multicasts to all 3 nodes in parallel (using ThreadPoolExecutor)
- Thread-safe counters using locks for result aggregation

**Features:**
- Python's `concurrent.futures.ThreadPoolExecutor` for thread management
- Each thread processes its batch of requests independently
- Results are collected thread-safely using locks
- Progress tracking across all threads

**Usage:**
```bash
python3 test_load_balancing_threaded.py
```

**Configuration:**
- `NUM_THREADS = 20` - Number of concurrent client threads
- `REQUESTS_PER_THREAD = TOTAL_REQUESTS // NUM_THREADS` - Auto-calculated
- `MAX_RETRIES = 2` - Client-side retry attempts
- `TIMEOUT = 10` - Request timeout in seconds

## Server-Side Threading

### Rust Async Runtime (Tokio)

**Runtime Configuration:**
- **Multi-threaded runtime** with 8 worker threads
- Configured in `main.rs`: `#[tokio::main(flavor = "multi_thread", worker_threads = 8)]`
- Allows true parallelism across CPU cores

### HTTP Request Handling

**Concurrency:**
- Axum HTTP server handles requests concurrently
- Each request handler is async and non-blocking
- Multiple requests can be processed simultaneously on different worker threads

**CPU-Intensive Operations:**
- Steganography image processing is spawned in `spawn_blocking` tasks
- Prevents blocking the async runtime during CPU-intensive work
- Multiple steganography operations can run in parallel on different threads

### RPC (Server-to-Server) Communication

**Architecture:**
- Each incoming RPC connection spawns a separate async task (`tokio::spawn`)
- Allows concurrent handling of multiple Raft protocol messages
- Raft operations are async and non-blocking

**Message Types Handled Concurrently:**
- AppendEntries (log replication)
- Vote (leader election)
- InstallSnapshot (state synchronization)

### Raft Consensus Threading

**Leader Election:**
- Runs in separate async tasks
- Non-blocking, handled by OpenRaft library
- Election timeouts and heartbeats managed concurrently

**Log Replication:**
- Forwarded to followers in parallel
- Each forward operation is async
- Health tracking happens concurrently

## Performance Benefits

### Client Side:
- **20x parallelism** - 20 threads sending requests simultaneously
- Reduced total test time
- Better load simulation (realistic concurrent traffic)

### Server Side:
- **8 worker threads** - True parallelism across CPU cores
- **Non-blocking I/O** - HTTP handlers don't block each other
- **Blocking tasks** - CPU-intensive work doesn't block async runtime
- **Concurrent RPC** - Multiple Raft messages processed simultaneously

## Thread Safety

### Shared State:
- `AppState` uses `Arc` (atomic reference counting) for sharing across threads
- Health tracking uses `Arc<RwLock<>>` for thread-safe updates
- HTTP addresses stored in `Arc<RwLock<>>` for concurrent reads/writes
- Raft node is `Arc<RaftNode>` and designed for concurrent access

### Counters:
- Client uses Python's `Lock` for thread-safe counter updates
- Server uses tokio's async locks (`RwLock`) that don't block async tasks

## Recommendations

1. **Adjust thread count** based on CPU cores:
   - Client: Match expected concurrent clients
   - Server: Worker threads = CPU cores (8 is good for most systems)

2. **Monitor resource usage**:
   - Watch CPU usage across all cores
   - Monitor memory for thread overhead
   - Check network bandwidth for concurrent requests

3. **Tune based on workload**:
   - More threads for I/O-bound operations (HTTP)
   - Appropriate blocking pool size for CPU-bound work (steganography)

