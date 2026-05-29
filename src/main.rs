use clap::Parser;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::RwLock;
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

use reestream::client::handle_publisher;
use reestream::config::Config;

#[derive(clap::Parser)]
struct Args {
    /// Define config.toml path
    #[clap(long, short, default_value = "config.toml")]
    config: PathBuf,

    /// Enable JSON structured logging
    #[clap(long)]
    json_log: bool,

    /// Log level (trace, debug, info, warn, error)
    #[clap(long, default_value = "info")]
    log_level: String,

    /// Run interactive first-time setup wizard
    #[clap(long)]
    setup: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    if args.setup {
        return run_setup(&args.config);
    }

    if reestream::setup::is_first_run(&args.config) {
        eprintln!("No config file found at '{}'.", args.config.display());
        eprintln!(
            "Run with --setup to create one, or open http://localhost:8080 for the web setup."
        );
        eprintln!();
        eprintln!("  reestream --setup");
        eprintln!();

        // Create minimal config so the server can start and serve the dashboard
        let default_config = reestream::config::ConfigBuilder::new()
            .stream_key("")
            .build();
        let toml = default_config.to_toml()?;
        std::fs::write(&args.config, toml)?;
        eprintln!(
            "Created minimal config at '{}' — starting server for web setup.",
            args.config.display()
        );
        eprintln!();
    }

    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&args.log_level));

    if args.json_log {
        tracing_subscriber::fmt()
            .json()
            .with_env_filter(env_filter)
            .with_line_number(true)
            .init();
    } else {
        tracing_subscriber::fmt()
            .with_env_filter(env_filter)
            .with_line_number(true)
            .init();
    }

    let config = Config::from_file(&args.config)?;
    let Config {
        rtmp_addr,
        rtmp_port,
        stream_key,
        platform,
        ..
    } = &config;

    info!(
        addr = %rtmp_addr,
        port = %rtmp_port,
        platforms = %platform.clone().unwrap_or_default().len(),
        "Configuration loaded"
    );

    let addr: SocketAddr = format!("{rtmp_addr}:{rtmp_port}").parse()?;
    let listener = TcpListener::bind(addr).await?;
    info!("RTMP relay listening on {}", addr);

    let platforms = Arc::new(RwLock::new(platform.clone().unwrap_or_default()));

    let shutdown = Arc::new(reestream::hardening::GracefulShutdown::new());
    reestream::hardening::setup_signal_handlers(shutdown.clone()).await;

    #[cfg(feature = "srt")]
    {
        let srt_config = reestream::srt::SrtConfig {
            enabled: true,
            listen_port: 3000,
            ..Default::default()
        };
        if srt_config.enabled {
            let srt_listener = Arc::new(reestream::srt::SrtListener::new(srt_config));
            let srt_l = srt_listener.clone();
            tokio::spawn(async move {
                if let Err(e) = srt_l.run().await {
                    error!("SRT listener error: {}", e);
                }
            });
            info!("SRT listener started on port 3000");
        }
    }

    let connection_pool = Arc::new(reestream::hardening::ConnectionPool::new(1000));
    let rate_limiter = Arc::new(reestream::hardening::RateLimiter::new(100));

    // Create StreamManager and DataBus shared between HTTP server and RTMP handler
    #[cfg(any(feature = "hls", feature = "api"))]
    let (stream_manager, data_bus): (
        Option<Arc<reestream::http_server::stream::StreamManager>>,
        Option<Arc<dyn reestream::client::DataPublisher>>,
    ) = {
        let sm = Arc::new(reestream::http_server::stream::StreamManager::new());
        if let Some(ref config_platforms) = *platform {
            for cp in config_platforms {
                let name = cp.url.host_str().unwrap_or("unknown").to_string();
                sm.add_platform(name, cp.url.to_string(), cp.key.clone())
                    .await;
            }
        }
        let hls_config = reestream::http_server::hls::HlsConfig::default();
        let recording_config = reestream::http_server::recording::RecordingConfig {
            enabled: true,
            output_dir: std::path::PathBuf::from("/tmp/reestream/recordings"),
            ..Default::default()
        };
        let data_bus = reestream::http_server::databus::DataBus::new();
        let flv_state = reestream::http_server::flv::FlvState::default();

        // Create HLS transmuxer (uses ffmpeg to create HLS segments from FLV data)
        let hls_segment_dir = std::path::PathBuf::from("/tmp/reestream/hls");
        let hls_transmuxer = reestream::http_server::hls_transmux::HlsTransmuxer::new(
            hls_segment_dir.clone(),
            hls_segment_dir.join("stream.m3u8"),
        );

        // Bridge DataBus → FlvState + HLS transmuxer + bitrate
        {
            let mut rx = data_bus.subscribe();
            let flv = flv_state.clone();
            let transmuxer = hls_transmuxer;
            let sm = sm.clone();
            tokio::spawn(async move {
                let mut hls_tx: Option<tokio::sync::mpsc::Sender<bytes::Bytes>> = None;
                let mut bytes_this_second: u64 = 0;
                let mut bitrate_interval = tokio::time::interval(std::time::Duration::from_secs(1));
                bitrate_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                let mut current_stream_id: Option<String> = None;

                loop {
                    tokio::select! {
                        biased;
                        _ = bitrate_interval.tick() => {
                            if let Some(ref stream_id) = current_stream_id {
                                let bitrate_kbps = (bytes_this_second * 8) / 1000;
                                sm.update_stream_stats(stream_id, 0, bitrate_kbps).await;
                                bytes_this_second = 0;
                            }
                        }
                        result = rx.recv() => {
                            match result {
                                Ok(packet) => {
                                    bytes_this_second += packet.data.len() as u64;
                                    if current_stream_id.is_none() {
                                        current_stream_id = Some(packet.stream_id.clone());
                                    }

                                    let tag_type = if packet.is_video { 0x09 } else { 0x08 };
                                    let flv_tag = reestream::http_server::flv::build_flv_tag(
                                        tag_type,
                                        packet.timestamp_ms,
                                        &packet.data,
                                    );

                                    flv.push_data(flv_tag.clone()).await;

                                    if hls_tx.is_none() {
                                        match transmuxer.start().await {
                                            Ok(tx) => {
                                                hls_tx = Some(tx);
                                                info!("HLS transmuxer started for stream");
                                            }
                                            Err(e) => {
                                                warn!("Failed to start HLS transmuxer: {e}");
                                            }
                                        }
                                    }

                                    if let Some(ref tx) = hls_tx {
                                        if tx.try_send(flv_tag).is_err() {
                                            transmuxer.stop().await;
                                            match transmuxer.start().await {
                                                Ok(new_tx) => {
                                                    hls_tx = Some(new_tx);
                                                    info!("HLS transmuxer restarted");
                                                }
                                                Err(e) => {
                                                    warn!("Failed to restart HLS transmuxer: {e}");
                                                    hls_tx = None;
                                                }
                                            }
                                        }
                                    }
                                }
                                Err(_) => break,
                            }
                        }
                    }
                }

                transmuxer.stop().await;
                info!("HLS transmuxer stopped (stream ended)");
            });
        }

        let data_bus_arc: Arc<dyn reestream::client::DataPublisher> = Arc::new(data_bus.clone());
        let app_state = reestream::http_server::http::AppState {
            stream_manager: sm.clone(),
            hls_segmenter: Arc::new(reestream::http_server::hls::HlsSegmenter::new(hls_config)),
            flv_state,
            data_bus,
            recording_manager: Arc::new(reestream::http_server::recording::RecordingManager::new(
                recording_config,
            )),
            start_time: std::time::Instant::now(),
            config_path: args.config.clone(),
        };
        tokio::spawn(async move {
            if let Err(e) =
                reestream::http_server::http::start_http_server("0.0.0.0", 8080, app_state).await
            {
                error!("HTTP server error: {}", e);
            }
        });
        info!("HTTP server starting on 0.0.0.0:8080");
        (Some(sm), Some(data_bus_arc))
    };

    #[cfg(not(any(feature = "hls", feature = "api")))]
    let (stream_manager, data_bus): (
        Option<Arc<dyn reestream::client::StreamRegistrar>>,
        Option<Arc<dyn reestream::client::DataPublisher>>,
    ) = (None, None);

    if !stream_key.is_empty() {
        info!("Open http://localhost:8080 for the dashboard");
    } else {
        warn!("No stream key configured — open http://localhost:8080/setup to complete setup");
    }

    loop {
        tokio::select! {
            biased;

            _ = shutdown.wait_for_shutdown() => {
                info!("Graceful shutdown initiated, draining connections...");
                let drained = shutdown.drain_timeout(std::time::Duration::from_secs(30)).await;
                if drained {
                    info!("All connections drained successfully");
                } else {
                    warn!("Shutdown timeout reached, forcing exit");
                }
                break;
            }

            accept = listener.accept() => {
                match accept {
                    Ok((socket, peer_addr)) => {
                        if !rate_limiter.try_acquire().await {
                            warn!("Rate limit exceeded, rejecting connection from {}", peer_addr);
                            continue;
                        }

                        let _guard = match connection_pool.try_acquire().await {
                            Some(g) => g,
                            None => {
                                warn!("Connection pool full, rejecting connection from {}", peer_addr);
                                continue;
                            }
                        };

                        if let Err(e) = socket.set_nodelay(true) {
                            warn!("Failed to set_nodelay on incoming socket: {}", e);
                        }

                        info!("New incoming connection from {}", peer_addr);
                        let platforms = platforms.clone();
                        let stream_key = stream_key.clone();
                        let registrar: Option<Arc<dyn reestream::client::StreamRegistrar>> = stream_manager.clone().map(|sm| sm as Arc<dyn reestream::client::StreamRegistrar>);
                        let pubber = data_bus.clone();
                        tokio::spawn(async move {
                            if let Err(e) = handle_publisher(socket, platforms, stream_key, registrar, pubber).await {
                                error!("Error in connection from {}: {:#}", peer_addr, e);
                            } else {
                                info!("Connection from {} ended correctly", peer_addr);
                            }
                        });
                    }
                    Err(e) => {
                        warn!("Error accepting connection: {}", e);
                    }
                }
            }
        }
    }

    info!("Reestream shutdown complete");
    Ok(())
}

fn run_setup(config_path: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    reestream::setup::run_cli_wizard(config_path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use reestream::AsyncReadWrite;

    fn parse_socket_addr(addr: &str, port: u16) -> Result<SocketAddr, Box<dyn std::error::Error>> {
        let addr: SocketAddr = format!("{addr}:{port}").parse()?;
        Ok(addr)
    }

    #[test]
    fn test_parse_socket_addr_valid() {
        let addr = parse_socket_addr("0.0.0.0", 1935).unwrap();
        assert_eq!(addr, "0.0.0.0:1935".parse::<SocketAddr>().unwrap());
    }

    #[test]
    fn test_parse_socket_addr_localhost() {
        let addr = parse_socket_addr("127.0.0.1", 8080).unwrap();
        assert_eq!(addr, "127.0.0.1:8080".parse::<SocketAddr>().unwrap());
    }

    #[test]
    fn test_parse_socket_addr_invalid() {
        let result = parse_socket_addr("not-an-address", 1935);
        assert!(result.is_err());
    }

    #[test]
    fn test_args_default_config() {
        let args = Args::try_parse_from(["reestream"]).unwrap();
        assert_eq!(args.config, PathBuf::from("config.toml"));
        assert!(!args.json_log);
        assert_eq!(args.log_level, "info");
        assert!(!args.setup);
    }

    #[test]
    fn test_args_setup_flag() {
        let args = Args::try_parse_from(["reestream", "--setup"]).unwrap();
        assert!(args.setup);
    }

    #[test]
    fn test_args_custom_config_short() {
        let args = Args::try_parse_from(["reestream", "-c", "/tmp/myconfig.toml"]).unwrap();
        assert_eq!(args.config, PathBuf::from("/tmp/myconfig.toml"));
    }

    #[test]
    fn test_args_custom_config_long() {
        let args =
            Args::try_parse_from(["reestream", "--config", "/etc/reestream/config.toml"]).unwrap();
        assert_eq!(args.config, PathBuf::from("/etc/reestream/config.toml"));
    }

    #[test]
    fn test_args_json_log() {
        let args = Args::try_parse_from(["reestream", "--json-log"]).unwrap();
        assert!(args.json_log);
    }

    #[test]
    fn test_args_log_level() {
        let args = Args::try_parse_from(["reestream", "--log-level", "debug"]).unwrap();
        assert_eq!(args.log_level, "debug");
    }

    #[test]
    fn test_async_read_write_trait_bounds() {
        fn _assert_impl<T: AsyncReadWrite>() {}
        _assert_impl::<tokio::net::TcpStream>();
    }
}
