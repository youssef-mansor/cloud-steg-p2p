#!/bin/bash
# Kill any processes using the Raft server ports

echo "🔍 Checking for processes on ports 8001-8003 and 7001-7003..."

for port in 8001 8002 8003 7001 7002 7003; do
    pid=$(lsof -ti :$port 2>/dev/null)
    if [ -n "$pid" ]; then
        echo "🛑 Killing PID $pid on port $port"
        kill -9 $pid 2>/dev/null
    fi
done

sleep 1

# Verify ports are clear
echo ""
echo "📊 Port status:"
for port in 8001 8002 8003 7001 7002 7003; do
    pid=$(lsof -ti :$port 2>/dev/null)
    if [ -n "$pid" ]; then
        echo "  ⚠️  Port $port: Still in use (PID: $pid)"
    else
        echo "  ✅ Port $port: Free"
    fi
done

