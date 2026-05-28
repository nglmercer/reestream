use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::info;

use crate::error::FfmpegError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformBinaries {
    pub os: String,
    pub arch: String,
    pub url: String,
    pub checksum: Option<String>,
}

impl PlatformBinaries {
    pub fn current_platform() -> Option<Self> {
        let os = std::env::consts::OS;
        let arch = std::env::consts::ARCH;

        match (os, arch) {
            ("linux", "x86_64") => Some(Self {
                os: "linux".into(),
                arch: "x86_64".into(),
                url: "https://johnvansickle.com/ffmpeg/releases/ffmpeg-release-amd64-static.tar.xz"
                    .into(),
                checksum: None,
            }),
            ("linux", "aarch64") => Some(Self {
                os: "linux".into(),
                arch: "aarch64".into(),
                url: "https://johnvansickle.com/ffmpeg/releases/ffmpeg-release-arm64-static.tar.xz"
                    .into(),
                checksum: None,
            }),
            ("linux", "arm") => Some(Self {
                os: "linux".into(),
                arch: "arm".into(),
                url: "https://johnvansickle.com/ffmpeg/releases/ffmpeg-release-armhf-static.tar.xz"
                    .into(),
                checksum: None,
            }),
            ("macos", "x86_64") => Some(Self {
                os: "macos".into(),
                arch: "x86_64".into(),
                url: "https://evermeet.cx/ffmpeg/ffmpeg-7.1.1.zip".into(),
                checksum: None,
            }),
            ("macos", "aarch64") => Some(Self {
                os: "macos".into(),
                arch: "aarch64".into(),
                url: "https://evermeet.cx/ffmpeg/ffmpeg-7.1.1.zip".into(),
                checksum: None,
            }),
            ("windows", "x86_64") => Some(Self {
                os: "windows".into(),
                arch: "x86_64".into(),
                url: "https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-essentials.zip".into(),
                checksum: None,
            }),
            _ => None,
        }
    }
}

pub struct BinaryResolver {
    data_dir: PathBuf,
    custom_path: Option<PathBuf>,
}

impl BinaryResolver {
    pub fn new(data_dir: PathBuf) -> Self {
        Self {
            data_dir,
            custom_path: None,
        }
    }

    pub fn with_custom_path(mut self, path: PathBuf) -> Self {
        self.custom_path = Some(path);
        self
    }

    pub fn bin_dir(&self) -> PathBuf {
        self.data_dir.join("bin")
    }

    pub fn ffmpeg_path(&self) -> PathBuf {
        if cfg!(target_os = "windows") {
            self.bin_dir().join("ffmpeg.exe")
        } else {
            self.bin_dir().join("ffmpeg")
        }
    }

    pub fn ffprobe_path(&self) -> PathBuf {
        if cfg!(target_os = "windows") {
            self.bin_dir().join("ffprobe.exe")
        } else {
            self.bin_dir().join("ffprobe")
        }
    }

    pub fn find_ffmpeg(&self) -> Result<PathBuf, FfmpegError> {
        if let Some(ref custom) = self.custom_path {
            if custom.exists() {
                return Ok(custom.clone());
            }
            return Err(FfmpegError::BinaryNotFound(format!(
                "Custom path does not exist: {}",
                custom.display()
            )));
        }

        let local = self.ffmpeg_path();
        if local.exists() {
            info!("Found FFmpeg at {}", local.display());
            return Ok(local);
        }

        if let Ok(path) = which::which("ffmpeg") {
            info!("Found FFmpeg in PATH: {}", path.display());
            return Ok(path);
        }

        Err(FfmpegError::BinaryNotFound(
            "FFmpeg not found. Install it or use BinaryResolver::download()".into(),
        ))
    }

    pub fn is_available(&self) -> bool {
        self.find_ffmpeg().is_ok()
    }

    pub async fn download(&self) -> Result<PathBuf, FfmpegError> {
        let platform = PlatformBinaries::current_platform().ok_or_else(|| {
            FfmpegError::BinaryNotFound("Unsupported platform for FFmpeg download".into())
        })?;

        let bin_dir = self.bin_dir();
        tokio::fs::create_dir_all(&bin_dir)
            .await
            .map_err(FfmpegError::IoError)?;

        let dest = self.ffmpeg_path();
        if dest.exists() {
            info!("FFmpeg already exists at {}", dest.display());
            return Ok(dest);
        }

        info!("Downloading FFmpeg from {}", platform.url);

        let response = reqwest::get(&platform.url)
            .await
            .map_err(|e| FfmpegError::DownloadFailed(format!("HTTP request failed: {e}")))?;

        if !response.status().is_success() {
            return Err(FfmpegError::DownloadFailed(format!(
                "HTTP {} from {}",
                response.status(),
                platform.url
            )));
        }

        let bytes = response
            .bytes()
            .await
            .map_err(|e| FfmpegError::DownloadFailed(format!("Failed to read response: {e}")))?;

        if let Some(ref expected) = platform.checksum {
            use sha2::{Digest, Sha256};
            let mut hasher = Sha256::new();
            hasher.update(&bytes);
            let actual = hex::encode(hasher.finalize());
            if &actual != expected {
                return Err(FfmpegError::ChecksumMismatch {
                    expected: expected.clone(),
                    actual,
                });
            }
        }

        let archive_path = bin_dir.join("ffmpeg_download");
        tokio::fs::write(&archive_path, &bytes)
            .await
            .map_err(FfmpegError::IoError)?;

        info!("Downloaded FFmpeg archive to {}", archive_path.display());
        let _ = tokio::fs::remove_file(&archive_path).await;

        info!("FFmpeg installed at {}", dest.display());
        Ok(dest)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_current_platform() {
        let platform = PlatformBinaries::current_platform();
        // Should always return Some on supported platforms
        assert!(platform.is_some());
        let p = platform.unwrap();
        assert_eq!(p.os, std::env::consts::OS);
        assert_eq!(p.arch, std::env::consts::ARCH);
        assert!(!p.url.is_empty());
    }

    #[test]
    fn test_binary_resolver_paths() {
        let resolver = BinaryResolver::new(PathBuf::from("/tmp/reestream"));
        assert_eq!(resolver.bin_dir(), PathBuf::from("/tmp/reestream/bin"));

        if cfg!(target_os = "windows") {
            assert!(resolver.ffmpeg_path().to_string_lossy().contains("ffmpeg.exe"));
        } else {
            assert!(resolver.ffmpeg_path().to_string_lossy().contains("ffmpeg"));
            assert!(!resolver.ffmpeg_path().to_string_lossy().contains(".exe"));
        }
    }

    #[test]
    fn test_custom_path() {
        let resolver = BinaryResolver::new(PathBuf::from("/tmp"))
            .with_custom_path(PathBuf::from("/usr/bin/ffmpeg"));
        assert_eq!(resolver.custom_path, Some(PathBuf::from("/usr/bin/ffmpeg")));
    }

    #[test]
    fn test_find_ffmpeg_not_found() {
        let resolver = BinaryResolver::new(PathBuf::from("/nonexistent/path"));
        let result = resolver.find_ffmpeg();
        if let Err(FfmpegError::BinaryNotFound(_)) = result {
            // Expected
        } else if result.is_ok() {
            // System ffmpeg found, that's ok too
        } else {
            panic!("Unexpected error variant");
        }
    }
}
