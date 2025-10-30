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

async fn connect_with_retry(addr: &str, retries: usize) -> Option<TcpStream> {
    for attempt in 1..=retries {
        match TcpStream::connect(addr).await {
            Ok(stream) => return Some(stream),
            Err(e) => {
                eprintln!("[connect] failed to connect to {addr} (attempt {attempt}): {e}");
                sleep(Duration::from_millis(200)).await;
            }
        }
    }
    eprintln!("[connect] giving up on {addr} after {retries} attempts");
    None
}

/// Command-line arguments
#[derive(Parser, Debug)]
struct Args {
    /// Node ID (unique)
    #[arg(long)]
    id: NodeId,

    /// Listening port, e.g. 7000
    #[arg(long)]
    port: u16,

    /// Comma-separated peer addresses, e.g. "127.0.0.1:7001,127.0.0.1:7002"
    #[arg(long)]
    peers: Option<String>,
}

/// Send RequestVote and return the response if any
async fn send_request_vote_rpc(
    target: &str,
    term: Term,
    id: NodeId,
) -> Option<RequestVoteResponse> {
    if let Some(mut stream) = connect_with_retry(target, 5).await {
        let req = RpcMessage::RequestVote(RequestVote {
            term,
            candidate_id: id,
            last_log_index: 0,
            last_log_term: 0,
        });
        if let Err(e) = send_message(&mut stream, &req).await {
            eprintln!("[send_request_vote_rpc] send to {} failed: {}", target, e);
            return None;
        }
        match read_message(&mut stream).await {
            Ok(RpcMessage::RequestVoteResponse(resp)) => Some(resp),
            Ok(other) => {
                eprintln!("[send_request_vote_rpc] unexpected reply from {}: {:?}", target, other);
                None
            }
            Err(e) => {
                eprintln!("[send_request_vote_rpc] read from {} failed: {}", target, e);
                None
            }
        }
    } else {
        None
    }
}

/// Send one AppendEntries (heartbeat) and optionally return response
async fn send_append_entries_rpc(target: &str, term: Term, id: NodeId) -> Option<AppendEntriesResponse> {
    if let Some(mut stream) = connect_with_retry(target, 3).await {
        let msg = RpcMessage::AppendEntries(AppendEntries {
            term,
            leader_id: id,
            prev_log_index: 0,
            prev_log_term: 0,
            entries: vec![],
            leader_commit: 0,
        });
        if let Err(e) = send_message(&mut stream, &msg).await {
            eprintln!("[append] send to {} failed: {}", target, e);
            return None;
        }
        match read_message(&mut stream).await {
            Ok(RpcMessage::AppendEntriesResponse(resp)) => Some(resp),
            Ok(other) => {
                eprintln!("[append] unexpected reply from {}: {:?}", target, other);
                None
            }
            Err(e) => {
                // it's okay if read fails; follower might close connection
                None
            }
        }
    } else {
        None
    }
}

/// Election loop: synchronous broadcasting & counting (avoids racey increments)
async fn election_timer_task(
    id: NodeId,
    peers: Vec<String>,
    state: Arc<Mutex<NodeState>>,
) {
    loop {
        // small sleep, then check elapsed since last heartbeat
        sleep(Duration::from_millis(50)).await;

        // compute randomized election timeout
        let election_timeout_ms = thread_rng().gen_range(300..=600);

        // check whether it's time to start an election
        let start_election_now = {
            let st = state.lock().await;
            st.is_follower() && st.last_heartbeat.elapsed() > Duration::from_millis(election_timeout_ms)
        };

        if !start_election_now {
            continue;
        }

        // Acquire lock and start election: increment term, become candidate, vote for self
        let current_term = {
            let mut st = state.lock().await;
            let new_term = st.start_election(id);
            // start_election sets votes_received = 1 (self)
            new_term
        };

        println!("[Node {}] Election timeout → starting election (term {})", id, current_term);

        // Synchronously send RequestVote RPCs and collect votes
        let mut votes = 1usize; // self-vote counted
        for peer in peers.iter() {
            if let Some(resp) = send_request_vote_rpc(peer, current_term, id).await {
                // if peer has higher term, step down immediately
                if resp.term > current_term {
                    let mut st = state.lock().await;
                    st.current_term = resp.term;
                    st.role = proto::state::Role::Follower;
                    st.voted_for = None;
                    st.votes_received = 0;
                    println!("[Node {}] saw higher term {} -> stepping down to follower", id, resp.term);
                    votes = 0;
                    break;
                } else if resp.vote_granted {
                    votes += 1;
                }
            }
        }

        // After collecting replies, decide with a lock
        {
            let mut st = state.lock().await;
            // ensure still candidate in this term
            if st.role == proto::state::Role::Candidate && st.current_term == current_term {
                let total_nodes = st.peers_count + 1;
                let majority = (total_nodes / 2) + 1;
                if votes >= majority {
                    // become leader
                    st.role = proto::state::Role::Leader;
                    st.votes_received = 0;
                    st.last_heartbeat = std::time::Instant::now();
                    println!("[Node {}] became LEADER for term {} (votes: {})", id, current_term, votes);

                    // send one immediate heartbeat synchronously to establish leadership
                    for peer in peers.iter() {
                        let _ = send_append_entries_rpc(peer, st.current_term, id).await;
                    }

                    // spawn heartbeat task to send periodic AppendEntries
                    let peers_hb = peers.clone();
                    let state_hb = state.clone();
                    let node_for_hb = id;
                    tokio::spawn(async move {
                        loop {
                            sleep(Duration::from_millis(100)).await;
                            let st = state_hb.lock().await;
                            if st.role != proto::state::Role::Leader {
                                break;
                            }
                            let term_now = st.current_term;
                            drop(st);
                            // send heartbeats concurrently but await each send to avoid massive concurrency
                            for p in peers_hb.iter() {
                                let _ = send_append_entries_rpc(p, term_now, node_for_hb).await;
                            }
                        }
                        println!("[Node {}] heartbeat task exiting (no longer leader)", node_for_hb);
                    });
                } else {
                    // election failed -> revert to follower
                    println!("[Node {}] election failed (got {} votes, need {}), reverting to follower", id, votes, majority);
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
    let id = args.id;
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
    println!("Node {} listening on {}", id, listen_addr);

    // listener task
    let state_listener = state.clone();
    let listener_task = {
        let listen_addr = listen_addr.clone();
        task::spawn(async move {
            let listener = TcpListener::bind(&listen_addr).await.unwrap();
            loop {
                let (mut socket, peer) = listener.accept().await.unwrap();
                //println!("Accepted connection from {}", peer);

                let msg = match read_message(&mut socket).await {
                    Ok(m) => m,
                    Err(e) => {
                        eprintln!("Error reading message from {}: {}", peer, e);
                        continue;
                    }
                };

                //println!("Received: {:?}", msg);

                match msg {
                    RpcMessage::RequestVote(req) => {
                        // handle vote request
                        let mut st = state_listener.lock().await;
                        let resp = st.handle_request_vote(&req);
                        let reply = RpcMessage::RequestVoteResponse(resp.clone());
                        if let Err(e) = send_message(&mut socket, &reply).await {
                            eprintln!("Failed to send RequestVoteResponse: {}", e);
                        }
                        // no additional side effects here (handle_request_vote updated term/voted_for)
                    }
                    RpcMessage::AppendEntries(req) => {
                        let mut st = state_listener.lock().await;
                        let resp = st.handle_append_entries(&req);
                        let reply = RpcMessage::AppendEntriesResponse(resp.clone());
                        if let Err(e) = send_message(&mut socket, &reply).await {
                            eprintln!("Failed to send AppendEntriesResponse: {}", e);
                        }
                        // handle_append_entries already updated term/last_heartbeat and role -> follower
                    }
                    other => {
                        eprintln!("Listener got unexpected RPC: {:?}", other);
                    }
                }
            }
        })
    };

    // spawn election timer if cluster size > 1
    if peers.len() > 0 {
        let state_clone = state.clone();
        let peers_clone = peers.clone();
        tokio::spawn(async move {
            election_timer_task(id, peers_clone, state_clone).await;
        });
    }

    listener_task.await?;
    Ok(())
}
