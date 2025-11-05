#!/bin/bash
set -e

# Distributed cluster status checker
# Usage (override optional): CLUSTER_HOST_1=... CLUSTER_HOST_2=... CLUSTER_HOST_3=... ./cluster-status.sh

HOST1="${CLUSTER_HOST_1:-10.40.56.135}"
HOST2="${CLUSTER_HOST_2:-10.40.42.221}"
HOST3="${CLUSTER_HOST_3:-10.40.44.75}"

echo ""
echo "📊 Cluster Status:"
echo -n "  Node 1 ($HOST1): "
curl -s http://$HOST1:8001/metrics | jq -r .data.state
echo -n "  Node 2 ($HOST2): "
curl -s http://$HOST2:8002/metrics | jq -r .data.state
echo -n "  Node 3 ($HOST3): "
curl -s http://$HOST3:8003/metrics | jq -r .data.state