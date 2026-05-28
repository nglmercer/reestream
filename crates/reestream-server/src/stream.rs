use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
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

#[derive(Default)]
pub struct StreamManager {
    streams: Arc<RwLock<Vec<StreamInfo>>>,
    platforms: Arc<RwLock<Vec<Platform>>>,
}

impl StreamManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn add_stream(&self, name: String, input_url: String) -> String {
        let id = Uuid::new_v4().to_string();
        let stream = StreamInfo {
            id: id.clone(),
            name,
            input_url,
            status: StreamStatus::Idle,
            started_at: None,
            viewers: 0,
            bitrate: 0,
        };
        self.streams.write().await.push(stream);
        id
    }

    pub async fn remove_stream(&self, id: &str) -> bool {
        let mut streams = self.streams.write().await;
        let len_before = streams.len();
        streams.retain(|s| s.id != id);
        streams.len() < len_before
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

    pub async fn add_platform(&self, name: String, url: String, key: String) -> String {
        let id = Uuid::new_v4().to_string();
        let platform = Platform {
            id: id.clone(),
            name,
            url,
            key,
            enabled: true,
        };
        self.platforms.write().await.push(platform);
        id
    }

    pub async fn remove_platform(&self, id: &str) -> bool {
        let mut platforms = self.platforms.write().await;
        let len_before = platforms.len();
        platforms.retain(|p| p.id != id);
        platforms.len() < len_before
    }

    pub async fn get_platforms(&self) -> Vec<Platform> {
        self.platforms.read().await.clone()
    }

    pub async fn toggle_platform(&self, id: &str, enabled: bool) {
        let mut platforms = self.platforms.write().await;
        if let Some(platform) = platforms.iter_mut().find(|p| p.id == id) {
            platform.enabled = enabled;
        }
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
}
