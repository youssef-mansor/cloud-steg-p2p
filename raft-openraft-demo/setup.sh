#!/bin/bash
set -e

echo "⏳ Waiting for servers to start..."
sleep 3

echo "🎬 Initializing cluster (direct to Node 1)..."
curl -s -X POST http://10.40.56.135:8001/cluster/init \
  -H 'Content-Type: application/json' -d '{}' > /dev/null
echo "✅ Initialized"

echo "📥 Adding nodes as learners..."
curl -s -X POST http://10.40.56.135:8001/cluster/add-learner \
  -H 'Content-Type: application/json' \
  -d '{"node_id": 2, "address": "10.40.39.217:7002"}' > /dev/null

sleep 1

curl -s -X POST http://10.40.56.135:8001/cluster/add-learner \
  -H 'Content-Type: application/json' \
  -d '{"node_id": 3, "address": "10.40.44.75:7003"}' > /dev/null

echo "✅ Learners added"

sleep 3

echo "🔄 Promoting to voters (attempt 1)..."
RESULT=$(curl -s -X POST http://10.40.56.135:8001/cluster/change-membership \
  -H 'Content-Type: application/json' \
  -d '{"members": [1, 2, 3]}')

if echo "$RESULT" | grep -q "undergoing"; then
    echo "⏳ Config change in progress, waiting..."
    sleep 5
    echo "🔄 Promoting to voters (attempt 2)..."
    curl -s -X POST http://10.40.56.135:8001/cluster/change-membership \
      -H 'Content-Type: application/json' \
      -d '{"members": [1, 2, 3]}' > /dev/null
fi

echo "✅ Cluster ready"

sleep 3

# Verify
echo ""
echo "📊 Cluster Status:"
echo -n "  Node 1: "
curl -s http://10.40.56.135:8001/metrics | jq -r .data.state
echo -n "  Node 2: "
curl -s http://10.40.39.217:8002/metrics | jq -r .data.state
echo -n "  Node 3: "
curl -s http://10.40.44.75:8003/metrics | jq -r .data.state

echo ""
echo "✅ Leader found, now testing through load balancer..."
echo ""