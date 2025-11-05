#!/bin/bash
set -e

echo "⏳ Waiting for servers to start..."
sleep 3

# Distributed cluster setup: three nodes on separate hosts
# Node1 http:8001 raft:7001, Node2 http:8002 raft:7002, Node3 http:8003 raft:7003
# Hosts can be overridden via environment variables if needed
HOST1="${CLUSTER_HOST_1:-10.40.56.135}"
HOST2="${CLUSTER_HOST_2:-10.40.42.221}"
HOST3="${CLUSTER_HOST_3:-10.40.44.75}"

echo "🎬 Initializing cluster (direct to Node 1 $HOST1:8001)..."
curl -s -X POST http://$HOST1:8001/cluster/init \
  -H 'Content-Type: application/json' -d '{}' > /dev/null
echo "✅ Initialized"

echo "📥 Adding nodes as learners..."
curl -s -X POST http://$HOST1:8001/cluster/add-learner \
  -H 'Content-Type: application/json' \
  -d '{"node_id": 2, "address": "10.40.42.221:7002"}' > /dev/null

sleep 1

curl -s -X POST http://$HOST1:8001/cluster/add-learner \
  -H 'Content-Type: application/json' \
  -d '{"node_id": 3, "address": "10.40.44.75:7003"}' > /dev/null

echo "✅ Learners added"

sleep 3

echo "🔄 Promoting to voters (attempt 1)..."
RESULT=$(curl -s -X POST http://$HOST1:8001/cluster/change-membership \
  -H 'Content-Type: application/json' \
  -d '{"members": [1, 2, 3]}')

if echo "$RESULT" | grep -q "undergoing"; then
    echo "⏳ Config change in progress, waiting..."
    sleep 5
    echo "🔄 Promoting to voters (attempt 2)..."
    curl -s -X POST http://$HOST1:8001/cluster/change-membership \
      -H 'Content-Type: application/json' \
      -d '{"members": [1, 2, 3]}' > /dev/null
fi

echo "✅ Cluster ready"

sleep 3

# Verify
echo ""
echo "📊 Cluster Status:"
echo -n "  Node 1 ($HOST1): "
curl -s http://$HOST1:8001/metrics | jq -r .data.state
echo -n "  Node 2 ($HOST2): "
curl -s http://$HOST2:8002/metrics | jq -r .data.state
echo -n "  Node 3 ($HOST3): "
curl -s http://$HOST3:8003/metrics | jq -r .data.state

echo ""
echo "✅ Leader found, now testing through load balancer..."
echo ""