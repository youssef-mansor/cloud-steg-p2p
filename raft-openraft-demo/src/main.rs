mod types;
mod store;
mod network;
mod api;
mod rpc;
mod rpc_handler;

use std::collections::BTreeMap;
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
    
    /// Comma-separated HTTP addresses for peers, e.g. "2=127.0.0.1:8002,3=127.0.0.1:8003"
    /// If not provided, will try to derive from peers (assumes HTTP port = RPC port + 1000)
    #[clap(long = "http-peers")]
    http_peers: Option<String>,
}

#[tokio::main(flavor = "multi_thread", worker_threads = 8)]
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

    // Build HTTP addresses map for load balancing
    let mut http_addresses = std::collections::BTreeMap::new();
    
    // Parse HTTP peers if provided, otherwise derive from RPC peers  
    if let Some(http_peers_str) = &args.http_peers {
        for peer in http_peers_str.split(',') {
            if let Some((id_str, addr)) = peer.split_once('=') {
                let peer_id: NodeId = id_str.trim().parse()?;
                http_addresses.insert(peer_id, addr.trim().to_string());
            }
        }
    } else if let Some(peers_str) = &args.peers {
        // Derive HTTP addresses from RPC addresses (assume HTTP = RPC port + 1000)
        for peer in peers_str.split(',') {
            if let Some((id_str, rpc_addr)) = peer.split_once('=') {
                let peer_id: NodeId = id_str.trim().parse()?;
                let rpc_addr = rpc_addr.trim();
                // Try to extract port and add 1000
                if let Some((host, port_str)) = rpc_addr.rsplit_once(':') {
                    if let Ok(port) = port_str.parse::<u16>() {
                        let http_port = port + 1000;
                        let http_addr = format!("{}:{}", host, http_port);
                        http_addresses.insert(peer_id, http_addr);
                        println!("📋 Derived HTTP address for node {}: {}", peer_id, http_addresses[&peer_id]);
                    }
                }
            }
        }
    }
    
    // Also add self HTTP address
    let self_http_addr = if args.http_addr.contains(':') {
        args.http_addr.clone()
    } else {
        format!("127.0.0.1:{}", args.http_addr)
    };
    // Normalize address format (remove 0.0.0.0, use 127.0.0.1 for local)
    let normalized_self_http = self_http_addr.replace("0.0.0.0", "127.0.0.1");
    http_addresses.insert(args.id, normalized_self_http.clone());
    
    println!("📋 HTTP addresses registered: {:?}", http_addresses);

    // Initialize healthy nodes - all nodes start as healthy
    let mut healthy_nodes = BTreeMap::new();
    for node_id in http_addresses.keys() {
        healthy_nodes.insert(*node_id, true);
    }
    healthy_nodes.insert(args.id, true); // Self is always healthy

    // Start HTTP server
    let app_state = AppState {
        raft: raft.clone(),
        node_id: args.id,
        http_addresses: Arc::new(tokio::sync::RwLock::new(http_addresses)),
        self_http_addr: normalized_self_http,
        healthy_nodes: Arc::new(tokio::sync::RwLock::new(healthy_nodes)),
    };

    let app = api::create_router(app_state);
    let listener = tokio::net::TcpListener::bind(&args.http_addr).await?;

    println!("🌐 HTTP API listening on {}", args.http_addr);
    println!("⚡ Server configured with multi-threaded async runtime (8 worker threads)");

    // Use tower::ServiceBuilder for better concurrency
    axum::serve(listener, app).await?;

    Ok(())
}
