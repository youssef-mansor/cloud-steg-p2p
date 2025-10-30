mod types;
mod store;
mod network;

use std::sync::Arc;
use clap::Parser;
use tokio::net::TcpListener;
use anyhow::Result;
use openraft::{Config, Raft};
use openraft::storage::Adaptor;
use network::NetworkFactory;
use openraft_memstore::MemStore;

pub type NodeId = u64;

/// Command-line arguments
#[derive(Parser, Debug)]
#[clap(author, version, about)]
struct Args {
    /// Node ID (unique integer)
    #[clap(long)]
    id: u64,

    /// Listening address, e.g. 0.0.0.0:7000
    #[clap(long)]
    addr: String,

    /// Comma-separated peer addresses, e.g. "10.40.56.135:7001,10.40.56.136:7002"
    #[clap(long)]
    peers: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing for better logging
    tracing_subscriber::fmt()
        .with_target(true)
        .with_thread_ids(true)
        .with_level(true)
        .init();

    let args = Args::parse();
    println!("🚀 Node {} starting on {}", args.id, args.addr);

    // Raft config
    let config = Config {
        cluster_name: "example-cluster".to_string(),
        heartbeat_interval: 500,
        election_timeout_min: 1500,
        election_timeout_max: 3000,
        ..Default::default()
    };
    
    let config = Arc::new(config.validate()?);
    println!("✓ Raft configuration validated");
    
    // Create network factory
    let network_factory = NetworkFactory::new();
    
    // Parse and register peer addresses if provided
    if let Some(peers_str) = &args.peers {
        for (idx, peer_addr) in peers_str.split(',').enumerate() {
            let peer_id = (idx + 2) as NodeId; // Assuming this is node 1, peers are 2, 3, etc.
            network_factory.add_peer(peer_id, peer_addr.trim().to_string()).await;
        }
    }
    println!("✓ RaftNetwork created");
    
    // Create storage - MemStore implements RaftStorage
    let store = MemStore::new_async().await;
    println!("✓ RaftStorage created");
    
    // Create Adaptor which splits the storage into log and state machine
    let (log_store, state_machine) = Adaptor::new(store);
    
    // Create the Raft instance
    let raft = Raft::new(
        args.id,
        config.clone(),
        network_factory,
        log_store,
        state_machine,
    ).await?;
    
    println!("✓ Raft instance created");
    println!("📊 Node {} is ready (not yet initialized as cluster)", args.id);
    println!();
    println!("Next steps:");
    println!("  1. Initialize this node as a single-node cluster");
    println!("  2. Add other nodes as learners");
    println!("  3. Change membership to add them as voters");
    println!();

    let listener = TcpListener::bind(&args.addr).await?;
    println!("🔌 Listening on {}", args.addr);

    loop {
        let (socket, peer_addr) = listener.accept().await?;
        println!("📥 Accepted connection from {}", peer_addr);
        drop(socket);
    }
}
