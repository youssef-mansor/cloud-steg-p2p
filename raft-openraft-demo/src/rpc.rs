use openraft::raft::{AppendEntriesRequest, AppendEntriesResponse, VoteRequest, VoteResponse, InstallSnapshotRequest, InstallSnapshotResponse};
use openraft_memstore::TypeConfig;
use serde::{Deserialize, Serialize};

pub type NodeId = u64;

/// RPC Message wrapper - wraps all Raft RPC types
#[derive(Debug, Serialize, Deserialize)]
pub enum RaftMessage {
    AppendEntries(AppendEntriesRequest<TypeConfig>),
    AppendEntriesResponse(AppendEntriesResponse<NodeId>),
    Vote(VoteRequest<NodeId>),
    VoteResponse(VoteResponse<NodeId>),
    InstallSnapshot(InstallSnapshotRequest<TypeConfig>),
    InstallSnapshotResponse(InstallSnapshotResponse<NodeId>),
}

/// Response wrapper
#[derive(Debug, Serialize, Deserialize)]
pub enum RpcResponse {
    AppendEntries(AppendEntriesResponse<NodeId>),
    Vote(VoteResponse<NodeId>),
    InstallSnapshot(InstallSnapshotResponse<NodeId>),
}

/// Serialize a message to bytes
pub fn serialize_message(msg: &RaftMessage) -> Vec<u8> {
    bincode::serialize(msg).expect("Failed to serialize RPC message")
}

/// Deserialize bytes to a message
pub fn deserialize_message(data: &[u8]) -> Result<RaftMessage, String> {
    bincode::deserialize(data).map_err(|e| format!("Failed to deserialize: {}", e))
}

/// Serialize a response
pub fn serialize_response(resp: &RpcResponse) -> Vec<u8> {
    bincode::serialize(resp).expect("Failed to serialize RPC response")
}

/// Deserialize a response
pub fn deserialize_response(data: &[u8]) -> Result<RpcResponse, String> {
    bincode::deserialize(data).map_err(|e| format!("Failed to deserialize response: {}", e))
}
