#!/bin/bash
set -e

# Remote distributed setup script
# Uses the three nodes across different devices
# Node 1: 10.40.49.211:8001
# Node 2: 10.40.41.162:8002
# Node 3: 10.40.39.217:8003

NODE1_IP="10.40.49.211"
NODE1_PORT="8001"
NODE2_IP="10.40.41.162"
NODE2_PORT="8002"
NODE3_IP="10.40.39.217"
NODE3_PORT="8003"

echo "🌐 Remote Distributed Cluster Setup"
echo "===================================="
echo ""
echo "📍 Nodes:"
echo "  Node 1: $NODE1_IP:$NODE1_PORT"
echo "  Node 2: $NODE2_IP:$NODE2_PORT"
echo "  Node 3: $NODE3_IP:$NODE3_PORT"
echo ""

echo "⏳ Waiting for servers to start..."
sleep 3

echo "🎬 Initializing cluster (direct to Node 1 at $NODE1_IP:$NODE1_PORT)..."
curl -s -X POST http://$NODE1_IP:$NODE1_PORT/cluster/init \
  -H 'Content-Type: application/json' -d '{}' > /dev/null
echo "✅ Initialized"

echo "📥 Adding nodes as learners..."
curl -s -X POST http://$NODE1_IP:$NODE1_PORT/cluster/add-learner \
  -H 'Content-Type: application/json' \
  -d '{"node_id": 2, "address": "'$NODE2_IP':7002"}' > /dev/null

sleep 1

curl -s -X POST http://$NODE1_IP:$NODE1_PORT/cluster/add-learner \
  -H 'Content-Type: application/json' \
  -d '{"node_id": 3, "address": "'$NODE3_IP':7003"}' > /dev/null

echo "✅ Learners added"

sleep 3

echo "🔄 Promoting to voters (attempt 1)..."
RESULT=$(curl -s -X POST http://$NODE1_IP:$NODE1_PORT/cluster/change-membership \
  -H 'Content-Type: application/json' \
  -d '{"members": [1, 2, 3]}')

if echo "$RESULT" | grep -q "undergoing"; then
    echo "⏳ Config change in progress, waiting..."
    sleep 5
    echo "🔄 Promoting to voters (attempt 2)..."
    curl -s -X POST http://$NODE1_IP:$NODE1_PORT/cluster/change-membership \
      -H 'Content-Type: application/json' \
      -d '{"members": [1, 2, 3]}' > /dev/null
fi

echo "✅ Cluster ready"

sleep 3

# Verify
echo ""
echo "📊 Cluster Status:"
echo -n "  Node 1 ($NODE1_IP:$NODE1_PORT): "
curl -s http://$NODE1_IP:$NODE1_PORT/metrics | jq -r .data.state 2>/dev/null || echo "❌ Unreachable"
echo -n "  Node 2 ($NODE2_IP:$NODE2_PORT): "
curl -s http://$NODE2_IP:$NODE2_PORT/metrics | jq -r .data.state 2>/dev/null || echo "❌ Unreachable"
echo -n "  Node 3 ($NODE3_IP:$NODE3_PORT): "
curl -s http://$NODE3_IP:$NODE3_PORT/metrics | jq -r .data.state 2>/dev/null || echo "❌ Unreachable"

echo ""
echo "✅ Setup Complete!"
echo ""
echo "🚀 Next steps:"
echo "  1. Make sure all three nodes are running with the correct startup commands"
echo "  2. Access the UI at: http://<your-ui-host>:5173"
echo "  3. Test connectivity:"
echo "     curl http://$NODE1_IP:$NODE1_PORT/metrics"
echo "     curl http://$NODE2_IP:$NODE2_PORT/metrics"
echo "     curl http://$NODE3_IP:$NODE3_PORT/metrics"
echo ""
