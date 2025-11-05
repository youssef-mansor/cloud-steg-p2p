import { useState, useRef } from 'react';
import { Upload, Download, Play, StopCircle, Loader2 } from 'lucide-react';
import { BarChart, Bar, LineChart, Line, XAxis, YAxis, CartesianGrid, Tooltip, Legend, ResponsiveContainer } from 'recharts';
import { apiClient } from '../api/client';
import type { NodeStats, TimeSeriesDataPoint } from '../types';

export function ImageEncryptionTab() {
  const [selectedCoverFile, setSelectedCoverFile] = useState<File | null>(null);
  const [selectedSecretFile, setSelectedSecretFile] = useState<File | null>(null);
  const [coverPreviewUrl, setCoverPreviewUrl] = useState<string | null>(null);
  const [secretPreviewUrl, setSecretPreviewUrl] = useState<string | null>(null);
  const [encryptedBlob, setEncryptedBlob] = useState<Blob | null>(null);
  const [decryptedBlob, setDecryptedBlob] = useState<Blob | null>(null);
  const [isProcessing, setIsProcessing] = useState(false);
  const [isDecrypting, setIsDecrypting] = useState(false);
  const [processingNode, setProcessingNode] = useState<number | null>(null);
  const [decryptingNode, setDecryptingNode] = useState<number | null>(null);
  const [latency, setLatency] = useState<number | null>(null);
  const [decryptLatency, setDecryptLatency] = useState<number | null>(null);
  const coverFileInputRef = useRef<HTMLInputElement>(null);
  const secretFileInputRef = useRef<HTMLInputElement>(null);
  const stegoFileInputRef = useRef<HTMLInputElement>(null);

  // Stress test state
  const [isStressTesting, setIsStressTesting] = useState(false);
  const [stressTestProgress, setStressTestProgress] = useState(0);
  const [stressTestTotal, setStressTestTotal] = useState(1000);
  const [stressTestThreads, setStressTestThreads] = useState(10);
  const [stressTestStats, setStressTestStats] = useState<NodeStats[]>([]);
  const [throughputTimeSeries, setThroughputTimeSeries] = useState<TimeSeriesDataPoint[]>([]);
  const [latencyTimeSeries, setLatencyTimeSeries] = useState<TimeSeriesDataPoint[]>([]);
  const [enableFailureTests, setEnableFailureTests] = useState(false);
  const [failureEvents, setFailureEvents] = useState<Array<{ time: number; node: number; event: string; requestNum: number }>>([]);
  const [stressTestDuration, setStressTestDuration] = useState<number | null>(null);

  const handleCoverFileSelect = (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;

    setSelectedCoverFile(file);
    setEncryptedBlob(null);
    setLatency(null);

    // Create preview
    const reader = new FileReader();
    reader.onload = () => setCoverPreviewUrl(reader.result as string);
    reader.readAsDataURL(file);
  };

  const handleSecretFileSelect = (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;

    setSelectedSecretFile(file);
    setEncryptedBlob(null);
    setLatency(null);

    // Create preview
    const reader = new FileReader();
    reader.onload = () => setSecretPreviewUrl(reader.result as string);
    reader.readAsDataURL(file);
  };

  const handleEncrypt = async () => {
    if (!selectedCoverFile || !selectedSecretFile) return;

    setIsProcessing(true);
    setLatency(null);

    try {
      const result = await apiClient.uploadImageToCluster(selectedCoverFile, selectedSecretFile);

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

    console.log(`💾 Downloading encrypted stego image: size=${encryptedBlob.size} bytes, type=${encryptedBlob.type}`);
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
      console.log(`🔓 Starting extraction from stego image`);
      const result = await apiClient.decryptImageFromCluster(stegoFile);

      if (result.success && result.data) {
        setDecryptedBlob(result.data);
        setDecryptingNode(result.nodeId || null);
        setDecryptLatency(result.latency);
        console.log(`✅ Extraction succeeded on node ${result.nodeId}, latency: ${result.latency}ms`);
        console.log(`📦 Decrypted blob: size=${result.data.size} bytes, type=${result.data.type}`);
      } else {
        console.error(`❌ Extraction failed: ${result.error}`);
        alert(`❌ Extraction failed:\n\n${result.error}\n\nPlease check:\n1. The stego image is intact\n2. At least one server node is running\n\nCheck browser console for more details.`);
      }
    } catch (error) {
      console.error('❌ Extraction error:', error);
      alert(`❌ Error during extraction:\n\n${error instanceof Error ? error.message : String(error)}\n\nCheck browser console for more details.`);
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

    // Determine file extension from MIME type
    let extension = 'png';
    if (decryptedBlob.type.includes('jpeg') || decryptedBlob.type.includes('jpg')) {
      extension = 'jpg';
    } else if (decryptedBlob.type.includes('png')) {
      extension = 'png';
    } else if (decryptedBlob.type.includes('gif')) {
      extension = 'gif';
    } else if (decryptedBlob.type.includes('webp')) {
      extension = 'webp';
    }
    
    console.log(`💾 Downloading extracted secret image: size=${decryptedBlob.size} bytes, type=${decryptedBlob.type}, extension=${extension}`);
    const url = URL.createObjectURL(decryptedBlob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `extracted-secret.${extension}`;
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    URL.revokeObjectURL(url);
  };

  const handleStressTest = async () => {
    if (!selectedCoverFile || !selectedSecretFile) {
      alert('Please select both cover and secret images first');
      return;
    }

    setIsStressTesting(true);
    setStressTestProgress(0);
    setStressTestStats([]);
    setThroughputTimeSeries([]);
    setLatencyTimeSeries([]);
    setFailureEvents([]);
    setStressTestDuration(null);

    const startTime = Date.now();

    const stats = new Map<number, { 
      success: number; 
      failure: number; 
      totalLatency: number;
      requestTimestamps: number[];
      latencyTimestamps: Array<{ timestamp: number; latency: number }>;
    }>();
    apiClient.getNodes().forEach((node) => {
      stats.set(node.id, { success: 0, failure: 0, totalLatency: 0, requestTimestamps: [], latencyTimestamps: [] });
    });

    const requestsPerThread = stressTestTotal;
    let completedRequests = 0;
    
    // Failure testing configuration - all nodes can fail, but at least 1 must be up
    const failedNodes = new Set<number>(); // Track currently failed nodes
    const nodeFailureSchedule = new Map<number, number>(); // node -> next failure request count
    const maxSimultaneousFailures = 2; // Max 2 nodes can be down at once (ensures 1 is always up)
    const maxFailuresPerNode = 3;
    const nodeFailureCount = new Map<number, number>([[1, 0], [2, 0], [3, 0]]);
    
    // Initialize first failure for each node at random intervals
    if (enableFailureTests) {
      nodeFailureSchedule.set(1, Math.floor(Math.random() * 50) + 50);  // 50-100
      nodeFailureSchedule.set(2, Math.floor(Math.random() * 100) + 50); // 50-150
      nodeFailureSchedule.set(3, Math.floor(Math.random() * 80) + 60);  // 60-140
    }
    
    // Background failure manager - ensures at least 1 node always up
    const failureManager = async () => {
      if (!enableFailureTests) return;
      
      console.log(`🎲 Failure testing enabled: All nodes can fail randomly, but at least 1 node always stays up`);
      
      while (completedRequests < stressTestTotal) {
        await new Promise(resolve => setTimeout(resolve, 200));
        
        // Check each node for scheduled failures
        for (const nodeId of [1, 2, 3]) {
          const nextFailure = nodeFailureSchedule.get(nodeId);
          const failureCount = nodeFailureCount.get(nodeId) || 0;
          
          // Check if this node should fail now
          if (
            nextFailure !== undefined &&
            completedRequests >= nextFailure &&
            failureCount < maxFailuresPerNode &&
            !failedNodes.has(nodeId)
          ) {
            // Safety check: Don't fail if it would leave us with no nodes
            if (failedNodes.size >= maxSimultaneousFailures) {
              console.log(`⚠️  Node ${nodeId} failure skipped - already ${failedNodes.size} nodes down`);
              // Reschedule for later
              nodeFailureSchedule.set(nodeId, completedRequests + 50);
              continue;
            }
            
            // Fail the node
            failedNodes.add(nodeId);
            nodeFailureCount.set(nodeId, failureCount + 1);
            const currentCount = completedRequests;
            
            console.log(`💥 [SIMULATED] Node ${nodeId} failure #${failureCount + 1} at ${currentCount} requests (${failedNodes.size} nodes down)`);
            setFailureEvents(prev => [...prev, { 
              time: Date.now(), 
              node: nodeId, 
              event: 'STOPPED', 
              requestNum: currentCount 
            }]);
            
            // Schedule recovery with random delay (5-15 seconds)
            const recoveryDelay = (Math.random() * 10000) + 5000;
            setTimeout(() => {
              failedNodes.delete(nodeId);
              console.log(`🔄 [SIMULATED] Node ${nodeId} recovered after ${(recoveryDelay/1000).toFixed(1)}s (${failedNodes.size} nodes down)`);
              setFailureEvents(prev => [...prev, { 
                time: Date.now(), 
                node: nodeId, 
                event: 'STARTED', 
                requestNum: completedRequests 
              }]);
            }, recoveryDelay);
            
            // Schedule next failure for this node with random interval
            const intervalMin = nodeId === 1 ? 50 : (nodeId === 2 ? 50 : 60);
            const intervalMax = nodeId === 1 ? 100 : (nodeId === 2 ? 150 : 140);
            const nextInterval = Math.floor(Math.random() * (intervalMax - intervalMin)) + intervalMin;
            nodeFailureSchedule.set(nodeId, currentCount + nextInterval);
          }
        }
      }
    };
    
    // Start failure manager if enabled
    const failureManagerPromise = enableFailureTests ? failureManager() : Promise.resolve();

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
            const result = await apiClient.uploadImageToCluster(selectedCoverFile, selectedSecretFile);
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
    
    // Wait for failure manager to complete if enabled
    await failureManagerPromise;

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
    
    const endTime = Date.now();
    const duration = (endTime - startTime) / 1000;
    setStressTestDuration(duration);
    
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

        {/* File Uploads */}
        <div className="mb-6 grid grid-cols-2 gap-6">
          {/* Cover Image */}
          <div>
            <input
              ref={coverFileInputRef}
              type="file"
              accept="image/*"
              onChange={handleCoverFileSelect}
              className="hidden"
            />
            <button
              onClick={() => coverFileInputRef.current?.click()}
              className="flex items-center gap-2 px-4 py-2 bg-blue-500 text-white rounded hover:bg-blue-600 w-full justify-center"
            >
              <Upload size={20} />
              Select Cover Image
            </button>
            {selectedCoverFile && (
              <p className="mt-2 text-sm text-gray-600">
                Cover: {selectedCoverFile.name} ({(selectedCoverFile.size / 1024).toFixed(2)} KB)
              </p>
            )}
          </div>

          {/* Secret Image */}
          <div>
            <input
              ref={secretFileInputRef}
              type="file"
              accept="image/*"
              onChange={handleSecretFileSelect}
              className="hidden"
            />
            <button
              onClick={() => secretFileInputRef.current?.click()}
              className="flex items-center gap-2 px-4 py-2 bg-purple-500 text-white rounded hover:bg-purple-600 w-full justify-center"
            >
              <Upload size={20} />
              Select Secret Image
            </button>
            {selectedSecretFile && (
              <p className="mt-2 text-sm text-gray-600">
                Secret: {selectedSecretFile.name} ({(selectedSecretFile.size / 1024).toFixed(2)} KB)
              </p>
            )}
          </div>
        </div>

        {/* Preview */}
        {(coverPreviewUrl || secretPreviewUrl) && (
          <div className="mb-6 grid grid-cols-2 gap-6">
            {coverPreviewUrl && (
              <div>
                <h3 className="font-semibold mb-2">Cover Image</h3>
                <img src={coverPreviewUrl} alt="Cover Preview" className="w-full border rounded" />
              </div>
            )}
            {secretPreviewUrl && (
              <div>
                <h3 className="font-semibold mb-2">Secret Image (to hide)</h3>
                <img src={secretPreviewUrl} alt="Secret Preview" className="w-full border rounded" />
              </div>
            )}
          </div>
        )}

        {/* Encrypt Button */}
        <div className="mb-6">
          <button
            onClick={handleEncrypt}
            disabled={!selectedCoverFile || !selectedSecretFile || isProcessing}
            className="flex items-center gap-2 px-6 py-3 bg-green-500 text-white rounded hover:bg-green-600 disabled:bg-gray-300 disabled:cursor-not-allowed"
          >
            {isProcessing ? (
              <>
                <Loader2 size={20} className="animate-spin" />
                Encrypting...
              </>
            ) : (
              <>Embed Secret into Cover Image</>
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
            <p className="text-sm text-gray-600 mb-4">
              Your secret image has been embedded into the cover image. Download the stego image below.
            </p>
            
            <button
              onClick={handleDownload}
              className="flex items-center gap-2 px-4 py-2 bg-purple-500 text-white rounded hover:bg-purple-600"
            >
              <Download size={20} />
              Download Stego Image
            </button>
          </div>
        )}

        {/* Decrypt Section */}
        <div className="mt-8 pt-8 border-t">
          <h2 className="text-xl font-bold mb-4">Extraction</h2>
          <p className="text-sm text-gray-600 mb-4">
            Upload a stego image to extract the hidden secret image
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
              {isDecrypting ? (
                <>
                  <Loader2 size={20} className="animate-spin" />
                  Extracting...
                </>
              ) : (
                <>
                  <Upload size={20} />
                  Select Stego Image to Extract From
                </>
              )}
            </button>
          </div>

          {/* Decrypt Results */}
          {decryptedBlob && (
            <div className="border rounded p-4 bg-blue-50">
              <h3 className="font-semibold mb-2 text-blue-600">✓ Extraction Complete</h3>
              <p className="text-sm text-gray-600 mb-2">
                Processed by Node {decryptingNode} • Latency: {decryptLatency?.toFixed(0)}ms
              </p>
              
              {/* Preview of decrypted image */}
              <div className="mb-3">
                <img 
                  src={URL.createObjectURL(decryptedBlob)} 
                  alt="Decrypted" 
                  className="max-w-full h-auto rounded border"
                  style={{ maxHeight: '200px' }}
                />
              </div>
              
              <button
                onClick={handleDownloadDecrypted}
                className="flex items-center gap-2 px-4 py-2 bg-indigo-500 text-white rounded hover:bg-indigo-600"
              >
                <Download size={20} />
                Download Decrypted Image
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
            <label className="block text-sm font-medium mb-1">Requests per Thread</label>
            <input
              type="number"
              value={stressTestTotal}
              onChange={(e) => setStressTestTotal(Number(e.target.value))}
              disabled={isStressTesting}
              className="w-full px-3 py-2 border rounded"
              min="1"
            />
            <p className="text-xs text-gray-500 mt-1">
              Total: {stressTestTotal * stressTestThreads} requests
            </p>
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
        
        {/* Failure Testing Toggle */}
        <div className="mb-4 p-4 border rounded bg-amber-50">
          <label className="flex items-center gap-3 cursor-pointer">
            <input
              type="checkbox"
              checked={enableFailureTests}
              onChange={(e) => setEnableFailureTests(e.target.checked)}
              disabled={isStressTesting}
              className="w-5 h-5 cursor-pointer"
            />
            <div>
              <span className="font-medium text-amber-900">Enable Automated Failure Testing</span>
              <p className="text-sm text-amber-700 mt-1">
                Simulates realistic server failures during stress test:
                <br />• <strong>All 3 nodes can fail</strong> at random intervals (50-150 requests)
                <br />• <strong>At least 1 node always stays up</strong> (max 2 nodes down simultaneously)
                <br />• Up to 3 failures per node, each recovering after 5-15 seconds
                <br />• Tests Raft's resilience and automatic load redistribution
              </p>
            </div>
          </label>
        </div>

        <div className="flex gap-4 mb-4">
          <button
            onClick={handleStressTest}
            disabled={!selectedCoverFile || !selectedSecretFile || isStressTesting}
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
                {stressTestProgress} / {stressTestTotal * stressTestThreads}
              </span>
            </div>
            <div className="w-full bg-gray-200 rounded-full h-4">
              <div
                className="bg-blue-500 h-4 rounded-full transition-all"
                style={{ width: `${(stressTestProgress / (stressTestTotal * stressTestThreads)) * 100}%` }}
              />
            </div>
          </div>
        )}

        {/* Stats */}
        {stressTestStats.length > 0 && (
          <div className="border-t pt-4">
            <h3 className="font-semibold mb-4">Test Results</h3>
            
            {/* Test Summary */}
            <div className="bg-blue-50 border border-blue-200 rounded p-4 mb-4">
              <div className="grid grid-cols-4 gap-4 mb-3">
                <div>
                  <p className="text-sm text-gray-600">Total Requests Sent</p>
                  <p className="text-2xl font-bold text-blue-600">{stressTestTotal * stressTestThreads}</p>
                </div>
                <div>
                  <p className="text-sm text-gray-600">Total Completed</p>
                  <p className="text-2xl font-bold text-green-600">
                    {stressTestStats.reduce((sum, s) => sum + s.requestsSent, 0)}
                  </p>
                </div>
                <div>
                  <p className="text-sm text-gray-600">Success Rate</p>
                  <p className="text-2xl font-bold text-green-600">
                    {stressTestStats.reduce((sum, s) => sum + s.successCount, 0) > 0
                      ? (
                          (stressTestStats.reduce((sum, s) => sum + s.successCount, 0) /
                            stressTestStats.reduce((sum, s) => sum + s.requestsSent, 0)) *
                          100
                        ).toFixed(1)
                      : '0'}
                    %
                  </p>
                </div>
                <div>
                  <p className="text-sm text-gray-600">Total Time</p>
                  <p className="text-2xl font-bold text-purple-600">
                    {stressTestDuration !== null ? `${stressTestDuration.toFixed(2)}s` : '-'}
                  </p>
                </div>
              </div>
              <div className="border-t pt-3">
                <p className="text-sm font-medium mb-2">Distribution per Node:</p>
                <div className="grid grid-cols-3 gap-2 text-sm">
                  {stressTestStats.map((stat) => {
                    const total = stressTestStats.reduce((sum, s) => sum + s.requestsSent, 0);
                    const percentage = total > 0 ? ((stat.requestsSent / total) * 100).toFixed(1) : '0';
                    return (
                      <div key={stat.nodeId} className="p-2 bg-white rounded border">
                        <span className="font-medium">Node {stat.nodeId}:</span>
                        <span className="ml-2">{stat.requestsSent} ({percentage}%)</span>
                      </div>
                    );
                  })}
                </div>
              </div>
            </div>

            {/* Results Table */}
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
        
        {/* Failure Events Log */}
        {failureEvents.length > 0 && (
          <div className="mt-6 border rounded p-4 bg-amber-50">
            <h3 className="font-semibold mb-3 text-amber-900">🔧 Failure Testing Events</h3>
            
            {/* Current Status Summary */}
            <div className="mb-3 p-3 bg-white rounded border">
              <div className="text-sm font-medium mb-2">Current Node Status:</div>
              <div className="flex gap-2">
                {[1, 2, 3].map(nodeId => {
                  // Find last event for this node
                  const lastEvent = [...failureEvents]
                    .reverse()
                    .find(e => e.node === nodeId);
                  const isUp = !lastEvent || lastEvent.event === 'STARTED';
                  
                  return (
                    <div 
                      key={nodeId}
                      className={`px-3 py-1 rounded text-sm font-medium ${
                        isUp 
                          ? 'bg-green-100 text-green-800' 
                          : 'bg-red-100 text-red-800'
                      }`}
                    >
                      Node {nodeId}: {isUp ? '✅ UP' : '💥 DOWN'}
                    </div>
                  );
                })}
              </div>
            </div>
            
            {/* Events Timeline */}
            <div className="space-y-1 max-h-60 overflow-y-auto">
              {failureEvents.map((event, idx) => {
                const time = new Date(event.time).toLocaleTimeString();
                const isFailure = event.event === 'STOPPED';
                return (
                  <div 
                    key={idx} 
                    className={`text-sm p-2 rounded ${isFailure ? 'bg-red-100 text-red-800' : 'bg-green-100 text-green-800'}`}
                  >
                    <span className="font-mono">[{time}]</span>
                    {' '}
                    <span className="font-semibold">Request #{event.requestNum}</span>
                    {' → '}
                    {isFailure ? '💥' : '🔄'} Node {event.node} {event.event}
                  </div>
                );
              })}
            </div>
            <p className="text-xs text-amber-700 mt-3">
              Note: These are simulated failures for testing purposes. 
              The system ensures at least 1 node is always available to maintain service continuity.
              In production, Raft consensus handles real failures automatically.
            </p>
          </div>
        )}
      </div>
    </div>
  );
}
