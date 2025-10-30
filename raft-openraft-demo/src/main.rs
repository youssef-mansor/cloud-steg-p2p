use std::net::SocketAddr;
use clap::Parser;
use tokio::net::TcpListener;
use anyhow::Result;
use openraft::Config;
use std::sync::Arc;

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
    let args = Args::parse();
    println!("Node {} starting on {}", args.id, args.addr);

    // Raft config
    let config = Arc::new(Config::default().validate().unwrap());
    // TODO: Add Raft node and storage here in the next step

    let listener = TcpListener::bind(&args.addr).await?;
    println!("Listening on {}", args.addr);

    loop {
        let (socket, peer_addr) = listener.accept().await?;
        println!("Accepted connection from {}", peer_addr);
        drop(socket);
    }
}
