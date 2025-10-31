use crate::rpc::{RaftMessage, RpcResponse, serialize_response};
use openraft::{Raft, Vote};
use openraft_memstore::TypeConfig;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

pub type NodeId = u64;
pub type RaftNode = Raft<TypeConfig>;

/// Start the RPC server listening for incoming Raft messages
pub async fn start_rpc_server(
    listen_addr: String,
    raft: Arc<RaftNode>,
) -> anyhow::Result<()> {
    let listener = TcpListener::bind(&listen_addr).await?;
    println!("🔌 RPC server listening on {}", listen_addr);

    loop {
        let (mut socket, peer_addr) = listener.accept().await?;
        let raft = raft.clone();

        tokio::spawn(async move {
            if let Err(e) = handle_rpc_connection(&mut socket, raft).await {
                println!("❌ RPC error from {}: {}", peer_addr, e);
            }
        });
    }
}

/// Handle a single RPC connection
async fn handle_rpc_connection(
    socket: &mut tokio::net::TcpStream,
    raft: Arc<RaftNode>,
) -> anyhow::Result<()> {
    let mut buffer = vec![0u8; 65536]; // 64KB buffer

    loop {
        // Read message length (4 bytes, big-endian)
        let mut len_bytes = [0u8; 4];
        if socket.read_exact(&mut len_bytes).await? == 0 {
            return Ok(()); // Connection closed
        }

        let msg_len = u32::from_be_bytes(len_bytes) as usize;
        if msg_len > 65536 {
            return Err(anyhow::anyhow!("Message too large: {}", msg_len));
        }

        // Read message
        if socket.read_exact(&mut buffer[..msg_len]).await? == 0 {
            return Ok(());
        }

        // Deserialize and handle
        match crate::rpc::deserialize_message(&buffer[..msg_len]) {
            Ok(msg) => {
                println!("📨 Received RPC: {:?}", std::mem::discriminant(&msg));
                if let Some(response) = handle_message(msg, raft.clone()).await {
                    // Send response
                    let response_bytes = serialize_response(&response);
                    let len = (response_bytes.len() as u32).to_be_bytes();
                    socket.write_all(&len).await?;
                    socket.write_all(&response_bytes).await?;
                    socket.flush().await?;
                }
            }
            Err(e) => {
                println!("⚠️  Failed to deserialize RPC: {}", e);
            }
        }
    }
}

/// Handle an RPC message and return response
async fn handle_message(msg: RaftMessage, raft: Arc<RaftNode>) -> Option<RpcResponse> {
    match msg {
        RaftMessage::AppendEntries(req) => {
            let current_metrics = raft.metrics().borrow().clone();
            let req_term = req.vote.leader_id.term;
            let req_leader = req.vote.leader_id.node_id;
            
            // Critical: If we're a leader but receive AppendEntries from a higher term, we must step down
            if matches!(current_metrics.state, openraft::ServerState::Leader) && req_term > current_metrics.current_term {
                println!("🚨 CRITICAL: Leader node received AppendEntries from higher term {} (our term: {}), must step down!", 
                         req_term, current_metrics.current_term);
            }
            
            println!("📥 AppendEntries: term {} from leader {}, current_term={}, state={:?}", 
                     req_term, req_leader, current_metrics.current_term, current_metrics.state);
            
            match raft.append_entries(req).await {
                Ok(resp) => {
                    let new_metrics = raft.metrics().borrow().clone();
                    if resp.is_success() {
                        println!("✅ AppendEntries succeeded");
                    } else {
                        println!("⚠️  AppendEntries rejected (likely log mismatch or wrong term)");
                    }
                    
                    // Log if state changed after processing AppendEntries
                    if new_metrics.state != current_metrics.state {
                        println!("🔄 State changed after AppendEntries: {:?} -> {:?}", 
                                current_metrics.state, new_metrics.state);
                    }
                    
                    Some(crate::rpc::RpcResponse::AppendEntries(resp))
                },
                Err(e) => {
                    println!("❌ AppendEntries error: {} - Cannot send proper response, this may cause timeout", e);
                    // Unfortunately, we cannot construct AppendEntriesResponse manually as it's an internal type
                    // The error/panic means OpenRaft couldn't process it, so we return None
                    // This will cause the leader to timeout, but it's better than crashing
                    // The leader will eventually detect the node is unresponsive
                    None
                }
            }
        }

        RaftMessage::Vote(req) => {
            let current_metrics = raft.metrics().borrow().clone();
            let req_term = req.vote.leader_id.term;
            let req_node = req.vote.leader_id.node_id;
            
            // Critical: If we're a leader but receive Vote request from a higher term, we must step down
            if matches!(current_metrics.state, openraft::ServerState::Leader) && req_term > current_metrics.current_term {
                println!("🚨 CRITICAL: Leader node received Vote request from higher term {} (our term: {}), must step down!", 
                         req_term, current_metrics.current_term);
            }
            
            println!("📥 Vote request: term {} from node {}, current_term={}, current_leader={:?}, state={:?}", 
                     req_term, req_node, current_metrics.current_term, current_metrics.current_leader, current_metrics.state);
            match raft.vote(req).await {
                Ok(resp) => {
                    let new_metrics = raft.metrics().borrow().clone();
                    println!("✅ Vote response: granted={}", resp.vote_granted);
                    
                    // Log if state changed after voting
                    if new_metrics.state != current_metrics.state {
                        println!("🔄 State changed after Vote: {:?} -> {:?}", 
                                current_metrics.state, new_metrics.state);
                    }
                    
                    // Log if term changed
                    if new_metrics.current_term != current_metrics.current_term {
                        println!("📈 Term updated after Vote: {} -> {}", 
                                current_metrics.current_term, new_metrics.current_term);
                    }
                    
                    Some(crate::rpc::RpcResponse::Vote(resp))
                },
                Err(e) => {
                    println!("❌ Vote error: {}", e);
                    None
                }
            }
        }

        RaftMessage::InstallSnapshot(req) => {
            println!("📥 InstallSnapshot request");
            match raft.install_snapshot(req).await {
                Ok(resp) => Some(crate::rpc::RpcResponse::InstallSnapshot(resp)),
                Err(e) => {
                    println!("❌ InstallSnapshot error: {}", e);
                    None
                }
            }
        }

        _ => {
            println!("⚠️  Unexpected message type");
            None
        }
    }
}
