use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::RwLock;
use openraft::network::{RaftNetwork, RaftNetworkFactory};
use openraft::error::{NetworkError, RPCError};
use openraft_memstore::TypeConfig;

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

    /// Send an RPC message and receive response
    async fn send_rpc(
        &self,
        msg: crate::rpc::RaftMessage,
    ) -> Result<crate::rpc::RpcResponse, String> {
        // Connect to target node
        let mut stream = TcpStream::connect(&self.target_addr)
            .await
            .map_err(|e| format!("Failed to connect to {}: {}", self.target_addr, e))?;

        // Serialize and send message with length prefix
        let msg_bytes = crate::rpc::serialize_message(&msg);
        let len = (msg_bytes.len() as u32).to_be_bytes();
        stream
            .write_all(&len)
            .await
            .map_err(|e| format!("Failed to write length: {}", e))?;
        stream
            .write_all(&msg_bytes)
            .await
            .map_err(|e| format!("Failed to write message: {}", e))?;
        stream.flush().await.map_err(|e| format!("Flush failed: {}", e))?;

        // Read response
        let mut len_bytes = [0u8; 4];
        stream
            .read_exact(&mut len_bytes)
            .await
            .map_err(|e| format!("Failed to read response length: {}", e))?;

        let resp_len = u32::from_be_bytes(len_bytes) as usize;
        let mut buffer = vec![0u8; resp_len];
        stream
            .read_exact(&mut buffer)
            .await
            .map_err(|e| format!("Failed to read response: {}", e))?;

        crate::rpc::deserialize_response(&buffer)
    }
}

/// Implement RaftNetwork for sending RPCs to a target node
impl RaftNetwork<TypeConfig> for NetworkClient {
    async fn append_entries(
        &mut self,
        req: openraft::raft::AppendEntriesRequest<TypeConfig>,
        _option: openraft::network::RPCOption,
    ) -> Result<
        openraft::raft::AppendEntriesResponse<NodeId>,
        RPCError<NodeId, (), openraft::error::RaftError<NodeId>>,
    > {
        println!(
            "📤 Sending append_entries to node {} at {}",
            self.target_node_id, self.target_addr
        );

        match self
            .send_rpc(crate::rpc::RaftMessage::AppendEntries(req))
            .await
        {
            Ok(crate::rpc::RpcResponse::AppendEntries(resp)) => Ok(resp),
            Ok(_) => Err(RPCError::Network(NetworkError::new(&std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Wrong response type",
            )))),
            Err(e) => Err(RPCError::Network(NetworkError::new(&std::io::Error::new(
                std::io::ErrorKind::Other,
                e,
            )))),
        }
    }

    async fn vote(
        &mut self,
        req: openraft::raft::VoteRequest<NodeId>,
        _option: openraft::network::RPCOption,
    ) -> Result<
        openraft::raft::VoteResponse<NodeId>,
        RPCError<NodeId, (), openraft::error::RaftError<NodeId>>,
    > {
        println!(
            "📤 Sending vote request to node {} at {}",
            self.target_node_id, self.target_addr
        );

        match self.send_rpc(crate::rpc::RaftMessage::Vote(req)).await {
            Ok(crate::rpc::RpcResponse::Vote(resp)) => Ok(resp),
            Ok(_) => Err(RPCError::Network(NetworkError::new(&std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Wrong response type",
            )))),
            Err(e) => Err(RPCError::Network(NetworkError::new(&std::io::Error::new(
                std::io::ErrorKind::Other,
                e,
            )))),
        }
    }

    async fn install_snapshot(
        &mut self,
        req: openraft::raft::InstallSnapshotRequest<TypeConfig>,
        _option: openraft::network::RPCOption,
    ) -> Result<
        openraft::raft::InstallSnapshotResponse<NodeId>,
        RPCError<NodeId, (), openraft::error::RaftError<NodeId, openraft::error::InstallSnapshotError>>,
    > {
        println!(
            "📤 Sending install_snapshot to node {} at {}",
            self.target_node_id, self.target_addr
        );

        match self
            .send_rpc(crate::rpc::RaftMessage::InstallSnapshot(req))
            .await
        {
            Ok(crate::rpc::RpcResponse::InstallSnapshot(resp)) => Ok(resp),
            Ok(_) => Err(RPCError::Network(NetworkError::new(&std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Wrong response type",
            )))),
            Err(e) => Err(RPCError::Network(NetworkError::new(&std::io::Error::new(
                std::io::ErrorKind::Other,
                e,
            )))),
        }
    }
}

/// Factory for creating network clients
pub struct NetworkFactory {
    peer_addresses: Arc<RwLock<HashMap<NodeId, String>>>,
}

impl NetworkFactory {
    pub fn new() -> Self {
        Self {
            peer_addresses: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn add_peer(&self, node_id: NodeId, addr: String) {
        let mut peers = self.peer_addresses.write().await;
        peers.insert(node_id, addr.clone());
        println!("📋 Registered peer: node {} -> {}", node_id, addr);
    }
}

impl RaftNetworkFactory<TypeConfig> for NetworkFactory {
    type Network = NetworkClient;

    async fn new_client(&mut self, target: NodeId, _node: &()) -> Self::Network {
        let peers = self.peer_addresses.read().await;
        let addr = peers
            .get(&target)
            .cloned()
            .unwrap_or_else(|| format!("unknown-{}", target));

        println!("🔌 Creating network client for node {} at {}", target, addr);
        NetworkClient::new(target, addr)
    }
}
