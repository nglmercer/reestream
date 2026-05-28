use bytes::Bytes;
use futures::prelude::*;
use srt_tokio::SrtSocket;
use std::time::Duration;
use tokio::sync::broadcast;
use tracing::{info, warn};

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

        let bind_addr = format!("{}:{}", self.config.listen_addr, self.config.listen_port);
        info!("SRT listener starting on {}", bind_addr);

        let mut builder = SrtSocket::builder()
            .latency(Duration::from_millis(self.config.latency_ms as u64));

        if let Some(ref pass) = self.config.passphrase {
            let key_len = self.config.pbkey_len.unwrap_or(16) as u16;
            builder = builder.encryption(key_len, pass.clone());
        }

        let mut socket = builder
            .listen_on(bind_addr.as_str())
            .await
            .map_err(|e| SrtError::BindFailed(format!("{e}")))?;

        info!("SRT listener bound on {}", bind_addr);

        let data_tx = self.data_tx.clone();

        while let Some(result) = socket.next().await {
            match result {
                Ok((_instant, bytes)) => {
                    let data = Bytes::from(bytes.to_vec());
                    let _ = data_tx.send(data);
                }
                Err(e) => {
                    warn!("SRT receive error: {}", e);
                }
            }
        }

        info!("SRT listener stopped");
        Ok(())
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
