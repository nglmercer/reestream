use std::io::{self, Write};
use std::path::Path;

use crate::config::{Config, ConfigBuilder, Orientation};

pub fn is_first_run(config_path: &Path) -> bool {
    !config_path.exists()
}

pub fn run_cli_wizard(config_path: &Path) -> Result<Config, Box<dyn std::error::Error>> {
    println!();
    println!("╔══════════════════════════════════════════╗");
    println!("║       Reestream First-Time Setup         ║");
    println!("╚══════════════════════════════════════════╝");
    println!();

    let rtmp_addr = prompt("RTMP bind address", "0.0.0.0");
    let rtmp_port = prompt("RTMP port", "1935").parse::<u16>().unwrap_or(1935);
    let stream_key = prompt_secret("Stream key (for publishing)");

    let mut builder = ConfigBuilder::new()
        .addr(&rtmp_addr)
        .port(rtmp_port)
        .stream_key(&stream_key);

    println!();
    println!("── Output Platforms ──");
    println!("Add platforms to forward streams to (leave URL empty to stop):");
    println!();

    let mut idx = 1;
    loop {
        println!("── Platform {} ──", idx);
        let url = prompt("  RTMP URL (empty to skip)", "");
        if url.is_empty() {
            break;
        }
        let key = prompt_secret("  Stream key");
        let orientation = prompt("  Orientation (horizontal/vertical)", "horizontal");
        let orientation = match orientation.to_lowercase().as_str() {
            "vertical" | "v" | "9:16" => Orientation::Vertical,
            _ => Orientation::Horizontal,
        };

        match url::Url::parse(&url) {
            Ok(parsed_url) => {
                builder = builder.add_platform(parsed_url, &key, orientation);
                println!("  ✓ Added\n");
            }
            Err(e) => {
                println!("  ✗ Invalid URL: {e}, skipping\n");
            }
        }
        idx += 1;
    }

    let config = builder.build();

    if let Err(e) = config.validate() {
        return Err(format!("Config validation failed: {e}").into());
    }

    let toml_content = config.to_toml()?;
    std::fs::write(config_path, &toml_content)?;

    println!();
    println!("✓ Configuration saved to {}", config_path.display());
    println!();
    println!("  RTMP: {}:{}", config.rtmp_addr, config.rtmp_port);
    println!("  Key:  {}", config.stream_key);
    println!(
        "  Platforms: {}",
        config.platform.as_ref().map_or(0, |p| p.len())
    );
    println!();
    println!("Run 'reestream' to start the server.");
    println!("Or open http://localhost:8080 for the web dashboard.");
    println!();

    Ok(config)
}

fn prompt(label: &str, default: &str) -> String {
    if default.is_empty() {
        print!("{label}: ");
    } else {
        print!("{label} [{default}]: ");
    }
    io::stdout().flush().unwrap();

    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    let input = input.trim();

    if input.is_empty() {
        default.to_string()
    } else {
        input.to_string()
    }
}

fn prompt_secret(label: &str) -> String {
    print!("{label}: ");
    io::stdout().flush().unwrap();

    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    input.trim().to_string()
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SetupStatus {
    pub first_run: bool,
    pub config_exists: bool,
    pub has_stream_key: bool,
    pub platform_count: usize,
}

pub fn get_setup_status(config_path: &Path) -> SetupStatus {
    let config_exists = config_path.exists();

    if !config_exists {
        return SetupStatus {
            first_run: true,
            config_exists: false,
            has_stream_key: false,
            platform_count: 0,
        };
    }

    match Config::from_file(config_path) {
        Ok(config) => {
            let has_stream_key = !config.stream_key.is_empty()
                && config.stream_key != "your-key"
                && config.stream_key != "test-key";
            let platform_count = config.platform.as_ref().map_or(0, |p| p.len());
            SetupStatus {
                // Output platforms are optional: a valid setup may only expose
                // the local RTMP ingest endpoint until platforms are added later.
                first_run: !has_stream_key,
                config_exists: true,
                has_stream_key,
                platform_count,
            }
        }
        Err(_) => SetupStatus {
            first_run: true,
            config_exists: true,
            has_stream_key: false,
            platform_count: 0,
        },
    }
}

#[derive(Debug, serde::Deserialize)]
pub struct SetupRequest {
    pub rtmp_addr: Option<String>,
    pub rtmp_port: Option<u16>,
    pub stream_key: String,
    pub platforms: Vec<SetupPlatform>,
}

#[derive(Debug, serde::Deserialize)]
pub struct SetupPlatform {
    pub name: String,
    pub url: String,
    pub key: String,
    pub orientation: Option<String>,
}

pub fn apply_setup(
    config_path: &Path,
    req: &SetupRequest,
) -> Result<Config, Box<dyn std::error::Error>> {
    let mut builder = ConfigBuilder::new()
        .addr(req.rtmp_addr.as_deref().unwrap_or("0.0.0.0"))
        .port(req.rtmp_port.unwrap_or(1935))
        .stream_key(&req.stream_key);

    for p in &req.platforms {
        let url = url::Url::parse(&p.url)?;
        let orientation = match p.orientation.as_deref() {
            Some("vertical") => Orientation::Vertical,
            _ => Orientation::Horizontal,
        };
        builder = builder.add_platform(url, &p.key, orientation);
    }

    let config = builder.build();
    config.validate()?;

    let toml_content = config.to_toml()?;
    std::fs::write(config_path, toml_content)?;

    Ok(config)
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ServerInfo {
    pub rtmp_url: String,
    pub rtmps_url: Option<String>,
    pub srt_url: Option<String>,
    pub http_url: String,
    pub hls_url: String,
    pub flv_url: String,
    pub dashboard_url: String,
    pub api_url: String,
    pub metrics_url: String,
    pub stream_key_masked: String,
    pub rtmp_port: u16,
    pub http_port: u16,
    pub srt_port: u16,
    pub hostname: String,
}

pub fn get_server_info(config_path: &Path) -> Result<ServerInfo, Box<dyn std::error::Error>> {
    let config = Config::from_file(config_path)?;

    let hostname = std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("COMPUTERNAME"))
        .unwrap_or_else(|_| "localhost".to_string());

    let rtmp_port = config.rtmp_port;
    let http_port = 8080;
    let srt_port = 3000;

    let key = &config.stream_key;
    let masked = if key.len() <= 4 {
        "****".to_string()
    } else {
        format!("{}…{}", &key[..4], &key[key.len() - 4..])
    };

    let rtmp_url = format!("rtmp://{hostname}:{rtmp_port}");
    let rtmps_url = Some(format!("rtmps://{hostname}:{rtmp_port}"));
    let srt_url = Some(format!("srt://{hostname}:{srt_port}"));
    let http_url = format!("http://{hostname}:{http_port}");

    Ok(ServerInfo {
        rtmp_url,
        rtmps_url,
        srt_url,
        http_url: http_url.clone(),
        hls_url: format!("{http_url}/stream.m3u8"),
        flv_url: format!("{http_url}/stream.flv"),
        dashboard_url: http_url.clone(),
        api_url: format!("{http_url}/api/status"),
        metrics_url: format!("{http_url}/metrics"),
        stream_key_masked: masked,
        rtmp_port,
        http_port,
        srt_port,
        hostname,
    })
}

pub fn get_stream_key(config_path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let config = Config::from_file(config_path)?;
    Ok(config.stream_key)
}

pub fn reset_stream_key(config_path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let mut config = Config::from_file(config_path)?;

    let new_key = uuid::Uuid::new_v4().to_string();
    config.stream_key = new_key.clone();

    let toml_content = config.to_toml()?;
    std::fs::write(config_path, toml_content)?;

    Ok(new_key)
}

pub fn read_config(config_path: &Path) -> Result<Config, Box<dyn std::error::Error>> {
    Config::from_file(config_path)
}

pub fn save_config(config_path: &Path, config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    let toml_content = config.to_toml()?;
    std::fs::write(config_path, toml_content)?;
    Ok(())
}

pub fn update_config_fields(
    config_path: &Path,
    rtmp_addr: Option<&str>,
    rtmp_port: Option<u16>,
    stream_key: Option<&str>,
) -> Result<Config, Box<dyn std::error::Error>> {
    let mut config = Config::from_file(config_path)?;

    if let Some(addr) = rtmp_addr {
        config.rtmp_addr = addr.to_string();
    }
    if let Some(port) = rtmp_port {
        config.rtmp_port = port;
    }
    if let Some(key) = stream_key {
        config.stream_key = key.to_string();
    }

    let toml_content = config.to_toml()?;
    std::fs::write(config_path, toml_content)?;

    Ok(config)
}

pub fn add_platform_to_config(
    config_path: &Path,
    url: &str,
    key: &str,
    orientation: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut config = Config::from_file(config_path)?;
    let parsed_url = url::Url::parse(url)?;
    let orient = match orientation {
        "vertical" => crate::config::Orientation::Vertical,
        _ => crate::config::Orientation::Horizontal,
    };

    let platforms = config.platform.get_or_insert_with(Vec::new);
    platforms.push(crate::config::Platform {
        url: parsed_url,
        key: key.to_string(),
        enabled: true,
        orientation: orient,
    });

    let toml_content = config.to_toml()?;
    std::fs::write(config_path, toml_content)?;
    Ok(())
}

pub fn update_platform_in_config(
    config_path: &Path,
    index: usize,
    url: Option<&str>,
    key: Option<&str>,
    orientation: Option<&str>,
) -> Result<bool, Box<dyn std::error::Error>> {
    let mut config = Config::from_file(config_path)?;

    let platforms = match config.platform.as_mut() {
        Some(p) => p,
        None => return Ok(false),
    };

    let platform = match platforms.get_mut(index) {
        Some(p) => p,
        None => return Ok(false),
    };

    if let Some(u) = url {
        platform.url = url::Url::parse(u)?;
    }
    if let Some(k) = key {
        platform.key = k.to_string();
    }
    if let Some(o) = orientation {
        platform.orientation = match o {
            "vertical" => crate::config::Orientation::Vertical,
            _ => crate::config::Orientation::Horizontal,
        };
    }

    let toml_content = config.to_toml()?;
    std::fs::write(config_path, toml_content)?;
    Ok(true)
}

pub fn remove_platform_from_config(
    config_path: &Path,
    index: usize,
) -> Result<bool, Box<dyn std::error::Error>> {
    let mut config = Config::from_file(config_path)?;

    let platforms = match config.platform.as_mut() {
        Some(p) => p,
        None => return Ok(false),
    };

    if index >= platforms.len() {
        return Ok(false);
    }

    platforms.remove(index);

    let toml_content = config.to_toml()?;
    std::fs::write(config_path, toml_content)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_first_run_no_file() {
        let path = std::env::temp_dir().join("reestream_test_noexist_setup.toml");
        let _ = std::fs::remove_file(&path);
        assert!(is_first_run(&path));
    }

    #[test]
    fn test_is_first_run_with_file() {
        let dir = std::env::temp_dir().join("reestream_test_setup_exist");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("config.toml");
        std::fs::write(&path, "test").unwrap();
        assert!(!is_first_run(&path));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_get_setup_status_no_config() {
        let path = std::env::temp_dir().join("reestream_test_nostatus.toml");
        let _ = std::fs::remove_file(&path);
        let status = get_setup_status(&path);
        assert!(status.first_run);
        assert!(!status.config_exists);
    }

    #[test]
    fn test_get_setup_status_valid_config() {
        let dir = std::env::temp_dir().join("reestream_test_status_ok");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("config.toml");
        std::fs::write(
            &path,
            r#"rtmp_addr = "0.0.0.0"
rtmp_port = 1935
stream_key = "real-key-here"
[[platform]]
url = "rtmp://twitch.tv/app"
key = "key"
orientation = "horizontal"
"#,
        )
        .unwrap();
        let status = get_setup_status(&path);
        assert!(!status.first_run);
        assert!(status.config_exists);
        assert!(status.has_stream_key);
        assert_eq!(status.platform_count, 1);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_get_setup_status_default_key() {
        let dir = std::env::temp_dir().join("reestream_test_status_default");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("config.toml");
        std::fs::write(
            &path,
            r#"rtmp_addr = "0.0.0.0"
rtmp_port = 1935
stream_key = "test-key"
"#,
        )
        .unwrap();
        let status = get_setup_status(&path);
        assert!(status.first_run);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_get_setup_status_without_platforms_is_complete() {
        let dir = std::env::temp_dir().join("reestream_test_status_no_platforms");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("config.toml");
        std::fs::write(
            &path,
            r#"rtmp_addr = "0.0.0.0"
rtmp_port = 1935
stream_key = "local-ingest-key"
"#,
        )
        .unwrap();

        let status = get_setup_status(&path);
        assert!(!status.first_run);
        assert!(status.has_stream_key);
        assert_eq!(status.platform_count, 0);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_apply_setup() {
        let dir = std::env::temp_dir().join("reestream_test_apply_setup");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("config.toml");
        let _ = std::fs::remove_file(&path);

        let req = SetupRequest {
            rtmp_addr: Some("127.0.0.1".into()),
            rtmp_port: Some(9999),
            stream_key: "my-new-key".into(),
            platforms: vec![SetupPlatform {
                name: "Twitch".into(),
                url: "rtmp://live.twitch.tv/app".into(),
                key: "twitch-key".into(),
                orientation: Some("horizontal".into()),
            }],
        };

        let config = apply_setup(&path, &req).unwrap();
        assert_eq!(config.rtmp_addr, "127.0.0.1");
        assert_eq!(config.rtmp_port, 9999);
        assert_eq!(config.stream_key, "my-new-key");
        assert_eq!(config.platform.as_ref().unwrap().len(), 1);
        assert!(path.exists());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_read_config() {
        let dir = std::env::temp_dir().join("reestream_test_read_config");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("config.toml");
        std::fs::write(
            &path,
            r#"rtmp_addr = "1.2.3.4"
rtmp_port = 9999
stream_key = "read-test-key"
"#,
        )
        .unwrap();
        let config = read_config(&path).unwrap();
        assert_eq!(config.rtmp_addr, "1.2.3.4");
        assert_eq!(config.rtmp_port, 9999);
        assert_eq!(config.stream_key, "read-test-key");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_save_config() {
        let dir = std::env::temp_dir().join("reestream_test_save_config");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("config.toml");

        let config = ConfigBuilder::new()
            .addr("5.6.7.8")
            .port(1234)
            .stream_key("save-key")
            .build();

        save_config(&path, &config).unwrap();
        let loaded = Config::from_file(&path).unwrap();
        assert_eq!(loaded.rtmp_addr, "5.6.7.8");
        assert_eq!(loaded.rtmp_port, 1234);
        assert_eq!(loaded.stream_key, "save-key");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_update_config_fields() {
        let dir = std::env::temp_dir().join("reestream_test_update_fields");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("config.toml");
        std::fs::write(
            &path,
            r#"rtmp_addr = "0.0.0.0"
rtmp_port = 1935
stream_key = "old-key"
"#,
        )
        .unwrap();

        let config =
            update_config_fields(&path, Some("10.0.0.1"), Some(8080), Some("new-key")).unwrap();
        assert_eq!(config.rtmp_addr, "10.0.0.1");
        assert_eq!(config.rtmp_port, 8080);
        assert_eq!(config.stream_key, "new-key");

        // Verify persisted
        let loaded = Config::from_file(&path).unwrap();
        assert_eq!(loaded.rtmp_addr, "10.0.0.1");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_update_config_fields_partial() {
        let dir = std::env::temp_dir().join("reestream_test_update_partial");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("config.toml");
        std::fs::write(
            &path,
            r#"rtmp_addr = "0.0.0.0"
rtmp_port = 1935
stream_key = "keep-key"
"#,
        )
        .unwrap();

        let config = update_config_fields(&path, None, Some(443), None).unwrap();
        assert_eq!(config.rtmp_addr, "0.0.0.0"); // unchanged
        assert_eq!(config.rtmp_port, 443); // changed
        assert_eq!(config.stream_key, "keep-key"); // unchanged
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_add_platform_to_config() {
        let dir = std::env::temp_dir().join("reestream_test_add_plat");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("config.toml");
        std::fs::write(
            &path,
            r#"rtmp_addr = "0.0.0.0"
rtmp_port = 1935
stream_key = "key"
"#,
        )
        .unwrap();

        add_platform_to_config(&path, "rtmp://twitch.tv/app", "tw-key", "horizontal").unwrap();
        let config = Config::from_file(&path).unwrap();
        assert_eq!(config.platform.as_ref().unwrap().len(), 1);
        assert_eq!(config.platform.as_ref().unwrap()[0].key, "tw-key");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_update_platform_in_config() {
        let dir = std::env::temp_dir().join("reestream_test_update_plat");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("config.toml");
        std::fs::write(
            &path,
            r#"rtmp_addr = "0.0.0.0"
rtmp_port = 1935
stream_key = "key"

[[platform]]
url = "rtmp://twitch.tv/app"
key = "old-key"
orientation = "horizontal"
"#,
        )
        .unwrap();

        let updated = update_platform_in_config(
            &path,
            0,
            Some("rtmp://youtube.com/live2"),
            Some("yt-key"),
            Some("vertical"),
        )
        .unwrap();
        assert!(updated);
        let config = Config::from_file(&path).unwrap();
        let p = &config.platform.as_ref().unwrap()[0];
        assert_eq!(p.url.host_str(), Some("youtube.com"));
        assert_eq!(p.key, "yt-key");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_remove_platform_from_config() {
        let dir = std::env::temp_dir().join("reestream_test_remove_plat");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("config.toml");
        std::fs::write(
            &path,
            r#"rtmp_addr = "0.0.0.0"
rtmp_port = 1935
stream_key = "key"

[[platform]]
url = "rtmp://twitch.tv/app"
key = "tw-key"
orientation = "horizontal"

[[platform]]
url = "rtmp://youtube.com/live2"
key = "yt-key"
orientation = "horizontal"
"#,
        )
        .unwrap();

        let removed = remove_platform_from_config(&path, 0).unwrap();
        assert!(removed);
        let config = Config::from_file(&path).unwrap();
        assert_eq!(config.platform.as_ref().unwrap().len(), 1);
        assert_eq!(config.platform.as_ref().unwrap()[0].key, "yt-key");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_remove_platform_from_config_out_of_bounds() {
        let dir = std::env::temp_dir().join("reestream_test_remove_oob");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("config.toml");
        std::fs::write(
            &path,
            r#"rtmp_addr = "0.0.0.0"
rtmp_port = 1935
stream_key = "key"
"#,
        )
        .unwrap();

        let removed = remove_platform_from_config(&path, 99).unwrap();
        assert!(!removed);
        let _ = std::fs::remove_file(&path);
    }
}
