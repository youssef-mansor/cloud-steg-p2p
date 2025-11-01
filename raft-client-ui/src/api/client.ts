import type { ApiResponse, NodeMetrics } from '../types';

// Default server addresses - can be configured
const DEFAULT_NODES = [
  { id: 1, httpAddr: 'http://localhost:8001' },
  { id: 2, httpAddr: 'http://localhost:8002' },
  { id: 3, httpAddr: 'http://localhost:8003' },
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

  // Get metrics from a specific node
  async getMetrics(nodeId: number): Promise<ApiResponse<NodeMetrics>> {
    const node = this.nodes.find((n) => n.id === nodeId);
    if (!node) {
      return { success: false, error: `Node ${nodeId} not found` };
    }

    try {
      const response = await fetch(`${node.httpAddr}/metrics`, {
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

  // Get metrics from all nodes
  async getAllMetrics(): Promise<Map<number, ApiResponse<NodeMetrics>>> {
    const results = new Map<number, ApiResponse<NodeMetrics>>();
    
    await Promise.all(
      this.nodes.map(async (node) => {
        const metrics = await this.getMetrics(node.id);
        results.set(node.id, metrics);
      })
    );

    return results;
  }

  // Upload image to a specific node for encryption (steganography)
  async uploadImageForEncryption(
    nodeId: number,
    imageFile: File
  ): Promise<{ success: boolean; data?: Blob; error?: string; latency: number; processedBy?: number }> {
    const node = this.nodes.find((n) => n.id === nodeId);
    if (!node) {
      return { success: false, error: `Node ${nodeId} not found`, latency: 0 };
    }

    const startTime = performance.now();

    try {
      const arrayBuffer = await imageFile.arrayBuffer();
      console.log(`Uploading to node ${nodeId} (${node.httpAddr}/image/steg), file size: ${arrayBuffer.byteLength} bytes`);
      
      const response = await fetch(`${node.httpAddr}/image/steg`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/octet-stream' },
        body: arrayBuffer,
      });

      const latency = performance.now() - startTime;

      if (!response.ok) {
        console.error(`Node ${nodeId} returned HTTP ${response.status}`);
        return { success: false, error: `HTTP ${response.status}`, latency };
      }

      // Read the X-Processed-By-Node header to see which node actually processed it
      const processedByHeader = response.headers.get('X-Processed-By-Node');
      const processedBy = processedByHeader ? parseInt(processedByHeader, 10) : nodeId;

      const blob = await response.blob();
      console.log(`Node ${nodeId} succeeded: processed by node ${processedBy}, received ${blob.size} bytes, latency: ${latency.toFixed(0)}ms`);
      return { success: true, data: blob, latency, processedBy };
    } catch (error) {
      const latency = performance.now() - startTime;
      console.error(`Node ${nodeId} error:`, error);
      return { success: false, error: String(error), latency };
    }
  }

  // Send image to any available node (load balanced)
  async uploadImageToCluster(imageFile: File): Promise<{
    success: boolean;
    data?: Blob;
    error?: string;
    nodeId?: number;
    processedBy?: number;
    latency: number;
  }> {
    // Try each node until one succeeds (finds the leader)
    for (const node of this.nodes) {
      const result = await this.uploadImageForEncryption(node.id, imageFile);
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

  // Decrypt/Extract image from stego image
  async decryptImageFromCluster(stegoFile: File): Promise<{
    success: boolean;
    data?: Blob;
    error?: string;
    nodeId?: number;
    processedBy?: number;
    latency: number;
  }> {
    const node = this.nodes[0];
    if (!node) {
      return { success: false, error: 'No nodes available', latency: 0 };
    }

    const startTime = performance.now();

    try {
      const arrayBuffer = await stegoFile.arrayBuffer();
      console.log(`Decrypting stego image (${node.httpAddr}/image/decrypt), file size: ${arrayBuffer.byteLength} bytes`);
      
      const response = await fetch(`${node.httpAddr}/image/decrypt`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/octet-stream' },
        body: arrayBuffer,
      });

      const latency = performance.now() - startTime;

      if (!response.ok) {
        console.error(`Decryption failed: HTTP ${response.status}`);
        return { success: false, error: `HTTP ${response.status}`, latency };
      }

      const processedByHeader = response.headers.get('X-Processed-By-Node');
      const processedBy = processedByHeader ? parseInt(processedByHeader, 10) : node.id;

      const blob = await response.blob();
      console.log(`Decryption succeeded: processed by node ${processedBy}, received ${blob.size} bytes, latency: ${latency.toFixed(0)}ms`);
      return { success: true, data: blob, latency, processedBy };
    } catch (error) {
      const latency = performance.now() - startTime;
      console.error('Decryption error:', error);
      return { success: false, error: String(error), latency };
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
