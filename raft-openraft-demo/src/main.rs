mod types;
mod store;
mod network;
mod api;
mod rpc;
mod rpc_handler;

use std::sync::Arc;
use clap::Parser;
use anyhow::Result;
use openraft::{Config, Raft};
use openraft::storage::Adaptor;
use network::NetworkFactory;
use openraft_memstore::MemStore;
use api::AppState;
use rpc_handler::start_rpc_server;

pub type NodeId = u64;

/// Command-line arguments
#[derive(Parser, Debug)]
#[clap(author, version, about)]
struct Args {
    /// Node ID (unique integer)
    #[clap(long)]
    id: u64,

    /// HTTP API address, e.g. 0.0.0.0:8000
    #[clap(long)]
    http_addr: String,

    /// Raft RPC address (for internal cluster communication)
    #[clap(long)]
    rpc_addr: String,

    /// Comma-separated peer addresses, e.g. "2=127.0.0.1:7002,3=127.0.0.1:7003"
    #[clap(long)]
    peers: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_target(false)
        .with_thread_ids(false)
        .with_level(true)
        .compact()
        .init();

    let args = Args::parse();
    println!("🚀 Node {} starting", args.id);
    println!("   HTTP API: {}", args.http_addr);
    println!("   Raft RPC: {}", args.rpc_addr);

    let config = Config {
        cluster_name: "example-cluster".to_string(),
        heartbeat_interval: 300,   // More frequent heartbeats to detect failures faster
        election_timeout_min: 1500,  // Balanced timeout
        election_timeout_max: 2500,  // Prevent split-brain while allowing timely elections
        max_payload_entries: 1000,
        install_snapshot_timeout: 60000,
        max_in_snapshot_log_to_keep: 2000,
        ..Default::default()
    };

    let config = Arc::new(config.validate()?);

    let network_factory = NetworkFactory::new();

    // Register all peers - ensure all nodes know about each other
    if let Some(peers_str) = &args.peers {
        for peer in peers_str.split(',') {
            if let Some((id_str, addr)) = peer.split_once('=') {
                let peer_id: NodeId = id_str.trim().parse()?;
                network_factory.add_peer(peer_id, addr.trim().to_string()).await;
            }
        }
    }
    
    // Also register self address (needed for some Raft operations)
    // Extract port from rpc_addr
    let self_addr = if args.rpc_addr.contains(':') {
        args.rpc_addr.clone()
    } else {
        format!("127.0.0.1:{}", args.rpc_addr)
    };
    network_factory.add_peer(args.id, self_addr).await;

    let store = MemStore::new_async().await;
    let (log_store, state_machine) = Adaptor::new(store);

    let raft = Raft::new(args.id, config.clone(), network_factory, log_store, state_machine).await?;
    let raft = Arc::new(raft);

    println!("✅ Raft node created (initial state: {:?})", raft.metrics().borrow().state);
    
    // Monitor leader changes and detect split-brain scenarios
    let raft_monitor = raft.clone();
    tokio::spawn(async move {
        let mut last_state = None;
        let mut last_term = None;
        let mut last_leader = None;
        let mut stale_leader_warnings = 0;
        
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            let metrics = raft_monitor.metrics().borrow().clone();
            
            // Log state changes
            if last_state != Some(metrics.state) {
                println!("🔄 Node {} state changed: {:?} -> {:?}", 
                         metrics.id, last_state.map(|s| format!("{:?}", s)).unwrap_or_default(), 
                         format!("{:?}", metrics.state));
                last_state = Some(metrics.state);
            }
            
            if last_term != Some(metrics.current_term) {
                println!("📈 Node {} term changed: {} -> {}", 
                         metrics.id, last_term.unwrap_or(0), metrics.current_term);
                last_term = Some(metrics.current_term);
                stale_leader_warnings = 0; // Reset on term change
            }
            
            if last_leader != metrics.current_leader {
                if let Some(leader) = metrics.current_leader {
                    println!("👑 Node {} detected leader: {} (term {})", 
                             metrics.id, leader, metrics.current_term);
                } else {
                    println!("⚠️  Node {} has no leader (term {})", 
                             metrics.id, metrics.current_term);
                }
                last_leader = metrics.current_leader;
            }
            
            // CRITICAL: Detect split-brain - if we're a leader but there's another leader in a different/higher term
            if matches!(metrics.state, openraft::ServerState::Leader) {
                stale_leader_warnings += 1;
                if stale_leader_warnings > 5 {
                    println!("⚠️  Node {} has been leader for {}s (term {}) - ensure no split-brain (check other nodes)", 
                             metrics.id, stale_leader_warnings, metrics.current_term);
                    stale_leader_warnings = 0; // Reset counter
                }
            }
        }
    });
    
    // Give the node a moment to establish connections before checking state
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    
    // Log current metrics to help debug
    let metrics = raft.metrics().borrow().clone();
    println!("📊 Initial metrics: state={:?}, term={}, leader={:?}", 
             metrics.state, metrics.current_term, metrics.current_leader);

    // Start RPC server in background
    let rpc_raft = raft.clone();
    let rpc_addr = args.rpc_addr.clone();
    tokio::spawn(async move {
        if let Err(e) = start_rpc_server(rpc_addr, rpc_raft).await {
            eprintln!("RPC server error: {}", e);
        }
    });

    // Start HTTP server
    let app_state = AppState {
        raft: raft.clone(),
        node_id: args.id,
    };

    let app = api::create_router(app_state);
    let listener = tokio::net::TcpListener::bind(&args.http_addr).await?;

    println!("🌐 HTTP API listening on {}", args.http_addr);

    axum::serve(listener, app).await?;

    Ok(())
}
