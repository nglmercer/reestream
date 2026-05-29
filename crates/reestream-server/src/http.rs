use axum::{
    Router,
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, post, put},
};
use futures_util::{SinkExt, stream::StreamExt};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::broadcast;
use tower_http::cors::CorsLayer;
use tracing::{info, warn};

use crate::dashboard;
use crate::databus::{DataBus, DataPacket};
use crate::flv::{self, FlvState};
use crate::hls::HlsSegmenter;
use crate::recording::RecordingManager;
use crate::stream::{StreamManager, StreamStatus};

#[derive(Clone)]
pub struct AppState {
    pub stream_manager: Arc<StreamManager>,
    pub hls_segmenter: Arc<HlsSegmenter>,
    pub flv_state: FlvState,
    pub data_bus: DataBus,
    pub recording_manager: Arc<RecordingManager>,
    pub start_time: std::time::Instant,
    pub config_path: std::path::PathBuf,
}

#[derive(Serialize)]
struct ApiResponse<T: Serialize> {
    success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

impl<T: Serialize> ApiResponse<T> {
    fn ok(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
        }
    }
}

impl ApiResponse<()> {
    fn err(msg: impl Into<String>) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(msg.into()),
        }
    }
}

#[derive(Serialize)]
struct ServerStatus {
    version: &'static str,
    uptime_seconds: u64,
    active_streams: usize,
    total_viewers: u32,
}

#[derive(Serialize)]
struct StreamStats {
    id: String,
    name: String,
    status: String,
    viewers: u32,
    bitrate: u64,
    uptime_secs: u64,
}

#[derive(Deserialize)]
struct AddPlatformRequest {
    name: String,
    url: String,
    key: String,
}

#[derive(Deserialize)]
struct UpdatePlatformRequest {
    name: Option<String>,
    url: Option<String>,
    key: Option<String>,
    enabled: Option<bool>,
}

#[derive(Deserialize)]
struct AddStreamRequest {
    name: String,
    input_url: String,
}

async fn health() -> impl IntoResponse {
    StatusCode::OK
}

async fn status(State(state): State<AppState>) -> impl IntoResponse {
    let streams = state.stream_manager.get_streams().await;
    let total_viewers: u32 = streams.iter().map(|s| s.viewers).sum();
    let resp = ApiResponse::ok(ServerStatus {
        version: env!("CARGO_PKG_VERSION"),
        uptime_seconds: state.start_time.elapsed().as_secs(),
        active_streams: streams.len(),
        total_viewers,
    });
    axum::Json(resp)
}

async fn list_streams(State(state): State<AppState>) -> impl IntoResponse {
    let streams = state.stream_manager.get_streams().await;
    axum::Json(ApiResponse::ok(streams))
}

async fn add_stream(
    State(state): State<AppState>,
    axum::Json(req): axum::Json<AddStreamRequest>,
) -> impl IntoResponse {
    let id = state
        .stream_manager
        .add_stream(req.name, req.input_url)
        .await;
    (StatusCode::CREATED, axum::Json(ApiResponse::ok(id)))
}

async fn remove_stream(State(state): State<AppState>, Path(id): Path<String>) -> impl IntoResponse {
    if state.stream_manager.remove_stream(&id).await {
        (StatusCode::OK, axum::Json(ApiResponse::ok("removed"))).into_response()
    } else {
        (
            StatusCode::NOT_FOUND,
            axum::Json(ApiResponse::err("stream not found")),
        )
            .into_response()
    }
}

async fn stream_stats(State(state): State<AppState>, Path(id): Path<String>) -> impl IntoResponse {
    let streams = state.stream_manager.get_streams().await;
    if let Some(stream) = streams.iter().find(|s| s.id == id) {
        let uptime = stream.started_at.map_or(0, |start| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
                .saturating_sub(start)
        });
        let stats = StreamStats {
            id: stream.id.clone(),
            name: stream.name.clone(),
            status: format!("{:?}", stream.status),
            viewers: stream.viewers,
            bitrate: stream.bitrate,
            uptime_secs: uptime,
        };
        axum::Json(ApiResponse::ok(stats)).into_response()
    } else {
        (
            StatusCode::NOT_FOUND,
            axum::Json(ApiResponse::err("stream not found")),
        )
            .into_response()
    }
}

async fn get_config(State(state): State<AppState>) -> impl IntoResponse {
    match reestream_core::setup::read_config(&state.config_path) {
        Ok(config) => {
            let platforms: Vec<serde_json::Value> = config
                .platform
                .unwrap_or_default()
                .iter()
                .enumerate()
                .map(|(i, p)| {
                    serde_json::json!({
                        "index": i,
                        "url": p.url.to_string(),
                        "key_masked": mask_key(&p.key),
                        "orientation": format!("{:?}", p.orientation).to_lowercase(),
                    })
                })
                .collect();

            axum::Json(ApiResponse::ok(serde_json::json!({
                "rtmp_addr": config.rtmp_addr,
                "rtmp_port": config.rtmp_port,
                "stream_key_masked": mask_key(&config.stream_key),
                "platform_count": platforms.len(),
                "platforms": platforms,
            })))
            .into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            axum::Json(ApiResponse::<()>::err(format!(
                "Failed to read config: {e}"
            ))),
        )
            .into_response(),
    }
}

fn mask_key(key: &str) -> String {
    if key.len() <= 4 {
        "****".to_string()
    } else {
        format!("{}…{}", &key[..4], &key[key.len() - 4..])
    }
}

async fn update_config(
    State(state): State<AppState>,
    axum::Json(req): axum::Json<serde_json::Value>,
) -> impl IntoResponse {
    let rtmp_addr = req.get("rtmp_addr").and_then(|v| v.as_str());
    let rtmp_port = req
        .get("rtmp_port")
        .and_then(|v| v.as_u64())
        .map(|p| p as u16);
    let stream_key = req.get("stream_key").and_then(|v| v.as_str());

    match reestream_core::setup::update_config_fields(
        &state.config_path,
        rtmp_addr,
        rtmp_port,
        stream_key,
    ) {
        Ok(config) => {
            info!("Config updated via API");
            axum::Json(ApiResponse::ok(serde_json::json!({
                "rtmp_addr": config.rtmp_addr,
                "rtmp_port": config.rtmp_port,
                "platform_count": config.platform.as_ref().map_or(0, |p| p.len()),
            })))
            .into_response()
        }
        Err(e) => (
            StatusCode::BAD_REQUEST,
            axum::Json(ApiResponse::<()>::err(format!("Config update failed: {e}"))),
        )
            .into_response(),
    }
}

async fn reload_config() -> impl IntoResponse {
    info!("Config reload requested via API");
    axum::Json(ApiResponse::ok("config reload triggered"))
}

async fn setup_status(State(state): State<AppState>) -> impl IntoResponse {
    let status = reestream_core::setup::get_setup_status(&state.config_path);
    axum::Json(ApiResponse::ok(status))
}

async fn setup_save(
    State(state): State<AppState>,
    axum::Json(req): axum::Json<serde_json::Value>,
) -> impl IntoResponse {
    let setup_req: reestream_core::setup::SetupRequest = match serde_json::from_value(req) {
        Ok(r) => r,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                axum::Json(ApiResponse::<()>::err(format!("Invalid request: {e}"))),
            )
                .into_response();
        }
    };

    match reestream_core::setup::apply_setup(&state.config_path, &setup_req) {
        Ok(config) => {
            info!("Configuration saved via setup wizard");
            (
                StatusCode::OK,
                axum::Json(ApiResponse::ok(format!(
                    "Config saved with {} platforms",
                    config.platform.as_ref().map_or(0, |p| p.len())
                ))),
            )
                .into_response()
        }
        Err(e) => (
            StatusCode::BAD_REQUEST,
            axum::Json(ApiResponse::<()>::err(format!("Setup failed: {e}"))),
        )
            .into_response(),
    }
}

async fn server_info(State(state): State<AppState>) -> impl IntoResponse {
    match reestream_core::setup::get_server_info(&state.config_path) {
        Ok(info) => axum::Json(ApiResponse::ok(info)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            axum::Json(ApiResponse::<()>::err(format!(
                "Failed to get server info: {e}"
            ))),
        )
            .into_response(),
    }
}

async fn reveal_stream_key(State(state): State<AppState>) -> impl IntoResponse {
    match reestream_core::setup::get_stream_key(&state.config_path) {
        Ok(key) => axum::Json(ApiResponse::ok(key)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            axum::Json(ApiResponse::<()>::err(format!(
                "Failed to get stream key: {e}"
            ))),
        )
            .into_response(),
    }
}

async fn reset_stream_key(State(state): State<AppState>) -> impl IntoResponse {
    match reestream_core::setup::reset_stream_key(&state.config_path) {
        Ok(new_key) => {
            info!("Stream key reset via API");
            axum::Json(ApiResponse::ok(new_key)).into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            axum::Json(ApiResponse::<()>::err(format!("Failed to reset key: {e}"))),
        )
            .into_response(),
    }
}

async fn list_platforms(State(state): State<AppState>) -> impl IntoResponse {
    let platforms = state.stream_manager.get_platforms().await;
    axum::Json(ApiResponse::ok(platforms))
}

async fn add_platform(
    State(state): State<AppState>,
    axum::Json(req): axum::Json<AddPlatformRequest>,
) -> impl IntoResponse {
    // Add to runtime
    let id = state
        .stream_manager
        .add_platform(req.name.clone(), req.url.clone(), req.key.clone())
        .await;

    // Sync to config.toml
    if let Err(e) = reestream_core::setup::add_platform_to_config(
        &state.config_path,
        &req.url,
        &req.key,
        "horizontal",
    ) {
        warn!("Failed to sync platform to config: {}", e);
    } else {
        info!("Platform added and synced to config.toml");
    }

    (StatusCode::CREATED, axum::Json(ApiResponse::ok(id)))
}

async fn remove_platform(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    // Find platform index in config before removing from runtime
    let platforms = state.stream_manager.get_platforms().await;
    let platform_index = platforms.iter().position(|p| p.id == id);

    if state.stream_manager.remove_platform(&id).await {
        // Sync to config.toml
        if let Some(idx) = platform_index {
            if let Err(e) =
                reestream_core::setup::remove_platform_from_config(&state.config_path, idx)
            {
                warn!("Failed to sync platform removal to config: {}", e);
            } else {
                info!("Platform removed and synced to config.toml");
            }
        }
        (StatusCode::OK, axum::Json(ApiResponse::ok("removed"))).into_response()
    } else {
        (
            StatusCode::NOT_FOUND,
            axum::Json(ApiResponse::err("platform not found")),
        )
            .into_response()
    }
}

async fn toggle_platform(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let platforms = state.stream_manager.get_platforms().await;
    if let Some(p) = platforms.iter().find(|p| p.id == id) {
        let new_enabled = !p.enabled;
        state.stream_manager.toggle_platform(&id, new_enabled).await;

        // Persist to config.toml
        if let Ok(mut config) = reestream_core::setup::read_config(&state.config_path)
            && let Some(ref mut cfg_platforms) = config.platform
        {
            for cp in cfg_platforms.iter_mut() {
                if cp.url.as_str() == p.url.as_str() && cp.key == p.key {
                    cp.enabled = new_enabled;
                    break;
                }
            }
            let _ = reestream_core::setup::save_config(&state.config_path, &config);
            info!(
                "Platform {} toggled to {} and saved to config",
                id, new_enabled
            );
        }

        let state_label = if new_enabled { "enabled" } else { "disabled" };
        (StatusCode::OK, axum::Json(ApiResponse::ok(state_label))).into_response()
    } else {
        (
            StatusCode::NOT_FOUND,
            axum::Json(ApiResponse::err("platform not found")),
        )
            .into_response()
    }
}

async fn update_platform(
    State(state): State<AppState>,
    Path(id): Path<String>,
    axum::Json(req): axum::Json<UpdatePlatformRequest>,
) -> impl IntoResponse {
    // Find platform index in config
    let platforms = state.stream_manager.get_platforms().await;
    let platform_index = platforms.iter().position(|p| p.id == id);

    let updated = state
        .stream_manager
        .update_platform(
            &id,
            req.name.clone(),
            req.url.clone(),
            req.key.clone(),
            req.enabled,
        )
        .await;

    if updated {
        // Sync to config.toml
        if let Some(idx) = platform_index {
            if let Err(e) = reestream_core::setup::update_platform_in_config(
                &state.config_path,
                idx,
                req.url.as_deref(),
                req.key.as_deref(),
                None,
            ) {
                warn!("Failed to sync platform update to config: {}", e);
            } else {
                info!("Platform {} updated and synced to config.toml", id);
            }
        }
        (StatusCode::OK, axum::Json(ApiResponse::ok("updated"))).into_response()
    } else {
        (
            StatusCode::NOT_FOUND,
            axum::Json(ApiResponse::err("platform not found")),
        )
            .into_response()
    }
}

async fn list_recordings(State(state): State<AppState>) -> impl IntoResponse {
    let recordings = state.recording_manager.list_recordings().await;
    axum::Json(ApiResponse::ok(recordings))
}

async fn start_recording(
    State(state): State<AppState>,
    axum::Json(req): axum::Json<serde_json::Value>,
) -> impl IntoResponse {
    let stream_id = req
        .get("stream_id")
        .and_then(|v| v.as_str())
        .unwrap_or("default");
    let input_url = req
        .get("input_url")
        .and_then(|v| v.as_str())
        .unwrap_or("rtmp://0.0.0.0:1935/live");

    match state
        .recording_manager
        .start_recording(stream_id, input_url)
        .await
    {
        Ok(id) => {
            info!("Recording started: {}", id);
            (StatusCode::CREATED, axum::Json(ApiResponse::ok(id))).into_response()
        }
        Err(e) => (
            StatusCode::BAD_REQUEST,
            axum::Json(ApiResponse::<()>::err(e)),
        )
            .into_response(),
    }
}

async fn stop_recording(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.recording_manager.stop_recording(&id).await {
        Ok(()) => axum::Json(ApiResponse::ok("stopped")).into_response(),
        Err(e) => (StatusCode::NOT_FOUND, axum::Json(ApiResponse::<()>::err(e))).into_response(),
    }
}

async fn delete_recording(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.recording_manager.delete_recording(&id).await {
        Ok(()) => axum::Json(ApiResponse::ok("deleted")).into_response(),
        Err(e) => (StatusCode::NOT_FOUND, axum::Json(ApiResponse::<()>::err(e))).into_response(),
    }
}

async fn hls_playlist(State(state): State<AppState>) -> impl IntoResponse {
    let segments = state.hls_segmenter.get_segments().await;
    let playlist = state.hls_segmenter.generate_playlist(&segments, true);
    (
        StatusCode::OK,
        [("content-type", "application/vnd.apple.mpegurl")],
        playlist,
    )
}

async fn hls_segment(
    State(state): State<AppState>,
    Path(filename): Path<String>,
) -> impl IntoResponse {
    let segments = state.hls_segmenter.get_segments().await;
    if segments.iter().any(|s| s.filename == filename) {
        let segment_dir = &state.hls_segmenter.config().segment_dir;
        let path = segment_dir.join(&filename);
        match tokio::fs::read(&path).await {
            Ok(data) => (StatusCode::OK, [("content-type", "video/mp2t")], data).into_response(),
            Err(_) => StatusCode::NOT_FOUND.into_response(),
        }
    } else {
        StatusCode::NOT_FOUND.into_response()
    }
}

async fn flv_stream(State(state): State<AppState>) -> impl IntoResponse {
    flv::flv_stream_response(state.flv_state)
}

async fn ws_streams(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_ws_stream(socket, state))
}

async fn handle_ws_stream(ws: WebSocket, state: AppState) {
    let (mut sender, mut _receiver) = ws.split();
    let mut rx = state.stream_manager.subscribe();

    // Send initial state
    let streams = state.stream_manager.get_streams().await;
    let msg = serde_json::json!({
        "type": "init",
        "streams": streams,
    });
    if let Ok(text) = serde_json::to_string(&msg) {
        let _ = sender.send(Message::text(text)).await;
    }

    // Forward events
    loop {
        match rx.recv().await {
            Ok(event) => {
                let msg = serde_json::json!({
                    "type": "event",
                    "event": event,
                });
                if let Ok(text) = serde_json::to_string(&msg)
                    && sender.send(Message::text(text)).await.is_err()
                {
                    break;
                }
            }
            Err(broadcast::error::RecvError::Lagged(_)) => continue,
            Err(_) => break,
        }
    }
}

async fn metrics(State(state): State<AppState>) -> impl IntoResponse {
    let streams = state.stream_manager.get_streams().await;
    let total_viewers: u32 = streams.iter().map(|s| s.viewers).sum();
    let uptime = state.start_time.elapsed().as_secs();

    let mut metrics = String::new();
    metrics.push_str("# HELP reestream_uptime_seconds Server uptime\n");
    metrics.push_str("# TYPE reestream_uptime_seconds gauge\n");
    metrics.push_str(&format!("reestream_uptime_seconds {uptime}\n"));
    metrics.push_str("# HELP reestream_streams_total Number of streams\n");
    metrics.push_str("# TYPE reestream_streams_total gauge\n");
    metrics.push_str(&format!("reestream_streams_total {}\n", streams.len()));
    metrics.push_str("# HELP reestream_viewers_total Total viewers\n");
    metrics.push_str("# TYPE reestream_viewers_total gauge\n");
    metrics.push_str(&format!("reestream_viewers_total {total_viewers}\n"));

    for stream in &streams {
        let status = match &stream.status {
            StreamStatus::Live => 1,
            _ => 0,
        };
        metrics.push_str(&format!(
            "reestream_stream_status{{id=\"{}\",name=\"{}\"}} {status}\n",
            stream.id, stream.name
        ));
        metrics.push_str(&format!(
            "reestream_stream_bitrate_kbps{{id=\"{}\"}} {}\n",
            stream.id, stream.bitrate
        ));
    }

    (
        StatusCode::OK,
        [("content-type", "text/plain; version=0.0.4")],
        metrics,
    )
}

pub fn create_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/", get(dashboard::serve_index))
        .route("/dashboard", get(dashboard::serve_index))
        .route("/assets/{*path}", get(dashboard::serve_assets))
        .route("/favicon.svg", get(dashboard::serve_favicon))
        .route("/ws/streams", get(ws_streams))
        .route("/api/status", get(status))
        .route("/api/streams", get(list_streams).post(add_stream))
        .route("/api/streams/{id}", delete(remove_stream))
        .route("/api/streams/{id}/stats", get(stream_stats))
        .route("/api/config", get(get_config).put(update_config))
        .route("/api/config/reload", post(reload_config))
        .route("/api/setup/status", get(setup_status))
        .route("/api/setup/save", post(setup_save))
        .route("/api/setup/info", get(server_info))
        .route(
            "/api/setup/key",
            get(reveal_stream_key).post(reset_stream_key),
        )
        .route("/api/platforms", get(list_platforms).post(add_platform))
        .route(
            "/api/platforms/{id}",
            delete(remove_platform).put(update_platform),
        )
        .route("/api/platforms/{id}/toggle", put(toggle_platform))
        .route("/api/recordings", get(list_recordings))
        .route("/api/recordings/start", post(start_recording))
        .route("/api/recordings/{id}/stop", post(stop_recording))
        .route("/api/recordings/{id}", delete(delete_recording))
        .route("/stream.m3u8", get(hls_playlist))
        .route("/hls/{filename}", get(hls_segment))
        .route("/stream.flv", get(flv_stream))
        .route("/metrics", get(metrics))
        .layer(CorsLayer::permissive())
        .with_state(state)
}

pub async fn start_http_server(
    addr: &str,
    port: u16,
    state: AppState,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let bind = format!("{addr}:{port}");
    let listener = tokio::net::TcpListener::bind(&bind).await?;
    info!("HTTP server listening on {}", bind);
    axum::serve(listener, create_router(state)).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hls::HlsConfig;
    use crate::recording::RecordingConfig;

    fn test_state() -> AppState {
        let hls_config = HlsConfig::default();
        AppState {
            stream_manager: Arc::new(StreamManager::new()),
            hls_segmenter: Arc::new(HlsSegmenter::new(hls_config)),
            flv_state: FlvState::default(),
            data_bus: crate::databus::DataBus::default(),
            recording_manager: Arc::new(RecordingManager::new(RecordingConfig::default())),
            start_time: std::time::Instant::now(),
            config_path: std::path::PathBuf::from("/tmp/test_config.toml"),
        }
    }

    #[test]
    fn test_api_response_ok() {
        let resp = ApiResponse::ok("test");
        assert!(resp.success);
        assert_eq!(resp.data.unwrap(), "test");
    }

    #[test]
    fn test_api_response_err() {
        let resp: ApiResponse<()> = ApiResponse::err("fail");
        assert!(!resp.success);
        assert_eq!(resp.error.unwrap(), "fail");
    }

    #[test]
    fn test_create_router() {
        let state = test_state();
        let _router = create_router(state);
    }

    #[test]
    fn test_stream_stats_serialize() {
        let stats = StreamStats {
            id: "test-id".into(),
            name: "test".into(),
            status: "Live".into(),
            viewers: 10,
            bitrate: 5000,
            uptime_secs: 3600,
        };
        let json = serde_json::to_string(&stats).unwrap();
        assert!(json.contains("test-id"));
        assert!(json.contains("5000"));
    }
}
