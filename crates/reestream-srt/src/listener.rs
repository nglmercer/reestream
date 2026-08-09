use bytes::Bytes;
use futures::prelude::*;
use srt_tokio::SrtListener as NetworkListener;
use std::time::Duration;
use tokio::sync::broadcast;
use tracing::{info, warn};

use crate::config::SrtConfig;
use crate::error::SrtError;

#[derive(Debug, Clone)]
pub struct SrtPacket {
    pub stream_id: Option<String>,
    pub data: Bytes,
}

pub struct SrtListener {
    config: SrtConfig,
    data_tx: broadcast::Sender<Bytes>,
    packet_tx: broadcast::Sender<SrtPacket>,
}

impl SrtListener {
    pub fn new(config: SrtConfig) -> Self {
        let (data_tx, _) = broadcast::channel(256);
        let (packet_tx, _) = broadcast::channel(256);
        Self {
            config,
            data_tx,
            packet_tx,
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Bytes> {
        self.data_tx.subscribe()
    }

    pub fn subscribe_packets(&self) -> broadcast::Receiver<SrtPacket> {
        self.packet_tx.subscribe()
    }

    pub async fn run(&self) -> Result<(), SrtError> {
        self.config.validate()?;

        let bind_addr = format!("{}:{}", self.config.listen_addr, self.config.listen_port);
        info!("SRT listener starting on {}", bind_addr);

        let mut builder = NetworkListener::builder()
            .latency(Duration::from_millis(self.config.latency_ms as u64));

        if let Some(ref pass) = self.config.passphrase {
            let key_len = self.config.pbkey_len.unwrap_or(16) as u16;
            builder = builder.encryption(key_len, pass.clone());
        }

        let (_listener, mut incoming) = builder
            .bind(bind_addr.as_str())
            .await
            .map_err(|e| SrtError::BindFailed(format!("{e}")))?;

        info!("SRT listener bound on {}", bind_addr);

        let data_tx = self.data_tx.clone();
        let packet_tx = self.packet_tx.clone();

        while let Some(request) = incoming.incoming().next().await {
            let stream_id = request.stream_id().map(ToString::to_string);
            let mut socket = match request.accept(None).await {
                Ok(socket) => socket,
                Err(error) => {
                    warn!("SRT connection rejected: {}", error);
                    continue;
                }
            };
            let data_tx = data_tx.clone();
            let packet_tx = packet_tx.clone();
            tokio::spawn(async move {
                while let Some(result) = socket.next().await {
                    match result {
                        Ok((_instant, bytes)) => {
                            let data = Bytes::from(bytes.to_vec());
                            let _ = data_tx.send(data.clone());
                            let _ = packet_tx.send(SrtPacket {
                                stream_id: stream_id.clone(),
                                data,
                            });
                        }
                        Err(error) => {
                            warn!("SRT receive error: {}", error);
                            break;
                        }
                    }
                }
                info!("SRT connection ended");
            });
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
        let _packet_rx = listener.subscribe_packets();
        let _rx2 = listener.subscribe();
    }
}
