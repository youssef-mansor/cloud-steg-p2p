# Distributed Deployment Guide

## Overview

This guide explains how to deploy the Raft load balancing system across multiple machines on the same network, instead of running everything locally on one machine.

## Network Architecture

### Local Setup (Current)
```
Single Machine:
  - Server 1 (Node 1): localhost:8001, localhost:7001
  - Server 2 (Node 2): localhost:8002, localhost:7002
  - Server 3 (Node 3): localhost:8003, localhost:7003
  - Client: localhost
```

### Distributed Setup (Target)
```
PC 1 (e.g., 192.168.1.10):
  - Server 1 (Node 1): 192.168.1.10:8001, 192.168.1.10:7001

PC 2 (e.g., 192.168.1.11):
  - Server 2 (Node 2): 192.168.1.11:8002, 192.168.1.11:7002

PC 3 (e.g., 192.168.1.12):
  - Server 3 (Node 3): 192.168.1.12:8003, 192.168.1.12:7003

PC 4 (e.g., 192.168.1.13):
  - Client: Connects to all 3 servers
```

## Prerequisites

1. **All machines on the same network** (same subnet)
2. **Firewall rules configured** to allow:
   - HTTP ports: 8001, 8002, 8003 (for client requests)
   - RPC ports: 7001, 7002, 7003 (for Raft communication)
3. **All machines can reach each other** (ping test)
4. **Rust and Cargo installed** on all server machines (PC 1, 2, 3)
5. **Python 3 installed** on the client machine (PC 4)

## Step-by-Step Deployment

### Step 1: Get IP Addresses

On each machine, find its IP address:

**Linux/macOS:**
```bash
ifconfig | grep "inet " | grep -v 127.0.0.1
# OR
ip addr show | grep "inet "
```

**Windows:**
```cmd
ipconfig
```

**Example outputs:**
- PC 1: `192.168.1.10`
- PC 2: `192.168.1.11`
- PC 3: `192.168.1.12`
- PC 4: `192.168.1.13`

### Step 2: Build on Server Machines

On **each server machine** (PC 1, PC 2, PC 3):

```bash
cd /path/to/raft-openraft-demo
cargo build --release
```

### Step 3: Start Server 1 (PC 1)

**On PC 1 (IP: 192.168.1.10):**

```bash
cd /path/to/raft-openraft-demo
cargo run --release -- \
  --id 1 \
  --http-addr 0.0.0.0:8001 \
  --rpc-addr 0.0.0.0:7001 \
  --peers "2=192.168.1.11:7002,3=192.168.1.12:7003"
```

**Changes from local:**
- `--peers` now uses actual IPs: `2=192.168.1.11:7002,3=192.168.1.12:7003`
- `--http-addr` and `--rpc-addr` stay `0.0.0.0` (listens on all interfaces)

**Wait for:** `🌐 HTTP API listening on 0.0.0.0:8001`

### Step 4: Start Server 2 (PC 2)

**On PC 2 (IP: 192.168.1.11):**

```bash
cd /path/to/raft-openraft-demo
cargo run --release -- \
  --id 2 \
  --http-addr 0.0.0.0:8002 \
  --rpc-addr 0.0.0.0:7002 \
  --peers "1=192.168.1.10:7001,3=192.168.1.12:7003"
```

**Changes from local:**
- `--peers` uses actual IPs: `1=192.168.1.10:7001,3=192.168.1.12:7003`

**Wait for:** `🌐 HTTP API listening on 0.0.0.0:8002`

### Step 5: Start Server 3 (PC 3)

**On PC 3 (IP: 192.168.1.12):**

```bash
cd /path/to/raft-openraft-demo
cargo run --release -- \
  --id 3 \
  --http-addr 0.0.0.0:8003 \
  --rpc-addr 0.0.0.0:7003 \
  --peers "1=192.168.1.10:7001,2=192.168.1.11:7002"
```

**Changes from local:**
- `--peers` uses actual IPs: `1=192.168.1.10:7001,2=192.168.1.11:7002`

**Wait for:** `🌐 HTTP API listening on 0.0.0.0:8003`

### Step 6: Initialize Cluster

**On any machine** (or from PC 4/client):

```bash
cd /path/to/raft-openraft-demo

# Update setup.sh or run manually:
curl -X POST http://192.168.1.10:8001/cluster/init \
  -H 'Content-Type: application/json' -d '{}'

curl -X POST http://192.168.1.10:8001/cluster/add-learner \
  -H 'Content-Type: application/json' \
  -d '{"node_id": 2, "address": "192.168.1.11:7002"}'

curl -X POST http://192.168.1.10:8001/cluster/add-learner \
  -H 'Content-Type: application/json' \
  -d '{"node_id": 3, "address": "192.168.1.12:7003"}'

curl -X POST http://192.168.1.10:8001/cluster/change-membership \
  -H 'Content-Type: application/json' \
  -d '{"members": [1, 2, 3]}'
```

**Changes from local:**
- All `127.0.0.1` replaced with actual IP: `192.168.1.10`
- Peer addresses use actual IPs: `192.168.1.11:7002`, `192.168.1.12:7003`

### Step 7: Configure Client (PC 4)

**On PC 4 (Client machine):**

1. **Copy the project** (or clone from git):
   ```bash
   # Copy test_load_balancing_advanced.py and input-image.png
   ```

2. **Modify the test script** to use server IPs instead of localhost:

   Edit `test_load_balancing_advanced.py`:
   
   Find the multicast function (around line 348):
   ```python
   def multicast_request(request_num):
       ports = [8001, 8002, 8003]  # These stay the same
   ```
   
   Change to use IP addresses:
   ```python
   # Add at top of file or in configuration
   SERVER_IPS = {
       1: "192.168.1.10",  # PC 1
       2: "192.168.1.11",  # PC 2
       3: "192.168.1.12",  # PC 3
   }
   
   def multicast_request(request_num):
       # Build URLs with actual IPs
       urls = [
           f"http://{SERVER_IPS[1]}:8001/image/steg",
           f"http://{SERVER_IPS[2]}:8002/image/steg",
           f"http://{SERVER_IPS[3]}:8003/image/steg",
       ]
   ```

3. **Update server detection** in the script:
   
   Find the `main()` function where it checks server status:
   ```python
   # Change from:
   response = requests.get(f"http://127.0.0.1:{8000 + node_id}/metrics", timeout=2)
   
   # To:
   response = requests.get(f"http://{SERVER_IPS[node_id]}:{8000 + node_id}/metrics", timeout=2)
   ```

4. **Update `find_server_pid()` function**:
   
   Since servers are remote, this function won't work. You have two options:
   
   **Option A:** Disable PID detection (recommended for distributed):
   ```python
   def find_server_pid(node_id):
       # For distributed deployment, we can't get PID of remote servers
       # Just check if HTTP endpoint responds
       return None  # Or return a dummy value
   ```
   
   **Option B:** Keep it but know it won't work for remote servers (the script will skip automatic restarts)

5. **Disable automatic server management** (recommended):
   
   The script's automatic server restart feature won't work for remote servers. You can either:
   - Comment out the server manager thread
   - Or manually manage servers from their respective machines

### Step 8: Run the Test

**On PC 4 (Client):**

```bash
cd /path/to/raft-openraft-demo
python3 test_load_balancing_advanced.py
```

## Firewall Configuration

### Linux (ufw)

On each server machine:
```bash
sudo ufw allow 8001/tcp  # Node 1 HTTP
sudo ufw allow 8002/tcp  # Node 2 HTTP
sudo ufw allow 8003/tcp  # Node 3 HTTP
sudo ufw allow 7001/tcp  # Node 1 RPC
sudo ufw allow 7002/tcp  # Node 2 RPC
sudo ufw allow 7003/tcp  # Node 3 RPC
```

### macOS

```bash
# Allow ports through System Preferences > Security & Privacy > Firewall > Firewall Options
# Or use pfctl (advanced)
```

### Windows

```cmd
netsh advfirewall firewall add rule name="Raft HTTP 1" dir=in action=allow protocol=TCP localport=8001
netsh advfirewall firewall add rule name="Raft HTTP 2" dir=in action=allow protocol=TCP localport=8002
netsh advfirewall firewall add rule name="Raft HTTP 3" dir=in action=allow protocol=TCP localport=8003
netsh advfirewall firewall add rule name="Raft RPC 1" dir=in action=allow protocol=TCP localport=7001
netsh advfirewall firewall add rule name="Raft RPC 2" dir=in action=allow protocol=TCP localport=7002
netsh advfirewall firewall add rule name="Raft RPC 3" dir=in action=allow protocol=TCP localport=7003
```

## Network Testing

Before running the test, verify connectivity:

**From PC 4 (Client), test each server:**
```bash
# Test HTTP endpoints
curl http://192.168.1.10:8001/metrics
curl http://192.168.1.11:8002/metrics
curl http://192.168.1.12:8003/metrics

# Test RPC ports (should timeout, but connection should be established)
telnet 192.168.1.10 7001
telnet 192.168.1.11 7002
telnet 192.168.1.12 7003
```

**From each server, test RPC connectivity:**
```bash
# From PC 1, test PC 2 and PC 3 RPC ports
telnet 192.168.1.11 7002
telnet 192.168.1.12 7003
```

## Important Notes

### 1. IP Address Binding

- Servers must bind to `0.0.0.0` (all interfaces), NOT `127.0.0.1`
- `0.0.0.0:8001` means "listen on port 8001 on all network interfaces"
- `127.0.0.1:8001` means "only listen on localhost" (won't accept remote connections)

### 2. Server Discovery

The servers automatically discover each other's HTTP addresses based on:
- RPC peer addresses
- Assumption: HTTP port = RPC port + 1000

This works if all servers follow the pattern:
- RPC: `7000 + node_id`
- HTTP: `8000 + node_id`

### 3. Client Multicasting

The client sends requests to all 3 servers simultaneously. This requires:
- Network latency should be low (< 10ms typical for LAN)
- All 3 servers should be reachable from the client
- Client must handle cases where some servers are unreachable

### 4. Automatic Server Management

The `test_load_balancing_advanced.py` script's automatic server restart feature **will NOT work** for distributed deployment because:
- The script can't kill/start processes on remote machines
- PID detection only works for local processes

**Solutions:**
- Manually restart servers from their respective machines
- Use SSH-based remote management (not included in current script)
- Use orchestration tools (Docker Swarm, Kubernetes, etc.)

### 5. Network Latency

Expected latency in distributed setup:
- **Local (current):** ~0.02-0.05 seconds
- **LAN (distributed):** ~0.05-0.15 seconds (depends on network)
- **Latency increases** due to network hops and TCP overhead

## Troubleshooting

### Servers Can't Connect to Each Other

**Symptoms:**
- Logs show connection errors
- Cluster never forms

**Solutions:**
1. Check firewall rules (see above)
2. Verify IP addresses are correct
3. Test with `telnet` or `nc`:
   ```bash
   telnet <peer_ip> <rpc_port>
   ```
4. Check if servers are on same subnet
5. Verify no router blocking inter-PC communication

### Client Can't Reach Servers

**Symptoms:**
- Test script shows connection errors
- All requests fail

**Solutions:**
1. Verify firewall allows HTTP ports (8001-8003)
2. Test with `curl`:
   ```bash
   curl http://<server_ip>:<port>/metrics
   ```
3. Check server is bound to `0.0.0.0`, not `127.0.0.1`
4. Verify IP addresses in client script are correct

### Cluster Forms But Leader Election Fails

**Symptoms:**
- Servers show "Learner" state
- No leader elected

**Solutions:**
1. Verify all servers can reach each other's RPC ports
2. Check cluster initialization completed
3. Wait longer (election can take 5-10 seconds)
4. Check server logs for Raft errors

### High Latency

**Symptoms:**
- Response times > 0.5 seconds
- Test shows slow throughput

**Solutions:**
1. Check network quality (ping times)
2. Verify all servers on same switch/VLAN
3. Check for network congestion
4. Consider using wired connections instead of WiFi

## Summary of Changes

| Component | Local Setup | Distributed Setup |
|-----------|-------------|-------------------|
| **Server peers** | `127.0.0.1:7002` | `192.168.1.11:7002` |
| **Client URLs** | `http://127.0.0.1:8001` | `http://192.168.1.10:8001` |
| **Server binding** | `0.0.0.0:8001` | `0.0.0.0:8001` (same) |
| **Firewall** | Not needed | Must allow ports |
| **Server restart** | Automatic | Manual only |
| **PID detection** | Works | Doesn't work remotely |

## Example Complete Setup

**PC 1 (192.168.1.10):**
```bash
cargo run --release -- \
  --id 1 \
  --http-addr 0.0.0.0:8001 \
  --rpc-addr 0.0.0.0:7001 \
  --peers "2=192.168.1.11:7002,3=192.168.1.12:7003"
```

**PC 2 (192.168.1.11):**
```bash
cargo run --release -- \
  --id 2 \
  --http-addr 0.0.0.0:8002 \
  --rpc-addr 0.0.0.0:7002 \
  --peers "1=192.168.1.10:7001,3=192.168.1.12:7003"
```

**PC 3 (192.168.1.12):**
```bash
cargo run --release -- \
  --id 3 \
  --http-addr 0.0.0.0:8003 \
  --rpc-addr 0.0.0.0:7003 \
  --peers "1=192.168.1.10:7001,2=192.168.1.11:7002"
```

**PC 4 (192.168.1.13) - Client:**
```python
# In test_load_balancing_advanced.py:
SERVER_IPS = {
    1: "192.168.1.10",
    2: "192.168.1.11", 
    3: "192.168.1.12",
}
```

Then run:
```bash
python3 test_load_balancing_advanced.py
```

