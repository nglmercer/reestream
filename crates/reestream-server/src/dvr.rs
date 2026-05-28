use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DvrConfig {
    pub enabled: bool,
    pub buffer_duration_secs: u64,
    pub storage_dir: PathBuf,
    pub max_storage_mb: u64,
    pub segment_duration_secs: u64,
}

impl Default for DvrConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            buffer_duration_secs: 7200,
            storage_dir: PathBuf::from("/tmp/reestream/dvr"),
            max_storage_mb: 10240,
            segment_duration_secs: 6,
        }
    }
}

pub struct DvrBuffer {
    config: DvrConfig,
    segments: Arc<RwLock<Vec<DvrSegment>>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DvrSegment {
    pub index: u64,
    pub filename: String,
    pub start_time: u64,
    pub duration: f64,
    pub size_bytes: u64,
}

impl DvrBuffer {
    pub fn new(config: DvrConfig) -> Self {
        Self {
            config,
            segments: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub async fn add_segment(&self, segment: DvrSegment) {
        let mut segments = self.segments.write().await;
        segments.push(segment);

        let max_segments =
            (self.config.buffer_duration_secs / self.config.segment_duration_secs) as usize;
        if segments.len() > max_segments {
            let excess = segments.len() - max_segments;
            segments.drain(..excess);
        }
    }

    pub async fn get_segments(&self) -> Vec<DvrSegment> {
        self.segments.read().await.clone()
    }

    pub async fn get_segment_count(&self) -> usize {
        self.segments.read().await.len()
    }

    pub async fn clear(&self) {
        self.segments.write().await.clear();
        info!("DVR buffer cleared");
    }

    pub fn build_ffmpeg_dvr_args(&self, input: &str) -> Vec<String> {
        vec![
            "-i".into(),
            input.into(),
            "-c".into(),
            "copy".into(),
            "-f".into(),
            "segment".into(),
            "-segment_time".into(),
            self.config.segment_duration_secs.to_string(),
            "-segment_format".into(),
            "mpegts".into(),
            "-strftime".into(),
            "1".into(),
            self.config
                .storage_dir
                .join("dvr_%Y%m%d_%H%M%S.ts")
                .to_string_lossy()
                .to_string(),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dvr_config_default() {
        let config = DvrConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.buffer_duration_secs, 7200);
        assert_eq!(config.segment_duration_secs, 6);
    }

    #[tokio::test]
    async fn test_dvr_buffer_add_and_get() {
        let buffer = DvrBuffer::new(DvrConfig {
            buffer_duration_secs: 60,
            segment_duration_secs: 6,
            ..Default::default()
        });

        buffer
            .add_segment(DvrSegment {
                index: 0,
                filename: "seg0.ts".into(),
                start_time: 0,
                duration: 6.0,
                size_bytes: 1024,
            })
            .await;

        assert_eq!(buffer.get_segment_count().await, 1);
    }

    #[tokio::test]
    async fn test_dvr_buffer_trim() {
        let buffer = DvrBuffer::new(DvrConfig {
            buffer_duration_secs: 18,
            segment_duration_secs: 6,
            ..Default::default()
        });

        for i in 0..5 {
            buffer
                .add_segment(DvrSegment {
                    index: i,
                    filename: format!("seg{i}.ts"),
                    start_time: i * 6,
                    duration: 6.0,
                    size_bytes: 1024,
                })
                .await;
        }

        assert_eq!(buffer.get_segment_count().await, 3);
    }

    #[tokio::test]
    async fn test_dvr_buffer_clear() {
        let buffer = DvrBuffer::new(DvrConfig::default());
        buffer
            .add_segment(DvrSegment {
                index: 0,
                filename: "seg0.ts".into(),
                start_time: 0,
                duration: 6.0,
                size_bytes: 1024,
            })
            .await;
        buffer.clear().await;
        assert_eq!(buffer.get_segment_count().await, 0);
    }

    #[test]
    fn test_dvr_ffmpeg_args() {
        let buffer = DvrBuffer::new(DvrConfig::default());
        let args = buffer.build_ffmpeg_dvr_args("rtmp://input");
        assert!(args.contains(&"-f".to_string()));
        assert!(args.contains(&"segment".to_string()));
        assert!(args.iter().any(|a| a.contains("dvr_")));
    }
}
