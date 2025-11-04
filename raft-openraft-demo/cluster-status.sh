# Verify
echo ""
echo "📊 Cluster Status:"
echo -n "  Node 1: "
curl -s http://10.40.56.135:8001/metrics | jq -r .data.state
echo -n "  Node 2: "
curl -s http://10.40.39.217:8002/metrics | jq -r .data.state
echo -n "  Node 3: "
curl -s http://10.40.44.75:8003/metrics | jq -r .data.state