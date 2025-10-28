use proto::{
    rpc::read_message,
    *,
};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let addr = "0.0.0.0:7000"; // listens on all LAN interfaces
    let listener = TcpListener::bind(addr).await?;
    println!("Server2 listening on {}", addr);

    let (mut socket, peer) = listener.accept().await?;
    println!("Accepted connection from {}", peer);

    let msg = read_message(&mut socket).await?;
    println!("Received message: {:?}", msg);

    Ok(())
}
