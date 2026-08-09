#![cfg(any(feature = "hls", feature = "api"))]

use axum::{body::Body, http::Request};
use reestream_server::{
    databus::DataBus,
    flv::FlvState,
    hls::{HlsConfig, HlsSegmenter},
    http::{AppState, create_router},
    playback::PlaybackManager,
    recording::{RecordingConfig, RecordingManager},
    restream::RestreamStore,
    stream::StreamManager,
};
use serde_json::{Value, json};
use std::sync::Arc;
use tower::ServiceExt;

fn state() -> AppState {
    AppState {
        stream_manager: Arc::new(StreamManager::new()),
        hls_segmenter: Arc::new(HlsSegmenter::new(HlsConfig::default())),
        flv_state: FlvState::default(),
        data_bus: DataBus::default(),
        recording_manager: Arc::new(RecordingManager::new(RecordingConfig::default())),
        playback_manager: Arc::new(PlaybackManager::default()),
        start_time: std::time::Instant::now(),
        config_path: std::env::temp_dir().join("reestream-api-test.toml"),
        restream: Arc::new(RestreamStore::new()),
    }
}

async fn json_response(
    app: &mut axum::Router,
    method: &str,
    uri: &str,
    body: Option<Value>,
) -> (axum::http::StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(uri);
    if body.is_some() {
        builder = builder.header("content-type", "application/json");
    }
    let request = builder
        .body(Body::from(
            body.map(|value| value.to_string()).unwrap_or_default(),
        ))
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, serde_json::from_slice(&bytes).unwrap())
}

#[tokio::test]
async fn v1_event_channel_chat_and_analytics_flow() {
    let mut app = create_router(state());

    let (status, health) = json_response(&mut app, "GET", "/api/v1/health", None).await;
    assert_eq!(status, axum::http::StatusCode::OK);
    assert_eq!(health["data"]["status"], "ok");

    let (status, channel) = json_response(
        &mut app,
        "POST",
        "/api/v1/channels",
        Some(json!({
            "platformId": "custom-rtmp",
            "displayName": "Test destination",
            "streamUrl": "rtmp://example.test/live",
            "streamKey": "secret-key"
        })),
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::CREATED);
    let channel_id = channel["data"]["id"].as_str().unwrap().to_string();
    assert!(channel["data"]["streamKey"].is_null());

    let (status, event) = json_response(
        &mut app,
        "POST",
        "/api/v1/events",
        Some(json!({
            "streamType": "encoder",
            "title": "Test show",
            "destinationIds": [channel_id]
        })),
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::CREATED);
    let event_id = event["data"]["id"].as_str().unwrap().to_string();
    assert!(event["data"]["ingest"]["streamKey"].is_null());

    let (status, credentials) = json_response(
        &mut app,
        "GET",
        &format!("/api/v1/events/{event_id}/stream-key"),
        None,
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::OK);
    assert!(credentials["data"]["streamKey"].is_string());

    let (status, _) = json_response(
        &mut app,
        "POST",
        &format!("/api/v1/events/{event_id}/go-live"),
        None,
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::OK);

    let (status, _) = json_response(
        &mut app,
        "POST",
        &format!("/api/v1/events/{event_id}/viewers"),
        Some(json!({"viewers": 12, "bitrateKbps": 3500})),
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::OK);

    let (status, message) = json_response(
        &mut app,
        "POST",
        "/api/v1/chat/messages",
        Some(json!({"eventId": event_id, "message": "hello"})),
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::CREATED);
    assert_eq!(message["data"]["message"], "hello");

    let (status, _) = json_response(
        &mut app,
        "POST",
        &format!("/api/v1/events/{event_id}/end"),
        None,
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::OK);

    let (status, analytics) = json_response(
        &mut app,
        "GET",
        &format!("/api/v1/events/{event_id}/analytics"),
        None,
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::OK);
    assert_eq!(analytics["data"]["peakConcurrentViewers"], 12);
    assert_eq!(analytics["data"]["chatMessages"], 1);
}

#[tokio::test]
async fn state_file_round_trip_restores_channel_and_event_secrets() {
    let state_path =
        std::env::temp_dir().join(format!("reestream-api-state-{}.json", std::process::id()));
    let _ = std::fs::remove_file(&state_path);
    let store = RestreamStore::with_state_path(&state_path);
    let channel = store
        .create_channel(
            "custom-rtmp".into(),
            "Destination".into(),
            None,
            "rtmp://example.test/live".into(),
            "channel-secret".into(),
            None,
            None,
        )
        .await;
    let event = store
        .create_event(
            None,
            reestream_server::restream::StreamType::Encoder,
            "Event".into(),
            String::new(),
            None,
            vec![channel.id.clone()],
            None,
            0,
        )
        .await;
    // Secrets are kept in the local control-plane snapshot so the relay can
    // recover after restart; HTTP serializers omit them from normal models.
    let public_state = std::fs::read_to_string(&state_path).unwrap();
    assert!(!public_state.contains("channel-secret"));
    assert!(state_path.with_extension("secrets").exists());

    let restored = RestreamStore::with_state_path(&state_path);
    assert_eq!(
        restored.get_channel(&channel.id).await.unwrap().stream_key,
        "channel-secret"
    );
    assert_eq!(
        restored
            .event_credentials(&event.id)
            .await
            .unwrap()
            .stream_key,
        event.ingest.stream_key
    );
    let _ = std::fs::remove_file(state_path);
    let _ = std::fs::remove_file(
        std::env::temp_dir().join(format!("reestream-api-state-{}.key", std::process::id())),
    );
    let _ = std::fs::remove_file(std::env::temp_dir().join(format!(
        "reestream-api-state-{}.secrets",
        std::process::id()
    )));
}

#[tokio::test]
async fn v1_private_aliases_and_official_event_subresources_are_available() {
    let mut app = create_router(state());

    let (status, profile) = json_response(&mut app, "GET", "/api/v1/profile", None).await;
    assert_eq!(status, axum::http::StatusCode::OK);
    assert_eq!(profile["data"]["id"], "local-user");

    let (status, ingest) = json_response(&mut app, "GET", "/api/v1/user/ingest", None).await;
    assert_eq!(status, axum::http::StatusCode::OK);
    assert_eq!(ingest["data"]["ingestId"], "local");
    assert!(ingest["data"]["srtUrl"].is_null());

    let (status, chat_url) = json_response(&mut app, "GET", "/api/v1/chat-url", None).await;
    assert_eq!(status, axum::http::StatusCode::OK);
    assert_eq!(chat_url["data"]["webchatUrl"], "/api/v1/chat/ws");

    let (status, event) = json_response(
        &mut app,
        "POST",
        "/api/v1/events",
        Some(json!({"title": "Subresources"})),
    )
    .await;
    let event_id = event["data"]["id"].as_str().unwrap();
    assert_eq!(status, axum::http::StatusCode::CREATED);

    let (status, srt_keys) = json_response(
        &mut app,
        "GET",
        &format!("/api/v1/events/{event_id}/srt-keys"),
        None,
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::OK);
    assert!(srt_keys["data"]["primary"].is_null());

    let (status, recording_error) = json_response(
        &mut app,
        "POST",
        &format!("/api/v1/events/{event_id}/recordings/start"),
        None,
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::CONFLICT);
    assert_eq!(recording_error["error"]["code"], "event_not_live");

    let (status, recordings) = json_response(
        &mut app,
        "GET",
        &format!("/api/v1/events/{event_id}/recordings"),
        None,
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::OK);
    assert!(recordings["data"]["active"].is_null());
    assert!(recordings["data"]["primaryVideos"].is_array());

    let (status, viewers) = json_response(
        &mut app,
        "GET",
        &format!("/api/v1/events/{event_id}/viewers"),
        None,
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::OK);
    assert!(viewers["data"].is_array());

    let (status, transcriptions) = json_response(
        &mut app,
        "GET",
        &format!("/api/v1/events/{event_id}/recordings/transcriptions"),
        None,
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::OK);
    assert!(transcriptions["data"]["transcriptions"].is_array());

    let (status, oauth) = json_response(
        &mut app,
        "GET",
        "/api/v1/oauth/provider-that-is-not-configured/authorize?redirectUri=https%3A%2F%2Fexample.test%2Fcallback&state=csrf",
        None,
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::NOT_IMPLEMENTED);
    assert_eq!(oauth["error"]["code"], "oauth_not_configured");
}

#[tokio::test]
async fn scheduled_events_are_promoted_by_the_store() {
    let store = RestreamStore::new();
    let event = store
        .create_event(
            None,
            reestream_server::restream::StreamType::Encoder,
            "Scheduled".into(),
            String::new(),
            Some("1970-01-01T00:00:00Z".into()),
            Vec::new(),
            None,
            0,
        )
        .await;
    let due = store.promote_due_events().await;
    assert_eq!(due.len(), 1);
    assert_eq!(
        store.get_event(&event.id).await.unwrap().status,
        reestream_server::restream::EventStatus::Live
    );
}
