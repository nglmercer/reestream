//! Destination reconnect and fan-out lifecycle helpers.

use super::*;

pub(super) async fn prime_push_client(
    client: &mut PushClient,
    video_header: Option<Bytes>,
    audio_header: Option<Bytes>,
    metadata: Option<rml_rtmp::sessions::StreamMetadata>,
) {
    let mut state = client.client_state.write().await;
    if video_header.is_some() {
        state.video_sequence_header = video_header;
    }
    if audio_header.is_some() {
        state.audio_sequence_header = audio_header;
    }
    if metadata.is_some() {
        state.prepublish_metadata = metadata;
    }
    if *client.publish_ready_rx.borrow() {
        PushClient::drain_buffers(&mut state, &client.tx_feed);
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn spawn_destination_reconnect(
    reconnect_tx: &mpsc::Sender<(String, PushClient)>,
    reconnect_stop: &watch::Receiver<bool>,
    platforms: Arc<RwLock<Vec<Platform>>>,
    status_reporter: Option<Arc<dyn DestinationStatusReporter>>,
    reconnecting_platforms: Arc<Mutex<HashSet<String>>>,
    url: Url,
    stream_key: String,
    cached_video_header: Option<Bytes>,
    cached_audio_header: Option<Bytes>,
    cached_metadata: Option<rml_rtmp::sessions::StreamMetadata>,
    platform_id: String,
) {
    {
        let mut active = reconnecting_platforms.lock().await;
        if !active.insert(platform_id.clone()) {
            return;
        }
    }

    let mut reconnect_stop = reconnect_stop.clone();
    let reconnect_tx = reconnect_tx.clone();
    tokio::spawn(async move {
        info!(
            "Starting destination reconnection loop for platform {}",
            platform_id
        );

        if *reconnect_stop.borrow() {
            reconnecting_platforms.lock().await.remove(&platform_id);
            return;
        }

        loop {
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(2)) => {}
                changed = reconnect_stop.changed() => {
                    if changed.is_err() || *reconnect_stop.borrow() {
                        break;
                    }
                }
            }
            if *reconnect_stop.borrow() {
                break;
            }

            let platform_still_enabled = platforms.read().await.iter().any(|platform| {
                platform_id_from(platform.url.as_str(), &platform.key) == platform_id
                    && platform.enabled
            });
            if !platform_still_enabled {
                report_destination_status(&status_reporter, &platform_id, "disconnected", None)
                    .await;
                info!(
                    "Platform {} is no longer enabled, abandoning reconnection",
                    platform_id
                );
                break;
            }

            match timeout(
                Duration::from_secs(5),
                PushClient::connect_and_publish(
                    &url,
                    stream_key.clone(),
                    cached_video_header.clone(),
                    cached_audio_header.clone(),
                    cached_metadata.clone(),
                    platform_id.clone(),
                ),
            )
            .await
            {
                Ok(Ok(new_client)) => {
                    if *reconnect_stop.borrow() {
                        new_client.shutdown().await;
                        break;
                    }
                    report_destination_status(&status_reporter, &platform_id, "connected", None)
                        .await;
                    if reconnect_tx
                        .send((platform_id.clone(), new_client))
                        .await
                        .is_err()
                    {
                        warn!(
                            "Main publisher loop closed, abandoning reconnection for {}",
                            platform_id
                        );
                    }
                    break;
                }
                Ok(Err(error)) => {
                    report_destination_status(
                        &status_reporter,
                        &platform_id,
                        "error",
                        Some("destination connection failed".into()),
                    )
                    .await;
                    warn!(
                        "Reconnection failed for platform {}: {}. Retrying...",
                        platform_id, error
                    );
                }
                Err(_) => {
                    report_destination_status(
                        &status_reporter,
                        &platform_id,
                        "error",
                        Some("destination connection timed out".into()),
                    )
                    .await;
                    warn!(
                        "Reconnection timed out for platform {}. Retrying...",
                        platform_id
                    );
                }
            }
        }

        reconnecting_platforms.lock().await.remove(&platform_id);
    });
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn forward_to_push_clients(
    push_clients: &mut [PushClient],
    reconnect_tx: &mpsc::Sender<(String, PushClient)>,
    data: Bytes,
    timestamp: RtmpTimestamp,
    is_video: bool,
    platforms: Arc<RwLock<Vec<Platform>>>,
    status_reporter: Option<Arc<dyn DestinationStatusReporter>>,
    reconnect_stop: watch::Receiver<bool>,
    reconnecting_platforms: Arc<Mutex<HashSet<String>>>,
) {
    for pc in push_clients.iter_mut() {
        let mut state = pc.client_state.write().await;

        if is_video && is_video_sequence_header(&data) {
            state.update_video_header(data.clone());
        } else if !is_video && is_audio_sequence_header(&data) {
            state.update_audio_header(data.clone());
        }

        if pc.tx_feed.is_closed() {
            let p_url = pc.url.clone();
            let p_key = pc.stream_key.clone();
            let p_platform_id = pc.platform_id.clone();
            let cached_vid = state.video_sequence_header.clone();
            let cached_aud = state.audio_sequence_header.clone();
            let cached_meta = state.prepublish_metadata.clone();

            drop(state);

            let (dummy_tx, mut dummy_rx) = mpsc::channel(1);
            pc.tx_feed = dummy_tx;

            tokio::spawn(async move { while dummy_rx.recv().await.is_some() {} });

            spawn_destination_reconnect(
                reconnect_tx,
                &reconnect_stop,
                platforms.clone(),
                status_reporter.clone(),
                reconnecting_platforms.clone(),
                p_url,
                p_key,
                cached_vid,
                cached_aud,
                cached_meta,
                p_platform_id,
            )
            .await;
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
        } else {
            state.buffer_audio(data.clone(), timestamp);
        }
    }
}
