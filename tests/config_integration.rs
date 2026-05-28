use std::path::PathBuf;

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

#[test]
fn test_load_valid_config_from_file() {
    let path = fixture_path("config_valid.toml");
    let config = reestream::config::Config::from_file(&path).unwrap();
    assert_eq!(config.rtmp_addr, "127.0.0.1");
    assert_eq!(config.rtmp_port, 1935);
    assert_eq!(config.stream_key, "test-stream-key");
    let platforms = config.platform.unwrap();
    assert_eq!(platforms.len(), 2);
    assert_eq!(platforms[0].key, "local-key-1");
    assert_eq!(platforms[1].key, "local-key-2");
}

#[test]
fn test_load_minimal_config_from_file() {
    let path = fixture_path("config_minimal.toml");
    let config = reestream::config::Config::from_file(&path).unwrap();
    assert_eq!(config.rtmp_addr, "127.0.0.1");
    assert_eq!(config.rtmp_port, 1935);
    assert_eq!(config.stream_key, "minimal-key");
    assert!(config.platform.is_none());
}

#[test]
fn test_load_empty_platforms_config_from_file() {
    let path = fixture_path("config_empty_platforms.toml");
    let config = reestream::config::Config::from_file(&path).unwrap();
    let platforms = config.platform.unwrap();
    assert!(platforms.is_empty());
}

#[test]
fn test_load_invalid_config_fails() {
    let path = fixture_path("config_invalid.toml");
    let result = reestream::config::Config::from_file(&path);
    assert!(result.is_err());
}

#[test]
fn test_load_nonexistent_config_fails() {
    let path = PathBuf::from("/nonexistent/path/to/config.toml");
    let result = reestream::config::Config::from_file(&path);
    assert!(result.is_err());
}

#[test]
fn test_config_platform_url_schemes() {
    let path = fixture_path("config_valid.toml");
    let config = reestream::config::Config::from_file(&path).unwrap();
    let platforms = config.platform.unwrap();
    assert_eq!(platforms[0].url.scheme(), "rtmp");
    assert_eq!(platforms[0].url.host_str(), Some("127.0.0.1"));
    assert_eq!(platforms[0].url.port(), Some(1936));
}
