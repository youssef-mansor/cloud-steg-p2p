use proto::{
    rpc::{read_message, send_message},
    *,
};
use tokio::{net::{TcpListener, TcpStream}, task};

#[tokio::test]
async fn test_send_and_receive_message() {
    // Start a temporary TCP listener
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    // Spawn server task to accept and read one message
    let server = task::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let msg = read_message(&mut socket).await.unwrap();
        msg
    });

    // Client connects and sends one message
    let mut stream = TcpStream::connect(addr).await.unwrap();
    let sent = RpcMessage::RequestVote(RequestVote {
        term: 1,
        candidate_id: 1,
        last_log_index: 5,
        last_log_term: 1,
    });
    send_message(&mut stream, &sent).await.unwrap();

    // Check round-trip equality
    let received = server.await.unwrap();
    assert_eq!(sent, received);
}
