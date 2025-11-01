import { useState, useEffect } from 'react';
import { Activity, Server, Crown, AlertCircle, RefreshCw } from 'lucide-react';
import { LineChart, Line, XAxis, YAxis, CartesianGrid, Tooltip, Legend, ResponsiveContainer } from 'recharts';
import { useClusterMetrics } from '../hooks/useClusterMetrics';
import type { TimeSeriesDataPoint } from '../types';

export function ClusterMonitoringTab() {
  const { nodeStatuses, lastUpdate, isPolling, setIsPolling, refresh } = useClusterMetrics(2000);
  const [timeSeriesData, setTimeSeriesData] = useState<TimeSeriesDataPoint[]>([]);
  
  // Track request rates per node - can be updated from stress test
  const [nodeRequestRates, setNodeRequestRates] = useState<{ node1: number; node2: number; node3: number }>({
    node1: 0,
    node2: 0,
    node3: 0,
  });

  // Expose method for updating rates from stress test (cast as unknown to avoid TypeScript any error)
  const updateRates = (rates: { node1: number; node2: number; node3: number }) => {
    setNodeRequestRates(rates);
  };
  
  // Make available globally for stress test component
  useEffect(() => {
    (globalThis as Record<string, unknown>).updateClusterRequestRates = updateRates;
  }, []);

  // Update time series data when request rates change
  useEffect(() => {
    const interval = setInterval(() => {
      const now = Date.now();
      
      // Create new data point with current request rates
      const dataPoint: TimeSeriesDataPoint = {
        timestamp: now,
        node1: nodeRequestRates.node1,
        node2: nodeRequestRates.node2,
        node3: nodeRequestRates.node3,
      };
      
      setTimeSeriesData((prev) => {
        const newData = [...prev];
        newData.push(dataPoint);
        
        // Keep only last 30 data points (1 minute of data at 2s intervals)
        return newData.slice(-30);
      });
    }, 2000);

    return () => clearInterval(interval);
  }, [nodeRequestRates]);

  // Format timestamp for graph
  const formatTime = (timestamp: number) => {
    const date = new Date(timestamp);
    return date.toLocaleTimeString('en-US', { hour12: false, minute: '2-digit', second: '2-digit' });
  };

  const leader = nodeStatuses.find((n) => n.isLeader);
  const currentTerm = leader?.term || Math.max(...nodeStatuses.map((n) => n.term), 0);

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
        
        {timeSeriesData.length > 0 ? (
          <ResponsiveContainer width="100%" height={400}>
            <LineChart data={timeSeriesData}>
              <CartesianGrid strokeDasharray="3 3" />
              <XAxis
                dataKey="timestamp"
                tickFormatter={formatTime}
                label={{ value: 'Time', position: 'insideBottom', offset: -5 }}
              />
              <YAxis label={{ value: 'Requests/sec', angle: -90, position: 'insideLeft' }} />
              <Tooltip labelFormatter={formatTime} />
              <Legend />
              <Line
                type="monotone"
                dataKey="node1"
                stroke="#3b82f6"
                name="Node 1"
                strokeWidth={2}
                dot={false}
              />
              <Line
                type="monotone"
                dataKey="node2"
                stroke="#10b981"
                name="Node 2"
                strokeWidth={2}
                dot={false}
              />
              <Line
                type="monotone"
                dataKey="node3"
                stroke="#f59e0b"
                name="Node 3"
                strokeWidth={2}
                dot={false}
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
