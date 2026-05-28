use std::path::PathBuf;
use std::process::ExitStatus;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::{Child, Command};
use tracing::{error, info, warn};

use crate::command::FfmpegCommand;
use crate::error::FfmpegError;

pub struct FfmpegProcess {
    child: Child,
    stderr_buf: Vec<u8>,
}

impl FfmpegProcess {
    pub fn spawn(cmd: &FfmpegCommand) -> Result<Self, FfmpegError> {
        let args = cmd.build_args();
        info!("Spawning FFmpeg: {} {}", cmd.ffmpeg_path.display(), args.join(" "));

        let mut command = Command::new(&cmd.ffmpeg_path);
        command.args(&args);
        command.stdin(std::process::Stdio::null());
        command.stdout(std::process::Stdio::piped());
        command.stderr(std::process::Stdio::piped());

        let child = command.spawn().map_err(FfmpegError::ProcessStartFailed)?;

        Ok(Self {
            child,
            stderr_buf: Vec::new(),
        })
    }

    pub async fn wait(&mut self) -> Result<ExitStatus, FfmpegError> {
        let status = self.child.wait().await?;

        if let Some(mut stderr) = self.child.stderr.take() {
            let mut buf = vec![0u8; 4096];
            loop {
                match stderr.read(&mut buf).await {
                    Ok(0) | Err(_) => break,
                    Ok(n) => self.stderr_buf.extend_from_slice(&buf[..n]),
                }
            }
        }

        Ok(status)
    }

    pub async fn wait_success(&mut self) -> Result<(), FfmpegError> {
        let status = self.wait().await?;
        if !status.success() {
            let stderr = String::from_utf8_lossy(&self.stderr_buf).to_string();
            let exit_code = status.code().unwrap_or(-1);
            error!("FFmpeg failed with code {}: {}", exit_code, stderr);
            return Err(FfmpegError::ProcessFailed { exit_code, stderr });
        }
        Ok(())
    }

    pub fn stderr_output(&self) -> String {
        String::from_utf8_lossy(&self.stderr_buf).to_string()
    }

    pub fn kill(&mut self) {
        if let Err(e) = self.child.start_kill() {
            warn!("Failed to kill FFmpeg process: {}", e);
        }
    }

    pub fn is_running(&mut self) -> bool {
        match self.child.try_wait() {
            Ok(Some(_)) => false,
            Ok(None) => true,
            Err(_) => false,
        }
    }

    pub async fn stdin_write(&mut self, data: &[u8]) -> Result<(), FfmpegError> {
        if let Some(ref mut stdin) = self.child.stdin {
            stdin.write_all(data).await?;
            Ok(())
        } else {
            Err(FfmpegError::InvalidArgument(
                "No stdin available (process not started with piped stdin)".into(),
            ))
        }
    }
}

impl Drop for FfmpegProcess {
    fn drop(&mut self) {
        self.kill();
    }
}

pub struct FfmpegSupervisor {
    ffmpeg_path: PathBuf,
    args: Vec<String>,
    max_restarts: u32,
    restart_delay_ms: u64,
}

impl FfmpegSupervisor {
    pub fn new(ffmpeg_path: PathBuf, args: Vec<String>) -> Self {
        Self {
            ffmpeg_path,
            args,
            max_restarts: 5,
            restart_delay_ms: 2000,
        }
    }

    pub fn max_restarts(mut self, max: u32) -> Self {
        self.max_restarts = max;
        self
    }

    pub fn restart_delay(mut self, ms: u64) -> Self {
        self.restart_delay_ms = ms;
        self
    }

    pub async fn run_with_restart(&self) -> Result<(), FfmpegError> {
        let mut restarts = 0;

        loop {
            info!(
                "Starting FFmpeg (attempt {}/{})",
                restarts + 1,
                self.max_restarts + 1
            );

            let mut command = Command::new(&self.ffmpeg_path);
            command.args(&self.args);
            command.stdin(std::process::Stdio::null());
            command.stdout(std::process::Stdio::null());
            command.stderr(std::process::Stdio::piped());

            let mut child = command.spawn().map_err(FfmpegError::ProcessStartFailed)?;
            let status = child.wait().await?;

            if status.success() {
                info!("FFmpeg exited successfully");
                return Ok(());
            }

            let exit_code = status.code().unwrap_or(-1);
            warn!("FFmpeg exited with code {}", exit_code);

            restarts += 1;
            if restarts > self.max_restarts {
                error!("FFmpeg exceeded max restarts ({}), giving up", self.max_restarts);
                return Err(FfmpegError::ProcessFailed {
                    exit_code,
                    stderr: "Max restarts exceeded".into(),
                });
            }

            info!(
                "Restarting FFmpeg in {}ms (restart {}/{})",
                self.restart_delay_ms, restarts, self.max_restarts
            );
            tokio::time::sleep(tokio::time::Duration::from_millis(self.restart_delay_ms)).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::{FfmpegCommand, InputSource};
    use std::path::PathBuf;

    #[test]
    fn test_supervisor_config() {
        let supervisor = FfmpegSupervisor::new(
            PathBuf::from("ffmpeg"),
            vec!["-i".into(), "test".into()],
        )
        .max_restarts(3)
        .restart_delay(1000);

        assert_eq!(supervisor.max_restarts, 3);
        assert_eq!(supervisor.restart_delay_ms, 1000);
    }

    #[test]
    fn test_supervisor_default_config() {
        let supervisor = FfmpegSupervisor::new(
            PathBuf::from("ffmpeg"),
            vec![],
        );
        assert_eq!(supervisor.max_restarts, 5);
        assert_eq!(supervisor.restart_delay_ms, 2000);
    }

    #[tokio::test]
    async fn test_process_spawn_nonexistent() {
        let cmd = FfmpegCommand::new(
            PathBuf::from("/nonexistent/ffmpeg"),
            InputSource::Pipe,
        );
        let result = FfmpegProcess::spawn(&cmd);
        assert!(result.is_err());
        let err_msg = result.err().unwrap().to_string();
        assert!(err_msg.contains("Failed to start") || err_msg.contains("No such file"));
    }

    #[tokio::test]
    async fn test_supervisor_run_nonexistent() {
        let supervisor = FfmpegSupervisor::new(
            PathBuf::from("/nonexistent/ffmpeg"),
            vec![],
        )
        .max_restarts(0);
        let result = supervisor.run_with_restart().await;
        assert!(result.is_err());
    }
}
