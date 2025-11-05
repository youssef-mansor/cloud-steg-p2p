import type { ApiResponse, NodeMetrics } from '../types';

// Default server addresses - can be configured
// Distributed: Node1/2/3 on provided hosts
const DEFAULT_NODES = [
  { id: 1, httpAddr: 'http://10.40.56.135:8001' },
  { id: 2, httpAddr: 'http://10.40.42.221:8002' },
  { id: 3, httpAddr: 'http://10.40.44.75:8003' },
];

export class RaftApiClient {
  private nodes: { id: number; httpAddr: string }[];

  constructor(nodes?: { id: number; httpAddr: string }[]) {
    this.nodes = nodes || DEFAULT_NODES;
  }

  // Update node addresses dynamically
  updateNodes(nodes: { id: number; httpAddr: string }[]) {
    this.nodes = nodes;
  }

  // Get metrics from a specific node with timeout
  async getMetrics(nodeId: number): Promise<ApiResponse<NodeMetrics>> {
    const node = this.nodes.find((n) => n.id === nodeId);
    if (!node) {
      return { success: false, error: `Node ${nodeId} not found` };
    }

    try {
      const controller = new AbortController();
      const timeoutId = setTimeout(() => controller.abort(), 5000); // 5 second timeout
      
      const response = await fetch(`${node.httpAddr}/metrics`, {
        method: 'GET',
        headers: { 'Content-Type': 'application/json' },
        signal: controller.signal,
      });

      clearTimeout(timeoutId);

      if (!response.ok) {
        return { success: false, error: `HTTP ${response.status}` };
      }

      const data = await response.json();
      return data;
    } catch (error) {
      return { success: false, error: String(error) };
    }
  }

  // Get throughput metrics from a specific node (typically the leader, which tracks all nodes)
  async getThroughput(nodeId: number): Promise<{ 
    success: boolean; 
    data?: { 
      reporting_node_id: number; 
      node_throughputs: Record<string, number>; 
      timestamp: number 
    }; 
    error?: string 
  }> {
    const node = this.nodes.find((n) => n.id === nodeId);
    if (!node) {
      return { success: false, error: `Node ${nodeId} not found` };
    }

    try {
      const response = await fetch(`${node.httpAddr}/metrics/throughput`, {
        method: 'GET',
        headers: { 'Content-Type': 'application/json' },
      });

      if (!response.ok) {
        return { success: false, error: `HTTP ${response.status}` };
      }

      const data = await response.json();
      return data;
    } catch (error) {
      return { success: false, error: String(error) };
    }
  }

  // Get metrics from all nodes with timeout
  async getAllMetrics(): Promise<Map<number, ApiResponse<NodeMetrics>>> {
    const results = new Map<number, ApiResponse<NodeMetrics>>();
    
    // Use Promise.allSettled to ensure all requests complete, even if some fail
    // Add a timeout to individual requests to prevent hanging
    const metricsPromises = this.nodes.map(async (node) => {
      try {
        const controller = new AbortController();
        const timeoutId = setTimeout(() => controller.abort(), 5000); // 5 second timeout per node
        
        const response = await fetch(`${node.httpAddr}/metrics`, {
          method: 'GET',
          headers: { 'Content-Type': 'application/json' },
          signal: controller.signal,
        });
        
        clearTimeout(timeoutId);
        
        if (!response.ok) {
          results.set(node.id, { success: false, error: `HTTP ${response.status}` });
        } else {
          const data = await response.json();
          results.set(node.id, data);
        }
      } catch (error) {
        results.set(node.id, { success: false, error: String(error) });
      }
    });

    await Promise.allSettled(metricsPromises);
    return results;
  }

  // Upload two images (cover + secret) for steganography encryption
  async uploadImageForEncryption(
    nodeId: number,
    coverImage: File,
    secretImage: File
  ): Promise<{ success: boolean; data?: Blob; error?: string; latency: number; processedBy?: number }> {
    const node = this.nodes.find((n) => n.id === nodeId);
    if (!node) {
      return { success: false, error: `Node ${nodeId} not found`, latency: 0 };
    }

    const startTime = performance.now();

    try {
      // Create multipart form data with both images
      const formData = new FormData();
      formData.append('cover', coverImage);
      formData.append('secret', secretImage);
      
      console.log(`Uploading to node ${nodeId} (${node.httpAddr}/image/steg)`);
      console.log(`  - Cover image: ${coverImage.name} (${coverImage.size} bytes)`);
      console.log(`  - Secret image: ${secretImage.name} (${secretImage.size} bytes)`);
      
      const controller = new AbortController();
      const timeoutId = setTimeout(() => controller.abort(), 30000); // 30 second timeout for image upload
      
      const response = await fetch(`${node.httpAddr}/image/steg`, {
        method: 'POST',
        body: formData,
        signal: controller.signal,
      });

      clearTimeout(timeoutId);
      const latency = performance.now() - startTime;

      if (!response.ok) {
        const errorText = await response.text();
        console.error(`Node ${nodeId} returned HTTP ${response.status}: ${errorText}`);
        return { success: false, error: `HTTP ${response.status}: ${errorText}`, latency };
      }

      // Response is the stego image (PNG binary)
      const arrayBuffer = await response.arrayBuffer();
      const blob = new Blob([arrayBuffer], { type: 'image/png' });

      // Read X-Processed-By-Node header to track which node actually processed the request
      const processedByHeader = response.headers.get('X-Processed-By-Node');
      const processedBy = processedByHeader ? parseInt(processedByHeader, 10) : nodeId;

      console.log(`Node ${nodeId} succeeded: processed by node ${processedBy}, received ${blob.size} bytes, latency: ${latency.toFixed(0)}ms`);
      return { success: true, data: blob, latency, processedBy };
    } catch (error) {
      const latency = performance.now() - startTime;
      console.error(`Node ${nodeId} error:`, error);
      return { success: false, error: String(error), latency };
    }
  }

  // Find the actual leader node
  private async findLeader(): Promise<number | null> {
    console.log('🔍 Detecting leader...');
    
    // Try each node to find who reports as the leader
    for (const node of this.nodes) {
      try {
        const metricsResult = await this.getMetrics(node.id);
        if (metricsResult.success && metricsResult.data) {
          const reportedLeaderId = metricsResult.data.current_leader;
          const isLeader = metricsResult.data.state === 'Leader';
          
          console.log(`  Node ${node.id}: state=${metricsResult.data.state}, reports leader=${reportedLeaderId}`);
          
          // If this node reports itself as leader, verify it
          if (isLeader && reportedLeaderId === node.id) {
            console.log(`✅ Confirmed leader: Node ${node.id}`);
            return node.id;
          }
        }
      } catch (error) {
        console.log(`  Node ${node.id}: unreachable`);
        continue;
      }
    }
    
    console.log('⚠️  Could not detect leader from any node');
    return null;
  }

  // Send two images to any available node (load balanced)
  async uploadImageToCluster(coverImage: File, secretImage: File): Promise<{
    success: boolean;
    data?: Blob;
    error?: string;
    nodeId?: number;
    processedBy?: number;
    latency: number;
  }> {
    // Find the leader
    const leaderId = await this.findLeader();

    if (leaderId) {
      console.log(`📤 Sending encryption request to leader Node ${leaderId}`);
      const result = await this.uploadImageForEncryption(leaderId, coverImage, secretImage);
      if (result.success) {
        return { ...result, nodeId: leaderId };
      }
      console.error(`❌ Leader Node ${leaderId} failed, trying other nodes...`);
    }

    // Fallback: try all nodes if leader is down or detection failed
    console.log('⚠️  Trying all nodes as fallback...');
    for (const node of this.nodes) {
      if (node.id === leaderId) continue; // Skip leader if we already tried it
      const result = await this.uploadImageForEncryption(node.id, coverImage, secretImage);
      if (result.success) {
        return { ...result, nodeId: node.id };
      }
    }

    return {
      success: false,
      error: 'All nodes failed',
      latency: 0,
    };
  }

  // Extract/Decrypt image from stego image - send to leader for load balancing (no key needed)
  async decryptImageFromCluster(stegoFile: File): Promise<{
    success: boolean;
    data?: Blob;
    error?: string;
    nodeId?: number;
    processedBy?: number;
    latency: number;
  }> {
    const startTime = performance.now();

    try {
      const arrayBuffer = await stegoFile.arrayBuffer();
      
      // Find the leader
      const leaderId = await this.findLeader();

      if (leaderId) {
        console.log(`📤 Sending extraction request to leader Node ${leaderId}`);
        const leaderNode = this.nodes.find((n) => n.id === leaderId);
        if (leaderNode) {
          const nodeStartTime = performance.now();
          const nodeController = new AbortController();
          const nodeTimeoutId = setTimeout(() => nodeController.abort(), 30000); // 30 second timeout
          
          try {
            const response = await fetch(
              `${leaderNode.httpAddr}/image/extract`,
              {
                method: 'POST',
                headers: { 'Content-Type': 'application/octet-stream' },
                body: arrayBuffer,
                signal: nodeController.signal,
              }
            );

            clearTimeout(nodeTimeoutId);
            const nodeLatency = performance.now() - nodeStartTime;

            if (response.ok) {
              const processedByHeader = response.headers.get('X-Processed-By-Node');
              const processedBy = processedByHeader ? parseInt(processedByHeader, 10) : leaderId;

              const arrayBufferResponse = await response.arrayBuffer();
              const blob = new Blob([arrayBufferResponse], { type: 'image/png' });
              console.log(`✅ Leader Node ${leaderId} succeeded: processed by node ${processedBy}, received ${blob.size} bytes, latency: ${nodeLatency.toFixed(0)}ms`);
              
              const totalLatency = performance.now() - startTime;
              return { 
                success: true, 
                data: blob, 
                nodeId: leaderId,
                processedBy, 
                latency: totalLatency 
              };
            }
          } catch (error) {
            clearTimeout(nodeTimeoutId);
            console.error(`❌ Leader Node ${leaderId} failed:`, error);
          }
        }
      }

      // Fallback: try all nodes if leader detection failed or leader is down
      console.log('⚠️  Leader detection failed or leader is down, trying all nodes');
      
      const results = await Promise.allSettled(
        this.nodes.map(async (node) => {
          console.log(`Attempting extraction on node ${node.id} (${node.httpAddr}/image/extract), file size: ${arrayBuffer.byteLength} bytes`);
          
          const nodeStartTime = performance.now();
          const nodeController = new AbortController();
          const nodeTimeoutId = setTimeout(() => nodeController.abort(), 30000); // 30 second timeout per node
          
          try {
            const response = await fetch(
              `${node.httpAddr}/image/extract`,
              {
                method: 'POST',
                headers: { 'Content-Type': 'application/octet-stream' },
                body: arrayBuffer,
                signal: nodeController.signal,
              }
            );

            clearTimeout(nodeTimeoutId);
            const nodeLatency = performance.now() - nodeStartTime;

            if (!response.ok) {
              const errorText = await response.text();
              console.error(`Node ${node.id} returned HTTP ${response.status}: ${errorText}`);
              throw new Error(`HTTP ${response.status}: ${errorText}`);
            }

            const processedByHeader = response.headers.get('X-Processed-By-Node');
            const processedBy = processedByHeader ? parseInt(processedByHeader, 10) : node.id;

            // Get the raw bytes and create a blob with image/png type
            const arrayBufferResponse = await response.arrayBuffer();
            const blob = new Blob([arrayBufferResponse], { type: 'image/png' });
            console.log(`✅ Node ${node.id} succeeded: processed by node ${processedBy}, received ${blob.size} bytes, latency: ${nodeLatency.toFixed(0)}ms`);
            
            return { 
              success: true, 
              data: blob, 
              nodeId: node.id,
              processedBy, 
              latency: nodeLatency 
            };
          } catch (error) {
            clearTimeout(nodeTimeoutId);
            throw error;
          }
        })
      );

      // Find first successful result
      for (const result of results) {
        if (result.status === 'fulfilled' && result.value.success) {
          const totalLatency = performance.now() - startTime;
          return { 
            success: true, 
            data: result.value.data, 
            nodeId: result.value.nodeId,
            processedBy: result.value.processedBy,
            latency: totalLatency 
          };
        }
      }

      // All failed
      const totalLatency = performance.now() - startTime;
      const errors = results
        .map((r, i) => {
          if (r.status === 'rejected') {
            const errorMsg = r.reason?.message || String(r.reason);
            // Provide helpful error interpretation
            let friendlyError = errorMsg;
            if (errorMsg.includes('ERR_CONNECTION_ABORTED') || errorMsg.includes('signal')) {
              friendlyError = 'Request timeout (took too long to process)';
            } else if (errorMsg.includes('ERR_CONNECTION_RESET')) {
              friendlyError = 'Server connection reset (server may have crashed)';
            } else if (errorMsg.includes('Failed to fetch')) {
              friendlyError = 'Network error or server not responding';
            }
            return `Node ${this.nodes[i].id}: ${friendlyError}`;
          } else if (r.status === 'fulfilled' && !r.value.success) {
            return `Node ${this.nodes[i].id}: Failed`;
          }
          return null;
        })
        .filter((e) => e !== null)
        .join('; ');

      console.error(`❌ All nodes failed: ${errors}`);
      return { 
        success: false, 
        error: `All nodes failed: ${errors}`, 
        latency: totalLatency 
      };
    } catch (error) {
      const latency = performance.now() - startTime;
      console.error('Extraction error:', error);
      const errorMsg = error instanceof Error ? error.message : String(error);
      let friendlyError = errorMsg;
      
      if (errorMsg.includes('AbortError')) {
        friendlyError = 'Request timeout: Large file took too long to process. Try with a smaller image or ensure servers have adequate resources.';
      } else if (errorMsg.includes('Failed to fetch')) {
        friendlyError = 'Network error: Cannot connect to servers. Make sure all 3 nodes are running.';
      }
      
      return { 
        success: false, 
        error: `Network error: ${friendlyError}`, 
        latency 
      };
    }
  }

  // Health check for a node
  async checkHealth(nodeId: number): Promise<boolean> {
    const node = this.nodes.find((n) => n.id === nodeId);
    if (!node) return false;

    try {
      const response = await fetch(`${node.httpAddr}/`, {
        method: 'GET',
        signal: AbortSignal.timeout(3000),
      });
      return response.ok;
    } catch {
      return false;
    }
  }

  getNodes() {
    return this.nodes;
  }
}

// Singleton instance
export const apiClient = new RaftApiClient();
