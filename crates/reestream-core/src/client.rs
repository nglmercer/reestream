// src/client.rs
pub mod push;

mod reconnect;
use reconnect::{forward_to_push_clients, prime_push_client, spawn_destination_reconnect};

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
pub use push::PushClient;
use rml_rtmp::handshake::{Handshake, HandshakeProcessResult, PeerType};
use rml_rtmp::sessions::{ClientSessionResult, ServerSessionEvent, ServerSessionResult};
use rml_rtmp::time::RtmpTimestamp;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::{Mutex, RwLock, broadcast, mpsc, watch};
use tokio::time::timeout;
use tracing::{error, info, warn};
use url::Url;

use crate::DynStream;
use crate::config::{Platform, PlatformEvent, platform_id_from};
use crate::server::handshake_and_create_server_session;

/// Trait for registering active streams (implemented by StreamManager)
#[async_trait::async_trait]
pub trait StreamRegistrar: Send + Sync {
    async fn register_stream(&self, name: String, input_url: String) -> String;
    async fn unregister_stream(&self, id: &str);
}

/// Trait for publishing stream data (implemented by DataBus)
pub trait DataPublisher: Send + Sync {
    fn publish(&self, stream_id: &str, data: Bytes, is_video: bool, timestamp_ms: u32);

    /// Claim a preview/recording data path for a stream. Implementations with
    /// one global preview can reject concurrent inputs.
    fn try_activate_stream(&self, _stream_id: &str) -> bool {
        true
    }

    fn deactivate_stream(&self, _stream_id: &str) {}
}

/// Receives destination connection state so the product API can expose an
/// honest status instead of leaving channels permanently disconnected.
#[async_trait::async_trait]
pub trait DestinationStatusReporter: Send + Sync {
    async fn set_destination_status(
        &self,
        destination_id: &str,
        status: &str,
        last_error: Option<String>,
    );
}

/// Resolves product-level event ingest keys to the destinations selected for
/// that event. The global config key continues to use the static platform
/// list, while scheduled/Studio events can provide their own key and fan-out.
#[async_trait::async_trait]
pub trait PublishKeyResolver: Send + Sync {
    async fn platforms_for_key(&self, stream_key: &str) -> Option<Vec<Platform>>;

    async fn input_url_for_key(&self, _stream_key: &str) -> Option<String> {
        None
    }
}

fn is_video_sequence_header(data: &Bytes) -> bool {
    data.len() > 1 && data[0] == 0x17 && data[1] == 0x00
}

fn is_audio_sequence_header(data: &Bytes) -> bool {
    data.len() > 1 && (data[0] & 0xF0) == 0xA0 && data[1] == 0x00
}

async fn sync_platforms_for_event(platforms: &Arc<RwLock<Vec<Platform>>>, event: &PlatformEvent) {
    let mut configured = platforms.write().await;
    match event {
        PlatformEvent::Added {
            platform_id,
            url,
            key,
        } => {
            if !configured.iter().any(|platform| {
                platform_id_from(platform.url.as_str(), &platform.key) == *platform_id
            }) && let Ok(url) = Url::parse(url)
            {
                configured.push(Platform {
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
                if let Ok(url) = Url::parse(url) {
                    platform.url = url;
                }
            } else if *enabled && let Ok(url) = Url::parse(url) {
                configured.push(Platform {
                    url,
                    key: key.clone(),
                    enabled: true,
                    orientation: Default::default(),
                });
            }
        }
    }
}

async fn report_destination_status(
    reporter: &Option<Arc<dyn DestinationStatusReporter>>,
    destination_id: &str,
    status: &str,
    last_error: Option<String>,
) {
    if let Some(reporter) = reporter {
        reporter
            .set_destination_status(destination_id, status, last_error)
            .await;
    }
}

pub async fn perform_client_handshake(
    stream: &mut DynStream,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut hs = Handshake::new(PeerType::Client);
    let c0_c1 = hs.generate_outbound_p0_and_p1()?;
    stream.write_all(&c0_c1).await?;
    let mut buf = [0u8; 4096];
    loop {
        let n = stream.read(&mut buf).await?;
        if n == 0 {
            return Err("EOF during client handshake".into());
        }

        match hs.process_bytes(&buf[..n])? {
            HandshakeProcessResult::InProgress { response_bytes } => {
                if !response_bytes.is_empty() {
                    stream.write_all(&response_bytes).await?;
                }
            }
            HandshakeProcessResult::Completed { response_bytes, .. } => {
                if !response_bytes.is_empty() {
                    stream.write_all(&response_bytes).await?;
                }
                break;
            }
        }
    }
    Ok(())
}

pub async fn handle_publisher(
    inbound: TcpStream,
    platforms: Arc<RwLock<Vec<Platform>>>,
    stream_key_conf: String,
    stream_manager: Option<Arc<dyn StreamRegistrar>>,
    data_publisher: Option<Arc<dyn DataPublisher>>,
    platform_events: tokio::sync::broadcast::Receiver<PlatformEvent>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    handle_publisher_with_resolver(
        inbound,
        platforms,
        stream_key_conf,
        stream_manager,
        data_publisher,
        platform_events,
        None,
    )
    .await
}

pub async fn handle_publisher_with_resolver(
    inbound: TcpStream,
    platforms: Arc<RwLock<Vec<Platform>>>,
    stream_key_conf: String,
    stream_manager: Option<Arc<dyn StreamRegistrar>>,
    data_publisher: Option<Arc<dyn DataPublisher>>,
    platform_events: tokio::sync::broadcast::Receiver<PlatformEvent>,
    key_resolver: Option<Arc<dyn PublishKeyResolver>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    handle_publisher_with_resolver_and_status(
        inbound,
        platforms,
        stream_key_conf,
        stream_manager,
        data_publisher,
        platform_events,
        key_resolver,
        None,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn handle_publisher_with_resolver_and_status(
    mut inbound: TcpStream,
    platforms: Arc<RwLock<Vec<Platform>>>,
    stream_key_conf: String,
    stream_manager: Option<Arc<dyn StreamRegistrar>>,
    data_publisher: Option<Arc<dyn DataPublisher>>,
    mut platform_events: tokio::sync::broadcast::Receiver<PlatformEvent>,
    key_resolver: Option<Arc<dyn PublishKeyResolver>>,
    status_reporter: Option<Arc<dyn DestinationStatusReporter>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (mut server_session, leftover) = handshake_and_create_server_session(&mut inbound).await?;
    let (reconnect_tx, mut reconnect_rx) = mpsc::channel::<(String, PushClient)>(10);
    let (reconnect_stop_tx, reconnect_stop_rx) = watch::channel(false);
    let reconnecting_platforms = Arc::new(Mutex::new(HashSet::new()));

    let pls: Vec<Platform> = platforms
        .read()
        .await
        .iter()
        .filter(|p| p.enabled)
        .cloned()
        .collect();
    let mut push_clients: Vec<PushClient> = Vec::new();

    // Cached sequence headers & metadata for passing to new/reconnected PushClients
    let mut cached_video_header: Option<Bytes> = None;
    let mut cached_audio_header: Option<Bytes> = None;
    let mut cached_metadata: Option<rml_rtmp::sessions::StreamMetadata> = None;

    if !leftover.is_empty() {
        let results = server_session.handle_input(&leftover)?;
        for res in results {
            if let ServerSessionResult::OutboundResponse(packet) = res {
                inbound.write_all(&packet.bytes).await?;
            }
        }
    }

    let mut read_buf = [0u8; 8192];
    let mut registered_stream_id: Option<String> = None;
    let mut platform_events_closed = false;

    let result: Result<(), Box<dyn std::error::Error + Send + Sync>> = async {
        loop {
        tokio::select! {
            Some((platform_id, new_client)) = reconnect_rx.recv() => {
                let mut new_client = new_client;
                if let Some(index) = push_clients
                    .iter()
                    .position(|pc| pc.platform_id == platform_id)
                {
                    info!("Replacing old client with reconnected client for platform {}", platform_id);
                    prime_push_client(
                        &mut new_client,
                        cached_video_header.clone(),
                        cached_audio_header.clone(),
                        cached_metadata.clone(),
                    )
                    .await;
                    push_clients[index] = new_client;
                } else if platforms.read().await.iter().any(|platform| {
                    platform_id_from(platform.url.as_str(), &platform.key) == platform_id
                        && platform.enabled
                }) {
                    info!("Adding reconnected client for platform {}", platform_id);
                    prime_push_client(
                        &mut new_client,
                        cached_video_header.clone(),
                        cached_audio_header.clone(),
                        cached_metadata.clone(),
                    )
                    .await;
                    push_clients.push(new_client);
                } else {
                    // The destination was removed or disabled while the
                    // retry was in flight. Do not revive it after removal.
                    new_client.shutdown().await;
                }
            }

            evt = platform_events.recv(), if !platform_events_closed => {
                match evt {
                    Ok(PlatformEvent::Toggled { platform_id, url, key, enabled }) => {
                        sync_platforms_for_event(
                            &platforms,
                            &PlatformEvent::Toggled {
                                platform_id: platform_id.clone(),
                                url: url.clone(),
                                key: key.clone(),
                                enabled,
                            },
                        )
                        .await;
                        if !enabled {
                            // Shutdown and remove the PushClient for this platform
                            if let Some(pos) = push_clients.iter().position(|pc| pc.platform_id == platform_id) {
                                let pc = push_clients.remove(pos);
                                info!("Platform {} disabled via toggle, shutting down PushClient", platform_id);
                                pc.shutdown().await;
                                report_destination_status(
                                    &status_reporter,
                                    &platform_id,
                                    "disconnected",
                                    None,
                                )
                                .await;
                            }
                        } else {
                            // Platform enabled: create a new PushClient if we don't already have one
                            if push_clients.iter().any(|pc| pc.platform_id == platform_id) {
                                info!("Platform {} already has an active PushClient", platform_id);
                            } else {
                                info!("Platform {} enabled, creating new PushClient", platform_id);
                                let url_parsed = match Url::parse(&url) {
                                    Ok(u) => u,
                                    Err(e) => {
                                        error!("Invalid platform URL for platform {}: {}", platform_id, e);
                                        continue;
                                    }
                                };
                                match timeout(Duration::from_secs(5), PushClient::connect_and_publish(&url_parsed, key.clone(), cached_video_header.clone(), cached_audio_header.clone(), cached_metadata.clone(), platform_id.clone())).await {
                                    Ok(Ok(pc)) => {
                                        report_destination_status(
                                            &status_reporter,
                                            &platform_id,
                                            "connected",
                                            None,
                                        )
                                        .await;
                                        info!("Connected to newly enabled platform {}", platform_id);
                                        push_clients.push(pc);
                                    },
                                    _ => {
                                        report_destination_status(
                                            &status_reporter,
                                            &platform_id,
                                            "error",
                                            Some("destination connection failed".into()),
                                        )
                                        .await;
                                        error!("Failed to connect to newly enabled platform {}", platform_id);
                                        spawn_destination_reconnect(
                                            &reconnect_tx,
                                            &reconnect_stop_rx,
                                            platforms.clone(),
                                            status_reporter.clone(),
                                            reconnecting_platforms.clone(),
                                            url_parsed,
                                            key,
                                            cached_video_header.clone(),
                                            cached_audio_header.clone(),
                                            cached_metadata.clone(),
                                            platform_id.clone(),
                                        )
                                        .await;
                                    },
                                }
                            }
                        }
                    }
                    Ok(PlatformEvent::Added { platform_id, url, key }) => {
                        sync_platforms_for_event(
                            &platforms,
                            &PlatformEvent::Added {
                                platform_id: platform_id.clone(),
                                url: url.clone(),
                                key: key.clone(),
                            },
                        )
                        .await;
                        // A new platform was added while streaming — connect to it
                        if push_clients.iter().any(|pc| pc.platform_id == platform_id) {
                            info!("Platform {} already has an active PushClient", platform_id);
                        } else {
                            info!("New platform added, creating PushClient: {}", platform_id);
                            let url_parsed = match Url::parse(&url) {
                                Ok(u) => u,
                                Err(e) => {
                                    error!("Invalid platform URL for platform {}: {}", platform_id, e);
                                    continue;
                                }
                            };
                            match timeout(Duration::from_secs(5), PushClient::connect_and_publish(&url_parsed, key.clone(), cached_video_header.clone(), cached_audio_header.clone(), cached_metadata.clone(), platform_id.clone())).await {
                                Ok(Ok(pc)) => {
                                    report_destination_status(
                                        &status_reporter,
                                        &platform_id,
                                        "connected",
                                        None,
                                    )
                                    .await;
                                    info!("Connected to new platform {}", platform_id);
                                    push_clients.push(pc);
                                },
                                _ => {
                                    report_destination_status(
                                        &status_reporter,
                                        &platform_id,
                                        "error",
                                        Some("destination connection failed".into()),
                                    )
                                    .await;
                                    error!("Failed to connect to new platform {}", platform_id);
                                    spawn_destination_reconnect(
                                        &reconnect_tx,
                                        &reconnect_stop_rx,
                                        platforms.clone(),
                                        status_reporter.clone(),
                                        reconnecting_platforms.clone(),
                                        url_parsed,
                                        key,
                                        cached_video_header.clone(),
                                        cached_audio_header.clone(),
                                        cached_metadata.clone(),
                                        platform_id.clone(),
                                    )
                                    .await;
                                },
                            }
                        }
                    }
                    Ok(PlatformEvent::Removed { platform_id }) => {
                        sync_platforms_for_event(
                            &platforms,
                            &PlatformEvent::Removed {
                                platform_id: platform_id.clone(),
                            },
                        )
                        .await;
                        // Platform removed — shutdown and remove the PushClient
                        if let Some(pos) = push_clients.iter().position(|pc| pc.platform_id == platform_id) {
                            let pc = push_clients.remove(pos);
                            info!("Platform {} removed, shutting down PushClient", platform_id);
                            pc.shutdown().await;
                            report_destination_status(
                                &status_reporter,
                                &platform_id,
                                "disconnected",
                                None,
                            )
                            .await;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => platform_events_closed = true,
                }
            }

            n_res = inbound.read(&mut read_buf) => {
                let n = match n_res {
                    Ok(0) => {
                        info!("Source stream ended (EOF). Shutting down push clients gracefully...");
                        break;
                    },
                    Ok(n) => n,
                    Err(e) => return Err(e.into()),
                };

                let results = server_session.handle_input(&read_buf[..n])?;

                for res in results {
                    match res {
                        ServerSessionResult::OutboundResponse(packet) => {
                            let _ = inbound.write_all(&packet.bytes).await;
                        }
                        ServerSessionResult::RaisedEvent(ev) => match ev {
                            ServerSessionEvent::ConnectionRequested { request_id, .. } => {
                                if let Ok(out) = server_session.accept_request(request_id) {
                                    for r in out {
                                        if let ServerSessionResult::OutboundResponse(p) = r {
                                            let _ = inbound.write_all(&p.bytes).await;
                                        }
                                    }
                                }
                            }
                            ServerSessionEvent::PublishStreamRequested { request_id, stream_key, .. } => {
                                let selected_platforms = if !stream_key_conf.is_empty()
                                    && stream_key == stream_key_conf
                                {
                                    Some(pls.clone())
                                } else if let Some(ref resolver) = key_resolver {
                                    resolver.platforms_for_key(&stream_key).await
                                } else {
                                    None
                                };

                                if let Some(selected_platforms) = selected_platforms {
                                    // Register the stream before accepting it so a
                                    // single global preview cannot interleave two
                                    // inputs and corrupt HLS/FLV state.
                                    let mut next_stream_id = None;
                                    if let Some(ref registrar) = stream_manager {
                                        let stream_name = "RTMP Stream".to_string();
                                        let input_url = if let Some(ref resolver) = key_resolver {
                                            resolver
                                                .input_url_for_key(&stream_key)
                                                .await
                                                .unwrap_or_else(|| {
                                                    format!("rtmp://localhost/live/{stream_key}")
                                                })
                                        } else {
                                            format!("rtmp://localhost/live/{stream_key}")
                                        };
                                        let id = registrar.register_stream(stream_name, input_url).await;
                                        if let Some(ref pubber) = data_publisher
                                            && !pubber.try_activate_stream(&id)
                                        {
                                            registrar.unregister_stream(&id).await;
                                            let _ = server_session.reject_request(
                                                request_id,
                                                "NetStream.Publish.BadName",
                                                "Another preview stream is already active",
                                            );
                                            continue;
                                        }
                                        next_stream_id = Some(id);
                                    }

                                    if let Ok(out) = server_session.accept_request(request_id) {
                                        for r in out {
                                            if let ServerSessionResult::OutboundResponse(p) = r {
                                                let _ = inbound.write_all(&p.bytes).await;
                                            }
                                        }
                                    }

                                    if let Some(id) = next_stream_id {
                                        registered_stream_id = Some(id.clone());
                                        info!("Registered stream: {}", id);
                                    }

                                    if push_clients.is_empty() {
                                        for p in &selected_platforms {
                                            let pid = platform_id_from(p.url.as_str(), &p.key);
                                            match timeout(Duration::from_secs(5), PushClient::connect_and_publish(&p.url, p.key.clone(), None, None, None, pid.clone())).await {
                                                Ok(Ok(pc)) => {
                                                    report_destination_status(
                                                        &status_reporter,
                                                        &pid,
                                                        "connected",
                                                        None,
                                                    )
                                                    .await;
                                                    info!("Connected to platform {}", pid);
                                                    push_clients.push(pc);
                                                },
                                                _ => {
                                                    report_destination_status(
                                                        &status_reporter,
                                                        &pid,
                                                        "error",
                                                        Some("destination connection failed".into()),
                                                    )
                                                    .await;
                                                    error!("Failed to connect to platform {}", pid);
                                                    spawn_destination_reconnect(
                                                        &reconnect_tx,
                                                        &reconnect_stop_rx,
                                                        platforms.clone(),
                                                        status_reporter.clone(),
                                                        reconnecting_platforms.clone(),
                                                        p.url.clone(),
                                                        p.key.clone(),
                                                        None,
                                                        None,
                                                        None,
                                                        pid.clone(),
                                                    )
                                                    .await;
                                                },
                                            }
                                        }
                                    }
                                } else {
                                    let _ = server_session.reject_request(request_id, "NetStream.Publish.BadName", "Invalid key");
                                    return Ok(());
                                }
                            }
                            ServerSessionEvent::VideoDataReceived { data, timestamp, .. } => {
                                if is_video_sequence_header(&data) {
                                    cached_video_header = Some(data.clone());
                                }
                                // Publish to DataBus for HLS/FLV
                                if let (Some(pubber), Some(sid)) = (&data_publisher, &registered_stream_id) {
                                    pubber.publish(sid, data.clone(), true, timestamp.value);
                                }
                                forward_to_push_clients(
                                    &mut push_clients,
                                    &reconnect_tx,
                                    data,
                                    timestamp,
                                    true,
                                    platforms.clone(),
                                    status_reporter.clone(),
                                    reconnect_stop_rx.clone(),
                                    reconnecting_platforms.clone(),
                                )
                                .await;
                            }
                            ServerSessionEvent::AudioDataReceived { data, timestamp, .. } => {
                                if is_audio_sequence_header(&data) {
                                    cached_audio_header = Some(data.clone());
                                }
                                // Publish to DataBus for HLS/FLV
                                if let (Some(pubber), Some(sid)) = (&data_publisher, &registered_stream_id) {
                                    pubber.publish(sid, data.clone(), false, timestamp.value);
                                }
                                forward_to_push_clients(
                                    &mut push_clients,
                                    &reconnect_tx,
                                    data,
                                    timestamp,
                                    false,
                                    platforms.clone(),
                                    status_reporter.clone(),
                                    reconnect_stop_rx.clone(),
                                    reconnecting_platforms.clone(),
                                )
                                .await;
                            }
                            ServerSessionEvent::StreamMetadataChanged { metadata, .. } => {
                                cached_metadata = Some(metadata.clone());
                                for pc in &push_clients {
                                    let mut state = pc.client_state.write().await;
                                    state.prepublish_metadata = Some(metadata.clone());

                                    if *pc.publish_ready_rx.borrow()
                                        && let Ok(ClientSessionResult::OutboundResponse(packet)) = state.session.publish_metadata(&metadata)
                                    {
                                        let _ = pc.tx_feed.try_send(Bytes::from(packet.bytes));
                                    }
                                }
                            }
                            _ => {}
                        },
                        _ => {}
                    }
                }
            }
        }
        }
        Ok(())
    }
    .await;

    let _ = reconnect_stop_tx.send(true);
    if let Some(id) = registered_stream_id.take() {
        if let Some(pubber) = &data_publisher {
            pubber.deactivate_stream(&id);
        }
        if let Some(registrar) = &stream_manager {
            registrar.unregister_stream(&id).await;
        }
        info!("Unregistered stream: {}", id);
    }
    for (i, pc) in push_clients.iter().enumerate() {
        report_destination_status(&status_reporter, &pc.platform_id, "disconnected", None).await;
        info!("Stopping client {}", i);
        pc.shutdown().await;
    }
    push_clients.clear();
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_video_sequence_header_valid() {
        let data = Bytes::from(vec![0x17, 0x00, 0x00, 0x00]);
        assert!(is_video_sequence_header(&data));
    }

    #[test]
    fn test_is_video_sequence_header_empty() {
        let data = Bytes::new();
        assert!(!is_video_sequence_header(&data));
    }

    #[test]
    fn test_is_video_sequence_header_single_byte() {
        let data = Bytes::from(vec![0x17]);
        assert!(!is_video_sequence_header(&data));
    }

    #[test]
    fn test_is_video_sequence_header_wrong_type() {
        let data = Bytes::from(vec![0x27, 0x00]);
        assert!(!is_video_sequence_header(&data));
    }

    #[test]
    fn test_is_video_sequence_header_wrong_flag() {
        let data = Bytes::from(vec![0x17, 0x01]);
        assert!(!is_video_sequence_header(&data));
    }

    #[test]
    fn test_is_audio_sequence_header_valid_aac() {
        let data = Bytes::from(vec![0xAF, 0x00, 0x01]);
        assert!(is_audio_sequence_header(&data));
    }

    #[test]
    fn test_is_audio_sequence_header_valid_other_codec() {
        let data = Bytes::from(vec![0xA0, 0x00]);
        assert!(is_audio_sequence_header(&data));
    }

    #[test]
    fn test_is_audio_sequence_header_empty() {
        let data = Bytes::new();
        assert!(!is_audio_sequence_header(&data));
    }

    #[test]
    fn test_is_audio_sequence_header_single_byte() {
        let data = Bytes::from(vec![0xAF]);
        assert!(!is_audio_sequence_header(&data));
    }

    #[test]
    fn test_is_audio_sequence_header_not_audio() {
        let data = Bytes::from(vec![0x17, 0x00]);
        assert!(!is_audio_sequence_header(&data));
    }

    #[test]
    fn test_is_audio_sequence_header_wrong_flag() {
        let data = Bytes::from(vec![0xAF, 0x01]);
        assert!(!is_audio_sequence_header(&data));
    }

    #[test]
    fn test_video_header_all_video_types() {
        let data = Bytes::from(vec![0x17, 0x00]);
        assert!(is_video_sequence_header(&data));

        for type_byte in 0x10..=0x16 {
            let data = Bytes::from(vec![type_byte, 0x00]);
            assert!(
                !is_video_sequence_header(&data),
                "Expected false for 0x{:02X} 0x00",
                type_byte
            );
        }
        for type_byte in 0x18..=0x1F {
            let data = Bytes::from(vec![type_byte, 0x00]);
            assert!(
                !is_video_sequence_header(&data),
                "Expected false for 0x{:02X} 0x00",
                type_byte
            );
        }
    }

    #[test]
    fn test_video_header_non_video_types() {
        for type_byte in 0x00..=0x0F {
            let data = Bytes::from(vec![type_byte, 0x00]);
            assert!(
                !is_video_sequence_header(&data),
                "Expected false for 0x{:02X} 0x00",
                type_byte
            );
        }
    }

    #[test]
    fn test_audio_header_all_audio_types() {
        for type_byte in 0xA0..=0xAF {
            let data = Bytes::from(vec![type_byte, 0x00]);
            assert!(
                is_audio_sequence_header(&data),
                "Expected true for 0x{:02X} 0x00",
                type_byte
            );
        }
    }

    #[test]
    fn test_audio_header_non_audio_types() {
        let non_audio = [0x00, 0x17, 0x27, 0x50, 0x80, 0x9F];
        for type_byte in non_audio {
            let data = Bytes::from(vec![type_byte, 0x00]);
            assert!(
                !is_audio_sequence_header(&data),
                "Expected false for 0x{:02X} 0x00",
                type_byte
            );
        }
    }

    #[test]
    fn test_video_header_long_payload() {
        let mut data = vec![0x17, 0x00, 0x00, 0x00, 0x00];
        data.extend_from_slice(&[0x01, 0x64, 0x00, 0x1E, 0xFF, 0xE1]);
        let data = Bytes::from(data);
        assert!(is_video_sequence_header(&data));
    }

    #[test]
    fn test_audio_header_long_payload() {
        let mut data = vec![0xAF, 0x00];
        data.extend_from_slice(&[0x12, 0x10, 0x56, 0xE5, 0x00]);
        let data = Bytes::from(data);
        assert!(is_audio_sequence_header(&data));
    }

    #[test]
    fn test_both_headers_independent() {
        let video = Bytes::from(vec![0x17, 0x00]);
        let audio = Bytes::from(vec![0xAF, 0x00]);
        assert!(is_video_sequence_header(&video));
        assert!(!is_audio_sequence_header(&video));
        assert!(!is_video_sequence_header(&audio));
        assert!(is_audio_sequence_header(&audio));
    }
}
