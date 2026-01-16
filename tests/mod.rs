//! Common test utilities for reestream tests
//!
//! This module provides shared utilities, fixtures, and helpers
//! for testing the RTMP relay functionality.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use rml_rtmp::sessions::{ServerSession, ServerSessionConfig};
use tokio::sync::RwLock;

/// Default test configuration values
pub const DEFAULT_TEST_ADDR: &str = "127.0.0.1:19350";
pub const DEFAULT_STREAM_KEY: &str = "test-stream-key";
pub const DEFAULT_RTMP_ADDR: &str = "127.0.0.1";
pub const DEFAULT_RTMP_PORT: u16 = 1935;

/// Test timeout duration for async operations
pub const TEST_TIMEOUT: Duration = Duration::from_secs(5);

/// Creates a minimal valid test configuration TOML content
pub fn minimal_config_toml() -> String {
    format!(
        r#"
rtmp_addr = "{}"
rtmp_port = {}
stream_key = "{}"
"#,
        DEFAULT_RTMP_ADDR, DEFAULT_RTMP_PORT, DEFAULT_STREAM_KEY
    )
}

/// Creates a test configuration with a single platform
pub fn single_platform_config_toml(platform_url: &str, platform_key: &str) -> String {
    format!(
        r#"
rtmp_addr = "{}"
rtmp_port = {}
stream_key = "{}"

[[platform]]
url = "{}"
key = "{}"
orientation = "horizontal"
"#,
        DEFAULT_RTMP_ADDR, DEFAULT_RTMP_PORT, DEFAULT_STREAM_KEY, platform_url, platform_key
    )
}

/// Creates a test configuration with multiple platforms
pub fn multi_platform_config_toml(platforms: Vec<(&str, &str)>) -> String {
    let platforms_section: String = platforms
        .iter()
        .enumerate()
        .map(|(_i, (url, key))| {
            format!(
                r#"
[[platform]]
url = "{}"
key = "{}"
orientation = "horizontal"
"#,
                url, key
            )
        })
        .collect();

    format!(
        r#"
rtmp_addr = "{}"
rtmp_port = {}
stream_key = "{}"
{}
"#,
        DEFAULT_RTMP_ADDR, DEFAULT_RTMP_PORT, DEFAULT_STREAM_KEY, platforms_section
    )
}

/// Creates a ServerSession with test-friendly configuration
pub fn create_test_server_session() -> Result<ServerSession, Box<dyn std::error::Error>> {
    let mut config = ServerSessionConfig::new();
    config.chunk_size = 128; // Use smaller chunks for faster tests
    config.window_ack_size = 262_144;

    let (session, _) = ServerSession::new(config)?;
    Ok(session)
}

/// A wrapper for creating an empty platforms list
pub async fn empty_platforms_list() -> Arc<RwLock<Vec<reestream::config::Platform>>> {
    Arc::new(RwLock::new(Vec::new()))
}

/// Helper to parse a socket address string
pub fn parse_socket_addr(addr: &str) -> Result<SocketAddr, Box<dyn std::error::Error>> {
    Ok(addr.parse()?)
}

/// Generates random test data for audio/video frames
pub fn generate_test_frame_data(size: usize) -> Vec<u8> {
    (0..size).map(|i| (i % 256) as u8).collect()
}

/// Mock error type for testing error handling
#[derive(Debug, thiserror::Error)]
pub enum MockError {
    #[error("Network error: {0}")]
    Network(String),

    #[error("Timeout")]
    Timeout,

    #[error("Invalid stream key")]
    InvalidStreamKey,

    #[error("Connection closed")]
    ConnectionClosed,
}

impl MockError {
    pub fn network(msg: &str) -> Self {
        Self::Network(msg.to_string())
    }
}

/// Test assertion helpers
pub mod assertions {
    use std::time::Duration;
    use tokio::time::timeout;

    /// Asserts that a future completes within the given timeout
    pub async fn assert_completes_within<F, T>(duration: Duration, future: F) -> Result<T, String>
    where
        F: std::future::Future<Output = T>,
    {
        match timeout(duration, future).await {
            Ok(result) => Ok(result),
            Err(_) => Err(format!("Operation did not complete within {:?}", duration)),
        }
    }

    /// Asserts that a future does NOT complete within the given timeout
    pub async fn assert_not_complete_within<F, T>(
        duration: Duration,
        future: F,
    ) -> Result<(), String>
    where
        F: std::future::Future<Output = T>,
    {
        match timeout(duration, future).await {
            Ok(_) => Err(format!(
                "Operation completed within {:?}, but should not have",
                duration
            )),
            Err(_) => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_minimal_config_toml_generates_valid_toml() {
        let toml = minimal_config_toml();
        assert!(toml.contains("rtmp_addr"));
        assert!(toml.contains("rtmp_port"));
        assert!(toml.contains("stream_key"));
        assert!(toml.contains(DEFAULT_RTMP_ADDR));
        assert!(toml.contains(DEFAULT_STREAM_KEY));
    }

    #[test]
    fn test_single_platform_config_toml_generates_valid_toml() {
        let toml = single_platform_config_toml("rtmp://localhost:1936/live", "test-key");
        assert!(toml.contains("[[platform]]"));
        assert!(toml.contains("rtmp://localhost:1936/live"));
        assert!(toml.contains("test-key"));
    }

    #[test]
    fn test_multi_platform_config_toml_generates_valid_toml() {
        let toml = multi_platform_config_toml(vec![
            ("rtmp://localhost:1936/live", "key1"),
            ("rtmp://localhost:1937/live", "key2"),
        ]);
        assert!(toml.contains("[[platform]]"));
        assert!(toml.contains("key1"));
        assert!(toml.contains("key2"));
    }

    #[test]
    fn test_generate_test_frame_data_produces_correct_size() {
        let data = generate_test_frame_data(1024);
        assert_eq!(data.len(), 1024);
    }

    #[test]
    fn test_generate_test_frame_data_is_deterministic() {
        let data1 = generate_test_frame_data(10);
        let data2 = generate_test_frame_data(10);
        assert_eq!(data1, data2);
    }

    #[test]
    fn test_mock_error_display() {
        let err = MockError::network("test");
        assert_eq!(err.to_string(), "Network error: test");
    }
}
