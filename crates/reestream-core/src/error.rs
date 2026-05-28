use std::fmt;

#[derive(Debug)]
#[allow(dead_code)]
pub enum RelayError {
    Io(std::io::Error),
    Tls(tokio_native_tls::native_tls::Error),
    Handshake(String),
    Session(String),
    Connection(String),
    Timeout(String),
    InvalidConfig(String),
    PublishRejected(String),
}

impl fmt::Display for RelayError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "IO error: {e}"),
            Self::Handshake(msg) => write!(f, "Handshake error: {msg}"),
            Self::Session(msg) => write!(f, "Session error: {msg}"),
            Self::Connection(msg) => write!(f, "Connection error: {msg}"),
            Self::Timeout(msg) => write!(f, "Timeout: {msg}"),
            Self::InvalidConfig(msg) => write!(f, "Invalid config: {msg}"),
            Self::PublishRejected(msg) => write!(f, "Publish rejected: {msg}"),
            Self::Tls(error) => write!(f, "Tls on rtmps: {error}"),
        }
    }
}

impl std::error::Error for RelayError {}

impl From<std::io::Error> for RelayError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<tokio_native_tls::native_tls::Error> for RelayError {
    fn from(e: tokio_native_tls::native_tls::Error) -> Self {
        Self::Tls(e)
    }
}

#[allow(dead_code)]
pub type Result<T> = std::result::Result<T, RelayError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_display_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::ConnectionRefused, "refused");
        let err = RelayError::Io(io_err);
        assert!(err.to_string().contains("IO error"));
        assert!(err.to_string().contains("refused"));
    }

    #[test]
    fn test_display_handshake() {
        let err = RelayError::Handshake("bad handshake".into());
        assert_eq!(err.to_string(), "Handshake error: bad handshake");
    }

    #[test]
    fn test_display_session() {
        let err = RelayError::Session("session expired".into());
        assert_eq!(err.to_string(), "Session error: session expired");
    }

    #[test]
    fn test_display_connection() {
        let err = RelayError::Connection("timeout".into());
        assert_eq!(err.to_string(), "Connection error: timeout");
    }

    #[test]
    fn test_display_timeout() {
        let err = RelayError::Timeout("30s".into());
        assert_eq!(err.to_string(), "Timeout: 30s");
    }

    #[test]
    fn test_display_invalid_config() {
        let err = RelayError::InvalidConfig("missing field".into());
        assert_eq!(err.to_string(), "Invalid config: missing field");
    }

    #[test]
    fn test_display_publish_rejected() {
        let err = RelayError::PublishRejected("bad key".into());
        assert_eq!(err.to_string(), "Publish rejected: bad key");
    }

    #[test]
    fn test_from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file missing");
        let err: RelayError = io_err.into();
        assert!(matches!(err, RelayError::Io(_)));
    }

    #[test]
    fn test_error_trait_implemented() {
        let err: Box<dyn std::error::Error> =
            Box::new(RelayError::Handshake("test".into()));
        assert_eq!(err.to_string(), "Handshake error: test");
    }

    #[test]
    fn test_relay_error_debug() {
        let err = RelayError::Connection("debug test".into());
        let debug = format!("{:?}", err);
        assert!(debug.contains("Connection"));
        assert!(debug.contains("debug test"));
    }

    #[test]
    fn test_relay_error_all_variants_display() {
        let variants = vec![
            RelayError::Handshake("h".into()),
            RelayError::Session("s".into()),
            RelayError::Connection("c".into()),
            RelayError::Timeout("t".into()),
            RelayError::InvalidConfig("i".into()),
            RelayError::PublishRejected("p".into()),
        ];
        for err in variants {
            let msg = err.to_string();
            assert!(!msg.is_empty(), "Display should not be empty for {:?}", err);
        }
    }

    #[test]
    fn test_from_io_error_various_kinds() {
        let kinds = vec![
            std::io::ErrorKind::NotFound,
            std::io::ErrorKind::PermissionDenied,
            std::io::ErrorKind::ConnectionRefused,
            std::io::ErrorKind::ConnectionReset,
            std::io::ErrorKind::TimedOut,
            std::io::ErrorKind::BrokenPipe,
            std::io::ErrorKind::AddrInUse,
            std::io::ErrorKind::AddrNotAvailable,
        ];
        for kind in kinds {
            let io_err = std::io::Error::new(kind, "test");
            let err: RelayError = io_err.into();
            assert!(matches!(err, RelayError::Io(_)));
            assert!(err.to_string().contains("IO error"));
        }
    }

    #[test]
    fn test_result_type_alias() {
        let ok: crate::error::Result<i32> = Ok(42);
        assert!(ok.is_ok());

        let err: crate::error::Result<i32> = Err(RelayError::Timeout("test".into()));
        assert!(err.is_err());
    }
}
