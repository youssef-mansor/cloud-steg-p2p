use crate::rpc::{RaftMessage, RpcResponse, serialize_response};
use openraft::Raft;
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
            println!("📥 AppendEntries from term {}", req.vote.leader_id.term);
            match raft.append_entries(req).await {
                Ok(resp) => Some(crate::rpc::RpcResponse::AppendEntries(resp)),
                Err(e) => {
                    println!("❌ AppendEntries error: {}", e);
                    None
                }
            }
        }

        RaftMessage::Vote(req) => {
            println!("📥 Vote request from leader term {}", req.vote.leader_id.term);
            match raft.vote(req).await {
                Ok(resp) => Some(crate::rpc::RpcResponse::Vote(resp)),
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
