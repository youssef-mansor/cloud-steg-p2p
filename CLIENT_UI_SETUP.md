# Quick Setup Guide - Raft Client UI on 4th Device

This guide walks you through setting up the web-based Raft client UI on a separate physical device from your 3 server nodes.

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│         Device 4: Client UI (This Machine)                   │
│         - React Web App                                      │
│         - Runs on http://localhost:5173                      │
│         - Or accessible at http://<your-ip>:5173             │
└─────────────────────────────────────────────────────────────┘
                          │
            ┌─────────────┼─────────────┐
            │             │             │
            ▼             ▼             ▼
    ┌─────────────┐ ┌─────────────┐ ┌─────────────┐
    │  Server 1   │ │  Server 2   │ │  Server 3   │
    │  :8001      │ │  :8002      │ │  :8003      │
    │  (Leader)   │ │  (Follower) │ │  (Follower) │
    └─────────────┘ └─────────────┘ └─────────────┘
```

## Prerequisites

### On Server Machines (Devices 1, 2, 3)
- ✅ Rust & Cargo installed
- ✅ CORS enabled (already done if you followed main setup)
- ✅ Servers running on ports 8001, 8002, 8003
- ✅ Firewall allows incoming connections on these ports

### On Client Machine (Device 4)
- ✅ Node.js v18+ installed
- ✅ npm v9+ installed
- ✅ Network access to server machines

## Step-by-Step Setup

### 1. Verify Server Setup

On **each server machine**, ensure the Rust server is running:

```powershell
# Check if server is responding
Test-NetConnection -ComputerName localhost -Port 8001
curl http://localhost:8001/
```

You should see JSON response with node info.

### 2. Install Client UI (On Device 4)

```powershell
# Clone or navigate to the project
cd E:\cloud-steg-p2p\raft-client-ui

# Install dependencies
npm install
```

### 3. Configure Server Addresses

If servers are on **different machines** (not localhost), edit the configuration:

**File**: `src\api\client.ts`

Change the DEFAULT_NODES to match your server IPs:

```typescript
const DEFAULT_NODES = [
  { id: 1, httpAddr: 'http://192.168.1.100:8001' },  // Server 1 IP
  { id: 2, httpAddr: 'http://192.168.1.101:8002' },  // Server 2 IP
  { id: 3, httpAddr: 'http://192.168.1.102:8003' },  // Server 3 IP
];
```

**If servers are on localhost** (same machine), leave as is:
```typescript
const DEFAULT_NODES = [
  { id: 1, httpAddr: 'http://localhost:8001' },
  { id: 2, httpAddr: 'http://localhost:8002' },
  { id: 3, httpAddr: 'http://localhost:8003' },
];
```

### 4. Start the Client UI

**Option A: Development Mode** (with hot-reload)

```powershell
# Start dev server (accessible only from this machine)
npm run dev

# OR start with network access (accessible from other devices)
npm run dev -- --host
```

**Access the UI:**
- Local: http://localhost:5173
- From other devices: http://<your-device-ip>:5173

**Option B: Production Build** (optimized, faster)

```powershell
# Build the app
npm run build

# Serve the production build
npm run preview

# OR use a simple HTTP server
cd dist
python -m http.server 8080
```

**Access the UI:**
- http://localhost:8080 (or 5173 if using preview)

### 5. Test Connectivity

Once the UI is running:

1. **Open in browser**: http://localhost:5173
2. **Go to "Cluster Monitoring" tab**
3. **Check node status** - you should see all 3 nodes
4. **Verify leader** - one node should show 👑 crown icon

If nodes show as **Offline**:
- Check firewall rules on server machines
- Verify server IPs in `src/api/client.ts`
- Test connectivity: `Test-NetConnection <server-ip> -Port 8001`

### 6. Use the UI

#### **Tab 1: Image Encryption & Testing**
- Upload an image
- Click "Encrypt" to process through cluster
- Download the encrypted result
- Run stress tests with configurable parameters

#### **Tab 2: Cluster Monitoring**
- View real-time node status
- See leader and election term
- Monitor requests per second
- View performance graphs

## Network Configuration

### Firewall Rules (On Server Machines)

**Windows (PowerShell as Admin):**
```powershell
New-NetFirewallRule -DisplayName "Raft HTTP 8001" -Direction Inbound -LocalPort 8001 -Protocol TCP -Action Allow
New-NetFirewallRule -DisplayName "Raft HTTP 8002" -Direction Inbound -LocalPort 8002 -Protocol TCP -Action Allow
New-NetFirewallRule -DisplayName "Raft HTTP 8003" -Direction Inbound -LocalPort 8003 -Protocol TCP -Action Allow
```

**Linux:**
```bash
sudo ufw allow 8001/tcp
sudo ufw allow 8002/tcp
sudo ufw allow 8003/tcp
```

### Testing Network Connectivity

**From Client Device (Device 4):**

```powershell
# Test if server is reachable
Test-NetConnection -ComputerName <server-ip> -Port 8001

# OR use curl
curl http://<server-ip>:8001/

# Expected response: JSON with node_id, status, etc.
```

## Troubleshooting

### ❌ "Cannot connect to servers"

**Check:**
1. Servers are running: `curl http://<server-ip>:8001/`
2. Firewall allows port 8001, 8002, 8003
3. Correct IPs in `src/api/client.ts`
4. Network connectivity: `ping <server-ip>`

### ❌ "CORS policy error"

**Fix:**
1. Verify `Cargo.toml` includes `tower-http = { version = "0.5", features = ["cors"] }`
2. Rebuild servers: `cargo build --release`
3. Restart all server nodes
4. Check server logs for "CORS enabled" message

### ❌ "All nodes show offline"

**Fix:**
1. Initialize cluster: `curl -X POST http://<server-ip>:8001/cluster/init`
2. Wait 5-10 seconds for leader election
3. Refresh UI (click Refresh button or toggle Auto-refresh)

### ❌ "Stress test failures"

**Fix:**
1. Ensure cluster has a leader (check monitoring tab)
2. Start with small test: 10 requests, 2 threads
3. Verify image is valid PNG/JPG
4. Check server logs for errors

## Advanced: Deploy to Other Devices

### Share UI with colleagues on same network:

```powershell
# Start with host flag
npm run dev -- --host

# Find your IP
ipconfig  # Look for IPv4 Address

# Share URL with others:
# http://<your-ip>:5173
```

### Build and deploy to web server:

```powershell
# Build production bundle
npm run build

# Upload dist/ folder to web server
# - IIS (Windows Server)
# - nginx (Linux)
# - Apache
# - Or any static hosting (Netlify, Vercel, etc.)
```

## Quick Reference

| Action | Command |
|--------|---------|
| Install dependencies | `npm install` |
| Start dev server | `npm run dev` |
| Start dev server (network) | `npm run dev -- --host` |
| Build for production | `npm run build` |
| Preview production build | `npm run preview` |
| Test server connectivity | `Test-NetConnection <ip> -Port 8001` |
| Check if running | `curl http://localhost:8001/` |

## Next Steps

1. ✅ Servers running with CORS enabled
2. ✅ Client UI installed and configured
3. ✅ Network connectivity verified
4. ✅ Firewall rules configured
5. 🚀 **Start using the UI!**

For more details, see the full README in `raft-client-ui/README.md`.
