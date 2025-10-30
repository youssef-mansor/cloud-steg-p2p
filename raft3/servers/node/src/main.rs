use proto::{
    rpc::{read_message, send_message},
    state::NodeState,
    *,
};
use tokio::{net::{TcpListener, TcpStream}, task};
use clap::Parser;
use tokio::sync::Mutex;
use std::sync::Arc;

/// Command-line arguments
#[derive(Parser, Debug)]
struct Args {
    /// Node ID (unique)
    #[arg(long)]
    id: NodeId,

    /// Listening port, e.g. 7000
    #[arg(long)]
    port: u16,

    /// Comma-separated peer addresses, e.g. "10.40.56.135:7001,10.40.56.136:7002"
    #[arg(long)]
    peers: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let listen_addr = format!("0.0.0.0:{}", args.port);
    println!("Node {} listening on {}", args.id, listen_addr);

    let state = Arc::new(Mutex::new(NodeState::new()));

    let state_listener = state.clone();
    let listener_task = task::spawn(async move {
        let listener = TcpListener::bind(&listen_addr).await.unwrap();
        loop {
            let (mut socket, peer) = listener.accept().await.unwrap();
            println!("Accepted connection from {}", peer);

            let msg = read_message(&mut socket).await.unwrap();
            println!("Received: {:?}", msg);

            if let RpcMessage::RequestVote(req) = msg {
                let mut st = state_listener.lock().await; // ✅ async lock
                let resp = st.handle_request_vote(&req);
                let reply = RpcMessage::RequestVoteResponse(resp.clone());
                send_message(&mut socket, &reply).await.unwrap();
                println!("Sent response: {:?}", reply);
            }
        }
    });


    // If peers are provided, send RequestVote to them
    if let Some(peer_str) = args.peers {
        let peers: Vec<_> = peer_str.split(',').map(|s| s.trim().to_string()).collect();
        for target in peers {
            if target.is_empty() {
                continue;
            }
            println!("Connecting to {}", target);
            match TcpStream::connect(&target).await {
                Ok(mut stream) => {
                    let msg = RpcMessage::RequestVote(RequestVote {
                        term: 1,
                        candidate_id: args.id,
                        last_log_index: 0,
                        last_log_term: 0,
                    });
                    send_message(&mut stream, &msg).await?;
                    println!("Sent RequestVote: {:?}", msg);

                    if let Ok(reply) = read_message(&mut stream).await {
                        println!("Received reply from {}: {:?}", target, reply);
                    }
                }
                Err(e) => println!("Failed to connect to {}: {}", target, e),
            }
        }
    }

    listener_task.await?;
    Ok(())
}
