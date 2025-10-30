use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use openraft::network::{RaftNetwork, RaftNetworkFactory};
use openraft::error::{NetworkError, RPCError};
use openraft_memstore::TypeConfig;
use anyhow::Result;

pub type NodeId = u64;

/// Network client for communicating with a specific node
#[derive(Clone)]
pub struct NetworkClient {
    target_node_id: NodeId,
    target_addr: String,
}

impl NetworkClient {
    pub fn new(target_node_id: NodeId, target_addr: String) -> Self {
        Self {
            target_node_id,
            target_addr,
        }
    }
}

/// Implement RaftNetwork for sending RPCs to a target node
impl RaftNetwork<TypeConfig> for NetworkClient {
    async fn append_entries(
        &mut self,
        _req: openraft::raft::AppendEntriesRequest<TypeConfig>,
        _option: openraft::network::RPCOption,
    ) -> Result<
        openraft::raft::AppendEntriesResponse<NodeId>,
        RPCError<NodeId, (), openraft::error::RaftError<NodeId>>,
    > {
        println!("📤 Sending append_entries to node {} at {}", self.target_node_id, self.target_addr);
        
        // TODO: Implement actual HTTP/gRPC call to target node
        // For now, return a placeholder error
        Err(RPCError::Network(NetworkError::new(&std::io::Error::new(
            std::io::ErrorKind::NotConnected,
            "Not implemented yet"
        ))))
    }

    async fn vote(
        &mut self,
        _req: openraft::raft::VoteRequest<NodeId>,
        _option: openraft::network::RPCOption,
    ) -> Result<
        openraft::raft::VoteResponse<NodeId>,
        RPCError<NodeId, (), openraft::error::RaftError<NodeId>>,
    > {
        println!("📤 Sending vote request to node {} at {}", self.target_node_id, self.target_addr);
        
        // TODO: Implement actual HTTP/gRPC call
        Err(RPCError::Network(NetworkError::new(&std::io::Error::new(
            std::io::ErrorKind::NotConnected,
            "Not implemented yet"
        ))))
    }

    async fn install_snapshot(
        &mut self,
        _req: openraft::raft::InstallSnapshotRequest<TypeConfig>,
        _option: openraft::network::RPCOption,
    ) -> Result<
        openraft::raft::InstallSnapshotResponse<NodeId>,
        RPCError<NodeId, (), openraft::error::RaftError<NodeId, openraft::error::InstallSnapshotError>>,
    > {
        println!("📤 Sending install_snapshot to node {} at {}", self.target_node_id, self.target_addr);
        
        // TODO: Implement actual HTTP/gRPC call
        Err(RPCError::Network(NetworkError::new(&std::io::Error::new(
            std::io::ErrorKind::NotConnected,
            "Not implemented yet"
        ))))
    }
}

/// Factory for creating network clients
pub struct NetworkFactory {
    /// Map of node_id to network address
    peer_addresses: Arc<RwLock<HashMap<NodeId, String>>>,
}

impl NetworkFactory {
    pub fn new() -> Self {
        Self {
            peer_addresses: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Add or update a peer's address
    pub async fn add_peer(&self, node_id: NodeId, addr: String) {
        let mut peers = self.peer_addresses.write().await;
        peers.insert(node_id, addr.clone());
        println!("📋 Registered peer: node {} -> {}", node_id, addr);
    }
}

/// Implement RaftNetworkFactory to create clients for target nodes
impl RaftNetworkFactory<TypeConfig> for NetworkFactory {
    type Network = NetworkClient;

    async fn new_client(&mut self, target: NodeId, _node: &()) -> Self::Network {
        let peers = self.peer_addresses.read().await;
        let addr = peers.get(&target)
            .cloned()
            .unwrap_or_else(|| format!("unknown-{}", target));
        
        println!("🔌 Creating network client for node {} at {}", target, addr);
        NetworkClient::new(target, addr)
    }
}
