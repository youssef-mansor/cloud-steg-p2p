# Raft Election and Fault Tolerance Testing Guide

## Overview

This guide explains what should happen at each test phase and how to interpret the results based on Raft consensus protocol behavior.

## Key Raft Concepts to Verify

### 1. Leader Election Timeout
- **Range**: 500ms (heartbeat) to 1500-3000ms (election timeout)
- **What to observe**: After leader stops, followers should detect missing heartbeats
- **Expected time to new election**: 1-5 seconds
- **Why**: Raft uses randomized timeouts to prevent simultaneous candidates splitting votes

### 2. Term Increment
- **What it is**: A logical clock in Raft, incremented each election
- **Why it matters**: Ensures older leaders cannot overwrite newer ones
- **Expected behavior**: `Term_new > Term_old` when new leader elected
- **Safety guarantee**: Protects against split-brain (two simultaneous leaders)

### 3. Quorum Requirements
- **3-node cluster**: Need at least 2 nodes for majority
- **With 1 node down**: 2 nodes still form quorum → leader can operate
- **With 2 nodes down**: Only 1 node left → NO quorum → no leader elected
- **Safety property**: Impossible for two independent majorities to exist

### 4. State Replication
- **Leader to followers**: Leader sends AppendEntries RPCs periodically
- **Consistency check**: All nodes should eventually see same committed entries
- **After rejoin**: Restarted node must catch up to leader's current term

---

## Test Phase Details

### TEST 1: Initial Cluster Setup ✓

**What your code should do** (from `main.rs` and `api.rs`):
1. All 3 nodes start listening on their RPC ports (7001, 7002, 7003)
2. HTTP APIs bind to 8001, 8002, 8003
3. `bootstrap.sh` calls `POST /cluster/init` → Node 1 becomes leader
4. `bootstrap.sh` calls `POST /cluster/add-learner` for nodes 2, 3
5. `bootstrap.sh` calls `POST /cluster/change-membership` to promote to voters
6. OpenRaft's leader election algorithm runs:
   - Current leader (Node 1) stays leader (has highest priority)
   - All nodes enter same term
   - Membership is {1, 2, 3}

**Expected output**:
```
Node 1: Leader (term=1, members=3)
Node 2: Follower (term=1, members=3)
Node 3: Follower (term=1, members=3)
```

**Why it works**: 
- OpenRaft guarantees only one leader per term
- All nodes start with knowledge of peer addresses
- Initialized leader immediately sends heartbeats to establish authority

---

### TEST 2: Leader Failure & Election 🔴 → 🟢

**Scenario**: Stop process running Node 1 (current leader)

**What happens internally** (from `rpc_handler.rs` and Raft algorithm):

1. **Leader stops**: 
   - Node 1 process terminates (no more heartbeats sent)
   - TCP connections from Node 1 close

2. **Followers detect timeout** (at Nodes 2, 3):
   - No heartbeat received for ~1500ms (election timeout)
   - OpenRaft timer fires in both followers
   - Both start election: increment term to 2, vote for themselves

3. **Vote exchange** (Network layer `network.rs`):
   - Node 2 sends VoteRequest to Node 3: "Vote for me, term=2"
   - Node 3 sends VoteRequest to Node 2: "Vote for me, term=2"
   - First to receive gets the vote (first one wins)
   - Winner gets 2/3 votes → becomes new leader

4. **New leader established**:
   - New leader (say Node 2) sends AppendEntries to Node 3
   - Node 3 acknowledges and updates its term to 2
   - All nodes see `current_term=2`

**Expected behavior**:
- **Time to election**: 1-5 seconds (depends on randomized timeout)
- **New leader**: One of Nodes 2 or 3 (random, roughly 50/50)
- **Term increase**: 1 → 2
- **Old leader**: No longer active, queries fail with timeout

**Observable in `/metrics`:
```
Before stopping Node 1:
Node 1 (Leader): { "state": "Leader", "current_term": 1 }
Node 2 (Follower): { "state": "Follower", "current_term": 1, "leader_id": 1 }
Node 3 (Follower): { "state": "Follower", "current_term": 1, "leader_id": 1 }

[Node 1 stopped...]

After ~3 seconds:
Node 2: { "state": "Leader", "current_term": 2 } [or Node 3 if it won election]
Node 3: { "state": "Follower", "current_term": 2, "leader_id": 2 }
Node 1: [UNREACHABLE - process is dead]
```

**Safety checks**:
- ✓ Exactly one leader elected (mutual exclusion)
- ✓ Leader is one of {2, 3} (dead node didn't somehow become leader)
- ✓ Term incremented (prevents old leaders from returning)
- ✓ New leader knows about old term (via heartbeat exchange)

---

### TEST 3: Remaining Cluster Operations ✓

**Verification**: The 2-node cluster continues operating

**Why this matters**: 
- Proves quorum = 2/3 is sufficient
- Demonstrates log replication working with subset
- Shows load can continue even with node down

**What should happen**:
- New leader (Node 2 or 3) can still commit new entries
- Heartbeats flow: Leader → Follower
- If you sent a write request, it would replicate to the remaining follower

**Observable**:
```
Node 2: { "state": "Leader", "current_term": 2 }
Node 3: { "state": "Follower", "current_term": 2, "leader_id": 2 }
```

**Safety guarantee**: OpenRaft will NOT elect another leader until one of the downed nodes comes back or rejoins

---

### TEST 4: Restart Failed Leader & Rejoin 🔄

**Scenario**: Restart Node 1 (the original leader that failed)

**What happens internally**:

1. **Node 1 restarts** (runs `cargo run -- --id 1 ...` again):
   - Starts fresh with term stored in memory (in this case, 1, since MemStore doesn't persist)
   - **NOTE**: With in-memory store, restarted node will NOT know about term=2 yet
   - RPC server starts listening on 7001

2. **New leader (say Node 2) detects Node 1 rejoined**:
   - Node 2 was sending AppendEntries to 7001, failed before
   - Now connection succeeds
   - Node 2 sends: "Your term is old, mine is 2. Here's the current state."
   - Node 1 learns term=2 and updates

3. **State synchronization** (from `network.rs`):
   - Node 1 receives AppendEntries from Node 2 with term=2
   - Node 1 updates its term to 2
   - Node 1 transitions to Follower state
   - Node 1 replicates any missing log entries

**Expected behavior**:
- **Time to rejoin**: 1-3 seconds
- **New node term**: Should match leader's term (term=2)
- **New node state**: Follower
- **Membership**: Still {1, 2, 3}

**Observable**:
```
After ~3 seconds of Node 1 restarting:
Node 1: { "state": "Follower", "current_term": 2, "leader_id": 2 }
Node 2: { "state": "Leader", "current_term": 2 }
Node 3: { "state": "Follower", "current_term": 2, "leader_id": 2 }
```

**Key check**: 
- Node 1's term should match leader's (both term=2)
- If they're different, state replication is incomplete

---

### TEST 5: Full Cluster Recovery ✅

**Verification**: All 3 nodes are now operational

**What you should see**:
```
Node 1: Follower (term=2, members=3)
Node 2: Leader (term=2, members=3)
Node 3: Follower (term=2, members=3)
```

**Why this proves correctness**:
- ✓ Failed node rejoined automatically
- ✓ No manual intervention needed
- ✓ Cluster recovered to 3/3 operational
- ✓ Leader unchanged (stable leadership in absence of failures)
- ✓ All nodes consistent on term and membership

**Durability note**: 
- With in-memory store, Node 1 lost all log entries on crash
- With persistent store (RocksDB, etc.), Node 1 would recover logs from disk
- Current implementation prioritizes simplicity for testing

---

### TEST 6: Partition Tolerance (Optional) 🔀

**Scenario**: Stop ONE follower (say Node 3) to simulate minority partition

**What happens**:
- Leader (Node 2) + Follower (Node 1) = 2/3 quorum
- Stopped Node 3 = partition in minority (1/3)

**Expected behavior**:
- **Majority (Nodes 1, 2)**: Continue operating
  - Leader sends heartbeats to Node 1
  - Can accept and replicate new writes
- **Minority (Node 3)**: Isolated
  - Times out waiting for leader
  - May campaign for leadership but loses (only 1 vote out of 3)
  - Stays as Candidate/Follower with stale term

**Observable**:
```
Majority partition (operational):
Node 1: { "state": "Follower", "current_term": 2, "leader_id": 2 }
Node 2: { "state": "Leader", "current_term": 2 }

Minority partition (isolated):
Node 3: { "state": "Candidate"/"Follower", "current_term": 2 or 3 }
        [No leader_id or stale leader_id]
```

**Why this is important**:
- **Partition tolerance**: System remains available during network splits
- **Consistency**: Impossible for both partitions to have leaders simultaneously
- **Safety**: Minority partition CANNOT commit writes (would violate quorum)

---

## Metrics Output Interpretation

### GET /metrics Response Structure

```json
{
  "data": {
    "state": "Leader|Follower|Candidate",
    "current_term": 2,
    "leader_id": 2,
    "membership": {
      "members": [1, 2, 3],
      "nodes": {
        "1": "127.0.0.1:7001",
        "2": "127.0.0.1:7002",
        "3": "127.0.0.1:7003"
      }
    }
  }
}
```

### Key Fields

| Field | Meaning | What to Watch |
|-------|---------|---------------|
| `state` | Current node role | Should be one of: Leader, Follower, Candidate |
| `current_term` | Logical clock | Increases during elections (1 → 2 → 3...) |
| `leader_id` | Who is the leader | Followers show current leader; leader shows itself |
| `members` | Voting nodes | Should be [1, 2, 3] in normal state |
| Absent in response | Node unreachable | Connection failed or process dead |

---

## Fault Tolerance Properties Being Tested

### 1. Safety (Nothing Bad Happens)

- **Two simultaneous leaders?** ❌ Impossible
  - Each leader needs majority vote
  - 3 nodes: need 2 votes each
  - Can't have 2 groups of 2+
  - Verification: Only one node has `"state": "Leader"` per term

- **Committed log entries lost?** ❌ No
  - Leader replicates to majority before committing
  - Even if leader dies, majority still has the entry
  - Verification: New leader knows about all committed entries

### 2. Liveness (Good Things Happen)

- **Progress despite failures?** ✓ Yes
  - With 2/3 quorum, can still elect leaders and replicate
  - With 1/3 quorum only, cannot make progress (expected)
  - Verification: Leader operates successfully with 2 nodes

- **Automatic recovery?** ✓ Yes
  - Failed nodes rejoin automatically
  - No manual coordination needed
  - Verification: Restarted nodes rejoin and sync state

- **Bounded detection time?** ✓ Yes (1-5 seconds)
  - Election timeout is 1.5-3 seconds
  - Plus randomization = 1-5 seconds typical
  - Verification: New leader elected within 5 seconds

### 3. Consistency

- **All nodes agree on state?** ✓ Yes (eventual consistency)
  - Leader determines current state
  - Followers apply same state
  - Verification: `/metrics` shows same `current_term` and `leader_id`

- **No stale leaders?** ✓ Yes
  - Old leader's term is less than current
  - Old leader cannot issue commands that stick
  - Verification: Dead leader's term < new leader's term

---

## What Could Go Wrong (Debugging)

### Symptom: No leader elected after 30 seconds

**Possible causes**:
1. All 3 nodes stopped (check processes running)
2. Network partition (check if nodes can reach each other on RPC ports)
3. Election timeout too long in config
4. Term conflict (try restarting all nodes)

**Debug steps**:
```bash
# Check if nodes are running
ps aux | grep "cargo run"

# Check network connectivity
telnet 127.0.0.1 7001
telnet 127.0.0.1 7002
telnet 127.0.0.1 7003

# Check metrics directly
curl http://127.0.0.1:8001/metrics | jq .
```

### Symptom: Two leaders detected

**This should be IMPOSSIBLE** - indicates a bug in OpenRaft or your code

**Debug**:
- Check if `/metrics` is really returning leader state or if parsing is wrong
- Verify both nodes are actually serving requests (not one is dead)
- Check if there was a clock skew or term reset

### Symptom: Node doesn't rejoin after restart

**Possible causes**:
1. Node listening on wrong port
2. Node can't connect to new leader (network issue)
3. Term mismatch preventing follower status

**Debug**:
```bash
# Check if restarted node is listening
netstat -an | grep 7001

# Manually query the node
curl http://127.0.0.1:8001/metrics

# Check logs (if available)
```

### Symptom: Quorum lost (< 2 nodes operational)

**This is expected with only 1 node alive** in a 3-node cluster

**Why**: Need 2 votes minimum for leader election in 3-node cluster

**Recovery**: Restart any second node to restore quorum

---

## Raft Protocol Visualization

```
INITIAL STATE (Term 1)
  Node 1 (Leader)
  ├─ heartbeat every 500ms
  │  └─> Node 2 (Follower)
  │  └─> Node 3 (Follower)

[Heartbeat stops from Node 1 (crashed)]

ELECTION (Term 2)
  Node 1: [DEAD]

  Node 2 & 3 timeout together or sequentially
    Node 2: "Candidacy! I want term 2, vote for me?"
    Node 3: "No, I want term 2 too"

    [Vote race - one succeeds]

    Winner: "I'm Leader! Everyone: term=2"
    Loser: "OK, you won. I'm Follower, term=2"

STABLE (Term 2)
  Node 1: [DEAD]
  Node 2 (Leader)
  ├─ heartbeat every 500ms
  │  └─> Node 3 (Follower)

[Node 1 restarts]

REJOIN (Still Term 2)
  Node 1: "Hi, I'm Node 1, what term is it?"
  Node 2: "It's term 2, you're old. Here's the log."
  Node 1: "Got it. term=2, I'm Follower."

STABLE (Still Term 2)
  Node 1 (Follower) - catching up
  Node 2 (Leader)
  └─ heartbeat every 500ms
     ├─> Node 1 (Follower) [getting AppendEntries]
     └─> Node 3 (Follower)
```

---

## Expected Timeline for Full Test Run

| Phase | Duration | Activity |
|-------|----------|----------|
| TEST 1: Setup | < 5s | Verify initial cluster |
| [Manual stop] | ~ | Stop Node 1 (manual action) |
| TEST 2: Election | 3-5s | Wait for new leader election |
| TEST 3: Verify | 1-2s | Check 2-node operation |
| [Manual restart] | ~ | Restart Node 1 (manual action) |
| TEST 4: Rejoin | 3-5s | Wait for node to rejoin |
| TEST 5: Verify | 1-2s | Check full recovery |
| [Manual stop follower] | ~ | Stop Node 3 (manual action) |
| TEST 6: Partition | 3-5s | Verify minority cannot lead |
| Total | ~30s | All automated waits combined |

**Add ~30-60s for manual actions** (stopping/restarting nodes)

---

## Summary Checklist ✅

After running the test suite, verify:

- [ ] Single leader elected initially
- [ ] Term increments during elections
- [ ] Failed leader's term < new leader's term
- [ ] New leader elected within 5 seconds
- [ ] 2-node cluster continues operating
- [ ] Restarted node rejoins with current term
- [ ] All 3 nodes operational after recovery
- [ ] Majority quorum (2/3) tolerates 1 failure
- [ ] Minority partition (1/3) cannot elect leader
- [ ] No two simultaneous leaders detected
