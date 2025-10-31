#!/bin/bash
set -e

NODE_ID=$1
if [ -z "$NODE_ID" ]; then
    echo "Usage: ./rejoin_node.sh <node_id>"
    exit 1
fi

echo "🔄 Rejoining Node $NODE_ID..."

# Find a healthy leader
LEADER=None
for port in 8001 8002 8003; do
    if curl -s http://127.0.0.1:$port/metrics | grep -q "Leader"; then
        LEADER_PORT=$port
        break
    fi
done

if [ -z "$LEADER_PORT" ]; then
    echo "❌ No leader found!"
    exit 1
fi

echo "✅ Found leader at port $LEADER_PORT"

# Add as learner
echo "📥 Adding as learner..."
curl -s -X POST http://127.0.0.1:$LEADER_PORT/cluster/add-learner \
  -H 'Content-Type: application/json' \
  -d "{\"node_id\": $NODE_ID, \"address\": \"127.0.0.1:$((7000 + NODE_ID))\"}" | jq .

sleep 2

# Promote to voter
echo "🔄 Promoting to voter..."
curl -s -X POST http://127.0.0.1:$LEADER_PORT/cluster/change-membership \
  -H 'Content-Type: application/json' \
  -d '{"members": [1, 2, 3]}' | jq .

echo "✅ Node $NODE_ID rejoined!"
