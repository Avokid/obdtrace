use super::Transport;
use anyhow::{Context, Result};
use async_trait::async_trait;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

pub struct TcpTransport {
    stream: TcpStream,
    addr: String,
}

impl TcpTransport {
    pub async fn connect(addr: &str) -> Result<Self> {
        let stream = tokio::time::timeout(Duration::from_secs(5), TcpStream::connect(addr))
            .await
            .with_context(|| format!("timed out connecting to {addr}; is the computer on the adapter's WiFi network?"))?
            .with_context(|| format!("cannot connect to {addr}"))?;
        Ok(Self {
            stream,
            addr: addr.to_string(),
        })
    }
}

#[async_trait]
impl Transport for TcpTransport {
    async fn write(&mut self, data: &[u8]) -> Result<()> {
        self.stream.write_all(data).await?;
        Ok(())
    }

    async fn read_chunk(&mut self, timeout: Duration) -> Result<Option<Vec<u8>>> {
        let mut buf = [0u8; 512];
        match tokio::time::timeout(timeout, self.stream.read(&mut buf)).await {
            Err(_) => Ok(None),
            Ok(Ok(0)) => anyhow::bail!("adapter closed the connection"),
            Ok(Ok(n)) => Ok(Some(buf[..n].to_vec())),
            Ok(Err(e)) => Err(e.into()),
        }
    }

    async fn close(&mut self) {
        let _ = self.stream.shutdown().await;
    }

    fn describe(&self) -> String {
        format!("tcp:{}", self.addr)
    }
}
