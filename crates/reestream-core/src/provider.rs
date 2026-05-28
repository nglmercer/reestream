use std::error::Error;
use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Debug)]
#[allow(dead_code)]
#[allow(clippy::enum_variant_names)]
pub enum StreamKeyError {
    OAuthError(String),
    ApiError(String),
    ParseError(String),
    NetworkError(String),
}

impl fmt::Display for StreamKeyError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            StreamKeyError::OAuthError(msg) => write!(f, "OAuth Error: {msg}"),
            StreamKeyError::ApiError(msg) => write!(f, "API Error: {msg}"),
            StreamKeyError::ParseError(msg) => write!(f, "Parse Error: {msg}"),
            StreamKeyError::NetworkError(msg) => write!(f, "Network Error: {msg}"),
        }
    }
}

impl Error for StreamKeyError {}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct StreamKey {
    pub key: String,
    pub rtmp_url: String,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct OAuth2Config {
    pub client_id: String,
    pub client_secret: String,
    pub redirect_uri: String,
    pub access_token: Option<String>,
}

#[allow(dead_code)]
#[allow(async_fn_in_trait)]
pub trait StreamKeyProvider: Send + Sync {
    const NAME: &str;

    fn get_auth_url(&self, state: &str, scopes: &[&str]) -> String;
    async fn exchange_code(&mut self, code: &str) -> Result<String, StreamKeyError>;
    async fn get_stream_key(&self) -> Result<StreamKey, StreamKeyError>;
    async fn refresh_token(&mut self, refresh_token: &str) -> Result<String, StreamKeyError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stream_key_serialization() {
        let key = StreamKey {
            key: "abc123".to_string(),
            rtmp_url: "rtmp://live.twitch.tv/app".to_string(),
        };
        let json = serde_json::to_string(&key).unwrap();
        assert!(json.contains("abc123"));
        assert!(json.contains("rtmp://live.twitch.tv/app"));
    }

    #[test]
    fn test_stream_key_deserialization() {
        let json = r#"{"key":"test-key","rtmp_url":"rtmp://example.com/live"}"#;
        let key: StreamKey = serde_json::from_str(json).unwrap();
        assert_eq!(key.key, "test-key");
        assert_eq!(key.rtmp_url, "rtmp://example.com/live");
    }

    #[test]
    fn test_stream_key_roundtrip() {
        let key = StreamKey {
            key: "roundtrip".to_string(),
            rtmp_url: "rtmps://facebook.com:443/rtmp".to_string(),
        };
        let json = serde_json::to_string(&key).unwrap();
        let deserialized: StreamKey = serde_json::from_str(&json).unwrap();
        assert_eq!(key.key, deserialized.key);
        assert_eq!(key.rtmp_url, deserialized.rtmp_url);
    }

    #[test]
    fn test_oauth2_config_fields() {
        let config = OAuth2Config {
            client_id: "my-id".to_string(),
            client_secret: "my-secret".to_string(),
            redirect_uri: "http://localhost/callback".to_string(),
            access_token: Some("token123".to_string()),
        };
        assert_eq!(config.client_id, "my-id");
        assert_eq!(config.client_secret, "my-secret");
        assert_eq!(config.redirect_uri, "http://localhost/callback");
        assert_eq!(config.access_token.as_deref(), Some("token123"));
    }

    #[test]
    fn test_oauth2_config_no_token() {
        let config = OAuth2Config {
            client_id: "id".to_string(),
            client_secret: "secret".to_string(),
            redirect_uri: "http://localhost".to_string(),
            access_token: None,
        };
        assert!(config.access_token.is_none());
    }

    #[test]
    fn test_stream_key_error_display() {
        let err = StreamKeyError::OAuthError("invalid_grant".into());
        assert_eq!(err.to_string(), "OAuth Error: invalid_grant");

        let err = StreamKeyError::ApiError("rate limited".into());
        assert_eq!(err.to_string(), "API Error: rate limited");

        let err = StreamKeyError::ParseError("bad json".into());
        assert_eq!(err.to_string(), "Parse Error: bad json");

        let err = StreamKeyError::NetworkError("connection refused".into());
        assert_eq!(err.to_string(), "Network Error: connection refused");
    }

    #[test]
    fn test_stream_key_error_is_std_error() {
        let err: Box<dyn std::error::Error> = Box::new(StreamKeyError::OAuthError("test".into()));
        assert!(err.to_string().contains("OAuth Error"));
    }

    #[test]
    fn test_stream_key_clone() {
        let key = StreamKey {
            key: "clone-test".to_string(),
            rtmp_url: "rtmp://test.com".to_string(),
        };
        let cloned = key.clone();
        assert_eq!(key.key, cloned.key);
        assert_eq!(key.rtmp_url, cloned.rtmp_url);
    }

    #[test]
    fn test_stream_key_debug() {
        let key = StreamKey {
            key: "debug".to_string(),
            rtmp_url: "rtmp://debug.com".to_string(),
        };
        let debug_str = format!("{:?}", key);
        assert!(debug_str.contains("StreamKey"));
        assert!(debug_str.contains("debug"));
    }
}
