#!/bin/bash
set -e

# Local cluster setup: assumes three nodes running on localhost
# Node1 http:8001 raft:7001, Node2 http:8002 raft:7002, Node3 http:8003 raft:7003
HOST="${CLUSTER_HOST:-127.0.0.1}"

echo "⏳ Waiting for servers to start..."
sleep 3

echo "🎬 Initializing cluster (direct to Node 1)..."
curl -s -X POST http://$HOST:8001/cluster/init \
  -H 'Content-Type: application/json' -d '{}' > /dev/null
echo "✅ Initialized"

echo "📥 Adding nodes as learners..."
curl -s -X POST http://$HOST:8001/cluster/add-learner \
  -H 'Content-Type: application/json' \
  -d '{"node_id": 2, "address": "127.0.0.1:7002"}' > /dev/null

sleep 1

curl -s -X POST http://$HOST:8001/cluster/add-learner \
  -H 'Content-Type: application/json' \
  -d '{"node_id": 3, "address": "127.0.0.1:7003"}' > /dev/null

echo "✅ Learners added"

sleep 3

echo "🔄 Promoting to voters (attempt 1)..."
RESULT=$(curl -s -X POST http://$HOST:8001/cluster/change-membership \
  -H 'Content-Type: application/json' \
  -d '{"members": [1, 2, 3]}')

if echo "$RESULT" | grep -q "undergoing"; then
    echo "⏳ Config change in progress, waiting..."
    sleep 5
    echo "🔄 Promoting to voters (attempt 2)..."
    curl -s -X POST http://$HOST:8001/cluster/change-membership \
      -H 'Content-Type: application/json' \
      -d '{"members": [1, 2, 3]}' > /dev/null
fi

echo "✅ Cluster ready"

sleep 2

# Verify
echo ""
echo "📊 Cluster Status (host: $HOST):"
echo -n "  Node 1: "
curl -s http://$HOST:8001/metrics | jq -r .data.state
echo -n "  Node 2: "
curl -s http://$HOST:8002/metrics | jq -r .data.state
echo -n "  Node 3: "
curl -s http://$HOST:8003/metrics | jq -r .data.state

echo ""
echo "✅ Local cluster configured"
echo ""


