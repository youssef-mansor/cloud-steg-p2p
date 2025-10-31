#!/bin/bash
set -e

echo "🧪 Testing Load Balancing for Read Requests"
echo "=============================================="
echo ""

# Check if input image exists
if [ ! -f "input-image.png" ]; then
    echo "❌ Error: input-image.png not found"
    echo "   Create a test image or use an existing one"
    exit 1
fi

echo "📋 Testing Scenarios:"
echo "  1. Send request to leader (should process or forward randomly)"
echo "  2. Send request to follower (should be rejected)"
echo "  3. Multiple requests to test random distribution"
echo ""

# Find which node is the leader
echo "🔍 Checking which node is the leader..."
LEADER_INFO=$(curl -s http://127.0.0.1:8001/metrics | jq -r '.data | {node_id, state, current_leader}')

if [ -z "$LEADER_INFO" ]; then
    echo "❌ Error: Could not connect to nodes. Make sure all 3 nodes are running!"
    exit 1
fi

LEADER_NODE=$(curl -s http://127.0.0.1:8001/metrics | jq -r '.data.current_leader // empty')
LEADER_PORT=$((8000 + LEADER_NODE))
LEADER_STATE=$(curl -s http://127.0.0.1:8001/metrics | jq -r '.data.state')

echo "   Node 1 state: $(curl -s http://127.0.0.1:8001/metrics | jq -r '.data.state')"
echo "   Node 2 state: $(curl -s http://127.0.0.1:8002/metrics | jq -r '.data.state')"
echo "   Node 3 state: $(curl -s http://127.0.0.1:8003/metrics | jq -r '.data.state')"
echo ""

# Test 1: Send to leader (should succeed)
echo "✅ Test 1: Sending echo request to leader (port $LEADER_PORT)..."
RESPONSE=$(curl -s -o /tmp/test-echo-leader.png -w "%{http_code}" \
    -X POST "http://127.0.0.1:$LEADER_PORT/image/echo" \
    --data-binary @input-image.png)

if [ "$RESPONSE" = "200" ]; then
    echo "   ✓ Success! HTTP $RESPONSE"
    if [ -f /tmp/test-echo-leader.png ]; then
        SIZE=$(stat -f%z /tmp/test-echo-leader.png 2>/dev/null || stat -c%s /tmp/test-echo-leader.png 2>/dev/null || echo "unknown")
        echo "   ✓ Received response: $SIZE bytes"
    fi
else
    echo "   ✗ Failed! HTTP $RESPONSE"
fi
echo ""

# Test 2: Send to follower (should be rejected)
FOLLOWER_PORT=8002
if [ "$LEADER_PORT" = "8002" ]; then
    FOLLOWER_PORT=8003
fi

echo "✅ Test 2: Sending echo request to follower (port $FOLLOWER_PORT)..."
RESPONSE=$(curl -s -o /tmp/test-echo-follower.txt -w "%{http_code}" \
    -X POST "http://127.0.0.1:$FOLLOWER_PORT/image/echo" \
    --data-binary @input-image.png)

if [ "$RESPONSE" = "503" ]; then
    echo "   ✓ Correctly rejected! HTTP $RESPONSE (Service Unavailable)"
    ERROR_MSG=$(cat /tmp/test-echo-follower.txt)
    echo "   ✓ Error message: $ERROR_MSG"
else
    echo "   ✗ Unexpected response! HTTP $RESPONSE (expected 503)"
fi
echo ""

# Test 3: Multiple requests to test random distribution
echo "✅ Test 3: Sending 5 requests to leader to test random distribution..."
for i in {1..5}; do
    echo -n "   Request $i: "
    RESPONSE=$(curl -s -o /tmp/test-echo-$i.png -w "%{http_code}" \
        -X POST "http://127.0.0.1:$LEADER_PORT/image/echo" \
        --data-binary @input-image.png)
    if [ "$RESPONSE" = "200" ]; then
        echo "✓ Success (HTTP $RESPONSE)"
    else
        echo "✗ Failed (HTTP $RESPONSE)"
    fi
done
echo ""
echo "   Check server logs to see which nodes processed each request!"
echo ""

# Test 4: Steganography endpoint
echo "✅ Test 4: Testing steganography endpoint (leader)..."
RESPONSE=$(curl -s -o /tmp/test-steg-leader.png -w "%{http_code}" \
    -X POST "http://127.0.0.1:$LEADER_PORT/image/steg" \
    --data-binary @input-image.png)

if [ "$RESPONSE" = "200" ]; then
    echo "   ✓ Success! HTTP $RESPONSE"
    if [ -f /tmp/test-steg-leader.png ]; then
        SIZE=$(stat -f%z /tmp/test-steg-leader.png 2>/dev/null || stat -c%s /tmp/test-steg-leader.png 2>/dev/null || echo "unknown")
        echo "   ✓ Received stego image: $SIZE bytes"
    fi
else
    echo "   ✗ Failed! HTTP $RESPONSE"
fi
echo ""

echo "📊 Summary:"
echo "  - Check the server terminal logs to see:"
echo "    • Which requests were dropped by followers"
echo "    • Which requests were forwarded by the leader"
echo "    • Random distribution across nodes"
echo ""
echo "✨ Load balancing test complete!"

