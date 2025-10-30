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
use std::time::Instant;

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

impl NodeState {
    pub fn new() -> Self {
        Self {
            current_term: 0,
            voted_for: None,
            role: Role::Follower,
            votes_received: 0,
            peers_count: 0,
            last_heard: Instant::now(),
        }
    }
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
            if st.last_heard.elapsed().as_millis() < 150 {
                continue; // heard from leader/candidate recently
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

            let mut st = state_listener.lock().await;
            st.last_heard = Instant::now();

            if let RpcMessage::RequestVote(req) = msg {
                let resp = st.handle_request_vote(&req);
                let reply = RpcMessage::RequestVoteResponse(resp.clone());
                send_message(&mut socket, &reply).await.unwrap();
                println!("Sent response: {:?}", reply);
            } else if let RpcMessage::RequestVoteResponse(resp) = msg {
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

                        let peers_clone = peers.clone();
                        let state_clone = state.clone();
                        tokio::spawn(async move {
                            loop {
                                {
                                    let st = state_clone.lock().await;
                                    if !st.is_leader() {
                                        break;
                                    }
                                }
                                for target in &peers_clone {
                                    let mut stream = match TcpStream::connect(target).await {
                                        Ok(s) => s,
                                        Err(_) => continue,
                                    };
                                    let msg = RpcMessage::AppendEntries(AppendEntries {
                                        term: state_clone.lock().await.current_term,
                                        leader_id: args.id,
                                        prev_log_index: 0,
                                        prev_log_term: 0,
                                        entries: vec![],
                                        leader_commit: 0,
                                    });
                                    let _ = send_message(&mut stream, &msg).await;
                                }
                                sleep(Duration::from_millis(50)).await;
                            }
                        });
                    }
                }
            } else if let RpcMessage::AppendEntries(req) = msg {
                st.last_heard = Instant::now();
                if req.term >= st.current_term {
                    st.role = proto::state::Role::Follower;
                    st.current_term = req.term;
                }
                // Optionally: send AppendEntriesResponse
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