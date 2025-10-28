use crate::RpcMessage;
use anyhow::{anyhow, Result};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};

/// Send a single RpcMessage over a TCP stream using length-prefixed framing.
pub async fn send_message(stream: &mut TcpStream, msg: &RpcMessage) -> Result<()> {
    let data = bincode::serialize(msg)?;
    let len = data.len() as u32;
    stream.write_u32_le(len).await?;
    stream.write_all(&data).await?;
    Ok(())
}

/// Read one RpcMessage from a TCP stream.
pub async fn read_message(stream: &mut TcpStream) -> Result<RpcMessage> {
    let len = stream.read_u32_le().await?;
    let mut buf = vec![0u8; len as usize];
    stream.read_exact(&mut buf).await?;
    let msg: RpcMessage = bincode::deserialize(&buf)
        .map_err(|e| anyhow!("deserialize error: {}", e))?;
    Ok(msg)
}
