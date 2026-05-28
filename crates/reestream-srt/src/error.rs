use std::fmt;

#[derive(Debug)]
pub enum SrtError {
    BindFailed(String),
    ConnectionFailed(String),
    SendFailed(String),
    ReceiveFailed(String),
    InvalidConfig(String),
    IoError(std::io::Error),
    Timeout(String),
}

impl fmt::Display for SrtError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BindFailed(msg) => write!(f, "SRT bind failed: {msg}"),
            Self::ConnectionFailed(msg) => write!(f, "SRT connection failed: {msg}"),
            Self::SendFailed(msg) => write!(f, "SRT send failed: {msg}"),
            Self::ReceiveFailed(msg) => write!(f, "SRT receive failed: {msg}"),
            Self::InvalidConfig(msg) => write!(f, "Invalid SRT config: {msg}"),
            Self::IoError(e) => write!(f, "IO error: {e}"),
            Self::Timeout(msg) => write!(f, "SRT timeout: {msg}"),
        }
    }
}

impl std::error::Error for SrtError {}

impl From<std::io::Error> for SrtError {
    fn from(e: std::io::Error) -> Self {
        Self::IoError(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_display_bind_failed() {
        let err = SrtError::BindFailed("port in use".into());
        assert!(err.to_string().contains("bind failed"));
    }

    #[test]
    fn test_display_connection_failed() {
        let err = SrtError::ConnectionFailed("refused".into());
        assert!(err.to_string().contains("connection failed"));
    }

    #[test]
    fn test_display_send_failed() {
        let err = SrtError::SendFailed("buffer full".into());
        assert!(err.to_string().contains("send failed"));
    }

    #[test]
    fn test_display_receive_failed() {
        let err = SrtError::ReceiveFailed("timeout".into());
        assert!(err.to_string().contains("receive failed"));
    }

    #[test]
    fn test_display_invalid_config() {
        let err = SrtError::InvalidConfig("bad latency".into());
        assert!(err.to_string().contains("Invalid SRT config"));
    }

    #[test]
    fn test_display_timeout() {
        let err = SrtError::Timeout("30s".into());
        assert!(err.to_string().contains("timeout"));
    }

    #[test]
    fn test_from_io_error() {
        let io = std::io::Error::new(std::io::ErrorKind::NotFound, "test");
        let err: SrtError = io.into();
        assert!(matches!(err, SrtError::IoError(_)));
    }

    #[test]
    fn test_error_trait() {
        let err: Box<dyn std::error::Error> = Box::new(SrtError::BindFailed("test".into()));
        assert!(err.to_string().contains("bind failed"));
    }
}
