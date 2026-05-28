use std::fmt;

#[derive(Debug)]
pub enum FfmpegError {
    BinaryNotFound(String),
    DownloadFailed(String),
    ChecksumMismatch { expected: String, actual: String },
    ProcessStartFailed(std::io::Error),
    ProcessFailed { exit_code: i32, stderr: String },
    InvalidArgument(String),
    IoError(std::io::Error),
}

impl fmt::Display for FfmpegError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BinaryNotFound(msg) => write!(f, "FFmpeg binary not found: {msg}"),
            Self::DownloadFailed(msg) => write!(f, "FFmpeg download failed: {msg}"),
            Self::ChecksumMismatch { expected, actual } => {
                write!(f, "Checksum mismatch: expected {expected}, got {actual}")
            }
            Self::ProcessStartFailed(e) => write!(f, "Failed to start FFmpeg: {e}"),
            Self::ProcessFailed { exit_code, stderr } => {
                write!(f, "FFmpeg exited with code {exit_code}: {stderr}")
            }
            Self::InvalidArgument(msg) => write!(f, "Invalid FFmpeg argument: {msg}"),
            Self::IoError(e) => write!(f, "IO error: {e}"),
        }
    }
}

impl std::error::Error for FfmpegError {}

impl From<std::io::Error> for FfmpegError {
    fn from(e: std::io::Error) -> Self {
        Self::IoError(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_display_binary_not_found() {
        let err = FfmpegError::BinaryNotFound("not in PATH".into());
        assert!(err.to_string().contains("not found"));
    }

    #[test]
    fn test_display_download_failed() {
        let err = FfmpegError::DownloadFailed("404".into());
        assert!(err.to_string().contains("download failed"));
    }

    #[test]
    fn test_display_checksum_mismatch() {
        let err = FfmpegError::ChecksumMismatch {
            expected: "abc".into(),
            actual: "def".into(),
        };
        assert!(err.to_string().contains("Checksum mismatch"));
    }

    #[test]
    fn test_display_process_failed() {
        let err = FfmpegError::ProcessFailed {
            exit_code: 1,
            stderr: "error".into(),
        };
        assert!(err.to_string().contains("exited with code 1"));
    }

    #[test]
    fn test_from_io_error() {
        let io = std::io::Error::new(std::io::ErrorKind::NotFound, "test");
        let err: FfmpegError = io.into();
        assert!(matches!(err, FfmpegError::IoError(_)));
    }

    #[test]
    fn test_error_trait() {
        let err: Box<dyn std::error::Error> = Box::new(FfmpegError::InvalidArgument("bad".into()));
        assert!(err.to_string().contains("Invalid"));
    }
}
