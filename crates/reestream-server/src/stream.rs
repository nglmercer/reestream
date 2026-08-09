use reestream_core::config::{PlatformEvent, platform_id_from};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::{RwLock, broadcast};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamInfo {
    pub id: String,
    pub name: String,
    pub input_url: String,
    pub status: StreamStatus,
    pub started_at: Option<u64>,
    pub viewers: u32,
    pub bitrate: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub enum StreamStatus {
    #[default]
    Idle,
    Live,
    Error(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Platform {
    pub id: String,
    pub name: String,
    pub url: String,
    pub key: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize)]
pub enum StreamEvent {
    Started {
        id: String,
        name: String,
        input_url: String,
    },
    Stopped {
        id: String,
    },
    Updated {
        id: String,
        viewers: u32,
        bitrate: u64,
    },
    Error {
        id: String,
        message: String,
    },
}

pub struct StreamManager {
    streams: Arc<RwLock<Vec<StreamInfo>>>,
    platforms: Arc<RwLock<Vec<Platform>>>,
    event_tx: broadcast::Sender<StreamEvent>,
    platform_event_tx: broadcast::Sender<PlatformEvent>,
}

impl Default for StreamManager {
    fn default() -> Self {
        Self::new()
    }
}

impl StreamManager {
    pub fn new() -> Self {
        let (event_tx, _) = broadcast::channel(256);
        let (platform_event_tx, _) = broadcast::channel(256);
        Self {
            streams: Arc::new(RwLock::new(Vec::new())),
            platforms: Arc::new(RwLock::new(Vec::new())),
            event_tx,
            platform_event_tx,
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<StreamEvent> {
        self.event_tx.subscribe()
    }

    pub fn subscribe_platform_events(&self) -> broadcast::Receiver<PlatformEvent> {
        self.platform_event_tx.subscribe()
    }

    pub async fn add_stream(&self, name: String, input_url: String) -> String {
        let id = Uuid::new_v4().to_string();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let stream = StreamInfo {
            id: id.clone(),
            name: name.clone(),
            input_url: input_url.clone(),
            status: StreamStatus::Live,
            started_at: Some(now),
            viewers: 0,
            bitrate: 0,
        };
        self.streams.write().await.push(stream);
        let _ = self.event_tx.send(StreamEvent::Started {
            id: id.clone(),
            name,
            input_url,
        });
        id
    }

    pub async fn remove_stream(&self, id: &str) -> bool {
        let mut streams = self.streams.write().await;
        let len_before = streams.len();
        streams.retain(|s| s.id != id);
        let removed = streams.len() < len_before;
        if removed {
            let _ = self
                .event_tx
                .send(StreamEvent::Stopped { id: id.to_string() });
        }
        removed
    }

    pub async fn get_streams(&self) -> Vec<StreamInfo> {
        self.streams.read().await.clone()
    }

    pub async fn update_status(&self, id: &str, status: StreamStatus) {
        let mut streams = self.streams.write().await;
        if let Some(stream) = streams.iter_mut().find(|s| s.id == id) {
            stream.status = status;
        }
    }

    pub async fn update_stream_stats(&self, id: &str, viewers: u32, bitrate: u64) {
        let mut streams = self.streams.write().await;
        if let Some(stream) = streams.iter_mut().find(|s| s.id == id) {
            stream.viewers = viewers;
            stream.bitrate = bitrate;
            let _ = self.event_tx.send(StreamEvent::Updated {
                id: id.to_string(),
                viewers,
                bitrate,
            });
        }
    }

    pub async fn add_platform(&self, name: String, url: String, key: String) -> String {
        let id = Uuid::new_v4().to_string();
        let platform_id = platform_id_from(&url, &key);
        let platform = Platform {
            id: id.clone(),
            name,
            url: url.clone(),
            key: key.clone(),
            enabled: true,
        };
        self.platforms.write().await.push(platform);
        let _ = self.platform_event_tx.send(PlatformEvent::Added {
            platform_id,
            url,
            key,
        });
        id
    }

    pub async fn remove_platform(&self, id: &str) -> bool {
        let mut platforms = self.platforms.write().await;
        let platform = platforms.iter().find(|p| p.id == id);
        let platform_id = platform.map(|p| platform_id_from(&p.url, &p.key));
        let len_before = platforms.len();
        platforms.retain(|p| p.id != id);
        let removed = platforms.len() < len_before;
        if removed && let Some(pid) = platform_id {
            let _ = self
                .platform_event_tx
                .send(PlatformEvent::Removed { platform_id: pid });
        }
        removed
    }

    pub async fn get_platforms(&self) -> Vec<Platform> {
        self.platforms.read().await.clone()
    }

    pub async fn toggle_platform(&self, id: &str, enabled: bool) {
        let mut platforms = self.platforms.write().await;
        if let Some(platform) = platforms.iter_mut().find(|p| p.id == id) {
            platform.enabled = enabled;
            let pid = platform_id_from(&platform.url, &platform.key);
            let _ = self.platform_event_tx.send(PlatformEvent::Toggled {
                platform_id: pid,
                url: platform.url.clone(),
                key: platform.key.clone(),
                enabled,
            });
        }
    }

    pub async fn update_platform(
        &self,
        id: &str,
        name: Option<String>,
        url: Option<String>,
        key: Option<String>,
        enabled: Option<bool>,
    ) -> bool {
        let mut platforms = self.platforms.write().await;
        if let Some(platform) = platforms.iter_mut().find(|p| p.id == id) {
            let previous_platform_id = platform_id_from(&platform.url, &platform.key);
            let previous_enabled = platform.enabled;
            if let Some(n) = name {
                platform.name = n;
            }
            if let Some(u) = url {
                platform.url = u;
            }
            if let Some(k) = key {
                platform.key = k;
            }
            if let Some(e) = enabled {
                platform.enabled = e;
            }

            let current_platform_id = platform_id_from(&platform.url, &platform.key);
            let current_enabled = platform.enabled;
            let current_url = platform.url.clone();
            let current_key = platform.key.clone();
            let platform_id_changed = previous_platform_id != current_platform_id;
            drop(platforms);

            if platform_id_changed {
                // URL/key are part of the stable destination identity. Remove
                // the old push client before advertising the replacement so a
                // live publisher cannot keep sending to stale credentials.
                let _ = self.platform_event_tx.send(PlatformEvent::Removed {
                    platform_id: previous_platform_id,
                });
                if current_enabled {
                    let _ = self.platform_event_tx.send(PlatformEvent::Added {
                        platform_id: current_platform_id,
                        url: current_url,
                        key: current_key,
                    });
                }
            } else if enabled.is_some() {
                let _ = self.platform_event_tx.send(PlatformEvent::Toggled {
                    platform_id: current_platform_id,
                    url: current_url,
                    key: current_key,
                    enabled: current_enabled,
                });
            } else if previous_enabled != current_enabled {
                // This branch is defensive: enabled can only change through
                // the explicit option above, but keeps the event contract
                // correct if the update logic changes later.
                let _ = self.platform_event_tx.send(PlatformEvent::Toggled {
                    platform_id: current_platform_id,
                    url: current_url,
                    key: current_key,
                    enabled: current_enabled,
                });
            }
            true
        } else {
            false
        }
    }

    /// Apply a destination change coming from the versioned product API.
    /// Keeping this adapter here lets the RTMP publisher keep its existing
    /// event-driven reconnection behavior while the web API owns channel
    /// persistence.
    pub async fn apply_platform_event(&self, event: PlatformEvent) {
        match event {
            PlatformEvent::Added {
                platform_id,
                url,
                key,
            } => {
                let exists =
                    self.platforms.read().await.iter().any(|platform| {
                        platform_id_from(&platform.url, &platform.key) == platform_id
                    });
                if !exists {
                    self.add_platform(url_host_name(&url), url, key).await;
                }
            }
            PlatformEvent::Removed { platform_id } => {
                let id = self
                    .platforms
                    .read()
                    .await
                    .iter()
                    .find(|platform| platform_id_from(&platform.url, &platform.key) == platform_id)
                    .map(|platform| platform.id.clone());
                if let Some(id) = id {
                    self.remove_platform(&id).await;
                }
            }
            PlatformEvent::Toggled {
                platform_id,
                enabled,
                ..
            } => {
                let id = self
                    .platforms
                    .read()
                    .await
                    .iter()
                    .find(|platform| platform_id_from(&platform.url, &platform.key) == platform_id)
                    .map(|platform| platform.id.clone());
                if let Some(id) = id {
                    self.toggle_platform(&id, enabled).await;
                }
            }
        }
    }
}

fn url_host_name(url: &str) -> String {
    url::Url::parse(url)
        .ok()
        .and_then(|parsed| parsed.host_str().map(ToOwned::to_owned))
        .unwrap_or_else(|| "custom destination".into())
}

#[async_trait::async_trait]
impl reestream_core::client::StreamRegistrar for StreamManager {
    async fn register_stream(&self, name: String, input_url: String) -> String {
        self.add_stream(name, input_url).await
    }

    async fn unregister_stream(&self, id: &str) {
        self.remove_stream(id).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_add_and_get_streams() {
        let manager = StreamManager::new();
        let id = manager
            .add_stream("test".into(), "rtmp://input".into())
            .await;
        let streams = manager.get_streams().await;
        assert_eq!(streams.len(), 1);
        assert_eq!(streams[0].id, id);
        assert_eq!(streams[0].name, "test");
    }

    #[tokio::test]
    async fn test_remove_stream() {
        let manager = StreamManager::new();
        let id = manager
            .add_stream("test".into(), "rtmp://input".into())
            .await;
        assert!(manager.remove_stream(&id).await);
        assert!(manager.get_streams().await.is_empty());
    }

    #[tokio::test]
    async fn test_remove_nonexistent_stream() {
        let manager = StreamManager::new();
        assert!(!manager.remove_stream("nonexistent").await);
    }

    #[tokio::test]
    async fn test_update_status() {
        let manager = StreamManager::new();
        let id = manager
            .add_stream("test".into(), "rtmp://input".into())
            .await;
        manager.update_status(&id, StreamStatus::Live).await;
        let streams = manager.get_streams().await;
        assert_eq!(streams[0].status, StreamStatus::Live);
    }

    #[tokio::test]
    async fn test_add_and_get_platforms() {
        let manager = StreamManager::new();
        let id = manager
            .add_platform("Twitch".into(), "rtmp://twitch.tv".into(), "key".into())
            .await;
        let platforms = manager.get_platforms().await;
        assert_eq!(platforms.len(), 1);
        assert_eq!(platforms[0].id, id);
    }

    #[tokio::test]
    async fn test_remove_platform() {
        let manager = StreamManager::new();
        let id = manager
            .add_platform("Twitch".into(), "rtmp://twitch.tv".into(), "key".into())
            .await;
        assert!(manager.remove_platform(&id).await);
        assert!(manager.get_platforms().await.is_empty());
    }

    #[tokio::test]
    async fn test_toggle_platform() {
        let manager = StreamManager::new();
        let id = manager
            .add_platform("Twitch".into(), "rtmp://twitch.tv".into(), "key".into())
            .await;
        manager.toggle_platform(&id, false).await;
        let platforms = manager.get_platforms().await;
        assert!(!platforms[0].enabled);
    }

    #[test]
    fn test_stream_status_default() {
        assert_eq!(StreamStatus::default(), StreamStatus::Idle);
    }

    #[test]
    fn test_stream_status_serialize() {
        let status = StreamStatus::Live;
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("Live"));
    }

    #[tokio::test]
    async fn test_update_platform_name() {
        let manager = StreamManager::new();
        let id = manager
            .add_platform("Twitch".into(), "rtmp://twitch.tv".into(), "key".into())
            .await;
        let updated = manager
            .update_platform(&id, Some("New Name".into()), None, None, None)
            .await;
        assert!(updated);
        let platforms = manager.get_platforms().await;
        assert_eq!(platforms[0].name, "New Name");
        assert_eq!(platforms[0].url, "rtmp://twitch.tv");
    }

    #[tokio::test]
    async fn test_update_platform_url_and_key() {
        let manager = StreamManager::new();
        let id = manager
            .add_platform("Twitch".into(), "rtmp://twitch.tv".into(), "key".into())
            .await;
        let updated = manager
            .update_platform(
                &id,
                None,
                Some("rtmp://new.server/app".into()),
                Some("new-key".into()),
                None,
            )
            .await;
        assert!(updated);
        let platforms = manager.get_platforms().await;
        assert_eq!(platforms[0].url, "rtmp://new.server/app");
        assert_eq!(platforms[0].key, "new-key");
    }

    #[tokio::test]
    async fn test_update_platform_url_and_key_emits_remove_and_add() {
        let manager = StreamManager::new();
        let mut events = manager.subscribe_platform_events();
        let id = manager
            .add_platform("Twitch".into(), "rtmp://twitch.tv".into(), "key".into())
            .await;
        let _ = events.recv().await.unwrap();

        let old_platform_id = platform_id_from("rtmp://twitch.tv", "key");
        let new_platform_id = platform_id_from("rtmp://new.server/app", "new-key");
        assert!(
            manager
                .update_platform(
                    &id,
                    None,
                    Some("rtmp://new.server/app".into()),
                    Some("new-key".into()),
                    None,
                )
                .await
        );

        assert!(matches!(
            events.recv().await.unwrap(),
            PlatformEvent::Removed { platform_id } if platform_id == old_platform_id
        ));
        assert!(matches!(
            events.recv().await.unwrap(),
            PlatformEvent::Added { platform_id, .. } if platform_id == new_platform_id
        ));
    }

    #[tokio::test]
    async fn test_update_platform_enabled() {
        let manager = StreamManager::new();
        let id = manager
            .add_platform("Twitch".into(), "rtmp://twitch.tv".into(), "key".into())
            .await;
        let updated = manager
            .update_platform(&id, None, None, None, Some(false))
            .await;
        assert!(updated);
        let platforms = manager.get_platforms().await;
        assert!(!platforms[0].enabled);
    }

    #[tokio::test]
    async fn test_update_platform_all_fields() {
        let manager = StreamManager::new();
        let id = manager
            .add_platform("Twitch".into(), "rtmp://twitch.tv".into(), "key".into())
            .await;
        let updated = manager
            .update_platform(
                &id,
                Some("YouTube".into()),
                Some("rtmp://youtube.com/live2".into()),
                Some("yt-key".into()),
                Some(false),
            )
            .await;
        assert!(updated);
        let platforms = manager.get_platforms().await;
        assert_eq!(platforms[0].name, "YouTube");
        assert_eq!(platforms[0].url, "rtmp://youtube.com/live2");
        assert_eq!(platforms[0].key, "yt-key");
        assert!(!platforms[0].enabled);
    }

    #[tokio::test]
    async fn test_update_platform_not_found() {
        let manager = StreamManager::new();
        let updated = manager
            .update_platform("nonexistent", Some("test".into()), None, None, None)
            .await;
        assert!(!updated);
    }
}
