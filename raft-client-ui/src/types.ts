// API Types
export interface NodeMetrics {
  node_id: number;
  state: string;
  current_term: number;
  current_leader: number | null;
  membership_config: any;
}

export interface ApiResponse<T> {
  success: boolean;
  data?: T;
  error?: string;
}

// Cluster State
export interface NodeStatus {
  nodeId: number;
  isLeader: boolean;
  isOnline: boolean;
  term: number;
  httpAddr: string;
  state: string;
}

// Performance Tracking
export interface NodeStats {
  nodeId: number;
  requestsSent: number;
  successCount: number;
  failureCount: number;
  averageLatency: number;
  requestsPerSecond: number;
  lastUpdated: number;
}

// Time-series data point for graphs
export interface TimeSeriesDataPoint {
  timestamp: number;
  node1: number;
  node2: number;
  node3: number;
  node1Latency?: number;
  node2Latency?: number;
  node3Latency?: number;
}

// Stress Test Configuration
export interface StressTestConfig {
  totalRequests: number;
  concurrentThreads: number;
  targetNodes: number[];
  endpoint: 'steg' | 'echo';
}

export interface StressTestResult {
  nodeId: number;
  success: boolean;
  latency: number;
  timestamp: number;
  error?: string;
}
