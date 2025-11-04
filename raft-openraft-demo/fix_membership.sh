#!/bin/bash
# Script to fix membership when only one node remains
# Usage: ./fix_membership.sh <node_id> [host]
# Example: ./fix_membership.sh 2

NODE_ID=${1:-2}
HOST=${2:-127.0.0.1}
PORT=$((8000 + NODE_ID))

echo "🔄 Attempting to fix membership for node $NODE_ID on $HOST:$PORT"
echo "   Setting membership to [$NODE_ID]..."

RESPONSE=$(curl -s -X POST "http://$HOST:$PORT/cluster/change-membership" \
  -H "Content-Type: application/json" \
  -d "{\"members\": [$NODE_ID]}")

echo "Response: $RESPONSE"

# Check if it worked
sleep 2
METRICS=$(curl -s "http://$HOST:$PORT/metrics")
echo ""
echo "Current state:"
echo "$METRICS" | grep -o '"state":"[^"]*"' | head -1

if echo "$METRICS" | grep -q '"state":"Leader"'; then
    echo "✅ Node $NODE_ID is now the leader!"
else
    echo "⚠️  Node $NODE_ID is not leader yet. It may need more time or the cluster may need manual intervention."
    echo ""
    echo "If it's still stuck, try:"
    echo "  1. Restart the node"
    echo "  2. Or wait - the degraded mode will handle requests even if not leader"
fi

