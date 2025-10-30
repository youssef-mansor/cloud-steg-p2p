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

async fn send_request_vote_once(
    target: &str,
    id: NodeId,
    state: Arc<Mutex<NodeState>>,
) {
    match TcpStream::connect(target).await {
        Ok(mut stream) => {
            let req = RpcMessage::RequestVote(RequestVote {
                term: state.lock().await.current_term,
                candidate_id: id,
                last_log_index: 0,
                last_log_term: 0,
            });
            if let Err(e) = send_message(&mut stream, &req).await {
                println!("[Node {}] send RequestVote to {} failed: {}", id, target, e);
                return;
            }

            // try to read reply (with short timeout) — if peer closes, we ignore
            if let Ok(reply) = read_message(&mut stream).await {
                println!("[Node {}] got reply from {}: {:?}", id, target, reply);
                if let RpcMessage::RequestVoteResponse(resp) = reply {
                    let mut st = state.lock().await;
                    // Accept votes only if we are still candidate and term matches
                    if st.role == proto::state::Role::Candidate && resp.vote_granted {
                        st.votes_received += 1;
                    }
                    // if follower or leader and resp.term > current_term, update term
                    if resp.term > st.current_term {
                        st.current_term = resp.term;
                        st.role = proto::state::Role::Follower;
                        st.voted_for = None;
                    }
                }
            }
        }
        Err(e) => {
            println!("[Node {}] connect to {} failed: {}", id, target, e);
        }
    }
}

async fn send_append_entries_once(
    target: &str,
    id: NodeId,
    state: Arc<Mutex<NodeState>>,
) {
    match TcpStream::connect(target).await {
        Ok(mut stream) => {
            let msg = RpcMessage::AppendEntries(AppendEntries {
                term: state.lock().await.current_term,
                leader_id: id,
                prev_log_index: 0,
                prev_log_term: 0,
                entries: vec![],
                leader_commit: 0,
            });
            let _ = send_message(&mut stream, &msg).await;
            // optional: read response and update term
            if let Ok(reply) = read_message(&mut stream).await {
                if let RpcMessage::AppendEntriesResponse(resp) = reply {
                    let mut st = state.lock().await;
                    if resp.term > st.current_term {
                        st.current_term = resp.term;
                        st.role = proto::state::Role::Follower;
                        st.voted_for = None;
                    }
                }
            }
        }
        Err(e) => {
            println!("[Node {}] heartbeat connect to {} failed: {}", id, target, e);
        }
    }
}

async fn election_timer_task(
    id: NodeId,
    peers: Vec<String>,
    state: Arc<tokio::sync::Mutex<NodeState>>,
) {
    loop {
        // Random timeout between 300–700 ms to reduce collisions
        let timeout = thread_rng().gen_range(300..=700);
        sleep(Duration::from_millis(timeout)).await;

        // Check role before starting election
        {
            let st = state.lock().await;
            if st.role != proto::state::Role::Follower {
                // if candidate or leader, skip starting a new election here
                continue;
            }
        }

        // start election
        {
            let mut st = state.lock().await;
            let new_term = st.start_election(id);
            // start_election should set role=Candidate, voted_for=self and reset votes_received
            println!("[Node {}] Election timeout → starting election (term {})", id, new_term);
        }

        // send RequestVote to peers concurrently
        for target in peers.clone() {
            let state_clone = state.clone();
            let id_clone = id;
            tokio::spawn(async move {
                send_request_vote_once(&target, id_clone, state_clone).await;
            });
        }

        // wait a bit to collect votes
        sleep(Duration::from_millis(400)).await;

        // decide election result
        {
            let mut st = state.lock().await;
            // check if heard higher term in the meantime
            if st.role == proto::state::Role::Candidate {
                let total_nodes = st.peers_count + 1;
                let majority = (total_nodes / 2) + 1;

                if st.votes_received >= majority {
                    st.role = proto::state::Role::Leader;
                    println!("[Node {}] became LEADER for term {} (votes: {})", id, st.current_term, st.votes_received);

                    // spawn heartbeat task for this leader
                    let peers_for_hb = peers.clone();
                    let state_for_hb = state.clone();
                    tokio::spawn(async move {
                        loop {
                            // heartbeat interval (less than election timeout lower bound)
                            sleep(Duration::from_millis(100)).await;
                            let st = state_for_hb.lock().await;
                            if st.role != proto::state::Role::Leader {
                                break; // stop heartbeat when no longer leader
                            }
                            drop(st);

                            for p in peers_for_hb.iter() {
                                let state_clone = state_for_hb.clone();
                                let id_clone = id;
                                let target = p.clone();
                                tokio::spawn(async move {
                                    send_append_entries_once(&target, id_clone, state_clone).await;
                                });
                            }
                        }
                        println!("[Node {}] heartbeat task exiting (no longer leader)", id);
                    });
                } else {
                    // election failed -> revert to follower to let others try
                    println!("[Node {}] election failed (votes: {}), reverting to follower", id, st.votes_received);
                    st.role = proto::state::Role::Follower;
                    st.voted_for = None;
                    st.votes_received = 0;
                }
            }
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let state = Arc::new(Mutex::new(NodeState::new()));

    // parse peers and set peers_count
    let peers: Vec<String> = if let Some(ref peer_str) = args.peers {
        peer_str.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
    } else {
        vec![]
    };

    {
        let mut st = state.lock().await;
        st.peers_count = peers.len();
    }

    let listen_addr = format!("0.0.0.0:{}", args.port);
    println!("Node {} listening on {}", args.id, listen_addr);

    let state_listener = state.clone();
    let listener_task = task::spawn(async move {
        let listener = TcpListener::bind(&listen_addr).await.unwrap();
        loop {
            let (mut socket, peer) = listener.accept().await.unwrap();
            println!("Accepted connection from {}", peer);

            let msg = match read_message(&mut socket).await {
                Ok(m) => m,
                Err(e) => {
                    println!("Error reading message from {}: {}", peer, e);
                    continue;
                }
            };

            println!("Received: {:?}", msg);

            // handle RPCs
            match msg {
                RpcMessage::RequestVote(req) => {
                    let mut st = state_listener.lock().await; // async lock
                    let resp = st.handle_request_vote(&req);
                    let reply = RpcMessage::RequestVoteResponse(resp.clone());
                    if let Err(e) = send_message(&mut socket, &reply).await {
                        println!("Failed to send RequestVoteResponse: {}", e);
                    }
                    println!("Sent response: {:?}", reply);
                }
                RpcMessage::RequestVoteResponse(resp) => {
                    let mut st = state_listener.lock().await;

                    // If the response term is higher, update term and become follower
                    if resp.term > st.current_term {
                        st.current_term = resp.term;
                        st.role = proto::state::Role::Follower;
                        st.voted_for = None;
                        st.votes_received = 0;
                        continue;
                    }

                    if st.role == proto::state::Role::Candidate && resp.vote_granted {
                        st.votes_received += 1;
                    }
                }
                RpcMessage::AppendEntries(req) => {
                    let mut st = state_listener.lock().await;
                    let resp = st.handle_append_entries(&req);
                    let reply = RpcMessage::AppendEntriesResponse(resp.clone());
                    if let Err(e) = send_message(&mut socket, &reply).await {
                        println!("Failed to send AppendEntriesResponse: {}", e);
                    }

                    // if AppendEntries is valid leader heartbeat, reset to follower
                    if req.term >= st.current_term {
                        st.role = proto::state::Role::Follower;
                        st.voted_for = None;
                        st.current_term = req.term;
                    }

                    println!("Sent AppendEntriesResponse: {:?}", reply);
                }
                RpcMessage::AppendEntriesResponse(resp) => {
                    let mut st = state_listener.lock().await;
                    if resp.term > st.current_term {
                        st.current_term = resp.term;
                        st.role = proto::state::Role::Follower;
                        st.voted_for = None;
                    }
                    // otherwise, leader might inspect success/failure to adjust nextIndex (not implemented here)
                }
                _ => {
                    println!("Unhandled message: {:?}", msg);
                }
            }
        }
    });

    // spawn election timer if there are peers (i.e. cluster)
    if !peers.is_empty() {
        let state_clone = state.clone();
        let peers_clone = peers.clone();
        tokio::spawn(async move {
            election_timer_task(args.id, peers_clone, state_clone).await;
        });
    }

    // block until listener exits (never)
    listener_task.await?;
    Ok(())
}
