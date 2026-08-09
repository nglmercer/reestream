use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::str::FromStr;
use url::Url;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Config {
    pub rtmp_addr: String,
    pub rtmp_port: u16,
    pub stream_key: String,
    pub platform: Option<Vec<Platform>>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Platform {
    pub url: Url,
    pub key: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub orientation: Orientation,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Orientation {
    #[default]
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone)]
pub enum PlatformEvent {
    Toggled {
        platform_id: String,
        url: String,
        key: String,
        enabled: bool,
    },
    Added {
        platform_id: String,
        url: String,
        key: String,
    },
    Removed {
        platform_id: String,
    },
}

impl PlatformEvent {
    pub fn platform_id(&self) -> &str {
        match self {
            PlatformEvent::Toggled { platform_id, .. } => platform_id,
            PlatformEvent::Added { platform_id, .. } => platform_id,
            PlatformEvent::Removed { platform_id } => platform_id,
        }
    }
}

/// Generate a stable platform ID from URL + key.
pub fn platform_id_from(url: &str, key: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    url.hash(&mut hasher);
    key.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

pub struct ConfigBuilder {
    rtmp_addr: String,
    rtmp_port: u16,
    stream_key: String,
    platforms: Vec<Platform>,
}

impl ConfigBuilder {
    pub fn new() -> Self {
        Self {
            rtmp_addr: "0.0.0.0".into(),
            rtmp_port: 1935,
            stream_key: String::new(),
            platforms: Vec::new(),
        }
    }

    pub fn addr(mut self, addr: impl Into<String>) -> Self {
        self.rtmp_addr = addr.into();
        self
    }

    pub fn port(mut self, port: u16) -> Self {
        self.rtmp_port = port;
        self
    }

    pub fn stream_key(mut self, key: impl Into<String>) -> Self {
        self.stream_key = key.into();
        self
    }

    pub fn add_platform(
        mut self,
        url: Url,
        key: impl Into<String>,
        orientation: Orientation,
    ) -> Self {
        self.platforms.push(Platform {
            url,
            key: key.into(),
            enabled: true,
            orientation,
        });
        self
    }

    pub fn build(self) -> Config {
        Config {
            rtmp_addr: self.rtmp_addr,
            rtmp_port: self.rtmp_port,
            stream_key: self.stream_key,
            platform: if self.platforms.is_empty() {
                None
            } else {
                Some(self.platforms)
            },
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.stream_key.is_empty() {
            return Err("stream_key cannot be empty".into());
        }
        if self.rtmp_port == 0 {
            return Err("rtmp_port cannot be 0".into());
        }
        if self.rtmp_addr.is_empty() {
            return Err("rtmp_addr cannot be empty".into());
        }
        for (i, p) in self.platforms.iter().enumerate() {
            if p.key.is_empty() {
                return Err(format!("platform[{i}] key cannot be empty"));
            }
            if p.url.host().is_none() {
                return Err(format!("platform[{i}] url has no host"));
            }
            if !matches!(p.url.scheme(), "rtmp" | "rtmps") {
                return Err(format!("platform[{i}] url must use rtmp:// or rtmps://"));
            }
        }
        Ok(())
    }
}

impl Default for ConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl Config {
    pub fn builder() -> ConfigBuilder {
        ConfigBuilder::new()
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.stream_key.is_empty() {
            return Err("stream_key cannot be empty".into());
        }
        if self.rtmp_port == 0 {
            return Err("rtmp_port cannot be 0".into());
        }
        Ok(())
    }

    pub fn to_toml(&self) -> Result<String, toml::ser::Error> {
        toml::to_string(self)
    }
}

impl FromStr for Config {
    type Err = Box<dyn std::error::Error>;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let config: Config = toml::from_str(s)?;
        Ok(config)
    }
}

impl Config {
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, Box<dyn std::error::Error>> {
        let contents = fs::read_to_string(path)?;
        Self::from_str(&contents)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

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

    #[test]
    fn test_from_file_success() {
        let dir = std::env::temp_dir().join("reestream_test_config");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("test.toml");
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(
            f,
            r#"rtmp_addr = "0.0.0.0"
rtmp_port = 1935
stream_key = "file-key""#
        )
        .unwrap();
        let config = Config::from_file(&path).unwrap();
        assert_eq!(config.stream_key, "file-key");
        assert_eq!(config.rtmp_port, 1935);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_from_file_not_found() {
        let result = Config::from_file("/nonexistent/path/config.toml");
        assert!(result.is_err());
    }

    #[test]
    fn test_empty_platforms_array() {
        let toml = r#"
            rtmp_addr = "0.0.0.0"
            rtmp_port = 1935
            stream_key = "key"
            platform = []
        "#;
        let config = Config::from_str(toml).unwrap();
        let platforms = config.platform.unwrap();
        assert!(platforms.is_empty());
    }

    #[test]
    fn test_port_boundary_min() {
        let toml = r#"
            rtmp_addr = "0.0.0.0"
            rtmp_port = 1
            stream_key = "key"
        "#;
        let config = Config::from_str(toml).unwrap();
        assert_eq!(config.rtmp_port, 1);
    }

    #[test]
    fn test_port_boundary_max() {
        let toml = r#"
            rtmp_addr = "0.0.0.0"
            rtmp_port = 65535
            stream_key = "key"
        "#;
        let config = Config::from_str(toml).unwrap();
        assert_eq!(config.rtmp_port, 65535);
    }

    #[test]
    fn test_orientation_vertical() {
        let toml = r#"
            rtmp_addr = "0.0.0.0"
            rtmp_port = 1935
            stream_key = "key"

            [[platform]]
            url = "rtmp://live.instagram.com/rtmp"
            key = "ig-key"
            orientation = "vertical"
        "#;
        let config = Config::from_str(toml).unwrap();
        let platforms = config.platform.unwrap();
        assert_eq!(platforms[0].orientation, Orientation::Vertical);
    }

    #[test]
    fn test_orientation_clone_copy() {
        let o = Orientation::Horizontal;
        let o2 = o;
        assert_eq!(o, o2);
    }

    #[test]
    fn test_platform_url_parsing() {
        let toml = r#"
            rtmp_addr = "0.0.0.0"
            rtmp_port = 1935
            stream_key = "key"

            [[platform]]
            url = "rtmps://custom.server.com:9999/live/stream"
            key = "custom-key"
            orientation = "horizontal"
        "#;
        let config = Config::from_str(toml).unwrap();
        let p = &config.platform.unwrap()[0];
        assert_eq!(p.url.scheme(), "rtmps");
        assert_eq!(p.url.host_str(), Some("custom.server.com"));
        assert_eq!(p.url.port(), Some(9999));
    }

    #[test]
    fn test_config_clone() {
        let toml = r#"
            rtmp_addr = "0.0.0.0"
            rtmp_port = 1935
            stream_key = "key"
        "#;
        let config = Config::from_str(toml).unwrap();
        let cloned = config.clone();
        assert_eq!(config.rtmp_addr, cloned.rtmp_addr);
        assert_eq!(config.rtmp_port, cloned.rtmp_port);
    }

    #[test]
    fn test_config_debug() {
        let toml = r#"
            rtmp_addr = "0.0.0.0"
            rtmp_port = 1935
            stream_key = "key"
        "#;
        let config = Config::from_str(toml).unwrap();
        let debug = format!("{:?}", config);
        assert!(debug.contains("Config"));
        assert!(debug.contains("0.0.0.0"));
    }

    #[test]
    fn test_multiple_platforms_same_url() {
        let toml = r#"
            rtmp_addr = "0.0.0.0"
            rtmp_port = 1935
            stream_key = "key"

            [[platform]]
            url = "rtmp://live.twitch.tv/app"
            key = "key1"
            orientation = "horizontal"

            [[platform]]
            url = "rtmp://live.twitch.tv/app"
            key = "key2"
            orientation = "horizontal"
        "#;
        let config = Config::from_str(toml).unwrap();
        let platforms = config.platform.unwrap();
        assert_eq!(platforms.len(), 2);
        assert_eq!(platforms[0].key, "key1");
        assert_eq!(platforms[1].key, "key2");
    }

    #[test]
    fn test_config_builder_defaults() {
        let config = ConfigBuilder::new().stream_key("test-key").build();
        assert_eq!(config.rtmp_addr, "0.0.0.0");
        assert_eq!(config.rtmp_port, 1935);
        assert_eq!(config.stream_key, "test-key");
        assert!(config.platform.is_none());
    }

    #[test]
    fn test_config_builder_full() {
        let config = ConfigBuilder::new()
            .addr("127.0.0.1")
            .port(9999)
            .stream_key("my-key")
            .add_platform(
                Url::parse("rtmp://twitch.tv/app").unwrap(),
                "twitch-key",
                Orientation::Horizontal,
            )
            .add_platform(
                Url::parse("rtmp://youtube.com/live2").unwrap(),
                "yt-key",
                Orientation::Vertical,
            )
            .build();
        assert_eq!(config.rtmp_addr, "127.0.0.1");
        assert_eq!(config.rtmp_port, 9999);
        let platforms = config.platform.unwrap();
        assert_eq!(platforms.len(), 2);
    }

    #[test]
    fn test_config_builder_validate_ok() {
        let builder = ConfigBuilder::new().stream_key("key");
        assert!(builder.validate().is_ok());
    }

    #[test]
    fn test_config_builder_validate_empty_key() {
        let builder = ConfigBuilder::new();
        assert!(builder.validate().is_err());
    }

    #[test]
    fn test_config_builder_validate_zero_port() {
        let builder = ConfigBuilder::new().port(0).stream_key("key");
        assert!(builder.validate().is_err());
    }

    #[test]
    fn test_config_builder_validate_empty_platform_key() {
        let builder = ConfigBuilder::new().stream_key("key").add_platform(
            Url::parse("rtmp://twitch.tv/app").unwrap(),
            "",
            Orientation::Horizontal,
        );
        assert!(builder.validate().is_err());
    }

    #[test]
    fn test_config_validate() {
        let config = ConfigBuilder::new().stream_key("key").build();
        assert!(config.validate().is_ok());

        let bad = ConfigBuilder::new().build();
        assert!(bad.validate().is_err());
    }

    #[test]
    fn test_config_to_toml() {
        let config = ConfigBuilder::new()
            .addr("0.0.0.0")
            .port(1935)
            .stream_key("key")
            .build();
        let toml = config.to_toml().unwrap();
        assert!(toml.contains("rtmp_addr"));
        assert!(toml.contains("stream_key"));
    }

    #[test]
    fn test_config_builder_default_trait() {
        let builder = ConfigBuilder::default();
        assert_eq!(builder.rtmp_addr, "0.0.0.0");
    }

    #[test]
    fn test_config_builder_chaining() {
        let config = Config::builder()
            .addr("10.0.0.1")
            .port(8080)
            .stream_key("chain-key")
            .build();
        assert_eq!(config.rtmp_addr, "10.0.0.1");
        assert_eq!(config.rtmp_port, 8080);
    }
}
