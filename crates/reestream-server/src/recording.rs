use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordingConfig {
    pub enabled: bool,
    pub output_dir: PathBuf,
    pub format: RecordingFormat,
    pub segment_duration_secs: u64,
    pub max_file_size_mb: u64,
}

impl Default for RecordingConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            output_dir: PathBuf::from("/tmp/reestream/recordings"),
            format: RecordingFormat::Mp4,
            segment_duration_secs: 0,
            max_file_size_mb: 4096,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum RecordingFormat {
    Mp4,
    Flv,
    Mkv,
    Ts,
}

impl RecordingFormat {
    pub fn extension(&self) -> &'static str {
        match self {
            Self::Mp4 => "mp4",
            Self::Flv => "flv",
            Self::Mkv => "mkv",
            Self::Ts => "ts",
        }
    }

    pub fn ffmpeg_format(&self) -> &'static str {
        match self {
            Self::Mp4 => "mp4",
            Self::Flv => "flv",
            Self::Mkv => "matroska",
            Self::Ts => "mpegts",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct RecordingInfo {
    pub id: String,
    pub stream_id: String,
    pub filename: String,
    pub path: PathBuf,
    pub format: RecordingFormat,
    pub started_at: u64,
    pub size_bytes: u64,
    pub status: RecordingStatus,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum RecordingStatus {
    Recording,
    Stopped,
    Error,
}

pub struct RecordingManager {
    config: RecordingConfig,
    recordings: Arc<RwLock<Vec<RecordingInfo>>>,
}

impl RecordingManager {
    pub fn new(config: RecordingConfig) -> Self {
        Self {
            config,
            recordings: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub async fn start_recording(
        &self,
        stream_id: &str,
        input_url: &str,
    ) -> Result<String, String> {
        if !self.config.enabled {
            return Err("Recording is not enabled".into());
        }

        tokio::fs::create_dir_all(&self.config.output_dir)
            .await
            .map_err(|e| format!("Failed to create output dir: {e}"))?;

        let id = uuid::Uuid::new_v4().to_string();
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let ext = self.config.format.extension();
        let filename = format!("{stream_id}_{timestamp}.{ext}");
        let path = self.config.output_dir.join(&filename);

        let info = RecordingInfo {
            id: id.clone(),
            stream_id: stream_id.to_string(),
            filename,
            path: path.clone(),
            format: self.config.format.clone(),
            started_at: timestamp,
            size_bytes: 0,
            status: RecordingStatus::Recording,
        };

        self.recordings.write().await.push(info);

        let ffmpeg_args = self.build_ffmpeg_args(input_url, &path);
        let recordings = self.recordings.clone();
        let rec_id = id.clone();
        let input_owned = input_url.to_string();
        let path_owned = path.clone();

        tokio::spawn(async move {
            match tokio::process::Command::new("ffmpeg")
                .args(&ffmpeg_args)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::piped())
                .spawn()
            {
                Ok(mut child) => {
                    info!(
                        "Recording started: {} -> {}",
                        input_owned,
                        path_owned.display()
                    );
                    let status = child.wait().await;
                    let mut recs = recordings.write().await;
                    if let Some(rec) = recs.iter_mut().find(|r| r.id == rec_id) {
                        match status {
                            Ok(s) if s.success() => {
                                rec.status = RecordingStatus::Stopped;
                                info!("Recording stopped: {}", rec.filename);
                            }
                            Ok(s) => {
                                rec.status = RecordingStatus::Error;
                                error!("Recording failed with code {}", s.code().unwrap_or(-1));
                            }
                            Err(e) => {
                                rec.status = RecordingStatus::Error;
                                error!("Recording process error: {}", e);
                            }
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to start recording: {}", e);
                    let mut recs = recordings.write().await;
                    if let Some(rec) = recs.iter_mut().find(|r| r.id == rec_id) {
                        rec.status = RecordingStatus::Error;
                    }
                }
            }
        });

        Ok(id)
    }

    pub async fn stop_recording(&self, id: &str) -> Result<(), String> {
        let mut recs = self.recordings.write().await;
        if let Some(rec) = recs.iter_mut().find(|r| r.id == id) {
            rec.status = RecordingStatus::Stopped;
            info!("Recording marked as stopped: {}", rec.filename);
            Ok(())
        } else {
            Err("Recording not found".into())
        }
    }

    pub async fn list_recordings(&self) -> Vec<RecordingInfo> {
        self.recordings.read().await.clone()
    }

    pub async fn get_recording(&self, id: &str) -> Option<RecordingInfo> {
        self.recordings
            .read()
            .await
            .iter()
            .find(|r| r.id == id)
            .cloned()
    }

    pub async fn delete_recording(&self, id: &str) -> Result<(), String> {
        let mut recs = self.recordings.write().await;
        if let Some(idx) = recs.iter().position(|r| r.id == id) {
            let rec = recs.remove(idx);
            if rec.path.exists() {
                tokio::fs::remove_file(&rec.path)
                    .await
                    .map_err(|e| format!("Failed to delete file: {e}"))?;
            }
            Ok(())
        } else {
            Err("Recording not found".into())
        }
    }

    fn build_ffmpeg_args(&self, input_url: &str, output_path: &std::path::Path) -> Vec<String> {
        let mut args = vec![
            "-i".to_string(),
            input_url.to_string(),
            "-c".to_string(),
            "copy".to_string(),
        ];

        if self.config.segment_duration_secs > 0 {
            args.extend([
                "-f".to_string(),
                "segment".to_string(),
                "-segment_time".to_string(),
                self.config.segment_duration_secs.to_string(),
                "-reset_timestamps".to_string(),
                "1".to_string(),
            ]);
        }

        args.push("-y".to_string());
        args.push(output_path.to_string_lossy().to_string());
        args
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_recording_config_default() {
        let config = RecordingConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.format, RecordingFormat::Mp4);
    }

    #[test]
    fn test_recording_format_extension() {
        assert_eq!(RecordingFormat::Mp4.extension(), "mp4");
        assert_eq!(RecordingFormat::Flv.extension(), "flv");
        assert_eq!(RecordingFormat::Mkv.extension(), "mkv");
        assert_eq!(RecordingFormat::Ts.extension(), "ts");
    }

    #[test]
    fn test_recording_format_ffmpeg() {
        assert_eq!(RecordingFormat::Mp4.ffmpeg_format(), "mp4");
        assert_eq!(RecordingFormat::Flv.ffmpeg_format(), "flv");
        assert_eq!(RecordingFormat::Mkv.ffmpeg_format(), "matroska");
        assert_eq!(RecordingFormat::Ts.ffmpeg_format(), "mpegts");
    }

    #[tokio::test]
    async fn test_recording_manager_list_empty() {
        let manager = RecordingManager::new(RecordingConfig::default());
        assert!(manager.list_recordings().await.is_empty());
    }

    #[tokio::test]
    async fn test_recording_manager_not_enabled() {
        let manager = RecordingManager::new(RecordingConfig::default());
        let result = manager.start_recording("stream1", "rtmp://input").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_recording_manager_stop_not_found() {
        let manager = RecordingManager::new(RecordingConfig::default());
        assert!(manager.stop_recording("nonexistent").await.is_err());
    }

    #[tokio::test]
    async fn test_recording_manager_delete_not_found() {
        let manager = RecordingManager::new(RecordingConfig::default());
        assert!(manager.delete_recording("nonexistent").await.is_err());
    }
}
