import { useState, useEffect, useCallback } from 'react';
import { apiClient } from '../api/client';
import type { NodeStatus } from '../types';

export function useClusterMetrics(pollingInterval = 2000) {
  const [nodeStatuses, setNodeStatuses] = useState<NodeStatus[]>([]);
  const [isPolling, setIsPolling] = useState(true);
  const [lastUpdate, setLastUpdate] = useState<Date | null>(null);

  const fetchMetrics = useCallback(async () => {
    const allMetrics = await apiClient.getAllMetrics();
    const statuses: NodeStatus[] = [];

    for (const [nodeId, response] of allMetrics.entries()) {
      const node = apiClient.getNodes().find((n) => n.id === nodeId);
      if (!node) continue;

      if (response.success && response.data) {
        statuses.push({
          nodeId,
          isLeader: response.data.state === 'Leader',
          isOnline: true,
          term: response.data.current_term,
          httpAddr: node.httpAddr,
          state: response.data.state,
        });
      } else {
        statuses.push({
          nodeId,
          isLeader: false,
          isOnline: false,
          term: 0,
          httpAddr: node.httpAddr,
          state: 'Offline',
        });
      }
    }

    setNodeStatuses(statuses);
    setLastUpdate(new Date());
  }, []);

  useEffect(() => {
    if (!isPolling) return;

    fetchMetrics();
    const interval = setInterval(fetchMetrics, pollingInterval);

    return () => clearInterval(interval);
  }, [isPolling, pollingInterval, fetchMetrics]);

  return {
    nodeStatuses,
    lastUpdate,
    isPolling,
    setIsPolling,
    refresh: fetchMetrics,
  };
}
