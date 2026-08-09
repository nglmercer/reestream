//! Local file-to-RTMP playback for scheduled and prerecorded events.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tokio::process::Command;
use tokio::sync::Mutex;
use tracing::{error, info};

type ChildHandle = Arc<Mutex<tokio::process::Child>>;

#[derive(Clone, Default)]
pub struct PlaybackManager {
    processes: Arc<Mutex<HashMap<String, ChildHandle>>>,
}

impl PlaybackManager {
    pub async fn start(
        &self,
        event_id: &str,
        source_path: &Path,
        output_url: &str,
        loops_count: u8,
    ) -> Result<(), String> {
        if !tokio::fs::metadata(source_path)
            .await
            .map(|metadata| metadata.is_file())
            .unwrap_or(false)
        {
            return Err("source file does not exist".into());
        }
        if self.processes.lock().await.contains_key(event_id) {
            return Ok(());
        }

        let mut args = vec![
            "-hide_banner".into(),
            "-loglevel".into(),
            "error".into(),
            "-re".into(),
        ];
        if loops_count > 0 {
            args.extend(["-stream_loop".into(), loops_count.to_string()]);
        }
        args.extend([
            "-i".into(),
            source_path.to_string_lossy().into_owned(),
            "-c".into(),
            "copy".into(),
            "-f".into(),
            "flv".into(),
            "-y".into(),
            output_url.to_string(),
        ]);
        let child = Command::new("ffmpeg")
            .args(args)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|error| format!("failed to start playback: {error}"))?;
        let child = Arc::new(Mutex::new(child));
        self.processes
            .lock()
            .await
            .insert(event_id.to_string(), child.clone());

        let processes = self.processes.clone();
        let event_id_owned = event_id.to_string();
        let source_path = source_path.to_path_buf();
        let output_url = output_url.to_string();
        tokio::spawn(async move {
            info!(
                event_id = %event_id_owned,
                source = %source_path.display(),
                output = %output_url,
                "file playback started"
            );
            loop {
                let status = {
                    let mut child = child.lock().await;
                    child.try_wait()
                };
                match status {
                    Ok(Some(status)) => {
                        if !status.success() {
                            error!(
                                event_id = %event_id_owned,
                                code = ?status.code(),
                                "file playback exited with an error"
                            );
                        }
                        processes.lock().await.remove(&event_id_owned);
                        break;
                    }
                    Ok(None) => {
                        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                    }
                    Err(error) => {
                        error!(event_id = %event_id_owned, %error, "file playback process error");
                        processes.lock().await.remove(&event_id_owned);
                        break;
                    }
                }
            }
        });
        Ok(())
    }

    pub async fn stop(&self, event_id: &str) -> Result<(), String> {
        let Some(child) = self.processes.lock().await.get(event_id).cloned() else {
            return Ok(());
        };
        let mut child = child.lock().await;
        if child
            .try_wait()
            .map_err(|error| format!("failed to inspect playback: {error}"))?
            .is_none()
        {
            child
                .kill()
                .await
                .map_err(|error| format!("failed to stop playback: {error}"))?;
        }
        Ok(())
    }

    pub async fn active_events(&self) -> Vec<String> {
        self.processes.lock().await.keys().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[tokio::test]
    async fn missing_source_is_rejected_before_spawning() {
        let manager = PlaybackManager::default();
        let result = manager
            .start(
                "event",
                &PathBuf::from("/definitely/missing/source.mp4"),
                "rtmp://localhost/live/key",
                0,
            )
            .await;
        assert!(result.is_err());
        assert!(manager.active_events().await.is_empty());
    }
}
