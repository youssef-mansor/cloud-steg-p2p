# Command Reference - Raft Client UI

Quick reference for all commands you'll need.

## Server Commands (On Server Machines 1, 2, 3)

### Build Servers (One-time, or after changes)
```powershell
cd E:\cloud-steg-p2p\raft-openraft-demo
cargo build --release
```

### Start Node 1
```powershell
cd E:\cloud-steg-p2p\raft-openraft-demo
cargo run --release -- --id 1 --http-addr 0.0.0.0:8001 --rpc-addr 0.0.0.0:7001 --peers "2=127.0.0.1:7002,3=127.0.0.1:7003"
```

### Start Node 2
```powershell
cd E:\cloud-steg-p2p\raft-openraft-demo
cargo run --release -- --id 2 --http-addr 0.0.0.0:8002 --rpc-addr 0.0.0.0:7002 --peers "1=127.0.0.1:7001,3=127.0.0.1:7003"
```

### Start Node 3
```powershell
cd E:\cloud-steg-p2p\raft-openraft-demo
cargo run --release -- --id 3 --http-addr 0.0.0.0:8003 --rpc-addr 0.0.0.0:7003 --peers "1=127.0.0.1:7001,2=127.0.0.1:7002"
```

### Initialize Cluster (After all 3 nodes start)
```powershell
curl -X POST http://localhost:8001/cluster/init
```

### Check Node Status
```powershell
curl http://localhost:8001/metrics
curl http://localhost:8002/metrics
curl http://localhost:8003/metrics
```

---

## Client UI Commands (On Device 4)

### Install Dependencies (First time only)
```powershell
cd E:\cloud-steg-p2p\raft-client-ui
npm install
```

### Start Development Server (Local access only)
```powershell
npm run dev
# Opens at: http://localhost:5173
```

### Start Development Server (Network access)
```powershell
npm run dev -- --host
# Access from any device: http://<your-ip>:5173
```

### Build for Production
```powershell
npm run build
# Output: dist/ folder
```

### Preview Production Build
```powershell
npm run preview
# Opens at: http://localhost:4173
```

### Serve Production Build (Simple)
```powershell
cd dist
python -m http.server 8080
# Opens at: http://localhost:8080
```

---

## Network Testing Commands

### Test Server Connectivity (PowerShell)
```powershell
Test-NetConnection -ComputerName <server-ip> -Port 8001
Test-NetConnection -ComputerName localhost -Port 8001
```

### Test with cURL
```powershell
curl http://<server-ip>:8001/
curl http://localhost:8001/
```

### Ping Server
```powershell
ping <server-ip>
```

### Get Your IP Address
```powershell
ipconfig
# Look for "IPv4 Address"
```

---

## Firewall Commands (On Server Machines)

### Add Firewall Rules (PowerShell as Admin)
```powershell
New-NetFirewallRule -DisplayName "Raft HTTP 8001" -Direction Inbound -LocalPort 8001 -Protocol TCP -Action Allow
New-NetFirewallRule -DisplayName "Raft HTTP 8002" -Direction Inbound -LocalPort 8002 -Protocol TCP -Action Allow
New-NetFirewallRule -DisplayName "Raft HTTP 8003" -Direction Inbound -LocalPort 8003 -Protocol TCP -Action Allow
```

### Check Firewall Rules
```powershell
Get-NetFirewallRule -DisplayName "Raft*"
```

### Remove Firewall Rules (if needed)
```powershell
Remove-NetFirewallRule -DisplayName "Raft HTTP 8001"
Remove-NetFirewallRule -DisplayName "Raft HTTP 8002"
Remove-NetFirewallRule -DisplayName "Raft HTTP 8003"
```

---

## Troubleshooting Commands

### Check if Node.js is installed
```powershell
node --version
npm --version
```

### Check if Rust is installed
```powershell
cargo --version
rustc --version
```

### Find process using port (Windows)
```powershell
netstat -ano | findstr :8001
netstat -ano | findstr :8002
netstat -ano | findstr :8003
```

### Kill process by PID
```powershell
Stop-Process -Id <PID> -Force
```

### Clear npm cache (if install fails)
```powershell
npm cache clean --force
npm install
```

### Reinstall dependencies
```powershell
Remove-Item -Recurse -Force node_modules
Remove-Item package-lock.json
npm install
```

---

## Git Commands (Optional)

### Check status
```powershell
cd E:\cloud-steg-p2p
git status
```

### Commit changes
```powershell
git add .
git commit -m "Added web client UI"
```

### Push to remote
```powershell
git push origin rust-open-raft-election-fixed
```

---

## Quick Start Sequence

### First Time Setup:
```powershell
# 1. Build servers
cd E:\cloud-steg-p2p\raft-openraft-demo
cargo build --release

# 2. Install UI dependencies
cd E:\cloud-steg-p2p\raft-client-ui
npm install

# 3. Configure server IPs (if remote)
# Edit: src\api\client.ts
```

### Every Time You Start:
```powershell
# Terminal 1: Start Node 1
cd E:\cloud-steg-p2p\raft-openraft-demo
cargo run --release -- --id 1 --http-addr 0.0.0.0:8001 --rpc-addr 0.0.0.0:7001 --peers "2=127.0.0.1:7002,3=127.0.0.1:7003"

# Terminal 2: Start Node 2
cargo run --release -- --id 2 --http-addr 0.0.0.0:8002 --rpc-addr 0.0.0.0:7002 --peers "1=127.0.0.1:7001,3=127.0.0.1:7003"

# Terminal 3: Start Node 3
cargo run --release -- --id 3 --http-addr 0.0.0.0:8003 --rpc-addr 0.0.0.0:7003 --peers "1=127.0.0.1:7001,2=127.0.0.1:7002"

# Wait 10 seconds, then Terminal 4: Initialize
curl -X POST http://localhost:8001/cluster/init

# Terminal 5: Start UI
cd E:\cloud-steg-p2p\raft-client-ui
npm run dev
```

---

## Environment Variables (Optional)

### Set custom ports
```powershell
$env:VITE_PORT="3000"
npm run dev
```

### Set custom host
```powershell
$env:VITE_HOST="0.0.0.0"
npm run dev
```

---

## Production Deployment

### Build optimized bundle
```powershell
npm run build
```

### Deploy to static hosting
```powershell
# Upload dist/ folder to:
# - Netlify (drag & drop)
# - Vercel (vercel deploy)
# - GitHub Pages
# - AWS S3
# - Azure Static Web Apps
```

### Serve with IIS (Windows Server)
```powershell
# 1. Copy dist/ to C:\inetpub\wwwroot\raft-client
# 2. Create new site in IIS Manager
# 3. Point to folder
# 4. Set bindings (port 8080)
```

---

**Save this file for quick reference!**
