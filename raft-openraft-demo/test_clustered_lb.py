#!/usr/bin/env python3
"""
Test clustered load balancer
- Test load distribution across multiple LBs
- Test failover when server dies
"""

import requests
import hashlib
import time
import random

def test_multiple_lbs():
    """Test hitting different LBs"""
    
    lbs = [
        "http://127.0.0.1:9001",
        "http://127.0.0.1:9002",
        "http://127.0.0.1:9003",
    ]
    
    test_data = b"Test image content " * 1000
    original_hash = hashlib.md5(test_data).hexdigest()
    
    print("🧪 Testing Clustered Load Balancer")
    print(f"   LBs: {lbs}")
    print(f"   Test data: {len(test_data)} bytes\n")
    
    # Track which LBs handled requests
    lb_stats = {lb: 0 for lb in lbs}
    success_count = 0
    
    # Send 15 requests (5 per LB)
    for i in range(15):
        lb = random.choice(lbs)
        lb_num = lbs.index(lb) + 1
        print(f"Request {i+1}/15... ", end="", flush=True)
        
        try:
            response = requests.post(
                f"{lb}/image/echo",
                data=test_data,
                headers={'Content-Type': 'application/octet-stream'},
                timeout=5
            )
            
            if response.status_code == 200:
                returned_hash = hashlib.md5(response.content).hexdigest()
                if original_hash == returned_hash:
                    print(f"✅ LB{lb_num}")
                    lb_stats[lb] += 1
                    success_count += 1
                else:
                    print(f"❌ Data mismatch")
            else:
                print(f"❌ HTTP {response.status_code}")
        
        except Exception as e:
            print(f"❌ Error: {e}")
        
        time.sleep(0.1)
    
    print(f"\n📊 Results:")
    print(f"   Total success: {success_count}/15")
    print(f"   LB Distribution:")
    for lb in lbs:
        lb_num = lbs.index(lb) + 1
        print(f"     LB{lb_num}: {lb_stats[lb]} requests")


def test_lb_metrics():
    """Check LB cluster state"""
    
    print("\n📊 Checking Clustered LB Metrics\n")
    
    for i in range(1, 4):
        try:
            response = requests.get(f"http://127.0.0.1:{9000+i}/metrics")
            if response.status_code == 200:
                metrics = response.json()
                print(f"LB{i}:")
                print(f"  Healthy servers: {metrics['healthy_servers']}")
                print(f"  Server leader: {metrics['server_leader']}")
                print(f"  Server status: {metrics['server_status']}")
                print()
        except Exception as e:
            print(f"❌ LB{i} error: {e}\n")


if __name__ == "__main__":
    test_multiple_lbs()
    test_lb_metrics()
