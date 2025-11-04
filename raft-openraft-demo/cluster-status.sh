#!/bin/bash
set -e

# Local cluster status checker
# Usage: CLUSTER_HOST=127.0.0.1 ./cluster-status.sh

HOST="${CLUSTER_HOST:-127.0.0.1}"

echo ""
echo "📊 Cluster Status (host: $HOST):"
echo -n "  Node 1: "
curl -s http://$HOST:8001/metrics | jq -r .data.state
echo -n "  Node 2: "
curl -s http://$HOST:8002/metrics | jq -r .data.state
echo -n "  Node 3: "
curl -s http://$HOST:8003/metrics | jq -r .data.state