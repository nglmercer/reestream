use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{RwLock, mpsc};
use tracing::info;
use uuid::Uuid;

use crate::pipeline::{
    PipelineEvent, PipelineInfo, PipelineManager, PipelineStats, PipelineStatus, StreamPipeline,
};

pub struct RtmpPipeline {
    name: String,
    #[allow(dead_code)]
    input_url: String,
    #[allow(dead_code)]
    outputs: Vec<String>,
    status: PipelineStatus,
    stats: PipelineStats,
    start_time: Option<std::time::Instant>,
    event_tx: mpsc::Sender<PipelineEvent>,
}

impl RtmpPipeline {
    pub fn new(
        name: String,
        input_url: String,
        outputs: Vec<String>,
        event_tx: mpsc::Sender<PipelineEvent>,
    ) -> Self {
        Self {
            name,
            input_url,
            outputs,
            status: PipelineStatus::Idle,
            stats: PipelineStats::default(),
            start_time: None,
            event_tx,
        }
    }
}

#[async_trait]
impl StreamPipeline for RtmpPipeline {
    fn name(&self) -> &str {
        &self.name
    }

    fn status(&self) -> PipelineStatus {
        self.status.clone()
    }

    fn stats(&self) -> PipelineStats {
        let mut stats = self.stats.clone();
        if let Some(start) = self.start_time {
            stats.uptime_secs = start.elapsed().as_secs();
        }
        stats
    }

    async fn start(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("Starting pipeline: {}", self.name);
        self.status = PipelineStatus::Running;
        self.start_time = Some(std::time::Instant::now());
        let _ = self.event_tx.send(PipelineEvent::Started).await;
        Ok(())
    }

    async fn stop(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("Stopping pipeline: {}", self.name);
        self.status = PipelineStatus::Stopped;
        self.start_time = None;
        let _ = self.event_tx.send(PipelineEvent::Stopped).await;
        Ok(())
    }
}

pub struct SrtPipeline {
    name: String,
    #[allow(dead_code)]
    input_url: String,
    #[allow(dead_code)]
    outputs: Vec<String>,
    status: PipelineStatus,
    stats: PipelineStats,
    start_time: Option<std::time::Instant>,
    event_tx: mpsc::Sender<PipelineEvent>,
}

impl SrtPipeline {
    pub fn new(
        name: String,
        input_url: String,
        outputs: Vec<String>,
        event_tx: mpsc::Sender<PipelineEvent>,
    ) -> Self {
        Self {
            name,
            input_url,
            outputs,
            status: PipelineStatus::Idle,
            stats: PipelineStats::default(),
            start_time: None,
            event_tx,
        }
    }
}

#[async_trait]
impl StreamPipeline for SrtPipeline {
    fn name(&self) -> &str {
        &self.name
    }

    fn status(&self) -> PipelineStatus {
        self.status.clone()
    }

    fn stats(&self) -> PipelineStats {
        let mut stats = self.stats.clone();
        if let Some(start) = self.start_time {
            stats.uptime_secs = start.elapsed().as_secs();
        }
        stats
    }

    async fn start(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("Starting SRT pipeline: {}", self.name);
        self.status = PipelineStatus::Running;
        self.start_time = Some(std::time::Instant::now());
        let _ = self.event_tx.send(PipelineEvent::Started).await;
        Ok(())
    }

    async fn stop(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("Stopping SRT pipeline: {}", self.name);
        self.status = PipelineStatus::Stopped;
        self.start_time = None;
        let _ = self.event_tx.send(PipelineEvent::Stopped).await;
        Ok(())
    }
}

pub struct FilePipeline {
    name: String,
    #[allow(dead_code)]
    file_path: String,
    #[allow(dead_code)]
    outputs: Vec<String>,
    status: PipelineStatus,
    stats: PipelineStats,
    start_time: Option<std::time::Instant>,
    event_tx: mpsc::Sender<PipelineEvent>,
}

impl FilePipeline {
    pub fn new(
        name: String,
        file_path: String,
        outputs: Vec<String>,
        event_tx: mpsc::Sender<PipelineEvent>,
    ) -> Self {
        Self {
            name,
            file_path,
            outputs,
            status: PipelineStatus::Idle,
            stats: PipelineStats::default(),
            start_time: None,
            event_tx,
        }
    }
}

#[async_trait]
impl StreamPipeline for FilePipeline {
    fn name(&self) -> &str {
        &self.name
    }

    fn status(&self) -> PipelineStatus {
        self.status.clone()
    }

    fn stats(&self) -> PipelineStats {
        let mut stats = self.stats.clone();
        if let Some(start) = self.start_time {
            stats.uptime_secs = start.elapsed().as_secs();
        }
        stats
    }

    async fn start(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!(
            "Starting file pipeline: {} from {}",
            self.name, self.file_path
        );
        self.status = PipelineStatus::Running;
        self.start_time = Some(std::time::Instant::now());
        let _ = self.event_tx.send(PipelineEvent::Started).await;
        Ok(())
    }

    async fn stop(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("Stopping file pipeline: {}", self.name);
        self.status = PipelineStatus::Stopped;
        self.start_time = None;
        let _ = self.event_tx.send(PipelineEvent::Stopped).await;
        Ok(())
    }
}

pub enum InputType {
    Rtmp,
    Srt,
    File,
}

pub fn detect_input_type(input: &str) -> InputType {
    if input.starts_with("rtmp://") || input.starts_with("rtmps://") {
        InputType::Rtmp
    } else if input.starts_with("srt://") {
        InputType::Srt
    } else {
        InputType::File
    }
}

pub struct DefaultPipelineManager {
    pipelines: Arc<RwLock<HashMap<String, Box<dyn StreamPipeline>>>>,
    event_tx: mpsc::Sender<PipelineEvent>,
}

impl DefaultPipelineManager {
    pub fn new() -> (Self, mpsc::Receiver<PipelineEvent>) {
        let (event_tx, event_rx) = mpsc::channel(256);
        (
            Self {
                pipelines: Arc::new(RwLock::new(HashMap::new())),
                event_tx,
            },
            event_rx,
        )
    }
}

#[async_trait]
impl PipelineManager for DefaultPipelineManager {
    async fn create_pipeline(
        &self,
        name: String,
        input: String,
        outputs: Vec<String>,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let id = Uuid::new_v4().to_string();

        let pipeline: Box<dyn StreamPipeline> = match detect_input_type(&input) {
            InputType::Rtmp => Box::new(RtmpPipeline::new(
                name.clone(),
                input.clone(),
                outputs.clone(),
                self.event_tx.clone(),
            )),
            InputType::Srt => Box::new(SrtPipeline::new(
                name.clone(),
                input.clone(),
                outputs.clone(),
                self.event_tx.clone(),
            )),
            InputType::File => Box::new(FilePipeline::new(
                name.clone(),
                input.clone(),
                outputs.clone(),
                self.event_tx.clone(),
            )),
        };

        let mut pipelines = self.pipelines.write().await;
        pipelines.insert(id.clone(), pipeline);
        info!("Created pipeline {} (id={})", name, id);
        Ok(id)
    }

    async fn remove_pipeline(
        &self,
        id: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut pipelines = self.pipelines.write().await;
        if let Some(mut pipeline) = pipelines.remove(id) {
            pipeline.stop().await?;
            info!("Removed pipeline {}", id);
            Ok(())
        } else {
            Err(format!("Pipeline {id} not found").into())
        }
    }

    async fn list_pipelines(&self) -> Vec<PipelineInfo> {
        let pipelines = self.pipelines.read().await;
        pipelines
            .iter()
            .map(|(id, p)| PipelineInfo {
                id: id.clone(),
                name: p.name().to_string(),
                input: String::new(),
                outputs: vec![],
                status: p.status(),
                stats: p.stats(),
            })
            .collect()
    }

    async fn get_pipeline(&self, id: &str) -> Option<Box<dyn StreamPipeline>> {
        let pipelines = self.pipelines.read().await;
        pipelines.get(id).map(|_| {
            // We can't easily clone trait objects, so return None
            // In practice, callers should use list_pipelines() or specific accessors
            None
        })?
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_input_type_rtmp() {
        assert!(matches!(
            detect_input_type("rtmp://live.twitch.tv/app"),
            InputType::Rtmp
        ));
    }

    #[test]
    fn test_detect_input_type_rtmps() {
        assert!(matches!(
            detect_input_type("rtmps://live-api-s.facebook.com:443/rtmp/"),
            InputType::Rtmp
        ));
    }

    #[test]
    fn test_detect_input_type_srt() {
        assert!(matches!(
            detect_input_type("srt://0.0.0.0:3000"),
            InputType::Srt
        ));
    }

    #[test]
    fn test_detect_input_type_file() {
        assert!(matches!(
            detect_input_type("/tmp/video.mp4"),
            InputType::File
        ));
    }

    #[tokio::test]
    async fn test_create_rtmp_pipeline() {
        let (manager, _rx) = DefaultPipelineManager::new();
        let id = manager
            .create_pipeline(
                "test".into(),
                "rtmp://input".into(),
                vec!["rtmp://output".into()],
            )
            .await
            .unwrap();
        assert!(!id.is_empty());
        let pipelines = manager.list_pipelines().await;
        assert_eq!(pipelines.len(), 1);
        assert_eq!(pipelines[0].name, "test");
    }

    #[tokio::test]
    async fn test_create_srt_pipeline() {
        let (manager, _rx) = DefaultPipelineManager::new();
        let id = manager
            .create_pipeline(
                "srt-test".into(),
                "srt://0.0.0.0:3000".into(),
                vec!["rtmp://output".into()],
            )
            .await
            .unwrap();
        assert!(!id.is_empty());
    }

    #[tokio::test]
    async fn test_create_file_pipeline() {
        let (manager, _rx) = DefaultPipelineManager::new();
        let id = manager
            .create_pipeline(
                "file-test".into(),
                "/tmp/video.mp4".into(),
                vec!["rtmp://output".into()],
            )
            .await
            .unwrap();
        assert!(!id.is_empty());
    }

    #[tokio::test]
    async fn test_remove_pipeline() {
        let (manager, _rx) = DefaultPipelineManager::new();
        let id = manager
            .create_pipeline("test".into(), "rtmp://input".into(), vec![])
            .await
            .unwrap();
        assert!(manager.remove_pipeline(&id).await.is_ok());
        assert!(manager.list_pipelines().await.is_empty());
    }

    #[tokio::test]
    async fn test_remove_nonexistent_pipeline() {
        let (manager, _rx) = DefaultPipelineManager::new();
        assert!(manager.remove_pipeline("nonexistent").await.is_err());
    }

    #[tokio::test]
    async fn test_rtmp_pipeline_lifecycle() {
        let (tx, _rx) = mpsc::channel(10);
        let mut pipeline = RtmpPipeline::new("test".into(), "rtmp://input".into(), vec![], tx);
        assert_eq!(pipeline.status(), PipelineStatus::Idle);
        pipeline.start().await.unwrap();
        assert_eq!(pipeline.status(), PipelineStatus::Running);
        pipeline.stop().await.unwrap();
        assert_eq!(pipeline.status(), PipelineStatus::Stopped);
    }

    #[tokio::test]
    async fn test_pipeline_stats() {
        let (tx, _rx) = mpsc::channel(10);
        let mut pipeline = RtmpPipeline::new("test".into(), "rtmp://input".into(), vec![], tx);
        pipeline.start().await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        let stats = pipeline.stats();
        assert!(stats.uptime_secs <= 1);
    }
}
