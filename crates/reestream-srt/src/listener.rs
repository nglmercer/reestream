use bytes::Bytes;
use futures::prelude::*;
use srt_tokio::{SrtListener as SrtTokioListener, SrtSocket};
use std::net::SocketAddr;
use tokio::sync::{broadcast, mpsc};
use tracing::{error, info, warn};

use crate::config::SrtConfig;
use crate::error::SrtError;

pub struct SrtListener {
    config: SrtConfig,
    data_tx: broadcast::Sender<Bytes>,
}

impl SrtListener {
    pub fn new(config: SrtConfig) -> Self {
        let (data_tx, _) = broadcast::channel(256);
        Self { config, data_tx }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Bytes> {
        self.data_tx.subscribe()
    }

    pub async fn run(&self) -> Result<(), SrtError> {
        self.config.validate()?;

        let addr: SocketAddr = format!("{}:{}", self.config.listen_addr, self.config.listen_port)
            .parse()
            .map_err(|e| SrtError::InvalidConfig(format!("Invalid address: {e}")))?;

        info!("SRT listener starting on {}", addr);

        let listener = SrtTokioListener::builder()
            .set(|options| {
                options.latency = std::time::Duration::from_millis(self.config.latency_ms as u64);
                if self.config.max_bandwidth > 0 {
                    options.max_bandwidth = self.config.max_bandwidth;
                }
                if let Some(ref pass) = self.config.passphrase {
                    options.encryption = srt_tokio::options::Encryption::Aes128 {
                        passphrase: pass.clone().into(),
                    };
                }
            })
            .bind(addr)
            .await
            .map_err(|e| SrtError::BindFailed(format!("{e}")))?;

        info!("SRT listener bound on {}", addr);

        let mut incoming = listener.incoming();

        while let Some(result) = incoming.next().await {
            let (sender, _req) = result.map_err(|e| SrtError::ConnectionFailed(format!("{e}")))?;

            let data_tx = self.data_tx.clone();
            tokio::spawn(async move {
                if let Err(e) = Self::handle_connection(sender, data_tx).await {
                    warn!("SRT connection error: {}", e);
                }
            });
        }

        Ok(())
    }

    async fn handle_connection(
        mut sender: SrtSocket,
        data_tx: broadcast::Sender<Bytes>,
    ) -> Result<(), SrtError> {
        loop {
            match sender.next().await {
                Some(Ok((Instant, bytes))) => {
                    let data = Bytes::from(bytes.to_vec());
                    let _ = data_tx.send(data);
                }
                Some(Err(e)) => {
                    warn!("SRT receive error: {}", e);
                    return Err(SrtError::ReceiveFailed(format!("{e}")));
                }
                None => {
                    info!("SRT connection closed");
                    return Ok(());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_srt_listener_creation() {
        let config = SrtConfig::default();
        let listener = SrtListener::new(config);
        assert_eq!(listener.config.listen_port, 3000);
    }

    #[test]
    fn test_srt_listener_subscribe() {
        let config = SrtConfig::default();
        let listener = SrtListener::new(config);
        let _rx = listener.subscribe();
        let _rx2 = listener.subscribe();
    }
}
