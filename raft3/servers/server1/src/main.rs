use proto::{
    rpc::send_message,
    *,
};
use tokio::net::TcpStream;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // replace this IP with the actual LAN IP of server2
    let target = "10.7.57.205:7000";

    let mut stream = TcpStream::connect(target).await?;
    println!("Connected to {}", target);

    let msg = RpcMessage::RequestVote(RequestVote {
        term: 1,
        candidate_id: 1,
        last_log_index: 0,
        last_log_term: 0,
    });

    send_message(&mut stream, &msg).await?;
    println!("Message sent: {:?}", msg);

    Ok(())
}
