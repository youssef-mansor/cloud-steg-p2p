# ✅ SETUP COMPLETE - Raft Client UI

## What Was Built

A complete **React + TypeScript + Vite** web application for managing and monitoring your Raft cluster, designed to run on a **4th physical device** separate from your 3 server nodes.

### ✨ Features Implemented

#### 📸 Tab 1: Image Encryption & Stress Testing
- ✅ Upload images for steganography encryption
- ✅ Send to cluster leader (auto-forwards to random node)
- ✅ Download encrypted results
- ✅ **Stress Testing Engine**:
  - Configurable total requests (e.g., 1000, 10000)
  - Configurable concurrent threads (e.g., 10, 20)
  - Real-time progress bar
  - Per-node statistics (success, failure, latency)
  - Throughput tracking (requests/second)

#### 📊 Tab 2: Cluster Monitoring Dashboard
- ✅ **Real-time node status** (auto-refresh every 2 seconds)
- ✅ **Leader detection** with crown icon 👑
- ✅ **Election term tracking**
- ✅ **Node health monitoring** (Online/Offline)
- ✅ **Live performance graph** (requests/sec over time)
- ✅ **Detailed statistics table**:
  - Total requests per node
  - Success/failure counts
  - Success rate percentage
  - Average latency
  - Current throughput

### 🔧 Technical Stack

- **React 18** - Modern UI framework
- **TypeScript** - Type safety
- **Vite** - Fast build tool and dev server
- **Recharts** - Real-time data visualization
- **Lucide React** - Beautiful icons
- **CORS-enabled servers** - Cross-origin requests supported

## 📁 Project Structure

```
E:\cloud-steg-p2p\
├── CLIENT_UI_SETUP.md          # Quick setup guide (START HERE!)
├── README.md                    # Main project documentation
│
├── raft-openraft-demo/          # Rust Raft servers (already built)
│   ├── Cargo.toml              # ✅ Updated with CORS support
│   ├── src/
│   │   ├── main.rs             # ✅ CORS enabled
│   │   └── api.rs              # ✅ CORS layer applied
│   └── ...
│
└── raft-client-ui/              # 🆕 NEW Web Client UI
    ├── README.md                # Full UI documentation
    ├── package.json             # Dependencies configured
    ├── src/
    │   ├── api/
    │   │   └── client.ts        # API client for Raft servers
    │   ├── components/
    │   │   ├── ImageEncryptionTab.tsx    # Image upload & stress test
    │   │   └── ClusterMonitoringTab.tsx  # Real-time monitoring
    │   ├── hooks/
    │   │   └── useClusterMetrics.ts      # Metrics polling hook
    │   ├── types.ts             # TypeScript definitions
    │   ├── App.tsx              # Main app with tabs
    │   └── main.tsx             # Entry point
    └── dist/                    # ✅ Production build ready
```

## 🚀 How to Run (Quick Start)

### On Server Machines (1, 2, 3)

**If not already running**, start the Rust servers:

```powershell
# Terminal 1 - Node 1
cd E:\cloud-steg-p2p\raft-openraft-demo
cargo run --release -- --id 1 --http-addr 0.0.0.0:8001 --rpc-addr 0.0.0.0:7001 --peers "2=127.0.0.1:7002,3=127.0.0.1:7003"

# Terminal 2 - Node 2
cargo run --release -- --id 2 --http-addr 0.0.0.0:8002 --rpc-addr 0.0.0.0:7002 --peers "1=127.0.0.1:7001,3=127.0.0.1:7003"

# Terminal 3 - Node 3
cargo run --release -- --id 3 --http-addr 0.0.0.0:8003 --rpc-addr 0.0.0.0:7003 --peers "1=127.0.0.1:7001,2=127.0.0.1:7002"
```

**Verify CORS is enabled** - look for this in server logs:
```
🌍 CORS enabled - allowing cross-origin requests from any origin
```

### On Client Machine (Device 4)

```powershell
# Navigate to UI folder
cd E:\cloud-steg-p2p\raft-client-ui

# Start development server
npm run dev

# Open browser to:
# http://localhost:5173
```

**To access from other devices on your network:**
```powershell
npm run dev -- --host
# Then access from: http://<your-ip>:5173
```

## 🌐 Network Configuration

### If Servers are on DIFFERENT Machines

Edit `raft-client-ui\src\api\client.ts`:

```typescript
const DEFAULT_NODES = [
  { id: 1, httpAddr: 'http://192.168.1.100:8001' },  // Change to server 1 IP
  { id: 2, httpAddr: 'http://192.168.1.101:8002' },  // Change to server 2 IP
  { id: 3, httpAddr: 'http://192.168.1.102:8003' },  // Change to server 3 IP
];
```

### Firewall Rules (On Server Machines)

**Windows (PowerShell as Administrator):**
```powershell
New-NetFirewallRule -DisplayName "Raft HTTP 8001" -Direction Inbound -LocalPort 8001 -Protocol TCP -Action Allow
New-NetFirewallRule -DisplayName "Raft HTTP 8002" -Direction Inbound -LocalPort 8002 -Protocol TCP -Action Allow
New-NetFirewallRule -DisplayName "Raft HTTP 8003" -Direction Inbound -LocalPort 8003 -Protocol TCP -Action Allow
```

## ✅ Verification Checklist

### Server Side
- [ ] All 3 Rust servers running
- [ ] Cluster initialized (run `./setup.sh` or POST to `/cluster/init`)
- [ ] CORS enabled message in logs
- [ ] Ports 8001, 8002, 8003 accessible
- [ ] Leader elected (check with `curl http://localhost:8001/metrics`)

### Client Side
- [ ] Node.js v18+ installed
- [ ] Dependencies installed (`npm install`)
- [ ] Server IPs configured in `client.ts` (if remote)
- [ ] Build successful (`npm run build`)
- [ ] Dev server starts (`npm run dev`)
- [ ] Browser opens at http://localhost:5173

### Network
- [ ] Client can reach servers (test with `Test-NetConnection <ip> -Port 8001`)
- [ ] Firewall rules allow ports 8001-8003
- [ ] No CORS errors in browser console

## 🎯 Usage Examples

### Example 1: Upload & Encrypt Single Image

1. Open UI → **Image Encryption & Testing** tab
2. Click "Select Image" → choose a PNG/JPG
3. Click "Encrypt & Get Steganography Image"
4. Wait for processing (view latency)
5. Click "Download Encrypted Image"

### Example 2: Run Stress Test

1. Select an image first
2. Set **Total Requests**: 1000
3. Set **Concurrent Threads**: 10
4. Click "Start Stress Test"
5. Watch progress bar and stats update
6. View per-node results in table

### Example 3: Monitor Cluster Health

1. Open **Cluster Monitoring** tab
2. Verify all 3 nodes show as "Online"
3. Check which node is Leader (crown icon)
4. View current election term
5. Run a stress test from Tab 1
6. Watch real-time graph update with throughput

### Example 4: Simulate Node Failure

1. Go to Cluster Monitoring tab
2. Kill one server (Ctrl+C in terminal)
3. Watch node turn "Offline"
4. Observe leader re-election (if leader was killed)
5. Restart node
6. Watch it come back "Online"
7. Graph shows recovery

## 📚 Documentation

| Document | Purpose |
|----------|---------|
| `CLIENT_UI_SETUP.md` | Quick setup guide for 4th device |
| `raft-client-ui/README.md` | Full UI documentation & troubleshooting |
| `README.md` (root) | Original project & architecture docs |

## 🔍 Troubleshooting

### Can't connect to servers

```powershell
# Test connectivity
Test-NetConnection -ComputerName <server-ip> -Port 8001

# Test with curl
curl http://<server-ip>:8001/

# Expected: JSON response with node_id, status, etc.
```

### CORS errors

1. Check `Cargo.toml` has: `tower-http = { version = "0.5", features = ["cors"] }`
2. Rebuild: `cargo build --release`
3. Restart servers
4. Look for "CORS enabled" in logs

### All nodes offline in UI

1. Initialize cluster: `curl -X POST http://localhost:8001/cluster/init`
2. Wait 10 seconds for election
3. Click Refresh in UI
4. Check server logs for errors

## 🎉 What's Next?

Your complete setup includes:

✅ **3 Raft servers** running with CORS enabled
✅ **Web UI client** with monitoring & stress testing
✅ **Cross-origin support** for remote access
✅ **Real-time dashboards** for cluster health
✅ **Load testing tools** comparable to Python scripts
✅ **Production-ready build** in `dist/` folder

### Deployment Options

1. **Keep running locally** - use `npm run dev`
2. **Share on LAN** - use `npm run dev -- --host`
3. **Deploy production** - serve `dist/` folder
4. **Cloud hosting** - upload to Netlify/Vercel/AWS

## 📞 Need Help?

- Check `raft-client-ui/README.md` for detailed troubleshooting
- Review server logs for errors
- Verify network connectivity
- Ensure firewall rules are configured

---

**Built with ❤️ for distributed systems monitoring and testing**
