use bytes::Bytes;
use futures::prelude::*;
use srt_tokio::SrtSocket;
use std::time::{Duration, Instant};
use tracing::info;
use url::Url;

use crate::error::SrtError;

pub struct SrtSender {
    url: Url,
    latency_ms: u32,
    passphrase: Option<String>,
    socket: Option<SrtSocket>,
}

impl SrtSender {
    pub fn new(url: Url, latency_ms: u32, passphrase: Option<String>) -> Self {
        Self {
            url,
            latency_ms,
            passphrase,
            socket: None,
        }
    }

    pub async fn connect(&mut self) -> Result<(), SrtError> {
        let host = self
            .url
            .host_str()
            .ok_or_else(|| SrtError::InvalidConfig("No host in SRT URL".into()))?;
        let port = self.url.port().unwrap_or(3000);
        let addr = format!("{host}:{port}");

        info!("Connecting SRT sender to {}", addr);

        let mut builder =
            SrtSocket::builder().latency(Duration::from_millis(self.latency_ms as u64));

        if let Some(ref pass) = self.passphrase {
            builder = builder.encryption(16, pass.clone());
        }

        let socket = builder
            .call(addr.as_str(), None)
            .await
            .map_err(|e| SrtError::ConnectionFailed(format!("{e}")))?;

        self.socket = Some(socket);
        info!("SRT sender connected to {}", addr);
        Ok(())
    }

    pub async fn send(&mut self, data: Bytes) -> Result<(), SrtError> {
        let socket = self
            .socket
            .as_mut()
            .ok_or_else(|| SrtError::SendFailed("Not connected".into()))?;

        let instant = Instant::now();
        socket
            .send((instant, data))
            .await
            .map_err(|e| SrtError::SendFailed(format!("{e}")))?;
        Ok(())
    }

    pub async fn disconnect(&mut self) {
        if let Some(mut socket) = self.socket.take() {
            let _ = socket.close().await;
            info!("SRT sender disconnected");
        }
    }

    pub fn is_connected(&self) -> bool {
        self.socket.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_srt_sender_creation() {
        let url: Url = "srt://example.com:3000".parse().unwrap();
        let sender = SrtSender::new(url.clone(), 200, None);
        assert_eq!(sender.url, url);
        assert_eq!(sender.latency_ms, 200);
        assert!(!sender.is_connected());
    }

    #[test]
    fn test_srt_sender_with_passphrase() {
        let url: Url = "srt://example.com:3000".parse().unwrap();
        let sender = SrtSender::new(url, 200, Some("longenoughpassphrase".into()));
        assert!(sender.passphrase.is_some());
    }
}
