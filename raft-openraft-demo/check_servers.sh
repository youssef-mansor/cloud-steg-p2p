#!/bin/bash
# Check which servers are running and show their PIDs

echo "🔍 Checking server status..."
echo ""

for port in 8001 8002 8003; do
    pid=$(lsof -ti :$port 2>/dev/null)
    if [ -n "$pid" ]; then
        node_id=$((port - 8000))
        echo "✅ Node $node_id (port $port): Running (PID: $pid)"
        # Try to show command
        ps -p $pid -o command= 2>/dev/null | head -1
    else
        node_id=$((port - 8000))
        echo "❌ Node $node_id (port $port): Not running"
    fi
    echo ""
done

echo "📋 Server logs are in: /tmp/load_balance_test_*/node_*.log"
