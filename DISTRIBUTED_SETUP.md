# Distributed Setup Configuration

## Network Node Configuration

The cluster is now configured for distributed deployment across three devices with the following Wi-Fi IPs:

| Node | Device IP | HTTP Port | RPC Port |
|------|-----------|-----------|----------|
| 1 | 10.40.49.211 | 8001 | 7001 |
| 2 | 10.40.41.162 | 8002 | 7002 |
| 3 | 10.40.39.217 | 8003 | 7003 |

## UI Configuration

The React UI (raft-client-ui) has been updated with the distributed IPs in:
- `src/api/client.ts` - DEFAULT_NODES array

```typescript
const DEFAULT_NODES = [
  { id: 1, httpAddr: 'http://10.40.49.211:8001' },
  { id: 2, httpAddr: 'http://10.40.41.162:8002' },
  { id: 3, httpAddr: 'http://10.40.39.217:8003' },
];
```

## Backend Node Startup Commands

### Node 1 (10.40.49.211)
```bash
cd raft-openraft-demo
cargo run -- \
  --id 1 \
  --http-addr 10.40.49.211:8001 \
  --rpc-addr 10.40.49.211:7001 \
  --peers "2=10.40.41.162:7002,3=10.40.39.217:7003" \
  --http-peers "2=10.40.41.162:8002,3=10.40.39.217:8003"
```

### Node 2 (10.40.41.162)
```bash
cd raft-openraft-demo
cargo run -- \
  --id 2 \
  --http-addr 10.40.41.162:8002 \
  --rpc-addr 10.40.41.162:7002 \
  --peers "1=10.40.49.211:7001,3=10.40.39.217:7003" \
  --http-peers "1=10.40.49.211:8001,3=10.40.39.217:8003"
```

### Node 3 (10.40.39.217)
```bash
cd raft-openraft-demo
cargo run -- \
  --id 3 \
  --http-addr 10.40.39.217:8003 \
  --rpc-addr 10.40.39.217:7003 \
  --peers "1=10.40.49.211:7001,2=10.40.41.162:7002" \
  --http-peers "1=10.40.49.211:8001,2=10.40.41.162:8002"
```

## Firewall Configuration

### Windows Firewall (on each node)

Allow inbound RPC and HTTP:
```powershell
# Allow RPC ports (7001, 7002, 7003)
New-NetFirewallRule -DisplayName "Raft RPC 7001" -Direction Inbound -LocalPort 7001 -Protocol TCP -Action Allow
New-NetFirewallRule -DisplayName "Raft RPC 7002" -Direction Inbound -LocalPort 7002 -Protocol TCP -Action Allow
New-NetFirewallRule -DisplayName "Raft RPC 7003" -Direction Inbound -LocalPort 7003 -Protocol TCP -Action Allow

# Allow HTTP ports (8001, 8002, 8003)
New-NetFirewallRule -DisplayName "Raft HTTP 8001" -Direction Inbound -LocalPort 8001 -Protocol TCP -Action Allow
New-NetFirewallRule -DisplayName "Raft HTTP 8002" -Direction Inbound -LocalPort 8002 -Protocol TCP -Action Allow
New-NetFirewallRule -DisplayName "Raft HTTP 8003" -Direction Inbound -LocalPort 8003 -Protocol TCP -Action Allow
```

### Linux Firewall (UFW)

```bash
# Allow RPC ports
sudo ufw allow 7001/tcp
sudo ufw allow 7002/tcp
sudo ufw allow 7003/tcp

# Allow HTTP ports
sudo ufw allow 8001/tcp
sudo ufw allow 8002/tcp
sudo ufw allow 8003/tcp
```

## Testing Connectivity

### Test from any device to Node 1:
```bash
# Check HTTP connectivity
curl http://10.40.49.211:8001/

# On Windows PowerShell
Test-NetConnection -ComputerName 10.40.49.211 -Port 8001
```

### Initialize the cluster (from Node 1):
```bash
curl -X POST http://10.40.49.211:8001/cluster/init
```

### Check cluster health:
```bash
curl http://10.40.49.211:8001/metrics
curl http://10.40.41.162:8002/metrics
curl http://10.40.39.217:8003/metrics
```

## UI Access

Once all three nodes are running, you can access the UI from any device on the Wi-Fi network:

```
http://10.40.49.211:5173    # If running dev server on Node 1
```

Or build and run the production version:
```bash
npm run build
npm run preview  # Opens on http://localhost:4173
```

## Troubleshooting

### Connection refused errors
- Verify firewall rules are applied on all three devices
- Ensure IPs match the Wi-Fi interface (not VPN or other adapters)
- Check with: `ipconfig` (Windows) or `ip addr` (Linux)

### RPC connection errors between nodes
- Verify both nodes can ping each other: `ping 10.40.x.x`
- Check that RPC ports (7001-7003) are not blocked
- Ensure peer addresses in startup commands match actual device IPs

### HTTP API not responding
- Verify the node is running with correct IP binding
- Check logs for "HTTP API listening on" message
- Verify firewall allows port 8001/8002/8003
