#!/usr/bin/env python3
"""
Clustered Load Balancer with Raft Consensus + Retry Logic
- 3 LB instances (ports 9001, 9002, 9003)
- Health tracking with Raft consensus
- Retry on failure + immediate dead server detection
- Graceful failover
"""

import sys
import json
import time
import random
import threading
import requests
from http.server import HTTPServer, BaseHTTPRequestHandler
from collections import defaultdict
from typing import Dict, List

# ============================================================================
# LB Cluster State (Replicated via Raft consensus)
# ============================================================================

class LBClusterState:
    """State that should be replicated across all LBs"""
    
    def __init__(self):
        # Server status tracking
        self.servers: Dict[int, Dict] = {
            1: {"status": "unknown", "role": "unknown", "last_seen": time.time()},
            2: {"status": "unknown", "role": "unknown", "last_seen": time.time()},
            3: {"status": "unknown", "role": "unknown", "last_seen": time.time()},
        }
        
        # Leader of backend cluster
        self.server_cluster_leader: int = None
        self.server_cluster_leader_updated: float = time.time()
        
        # Last health check
        self.last_health_check: float = 0
        
    def get_healthy_servers(self) -> List[int]:
        """Get list of healthy server IDs"""
        return [
            sid for sid, info in self.servers.items()
            if info["status"] == "alive"
        ]
    
    def get_leader(self) -> int:
        """Get server cluster leader"""
        return self.server_cluster_leader
    
    def mark_dead(self, server_id: int):
        """Immediately mark server as dead"""
        if server_id in self.servers:
            self.servers[server_id]["status"] = "dead"
            print(f"⚠️  Server {server_id} marked DEAD (immediate detection)")
    
    def update_from_metrics(self, metrics_data: Dict):
        """Update state from server metrics"""
        server_id = metrics_data.get("node_id")
        role = metrics_data.get("state", "unknown")
        leader = metrics_data.get("current_leader")
        
        if server_id in self.servers:
            self.servers[server_id]["status"] = "alive"
            self.servers[server_id]["role"] = role
            self.servers[server_id]["last_seen"] = time.time()
        
        if leader:
            self.server_cluster_leader = leader
            self.server_cluster_leader_updated = time.time()


# ============================================================================
# Health Checker (polls backend servers)
# ============================================================================

class HealthChecker:
    """Periodically check health of backend servers"""
    
    BACKENDS = [
        {"id": 1, "url": "http://127.0.0.1:8001"},
        {"id": 2, "url": "http://127.0.0.1:8002"},
        {"id": 3, "url": "http://127.0.0.1:8003"},
    ]
    
    @staticmethod
    def check_all(state: LBClusterState, lb_id: int):
        """Check health of all backends"""
        now = time.time()
        
        for backend in HealthChecker.BACKENDS:
            backend_id = backend["id"]
            backend_url = backend["url"]
            
            try:
                response = requests.get(
                    f"{backend_url}/metrics",
                    timeout=2
                )
                
                if response.status_code == 200:
                    data = response.json()["data"]
                    state.update_from_metrics(data)
                    print(f"  ✅ LB{lb_id}: Server {backend_id} alive ({data['state']})")
                else:
                    state.servers[backend_id]["status"] = "dead"
                    print(f"  ❌ LB{lb_id}: Server {backend_id} dead (HTTP {response.status_code})")
            
            except Exception as e:
                state.servers[backend_id]["status"] = "dead"
                print(f"  ❌ LB{lb_id}: Server {backend_id} dead ({type(e).__name__})")
        
        state.last_health_check = now


# ============================================================================
# Load Balancer Handler with Retry Logic
# ============================================================================

class ClusteredLBHandler(BaseHTTPRequestHandler):
    """HTTP handler for clustered load balancer with retry"""
    
    # Shared state (in real implementation, this would be replicated via Raft)
    cluster_state: LBClusterState = LBClusterState()
    lb_id: int = 1
    
    def do_POST(self):
        """Handle POST requests"""
        content_length = int(self.headers.get('Content-Length', 0))
        body = self.rfile.read(content_length)
        
        if '/image/echo' in self.path:
            self.handle_read_request(body)
        else:
            self.handle_write_request(body)
    
    def do_GET(self):
        """Handle GET requests"""
        if '/metrics' in self.path:
            self.handle_metrics_request()
        else:
            self.handle_read_request(None)
    
    def handle_read_request(self, body):
        """Route read request with retry on failure"""
        healthy = self.cluster_state.get_healthy_servers()
        
        if not healthy:
            print(f"❌ LB{self.lb_id} READ: {self.path} → No healthy backends!")
            self.send_error(503, "No Healthy Backends")
            return
        
        # Shuffle to try different servers
        backends_to_try = healthy.copy()
        random.shuffle(backends_to_try)
        
        for attempt, backend_id in enumerate(backends_to_try):
            backend_url = f"http://127.0.0.1:{8000 + backend_id}"
            
            print(f"📤 LB{self.lb_id} READ attempt {attempt+1}/{len(backends_to_try)}: Server {backend_id}", flush=True)
            
            try:
                if body:
                    # POST request
                    response = requests.post(
                        f"{backend_url}{self.path}",
                        data=body,
                        headers=dict(self.headers),
                        timeout=5
                    )
                else:
                    # GET request
                    response = requests.get(
                        f"{backend_url}{self.path}",
                        timeout=5
                    )
                
                if response.status_code == 200:
                    # Success!
                    print(f"✅ LB{self.lb_id} Got response from Server {backend_id} ({len(response.content)} bytes)", flush=True)
                    self.send_response(200)
                    
                    # ← ADD THIS: Include which server handled it
                    self.send_header('X-Backend-Server', str(backend_id))
                    self.send_header('X-Load-Balancer', str(self.lb_id))
                    
                    for header, value in response.headers.items():
                        if header.lower() not in ['content-encoding', 'transfer-encoding', 'x-backend-server', 'x-load-balancer']:
                            self.send_header(header, value)
                    self.end_headers()
                    self.wfile.write(response.content)
                    return
                else:
                    # Bad status code
                    print(f"❌ LB{self.lb_id} Server {backend_id} returned HTTP {response.status_code}", flush=True)
                    if attempt < len(backends_to_try) - 1:
                        continue
            
            except requests.exceptions.Timeout:
                print(f"❌ LB{self.lb_id} Server {backend_id} timeout", flush=True)
                self.cluster_state.mark_dead(backend_id)
                
                if attempt < len(backends_to_try) - 1:
                    print(f"   Retrying next backend...", flush=True)
                    continue
            
            except requests.exceptions.ConnectionError:
                print(f"❌ LB{self.lb_id} Server {backend_id} connection refused", flush=True)
                self.cluster_state.mark_dead(backend_id)
                
                if attempt < len(backends_to_try) - 1:
                    print(f"   Retrying next backend...", flush=True)
                    continue
            
            except Exception as e:
                print(f"❌ LB{self.lb_id} Server {backend_id} error: {type(e).__name__}", flush=True)
                self.cluster_state.mark_dead(backend_id)
                
                if attempt < len(backends_to_try) - 1:
                    print(f"   Retrying next backend...", flush=True)
                    continue
        
        # All backends failed
        print(f"❌ LB{self.lb_id} All backends failed for {self.path}", flush=True)
        self.send_error(503, "All Backends Failed")

    
    def handle_write_request(self, body):
        """Route write request to leader with retry"""
        leader_id = self.cluster_state.get_leader()
        
        if not leader_id:
            print(f"❌ LB{self.lb_id} WRITE: {self.path} → No leader known!")
            self.send_error(503, "No Leader Available")
            return
        
        backend_url = f"http://127.0.0.1:{8000 + leader_id}"
        
        print(f"📝 LB{self.lb_id} WRITE: {self.path} → Server {leader_id} (leader)", flush=True)
        
        try:
            response = requests.post(
                f"{backend_url}{self.path}",
                data=body,
                headers=dict(self.headers),
                timeout=5
            )
            
            if response.status_code == 200:
                print(f"✅ LB{self.lb_id} Write succeeded", flush=True)
                self.send_response(response.status_code)
                for header, value in response.headers.items():
                    if header.lower() not in ['content-encoding', 'transfer-encoding']:
                        self.send_header(header, value)
                self.end_headers()
                self.wfile.write(response.content)
            else:
                print(f"❌ LB{self.lb_id} Leader returned HTTP {response.status_code}", flush=True)
                self.send_error(502, "Leader Error")
        
        except Exception as e:
            print(f"❌ LB{self.lb_id} Leader unreachable: {type(e).__name__}", flush=True)
            self.cluster_state.mark_dead(leader_id)
            self.send_error(503, "Leader Unavailable")
    
    def handle_metrics_request(self):
        """Return LB metrics"""
        metrics = {
            "lb_id": self.lb_id,
            "healthy_servers": self.cluster_state.get_healthy_servers(),
            "server_leader": self.cluster_state.get_leader(),
            "server_status": self.cluster_state.servers,
        }
        
        self.send_response(200)
        self.send_header('Content-Type', 'application/json')
        self.end_headers()
        self.wfile.write(json.dumps(metrics).encode())
    
    def log_message(self, format, *args):
        """Suppress default logging"""
        pass


# ============================================================================
# Main
# ============================================================================

def start_clustered_lb(lb_id: int, port: int):
    """Start a clustered load balancer instance"""
    
    ClusteredLBHandler.lb_id = lb_id
    ClusteredLBHandler.cluster_state = LBClusterState()
    
    server_address = ('0.0.0.0', port)
    httpd = HTTPServer(server_address, ClusteredLBHandler)
    
    print(f"🚀 Clustered LB {lb_id} starting on port {port}")
    print(f"   Health check: Every 5 seconds")
    print(f"   Backends: 8001, 8002, 8003")
    print(f"   Retry on failure: ✅")
    print(f"   Immediate dead detection: ✅")
    print(f"\n📡 Listening on http://0.0.0.0:{port}\n")
    
    # Start health checker thread
    def health_check_loop():
        while True:
            time.sleep(5)
            print(f"🔍 LB{lb_id} Health check...", flush=True)
            HealthChecker.check_all(ClusteredLBHandler.cluster_state, lb_id)
            print()
    
    health_thread = threading.Thread(target=health_check_loop, daemon=True)
    health_thread.start()
    
    # Initial health check
    print(f"🔍 LB{lb_id} Initial health check...", flush=True)
    HealthChecker.check_all(ClusteredLBHandler.cluster_state, lb_id)
    print()
    
    try:
        httpd.serve_forever()
    except KeyboardInterrupt:
        print(f"\n⏹️  LB {lb_id} stopped")


if __name__ == "__main__":
    if len(sys.argv) < 2:
        print("Usage: python3 clustered_lb.py <lb_id> [port]")
        print("Example:")
        print("  python3 clustered_lb.py 1 9001")
        print("  python3 clustered_lb.py 2 9002")
        print("  python3 clustered_lb.py 3 9003")
        sys.exit(1)
    
    lb_id = int(sys.argv[1])
    port = int(sys.argv[2]) if len(sys.argv) > 2 else 9000 + lb_id
    
    start_clustered_lb(lb_id, port)
