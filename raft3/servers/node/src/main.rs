use proto::{
    rpc::{read_message, send_message},
    state::NodeState,
    *,
};
use tokio::{net::{TcpListener, TcpStream}, task};
use clap::Parser;
use tokio::sync::Mutex;
use std::sync::Arc;
use rand::{thread_rng, Rng};
use tokio::time::{sleep, Duration};

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

async fn election_timer_task(
    id: NodeId,
    peers: Vec<String>,
    state: Arc<tokio::sync::Mutex<proto::state::NodeState>>,
) {
    loop {
        // Random timeout between 150–300 ms
        let timeout = thread_rng().gen_range(150..=300);
        sleep(Duration::from_millis(timeout)).await;

        // Trigger election
        {
            let mut st = state.lock().await;
            if !st.is_follower() {
                continue; // already candidate/leader, skip election
            }
            let new_term = st.start_election(id);
            println!(
                "[Node {}] Election timeout → starting election (term {})",
                id, new_term
            );
        }

        // Broadcast RequestVote to peers
        for target in &peers {
            let mut stream = loop {
                match TcpStream::connect(target).await {
                    Ok(s) => break s,
                    Err(e) => {
                        println!(
                            "[Node {}] connect to {} failed: {}. Retrying in 200ms...",
                            id, target, e
                        );
                        sleep(Duration::from_millis(200)).await;
                    }
                }
            };

            let msg = RpcMessage::RequestVote(RequestVote {
                term: state.lock().await.current_term,
                candidate_id: id,
                last_log_index: 0,
                last_log_term: 0,
            });

            if let Err(e) = send_message(&mut stream, &msg).await {
                println!("[Node {}] send to {} failed: {}. Skipping this peer.", id, target, e);
                continue;
            }

            println!("[Node {}] sent RequestVote → {}", id, target);
        }
    } // <-- added closing brace for loop
} // <-- added closing brace for function

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let state = Arc::new(Mutex::new(NodeState::new()));
    // Set peers_count in NodeState
    if let Some(ref peer_str) = args.peers {
        let peers: Vec<_> = peer_str.split(',').map(|s| s.trim().to_string()).collect();
        {
            let mut st = state.lock().await;
            st.peers_count = peers.len();
        }
    }

    let listen_addr = format!("0.0.0.0:{}", args.port);
    println!("Node {} listening on {}", args.id, listen_addr);

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
            } else if let RpcMessage::RequestVoteResponse(resp) = msg {
                let mut st = state_listener.lock().await;

                if st.role == proto::state::Role::Candidate && resp.vote_granted {
                    st.votes_received += 1;
                    let total_nodes = st.peers_count + 1;
                    let majority = (total_nodes / 2) + 1;

                    if st.votes_received >= majority {
                        st.role = proto::state::Role::Leader;
                        println!(
                            "[Node {}] became LEADER for term {} (votes: {})",
                            st.voted_for.unwrap_or(0),
                            st.current_term,
                            st.votes_received
                        );
                    }
                }
            }
        }
    });

    // If peers are provided, send RequestVote to them
    if let Some(ref peer_str) = args.peers {
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

    // Spawn election timer if peers are defined
    if let Some(ref peer_str) = args.peers {
        let peers: Vec<_> = peer_str.split(',').map(|s| s.trim().to_string()).collect();
        let state_clone = state.clone();
        tokio::spawn(election_timer_task(args.id, peers, state_clone));
    }

    listener_task.await?;
    Ok(())
}