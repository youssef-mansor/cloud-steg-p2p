#!/usr/bin/env python3
"""
Simple test for /image/echo endpoint - sends raw image binary data
"""
import requests
import time

# Read the image
with open('input-image.png', 'rb') as f:
    image_data = f.read()

print(f"📷 Loaded image: {len(image_data)} bytes")
print(f"🚀 Sending to /image/echo endpoint...")

# Test all three nodes
for node_id in [1, 2, 3]:
    port = 8000 + node_id
    url = f"http://127.0.0.1:{port}/image/echo"
    
    try:
        start = time.time()
        response = requests.post(
            url,
            data=image_data,
            headers={'Content-Type': 'application/octet-stream'},
            timeout=5
        )
        elapsed = time.time() - start
        
        processed_by = response.headers.get('X-Processed-By-Node', 'unknown')
        
        if response.status_code == 200:
            print(f"✅ Node {node_id}: SUCCESS ({response.status_code}) - processed by node {processed_by} in {elapsed:.3f}s")
        else:
            print(f"❌ Node {node_id}: FAILED ({response.status_code}) - {response.text[:100]}")
    except Exception as e:
        print(f"❌ Node {node_id}: ERROR - {type(e).__name__}: {e}")

print(f"\n✅ Test complete!")
