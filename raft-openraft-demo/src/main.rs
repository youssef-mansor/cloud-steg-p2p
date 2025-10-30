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
        heartbeat_interval: 500,
        election_timeout_min: 1500,
        election_timeout_max: 3000,
        ..Default::default()
    };

    let config = Arc::new(config.validate()?);

    let network_factory = NetworkFactory::new();

    if let Some(peers_str) = &args.peers {
        for peer in peers_str.split(',') {
            if let Some((id_str, addr)) = peer.split_once('=') {
                let peer_id: NodeId = id_str.trim().parse()?;
                network_factory.add_peer(peer_id, addr.trim().to_string()).await;
            }
        }
    }

    let store = MemStore::new_async().await;
    let (log_store, state_machine) = Adaptor::new(store);

    let raft = Raft::new(args.id, config.clone(), network_factory, log_store, state_machine).await?;
    let raft = Arc::new(raft);

    println!("✅ Raft node created");

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
