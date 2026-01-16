use reestream::config::{Config, Orientation};
use std::io::Write;
use tempfile::NamedTempFile;

fn create_temp_config(content: &str) -> NamedTempFile {
    let mut file = NamedTempFile::new().unwrap();
    write!(file, "{}", content).unwrap();
    file
}

#[test]
fn test_valid_config_basic() {
    let content = r#"
        rtmp_addr = "127.0.0.1"
        rtmp_port = 1935
        stream_key = "test_key"
    "#;

    let file = create_temp_config(content);
    let config = Config::from_file(file.path()).unwrap();

    assert_eq!(config.rtmp_addr, "127.0.0.1");
    assert_eq!(config.rtmp_port, 1935);
    assert_eq!(config.stream_key, "test_key");
    assert!(config.platform.is_none());
}

#[test]
fn test_valid_config_with_platforms() {
    let content = r#"
        rtmp_addr = "0.0.0.0"
        rtmp_port = 1935
        stream_key = "live_stream"

        [[platform]]
        url = "rtmp://a.rtmp.youtube.com/live2"
        key = "youtube_key"
        orientation = "horizontal"

        [[platform]]
        url = "rtmp://live.twitch.tv/app"
        key = "twitch_key"
        orientation = "vertical"
    "#;

    let file = create_temp_config(content);
    let config = Config::from_file(file.path()).unwrap();

    assert_eq!(config.rtmp_addr, "0.0.0.0");
    assert_eq!(config.rtmp_port, 1935);
    assert_eq!(config.stream_key, "live_stream");

    let platforms = config.platform.unwrap();
    assert_eq!(platforms.len(), 2);

    assert_eq!(platforms[0].url.scheme(), "rtmp");
    assert_eq!(platforms[0].key, "youtube_key");
    assert_eq!(platforms[0].orientation, Orientation::Horizontal);

    assert_eq!(platforms[1].url.scheme(), "rtmp");
    assert_eq!(platforms[1].key, "twitch_key");
    assert_eq!(platforms[1].orientation, Orientation::Vertical);
}

#[test]
fn test_config_with_rtmps() {
    let content = r#"
        rtmp_addr = "192.168.1.100"
        rtmp_port = 1935
        stream_key = "secure_stream"

        [[platform]]
        url = "rtmps://secure.server.com:443/live"
        key = "secure_key"
        orientation = "horizontal"
    "#;

    let file = create_temp_config(content);
    let config = Config::from_file(file.path()).unwrap();

    let platforms = config.platform.unwrap();
    assert_eq!(platforms.len(), 1);
    assert_eq!(platforms[0].url.scheme(), "rtmps");
}

#[test]
fn test_config_default_orientation() {
    let content = r#"
        rtmp_addr = "127.0.0.1"
        rtmp_port = 1935
        stream_key = "test"

        [[platform]]
        url = "rtmp://example.com/live"
        key = "key"
    "#;

    let file = create_temp_config(content);
    let config = Config::from_file(file.path()).unwrap();

    let platforms = config.platform.unwrap();
    assert_eq!(platforms[0].orientation, Orientation::Horizontal);
}

#[test]
fn test_config_missing_rtmp_addr() {
    let content = r#"
        rtmp_port = 1935
        stream_key = "test"
    "#;

    let file = create_temp_config(content);
    let result = Config::from_file(file.path());

    assert!(result.is_err());
}

#[test]
fn test_config_missing_stream_key() {
    let content = r#"
        rtmp_addr = "127.0.0.1"
        rtmp_port = 1935
    "#;

    let file = create_temp_config(content);
    let result = Config::from_file(file.path());

    assert!(result.is_err());
}

#[test]
fn test_config_invalid_port_type() {
    let content = r#"
        rtmp_addr = "127.0.0.1"
        rtmp_port = "not_a_number"
        stream_key = "test"
    "#;

    let file = create_temp_config(content);
    let result = Config::from_file(file.path());

    assert!(result.is_err());
}

#[test]
fn test_config_high_port_value() {
    let content = r#"
        rtmp_addr = "127.0.0.1"
        rtmp_port = 65535
        stream_key = "test"
    "#;

    let file = create_temp_config(content);
    let config = Config::from_file(file.path()).unwrap();

    // u16::MAX is 65535, which is technically a valid port number
    // This test documents that we don't validate port ranges
    assert_eq!(config.rtmp_port, 65535);
}

#[test]
fn test_config_invalid_url() {
    let content = r#"
        rtmp_addr = "127.0.0.1"
        rtmp_port = 1935
        stream_key = "test"

        [[platform]]
        url = "not-a-valid-url"
        key = "key"
        orientation = "horizontal"
    "#;

    let file = create_temp_config(content);
    let result = Config::from_file(file.path());

    assert!(result.is_err());
}

#[test]
fn test_config_empty_platform_list() {
    let content = r#"
        rtmp_addr = "127.0.0.1"
        rtmp_port = 1935
        stream_key = "test"
        platform = []
    "#;

    let file = create_temp_config(content);
    let config = Config::from_file(file.path()).unwrap();

    assert!(config.platform.is_some());
    assert_eq!(config.platform.unwrap().len(), 0);
}

#[test]
fn test_config_multiple_platforms_same_type() {
    let content = r#"
        rtmp_addr = "127.0.0.1"
        rtmp_port = 1935
        stream_key = "test"

        [[platform]]
        url = "rtmp://server1.com/live"
        key = "key1"

        [[platform]]
        url = "rtmp://server2.com/live"
        key = "key2"

        [[platform]]
        url = "rtmp://server3.com/live"
        key = "key3"
    "#;

    let file = create_temp_config(content);
    let config = Config::from_file(file.path()).unwrap();

    let platforms = config.platform.unwrap();
    assert_eq!(platforms.len(), 3);
}

#[test]
fn test_config_platform_with_custom_port() {
    let content = r#"
        rtmp_addr = "127.0.0.1"
        rtmp_port = 1935
        stream_key = "test"

        [[platform]]
        url = "rtmp://custom.server.com:5000/live"
        key = "custom_key"
    "#;

    let file = create_temp_config(content);
    let config = Config::from_file(file.path()).unwrap();

    let platforms = config.platform.unwrap();
    assert_eq!(platforms[0].url.port().unwrap(), 5000);
}

#[test]
fn test_config_with_comments() {
    let content = r#"
        # RTMP server configuration
        rtmp_addr = "127.0.0.1"  # Local address
        rtmp_port = 1935        # Standard RTMP port
        stream_key = "test_key" # Authentication key

        # Platform destinations
        [[platform]]
        url = "rtmp://example.com/live"
        key = "platform_key"
        # orientation defaults to horizontal
    "#;

    let file = create_temp_config(content);
    let config = Config::from_file(file.path()).unwrap();

    assert_eq!(config.rtmp_addr, "127.0.0.1");
    assert_eq!(config.rtmp_port, 1935);
    assert_eq!(config.stream_key, "test_key");
}

#[test]
fn test_config_invalid_orientation() {
    let content = r#"
        rtmp_addr = "127.0.0.1"
        rtmp_port = 1935
        stream_key = "test"

        [[platform]]
        url = "rtmp://example.com/live"
        key = "key"
        orientation = "diagonal"
    "#;

    let file = create_temp_config(content);
    let result = Config::from_file(file.path());

    assert!(result.is_err());
}

#[test]
fn test_config_file_not_found() {
    let result = Config::from_file("/nonexistent/path/config.toml");

    assert!(result.is_err());
}

#[test]
fn test_config_malformed_toml() {
    let content = r#"
        rtmp_addr = "127.0.0.1"
        rtmp_port = 1935
        stream_key = "test"
        invalid toml syntax here
    "#;

    let file = create_temp_config(content);
    let result = Config::from_file(file.path());

    assert!(result.is_err());
}

#[test]
fn test_config_platform_url_without_scheme() {
    let content = r#"
        rtmp_addr = "127.0.0.1"
        rtmp_port = 1935
        stream_key = "test"

        [[platform]]
        url = "example.com/live"
        key = "key"
    "#;

    let file = create_temp_config(content);
    let result = Config::from_file(file.path());

    // URL without scheme should fail to parse
    assert!(result.is_err());
    let error_msg = result.unwrap_err().to_string();
    assert!(
        error_msg.contains("relative URL without a base") || error_msg.contains("missing field")
    );
}

#[test]
fn test_config_empty_fields() {
    let content = r#"
        rtmp_addr = ""
        rtmp_port = 1935
        stream_key = ""
    "#;

    let file = create_temp_config(content);
    let config = Config::from_file(file.path()).unwrap();

    // Empty strings are allowed by current implementation
    assert_eq!(config.rtmp_addr, "");
    assert_eq!(config.stream_key, "");
}
