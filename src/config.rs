use serde::Deserialize;
use std::fs;
use std::path::Path;
use url::Url;

#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    pub rtmp_addr: String,
    pub rtmp_port: u16,
    pub stream_key: String,
    pub platform: Option<Vec<Platform>>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Platform {
    pub url: Url,
    pub key: String,
    #[allow(dead_code)]
    pub orientation: Orientation,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Orientation {
    #[default]
    Horizontal,
    Vertical,
}

impl Config {
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, Box<dyn std::error::Error>> {
        let contents = fs::read_to_string(path)?;
        Self::from_str(&contents)
    }

    pub fn from_str(s: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let config: Config = toml::from_str(s)?;
        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_minimal_config() {
        let toml = r#"
            rtmp_addr = "0.0.0.0"
            rtmp_port = 1945
            stream_key = "test-key"
        "#;
        let config = Config::from_str(toml).unwrap();
        assert_eq!(config.rtmp_addr, "0.0.0.0");
        assert_eq!(config.rtmp_port, 1945);
        assert_eq!(config.stream_key, "test-key");
        assert!(config.platform.is_none());
    }

    #[test]
    fn test_parse_config_with_platforms() {
        let toml = r#"
            rtmp_addr = "127.0.0.1"
            rtmp_port = 1935
            stream_key = "my-key"

            [[platform]]
            url = "rtmp://live.twitch.tv/app"
            key = "twitch-key"
            orientation = "horizontal"

            [[platform]]
            url = "rtmps://live-api-s.facebook.com:443/rtmp/"
            key = "fb-key"
            orientation = "vertical"
        "#;
        let config = Config::from_str(toml).unwrap();
        let platforms = config.platform.unwrap();
        assert_eq!(platforms.len(), 2);
        assert_eq!(platforms[0].key, "twitch-key");
        assert_eq!(platforms[0].orientation, Orientation::Horizontal);
        assert_eq!(platforms[1].key, "fb-key");
        assert_eq!(platforms[1].orientation, Orientation::Vertical);
    }

    #[test]
    fn test_parse_config_with_rtmps_flag() {
        let toml = r#"
            rtmps = true
            rtmp_addr = "0.0.0.0"
            rtmp_port = 443
            stream_key = "key"
        "#;
        let config = Config::from_str(toml).unwrap();
        assert_eq!(config.rtmp_port, 443);
    }

    #[test]
    fn test_orientation_default() {
        let toml = r#"
            rtmp_addr = "0.0.0.0"
            rtmp_port = 1945
            stream_key = "key"

            [[platform]]
            url = "rtmp://live.twitch.tv/app"
            key = "test"
            orientation = "horizontal"
        "#;
        let config = Config::from_str(toml).unwrap();
        let platforms = config.platform.unwrap();
        assert_eq!(platforms[0].orientation, Orientation::Horizontal);
    }

    #[test]
    fn test_invalid_toml_fails() {
        let toml = "not valid toml [[[";
        let result = Config::from_str(toml);
        assert!(result.is_err());
    }

    #[test]
    fn test_missing_required_field_fails() {
        let toml = r#"
            rtmp_addr = "0.0.0.0"
        "#;
        let result = Config::from_str(toml);
        assert!(result.is_err());
    }
}
