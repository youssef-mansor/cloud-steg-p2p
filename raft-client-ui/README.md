# Raft Cluster Client UI# React + TypeScript + Vite



A modern web-based client for managing and monitoring a Raft distributed cluster with image encryption capabilities and real-time performance monitoring.This template provides a minimal setup to get React working in Vite with HMR and some ESLint rules.



## FeaturesCurrently, two official plugins are available:



### Tab 1: Image Encryption & Stress Testing- [@vitejs/plugin-react](https://github.com/vitejs/vite-plugin-react/blob/main/packages/plugin-react) uses [Babel](https://babeljs.io/) (or [oxc](https://oxc.rs) when used in [rolldown-vite](https://vite.dev/guide/rolldown)) for Fast Refresh

- **Upload & Encrypt Images**: Upload an image and send it to the cluster for steganography encryption- [@vitejs/plugin-react-swc](https://github.com/vitejs/vite-plugin-react/blob/main/packages/plugin-react-swc) uses [SWC](https://swc.rs/) for Fast Refresh

- **Download Encrypted Images**: Download the encrypted result

- **Stress Testing**: Run concurrent load tests with configurable parameters## React Compiler

  - Configure total requests and concurrent threads

  - Real-time progress trackingThe React Compiler is not enabled on this template because of its impact on dev & build performances. To add it, see [this documentation](https://react.dev/learn/react-compiler/installation).

  - Per-node success/failure statistics

  - Average latency and throughput metrics## Expanding the ESLint configuration



### Tab 2: Cluster MonitoringIf you are developing a production application, we recommend updating the configuration to enable type-aware lint rules:

- **Real-time Cluster Status**: View live status of all nodes

- **Leader Detection**: See which node is currently the leader and the election term```js

- **Node Health**: Monitor which nodes are online/offlineexport default defineConfig([

- **Performance Graphs**: Time-series chart showing requests per second for each node  globalIgnores(['dist']),

- **Detailed Statistics**: Success rates, failure counts, and latency per node  {

- **Auto-refresh**: Configurable auto-polling (default: every 2 seconds)    files: ['**/*.{ts,tsx}'],

    extends: [

## Prerequisites      // Other configs...



- **Node.js**: v18 or higher      // Remove tseslint.configs.recommended and replace with this

- **npm**: v9 or higher      tseslint.configs.recommendedTypeChecked,

- **Rust Servers**: The 3 Raft servers must be running (see server setup below)      // Alternatively, use this for stricter rules

      tseslint.configs.strictTypeChecked,

## Quick Start      // Optionally, add this for stylistic rules

      tseslint.configs.stylisticTypeChecked,

### 1. Server Setup (On Server Machines)

      // Other configs...

First, make sure CORS is enabled on the Rust servers (already applied if you followed the main guide).    ],

    languageOptions: {

**Terminal 1 - Node 1:**      parserOptions: {

```bash        project: ['./tsconfig.node.json', './tsconfig.app.json'],

cd raft-openraft-demo        tsconfigRootDir: import.meta.dirname,

cargo run --release -- --id 1 --http-addr 0.0.0.0:8001 --rpc-addr 0.0.0.0:7001 --peers "2=127.0.0.1:7002,3=127.0.0.1:7003"      },

```      // other options...

    },

**Terminal 2 - Node 2:**  },

```bash])

cargo run --release -- --id 2 --http-addr 0.0.0.0:8002 --rpc-addr 0.0.0.0:7002 --peers "1=127.0.0.1:7001,3=127.0.0.1:7003"```

```

You can also install [eslint-plugin-react-x](https://github.com/Rel1cx/eslint-react/tree/main/packages/plugins/eslint-plugin-react-x) and [eslint-plugin-react-dom](https://github.com/Rel1cx/eslint-react/tree/main/packages/plugins/eslint-plugin-react-dom) for React-specific lint rules:

**Terminal 3 - Node 3:**

```bash```js

cargo run --release -- --id 3 --http-addr 0.0.0.0:8003 --rpc-addr 0.0.0.0:7003 --peers "1=127.0.0.1:7001,2=127.0.0.1:7002"// eslint.config.js

```import reactX from 'eslint-plugin-react-x'

import reactDom from 'eslint-plugin-react-dom'

**Terminal 4 - Initialize Cluster:**

```bashexport default defineConfig([

cd raft-openraft-demo  globalIgnores(['dist']),

./setup.sh  # On Unix/Mac  {

# Or manually: curl -X POST http://localhost:8001/cluster/init    files: ['**/*.{ts,tsx}'],

```    extends: [

      // Other configs...

### 2. Client UI Setup (On Your Local Machine / 4th Device)      // Enable lint rules for React

      reactX.configs['recommended-typescript'],

#### Installation      // Enable lint rules for React DOM

      reactDom.configs.recommended,

```bash    ],

# Navigate to the client UI folder    languageOptions: {

cd raft-client-ui      parserOptions: {

        project: ['./tsconfig.node.json', './tsconfig.app.json'],

# Install dependencies (if not already done)        tsconfigRootDir: import.meta.dirname,

npm install      },

      // other options...

# Start development server    },

npm run dev  },

```])

```

The UI will be available at: **http://localhost:5173**

#### Build for Production

```bash
# Build optimized production bundle
npm run build

# Preview the production build
npm run preview
```

The built files will be in the `dist/` folder.

## Configuration

### Changing Server Addresses

If your servers are on different machines or use different ports, edit `src/api/client.ts`:

```typescript
const DEFAULT_NODES = [
  { id: 1, httpAddr: 'http://192.168.1.100:8001' },  // Change IPs as needed
  { id: 2, httpAddr: 'http://192.168.1.101:8002' },
  { id: 3, httpAddr: 'http://192.168.1.102:8003' },
];
```

### Adjusting Polling Interval

In `src/hooks/useClusterMetrics.ts`, change the default interval:

```typescript
export function useClusterMetrics(pollingInterval = 2000) {  // Change 2000 to desired ms
```

## Running on a Different Physical Device

### Option A: Development Mode (Recommended for Testing)

1. **On the client device**, ensure you can reach the server IPs:
   ```bash
   # Test connectivity (PowerShell)
   Test-NetConnection -ComputerName <server-ip> -Port 8001
   ```

2. **Start the dev server** with network access:
   ```bash
   npm run dev -- --host
   ```

3. **Access from any device** on the same network:
   ```
   http://<client-device-ip>:5173
   ```

### Option B: Production Build (Recommended for Deployment)

1. **Build the application:**
   ```bash
   npm run build
   ```

2. **Serve the static files** using any web server:

   **Using Python (simple):**
   ```bash
   cd dist
   python -m http.server 8080
   ```

   **Using Node.js serve:**
   ```bash
   npx serve -s dist -p 8080
   ```

   **Using IIS (Windows):**
   - Copy `dist` folder to `C:\inetpub\wwwroot\raft-client`
   - Create new website in IIS Manager
   - Point to the folder and set port 8080

3. **Access from browser:**
   ```
   http://<client-device-ip>:8080
   ```

### Option C: Deploy to Cloud/VPS

Upload the `dist/` folder to any static hosting service:
- **Netlify**: Drag & drop the `dist` folder
- **Vercel**: `vercel --prod`
- **GitHub Pages**: Push to `gh-pages` branch
- **AWS S3 + CloudFront**: Upload to S3 bucket

## Network Configuration

### Firewall Rules

Ensure these ports are open on the server machines:
- **8001, 8002, 8003**: HTTP API endpoints
- **7001, 7002, 7003**: Raft RPC endpoints (internal)

On **Windows** servers (PowerShell as Administrator):
```powershell
New-NetFirewallRule -DisplayName "Raft HTTP 8001" -Direction Inbound -LocalPort 8001 -Protocol TCP -Action Allow
New-NetFirewallRule -DisplayName "Raft HTTP 8002" -Direction Inbound -LocalPort 8002 -Protocol TCP -Action Allow
New-NetFirewallRule -DisplayName "Raft HTTP 8003" -Direction Inbound -LocalPort 8003 -Protocol TCP -Action Allow
```

On **Linux** servers:
```bash
sudo ufw allow 8001/tcp
sudo ufw allow 8002/tcp
sudo ufw allow 8003/tcp
```

### Router Configuration (if servers are on different networks)

If servers and client are on different networks, you'll need:
1. **Port forwarding** on the router where servers are located
2. **Dynamic DNS** if servers don't have static IPs
3. **VPN** for secure cross-network communication (recommended)

## Usage Guide

### Image Encryption

1. Click **"Select Image"** to choose a PNG/JPG file
2. Click **"Encrypt & Get Steganography Image"** to send to cluster
3. The leader will forward the request to a random healthy node
4. View processing time and which node handled the request
5. Click **"Download Encrypted Image"** to save the result

### Stress Testing

1. Select an image first
2. Configure **Total Requests** (e.g., 1000)
3. Configure **Concurrent Threads** (e.g., 10)
4. Click **"Start Stress Test"**
5. Monitor real-time progress bar
6. View per-node statistics after completion

### Cluster Monitoring

1. **Auto-refresh** is enabled by default (every 2 seconds)
2. View leader node (marked with crown 👑)
3. Check current election term
4. Monitor node status (Online/Offline)
5. View real-time requests/second graph
6. Check detailed statistics table for success rates

## Troubleshooting

### Cannot Connect to Servers

**Problem**: "Failed to fetch" errors in browser console

**Solutions**:
1. Verify servers are running: `curl http://localhost:8001/`
2. Check CORS is enabled (look for "CORS enabled" in server logs)
3. Verify firewall allows connections
4. Check network connectivity: `ping <server-ip>` or `Test-NetConnection <server-ip> -Port 8001`
5. Ensure server addresses in `src/api/client.ts` are correct

### CORS Errors

**Problem**: "CORS policy: No 'Access-Control-Allow-Origin' header"

**Solutions**:
1. Ensure servers are built with CORS support (check `Cargo.toml` has `tower-http`)
2. Rebuild servers: `cargo build --release`
3. Restart all server nodes

### Performance Issues

**Problem**: Slow response times or UI freezing

**Solutions**:
1. Reduce polling interval in cluster monitoring
2. Lower concurrent threads in stress tests
3. Check server CPU/memory usage
4. Ensure servers are running in `--release` mode

### Stress Test Not Working

**Problem**: All requests failing during stress test

**Solutions**:
1. Ensure cluster is initialized (`./setup.sh` or manual POST to `/cluster/init`)
2. Check that at least one node is leader
3. Verify selected image is valid
4. Try with smaller request count first (e.g., 10 requests)

## Development

### Project Structure

```
raft-client-ui/
├── src/
│   ├── api/
│   │   └── client.ts          # API client for Raft servers
│   ├── components/
│   │   ├── ImageEncryptionTab.tsx    # Tab 1: Image upload & stress test
│   │   └── ClusterMonitoringTab.tsx  # Tab 2: Cluster monitoring
│   ├── hooks/
│   │   └── useClusterMetrics.ts      # React hook for polling metrics
│   ├── types.ts               # TypeScript type definitions
│   ├── App.tsx                # Main app component
│   ├── main.tsx               # Entry point
│   └── index.css              # Global styles
├── package.json
├── tsconfig.json
├── vite.config.ts
└── README.md
```

### Tech Stack

- **React 18**: UI framework
- **TypeScript**: Type safety
- **Vite**: Build tool and dev server
- **Recharts**: Real-time graphing
- **Lucide React**: Icons
- **Fetch API**: HTTP requests

## License

Part of the cloud-steg-p2p Raft demonstration project.
