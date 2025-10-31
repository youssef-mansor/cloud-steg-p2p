#!/usr/bin/env python3
"""
Simple Python Load Balancer
- Listens on port 9000
- Distributes requests to 3 backend servers
- For reads (echo): random server
- For writes: leader only (future)
"""

import random
import requests
import threading
import time
from http.server import HTTPServer, BaseHTTPRequestHandler
import io
import sys

class LoadBalancerConfig:
    """Configuration for load balancer"""
    BACKENDS = [
        "http://127.0.0.1:8001",
        "http://127.0.0.1:8002",
        "http://127.0.0.1:8003",
    ]
    
    # Track leader for write operations
    leader = None
    leader_last_update = 0
    
    @classmethod
    def find_leader(cls):
        """Query backends to find current leader"""
        for backend in cls.BACKENDS:
            try:
                response = requests.get(f"{backend}/metrics", timeout=2)
                if response.status_code == 200:
                    data = response.json()['data']
                    leader_id = data.get('current_leader')
                    if leader_id:
                        # Map leader ID to backend
                        leader_port = 8000 + leader_id
                        cls.leader = f"http://127.0.0.1:{leader_port}"
                        cls.leader_last_update = time.time()
                        print(f"✅ Found leader: {cls.leader}", flush=True)
                        return cls.leader
            except Exception as e:
                continue
        
        print("❌ Could not find leader", flush=True)
        return None
    
    @classmethod
    def get_leader(cls):
        """Get leader, refresh if stale (> 5 seconds)"""
        if cls.leader is None or (time.time() - cls.leader_last_update) > 5:
            cls.find_leader()
        return cls.leader


class LoadBalancerHandler(BaseHTTPRequestHandler):
    """HTTP request handler for load balancing"""
    
    def do_POST(self):
        """Handle POST requests"""
        content_length = int(self.headers.get('Content-Length', 0))
        body = self.rfile.read(content_length)
        
        # Determine if read or write based on endpoint
        if '/image/echo' in self.path:
            # Read operation - random server
            self.handle_read_request(body)
        else:
            # Write operation - leader only (future)
            self.handle_write_request(body)
    
    def do_GET(self):
        """Handle GET requests"""
        # GET requests (like /metrics) go to random server
        self.handle_read_request(None)
    
    def handle_read_request(self, body):
        """Route read request to random backend"""
        backend = random.choice(LoadBalancerConfig.BACKENDS)
        
        print(f"📤 READ: {self.path} → {backend}", flush=True)
        
        try:
            if body:
                # POST request
                response = requests.post(
                    f"{backend}{self.path}",
                    data=body,
                    headers=dict(self.headers),
                    timeout=30
                )
            else:
                # GET request
                response = requests.get(
                    f"{backend}{self.path}",
                    timeout=30
                )
            
            # Send response back to client
            self.send_response(response.status_code)
            
            # Forward response headers
            for header, value in response.headers.items():
                if header.lower() not in ['content-encoding', 'transfer-encoding']:
                    self.send_header(header, value)
            self.end_headers()
            
            # Send response body
            self.wfile.write(response.content)
            print(f"✅ Response sent: {response.status_code} ({len(response.content)} bytes)", flush=True)
            
        except Exception as e:
            print(f"❌ Error: {e}", flush=True)
            self.send_error(502, "Bad Gateway")
    
    def handle_write_request(self, body):
        """Route write request to leader only"""
        leader = LoadBalancerConfig.get_leader()
        
        if not leader:
            print(f"❌ WRITE: {self.path} → No leader available", flush=True)
            self.send_error(503, "Service Unavailable - No Leader")
            return
        
        print(f"📝 WRITE: {self.path} → {leader} (leader)", flush=True)
        
        try:
            response = requests.post(
                f"{leader}{self.path}",
                data=body,
                headers=dict(self.headers),
                timeout=30
            )
            
            # Send response back to client
            self.send_response(response.status_code)
            
            for header, value in response.headers.items():
                if header.lower() not in ['content-encoding', 'transfer-encoding']:
                    self.send_header(header, value)
            self.end_headers()
            
            self.wfile.write(response.content)
            print(f"✅ Response sent: {response.status_code}", flush=True)
            
        except Exception as e:
            print(f"❌ Error: {e}", flush=True)
            self.send_error(502, "Bad Gateway")
    
    def log_message(self, format, *args):
        """Suppress default logging"""
        pass


def start_load_balancer(port=9000):
    """Start the load balancer"""
    server_address = ('0.0.0.0', port)
    httpd = HTTPServer(server_address, LoadBalancerHandler)
    
    print(f"🚀 Load Balancer starting on port {port}", flush=True)
    print(f"   Backends: {', '.join(LoadBalancerConfig.BACKENDS)}", flush=True)
    print(f"   Read operations: random backend", flush=True)
    print(f"   Write operations: leader only", flush=True)
    print(f"\n📡 Listening on http://0.0.0.0:{port}", flush=True)
    
    # Find initial leader
    LoadBalancerConfig.find_leader()
    
    # Start leader detection thread
    def detect_leader():
        while True:
            time.sleep(10)
            LoadBalancerConfig.find_leader()
    
    leader_thread = threading.Thread(target=detect_leader, daemon=True)
    leader_thread.start()
    
    try:
        httpd.serve_forever()
    except KeyboardInterrupt:
        print("\n⏹️  Load Balancer stopped", flush=True)


if __name__ == "__main__":
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 9000
    start_load_balancer(port)
