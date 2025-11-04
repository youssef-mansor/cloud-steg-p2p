import { useState, useEffect } from 'react';
import { Activity, Server, Crown, AlertCircle, RefreshCw } from 'lucide-react';
import { LineChart, Line, XAxis, YAxis, CartesianGrid, Tooltip, Legend, ResponsiveContainer } from 'recharts';
import { useClusterMetrics } from '../hooks/useClusterMetrics';
import { apiClient } from '../api/client';
import type { TimeSeriesDataPoint } from '../types';

export function ClusterMonitoringTab() {
  const { nodeStatuses, lastUpdate, isPolling, setIsPolling, refresh } = useClusterMetrics(2000);
  const [timeSeriesData, setTimeSeriesData] = useState<TimeSeriesDataPoint[]>([]);
  const [lastRequestCounts, setLastRequestCounts] = useState<{ node1: number; node2: number; node3: number }>({ node1: 0, node2: 0, node3: 0 });
  const [lastTimestamp, setLastTimestamp] = useState<number>(Date.now());
  const [isInitialized, setIsInitialized] = useState<boolean>(false);

  // Fetch throughput data from the leader node every second (independent of metrics polling)
  useEffect(() => {
    if (!isPolling) return;

    // Find the current leader
    const currentLeader = nodeStatuses.find((n) => n.isLeader);
    if (!currentLeader) return; // Wait until leader is elected
    
    const leaderNodeId = currentLeader.nodeId;
    
    const fetchThroughput = async () => {
      try {
        // Fetch throughput data from the leader (it tracks all nodes)
        const response = await apiClient.getThroughput(leaderNodeId);
        
        if (response.success && response.data) {
          const now = Date.now();
          const nodeThroughputs = response.data.node_throughputs || {};

          // Get current cumulative counts
          const currentCounts = {
            node1: nodeThroughputs.node_1 || 0,
            node2: nodeThroughputs.node_2 || 0,
            node3: nodeThroughputs.node_3 || 0,
          };

          // On first initialization, just record the baseline - don't calculate throughput yet
          if (!isInitialized) {
            setLastRequestCounts(currentCounts);
            setLastTimestamp(now);
            setIsInitialized(true);
            return;
          }

          // Calculate req/sec based on count difference and time elapsed
          const timeElapsed = (now - lastTimestamp) / 1000; // seconds
          
          // Only add data point if at least 1 second has elapsed
          if (timeElapsed >= 0.9) { // Allow small margin for timing variations
            const reqPerSec = {
              node1: timeElapsed > 0 ? (currentCounts.node1 - lastRequestCounts.node1) / timeElapsed : 0,
              node2: timeElapsed > 0 ? (currentCounts.node2 - lastRequestCounts.node2) / timeElapsed : 0,
              node3: timeElapsed > 0 ? (currentCounts.node3 - lastRequestCounts.node3) / timeElapsed : 0,
            };

            // Update state
            setLastRequestCounts(currentCounts);
            setLastTimestamp(now);

            // Add data point with calculated req/sec
            const dataPoint: TimeSeriesDataPoint = {
              timestamp: now,
              node1: Math.max(0, reqPerSec.node1), // Ensure non-negative
              node2: Math.max(0, reqPerSec.node2),
              node3: Math.max(0, reqPerSec.node3),
            };

            setTimeSeriesData((prev) => {
              const newData = [...prev];
              newData.push(dataPoint);

              // Keep only last 60 data points (1 minute of data at 1s intervals)
              return newData.slice(-60);
            });
          }
        }
      } catch (error) {
        // Silently handle errors during throughput fetch
      }
    };

    // Fetch once immediately when polling starts or leader changes
    fetchThroughput();
    
    // Set up the interval - runs every second
    const interval = setInterval(fetchThroughput, 1000);

    return () => clearInterval(interval);
  }, [isPolling, nodeStatuses.find((n) => n.isLeader)?.nodeId, lastRequestCounts, lastTimestamp, isInitialized]);

  // Format timestamp for graph - memoized to avoid recreation on every render
  const formatTime = (timestamp: number) => {
    const date = new Date(timestamp);
    return date.toLocaleTimeString('en-US', { hour12: false, minute: '2-digit', second: '2-digit' });
  };

  // Find leader - but don't re-run the entire component when this changes
  const leader = nodeStatuses.find((n) => n.isLeader);
  const currentTerm = leader?.term || (nodeStatuses.length > 0 ? Math.max(...nodeStatuses.map((n) => n.term), 0) : 0);

  return (
    <div className="space-y-6">
      {/* Cluster Overview */}
      <div className="bg-white rounded-lg shadow p-6">
        <div className="flex items-center justify-between mb-4">
          <h2 className="text-2xl font-bold flex items-center gap-2">
            <Activity size={28} />
            Cluster Status
          </h2>
          <div className="flex items-center gap-4">
            <label className="flex items-center gap-2">
              <input
                type="checkbox"
                checked={isPolling}
                onChange={(e) => setIsPolling(e.target.checked)}
                className="w-4 h-4"
              />
              <span className="text-sm">Auto-refresh</span>
            </label>
            <button
              onClick={refresh}
              className="flex items-center gap-2 px-3 py-1 bg-blue-500 text-white rounded hover:bg-blue-600"
            >
              <RefreshCw size={16} />
              Refresh
            </button>
          </div>
        </div>

        {lastUpdate && (
          <p className="text-sm text-gray-500 mb-4">Last updated: {lastUpdate.toLocaleTimeString()}</p>
        )}

        {/* Leader Info */}
        <div className="bg-gradient-to-r from-yellow-50 to-amber-50 border-l-4 border-yellow-500 p-4 mb-6">
          <div className="flex items-center gap-3">
            <Crown size={24} className="text-yellow-600" />
            <div>
              <h3 className="font-bold text-lg">
                {leader ? `Node ${leader.nodeId} is Leader` : 'No Leader Elected'}
              </h3>
              <p className="text-sm text-gray-700">Current Term: {currentTerm}</p>
            </div>
          </div>
        </div>

        {/* Node Grid */}
        <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
          {nodeStatuses.map((node) => (
            <div
              key={node.nodeId}
              className={`border-2 rounded-lg p-4 ${
                node.isOnline
                  ? node.isLeader
                    ? 'border-yellow-400 bg-yellow-50'
                    : 'border-green-400 bg-green-50'
                  : 'border-red-400 bg-red-50'
              }`}
            >
              <div className="flex items-center justify-between mb-3">
                <div className="flex items-center gap-2">
                  <Server size={20} />
                  <h3 className="font-bold">Node {node.nodeId}</h3>
                </div>
                {node.isLeader && <Crown size={20} className="text-yellow-600" />}
              </div>

              <div className="space-y-2 text-sm">
                <div className="flex justify-between">
                  <span className="text-gray-600">Status:</span>
                  <span
                    className={`font-semibold ${
                      node.isOnline ? 'text-green-600' : 'text-red-600'
                    }`}
                  >
                    {node.isOnline ? '🟢 Online' : '🔴 Offline'}
                  </span>
                </div>
                <div className="flex justify-between">
                  <span className="text-gray-600">State:</span>
                  <span className="font-mono">{node.state}</span>
                </div>
                <div className="flex justify-between">
                  <span className="text-gray-600">Term:</span>
                  <span className="font-mono">{node.term}</span>
                </div>
                <div className="flex justify-between text-xs">
                  <span className="text-gray-600">Address:</span>
                  <span className="font-mono">{node.httpAddr}</span>
                </div>
              </div>
            </div>
          ))}
        </div>

        {nodeStatuses.length === 0 && (
          <div className="flex items-center gap-3 p-4 bg-yellow-50 border border-yellow-200 rounded">
            <AlertCircle size={24} className="text-yellow-600" />
            <p>No nodes detected. Make sure the servers are running.</p>
          </div>
        )}
      </div>

      {/* Performance Graph */}
      <div className="bg-white rounded-lg shadow p-6">
        <h2 className="text-2xl font-bold mb-4">Requests Per Second (Real-time)</h2>
        
        {/* Debug info */}
        {timeSeriesData.length > 0 && (
          <div className="mb-2 text-xs text-gray-500 font-mono">
            Data points: {timeSeriesData.length} | 
            Last values: N1={timeSeriesData[timeSeriesData.length - 1]?.node1.toFixed(1)}, 
            N2={timeSeriesData[timeSeriesData.length - 1]?.node2.toFixed(1)}, 
            N3={timeSeriesData[timeSeriesData.length - 1]?.node3.toFixed(1)}
          </div>
        )}
        
        {timeSeriesData.length > 0 ? (
          <ResponsiveContainer width="100%" height={400}>
            <LineChart data={timeSeriesData}>
              <CartesianGrid strokeDasharray="3 3" />
              <XAxis
                dataKey="timestamp"
                tickFormatter={formatTime}
                label={{ value: 'Time', position: 'insideBottom', offset: -5 }}
                interval="preserveStartEnd"
                minTickGap={50}
              />
              <YAxis 
                label={{ value: 'Requests/sec', angle: -90, position: 'insideLeft' }}
                domain={[0, 'auto']}
                allowDecimals={true}
              />
              <Tooltip 
                labelFormatter={formatTime}
                formatter={(value: number, name: string) => {
                  const nodeNames: { [key: string]: string } = {
                    'node1': 'Node 1',
                    'node2': 'Node 2',
                    'node3': 'Node 3',
                  };
                  return [value.toFixed(1), nodeNames[name as keyof typeof nodeNames] || name];
                }}
              />
              <Legend />
              <Line
                type="monotone"
                dataKey="node1"
                stroke="#3b82f6"
                name="Node 1"
                strokeWidth={2}
                dot={false}
                isAnimationActive={false}
                connectNulls={true}
              />
              <Line
                type="monotone"
                dataKey="node2"
                stroke="#10b981"
                name="Node 2"
                strokeWidth={2}
                dot={false}
                isAnimationActive={false}
                connectNulls={true}
              />
              <Line
                type="monotone"
                dataKey="node3"
                stroke="#f59e0b"
                name="Node 3"
                strokeWidth={2}
                dot={false}
                isAnimationActive={false}
                connectNulls={true}
              />
            </LineChart>
          </ResponsiveContainer>
        ) : (
          <div className="h-64 flex items-center justify-center text-gray-500">
            <p>Run a stress test to see performance data</p>
          </div>
        )}
      </div>
    </div>
  );
}
