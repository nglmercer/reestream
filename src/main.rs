use clap::Parser;
#[cfg(all(feature = "srt", any(feature = "hls", feature = "api")))]
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
#[cfg(all(feature = "srt", any(feature = "hls", feature = "api")))]
use std::time::{Duration, Instant};
#[cfg(all(feature = "srt", any(feature = "hls", feature = "api")))]
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;
#[cfg(all(feature = "srt", any(feature = "hls", feature = "api")))]
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::RwLock;
#[cfg(any(feature = "hls", feature = "api"))]
use tokio::sync::broadcast;
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

use reestream::config::Config;
#[cfg(any(feature = "hls", feature = "api"))]
use reestream::config::{PlatformEvent, platform_id_from};

type StreamManagerPair = (
    Option<Arc<dyn reestream::client::StreamRegistrar>>,
    Option<Arc<dyn reestream::client::DataPublisher>>,
    Option<tokio::sync::broadcast::Receiver<reestream::config::PlatformEvent>>,
);

#[cfg(any(feature = "hls", feature = "api"))]
async fn sync_product_platform(
    platforms: &Arc<RwLock<Vec<reestream::config::Platform>>>,
    event: &PlatformEvent,
) {
    let mut configured = platforms.write().await;
    match event {
        PlatformEvent::Added {
            platform_id,
            url,
            key,
        } => {
            let exists = configured.iter().any(|platform| {
                platform_id_from(platform.url.as_str(), &platform.key) == *platform_id
            });
            if !exists && let Ok(url) = url::Url::parse(url) {
                configured.push(reestream::config::Platform {
                    url,
                    key: key.clone(),
                    enabled: true,
                    orientation: Default::default(),
                });
            }
        }
        PlatformEvent::Removed { platform_id } => {
            configured.retain(|platform| {
                platform_id_from(platform.url.as_str(), &platform.key) != *platform_id
            });
        }
        PlatformEvent::Toggled {
            platform_id,
            url,
            key,
            enabled,
        } => {
            if let Some(platform) = configured.iter_mut().find(|platform| {
                platform_id_from(platform.url.as_str(), &platform.key) == *platform_id
            }) {
                platform.enabled = *enabled;
                platform.key = key.clone();
                if let Ok(url) = url::Url::parse(url) {
                    platform.url = url;
                }
            } else if *enabled && let Ok(url) = url::Url::parse(url) {
                configured.push(reestream::config::Platform {
                    url,
                    key: key.clone(),
                    enabled: true,
                    orientation: Default::default(),
                });
            }
        }
    }
}

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
        eprintln!("Run with --setup, or open the configured HTTP endpoint for the web setup.");
        eprintln!();
        eprintln!("  reestream --setup");
        eprintln!();

        // Create minimal config so the server can start and serve the dashboard
        let default_config = reestream::config::ConfigBuilder::new()
            .stream_key("")
            .build();
        let toml = default_config.to_toml()?;
        reestream::setup::write_config_file(&args.config, &toml)?;
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

    let connection_pool = Arc::new(reestream::hardening::ConnectionPool::new(1000));
    let rate_limiter = Arc::new(reestream::hardening::RateLimiter::new(100));

    #[cfg(any(feature = "hls", feature = "api"))]
    let restream_store = Arc::new(
        reestream::http_server::restream::RestreamStore::with_runtime_config(
            args.config.with_extension("state.json"),
            rtmp_addr.clone(),
            *rtmp_port,
            stream_key.clone(),
            platforms.clone(),
        ),
    );
    #[cfg(any(feature = "hls", feature = "api"))]
    let publish_resolver: Option<Arc<dyn reestream::client::PublishKeyResolver>> =
        Some(restream_store.clone() as Arc<dyn reestream::client::PublishKeyResolver>);
    #[cfg(not(any(feature = "hls", feature = "api")))]
    let publish_resolver: Option<Arc<dyn reestream::client::PublishKeyResolver>> = None;
    #[cfg(any(feature = "hls", feature = "api"))]
    let status_reporter: Option<Arc<dyn reestream::client::DestinationStatusReporter>> =
        Some(restream_store.clone() as Arc<dyn reestream::client::DestinationStatusReporter>);
    #[cfg(not(any(feature = "hls", feature = "api")))]
    let status_reporter: Option<Arc<dyn reestream::client::DestinationStatusReporter>> = None;

    #[cfg(all(feature = "srt", any(feature = "hls", feature = "api")))]
    {
        let enabled = std::env::var("RESTREAM_SRT_ENABLED")
            .map(|value| matches!(value.as_str(), "1" | "true" | "yes"))
            .unwrap_or(false);
        let passphrase = std::env::var("RESTREAM_SRT_PASSPHRASE")
            .ok()
            .filter(|value| !value.trim().is_empty());
        let listen_port = std::env::var("RESTREAM_SRT_PORT")
            .ok()
            .and_then(|value| value.parse::<u16>().ok())
            .unwrap_or(3000);
        if enabled && passphrase.is_some() {
            let srt_config = reestream::srt::SrtConfig {
                enabled: true,
                listen_port,
                passphrase,
                ..Default::default()
            };
            let srt_listener = Arc::new(reestream::srt::SrtListener::new(srt_config));
            let mut srt_packets = srt_listener.subscribe_packets();
            let fallback_key = stream_key.clone();
            let srt_rtmp_port = *rtmp_port;
            let srt_store = restream_store.clone();
            tokio::spawn(async move {
                forward_srt_packets(&mut srt_packets, fallback_key, srt_rtmp_port, srt_store).await;
            });

            let srt_l = srt_listener.clone();
            tokio::spawn(async move {
                if let Err(error) = srt_l.run().await {
                    error!("SRT listener error: {}", error);
                }
            });
            info!("SRT listener started on port {}", listen_port);
        } else if enabled {
            warn!("SRT requested but disabled because RESTREAM_SRT_PASSPHRASE is missing");
        }
    }

    // Create StreamManager and DataBus shared between HTTP server and RTMP handler
    #[cfg(any(feature = "hls", feature = "api"))]
    let (stream_manager, data_bus, platform_event_rx): StreamManagerPair = {
        let sm = Arc::new(reestream::http_server::stream::StreamManager::new());
        let platform_event_rx = sm.subscribe_platform_events();
        let mut stream_manager_platform_events = sm.subscribe_platform_events();
        let core_platforms_for_stream_manager = platforms.clone();
        tokio::spawn(async move {
            loop {
                match stream_manager_platform_events.recv().await {
                    Ok(event) => {
                        sync_product_platform(&core_platforms_for_stream_manager, &event).await;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        });
        if let Some(ref config_platforms) = *platform {
            for cp in config_platforms {
                let name = cp.url.host_str().unwrap_or("unknown").to_string();
                let id = sm
                    .add_platform(name, cp.url.to_string(), cp.key.clone())
                    .await;
                if !cp.enabled {
                    sm.toggle_platform(&id, false).await;
                }
            }
        }
        // Restore API-managed destinations on startup and bridge future
        // channel changes into the relay's existing PlatformEvent pipeline.
        for channel in restream_store.list_channels().await {
            sync_product_platform(
                &platforms,
                &PlatformEvent::Added {
                    platform_id: reestream::config::platform_id_from(
                        &channel.stream_url,
                        &channel.stream_key,
                    ),
                    url: channel.stream_url.clone(),
                    key: channel.stream_key.clone(),
                },
            )
            .await;
            let id = sm
                .add_platform(
                    channel.display_name.clone(),
                    channel.stream_url.clone(),
                    channel.stream_key.clone(),
                )
                .await;
            if !channel.enabled {
                sm.toggle_platform(&id, false).await;
            }
        }
        let mut product_platform_events = restream_store.subscribe_platform_events();
        let sm_for_product_events = sm.clone();
        let core_platforms_for_product_events = platforms.clone();
        tokio::spawn(async move {
            loop {
                match product_platform_events.recv().await {
                    Ok(event) => {
                        sync_product_platform(&core_platforms_for_product_events, &event).await;
                        sm_for_product_events.apply_platform_event(event).await;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        });
        let (http_addr, http_port) = configured_http_endpoint();
        let hls_segment_dir = std::env::var_os("RESTREAM_HLS_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| restream_store.storage_root_path().join("hls"));
        let hls_config = reestream::http_server::hls::HlsConfig {
            segment_dir: hls_segment_dir.clone(),
            playlist_path: hls_segment_dir.join("stream.m3u8"),
            http_addr: http_addr.clone(),
            http_port,
            ..Default::default()
        };
        let ffmpeg_path = configured_ffmpeg_path(&args.config);
        let recording_config = reestream::http_server::recording::RecordingConfig {
            enabled: std::env::var("RESTREAM_RECORDING_ENABLED")
                .map(|value| matches!(value.as_str(), "1" | "true" | "yes"))
                .unwrap_or(true),
            output_dir: restream_store.storage_root_path().join("recordings"),
            ffmpeg_path: ffmpeg_path.clone(),
            ..Default::default()
        };
        let recording_manager = Arc::new(reestream::http_server::recording::RecordingManager::new(
            recording_config,
        ));
        let playback_manager = Arc::new(
            reestream::http_server::playback::PlaybackManager::with_ffmpeg_path(
                ffmpeg_path.clone(),
            ),
        );
        let scheduler_store = restream_store.clone();
        let scheduler_recording_manager = recording_manager.clone();
        let scheduler_playback_manager = playback_manager.clone();
        let scheduler_shutdown = shutdown.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tokio::select! {
                        _ = interval.tick() => {
                            for event in scheduler_store.promote_due_events().await {
                                let playback = reestream::http_server::api_v1::start_event_playback(
                                    &scheduler_playback_manager,
                                    &scheduler_store,
                                    &event,
                                ).await;
                                if playback.is_ok() {
                                    let _ = reestream::http_server::api_v1::start_event_recording(
                                        &scheduler_recording_manager,
                                        &scheduler_store,
                                        &event,
                                    ).await;
                                } else {
                                    let _ = scheduler_store.cancel_event(&event.id).await;
                                }
                            }
                    }
                    _ = scheduler_shutdown.wait_for_shutdown() => break,
                }
            }
        });
        let data_bus = reestream::http_server::databus::DataBus::new();
        let flv_state = reestream::http_server::flv::FlvState::default();

        // Create HLS transmuxer (uses ffmpeg to create HLS segments from FLV data)
        let hls_transmuxer = reestream::http_server::hls_transmux::HlsTransmuxer::new_with_ffmpeg(
            hls_segment_dir.clone(),
            hls_segment_dir.join("stream.m3u8"),
            ffmpeg_path,
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
                                    if current_stream_id.as_deref() != Some(packet.stream_id.as_str()) {
                                        if hls_tx.is_some() {
                                            transmuxer.stop().await;
                                            hls_tx = None;
                                        }
                                        flv.clear().await;
                                        current_stream_id = Some(packet.stream_id.clone());
                                        bytes_this_second = 0;
                                    }
                                    bytes_this_second += packet.data.len() as u64;

                                    let tag_type = if packet.is_video { 0x09 } else { 0x08 };
                                    let flv_tag = reestream::http_server::flv::build_flv_tag(
                                        tag_type,
                                        packet.timestamp_ms,
                                        &packet.data,
                                    );

                                    // Detect and store sequence headers for new FLV viewers
                                    if packet.is_video && packet.data.len() > 1 && packet.data[0] == 0x17 && packet.data[1] == 0x00 {
                                        flv.set_video_header(flv_tag.clone()).await;
                                    } else if !packet.is_video && packet.data.len() > 1 && (packet.data[0] & 0xF0) == 0xA0 && packet.data[1] == 0x00 {
                                        flv.set_audio_header(flv_tag.clone()).await;
                                    }

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

                                    if let Some(ref tx) = hls_tx
                                        && tx.try_send(flv_tag).is_err()
                                    {
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
                                Err(broadcast::error::RecvError::Lagged(n)) => {
                                    warn!("DataBus receiver lagged by {} messages, continuing", n);
                                }
                                Err(broadcast::error::RecvError::Closed) => {
                                    info!("DataBus channel closed, stopping bridge");
                                    break;
                                }
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
            recording_manager,
            playback_manager,
            start_time: std::time::Instant::now(),
            config_path: args.config.clone(),
            restream: restream_store,
        };
        let http_addr_for_server = http_addr.clone();
        tokio::spawn(async move {
            if let Err(e) = reestream::http_server::http::start_http_server(
                &http_addr_for_server,
                http_port,
                app_state,
            )
            .await
            {
                error!("HTTP server error: {}", e);
            }
        });
        info!("HTTP server starting on {}:{}", http_addr, http_port);
        (Some(sm), Some(data_bus_arc), Some(platform_event_rx))
    };

    #[cfg(not(any(feature = "hls", feature = "api")))]
    let (stream_manager, data_bus, platform_event_rx): StreamManagerPair = {
        let (_, rx) = tokio::sync::broadcast::channel(1);
        (None, None, Some(rx))
    };

    if !stream_key.is_empty() {
        info!("Open the configured HTTP endpoint for the dashboard");
    } else {
        warn!(
            "No stream key configured — open the configured HTTP endpoint at /setup to complete setup"
        );
    }

    loop {
        tokio::select! {
            biased;

            _ = shutdown.wait_for_shutdown() => {
                info!("Graceful shutdown initiated, draining connections...");
                let drained = connection_pool
                    .drain_timeout(std::time::Duration::from_secs(30))
                    .await;
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

                        let connection_guard = match connection_pool.try_acquire().await {
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
                        let stream_key = if publish_resolver.is_some() {
                            String::new()
                        } else {
                            stream_key.clone()
                        };
                        let registrar: Option<Arc<dyn reestream::client::StreamRegistrar>> = stream_manager.clone().map(|sm| sm as Arc<dyn reestream::client::StreamRegistrar>);
                        let pubber = data_bus.clone();
                        let pev = platform_event_rx.as_ref().unwrap().resubscribe();
                        let resolver = publish_resolver.clone();
                        let reporter = status_reporter.clone();
                        tokio::spawn(async move {
                            let _connection_guard = connection_guard;
                            if let Err(e) = reestream::client::handle_publisher_with_resolver_and_status(
                                socket,
                                platforms,
                                stream_key,
                                registrar,
                                pubber,
                                pev,
                                resolver,
                                reporter,
                            )
                            .await
                            {
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

#[cfg(any(feature = "hls", feature = "api"))]
fn configured_ffmpeg_path(_config_path: &std::path::Path) -> PathBuf {
    if let Some(path) = std::env::var_os("RESTREAM_FFMPEG_PATH") {
        return PathBuf::from(path);
    }
    #[cfg(feature = "ffmpeg")]
    {
        let data_dir = _config_path
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."))
            .join(".reestream");
        let resolver = reestream::ffmpeg::BinaryResolver::new(data_dir);
        if let Ok(path) = resolver.find_ffmpeg() {
            return path;
        }
    }
    PathBuf::from("ffmpeg")
}

#[cfg(any(feature = "hls", feature = "api"))]
fn configured_http_endpoint() -> (String, u16) {
    let addr = std::env::var("RESTREAM_HTTP_ADDR").unwrap_or_else(|_| "0.0.0.0".into());
    let port = std::env::var("RESTREAM_HTTP_PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(8080);
    (addr, port)
}

#[cfg(all(feature = "srt", any(feature = "hls", feature = "api")))]
struct SrtForwardProcess {
    stdin: ChildStdin,
    child: Child,
    last_packet: Instant,
}

#[cfg(all(feature = "srt", any(feature = "hls", feature = "api")))]
fn is_safe_srt_stream_id(stream_id: &str) -> bool {
    !stream_id.is_empty()
        && stream_id.len() <= 256
        && stream_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

#[cfg(all(feature = "srt", any(feature = "hls", feature = "api")))]
async fn forward_srt_packets(
    packets: &mut tokio::sync::broadcast::Receiver<reestream::srt::SrtPacket>,
    fallback_key: String,
    rtmp_port: u16,
    store: Arc<reestream::http_server::restream::RestreamStore>,
) {
    const MAX_BRIDGES: usize = 8;
    let mut forwards = HashMap::<String, SrtForwardProcess>::new();
    let mut cleanup = tokio::time::interval(Duration::from_secs(10));
    cleanup.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            packet = packets.recv() => {
                let packet = match packet {
                    Ok(packet) => packet,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(count)) => {
                        warn!("SRT packet bridge lagged by {} packets", count);
                        continue;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                };
                let stream_key = packet
                    .stream_id
                    .filter(|value| !value.is_empty())
                    .unwrap_or_else(|| fallback_key.clone());
                if !is_safe_srt_stream_id(&stream_key) {
                    warn!("Ignoring SRT packet with invalid stream id");
                    continue;
                }

                if !store.accepts_stream_key(&stream_key).await {
                    warn!("Ignoring SRT packet with an unauthorized stream id");
                    continue;
                }

                if !forwards.contains_key(&stream_key) {
                    if forwards.len() >= MAX_BRIDGES {
                        warn!("SRT bridge limit reached; rejecting a new stream");
                        continue;
                    }
                    let target = format!("rtmp://127.0.0.1:{rtmp_port}/live/{stream_key}");
                    let ffmpeg = std::env::var("RESTREAM_FFMPEG_PATH")
                        .unwrap_or_else(|_| "ffmpeg".into());
                    let mut child = match Command::new(ffmpeg)
                        .args([
                            "-hide_banner",
                            "-loglevel",
                            "error",
                            "-f",
                            "mpegts",
                            "-i",
                            "pipe:0",
                            "-c",
                            "copy",
                            "-f",
                            "flv",
                            &target,
                        ])
                        .stdin(std::process::Stdio::piped())
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::null())
                        .spawn()
                    {
                        Ok(child) => child,
                        Err(error) => {
                            error!("Failed to start SRT-to-RTMP bridge: {}", error);
                            continue;
                        }
                    };
                    let Some(stdin) = child.stdin.take() else {
                        error!("SRT-to-RTMP bridge has no writable input");
                        let _ = child.kill().await;
                        continue;
                    };
                    info!("SRT-to-RTMP bridge started");
                    forwards.insert(
                        stream_key.clone(),
                        SrtForwardProcess {
                            stdin,
                            child,
                            last_packet: Instant::now(),
                        },
                    );
                }

                let write_result = if let Some(forward) = forwards.get_mut(&stream_key) {
                    forward.last_packet = Instant::now();
                    forward.stdin.write_all(&packet.data).await
                } else {
                    continue;
                };
                if let Err(error) = write_result {
                    warn!("SRT-to-RTMP bridge stopped: {}", error);
                    if let Some(mut forward) = forwards.remove(&stream_key) {
                        let _ = forward.child.kill().await;
                    }
                }
            }
            _ = cleanup.tick() => {
                let expired = forwards
                    .iter()
                    .filter(|(_, forward)| forward.last_packet.elapsed() > Duration::from_secs(15))
                    .map(|(stream_key, _)| stream_key.clone())
                    .collect::<Vec<_>>();
                for stream_key in expired {
                    if let Some(mut forward) = forwards.remove(&stream_key) {
                        let _ = forward.stdin.shutdown().await;
                        let _ = forward.child.kill().await;
                        info!("SRT-to-RTMP bridge timed out");
                    }
                }
            }
        }
    }

    for (_, mut forward) in forwards {
        let _ = forward.stdin.shutdown().await;
        let _ = forward.child.kill().await;
    }
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
