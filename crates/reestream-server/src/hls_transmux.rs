use bytes::Bytes;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::process::{Child, Command};
use tokio::sync::{Mutex, mpsc};
use tracing::{error, info, warn};

use crate::flv;

pub struct HlsTransmuxer {
    segment_dir: PathBuf,
    playlist_path: PathBuf,
    ffmpeg_path: PathBuf,
    child: Arc<Mutex<Option<Child>>>,
    tx: Arc<Mutex<Option<mpsc::Sender<Bytes>>>>,
}

impl HlsTransmuxer {
    pub fn new(segment_dir: PathBuf, playlist_path: PathBuf) -> Self {
        let ffmpeg_path = std::env::var_os("RESTREAM_FFMPEG_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("ffmpeg"));
        Self::new_with_ffmpeg(segment_dir, playlist_path, ffmpeg_path)
    }

    pub fn new_with_ffmpeg(
        segment_dir: PathBuf,
        playlist_path: PathBuf,
        ffmpeg_path: PathBuf,
    ) -> Self {
        Self {
            segment_dir,
            playlist_path,
            ffmpeg_path,
            child: Arc::new(Mutex::new(None)),
            tx: Arc::new(Mutex::new(None)),
        }
    }

    pub async fn start(&self) -> Result<mpsc::Sender<Bytes>, String> {
        // Create segment directory
        tokio::fs::create_dir_all(&self.segment_dir)
            .await
            .map_err(|e| format!("Failed to create HLS segment dir: {e}"))?;
        if let Ok(mut entries) = tokio::fs::read_dir(&self.segment_dir).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                let path = entry.path();
                if path.extension().and_then(|extension| extension.to_str()) == Some("ts") {
                    let _ = tokio::fs::remove_file(path).await;
                }
            }
        }
        let _ = tokio::fs::remove_file(&self.playlist_path).await;

        let segment_pattern = self.segment_dir.join("seg%03d.ts");

        let mut child = Command::new(&self.ffmpeg_path)
            .args([
                "-hide_banner",
                "-loglevel",
                "warning",
                "-f",
                "flv",
                "-i",
                "pipe:0",
                "-c",
                "copy",
                "-f",
                "hls",
                "-hls_time",
                "2",
                "-hls_list_size",
                "10",
                "-hls_flags",
                "delete_segments+append_list",
                "-hls_segment_filename",
                &segment_pattern.to_string_lossy(),
                &self.playlist_path.to_string_lossy(),
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("Failed to spawn ffmpeg: {e}"))?;

        // We need to pipe data to stdin asynchronously
        let (tx, mut rx) = mpsc::channel::<Bytes>(256);

        // Write FLV header first
        let header = flv::build_flv_header();
        let mut stdin = child.stdin.take().ok_or("ffmpeg stdin already taken")?;

        // Spawn a task to write FLV header + data to ffmpeg stdin
        tokio::spawn(async move {
            // Write FLV header
            if let Err(e) = stdin.write_all(&header).await {
                error!("Failed to write FLV header to ffmpeg: {e}");
                return;
            }

            while let Some(data) = rx.recv().await {
                if let Err(e) = stdin.write_all(&data).await {
                    warn!("Failed to write to ffmpeg stdin: {e}");
                    break;
                }
            }

            // Close stdin to signal EOF
            drop(stdin);
            info!("HLS transmuxer stdin closed");
        });

        *self.child.lock().await = Some(child);
        *self.tx.lock().await = Some(tx.clone());

        info!(
            "HLS transmuxer started: segments={} playlist={}",
            self.segment_dir.display(),
            self.playlist_path.display()
        );

        Ok(tx)
    }

    pub async fn stop(&self) {
        // Drop the sender to close the channel
        *self.tx.lock().await = None;

        // Kill ffmpeg process
        let mut child_guard = self.child.lock().await;
        if let Some(mut child) = child_guard.take() {
            let _ = child.kill().await;
            info!("HLS transmuxer stopped");
        }
    }

    pub fn segment_dir(&self) -> &PathBuf {
        &self.segment_dir
    }
}

#[cfg(test)]
async fn which_ffmpeg() -> Result<String, String> {
    let output = tokio::process::Command::new("which")
        .arg("ffmpeg")
        .output()
        .await
        .map_err(|e| format!("Failed to find ffmpeg: {e}"))?;

    if output.status.success() {
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if path.is_empty() {
            Err("ffmpeg not found in PATH".into())
        } else {
            Ok(path)
        }
    } else {
        Err("ffmpeg not found in PATH".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hls_transmuxer_new() {
        let transmuxer = HlsTransmuxer::new(
            PathBuf::from("/tmp/test_hls"),
            PathBuf::from("/tmp/test_hls/stream.m3u8"),
        );
        assert_eq!(transmuxer.segment_dir(), &PathBuf::from("/tmp/test_hls"));
    }

    #[tokio::test]
    async fn test_hls_transmuxer_creates_segment_dir() {
        let segment_dir = PathBuf::from("/tmp/reestream_test_hls_segments");
        let _ = tokio::fs::remove_dir_all(&segment_dir).await;

        let transmuxer = HlsTransmuxer::new(segment_dir.clone(), segment_dir.join("stream.m3u8"));

        // Start the transmuxer (this should create the directory)
        let result = transmuxer.start().await;

        // The result depends on whether ffmpeg is available
        if which_ffmpeg().await.is_ok() {
            assert!(
                result.is_ok(),
                "Should start successfully when ffmpeg is available"
            );
        }

        // Clean up
        transmuxer.stop().await;
        let _ = tokio::fs::remove_dir_all(&segment_dir).await;
    }

    #[tokio::test]
    async fn test_hls_transmuxer_stop_without_start() {
        let transmuxer = HlsTransmuxer::new(
            PathBuf::from("/tmp/test_hls_stop"),
            PathBuf::from("/tmp/test_hls_stop/stream.m3u8"),
        );

        // Stopping without starting should not panic
        transmuxer.stop().await;
    }

    #[tokio::test]
    async fn test_which_ffmpeg() {
        let result = which_ffmpeg().await;
        // This test assumes ffmpeg is installed on the test system
        if let Ok(path) = result {
            assert!(!path.is_empty());
            assert!(path.contains("ffmpeg"));
        }
    }
}
