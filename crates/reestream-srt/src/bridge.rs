use bytes::Bytes;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::info;

use crate::config::SrtConfig;
use crate::error::SrtError;
use crate::listener::SrtListener;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeConfig {
    pub enabled: bool,
    pub srt_listen_port: u16,
    pub rtmp_forward_url: Option<String>,
    pub hls_output: bool,
    pub latency_ms: u32,
}

impl Default for BridgeConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            srt_listen_port: 3000,
            rtmp_forward_url: None,
            hls_output: true,
            latency_ms: 200,
        }
    }
}

pub struct SrtBridge {
    config: BridgeConfig,
    data_tx: broadcast::Sender<Bytes>,
    stats: Arc<BridgeStats>,
}

#[derive(Debug, Default)]
pub struct BridgeStats {
    pub packets_received: std::sync::atomic::AtomicU64,
    pub packets_forwarded: std::sync::atomic::AtomicU64,
    pub bytes_received: std::sync::atomic::AtomicU64,
    pub active: std::sync::atomic::AtomicBool,
}

impl BridgeStats {
    pub fn to_snapshot(&self) -> BridgeStatsSnapshot {
        BridgeStatsSnapshot {
            packets_received: self
                .packets_received
                .load(std::sync::atomic::Ordering::Relaxed),
            packets_forwarded: self
                .packets_forwarded
                .load(std::sync::atomic::Ordering::Relaxed),
            bytes_received: self
                .bytes_received
                .load(std::sync::atomic::Ordering::Relaxed),
            active: self.active.load(std::sync::atomic::Ordering::Relaxed),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct BridgeStatsSnapshot {
    pub packets_received: u64,
    pub packets_forwarded: u64,
    pub bytes_received: u64,
    pub active: bool,
}

impl SrtBridge {
    pub fn new(config: BridgeConfig) -> Self {
        let (data_tx, _) = broadcast::channel(1024);
        Self {
            config,
            data_tx,
            stats: Arc::new(BridgeStats::default()),
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Bytes> {
        self.data_tx.subscribe()
    }

    pub fn stats(&self) -> BridgeStatsSnapshot {
        self.stats.to_snapshot()
    }

    pub async fn run(&self) -> Result<(), SrtError> {
        if !self.config.enabled {
            return Err(SrtError::InvalidConfig("Bridge is disabled".into()));
        }

        let srt_config = SrtConfig {
            enabled: true,
            listen_port: self.config.srt_listen_port,
            latency_ms: self.config.latency_ms,
            ..Default::default()
        };

        let listener = SrtListener::new(srt_config);
        let mut rx = listener.subscribe();
        let data_tx = self.data_tx.clone();
        let stats = self.stats.clone();

        self.stats
            .active
            .store(true, std::sync::atomic::Ordering::Relaxed);

        info!(
            "SRT bridge starting on port {}",
            self.config.srt_listen_port
        );

        let bridge_handle = tokio::spawn(async move {
            while let Ok(data) = rx.recv().await {
                stats
                    .packets_received
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                stats
                    .bytes_received
                    .fetch_add(data.len() as u64, std::sync::atomic::Ordering::Relaxed);

                let _ = data_tx.send(data.clone());

                stats
                    .packets_forwarded
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
        });

        if let Some(ref rtmp_url) = self.config.rtmp_forward_url {
            info!("SRT bridge forwarding to RTMP: {}", rtmp_url);
        }

        listener.run().await?;

        bridge_handle.abort();
        self.stats
            .active
            .store(false, std::sync::atomic::Ordering::Relaxed);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bridge_config_default() {
        let config = BridgeConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.srt_listen_port, 3000);
        assert!(config.rtmp_forward_url.is_none());
        assert!(config.hls_output);
    }

    #[test]
    fn test_bridge_stats_default() {
        let stats = BridgeStats::default();
        let snap = stats.to_snapshot();
        assert_eq!(snap.packets_received, 0);
        assert!(!snap.active);
    }

    #[test]
    fn test_bridge_creation() {
        let config = BridgeConfig::default();
        let bridge = SrtBridge::new(config);
        let snap = bridge.stats();
        assert!(!snap.active);
    }

    #[test]
    fn test_bridge_subscribe() {
        let bridge = SrtBridge::new(BridgeConfig::default());
        let _rx1 = bridge.subscribe();
        let _rx2 = bridge.subscribe();
    }
}
