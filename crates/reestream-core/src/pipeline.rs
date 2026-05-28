use async_trait::async_trait;
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub enum PipelineStatus {
    #[default]
    Idle,
    Running,
    Error(String),
    Stopped,
}

impl fmt::Display for PipelineStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Idle => write!(f, "idle"),
            Self::Running => write!(f, "running"),
            Self::Error(msg) => write!(f, "error: {msg}"),
            Self::Stopped => write!(f, "stopped"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineStats {
    pub bytes_in: u64,
    pub bytes_out: u64,
    pub uptime_secs: u64,
    pub viewers: u32,
    pub bitrate_kbps: u32,
    pub fps: f64,
}

impl Default for PipelineStats {
    fn default() -> Self {
        Self {
            bytes_in: 0,
            bytes_out: 0,
            uptime_secs: 0,
            viewers: 0,
            bitrate_kbps: 0,
            fps: 0.0,
        }
    }
}

#[derive(Debug, Clone)]
pub enum PipelineEvent {
    Started,
    Stopped,
    Error(String),
    ViewerConnected,
    ViewerDisconnected,
    DataReceived(Bytes),
}

#[async_trait]
pub trait StreamPipeline: Send + Sync {
    fn name(&self) -> &str;
    fn status(&self) -> PipelineStatus;
    fn stats(&self) -> PipelineStats;

    async fn start(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
    async fn stop(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
    async fn restart(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.stop().await?;
        self.start().await
    }
}

#[async_trait]
pub trait PipelineManager: Send + Sync {
    async fn create_pipeline(
        &self,
        name: String,
        input: String,
        outputs: Vec<String>,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>>;

    async fn remove_pipeline(&self, id: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;

    async fn list_pipelines(&self) -> Vec<PipelineInfo>;

    async fn get_pipeline(&self, id: &str) -> Option<Box<dyn StreamPipeline>>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineInfo {
    pub id: String,
    pub name: String,
    pub input: String,
    pub outputs: Vec<String>,
    pub status: PipelineStatus,
    pub stats: PipelineStats,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pipeline_status_display() {
        assert_eq!(PipelineStatus::Idle.to_string(), "idle");
        assert_eq!(PipelineStatus::Running.to_string(), "running");
        assert_eq!(
            PipelineStatus::Error("test".into()).to_string(),
            "error: test"
        );
        assert_eq!(PipelineStatus::Stopped.to_string(), "stopped");
    }

    #[test]
    fn test_pipeline_status_default() {
        assert_eq!(PipelineStatus::default(), PipelineStatus::Idle);
    }

    #[test]
    fn test_pipeline_status_serialize() {
        let status = PipelineStatus::Running;
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("Running"));
    }

    #[test]
    fn test_pipeline_stats_default() {
        let stats = PipelineStats::default();
        assert_eq!(stats.bytes_in, 0);
        assert_eq!(stats.bytes_out, 0);
        assert_eq!(stats.viewers, 0);
    }

    #[test]
    fn test_pipeline_stats_serialize() {
        let stats = PipelineStats {
            bytes_in: 1024,
            bytes_out: 2048,
            uptime_secs: 60,
            viewers: 5,
            bitrate_kbps: 2500,
            fps: 30.0,
        };
        let json = serde_json::to_string(&stats).unwrap();
        assert!(json.contains("1024"));
        assert!(json.contains("2500"));
    }

    #[test]
    fn test_pipeline_info_serialize() {
        let info = PipelineInfo {
            id: "test-id".into(),
            name: "test".into(),
            input: "rtmp://input".into(),
            outputs: vec!["rtmp://output".into()],
            status: PipelineStatus::Running,
            stats: PipelineStats::default(),
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("test-id"));
    }

    #[test]
    fn test_pipeline_event_variants() {
        let events = [
            PipelineEvent::Started,
            PipelineEvent::Stopped,
            PipelineEvent::Error("test".into()),
            PipelineEvent::ViewerConnected,
            PipelineEvent::ViewerDisconnected,
            PipelineEvent::DataReceived(Bytes::from_static(&[0x17, 0x00])),
        ];
        assert_eq!(events.len(), 6);
    }
}
