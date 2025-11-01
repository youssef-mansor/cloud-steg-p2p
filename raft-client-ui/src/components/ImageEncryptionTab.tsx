import { useState, useRef } from 'react';
import { Upload, Download, Play, StopCircle, Loader2 } from 'lucide-react';
import { BarChart, Bar, LineChart, Line, XAxis, YAxis, CartesianGrid, Tooltip, Legend, ResponsiveContainer } from 'recharts';
import { apiClient } from '../api/client';
import type { NodeStats, TimeSeriesDataPoint } from '../types';

export function ImageEncryptionTab() {
  const [selectedFile, setSelectedFile] = useState<File | null>(null);
  const [previewUrl, setPreviewUrl] = useState<string | null>(null);
  const [encryptedBlob, setEncryptedBlob] = useState<Blob | null>(null);
  const [decryptedBlob, setDecryptedBlob] = useState<Blob | null>(null);
  const [isProcessing, setIsProcessing] = useState(false);
  const [isDecrypting, setIsDecrypting] = useState(false);
  const [processingNode, setProcessingNode] = useState<number | null>(null);
  const [decryptingNode, setDecryptingNode] = useState<number | null>(null);
  const [latency, setLatency] = useState<number | null>(null);
  const [decryptLatency, setDecryptLatency] = useState<number | null>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const stegoFileInputRef = useRef<HTMLInputElement>(null);

  // Stress test state
  const [isStressTesting, setIsStressTesting] = useState(false);
  const [stressTestProgress, setStressTestProgress] = useState(0);
  const [stressTestTotal, setStressTestTotal] = useState(1000);
  const [stressTestThreads, setStressTestThreads] = useState(10);
  const [stressTestStats, setStressTestStats] = useState<NodeStats[]>([]);
  const [throughputTimeSeries, setThroughputTimeSeries] = useState<TimeSeriesDataPoint[]>([]);
  const [latencyTimeSeries, setLatencyTimeSeries] = useState<TimeSeriesDataPoint[]>([]);

  const handleFileSelect = (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;

    setSelectedFile(file);
    setEncryptedBlob(null);
    setLatency(null);

    // Create preview
    const reader = new FileReader();
    reader.onload = () => setPreviewUrl(reader.result as string);
    reader.readAsDataURL(file);
  };

  const handleEncrypt = async () => {
    if (!selectedFile) return;

    setIsProcessing(true);
    setLatency(null);

    try {
      const result = await apiClient.uploadImageToCluster(selectedFile);

      if (result.success && result.data) {
        setEncryptedBlob(result.data);
        setProcessingNode(result.nodeId || null);
        setLatency(result.latency);
      } else {
        alert(`Encryption failed: ${result.error}`);
      }
    } catch (error) {
      alert(`Error: ${error}`);
    } finally {
      setIsProcessing(false);
    }
  };

  const handleDownload = () => {
    if (!encryptedBlob) return;

    const url = URL.createObjectURL(encryptedBlob);
    const a = document.createElement('a');
    a.href = url;
    a.download = 'encrypted-image.png';
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    URL.revokeObjectURL(url);
  };

  const handleDecrypt = async (stegoFile: File) => {
    setIsDecrypting(true);
    setDecryptLatency(null);

    try {
      const result = await apiClient.decryptImageFromCluster(stegoFile);

      if (result.success && result.data) {
        setDecryptedBlob(result.data);
        setDecryptingNode(result.nodeId || null);
        setDecryptLatency(result.latency);
      } else {
        alert(`Decryption failed: ${result.error}`);
      }
    } catch (error) {
      alert(`Error: ${error}`);
    } finally {
      setIsDecrypting(false);
    }
  };

  const handleDecryptFileSelect = (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;

    handleDecrypt(file);
  };

  const handleDownloadDecrypted = () => {
    if (!decryptedBlob) return;

    const url = URL.createObjectURL(decryptedBlob);
    const a = document.createElement('a');
    a.href = url;
    a.download = 'decrypted-image.bin';
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    URL.revokeObjectURL(url);
  };

  const handleStressTest = async () => {
    if (!selectedFile) {
      alert('Please select an image first');
      return;
    }

    setIsStressTesting(true);
    setStressTestProgress(0);
    setStressTestStats([]); // Clear previous stats
    setThroughputTimeSeries([]); // Clear previous throughput data
    setLatencyTimeSeries([]); // Clear previous latency data

    const stats = new Map<number, { 
      success: number; 
      failure: number; 
      totalLatency: number;
      requestTimestamps: number[]; // Track when each request completed
      latencyTimestamps: Array<{ timestamp: number; latency: number }>; // Track latency at each timestamp
    }>();
    apiClient.getNodes().forEach((node) => {
      stats.set(node.id, { success: 0, failure: 0, totalLatency: 0, requestTimestamps: [], latencyTimestamps: [] });
    });

    const startTime = Date.now();
    const requestsPerThread = Math.floor(stressTestTotal / stressTestThreads);
    let completedRequests = 0;

    // Helper function to calculate and update stats
    const updateStatsDisplay = () => {
      const totalElapsedSeconds = Math.max((Date.now() - startTime) / 1000, 0.1);
      const nodeStats: NodeStats[] = [];

      stats.forEach((stat, nodeId) => {
        const total = stat.success + stat.failure;
        
        // Calculate true throughput: requests per second based on actual request timing
        let requestsPerSecond = 0;
        if (stat.requestTimestamps.length > 1) {
          // Time span from first to last request for this node
          const nodeStartTime = stat.requestTimestamps[0];
          const nodeEndTime = stat.requestTimestamps[stat.requestTimestamps.length - 1];
          const nodeElapsedSeconds = (nodeEndTime - nodeStartTime) / 1000;
          
          // Avoid division by zero if all requests completed instantly
          if (nodeElapsedSeconds > 0) {
            requestsPerSecond = stat.success / nodeElapsedSeconds;
          } else {
            requestsPerSecond = stat.success; // Fallback: assume 1 second minimum
          }
        } else if (stat.success > 0) {
          // Only one request completed, estimate based on total test time
          requestsPerSecond = stat.success / totalElapsedSeconds;
        }
        
        nodeStats.push({
          nodeId,
          requestsSent: total,
          successCount: stat.success,
          failureCount: stat.failure,
          averageLatency: stat.success > 0 ? stat.totalLatency / stat.success : 0,
          requestsPerSecond,
          lastUpdated: Date.now(),
        });
      });

      setStressTestStats(nodeStats);
      
      // Update the monitoring tab graph if it exists

      const updateFunc = (globalThis as Record<string, unknown>).updateClusterRequestRates as
        | ((rates: { node1: number; node2: number; node3: number }) => void)
        | undefined;
      
      if (updateFunc) {
        updateFunc({
          node1: nodeStats.find((s) => s.nodeId === 1)?.requestsPerSecond || 0,
          node2: nodeStats.find((s) => s.nodeId === 2)?.requestsPerSecond || 0,
          node3: nodeStats.find((s) => s.nodeId === 3)?.requestsPerSecond || 0,
        });
      }
    };

    // Run stress test with concurrent threads
    const threads: Promise<void>[] = [];

    for (let t = 0; t < stressTestThreads; t++) {
      const thread = (async () => {
        for (let i = 0; i < requestsPerThread; i++) {
          try {
            // Send to cluster - leader will load balance
            const result = await apiClient.uploadImageToCluster(selectedFile);
            const requestCompletionTime = Date.now();
            
            if (result.success && result.processedBy) {
              // Track which node actually processed the request (not which node received it)
              const nodeStat = stats.get(result.processedBy);
              if (nodeStat) {
                nodeStat.success++;
                nodeStat.totalLatency += result.latency;
                nodeStat.requestTimestamps.push(requestCompletionTime);
                nodeStat.latencyTimestamps.push({ timestamp: requestCompletionTime, latency: result.latency });
              }
            } else {
              // Count failure for all nodes (unknown which one failed)
              stats.forEach((stat) => stat.failure++);
            }
          } catch {
            stats.forEach((stat) => stat.failure++);
          }

          completedRequests++;
          setStressTestProgress(completedRequests);
        }
      })();

      threads.push(thread);
    }

    await Promise.all(threads);

    // Final stats update and graphs - ONLY shown at the end
    updateStatsDisplay();

    // Calculate time-series data for throughput graph ONLY at the end (faster!)
    // Sample the data at regular intervals instead of every 5 requests
    const timeSeries: TimeSeriesDataPoint[] = [];
    const minTimestamp = Math.min(
      ...Array.from(stats.values())
        .filter((s) => s.requestTimestamps.length > 0)
        .map((s) => s.requestTimestamps[0]),
      Date.now()
    );
    const maxTimestamp = Math.max(
      ...Array.from(stats.values())
        .filter((s) => s.requestTimestamps.length > 0)
        .map((s) => s.requestTimestamps[s.requestTimestamps.length - 1]),
      Date.now()
    );
    
    // Create 20 data points evenly spaced across the test duration
    const interval = (maxTimestamp - minTimestamp) / 20;
    const latencyTimeSeries: TimeSeriesDataPoint[] = [];
    
    for (let i = 0; i < 20; i++) {
      const sampleTime = minTimestamp + interval * i;
      
      // Calculate how many requests each node had completed by sampleTime
      const node1Requests = Array.from(stats.get(1)?.requestTimestamps || []).filter(
        (t) => t <= sampleTime
      ).length;
      const node2Requests = Array.from(stats.get(2)?.requestTimestamps || []).filter(
        (t) => t <= sampleTime
      ).length;
      const node3Requests = Array.from(stats.get(3)?.requestTimestamps || []).filter(
        (t) => t <= sampleTime
      ).length;
      
      // Calculate avg latency up to that point for each node
      const node1Latencies = Array.from(stats.get(1)?.latencyTimestamps || []).filter(
        (lt) => lt.timestamp <= sampleTime
      );
      const node2Latencies = Array.from(stats.get(2)?.latencyTimestamps || []).filter(
        (lt) => lt.timestamp <= sampleTime
      );
      const node3Latencies = Array.from(stats.get(3)?.latencyTimestamps || []).filter(
        (lt) => lt.timestamp <= sampleTime
      );
      
      const node1AvgLatency = node1Latencies.length > 0 
        ? node1Latencies.reduce((sum, lt) => sum + lt.latency, 0) / node1Latencies.length 
        : 0;
      const node2AvgLatency = node2Latencies.length > 0 
        ? node2Latencies.reduce((sum, lt) => sum + lt.latency, 0) / node2Latencies.length 
        : 0;
      const node3AvgLatency = node3Latencies.length > 0 
        ? node3Latencies.reduce((sum, lt) => sum + lt.latency, 0) / node3Latencies.length 
        : 0;
      
      // Calculate req/sec up to that point
      const timeWindowSeconds = Math.max((sampleTime - minTimestamp) / 1000, 0.1);
      
      timeSeries.push({
        timestamp: sampleTime,
        node1: node1Requests / timeWindowSeconds,
        node2: node2Requests / timeWindowSeconds,
        node3: node3Requests / timeWindowSeconds,
        node1Latency: node1AvgLatency,
        node2Latency: node2AvgLatency,
        node3Latency: node3AvgLatency,
      });
      
      // Also create a latency-focused time-series
      latencyTimeSeries.push({
        timestamp: sampleTime,
        node1: node1AvgLatency,
        node2: node2AvgLatency,
        node3: node3AvgLatency,
      });
    }
    
    setThroughputTimeSeries(timeSeries);
    setLatencyTimeSeries(latencyTimeSeries);
    setIsStressTesting(false);
  };

  const handleStopStressTest = () => {
    setIsStressTesting(false);
  };

  return (
    <div className="space-y-6">
      <div className="bg-white rounded-lg shadow p-6">
        <h2 className="text-2xl font-bold mb-4">Image Encryption</h2>

        {/* File Upload */}
        <div className="mb-6">
          <input
            ref={fileInputRef}
            type="file"
            accept="image/*"
            onChange={handleFileSelect}
            className="hidden"
          />
          <button
            onClick={() => fileInputRef.current?.click()}
            className="flex items-center gap-2 px-4 py-2 bg-blue-500 text-white rounded hover:bg-blue-600"
          >
            <Upload size={20} />
            Select Image
          </button>
          {selectedFile && (
            <p className="mt-2 text-sm text-gray-600">
              Selected: {selectedFile.name} ({(selectedFile.size / 1024).toFixed(2)} KB)
            </p>
          )}
        </div>

        {/* Preview */}
        {previewUrl && (
          <div className="mb-6">
            <h3 className="font-semibold mb-2">Original Image</h3>
            <img src={previewUrl} alt="Preview" className="max-w-md border rounded" />
          </div>
        )}

        {/* Encrypt Button */}
        <div className="mb-6">
          <button
            onClick={handleEncrypt}
            disabled={!selectedFile || isProcessing}
            className="flex items-center gap-2 px-6 py-3 bg-green-500 text-white rounded hover:bg-green-600 disabled:bg-gray-300 disabled:cursor-not-allowed"
          >
            {isProcessing ? (
              <>
                <Loader2 size={20} className="animate-spin" />
                Encrypting...
              </>
            ) : (
              <>Encrypt & Get Steganography Image</>
            )}
          </button>
        </div>

        {/* Results */}
        {encryptedBlob && (
          <div className="border-t pt-4">
            <h3 className="font-semibold mb-2 text-green-600">✓ Encryption Complete</h3>
            <p className="text-sm text-gray-600 mb-2">
              Processed by Node {processingNode} • Latency: {latency?.toFixed(0)}ms
            </p>
            <button
              onClick={handleDownload}
              className="flex items-center gap-2 px-4 py-2 bg-purple-500 text-white rounded hover:bg-purple-600"
            >
              <Download size={20} />
              Download Encrypted Image
            </button>
          </div>
        )}

        {/* Decrypt Section */}
        <div className="mt-8 pt-8 border-t">
          <h2 className="text-xl font-bold mb-4">Decryption</h2>
          <p className="text-sm text-gray-600 mb-4">
            Upload a stego image to extract the hidden data
          </p>

          {/* File Input for Decryption */}
          <div className="mb-6">
            <input
              ref={stegoFileInputRef}
              type="file"
              accept="image/*"
              onChange={handleDecryptFileSelect}
              className="hidden"
            />
            <button
              onClick={() => stegoFileInputRef.current?.click()}
              disabled={isDecrypting}
              className="flex items-center gap-2 px-6 py-3 bg-blue-500 text-white rounded hover:bg-blue-600 disabled:bg-gray-300 disabled:cursor-not-allowed"
            >
              <Upload size={20} />
              Select Stego Image to Decrypt
            </button>
          </div>

          {/* Decrypt Button */}
          {decryptedBlob && (
            <div className="border rounded p-4 bg-blue-50">
              <h3 className="font-semibold mb-2 text-blue-600">✓ Decryption Complete</h3>
              <p className="text-sm text-gray-600 mb-2">
                Processed by Node {decryptingNode} • Latency: {decryptLatency?.toFixed(0)}ms
              </p>
              <button
                onClick={handleDownloadDecrypted}
                className="flex items-center gap-2 px-4 py-2 bg-indigo-500 text-white rounded hover:bg-indigo-600"
              >
                <Download size={20} />
                Download Extracted Data
              </button>
            </div>
          )}
        </div>
      </div>

      {/* Stress Test Section */}
      <div className="bg-white rounded-lg shadow p-6">
        <h2 className="text-2xl font-bold mb-4">Stress Test</h2>

        <div className="grid grid-cols-2 gap-4 mb-4">
          <div>
            <label className="block text-sm font-medium mb-1">Total Requests</label>
            <input
              type="number"
              value={stressTestTotal}
              onChange={(e) => setStressTestTotal(Number(e.target.value))}
              disabled={isStressTesting}
              className="w-full px-3 py-2 border rounded"
              min="1"
            />
          </div>
          <div>
            <label className="block text-sm font-medium mb-1">Concurrent Threads</label>
            <input
              type="number"
              value={stressTestThreads}
              onChange={(e) => setStressTestThreads(Number(e.target.value))}
              disabled={isStressTesting}
              className="w-full px-3 py-2 border rounded"
              min="1"
              max="50"
            />
          </div>
        </div>

        <div className="flex gap-4 mb-4">
          <button
            onClick={handleStressTest}
            disabled={!selectedFile || isStressTesting}
            className="flex items-center gap-2 px-6 py-3 bg-orange-500 text-white rounded hover:bg-orange-600 disabled:bg-gray-300 disabled:cursor-not-allowed"
          >
            <Play size={20} />
            Start Stress Test
          </button>
          {isStressTesting && (
            <button
              onClick={handleStopStressTest}
              className="flex items-center gap-2 px-6 py-3 bg-red-500 text-white rounded hover:bg-red-600"
            >
              <StopCircle size={20} />
              Stop
            </button>
          )}
        </div>

        {/* Progress */}
        {isStressTesting && (
          <div className="mb-4">
            <div className="flex justify-between text-sm mb-1">
              <span>Progress</span>
              <span>
                {stressTestProgress} / {stressTestTotal}
              </span>
            </div>
            <div className="w-full bg-gray-200 rounded-full h-4">
              <div
                className="bg-blue-500 h-4 rounded-full transition-all"
                style={{ width: `${(stressTestProgress / stressTestTotal) * 100}%` }}
              />
            </div>
          </div>
        )}

        {/* Stats */}
        {stressTestStats.length > 0 && (
          <div className="border-t pt-4">
            <h3 className="font-semibold mb-2">Test Results</h3>
            <div className="overflow-x-auto">
              <table className="w-full text-sm">
                <thead className="bg-gray-100">
                  <tr>
                    <th className="p-2 text-left">Node</th>
                    <th className="p-2 text-right">Requests</th>
                    <th className="p-2 text-right">Success</th>
                    <th className="p-2 text-right">Failure</th>
                    <th className="p-2 text-right">Avg Latency</th>
                    <th className="p-2 text-right">Req/sec</th>
                  </tr>
                </thead>
                <tbody>
                  {stressTestStats.map((stat) => (
                    <tr key={stat.nodeId} className="border-t">
                      <td className="p-2">Node {stat.nodeId}</td>
                      <td className="p-2 text-right">{stat.requestsSent}</td>
                      <td className="p-2 text-right text-green-600">{stat.successCount}</td>
                      <td className="p-2 text-right text-red-600">{stat.failureCount}</td>
                      <td className="p-2 text-right">{stat.averageLatency.toFixed(0)}ms</td>
                      <td className="p-2 text-right">{stat.requestsPerSecond.toFixed(1)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>

            {/* Charts below stats */}
            <div className="mt-8 grid grid-cols-1 lg:grid-cols-2 gap-6">
              {/* Requests per Node */}
              <div className="border rounded p-4">
                <h4 className="font-semibold mb-4">Requests per Node</h4>
                <ResponsiveContainer width="100%" height={300}>
                  <BarChart data={stressTestStats}>
                    <CartesianGrid strokeDasharray="3 3" />
                    <XAxis dataKey={(d) => `Node ${d.nodeId}`} />
                    <YAxis />
                    <Tooltip />
                    <Legend />
                    <Bar dataKey="successCount" fill="#10b981" name="Successful" />
                    <Bar dataKey="failureCount" fill="#ef4444" name="Failed" />
                  </BarChart>
                </ResponsiveContainer>
              </div>

              {/* Requests per Second - Time Series */}
              <div className="border rounded p-4">
                <h4 className="font-semibold mb-4">Throughput (Req/sec) - Real-time</h4>
                {throughputTimeSeries.length > 0 ? (
                  <ResponsiveContainer width="100%" height={300}>
                    <LineChart data={throughputTimeSeries}>
                      <CartesianGrid strokeDasharray="3 3" />
                      <XAxis 
                        dataKey={(d) => {
                          const date = new Date(d.timestamp);
                          return date.toLocaleTimeString('en-US', { 
                            hour12: false, 
                            minute: '2-digit', 
                            second: '2-digit' 
                          });
                        }}
                        angle={-45}
                        textAnchor="end"
                        height={80}
                      />
                      <YAxis label={{ value: 'Req/sec', angle: -90, position: 'insideLeft' }} />
                      <Tooltip 
                        labelFormatter={(value) => {
                          const date = new Date(value);
                          return date.toLocaleTimeString();
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
                  <div className="h-[300px] flex items-center justify-center text-gray-500">
                    <p>Run stress test to see throughput over time</p>
                  </div>
                )}
              </div>

              {/* Latency Over Time */}
              <div className="border rounded p-4">
                <h4 className="font-semibold mb-4">Latency Over Time (ms)</h4>
                {latencyTimeSeries.length > 0 ? (
                  <ResponsiveContainer width="100%" height={300}>
                    <LineChart data={latencyTimeSeries}>
                      <CartesianGrid strokeDasharray="3 3" />
                      <XAxis 
                        dataKey={(d) => new Date(d.timestamp).toLocaleTimeString()} 
                        label={{ value: 'Time', position: 'insideBottomRight', offset: -5 }}
                      />
                      <YAxis label={{ value: 'Latency (ms)', angle: -90, position: 'insideLeft' }} />
                      <Tooltip 
                        formatter={(value) => `${(value as number).toFixed(2)}ms`}
                        labelFormatter={(label) => `Time: ${new Date(label as number).toLocaleTimeString()}`}
                      />
                      <Legend />
                      <Line 
                        type="monotone" 
                        dataKey="node1" 
                        stroke="#ef4444" 
                        name="Node 1" 
                        strokeWidth={2}
                        dot={false}
                      />
                      <Line 
                        type="monotone" 
                        dataKey="node2" 
                        stroke="#3b82f6" 
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
                  <div className="h-[300px] flex items-center justify-center text-gray-500">
                    <p>Run stress test to see latency over time</p>
                  </div>
                )}
              </div>

              {/* Load Distribution (Pie equivalent with Bar) */}
              <div className="border rounded p-4">
                <h4 className="font-semibold mb-4">Total Requests</h4>
                <ResponsiveContainer width="100%" height={300}>
                  <BarChart data={stressTestStats}>
                    <CartesianGrid strokeDasharray="3 3" />
                    <XAxis dataKey={(d) => `Node ${d.nodeId}`} />
                    <YAxis />
                    <Tooltip />
                    <Legend />
                    <Bar dataKey="requestsSent" fill="#8b5cf6" name="Total Requests" />
                  </BarChart>
                </ResponsiveContainer>
              </div>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
