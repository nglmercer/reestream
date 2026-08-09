use axum::{
    Router,
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    extract::{Path, State},
    http::{HeaderValue, StatusCode},
    middleware,
    response::IntoResponse,
    routing::{delete, get, post, put},
};
use futures_util::{SinkExt, stream::StreamExt};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::broadcast;
use tower_http::cors::{Any, CorsLayer};
use tracing::{info, warn};

use crate::dashboard;
use crate::databus::DataBus;
use crate::flv::{self, FlvState};
use crate::hls::HlsSegmenter;
use crate::playback::PlaybackManager;
use crate::recording::RecordingManager;
use crate::stream::{StreamManager, StreamStatus};

#[derive(Clone)]
pub struct AppState {
    pub stream_manager: Arc<StreamManager>,
    pub hls_segmenter: Arc<HlsSegmenter>,
    pub flv_state: FlvState,
    pub data_bus: DataBus,
    pub recording_manager: Arc<RecordingManager>,
    pub playback_manager: Arc<PlaybackManager>,
    pub start_time: std::time::Instant,
    pub config_path: std::path::PathBuf,
    pub restream: Arc<crate::restream::RestreamStore>,
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
    (StatusCode::CREATED, axum::Json(ApiResponse::ok(id))).into_response()
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

    let config = match reestream_core::setup::update_config_fields(
        &state.config_path,
        rtmp_addr,
        rtmp_port,
        stream_key,
    ) {
        Ok(config) => config,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                axum::Json(ApiResponse::<()>::err(format!("Config update failed: {e}"))),
            )
                .into_response();
        }
    };
    state
        .restream
        .set_runtime_stream_key(config.stream_key.clone())
        .await;
    info!("Config updated via API");
    axum::Json(ApiResponse::ok(serde_json::json!({
        "rtmp_addr": config.rtmp_addr,
        "rtmp_port": config.rtmp_port,
        "restart_required": true,
        "platform_count": config.platform.as_ref().map_or(0, |p| p.len()),
    })))
    .into_response()
}

async fn reload_config() -> impl IntoResponse {
    info!("Config reload requested via API; restart is required for listener changes");
    axum::Json(ApiResponse::ok(serde_json::json!({
        "reload": false,
        "restart_required": true,
        "message": "listener configuration changes require a process restart",
    })))
}

async fn setup_status(State(state): State<AppState>) -> impl IntoResponse {
    let status = reestream_core::setup::get_setup_status(&state.config_path);
    axum::Json(ApiResponse::ok(status))
}

async fn setup_save(
    State(state): State<AppState>,
    axum::Json(req): axum::Json<serde_json::Value>,
) -> impl IntoResponse {
    if !reestream_core::setup::get_setup_status(&state.config_path).first_run {
        return (
            StatusCode::CONFLICT,
            axum::Json(ApiResponse::<()>::err(
                "initial setup is already complete; authenticate before changing configuration",
            )),
        )
            .into_response();
    }
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

    let config = match reestream_core::setup::apply_setup(&state.config_path, &setup_req) {
        Ok(config) => config,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                axum::Json(ApiResponse::<()>::err(format!("Setup failed: {e}"))),
            )
                .into_response();
        }
    };
    state
        .restream
        .set_runtime_stream_key(config.stream_key.clone())
        .await;
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
    let new_key = match reestream_core::setup::reset_stream_key(&state.config_path) {
        Ok(new_key) => new_key,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                axum::Json(ApiResponse::<()>::err(format!("Failed to reset key: {e}"))),
            )
                .into_response();
        }
    };
    state.restream.set_runtime_stream_key(new_key.clone()).await;
    info!("Stream key reset via API");
    axum::Json(ApiResponse::ok(new_key)).into_response()
}

async fn list_platforms(State(state): State<AppState>) -> impl IntoResponse {
    let platforms = state.stream_manager.get_platforms().await;
    axum::Json(ApiResponse::ok(platforms))
}

async fn add_platform(
    State(state): State<AppState>,
    axum::Json(req): axum::Json<AddPlatformRequest>,
) -> impl IntoResponse {
    if req.name.trim().is_empty() || req.key.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            axum::Json(ApiResponse::<()>::err("platform name and key are required")),
        )
            .into_response();
    }
    if let Err(response) = crate::api_v1::parse_destination_url(&req.url, "url") {
        return response;
    }
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

    (StatusCode::CREATED, axum::Json(ApiResponse::ok(id))).into_response()
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
    if let Some(url) = req.url.as_deref()
        && let Err(response) = crate::api_v1::parse_destination_url(url, "url")
    {
        return response;
    }
    if req
        .key
        .as_deref()
        .is_some_and(|value| value.trim().is_empty())
    {
        return (
            StatusCode::BAD_REQUEST,
            axum::Json(ApiResponse::<()>::err("platform key cannot be empty")),
        )
            .into_response();
    }
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
        .map(ToOwned::to_owned);
    let input_url = match input_url {
        Some(input_url) => input_url,
        None => state.restream.runtime_ingest_url().await,
    };
    if let Err(response) = crate::api_v1::validate_media_input_url(&input_url, "input_url") {
        return response;
    }

    match state
        .recording_manager
        .start_recording(stream_id, &input_url)
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
    // Try to read ffmpeg-generated playlist first
    let segment_dir = &state.hls_segmenter.config().segment_dir;
    let playlist_path = segment_dir.join("stream.m3u8");

    if let Ok(playlist) = tokio::fs::read_to_string(&playlist_path).await {
        // Rewrite segment paths to use /hls/ prefix
        let rewritten = playlist
            .lines()
            .map(|line| {
                if line.ends_with(".ts") && !line.starts_with('#') {
                    format!("/hls/{}", line)
                } else {
                    line.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n");

        return (
            StatusCode::OK,
            [("content-type", "application/vnd.apple.mpegurl")],
            rewritten,
        );
    }

    // Fall back to in-memory segmenter
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
    if filename.is_empty()
        || filename
            != std::path::Path::new(&filename)
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default()
        || !filename.ends_with(".ts")
    {
        return StatusCode::NOT_FOUND.into_response();
    }
    // Try ffmpeg-generated segments first
    let segment_dir = &state.hls_segmenter.config().segment_dir;
    let path = segment_dir.join(&filename);
    if path.exists() {
        match tokio::fs::read(&path).await {
            Ok(data) => {
                return (StatusCode::OK, [("content-type", "video/mp2t")], data).into_response();
            }
            Err(_) => return StatusCode::NOT_FOUND.into_response(),
        }
    }

    // Fall back to in-memory segmenter
    let segments = state.hls_segmenter.get_segments().await;
    if segments.iter().any(|s| s.filename == filename) {
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
            prometheus_escape(&stream.id),
            prometheus_escape(&stream.name)
        ));
        metrics.push_str(&format!(
            "reestream_stream_bitrate_kbps{{id=\"{}\"}} {}\n",
            prometheus_escape(&stream.id),
            stream.bitrate
        ));
    }

    (
        StatusCode::OK,
        [("content-type", "text/plain; version=0.0.4")],
        metrics,
    )
}

fn prometheus_escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace('"', "\\\"")
}

pub fn create_router(state: AppState) -> Router {
    let protected_legacy = Router::new()
        .route("/ws/streams", get(ws_streams))
        .route("/api/streams", get(list_streams).post(add_stream))
        .route("/api/streams/{id}", delete(remove_stream))
        .route("/api/streams/{id}/stats", get(stream_stats))
        .route("/api/config", get(get_config).put(update_config))
        .route("/api/config/reload", post(reload_config))
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
        .route("/metrics", get(metrics))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            crate::api_v1::require_auth,
        ));

    let cors = match std::env::var("RESTREAM_CORS_ORIGIN") {
        Ok(origin) if !origin.trim().is_empty() => match origin.parse::<HeaderValue>() {
            Ok(origin) => CorsLayer::new()
                .allow_origin(origin)
                .allow_methods(Any)
                .allow_headers(Any),
            Err(error) => {
                warn!(%error, "invalid RESTREAM_CORS_ORIGIN; CORS disabled");
                CorsLayer::new()
            }
        },
        _ => CorsLayer::new(),
    };

    Router::new()
        .route("/health", get(health))
        .route("/", get(dashboard::serve_index))
        .route("/dashboard", get(dashboard::serve_index))
        .route("/setup", get(dashboard::serve_index))
        .route("/assets/{*path}", get(dashboard::serve_assets))
        .route("/favicon.svg", get(dashboard::serve_favicon))
        .route("/{*path}", get(dashboard::serve_static))
        .route("/api/status", get(status))
        .route("/api/setup/status", get(setup_status))
        .route("/api/setup/save", post(setup_save))
        .route("/stream.m3u8", get(hls_playlist))
        .route("/hls/{filename}", get(hls_segment))
        .route("/stream.flv", get(flv_stream))
        .merge(protected_legacy)
        .merge(crate::api_v1::routes(state.clone()))
        .layer(cors)
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
            playback_manager: Arc::new(PlaybackManager::default()),
            start_time: std::time::Instant::now(),
            config_path: std::path::PathBuf::from("/tmp/test_config.toml"),
            restream: Arc::new(crate::restream::RestreamStore::new()),
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

    #[tokio::test]
    async fn test_hls_playlist_empty() {
        use axum::body::Body;
        use axum::http::{Request, StatusCode};
        use tower::ServiceExt;

        let state = test_state();
        let app = create_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/stream.m3u8")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let playlist = String::from_utf8(body.to_vec()).unwrap();

        // Should be a valid M3U8 playlist
        assert!(playlist.contains("#EXTM3U"));
        assert!(playlist.contains("#EXT-X-VERSION:3"));
        assert!(playlist.contains("#EXT-X-TARGETDURATION:"));
    }

    #[tokio::test]
    async fn test_hls_segment_not_found() {
        use axum::body::Body;
        use axum::http::{Request, StatusCode};
        use tower::ServiceExt;

        let state = test_state();
        let app = create_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/hls/nonexistent.ts")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_flv_stream_returns_flv_content_type() {
        use axum::body::Body;
        use axum::http::{Request, StatusCode};
        use tower::ServiceExt;

        let state = test_state();
        let app = create_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/stream.flv")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response
                .headers()
                .get("content-type")
                .unwrap()
                .to_str()
                .unwrap(),
            "video/x-flv"
        );
        assert_eq!(
            response
                .headers()
                .get("cache-control")
                .unwrap()
                .to_str()
                .unwrap(),
            "no-cache"
        );
    }

    #[tokio::test]
    async fn test_api_streams_endpoint() {
        use axum::body::Body;
        use axum::http::{Request, StatusCode};
        use tower::ServiceExt;

        let state = test_state();
        let app = create_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/streams")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["success"].as_bool().unwrap());
        assert!(json["data"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_api_status_endpoint() {
        use axum::body::Body;
        use axum::http::{Request, StatusCode};
        use tower::ServiceExt;

        let state = test_state();
        let app = create_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/status")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["success"].as_bool().unwrap());
        assert!(json["data"]["version"].is_string());
    }

    #[tokio::test]
    async fn test_setup_route_serves_dashboard() {
        use axum::body::Body;
        use axum::http::{Request, StatusCode};
        use tower::ServiceExt;

        let response = create_router(test_state())
            .oneshot(
                Request::builder()
                    .uri("/setup")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response
                .headers()
                .get("content-type")
                .unwrap()
                .to_str()
                .unwrap(),
            "text/html; charset=utf-8"
        );
    }

    #[tokio::test]
    async fn test_dashboard_routes_support_spa_refreshes() {
        use axum::body::Body;
        use axum::http::Request;
        use tower::ServiceExt;

        for path in ["/home", "/shows/stream-1"] {
            let response = create_router(test_state())
                .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
                .await
                .unwrap();

            assert_eq!(response.status(), StatusCode::OK, "route {path}");
            assert_eq!(
                response
                    .headers()
                    .get("content-type")
                    .unwrap()
                    .to_str()
                    .unwrap(),
                "text/html; charset=utf-8",
                "route {path}"
            );
        }
    }
}
