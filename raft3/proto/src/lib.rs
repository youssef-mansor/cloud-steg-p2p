use serde::{Deserialize, Serialize};

/// Raft term number.
pub type Term = u64;
/// Index within the replicated log.
pub type LogIndex = u64;
/// Unique node identifier.
pub type NodeId = u64;

/// A single entry in the replicated log.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LogEntry {
    pub term: Term,
    pub command: String,
}

/// RequestVote RPC
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RequestVote {
    pub term: Term,
    pub candidate_id: NodeId,
    pub last_log_index: LogIndex,
    pub last_log_term: Term,
}

/// RequestVote RPC reply
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RequestVoteResponse {
    pub term: Term,
    pub vote_granted: bool,
}

/// AppendEntries RPC (heartbeats + replication)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppendEntries {
    pub term: Term,
    pub leader_id: NodeId,
    pub prev_log_index: LogIndex,
    pub prev_log_term: Term,
    pub entries: Vec<LogEntry>,
    pub leader_commit: LogIndex,
}

/// AppendEntries RPC reply
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppendEntriesResponse {
    pub term: Term,
    pub success: bool,
}

/// Unified RPC message wrapper for network transport.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RpcMessage {
    RequestVote(RequestVote),
    RequestVoteResponse(RequestVoteResponse),
    AppendEntries(AppendEntries),
    AppendEntriesResponse(AppendEntriesResponse),
}

pub mod rpc;