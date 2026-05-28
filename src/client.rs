// src/client.rs
mod push;

use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
pub use push::PushClient;
use rml_rtmp::handshake::{Handshake, HandshakeProcessResult, PeerType};
use rml_rtmp::sessions::{ClientSessionResult, ServerSessionEvent, ServerSessionResult};
use rml_rtmp::time::RtmpTimestamp;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::{RwLock, mpsc};
use tokio::time::timeout;
use tracing::{error, info, warn};

use crate::DynStream;
use crate::config::Platform;
use crate::server::handshake_and_create_server_session;

fn is_video_sequence_header(data: &Bytes) -> bool {
    data.len() > 1 && data[0] == 0x17 && data[1] == 0x00
}

fn is_audio_sequence_header(data: &Bytes) -> bool {
    data.len() > 1 && (data[0] & 0xF0) == 0xA0 && data[1] == 0x00
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
        // 0xA0 = audio flag with codec id 0
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
        // 0x17 is video, not audio
        let data = Bytes::from(vec![0x17, 0x00]);
        assert!(!is_audio_sequence_header(&data));
    }

    #[test]
    fn test_is_audio_sequence_header_wrong_flag() {
        // second byte != 0x00
        let data = Bytes::from(vec![0xAF, 0x01]);
        assert!(!is_audio_sequence_header(&data));
    }
}

pub(crate) async fn perform_client_handshake(
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
    mut inbound: TcpStream,
    platforms: Arc<RwLock<Vec<Platform>>>,
    stream_key_conf: String,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (mut server_session, leftover) = handshake_and_create_server_session(&mut inbound).await?;
    let (reconnect_tx, mut reconnect_rx) = mpsc::channel::<(usize, PushClient)>(10);

    let pls = platforms.read().await.clone();
    let mut push_clients: Vec<PushClient> = Vec::new();

    if !leftover.is_empty() {
        let results = server_session.handle_input(&leftover)?;
        for res in results {
            if let ServerSessionResult::OutboundResponse(packet) = res {
                inbound.write_all(&packet.bytes).await?;
            }
        }
    }

    let mut read_buf = [0u8; 8192];

    loop {
        tokio::select! {
            Some((index, new_client)) = reconnect_rx.recv() => {
                if index < push_clients.len() {
                    info!("Replacing old client with reconnected client at index {}", index);
                    push_clients[index] = new_client;
                }
            }

            n_res = inbound.read(&mut read_buf) => {
                let n = match n_res {
                    Ok(0) => {
                        info!("Source stream ended (EOF). Shutting down push clients gracefully...");
                        for (i, pc) in push_clients.iter().enumerate() {
                            info!("Stopping client {}", i);
                            pc.shutdown().await;
                        }
                        push_clients.clear();
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
                                if stream_key == stream_key_conf {
                                    if let Ok(out) = server_session.accept_request(request_id) {
                                        for r in out {
                                            if let ServerSessionResult::OutboundResponse(p) = r {
                                                let _ = inbound.write_all(&p.bytes).await;
                                            }
                                        }
                                    }

                                    if push_clients.is_empty() {
                                        for p in &pls {
                                            match timeout(Duration::from_secs(5), PushClient::connect_and_publish(&p.url, p.key.clone(), None, None, None)).await {
                                                Ok(Ok(pc)) => {
                                                    info!("Connected to platform: {}", p.url);
                                                    push_clients.push(pc);
                                                },
                                                _ => error!("Failed to connect to platform: {}", p.url),
                                            }
                                        }
                                    }
                                } else {
                                    let _ = server_session.reject_request(request_id, "NetStream.Publish.BadName", "Invalid key");
                                    return Ok(());
                                }
                            }
                            ServerSessionEvent::VideoDataReceived { data, timestamp, .. } => {
                                forward_to_push_clients(&mut push_clients, &reconnect_tx, data, timestamp, true).await;
                            }
                            ServerSessionEvent::AudioDataReceived { data, timestamp, .. } => {
                                forward_to_push_clients(&mut push_clients, &reconnect_tx, data, timestamp, false).await;
                            }
                            ServerSessionEvent::StreamMetadataChanged { metadata, .. } => {
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

async fn forward_to_push_clients(
    push_clients: &mut [PushClient],
    reconnect_tx: &mpsc::Sender<(usize, PushClient)>,
    data: Bytes,
    timestamp: RtmpTimestamp,
    is_video: bool,
) {
    for (i, pc) in push_clients.iter_mut().enumerate() {
        let mut state = pc.client_state.write().await;

        if is_video && is_video_sequence_header(&data) {
            state.update_video_header(data.clone());
        } else if !is_video && is_audio_sequence_header(&data) {
            state.update_audio_header(data.clone());
        }

        if pc.tx_feed.is_closed() {
            let p_url = pc.url.clone();
            let p_key = pc.stream_key.clone();
            let tx_back = reconnect_tx.clone();

            let cached_vid = state.video_sequence_header.clone();
            let cached_aud = state.audio_sequence_header.clone();
            let cached_meta = state.prepublish_metadata.clone();

            drop(state);

            let (dummy_tx, mut dummy_rx) = mpsc::channel(1);
            pc.tx_feed = dummy_tx;

            tokio::spawn(async move {
                let _drainer =
                    tokio::spawn(async move { while dummy_rx.recv().await.is_some() {} });

                info!(
                    "Connection lost for platform {}. Starting reconnection loop...",
                    i
                );

                loop {
                    tokio::time::sleep(Duration::from_secs(2)).await;

                    match PushClient::connect_and_publish(
                        &p_url,
                        p_key.clone(),
                        cached_vid.clone(),
                        cached_aud.clone(),
                        cached_meta.clone(),
                    )
                    .await
                    {
                        Ok(new_pc) => {
                            info!("Reconnection successful for platform index {}", i);
                            if tx_back.send((i, new_pc)).await.is_err() {
                                warn!("Main loop closed, abandoning reconnection for {}", i);
                            }
                            break;
                        }
                        Err(e) => {
                            warn!("Reconnection failed for platform {}: {}. Retrying...", i, e);
                        }
                    }
                }
            });
            continue;
        }

        if *pc.publish_ready_rx.borrow() {
            let res = if is_video {
                state
                    .session
                    .publish_video_data(data.clone(), timestamp, true)
            } else {
                state
                    .session
                    .publish_audio_data(data.clone(), timestamp, true)
            };

            if let Ok(ClientSessionResult::OutboundResponse(packet)) = res {
                let _ = pc.tx_feed.try_send(Bytes::from(packet.bytes));
            }
        } else if is_video {
            state.buffer_video(data.clone(), timestamp);
        }
    }
}
