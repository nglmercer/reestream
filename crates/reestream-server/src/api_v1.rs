//! Versioned product API used by the future web client.
//!
//! The legacy `/api/*` handlers remain available for the existing dashboard.
//! New clients should use `/api/v1/*`; these routes use camelCase JSON,
//! explicit resource lifecycles, pagination-ready list responses, and a
//! stable envelope.

use axum::{
    Json, Router,
    body::Body,
    extract::{
        Multipart, Path, Query, State, WebSocketUpgrade,
        multipart::Field,
        ws::{Message, WebSocket},
    },
    http::{HeaderMap, HeaderValue, Request, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{delete, get, patch, post},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use uuid::Uuid;

use crate::http::AppState;
use crate::restream::{
    Brand, Caption, Channel, EventStatus, QrCode, RestreamStore, StorageFile, StreamType, Ticker,
    WebhookSubscription, ingest_servers_for_url, is_valid_scheduled_for, now, platform_catalog,
};

#[derive(Debug, Serialize)]
struct Envelope<T: Serialize> {
    success: bool,
    data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<ApiError>,
    #[serde(skip_serializing_if = "Option::is_none")]
    meta: Option<PaginationMeta>,
}

#[derive(Debug, Serialize)]
struct ApiError {
    code: String,
    message: String,
}

#[derive(Debug, Serialize)]
struct PaginationMeta {
    page: usize,
    limit: usize,
    total: usize,
}

fn ok<T: Serialize>(data: T) -> Response {
    Json(Envelope {
        success: true,
        data: Some(data),
        error: None,
        meta: None,
    })
    .into_response()
}

fn created<T: Serialize>(data: T) -> Response {
    (StatusCode::CREATED, ok(data)).into_response()
}

fn list<T: Serialize>(items: Vec<T>, query: &ListQuery) -> Response {
    let total = items.len();
    let page = query.page.unwrap_or(1).max(1);
    let limit = query.limit.unwrap_or(50).clamp(1, 200);
    let start = page.saturating_sub(1).saturating_mul(limit);
    let page_items = items
        .into_iter()
        .skip(start)
        .take(limit)
        .collect::<Vec<_>>();
    Json(Envelope {
        success: true,
        data: Some(page_items),
        error: None,
        meta: Some(PaginationMeta { page, limit, total }),
    })
    .into_response()
}

fn error(status: StatusCode, code: &str, message: impl Into<String>) -> Response {
    (
        status,
        Json(Envelope::<Value> {
            success: false,
            data: None,
            error: Some(ApiError {
                code: code.into(),
                message: message.into(),
            }),
            meta: None,
        }),
    )
        .into_response()
}

fn not_found(resource: &str) -> Response {
    error(
        StatusCode::NOT_FOUND,
        "not_found",
        format!("{resource} not found"),
    )
}

fn bad_request(message: impl Into<String>) -> Response {
    error(StatusCode::BAD_REQUEST, "invalid_request", message)
}

#[allow(clippy::result_large_err)]
fn parse_url(value: &str, field: &str) -> Result<(), Response> {
    let parsed =
        url::Url::parse(value).map_err(|_| bad_request(format!("{field} must be a URL")))?;
    if parsed.scheme().is_empty() || parsed.host_str().is_none() {
        return Err(bad_request(format!("{field} must include a host")));
    }
    Ok(())
}

#[allow(clippy::result_large_err)]
pub(crate) fn parse_destination_url(value: &str, field: &str) -> Result<(), Response> {
    if value.len() > 2048 {
        return Err(bad_request(format!("{field} is too long")));
    }
    let parsed = url::Url::parse(value)
        .map_err(|_| bad_request(format!("{field} must be a valid RTMP URL")))?;
    if !matches!(parsed.scheme(), "rtmp" | "rtmps")
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return Err(bad_request(format!(
            "{field} must use rtmp:// or rtmps:// and include a host"
        )));
    }
    Ok(())
}

#[allow(clippy::result_large_err)]
fn parse_http_url(value: &str, field: &str) -> Result<url::Url, Response> {
    let parsed = url::Url::parse(value)
        .map_err(|_| bad_request(format!("{field} must be a valid HTTP(S) URL")))?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
        return Err(bad_request(format!(
            "{field} must use http:// or https:// and include a host"
        )));
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(bad_request(format!(
            "{field} cannot contain embedded credentials"
        )));
    }
    if is_private_host(&parsed) {
        return Err(bad_request(format!(
            "{field} cannot target a private address"
        )));
    }
    Ok(parsed)
}

#[allow(clippy::result_large_err)]
pub(crate) fn validate_media_input_url(value: &str, field: &str) -> Result<(), Response> {
    if value.len() > 2048 {
        return Err(bad_request(format!("{field} is too long")));
    }
    let parsed = url::Url::parse(value)
        .map_err(|_| bad_request(format!("{field} must be a valid media URL")))?;
    if !matches!(parsed.scheme(), "http" | "https" | "rtmp" | "rtmps")
        || parsed.host_str().is_none()
        || parsed.username() != ""
        || parsed.password().is_some()
    {
        return Err(bad_request(format!(
            "{field} must use http(s) or RTMP without embedded credentials"
        )));
    }
    let allow_private = std::env::var("RESTREAM_ALLOW_PRIVATE_MEDIA_INPUTS")
        .map(|value| matches!(value.as_str(), "1" | "true" | "yes"))
        .unwrap_or(false);
    if !allow_private && is_private_host(&parsed) {
        return Err(bad_request(format!(
            "{field} cannot target a private address unless RESTREAM_ALLOW_PRIVATE_MEDIA_INPUTS is enabled"
        )));
    }
    Ok(())
}

fn is_private_host(parsed: &url::Url) -> bool {
    let Some(host) = parsed.host_str() else {
        return false;
    };
    if host.eq_ignore_ascii_case("localhost")
        || host.ends_with(".localhost")
        || host.ends_with(".local")
    {
        return true;
    }
    host.parse::<std::net::IpAddr>()
        .map(|ip| match ip {
            std::net::IpAddr::V4(ip) => {
                ip.is_loopback() || ip.is_private() || ip.is_link_local() || ip.is_unspecified()
            }
            std::net::IpAddr::V6(ip) => {
                ip.is_loopback()
                    || ip.is_unspecified()
                    || ip.is_unique_local()
                    || ip.is_unicast_link_local()
            }
        })
        .unwrap_or(false)
}

async fn validate_destination_ids(store: &RestreamStore, ids: &[String]) -> Result<(), Response> {
    for destination_id in ids {
        if store.get_channel(destination_id).await.is_none() {
            return Err(error(
                StatusCode::UNPROCESSABLE_ENTITY,
                "unknown_channel",
                format!("channel {destination_id} does not exist"),
            ));
        }
    }
    Ok(())
}

fn max_upload_bytes() -> u64 {
    std::env::var("RESTREAM_MAX_UPLOAD_BYTES")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(2 * 1024 * 1024 * 1024)
}

async fn multipart_text(field: &mut Field<'_>, max_bytes: usize) -> Result<String, String> {
    let mut value = Vec::new();
    while let Some(chunk) = field.chunk().await.map_err(|error| error.to_string())? {
        if value.len().saturating_add(chunk.len()) > max_bytes {
            return Err(format!(
                "multipart text field exceeds the {max_bytes} byte limit"
            ));
        }
        value.extend_from_slice(&chunk);
    }
    String::from_utf8(value).map_err(|_| "multipart text field must be UTF-8".into())
}

fn safe_download_filename(value: &str) -> String {
    let safe: String = value
        .chars()
        .take(200)
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-') {
                character
            } else {
                '_'
            }
        })
        .collect();
    if safe.is_empty() {
        "download".into()
    } else {
        safe
    }
}

#[allow(clippy::result_large_err)]
fn required(value: &str, field: &str) -> Result<String, Response> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        Err(bad_request(format!("{field} is required")))
    } else {
        Ok(trimmed.into())
    }
}

#[allow(clippy::result_large_err)]
fn parse_stream_type(value: Option<&str>) -> Result<StreamType, Response> {
    match value.unwrap_or("encoder").to_ascii_lowercase().as_str() {
        "studio" => Ok(StreamType::Studio),
        "encoder" | "rtmp" => Ok(StreamType::Encoder),
        "file" | "video" => Ok(StreamType::File),
        "playlist" => Ok(StreamType::Playlist),
        value => Err(bad_request(format!(
            "streamType must be studio, encoder, file, or playlist; got {value}"
        ))),
    }
}

fn parse_event_status(value: Option<&str>) -> Option<EventStatus> {
    match value?.to_ascii_lowercase().as_str() {
        "draft" => Some(EventStatus::Draft),
        "scheduled" => Some(EventStatus::Scheduled),
        "live" | "in_progress" | "in-progress" => Some(EventStatus::Live),
        "ended" | "history" => Some(EventStatus::Ended),
        "cancelled" | "canceled" => Some(EventStatus::Cancelled),
        _ => None,
    }
}

#[derive(Debug, Deserialize, Default)]
struct ListQuery {
    page: Option<usize>,
    limit: Option<usize>,
    q: Option<String>,
    status: Option<String>,
    event_id: Option<String>,
    from: Option<String>,
    to: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetupSaveRequest {
    rtmp_addr: Option<String>,
    rtmp_port: Option<u16>,
    stream_key: String,
    platforms: Vec<SetupPlatformRequest>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetupPlatformRequest {
    name: String,
    url: String,
    key: String,
    orientation: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StartRecordingRequest {
    stream_id: String,
    input_url: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LoginRequest {
    email: String,
    password: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RefreshRequest {
    refresh_token: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProfilePatch {
    display_name: Option<String>,
    timezone: Option<String>,
    avatar_url: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateChannelRequest {
    platform_id: String,
    display_name: Option<String>,
    channel_url: Option<String>,
    stream_url: String,
    stream_key: String,
    rtmp_username: Option<String>,
    rtmp_password: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateChannelRequest {
    display_name: Option<String>,
    channel_url: Option<String>,
    stream_url: Option<String>,
    stream_key: Option<String>,
    enabled: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateDraftRequest {
    name: String,
    stream_type: Option<String>,
    title: Option<String>,
    description: Option<String>,
    destination_ids: Option<Vec<String>>,
    brand_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateDraftRequest {
    name: Option<String>,
    title: Option<String>,
    description: Option<String>,
    destination_ids: Option<Vec<String>>,
    brand_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateEventRequest {
    stream_type: Option<String>,
    draft_id: Option<String>,
    title: Option<String>,
    description: Option<String>,
    scheduled_for: Option<String>,
    destination_ids: Option<Vec<String>>,
    file_id: Option<String>,
    loops_count: Option<u8>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateEventRequest {
    title: Option<String>,
    description: Option<String>,
    scheduled_for: Option<Option<String>>,
    destination_ids: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AddDestinationRequest {
    channel_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChatRequest {
    event_id: String,
    destination_id: Option<String>,
    author_name: Option<String>,
    author_id: Option<String>,
    message: String,
    reply_to: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ViewerSampleRequest {
    viewers: u32,
    bitrate_kbps: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RecordingDownloadRequest {
    file_name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateClipRequest {
    event_id: String,
    name: Option<String>,
    start_seconds: u64,
    end_seconds: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateStorageMetadataRequest {
    name: String,
    mime_type: Option<String>,
    size_bytes: Option<u64>,
    duration_seconds: Option<u64>,
    labels: Option<Vec<String>>,
    path: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateStorageRequest {
    name: Option<String>,
    labels: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StudioSessionRequest {
    event_id: String,
    layout: Option<String>,
    settings: Option<Value>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StudioSessionPatch {
    layout: Option<String>,
    settings: Option<Value>,
    status: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GuestRequest {
    name: String,
    role: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SceneRequest {
    name: String,
    layout: Option<String>,
    source_ids: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ScenePatch {
    name: Option<String>,
    layout: Option<String>,
    source_ids: Option<Vec<String>>,
    active: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClipListQuery {
    event_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WebhookRequest {
    url: Option<String>,
    secret: Option<String>,
    events: Option<Vec<String>>,
    enabled: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReorderRequest {
    ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OAuthAuthorizeQuery {
    redirect_uri: String,
    state: String,
    scope: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OAuthTokenRequest {
    code: Option<String>,
    redirect_uri: Option<String>,
    state: Option<String>,
    refresh_token: Option<String>,
    grant_type: Option<String>,
}

fn protected_routes(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/api/v1/me", get(me).patch(update_me))
        .route("/api/v1/profile", get(me).patch(update_me))
        .route("/api/v1/user/profile", get(me).patch(update_me))
        .route("/api/v1/ingest", get(selected_ingest))
        .route("/api/v1/user/ingest", get(selected_ingest))
        .route("/api/v1/stream-key", get(global_stream_key))
        .route("/api/v1/stream-key/reset", post(reset_global_stream_key))
        .route("/api/v1/user/streamKey", get(global_stream_key))
        .route("/api/v1/chat-url", get(chat_url))
        .route("/api/v1/user/webchat/url", get(chat_url))
        .route("/api/v1/connections", get(list_connections))
        .route("/api/v1/connections/{id}", delete(delete_connection))
        .route("/api/v1/oauth/{platform}/authorize", get(oauth_authorize))
        .route("/api/v1/oauth/{platform}/token", post(oauth_token))
        .route("/api/v1/channels", get(list_channels).post(create_channel))
        .route(
            "/api/v1/channels/{id}",
            get(get_channel)
                .patch(update_channel)
                .delete(delete_channel),
        )
        .route(
            "/api/v1/channels/{id}/credentials",
            get(channel_credentials),
        )
        .route("/api/v1/streams", get(list_drafts).post(create_draft))
        .route(
            "/api/v1/streams/{id}",
            get(get_draft).patch(update_draft).delete(delete_draft),
        )
        .route("/api/v1/streams/{id}/duplicate", post(duplicate_draft))
        .route("/api/v1/events", get(list_events).post(create_event))
        .route("/api/v1/events/upcoming", get(upcoming_events))
        .route("/api/v1/events/live", get(live_events))
        .route("/api/v1/events/history", get(history_events))
        .route(
            "/api/v1/events/{id}",
            get(get_event).patch(update_event).delete(delete_event),
        )
        .route(
            "/api/v1/events/{id}/destinations",
            post(add_event_destination),
        )
        .route(
            "/api/v1/events/{id}/destinations/{destination_id}",
            delete(remove_event_destination),
        )
        .route("/api/v1/events/{id}/stream-key", get(event_stream_key))
        .route("/api/v1/events/{id}/srt-keys", get(event_srt_keys))
        .route("/api/v1/events/{id}/go-live", post(go_live))
        .route("/api/v1/events/{id}/end", post(end_event))
        .route("/api/v1/events/{id}/recordings", get(event_recordings))
        .route(
            "/api/v1/events/{id}/recordings/start",
            post(start_event_recording_route),
        )
        .route(
            "/api/v1/events/{id}/recordings/stop",
            post(stop_event_recording_route),
        )
        .route(
            "/api/v1/events/{id}/recordings/download-url",
            post(recording_download_url),
        )
        .route(
            "/api/v1/events/{id}/recordings/transcriptions",
            get(event_transcriptions).post(request_transcription),
        )
        .route("/api/v1/events/{id}/chat", get(event_chat))
        .route(
            "/api/v1/events/{id}/chat/history/download-url",
            post(chat_history_download_url),
        )
        .route("/api/v1/events/{id}/analytics", get(event_analytics))
        .route(
            "/api/v1/events/{id}/analytics/viewers",
            get(event_viewer_analytics),
        )
        .route(
            "/api/v1/events/{id}/analytics/messages",
            get(event_message_analytics),
        )
        .route(
            "/api/v1/events/{id}/transcriptions",
            get(event_transcriptions).post(request_transcription),
        )
        .route("/api/v1/events/{id}/chat-export", get(event_chat_export))
        .route(
            "/api/v1/events/{id}/viewers",
            get(list_viewers).post(record_viewers),
        )
        .route("/api/v1/chat/messages", get(list_chat).post(send_chat))
        .route("/api/v1/chat/messages/{id}", delete(delete_chat))
        .route("/api/v1/chat/sources", get(chat_sources))
        .route("/api/v1/chat/actions", get(chat_actions))
        .route("/api/v1/chat/connections", get(chat_connections))
        .route("/api/v1/chat/events", get(chat_events))
        .route("/api/v1/chat/reply", post(reply_chat))
        .route("/api/v1/chat/relay", post(relay_chat))
        .route("/api/v1/chat/ws", get(chat_ws))
        .route("/api/v1/streaming/ws", get(streaming_ws))
        .route(
            "/api/v1/recordings",
            get(list_product_recordings).post(start_product_recording),
        )
        .route("/api/v1/recordings/{id}/stop", post(stop_product_recording))
        .route("/api/v1/recordings/{id}", delete(delete_product_recording))
        .route("/api/v1/analytics/overview", get(analytics_overview))
        .route("/api/v1/analytics/timeseries", get(analytics_timeseries))
        .route("/api/v1/storage/files", get(list_files).post(upload_file))
        .route("/api/v1/storage/metadata", post(create_storage_metadata))
        .route(
            "/api/v1/storage/files/{id}",
            get(get_file).patch(update_file).delete(delete_file),
        )
        .route("/api/v1/storage/files/{id}/download", get(download_file))
        .route(
            "/api/v1/storage/files/{id}/download-url",
            post(file_download_url),
        )
        .route("/api/v1/clips/projects", get(list_clips).post(create_clip))
        .route(
            "/api/v1/clips/projects/{id}",
            get(get_clip).delete(delete_clip),
        )
        .route("/api/v1/clips/projects/{id}/download", get(download_clip))
        .route(
            "/api/v1/studio/sessions",
            get(list_studio_sessions).post(create_studio_session),
        )
        .route(
            "/api/v1/studio/sessions/{id}",
            get(get_studio_session).patch(update_studio_session),
        )
        .route(
            "/api/v1/studio/sessions/{id}/start",
            post(start_studio_session),
        )
        .route("/api/v1/studio/sessions/{id}/end", post(end_studio_session))
        .route("/api/v1/studio/sessions/{id}/guests", post(add_guest))
        .route(
            "/api/v1/studio/sessions/{id}/guests/{guest_id}",
            delete(remove_guest),
        )
        .route("/api/v1/studio/sessions/{id}/scenes", post(add_scene))
        .route(
            "/api/v1/studio/sessions/{id}/scenes/{scene_id}",
            patch(update_scene),
        )
        .route("/api/v1/studio/brands", get(list_brands).post(create_brand))
        .route(
            "/api/v1/studio/brands/{id}",
            patch(update_brand).delete(delete_brand),
        )
        .route(
            "/api/v1/studio/captions",
            get(list_captions).post(create_caption),
        )
        .route(
            "/api/v1/studio/captions/{id}",
            patch(update_caption).delete(delete_caption),
        )
        .route(
            "/api/v1/studio/qr-codes",
            get(list_qr_codes).post(create_qr_code),
        )
        .route(
            "/api/v1/studio/qr-codes/{id}",
            patch(update_qr_code).delete(delete_qr_code),
        )
        .route("/api/v1/studio/qr-codes/reorder", patch(reorder_qr_codes))
        .route(
            "/api/v1/studio/tickers",
            get(list_tickers).post(create_ticker),
        )
        .route(
            "/api/v1/studio/tickers/{id}",
            patch(update_ticker).delete(delete_ticker),
        )
        .route("/api/v1/studio/tickers/reorder", patch(reorder_tickers))
        .route("/api/v1/studio/fonts", get(list_fonts))
        .route("/api/v1/studio/audio/countdown", get(list_countdown_audio))
        .route(
            "/api/v1/studio/audio/backgrounds",
            get(list_background_audio),
        )
        .route("/api/v1/webhooks", get(list_webhooks).post(create_webhook))
        .route(
            "/api/v1/webhooks/{id}",
            patch(update_webhook).delete(delete_webhook),
        )
        .route("/api/v1/webhooks/{id}/test", post(test_webhook))
        .layer(middleware::from_fn_with_state(state, require_auth))
}

/// Build all v1 routes.  Public platform metadata and authentication routes
/// stay outside the auth middleware; every product mutation and private read
/// is protected when `RESTREAM_AUTH_REQUIRED=true` (or an admin password is
/// configured).
pub fn routes(state: AppState) -> Router<AppState> {
    let public = Router::new()
        .route("/api/v1/health", get(health))
        .route("/api/v1/status", get(dashboard_status))
        .route("/api/v1/openapi.json", get(openapi))
        .route("/api/v1/setup/status", get(setup_status))
        .route("/api/v1/setup", post(save_setup))
        .route("/api/v1/platforms", get(list_platform_catalog))
        .route("/api/v1/ingest-servers", get(list_ingest_servers))
        .route("/api/v1/servers", get(list_ingest_servers))
        .route("/api/v1/auth/login", post(login))
        .route("/api/v1/auth/refresh", post(refresh))
        .route("/api/v1/auth/logout", post(logout));
    public.merge(protected_routes(state))
}

pub(crate) async fn require_auth(
    State(state): State<AppState>,
    request: Request<Body>,
    next: Next,
) -> Response {
    if !state.restream.auth_required() {
        return next.run(request).await;
    }
    let token = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .or_else(|| websocket_access_token(request.headers()))
        .or_else(|| query_access_token(request.uri().query()))
        .unwrap_or_default();
    if state.restream.validate_access_token(token).await {
        next.run(request).await
    } else {
        error(
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "valid Bearer token required",
        )
    }
}

fn websocket_access_token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get("sec-websocket-protocol")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| {
            value
                .split(',')
                .find_map(|protocol| protocol.trim().strip_prefix("reestream-bearer-"))
        })
}

fn query_access_token(query: Option<&str>) -> Option<&str> {
    query?.split('&').find_map(|part| {
        let (key, value) = part.split_once('=')?;
        (key == "access_token").then_some(value)
    })
}

async fn health() -> Response {
    ok(json!({"status": "ok", "version": env!("CARGO_PKG_VERSION"), "timestamp": now()}))
}

async fn dashboard_status(State(state): State<AppState>) -> Response {
    let streams = state.stream_manager.get_streams().await;
    let total_viewers: u32 = streams.iter().map(|stream| stream.viewers).sum();
    ok(json!({
        "version": env!("CARGO_PKG_VERSION"),
        "uptimeSeconds": state.start_time.elapsed().as_secs(),
        "activeStreams": streams.len(),
        "totalViewers": total_viewers,
        "authRequired": state.restream.auth_required(),
    }))
}

async fn setup_status(State(state): State<AppState>) -> Response {
    let status = reestream_core::setup::get_setup_status(&state.config_path);
    ok(json!({
        "firstRun": status.first_run,
        "configExists": status.config_exists,
        "hasStreamKey": status.has_stream_key,
        "platformCount": status.platform_count,
    }))
}

async fn save_setup(
    State(state): State<AppState>,
    Json(request): Json<SetupSaveRequest>,
) -> Response {
    if !reestream_core::setup::get_setup_status(&state.config_path).first_run {
        return error(
            StatusCode::CONFLICT,
            "setup_already_completed",
            "initial setup is already complete; authenticate before changing configuration",
        );
    }
    if request.stream_key.trim().is_empty() {
        return bad_request("streamKey is required");
    }
    for platform in &request.platforms {
        if platform.key.trim().is_empty() {
            return bad_request("platform keys are required");
        }
        if let Err(response) = parse_destination_url(&platform.url, "platform url") {
            return response;
        }
    }
    let request = reestream_core::setup::SetupRequest {
        rtmp_addr: request.rtmp_addr,
        rtmp_port: request.rtmp_port,
        stream_key: request.stream_key,
        platforms: request
            .platforms
            .into_iter()
            .map(|platform| reestream_core::setup::SetupPlatform {
                name: platform.name,
                url: platform.url,
                key: platform.key,
                orientation: platform.orientation,
            })
            .collect(),
    };

    match reestream_core::setup::apply_setup(&state.config_path, &request) {
        Ok(config) => ok(json!({
            "rtmpAddr": config.rtmp_addr,
            "rtmpPort": config.rtmp_port,
            "platformCount": config.platform.as_ref().map_or(0, Vec::len),
        })),
        Err(error) => bad_request(format!("setup failed: {error}")),
    }
}

async fn openapi() -> Response {
    ok(openapi_document())
}

async fn list_platform_catalog() -> Response {
    ok(platform_catalog())
}

async fn list_ingest_servers(State(state): State<AppState>) -> Response {
    ok(ingest_servers_for_url(
        &state.restream.runtime_ingest_url().await,
    ))
}

async fn login(State(state): State<AppState>, Json(request): Json<LoginRequest>) -> Response {
    match state
        .restream
        .login(&request.email, &request.password)
        .await
    {
        Ok(tokens) => ok(tokens),
        Err(message) => error(StatusCode::UNAUTHORIZED, "invalid_credentials", message),
    }
}

async fn refresh(State(state): State<AppState>, Json(request): Json<RefreshRequest>) -> Response {
    match state.restream.refresh_session(&request.refresh_token).await {
        Ok(tokens) => ok(tokens),
        Err(message) => error(StatusCode::UNAUTHORIZED, "invalid_refresh_token", message),
    }
}

async fn logout(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Some(token) = bearer(&headers) {
        state.restream.revoke_session(token).await;
    }
    ok(json!({"loggedOut": true}))
}

fn bearer(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
}

async fn me(State(state): State<AppState>) -> Response {
    ok(state.restream.profile().await)
}

async fn selected_ingest(State(state): State<AppState>) -> Response {
    let stream_id = state.restream.runtime_stream_key().await;
    ok(json!({
        "ingestId": "local",
        "serverUrl": state.restream.runtime_ingest_url().await,
        "backupServerUrl": configured_rtmps_url(),
        "srtUrl": srt_ingest_url(&stream_id),
        "protocol": "rtmp"
    }))
}

async fn global_stream_key(State(state): State<AppState>) -> Response {
    let stream_key = state.restream.runtime_stream_key().await;
    if !stream_key.is_empty() {
        let srt_url = srt_ingest_url(&stream_key);
        ok(json!({"streamKey": stream_key, "srtUrl": srt_url}))
    } else {
        error(
            StatusCode::NOT_FOUND,
            "stream_key_not_configured",
            "global stream key is not configured",
        )
    }
}

async fn reset_global_stream_key(State(state): State<AppState>) -> Response {
    let stream_key = match reestream_core::setup::reset_stream_key(&state.config_path) {
        Ok(stream_key) => stream_key,
        Err(reset_error) => {
            return error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "config_unavailable",
                reset_error.to_string(),
            );
        }
    };
    state
        .restream
        .set_runtime_stream_key(stream_key.clone())
        .await;
    let srt_url = srt_ingest_url(&stream_key);
    ok(json!({"streamKey": stream_key, "srtUrl": srt_url}))
}

async fn chat_url() -> Response {
    ok(json!({"webchatUrl": "/api/v1/chat/ws"}))
}

fn oauth_env_key(platform: &str, suffix: &str) -> String {
    format!(
        "RESTREAM_OAUTH_{}_{}",
        platform.replace('-', "_").to_ascii_uppercase(),
        suffix
    )
}

fn oauth_setting(platform: &str, suffix: &str) -> Option<String> {
    std::env::var(oauth_env_key(platform, suffix))
        .ok()
        .filter(|value| !value.trim().is_empty())
}

async fn list_connections(State(state): State<AppState>) -> Response {
    ok(state.restream.list_connections().await)
}

async fn delete_connection(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    if state.restream.delete_connection(&id).await {
        ok(json!({"deleted": true, "id": id}))
    } else {
        not_found("connection")
    }
}

async fn oauth_authorize(
    State(state): State<AppState>,
    Path(platform): Path<String>,
    Query(query): Query<OAuthAuthorizeQuery>,
    headers: HeaderMap,
) -> Response {
    let Some(client_id) = oauth_setting(&platform, "CLIENT_ID") else {
        return error(
            StatusCode::NOT_IMPLEMENTED,
            "oauth_not_configured",
            format!("OAuth client is not configured for platform {platform}"),
        );
    };
    let Some(authorize_url) = oauth_setting(&platform, "AUTHORIZE_URL") else {
        return error(
            StatusCode::NOT_IMPLEMENTED,
            "oauth_not_configured",
            format!("OAuth authorize URL is not configured for platform {platform}"),
        );
    };
    if query.state.trim().is_empty() {
        return bad_request("state is required for OAuth CSRF protection");
    }
    let Ok(mut url) = url::Url::parse(&authorize_url) else {
        return error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "oauth_configuration_invalid",
            "OAuth authorize URL is invalid",
        );
    };
    if let Err(response) = parse_url(&query.redirect_uri, "redirectUri") {
        return response;
    }
    state
        .restream
        .save_oauth_state(
            query.state.clone(),
            platform.clone(),
            query.redirect_uri.clone(),
            bearer(&headers).unwrap_or_default().to_string(),
        )
        .await;
    let scope = query
        .scope
        .or_else(|| oauth_setting(&platform, "SCOPES"))
        .unwrap_or_default();
    url.query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", &client_id)
        .append_pair("redirect_uri", &query.redirect_uri)
        .append_pair("state", &query.state);
    if !scope.is_empty() {
        url.query_pairs_mut().append_pair("scope", &scope);
    }
    ok(json!({
        "platformId": platform,
        "authorizationUrl": url.to_string(),
        "state": query.state
    }))
}

async fn oauth_token(
    State(state): State<AppState>,
    Path(platform): Path<String>,
    headers: HeaderMap,
    Json(request): Json<OAuthTokenRequest>,
) -> Response {
    let Some(client_id) = oauth_setting(&platform, "CLIENT_ID") else {
        return error(
            StatusCode::NOT_IMPLEMENTED,
            "oauth_not_configured",
            format!("OAuth client is not configured for platform {platform}"),
        );
    };
    let Some(client_secret) = oauth_setting(&platform, "CLIENT_SECRET") else {
        return error(
            StatusCode::NOT_IMPLEMENTED,
            "oauth_not_configured",
            format!("OAuth client secret is not configured for platform {platform}"),
        );
    };
    let Some(token_url) = oauth_setting(&platform, "TOKEN_URL") else {
        return error(
            StatusCode::NOT_IMPLEMENTED,
            "oauth_not_configured",
            format!("OAuth token URL is not configured for platform {platform}"),
        );
    };
    let grant_type = request.grant_type.unwrap_or_else(|| {
        if request.refresh_token.is_some() {
            "refresh_token".into()
        } else {
            "authorization_code".into()
        }
    });
    if grant_type == "authorization_code" && request.code.is_none() {
        return bad_request("code is required for authorization_code exchange");
    }
    if grant_type == "refresh_token" && request.refresh_token.is_none() {
        return bad_request("refreshToken is required for refresh_token exchange");
    }
    if grant_type == "authorization_code" {
        let Some(state_token) = request.state.as_deref() else {
            return bad_request("state is required for OAuth code exchange");
        };
        let Some(redirect_uri) = request.redirect_uri.as_deref() else {
            return bad_request("redirectUri is required for OAuth code exchange");
        };
        if !state
            .restream
            .consume_oauth_state(
                state_token,
                &platform,
                redirect_uri,
                bearer(&headers).unwrap_or_default(),
            )
            .await
        {
            return error(
                StatusCode::UNAUTHORIZED,
                "oauth_state_invalid",
                "OAuth state is invalid or expired",
            );
        }
    }
    let mut form: Vec<(String, String)> = vec![("grant_type".into(), grant_type.clone())];
    if let Some(code) = request.code {
        form.push(("code".into(), code));
    }
    if let Some(refresh_token) = request.refresh_token {
        form.push(("refresh_token".into(), refresh_token));
    }
    if let Some(redirect_uri) = request.redirect_uri {
        if let Err(response) = parse_url(&redirect_uri, "redirectUri") {
            return response;
        }
        form.push(("redirect_uri".into(), redirect_uri));
    }
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::none())
        .build()
    {
        Ok(client) => client,
        Err(client_error) => {
            return error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "oauth_client",
                client_error.to_string(),
            );
        }
    };
    let response = match client
        .post(&token_url)
        .basic_auth(&client_id, Some(&client_secret))
        .form(&form)
        .send()
        .await
    {
        Ok(response) => response,
        Err(request_error) => {
            return error(
                StatusCode::BAD_GATEWAY,
                "oauth_request_failed",
                request_error.to_string(),
            );
        }
    };
    let status = response.status();
    let payload = match response.json::<Value>().await {
        Ok(payload) => payload,
        Err(parse_error) => {
            return error(
                StatusCode::BAD_GATEWAY,
                "oauth_invalid_response",
                parse_error.to_string(),
            );
        }
    };
    if !status.is_success() {
        return error(
            StatusCode::BAD_GATEWAY,
            "oauth_exchange_failed",
            payload["error_description"]
                .as_str()
                .or_else(|| payload["error"].as_str())
                .unwrap_or("provider rejected the OAuth exchange"),
        );
    }
    let Some(access_token) = payload["access_token"].as_str() else {
        return error(
            StatusCode::BAD_GATEWAY,
            "oauth_invalid_response",
            "provider response did not contain access_token",
        );
    };
    let expires_in = payload["expires_in"].as_u64();
    let scopes = payload["scope"]
        .as_str()
        .map(|scope| scope.split_whitespace().map(ToOwned::to_owned).collect())
        .or_else(|| {
            oauth_setting(&platform, "SCOPES")
                .map(|scope| scope.split_whitespace().map(ToOwned::to_owned).collect())
        })
        .unwrap_or_default();
    let connection = state
        .restream
        .save_oauth_connection(
            platform.clone(),
            access_token.to_string(),
            payload["refresh_token"].as_str().map(ToOwned::to_owned),
            payload["token_type"].as_str().unwrap_or("Bearer").into(),
            expires_in.map(|expires_in| now() + expires_in),
            scopes,
        )
        .await;
    ok(json!({
        "connection": connection,
        "tokenType": payload["token_type"].as_str().unwrap_or("Bearer"),
        "expiresIn": expires_in
    }))
}

async fn update_me(State(state): State<AppState>, Json(request): Json<ProfilePatch>) -> Response {
    ok(state
        .restream
        .update_profile(request.display_name, request.timezone, request.avatar_url)
        .await)
}

async fn list_channels(State(state): State<AppState>, query: Query<ListQuery>) -> Response {
    list(state.restream.list_channels().await, &query)
}

fn public_channel(channel: Channel) -> Value {
    json!({
        "id": channel.id,
        "platformId": channel.platform_id,
        "displayName": channel.display_name,
        "channelUrl": channel.channel_url,
        "streamUrl": channel.stream_url,
        "enabled": channel.enabled,
        "status": channel.status,
        "createdAt": channel.created_at,
        "updatedAt": channel.updated_at,
        "lastError": channel.last_error,
    })
}

fn public_file(file: StorageFile) -> Value {
    json!({
        "id": file.id.clone(),
        "name": file.name,
        "mimeType": file.mime_type,
        "sizeBytes": file.size_bytes,
        "durationSeconds": file.duration_seconds,
        "status": file.status,
        "labels": file.labels,
        "createdAt": file.created_at,
        "updatedAt": file.updated_at,
        "downloadUrl": format!("/api/v1/storage/files/{}/download", file.id),
    })
}

async fn create_channel(
    State(state): State<AppState>,
    Json(request): Json<CreateChannelRequest>,
) -> Response {
    let platform_id = match required(&request.platform_id, "platformId") {
        Ok(value) => value,
        Err(response) => return response,
    };
    let stream_url = match required(&request.stream_url, "streamUrl") {
        Ok(value) => value,
        Err(response) => return response,
    };
    if let Err(response) = parse_destination_url(&stream_url, "streamUrl") {
        return response;
    }
    let stream_key = match required(&request.stream_key, "streamKey") {
        Ok(value) => value,
        Err(response) => return response,
    };
    let channel = state
        .restream
        .create_channel(
            platform_id,
            request.display_name.unwrap_or_else(|| "New channel".into()),
            request.channel_url,
            stream_url,
            stream_key,
            request.rtmp_username,
            request.rtmp_password,
        )
        .await;
    created(public_channel(channel))
}

async fn get_channel(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    match state.restream.get_channel(&id).await {
        Some(channel) => ok(public_channel(channel)),
        None => not_found("channel"),
    }
}

async fn channel_credentials(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    match state.restream.get_channel(&id).await {
        Some(channel) => ok(json!({
            "id": channel.id,
            "streamUrl": channel.stream_url,
            "streamKey": channel.stream_key,
            "rtmpUsername": channel.rtmp_username,
            "rtmpPassword": channel.rtmp_password,
        })),
        None => not_found("channel"),
    }
}

async fn update_channel(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<UpdateChannelRequest>,
) -> Response {
    if let Some(ref value) = request.stream_url
        && let Err(response) = parse_destination_url(value, "streamUrl")
    {
        return response;
    }
    if request
        .stream_key
        .as_deref()
        .is_some_and(|value| value.trim().is_empty())
    {
        return bad_request("streamKey cannot be empty; omit it to keep the existing key");
    }
    match state
        .restream
        .update_channel(
            &id,
            request.display_name,
            request.channel_url,
            request.stream_url,
            request.stream_key,
            request.enabled,
        )
        .await
    {
        Some(channel) => ok(public_channel(channel)),
        None => not_found("channel"),
    }
}

async fn delete_channel(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    if state.restream.delete_channel(&id).await {
        ok(json!({"deleted": true, "id": id}))
    } else {
        not_found("channel")
    }
}

async fn list_drafts(State(state): State<AppState>, query: Query<ListQuery>) -> Response {
    list(state.restream.list_drafts().await, &query)
}

async fn create_draft(
    State(state): State<AppState>,
    Json(request): Json<CreateDraftRequest>,
) -> Response {
    let name = match required(&request.name, "name") {
        Ok(value) => value,
        Err(response) => return response,
    };
    let stream_type = match parse_stream_type(request.stream_type.as_deref()) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let destination_ids = request.destination_ids.unwrap_or_default();
    if let Err(response) = validate_destination_ids(&state.restream, &destination_ids).await {
        return response;
    }
    if let Some(brand_id) = request.brand_id.as_deref()
        && !state
            .restream
            .list_brands()
            .await
            .iter()
            .any(|brand| brand.id == brand_id)
    {
        return not_found("brand");
    }
    let draft = state
        .restream
        .create_draft(
            name,
            stream_type,
            request.title.unwrap_or_default(),
            request.description.unwrap_or_default(),
            destination_ids,
            request.brand_id,
        )
        .await;
    created(draft)
}

async fn get_draft(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    match state.restream.get_draft(&id).await {
        Some(draft) => ok(draft),
        None => not_found("stream draft"),
    }
}

async fn update_draft(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<UpdateDraftRequest>,
) -> Response {
    if let Some(destination_ids) = request.destination_ids.as_ref()
        && let Err(response) = validate_destination_ids(&state.restream, destination_ids).await
    {
        return response;
    }
    if let Some(brand_id) = request.brand_id.as_deref()
        && !state
            .restream
            .list_brands()
            .await
            .iter()
            .any(|brand| brand.id == brand_id)
    {
        return not_found("brand");
    }
    match state
        .restream
        .update_draft(
            &id,
            request.name,
            request.title,
            request.description,
            request.destination_ids,
            request.brand_id,
        )
        .await
    {
        Some(draft) => ok(draft),
        None => not_found("stream draft"),
    }
}

async fn delete_draft(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    if state.restream.delete_draft(&id).await {
        ok(json!({"deleted": true, "id": id}))
    } else {
        not_found("stream draft")
    }
}

async fn duplicate_draft(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    match state.restream.get_draft(&id).await {
        Some(draft) => created(
            state
                .restream
                .create_draft(
                    format!("Copy of {}", draft.name),
                    draft.stream_type,
                    draft.title,
                    draft.description,
                    draft.destination_ids,
                    draft.brand_id,
                )
                .await,
        ),
        None => not_found("stream draft"),
    }
}

async fn list_events(State(state): State<AppState>, query: Query<ListQuery>) -> Response {
    list(
        state
            .restream
            .list_events(parse_event_status(query.status.as_deref()))
            .await,
        &query,
    )
}

async fn upcoming_events(State(state): State<AppState>, query: Query<ListQuery>) -> Response {
    list(
        state
            .restream
            .list_events(Some(EventStatus::Scheduled))
            .await,
        &query,
    )
}

async fn live_events(State(state): State<AppState>, query: Query<ListQuery>) -> Response {
    list(
        state.restream.list_events(Some(EventStatus::Live)).await,
        &query,
    )
}

async fn history_events(State(state): State<AppState>, query: Query<ListQuery>) -> Response {
    list(
        state.restream.list_events(Some(EventStatus::Ended)).await,
        &query,
    )
}

async fn create_event(
    State(state): State<AppState>,
    Json(request): Json<CreateEventRequest>,
) -> Response {
    let stream_type = match parse_stream_type(request.stream_type.as_deref()) {
        Ok(value) => value,
        Err(response) => return response,
    };
    if request.loops_count.unwrap_or(0) > 9 {
        return bad_request("loopsCount must be between 0 and 9");
    }
    if let Some(scheduled_for) = request.scheduled_for.as_deref()
        && !is_valid_scheduled_for(scheduled_for)
    {
        return bad_request("scheduledFor must be a Unix timestamp or RFC3339 value");
    }
    if matches!(stream_type, StreamType::File | StreamType::Playlist) && request.file_id.is_none() {
        return bad_request("fileId is required for file and playlist events");
    }
    if let Some(file_id) = request.file_id.as_deref() {
        let Some(file) = state.restream.get_file(file_id).await else {
            return not_found("storage file");
        };
        if !state.restream.is_managed_storage_path(&file.path) {
            return bad_request("source file must be inside the storage root");
        }
    }
    if let Some(draft_id) = request.draft_id.as_deref()
        && state.restream.get_draft(draft_id).await.is_none()
    {
        return not_found("stream draft");
    }
    let destination_ids = request.destination_ids.unwrap_or_default();
    if let Err(response) = validate_destination_ids(&state.restream, &destination_ids).await {
        return response;
    }
    let event = state
        .restream
        .create_event(
            request.draft_id,
            stream_type,
            request.title.unwrap_or_default(),
            request.description.unwrap_or_default(),
            request.scheduled_for,
            destination_ids,
            request.file_id,
            request.loops_count.unwrap_or(0),
        )
        .await;
    created(event)
}

async fn get_event(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    match state.restream.get_event(&id).await {
        Some(event) => ok(event),
        None => not_found("event"),
    }
}

async fn update_event(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<UpdateEventRequest>,
) -> Response {
    if let Some(scheduled_for) = request
        .scheduled_for
        .as_ref()
        .and_then(|value| value.as_deref())
        && !is_valid_scheduled_for(scheduled_for)
    {
        return bad_request("scheduledFor must be a Unix timestamp or RFC3339 value");
    }
    if let Some(destination_ids) = request.destination_ids.as_ref()
        && let Err(response) = validate_destination_ids(&state.restream, destination_ids).await
    {
        return response;
    }
    match state
        .restream
        .update_event(
            &id,
            request.title,
            request.description,
            request.scheduled_for,
            request.destination_ids,
        )
        .await
    {
        Some(event) => ok(event),
        None => not_found("event"),
    }
}

async fn delete_event(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    if state.restream.delete_event(&id).await {
        ok(json!({"deleted": true, "id": id}))
    } else {
        not_found("event")
    }
}

async fn add_event_destination(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<AddDestinationRequest>,
) -> Response {
    match state
        .restream
        .add_event_destination(&id, &request.channel_id)
        .await
    {
        Some(event) => ok(event),
        None => not_found("event or channel"),
    }
}

async fn remove_event_destination(
    State(state): State<AppState>,
    Path((id, destination_id)): Path<(String, String)>,
) -> Response {
    match state
        .restream
        .remove_event_destination(&id, &destination_id)
        .await
    {
        Some(event) => ok(event),
        None => not_found("event"),
    }
}

async fn event_stream_key(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    match state.restream.event_credentials(&id).await {
        Some(credentials) => ok(json!({
            "serverUrl": credentials.server_url,
            "streamKey": credentials.stream_key,
            "backupServerUrl": credentials.backup_server_url.or_else(configured_rtmps_url),
            "protocol": credentials.protocol,
        })),
        None => not_found("event"),
    }
}

async fn event_srt_keys(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    let Some(event) = state.restream.get_event(&id).await else {
        return not_found("event");
    };
    let stream_id = if event.ingest.stream_key.is_empty() {
        id
    } else {
        event.ingest.stream_key
    };
    let passphrase = if srt_enabled() {
        std::env::var("RESTREAM_SRT_PASSPHRASE")
            .ok()
            .filter(|value| !value.is_empty())
    } else {
        None
    };
    let primary = srt_ingest_url(&stream_id).map(|url| {
        json!({
            "url": url,
            "passphrase": passphrase,
        })
    });
    ok(json!({
        "primary": primary,
        "backup": null
    }))
}

fn configured_rtmps_url() -> Option<String> {
    std::env::var("RESTREAM_RTMPS_URL")
        .ok()
        .filter(|value| value.starts_with("rtmps://"))
}

fn srt_enabled() -> bool {
    std::env::var("RESTREAM_SRT_ENABLED")
        .map(|value| matches!(value.as_str(), "1" | "true" | "yes"))
        .unwrap_or(false)
        && std::env::var("RESTREAM_SRT_PASSPHRASE")
            .ok()
            .is_some_and(|value| !value.trim().is_empty())
}

fn srt_ingest_url(stream_id: &str) -> Option<String> {
    if !srt_enabled() {
        return None;
    }
    let port = std::env::var("RESTREAM_SRT_PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(3000);
    let mut url =
        url::Url::parse(&format!("srt://localhost:{port}")).expect("static SRT URL is valid");
    url.query_pairs_mut().append_pair("streamid", stream_id);
    Some(url.to_string())
}

pub async fn start_event_recording(
    recording_manager: &crate::recording::RecordingManager,
    store: &RestreamStore,
    event: &crate::restream::Event,
) -> Option<StorageFile> {
    if !recording_manager.is_enabled() || store.recording_session(&event.id).await.is_some() {
        return None;
    }
    let input_url = std::env::var("RESTREAM_RECORDING_INPUT_URL")
        .unwrap_or_else(|_| {
            let port = std::env::var("RESTREAM_HTTP_PORT")
                .ok()
                .and_then(|value| value.parse::<u16>().ok())
                .unwrap_or(8080);
            format!("http://127.0.0.1:{port}/stream.flv")
        });
    let recording_id = recording_manager
        .start_recording(&format!("event-{}", event.id), &input_url)
        .await
        .ok()?;
    let recording = recording_manager.get_recording(&recording_id).await?;
    let mime_type = match recording.format {
        crate::recording::RecordingFormat::Mp4 => "video/mp4",
        crate::recording::RecordingFormat::Flv => "video/x-flv",
        crate::recording::RecordingFormat::Mkv => "video/x-matroska",
        crate::recording::RecordingFormat::Ts => "video/mp2t",
    };
    let file = store
        .create_file_metadata_with_status(
            recording.filename.clone(),
            mime_type.into(),
            recording.size_bytes,
            None,
            vec!["recording".into(), event.id.clone()],
            recording.path.to_string_lossy().into_owned(),
            "recording".into(),
        )
        .await;
    store.link_event_recording(&event.id, &file.id).await?;
    store.set_recording_session(&event.id, &recording_id).await;
    Some(file)
}

pub async fn finish_event_recording(
    recording_manager: &crate::recording::RecordingManager,
    store: &RestreamStore,
    event: &crate::restream::Event,
) {
    let Some(recording_id) = store.take_recording_session(&event.id).await else {
        return;
    };
    let _ = recording_manager.stop_recording(&recording_id).await;
    let Some(recording) = recording_manager.get_recording(&recording_id).await else {
        return;
    };
    let Some(file_id) = event.recording_file_id.as_deref() else {
        return;
    };
    let status = if recording.path.exists()
        && !matches!(recording.status, crate::recording::RecordingStatus::Error)
    {
        "ready"
    } else {
        "failed"
    };
    let _ = store
        .update_file_state(file_id, recording.size_bytes, status.into())
        .await;
}

async fn start_event_recording_route(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    let Some(event) = state.restream.get_event(&id).await else {
        return not_found("event");
    };
    if event.status != EventStatus::Live {
        return error(
            StatusCode::CONFLICT,
            "event_not_live",
            "recording can only start for a live event",
        );
    }
    if state.restream.recording_session(&id).await.is_some() {
        return error(
            StatusCode::CONFLICT,
            "recording_already_active",
            "event recording is already active",
        );
    }

    match start_event_recording(&state.recording_manager, &state.restream, &event).await {
        Some(file) => {
            let recording_id = state.restream.recording_session(&id).await;
            created(json!({
                "recordingId": recording_id,
                "file": public_file(file),
            }))
        }
        None => error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "recording_failed",
            "recording is disabled or ffmpeg could not start",
        ),
    }
}

async fn stop_event_recording_route(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    let Some(event) = state.restream.get_event(&id).await else {
        return not_found("event");
    };
    if state.restream.recording_session(&id).await.is_none() {
        return not_found("active recording");
    }
    finish_event_recording(&state.recording_manager, &state.restream, &event).await;
    ok(json!({"eventId": id, "status": "completed"}))
}

async fn validate_event_playback(
    store: &RestreamStore,
    event: &crate::restream::Event,
) -> Result<(), String> {
    if !matches!(event.stream_type, StreamType::File | StreamType::Playlist) {
        return Ok(());
    }
    let source_file_id = event
        .source_file_id
        .as_deref()
        .ok_or_else(|| "file event has no source file".to_string())?;
    let source_file = store
        .get_file(source_file_id)
        .await
        .ok_or_else(|| "source storage file not found".to_string())?;
    if !store.is_managed_storage_path(&source_file.path) {
        return Err("source file is outside the local storage root".into());
    }
    Ok(())
}

pub async fn start_event_playback(
    playback_manager: &crate::playback::PlaybackManager,
    store: &RestreamStore,
    event: &crate::restream::Event,
) -> Result<(), String> {
    if !matches!(event.stream_type, StreamType::File | StreamType::Playlist) {
        return Ok(());
    }
    validate_event_playback(store, event).await?;
    let source_file_id = event
        .source_file_id
        .as_deref()
        .expect("validated source file");
    let source_file = store
        .get_file(source_file_id)
        .await
        .ok_or_else(|| "source storage file not found".to_string())?;
    let output_url = format!(
        "{}/{}",
        event.ingest.server_url.trim_end_matches('/'),
        event.ingest.stream_key
    );
    playback_manager
        .start(
            &event.id,
            std::path::Path::new(&source_file.path),
            &output_url,
            event.loops_count,
        )
        .await
}

pub async fn finish_event_playback(
    playback_manager: &crate::playback::PlaybackManager,
    event_id: &str,
) {
    let _ = playback_manager.stop(event_id).await;
}

async fn go_live(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    let Some(current_event) = state.restream.get_event(&id).await else {
        return not_found("event");
    };
    if let Err(message) = validate_event_playback(&state.restream, &current_event).await {
        return error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "playback_unavailable",
            message,
        );
    }
    let Some(event) = state.restream.set_event_live(&id).await else {
        return error(
            StatusCode::CONFLICT,
            "event_not_startable",
            "ended or cancelled events cannot go live",
        );
    };

    let _ = start_event_recording(&state.recording_manager, &state.restream, &event).await;
    if let Err(message) =
        start_event_playback(&state.playback_manager, &state.restream, &event).await
    {
        let current = state.restream.get_event(&id).await.unwrap_or(event.clone());
        finish_event_recording(&state.recording_manager, &state.restream, &current).await;
        let _ = state.restream.cancel_event(&id).await;
        return error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "playback_unavailable",
            message,
        );
    }

    ok(state.restream.get_event(&id).await.unwrap_or(event))
}

async fn end_event(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    let Some(current_event) = state.restream.get_event(&id).await else {
        return not_found("event");
    };
    if current_event.status != EventStatus::Live {
        return error(
            StatusCode::CONFLICT,
            "event_not_live",
            "only live events can be ended",
        );
    }
    let Some(event) = state.restream.end_event(&id).await else {
        return not_found("event");
    };
    finish_event_recording(&state.recording_manager, &state.restream, &event).await;
    finish_event_playback(&state.playback_manager, &id).await;
    ok(state.restream.get_event(&id).await.unwrap_or(event))
}

async fn reconcile_event_recording(state: &AppState, event: &crate::restream::Event) {
    let Some(recording_id) = state.restream.recording_session(&event.id).await else {
        return;
    };
    let Some(recording) = state.recording_manager.get_recording(&recording_id).await else {
        return;
    };
    let Some(file_id) = event.recording_file_id.as_deref() else {
        return;
    };
    let status = if matches!(
        recording.status,
        crate::recording::RecordingStatus::Recording
    ) {
        "recording"
    } else if matches!(recording.status, crate::recording::RecordingStatus::Error) {
        "failed"
    } else if recording.path.exists() {
        "ready"
    } else {
        "failed"
    };
    let _ = state
        .restream
        .update_file_state(file_id, recording.size_bytes, status.into())
        .await;
}

async fn event_recordings(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    let Some(event) = state.restream.get_event(&id).await else {
        return not_found("event");
    };
    reconcile_event_recording(&state, &event).await;
    let event = state.restream.get_event(&id).await.unwrap_or(event);
    let files: Vec<StorageFile> = match event.recording_file_id {
        Some(file_id) => state
            .restream
            .get_file(&file_id)
            .await
            .into_iter()
            .collect(),
        None => Vec::new(),
    };
    let primary_videos = files
        .iter()
        .map(|file| {
            json!({
                "fileId": file.id,
                "fileName": file.name,
                "expiresAt": Value::Null,
                "downloadUrl": format!("/api/v1/storage/files/{}/download", file.id)
            })
        })
        .collect::<Vec<_>>();
    let active = match state.restream.recording_session(&id).await {
        Some(recording_id) => state
            .recording_manager
            .get_recording(&recording_id)
            .await
            .map(public_recording),
        None => None,
    };
    ok(json!({
        "active": active,
        "primaryVideos": primary_videos,
        "secondaryVideos": [],
        "audio": [],
        "files": files.into_iter().map(public_file).collect::<Vec<_>>()
    }))
}

async fn recording_download_url(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<RecordingDownloadRequest>,
) -> Response {
    let Some(event) = state.restream.get_event(&id).await else {
        return not_found("event");
    };
    let Some(file_id) = event.recording_file_id else {
        return not_found("recording");
    };
    let Some(file) = state.restream.get_file(&file_id).await else {
        return not_found("recording");
    };
    if file.name != request.file_name {
        return not_found("recording");
    }
    ok(json!({
        "downloadUrl": format!("/api/v1/storage/files/{}/download", file.id),
        "expiresIn": 3600
    }))
}

async fn event_chat(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    if state.restream.get_event(&id).await.is_none() {
        return not_found("event");
    }
    ok(state.restream.list_chat(Some(&id)).await)
}

async fn event_analytics(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    match state.restream.analytics(&id).await {
        Some(report) => ok(report),
        None => not_found("event"),
    }
}

async fn event_transcriptions(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    if state.restream.get_event(&id).await.is_none() {
        return not_found("event");
    }
    let mut transcriptions = state
        .restream
        .list_transcriptions(&id)
        .await
        .unwrap_or_default();
    if transcriptions.is_empty() {
        let _ = state.restream.ensure_transcription(&id).await;
        transcriptions = state
            .restream
            .list_transcriptions(&id)
            .await
            .unwrap_or_default();
    }
    ok(json!({"transcriptions": transcriptions}))
}

async fn request_transcription(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    if state.restream.get_event(&id).await.is_none() {
        return not_found("event");
    }
    match state.restream.ensure_transcription(&id).await {
        Some(transcription) => created(transcription),
        None => not_found("recording"),
    }
}

async fn chat_history_download_url(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    if state.restream.get_event(&id).await.is_none() {
        return not_found("event");
    }
    ok(json!({
        "downloadUrl": format!("/api/v1/events/{id}/chat-export"),
        "expiresIn": 3600
    }))
}

async fn event_chat_export(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    if state.restream.get_event(&id).await.is_none() {
        return not_found("event");
    }
    let messages = state.restream.list_chat(Some(&id)).await;
    let mut csv = String::from("id,eventId,destinationId,authorName,message,createdAt\n");
    for message in messages {
        csv.push_str(&format!(
            "{},{},{},{},{},{}\n",
            message.id,
            message.event_id,
            message.destination_id.unwrap_or_default(),
            csv_escape(&message.author_name),
            csv_escape(&message.message),
            message.created_at
        ));
    }
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, HeaderValue::from_static("text/csv"))],
        csv,
    )
        .into_response()
}

fn csv_escape(value: &str) -> String {
    if value.contains([',', '"', '\n']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.into()
    }
}

async fn record_viewers(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<ViewerSampleRequest>,
) -> Response {
    if state
        .restream
        .record_viewers(&id, request.viewers, request.bitrate_kbps.unwrap_or(0))
        .await
    {
        ok(json!({"recorded": true, "timestamp": now()}))
    } else {
        not_found("event")
    }
}

async fn list_viewers(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    match state.restream.analytics(&id).await {
        Some(report) => ok(report.timeseries),
        None => not_found("event"),
    }
}

async fn event_viewer_analytics(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    let Some(report) = state.restream.analytics(&id).await else {
        return not_found("event");
    };
    let peak_time = report
        .timeseries
        .iter()
        .max_by_key(|sample| sample.viewers)
        .map(|sample| sample.timestamp);
    let by_channel = report
        .destinations
        .iter()
        .map(|destination| {
            json!({
                "channelId": destination.destination_id,
                "mean": report.average_concurrent_viewers,
                "max": destination.peak_viewers.max(report.peak_concurrent_viewers),
                "viewsTotal": report.views,
                "peakTime": peak_time,
                "watchedTime": report.duration_seconds.saturating_mul(report.average_concurrent_viewers as u64),
                "viewersPerMinute": report.timeseries.iter().map(|sample| json!({"timestamp": sample.timestamp, "viewers": sample.viewers})).collect::<Vec<_>>()
            })
        })
        .collect::<Vec<_>>();
    ok(json!({
        "total": {
            "mean": report.average_concurrent_viewers,
            "max": report.peak_concurrent_viewers,
            "viewsTotal": report.views,
            "peakTime": peak_time,
            "watchedTime": report.duration_seconds.saturating_mul(report.average_concurrent_viewers as u64),
            "viewersPerMinute": report.timeseries.iter().map(|sample| json!({"timestamp": sample.timestamp, "viewers": sample.viewers})).collect::<Vec<_>>()
        },
        "byChannel": by_channel
    }))
}

async fn event_message_analytics(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    if state.restream.get_event(&id).await.is_none() {
        return not_found("event");
    }
    let messages = state.restream.list_chat(Some(&id)).await;
    let total = messages.len() as u64;
    let chatters = messages
        .iter()
        .map(|message| message.author_id.as_deref().unwrap_or(&message.author_name))
        .collect::<std::collections::HashSet<_>>()
        .len() as u64;
    let mut by_minute = std::collections::BTreeMap::<u64, u64>::new();
    for message in &messages {
        *by_minute.entry(message.created_at / 60 * 60).or_default() += 1;
    }
    let messages_per_minute = by_minute
        .into_iter()
        .map(|(timestamp, count)| json!({"timestamp": timestamp, "messages": count}))
        .collect::<Vec<_>>();
    let by_channel = messages
        .iter()
        .fold(
            std::collections::BTreeMap::<String, (u64, std::collections::HashSet<String>)>::new(),
            |mut counts, message| {
            let channel = message
                .destination_id
                .clone()
                .unwrap_or_else(|| "unified".into());
            let entry = counts.entry(channel).or_default();
            entry.0 += 1;
            entry.1.insert(
                message
                    .author_id
                    .clone()
                    .unwrap_or_else(|| message.author_name.clone()),
            );
            counts
        },
        )
        .into_iter()
        .map(|(channel_id, (messages_total, chatters))| {
            json!({"channelId": channel_id, "messagesTotal": messages_total, "chattersTotal": chatters.len(), "messagesPerMinute": messages_per_minute})
        })
        .collect::<Vec<_>>();
    ok(json!({
        "total": {"messagesTotal": total, "chattersTotal": chatters, "messagesPerMinute": messages_per_minute},
        "byChannel": by_channel
    }))
}

async fn list_chat(State(state): State<AppState>, query: Query<ListQuery>) -> Response {
    list(
        state.restream.list_chat(query.event_id.as_deref()).await,
        &query,
    )
}

async fn chat_sources(State(state): State<AppState>) -> Response {
    ok(state
        .restream
        .list_channels()
        .await
        .into_iter()
        .map(|channel| {
            json!({
                "id": channel.id,
                "platformId": channel.platform_id,
                "displayName": channel.display_name,
                "status": channel.status,
                "enabled": channel.enabled
            })
        })
        .collect::<Vec<_>>())
}

async fn chat_actions() -> Response {
    ok(json!([
        {"id": "reply", "method": "POST", "path": "/api/v1/chat/reply"},
        {"id": "relay", "method": "POST", "path": "/api/v1/chat/relay"},
        {"id": "delete", "method": "DELETE", "path": "/api/v1/chat/messages/{id}"}
    ]))
}

async fn chat_connections(State(state): State<AppState>) -> Response {
    ok(state
        .restream
        .list_channels()
        .await
        .into_iter()
        .map(|channel| {
            json!({
                "id": channel.id,
                "platformId": channel.platform_id,
                "status": channel.status,
                "connected": channel.enabled
            })
        })
        .collect::<Vec<_>>())
}

async fn chat_events(State(state): State<AppState>, query: Query<ListQuery>) -> Response {
    list(state.restream.list_events(None).await, &query)
}

async fn send_chat(State(state): State<AppState>, Json(request): Json<ChatRequest>) -> Response {
    let message = match required(&request.message, "message") {
        Ok(value) => value,
        Err(response) => return response,
    };
    match state
        .restream
        .add_chat_message(
            request.event_id,
            request.destination_id,
            request.author_name.unwrap_or_else(|| "Host".into()),
            request.author_id,
            message,
            "message".into(),
            request.reply_to,
        )
        .await
    {
        Ok(message) => created(message),
        Err(message) => not_found(&message),
    }
}

async fn reply_chat(State(state): State<AppState>, Json(request): Json<ChatRequest>) -> Response {
    send_chat(State(state), Json(request)).await
}

async fn relay_chat(State(state): State<AppState>, Json(request): Json<ChatRequest>) -> Response {
    let event_id = request.event_id.clone();
    let message = send_chat(State(state.clone()), Json(request))
        .await
        .into_response()
        .into_body();
    // The normal send operation is the source of truth.  A relay fan-out is
    // represented by one broadcast message with destinationId omitted.
    let _ = event_id;
    message.into_response()
}

async fn delete_chat(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    match state.restream.delete_chat_message(&id).await {
        Some(message) => ok(message),
        None => not_found("chat message"),
    }
}

#[derive(Debug, Deserialize)]
struct WsQuery {
    event_id: Option<String>,
}

async fn chat_ws(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Query(query): Query<WsQuery>,
) -> Response {
    ws.on_upgrade(move |socket| {
        handle_chat_ws(socket, state.restream.as_ref().clone(), query.event_id)
    })
    .into_response()
}

async fn handle_chat_ws(mut socket: WebSocket, store: RestreamStore, event_id: Option<String>) {
    let initial = store.list_chat(event_id.as_deref()).await;
    let _ = socket
        .send(Message::Text(
            serde_json::to_string(&json!({"type": "init", "messages": initial}))
                .unwrap_or_default()
                .into(),
        ))
        .await;
    let mut receiver = store.subscribe_chat();
    while let Ok(message) = receiver.recv().await {
        if event_id
            .as_deref()
            .is_some_and(|event_id| event_id != message.event_id)
        {
            continue;
        }
        let payload = json!({"type": "message", "message": message});
        if socket
            .send(Message::Text(
                serde_json::to_string(&payload).unwrap_or_default().into(),
            ))
            .await
            .is_err()
        {
            break;
        }
    }
}

async fn streaming_ws(ws: WebSocketUpgrade, State(state): State<AppState>) -> Response {
    ws.on_upgrade(move |socket| handle_streaming_ws(socket, state.restream.as_ref().clone()))
        .into_response()
}

async fn handle_streaming_ws(mut socket: WebSocket, store: RestreamStore) {
    let events = store.list_events(None).await;
    let _ = socket
        .send(Message::Text(
            serde_json::to_string(&json!({"type": "init", "events": events}))
                .unwrap_or_default()
                .into(),
        ))
        .await;
    let mut receiver = store.subscribe_events();
    while let Ok(notification) = receiver.recv().await {
        let payload = json!({"type": "event", "notification": notification});
        if socket
            .send(Message::Text(
                serde_json::to_string(&payload).unwrap_or_default().into(),
            ))
            .await
            .is_err()
        {
            break;
        }
    }
}

async fn analytics_overview(
    State(state): State<AppState>,
    Query(query): Query<ListQuery>,
) -> Response {
    ok(state
        .restream
        .analytics_overview(query.from, query.to)
        .await)
}

async fn analytics_timeseries(
    State(state): State<AppState>,
    Query(query): Query<ListQuery>,
) -> Response {
    if let Some(event_id) = query.event_id {
        match state.restream.analytics(&event_id).await {
            Some(report) => ok(report.timeseries),
            None => not_found("event"),
        }
    } else {
        bad_request("eventId is required")
    }
}

fn public_recording(recording: crate::recording::RecordingInfo) -> Value {
    let status = match recording.status {
        crate::recording::RecordingStatus::Recording => "recording",
        crate::recording::RecordingStatus::Stopped => "completed",
        crate::recording::RecordingStatus::Error => "failed",
    };
    json!({
        "id": recording.id,
        "streamId": recording.stream_id,
        "filename": recording.filename,
        "format": format!("{:?}", recording.format).to_ascii_lowercase(),
        "startedAt": recording.started_at,
        "sizeBytes": recording.size_bytes,
        "status": status,
    })
}

async fn list_product_recordings(State(state): State<AppState>) -> Response {
    let recordings = state
        .recording_manager
        .list_recordings()
        .await
        .into_iter()
        .map(public_recording)
        .collect::<Vec<_>>();
    ok(recordings)
}

async fn start_product_recording(
    State(state): State<AppState>,
    Json(request): Json<StartRecordingRequest>,
) -> Response {
    if let Err(response) = validate_media_input_url(&request.input_url, "inputUrl") {
        return response;
    }
    match state
        .recording_manager
        .start_recording(&request.stream_id, &request.input_url)
        .await
    {
        Ok(id) => created(json!({ "id": id })),
        Err(message) => error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "recording_failed",
            message,
        ),
    }
}

async fn stop_product_recording(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    match state.recording_manager.stop_recording(&id).await {
        Ok(()) => ok(json!({ "id": id, "status": "completed" })),
        Err(message) => not_found(&message),
    }
}

async fn delete_product_recording(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    match state.recording_manager.delete_recording(&id).await {
        Ok(()) => ok(json!({ "deleted": true, "id": id })),
        Err(message) => not_found(&message),
    }
}

async fn list_files(State(state): State<AppState>, query: Query<ListQuery>) -> Response {
    list(
        state
            .restream
            .list_files(query.q.as_deref())
            .await
            .into_iter()
            .map(public_file)
            .collect(),
        &query,
    )
}

async fn upload_file(State(state): State<AppState>, mut multipart: Multipart) -> Response {
    let mut name = None;
    let mut mime_type = "application/octet-stream".to_string();
    let mut labels = Vec::new();
    let upload_id = Uuid::new_v4().to_string();
    let temporary_path = state.restream.storage_path(&upload_id, "upload.part");
    let mut total_bytes = 0u64;
    let mut received_file = false;
    loop {
        let Some(mut field) = (match multipart.next_field().await {
            Ok(field) => field,
            Err(error) => {
                let _ = tokio::fs::remove_file(&temporary_path).await;
                return bad_request(format!("unable to read multipart upload: {error}"));
            }
        }) else {
            break;
        };
        let field_name = field.name().unwrap_or_default().to_string();
        if field_name == "file" {
            if received_file {
                return bad_request("only one file field is supported");
            }
            received_file = true;
            if let Some(content_type) = field.content_type() {
                mime_type = content_type.to_string();
            }
            if name.is_none() {
                name = field.file_name().map(safe_download_filename);
            }
            if let Some(parent) = temporary_path.parent()
                && let Err(io_error) = tokio::fs::create_dir_all(parent).await
            {
                return error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "storage_error",
                    io_error.to_string(),
                );
            }
            let mut file = match tokio::fs::File::create(&temporary_path).await {
                Ok(file) => file,
                Err(io_error) => {
                    return error(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "storage_error",
                        io_error.to_string(),
                    );
                }
            };
            loop {
                match field.chunk().await {
                    Ok(Some(chunk)) => {
                        total_bytes = total_bytes.saturating_add(chunk.len() as u64);
                        if total_bytes > max_upload_bytes() {
                            let _ = tokio::fs::remove_file(&temporary_path).await;
                            return error(
                                StatusCode::PAYLOAD_TOO_LARGE,
                                "upload_too_large",
                                format!(
                                    "file exceeds the {} byte upload limit",
                                    max_upload_bytes()
                                ),
                            );
                        }
                        if let Err(io_error) = file.write_all(&chunk).await {
                            let _ = tokio::fs::remove_file(&temporary_path).await;
                            return error(
                                StatusCode::INTERNAL_SERVER_ERROR,
                                "storage_error",
                                io_error.to_string(),
                            );
                        }
                    }
                    Ok(None) => break,
                    Err(error) => {
                        let _ = tokio::fs::remove_file(&temporary_path).await;
                        return bad_request(format!("unable to read upload: {error}"));
                    }
                }
            }
        } else if field_name == "name" {
            match multipart_text(&mut field, 255).await {
                Ok(value) => name = Some(value),
                Err(error) => return bad_request(format!("invalid name: {error}")),
            }
        } else if field_name == "labels"
            && let Ok(value) = multipart_text(&mut field, 4096).await
        {
            labels = value
                .split(',')
                .map(str::trim)
                .filter(|label| !label.is_empty())
                .map(ToOwned::to_owned)
                .collect();
        }
    }
    let name = match name.filter(|value| !value.trim().is_empty()) {
        Some(value) => value,
        None => {
            let _ = tokio::fs::remove_file(&temporary_path).await;
            return bad_request("multipart field `file` is required");
        }
    };
    if !received_file {
        return bad_request("multipart field `file` is required");
    }
    let path = state.restream.storage_path(&upload_id, &name);
    if let Err(io_error) = tokio::fs::rename(&temporary_path, &path).await {
        let _ = tokio::fs::remove_file(&temporary_path).await;
        return error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "storage_error",
            io_error.to_string(),
        );
    }
    let file = state
        .restream
        .create_file_metadata(
            name,
            mime_type,
            total_bytes,
            None,
            labels,
            path.to_string_lossy().into(),
        )
        .await;
    created(public_file(file))
}

async fn create_storage_metadata(
    State(state): State<AppState>,
    Json(request): Json<CreateStorageMetadataRequest>,
) -> Response {
    let name = match required(&request.name, "name") {
        Ok(value) => value,
        Err(response) => return response,
    };
    if request.size_bytes.unwrap_or(0) > max_upload_bytes() {
        return error(
            StatusCode::PAYLOAD_TOO_LARGE,
            "upload_too_large",
            format!("file exceeds the {} byte upload limit", max_upload_bytes()),
        );
    }
    let path = request.path.unwrap_or_default();
    if !path.is_empty() && !state.restream.is_managed_storage_path(&path) {
        return bad_request("path must point to an existing file inside the storage root");
    }
    let file = state
        .restream
        .create_file_metadata(
            name,
            request
                .mime_type
                .unwrap_or_else(|| "application/octet-stream".into()),
            request.size_bytes.unwrap_or(0),
            request.duration_seconds,
            request.labels.unwrap_or_default(),
            path,
        )
        .await;
    created(public_file(file))
}

async fn get_file(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    match state.restream.get_file(&id).await {
        Some(file) => ok(public_file(file)),
        None => not_found("storage file"),
    }
}

async fn update_file(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<UpdateStorageRequest>,
) -> Response {
    match state
        .restream
        .update_file(&id, request.name, request.labels)
        .await
    {
        Some(file) => ok(public_file(file)),
        None => not_found("storage file"),
    }
}

async fn delete_file(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    if state.restream.delete_file(&id).await.is_some() {
        ok(json!({"deleted": true, "id": id}))
    } else {
        not_found("storage file")
    }
}

async fn download_file(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    let Some(file) = state.restream.get_file(&id).await else {
        return not_found("storage file");
    };
    if !state.restream.is_managed_storage_path(&file.path) {
        return error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "file_not_downloadable",
            "file contents are not available in the local storage root",
        );
    }
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(&file.mime_type)
            .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream")),
    );
    if let Ok(value) = HeaderValue::from_str(&format!(
        "attachment; filename=\"{}\"",
        safe_download_filename(&file.name)
    )) {
        headers.insert(header::CONTENT_DISPOSITION, value);
    }
    let path = file.path.clone();
    let stream = async_stream::stream! {
        let mut input = match tokio::fs::File::open(path).await {
            Ok(input) => input,
            Err(error) => {
                yield Err::<bytes::Bytes, std::io::Error>(error);
                return;
            }
        };
        let mut buffer = vec![0u8; 64 * 1024];
        loop {
            let read = match input.read(&mut buffer).await {
                Ok(read) => read,
                Err(error) => {
                    yield Err::<bytes::Bytes, std::io::Error>(error);
                    return;
                }
            };
            if read == 0 {
                break;
            }
            yield Ok::<bytes::Bytes, std::io::Error>(bytes::Bytes::copy_from_slice(&buffer[..read]));
        }
    };
    (StatusCode::OK, headers, Body::from_stream(stream)).into_response()
}

async fn file_download_url(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    if state.restream.get_file(&id).await.is_none() {
        return not_found("storage file");
    }
    let download_url = format!("/api/v1/storage/files/{id}/download");
    ok(
        json!({"url": download_url, "downloadUrl": format!("/api/v1/storage/files/{id}/download"), "expiresIn": 3600}),
    )
}

async fn list_clips(State(state): State<AppState>, query: Query<ClipListQuery>) -> Response {
    ok(state.restream.list_clips(query.event_id.as_deref()).await)
}

async fn create_clip(
    State(state): State<AppState>,
    Json(request): Json<CreateClipRequest>,
) -> Response {
    if request.end_seconds <= request.start_seconds {
        return bad_request("endSeconds must be greater than startSeconds");
    }
    match state
        .restream
        .create_clip(
            request.event_id,
            request.name.unwrap_or_else(|| "Untitled clip".into()),
            request.start_seconds,
            request.end_seconds,
        )
        .await
    {
        Some(clip) => created(clip),
        None => not_found("event"),
    }
}

async fn get_clip(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    match state.restream.get_clip(&id).await {
        Some(clip) => ok(clip),
        None => not_found("clip project"),
    }
}

async fn delete_clip(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    if state.restream.delete_clip(&id).await {
        ok(json!({"deleted": true, "id": id}))
    } else {
        not_found("clip project")
    }
}

async fn download_clip(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    let Some(clip) = state.restream.get_clip(&id).await else {
        return not_found("clip project");
    };
    if let Some(file_id) = clip.output_file_id {
        return download_file(State(state), Path(file_id)).await;
    }
    if clip.status == "processing" {
        return error(
            StatusCode::CONFLICT,
            "clip_processing",
            "clip is still being generated",
        );
    }
    error(
        StatusCode::UNPROCESSABLE_ENTITY,
        "clip_failed",
        "clip has no output file",
    )
}

async fn list_studio_sessions(State(state): State<AppState>) -> Response {
    ok(state.restream.list_studio_sessions().await)
}

async fn create_studio_session(
    State(state): State<AppState>,
    Json(request): Json<StudioSessionRequest>,
) -> Response {
    match state
        .restream
        .get_or_create_studio_session(&request.event_id)
        .await
    {
        Some(session) => {
            if request.layout.is_some() || request.settings.is_some() {
                let _ = state
                    .restream
                    .update_studio_session(&session.id, request.layout, request.settings, None)
                    .await;
            }
            match state
                .restream
                .get_or_create_studio_session(&request.event_id)
                .await
            {
                Some(session) => created(session),
                None => not_found("event"),
            }
        }
        None => not_found("event"),
    }
}

async fn get_studio_session(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    match state
        .restream
        .list_studio_sessions()
        .await
        .into_iter()
        .find(|session| session.id == id)
    {
        Some(session) => ok(session),
        None => not_found("Studio session"),
    }
}

async fn update_studio_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<StudioSessionPatch>,
) -> Response {
    match state
        .restream
        .update_studio_session(&id, request.layout, request.settings, request.status)
        .await
    {
        Some(session) => ok(session),
        None => not_found("Studio session"),
    }
}

async fn start_studio_session(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    let Some(session) = state
        .restream
        .list_studio_sessions()
        .await
        .into_iter()
        .find(|session| session.id == id)
    else {
        return not_found("Studio session");
    };
    let Some(event) = state.restream.get_event(&session.event_id).await else {
        return not_found("event");
    };
    let event = match state.restream.set_event_live(&event.id).await {
        Some(event) => event,
        None => {
            return error(
                StatusCode::CONFLICT,
                "event_not_startable",
                "event cannot go live",
            );
        }
    };
    let _ = start_event_recording(&state.recording_manager, &state.restream, &event).await;
    if let Err(message) =
        start_event_playback(&state.playback_manager, &state.restream, &event).await
    {
        let _ = state.restream.cancel_event(&event.id).await;
        return error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "playback_unavailable",
            message,
        );
    }
    match state
        .restream
        .update_studio_session(&id, None, None, Some("live".into()))
        .await
    {
        Some(session) => ok(session),
        None => not_found("Studio session"),
    }
}

async fn end_studio_session(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    let session = state
        .restream
        .list_studio_sessions()
        .await
        .into_iter()
        .find(|session| session.id == id);
    if let Some(session) = session
        && let Some(event) = state.restream.end_event(&session.event_id).await
    {
        finish_event_recording(&state.recording_manager, &state.restream, &event).await;
        finish_event_playback(&state.playback_manager, &event.id).await;
    }
    match state
        .restream
        .update_studio_session(&id, None, None, Some("ended".into()))
        .await
    {
        Some(session) => ok(session),
        None => not_found("Studio session"),
    }
}

async fn add_guest(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<GuestRequest>,
) -> Response {
    match state
        .restream
        .add_guest(
            &id,
            request.name,
            request.role.unwrap_or_else(|| "guest".into()),
        )
        .await
    {
        Some(guest) => created(guest),
        None => not_found("Studio session"),
    }
}

async fn remove_guest(
    State(state): State<AppState>,
    Path((id, guest_id)): Path<(String, String)>,
) -> Response {
    if state.restream.remove_guest(&id, &guest_id).await {
        ok(json!({"deleted": true, "id": guest_id}))
    } else {
        not_found("guest")
    }
}

async fn add_scene(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<SceneRequest>,
) -> Response {
    match state
        .restream
        .add_scene(
            &id,
            request.name,
            request.layout.unwrap_or_else(|| "grid".into()),
            request.source_ids.unwrap_or_default(),
        )
        .await
    {
        Some(scene) => created(scene),
        None => not_found("Studio session"),
    }
}

async fn update_scene(
    State(state): State<AppState>,
    Path((id, scene_id)): Path<(String, String)>,
    Json(request): Json<ScenePatch>,
) -> Response {
    match state
        .restream
        .update_scene(
            &id,
            &scene_id,
            request.name,
            request.layout,
            request.source_ids,
            request.active,
        )
        .await
    {
        Some(scene) => ok(scene),
        None => not_found("scene"),
    }
}

fn brand_from_value(value: Value) -> Brand {
    let timestamp = now();
    Brand {
        id: String::new(),
        name: value
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("New brand")
            .into(),
        logo_url: value.get("logoUrl").and_then(Value::as_str).map(Into::into),
        primary_color: value
            .get("primaryColor")
            .and_then(Value::as_str)
            .unwrap_or("#ffffff")
            .into(),
        secondary_color: value
            .get("secondaryColor")
            .and_then(Value::as_str)
            .unwrap_or("#000000")
            .into(),
        font_family: value
            .get("fontFamily")
            .and_then(Value::as_str)
            .unwrap_or("Inter")
            .into(),
        created_at: timestamp,
        updated_at: timestamp,
    }
}

async fn list_brands(State(state): State<AppState>) -> Response {
    ok(state.restream.list_brands().await)
}

async fn create_brand(State(state): State<AppState>, Json(value): Json<Value>) -> Response {
    created(state.restream.create_brand(brand_from_value(value)).await)
}

async fn update_brand(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(value): Json<Value>,
) -> Response {
    match state.restream.update_brand(&id, value).await {
        Some(brand) => ok(brand),
        None => not_found("brand"),
    }
}

async fn delete_brand(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    if state.restream.delete_brand(&id).await {
        ok(json!({"deleted": true, "id": id}))
    } else {
        not_found("brand")
    }
}

fn caption_from_value(value: Value) -> Caption {
    let timestamp = now();
    Caption {
        id: String::new(),
        name: value
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("Captions")
            .into(),
        language: value
            .get("language")
            .and_then(Value::as_str)
            .unwrap_or("en")
            .into(),
        style: value.get("style").cloned().unwrap_or_else(|| json!({})),
        enabled: value
            .get("enabled")
            .and_then(Value::as_bool)
            .unwrap_or(true),
        created_at: timestamp,
        updated_at: timestamp,
    }
}

async fn list_captions(State(state): State<AppState>) -> Response {
    ok(state.restream.list_captions().await)
}

async fn create_caption(State(state): State<AppState>, Json(value): Json<Value>) -> Response {
    created(
        state
            .restream
            .create_caption(caption_from_value(value))
            .await,
    )
}

async fn update_caption(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(value): Json<Value>,
) -> Response {
    match state.restream.update_caption(&id, value).await {
        Some(caption) => ok(caption),
        None => not_found("caption"),
    }
}

async fn delete_caption(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    if state.restream.delete_caption(&id).await {
        ok(json!({"deleted": true, "id": id}))
    } else {
        not_found("caption")
    }
}

fn qr_from_value(value: Value) -> QrCode {
    let timestamp = now();
    QrCode {
        id: String::new(),
        name: value
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("QR code")
            .into(),
        data: value
            .get("data")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .into(),
        foreground: value
            .get("foreground")
            .and_then(Value::as_str)
            .unwrap_or("#000000")
            .into(),
        background: value
            .get("background")
            .and_then(Value::as_str)
            .unwrap_or("#ffffff")
            .into(),
        enabled: value
            .get("enabled")
            .and_then(Value::as_bool)
            .unwrap_or(true),
        position: value
            .get("position")
            .and_then(Value::as_str)
            .unwrap_or("bottom-right")
            .into(),
        created_at: timestamp,
        updated_at: timestamp,
    }
}

async fn list_qr_codes(State(state): State<AppState>) -> Response {
    ok(state.restream.list_qr_codes().await)
}

async fn create_qr_code(State(state): State<AppState>, Json(value): Json<Value>) -> Response {
    created(state.restream.create_qr_code(qr_from_value(value)).await)
}

async fn update_qr_code(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(value): Json<Value>,
) -> Response {
    match state.restream.update_qr_code(&id, value).await {
        Some(qr_code) => ok(qr_code),
        None => not_found("QR code"),
    }
}

async fn delete_qr_code(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    if state.restream.delete_qr_code(&id).await {
        ok(json!({"deleted": true, "id": id}))
    } else {
        not_found("QR code")
    }
}

async fn reorder_qr_codes(
    State(state): State<AppState>,
    Json(request): Json<ReorderRequest>,
) -> Response {
    ok(state.restream.reorder_qr_codes(&request.ids).await)
}

fn ticker_from_value(value: Value) -> Ticker {
    let timestamp = now();
    Ticker {
        id: String::new(),
        text: value
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .into(),
        speed: value.get("speed").and_then(Value::as_u64).unwrap_or(40) as u32,
        color: value
            .get("color")
            .and_then(Value::as_str)
            .unwrap_or("#ffffff")
            .into(),
        background_color: value
            .get("backgroundColor")
            .and_then(Value::as_str)
            .unwrap_or("#000000")
            .into(),
        enabled: value
            .get("enabled")
            .and_then(Value::as_bool)
            .unwrap_or(true),
        order: value.get("order").and_then(Value::as_u64).unwrap_or(0) as u32,
        created_at: timestamp,
        updated_at: timestamp,
    }
}

async fn list_tickers(State(state): State<AppState>) -> Response {
    ok(state.restream.list_tickers().await)
}

async fn create_ticker(State(state): State<AppState>, Json(value): Json<Value>) -> Response {
    created(state.restream.create_ticker(ticker_from_value(value)).await)
}

async fn update_ticker(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(value): Json<Value>,
) -> Response {
    match state.restream.update_ticker(&id, value).await {
        Some(ticker) => ok(ticker),
        None => not_found("ticker"),
    }
}

async fn delete_ticker(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    if state.restream.delete_ticker(&id).await {
        ok(json!({"deleted": true, "id": id}))
    } else {
        not_found("ticker")
    }
}

async fn reorder_tickers(
    State(state): State<AppState>,
    Json(request): Json<ReorderRequest>,
) -> Response {
    ok(state.restream.reorder_tickers(&request.ids).await)
}

async fn list_fonts() -> Response {
    ok(vec!["Inter", "Roboto", "Open Sans", "Montserrat", "Arial"])
}

async fn list_countdown_audio(State(state): State<AppState>) -> Response {
    let files = state
        .restream
        .list_files(None)
        .await
        .into_iter()
        .filter(|file| {
            file.labels
                .iter()
                .any(|label| label.eq_ignore_ascii_case("countdown"))
        })
        .map(public_file)
        .collect::<Vec<_>>();
    ok(files)
}

async fn list_background_audio(State(state): State<AppState>) -> Response {
    let files = state
        .restream
        .list_files(None)
        .await
        .into_iter()
        .filter(|file| {
            file.labels
                .iter()
                .any(|label| label.eq_ignore_ascii_case("background"))
        })
        .map(public_file)
        .collect::<Vec<_>>();
    ok(files)
}

fn webhook_from_request(request: WebhookRequest) -> WebhookSubscription {
    WebhookSubscription {
        id: String::new(),
        url: request.url.unwrap_or_default(),
        secret: request.secret,
        events: request
            .events
            .unwrap_or_else(|| vec!["event.started".into(), "event.ended".into()]),
        enabled: request.enabled.unwrap_or(true),
        created_at: 0,
        updated_at: 0,
    }
}

async fn list_webhooks(State(state): State<AppState>) -> Response {
    let hooks = state.restream.list_webhooks().await;
    ok(hooks
        .into_iter()
        .map(|hook| {
            json!({"id": hook.id, "url": hook.url, "events": hook.events, "enabled": hook.enabled, "createdAt": hook.created_at, "updatedAt": hook.updated_at})
        })
        .collect::<Vec<_>>())
}

async fn create_webhook(
    State(state): State<AppState>,
    Json(request): Json<WebhookRequest>,
) -> Response {
    let Some(url) = request.url.as_deref() else {
        return bad_request("url is required");
    };
    if let Err(response) = parse_http_url(url, "url") {
        return response;
    }
    let hook = state
        .restream
        .create_webhook(webhook_from_request(request))
        .await;
    ok(
        json!({"id": hook.id, "url": hook.url, "events": hook.events, "enabled": hook.enabled, "createdAt": hook.created_at, "updatedAt": hook.updated_at}),
    )
}

async fn update_webhook(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<WebhookRequest>,
) -> Response {
    if let Some(ref url) = request.url
        && let Err(response) = parse_http_url(url, "url")
    {
        return response;
    }
    match state
        .restream
        .update_webhook(
            &id,
            request.url,
            request.secret,
            request.events,
            request.enabled,
        )
        .await
    {
        Some(hook) => ok(
            json!({"id": hook.id, "url": hook.url, "events": hook.events, "enabled": hook.enabled, "createdAt": hook.created_at, "updatedAt": hook.updated_at}),
        ),
        None => not_found("webhook"),
    }
}

async fn delete_webhook(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    if state.restream.delete_webhook(&id).await {
        ok(json!({"deleted": true, "id": id}))
    } else {
        not_found("webhook")
    }
}

async fn test_webhook(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    let hook = state
        .restream
        .list_webhooks()
        .await
        .into_iter()
        .find(|hook| hook.id == id);
    let Some(hook) = hook else {
        return not_found("webhook");
    };
    let payload = json!({"event": "webhook.test", "timestamp": now(), "data": {"healthy": true}});
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .build()
    {
        Ok(client) => client,
        Err(client_error) => {
            return error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "webhook_client",
                client_error.to_string(),
            );
        }
    };
    match client.post(&hook.url).json(&payload).send().await {
        Ok(response) => ok(
            json!({"delivered": response.status().is_success(), "status": response.status().as_u16()}),
        ),
        Err(request_error) => error(
            StatusCode::BAD_GATEWAY,
            "webhook_failed",
            request_error.to_string(),
        ),
    }
}

fn openapi_document() -> Value {
    let paths = [
        "/api/v1/health",
        "/api/v1/status",
        "/api/v1/openapi.json",
        "/api/v1/setup/status",
        "/api/v1/setup",
        "/api/v1/auth/login",
        "/api/v1/auth/refresh",
        "/api/v1/auth/logout",
        "/api/v1/me",
        "/api/v1/profile",
        "/api/v1/user/profile",
        "/api/v1/ingest",
        "/api/v1/user/ingest",
        "/api/v1/stream-key",
        "/api/v1/stream-key/reset",
        "/api/v1/user/streamKey",
        "/api/v1/chat-url",
        "/api/v1/user/webchat/url",
        "/api/v1/connections",
        "/api/v1/connections/{id}",
        "/api/v1/oauth/{platform}/authorize",
        "/api/v1/oauth/{platform}/token",
        "/api/v1/platforms",
        "/api/v1/ingest-servers",
        "/api/v1/servers",
        "/api/v1/channels",
        "/api/v1/channels/{id}",
        "/api/v1/channels/{id}/credentials",
        "/api/v1/streams",
        "/api/v1/streams/{id}",
        "/api/v1/streams/{id}/duplicate",
        "/api/v1/events",
        "/api/v1/events/upcoming",
        "/api/v1/events/live",
        "/api/v1/events/history",
        "/api/v1/events/{id}",
        "/api/v1/events/{id}/destinations",
        "/api/v1/events/{id}/destinations/{destination_id}",
        "/api/v1/events/{id}/stream-key",
        "/api/v1/events/{id}/srt-keys",
        "/api/v1/events/{id}/go-live",
        "/api/v1/events/{id}/end",
        "/api/v1/events/{id}/recordings",
        "/api/v1/events/{id}/recordings/start",
        "/api/v1/events/{id}/recordings/stop",
        "/api/v1/events/{id}/recordings/download-url",
        "/api/v1/events/{id}/recordings/transcriptions",
        "/api/v1/events/{id}/chat",
        "/api/v1/events/{id}/chat/history/download-url",
        "/api/v1/events/{id}/analytics",
        "/api/v1/events/{id}/analytics/viewers",
        "/api/v1/events/{id}/analytics/messages",
        "/api/v1/events/{id}/viewers",
        "/api/v1/events/{id}/transcriptions",
        "/api/v1/events/{id}/chat-export",
        "/api/v1/chat/messages",
        "/api/v1/chat/messages/{id}",
        "/api/v1/chat/sources",
        "/api/v1/chat/actions",
        "/api/v1/chat/connections",
        "/api/v1/chat/events",
        "/api/v1/chat/reply",
        "/api/v1/chat/relay",
        "/api/v1/chat/ws",
        "/api/v1/streaming/ws",
        "/api/v1/recordings",
        "/api/v1/recordings/{id}/stop",
        "/api/v1/recordings/{id}",
        "/api/v1/analytics/overview",
        "/api/v1/analytics/timeseries",
        "/api/v1/storage/files",
        "/api/v1/storage/metadata",
        "/api/v1/storage/files/{id}",
        "/api/v1/storage/files/{id}/download",
        "/api/v1/storage/files/{id}/download-url",
        "/api/v1/clips/projects",
        "/api/v1/clips/projects/{id}",
        "/api/v1/clips/projects/{id}/download",
        "/api/v1/studio/sessions",
        "/api/v1/studio/sessions/{id}",
        "/api/v1/studio/sessions/{id}/start",
        "/api/v1/studio/sessions/{id}/end",
        "/api/v1/studio/sessions/{id}/guests",
        "/api/v1/studio/sessions/{id}/guests/{guest_id}",
        "/api/v1/studio/sessions/{id}/scenes",
        "/api/v1/studio/sessions/{id}/scenes/{scene_id}",
        "/api/v1/studio/brands",
        "/api/v1/studio/brands/{id}",
        "/api/v1/studio/captions",
        "/api/v1/studio/captions/{id}",
        "/api/v1/studio/qr-codes",
        "/api/v1/studio/qr-codes/{id}",
        "/api/v1/studio/qr-codes/reorder",
        "/api/v1/studio/tickers",
        "/api/v1/studio/tickers/{id}",
        "/api/v1/studio/tickers/reorder",
        "/api/v1/studio/fonts",
        "/api/v1/studio/audio/countdown",
        "/api/v1/studio/audio/backgrounds",
        "/api/v1/webhooks",
        "/api/v1/webhooks/{id}",
        "/api/v1/webhooks/{id}/test",
    ];
    let path_map = paths
        .into_iter()
        .map(|path| {
            (
                path.to_string(),
                json!({"description": "See docs/API.md for request and response schemas."}),
            )
        })
        .collect::<serde_json::Map<_, _>>();
    json!({
        "openapi": "3.1.0",
        "info": {"title": "Reestream API", "version": "v1", "description": "Local multistream, Studio, events, chat, analytics, storage, and clips control plane."},
        "servers": [{"url": "/"}],
        "security": [{"bearerAuth": []}],
        "components": {"securitySchemes": {"bearerAuth": {"type": "http", "scheme": "bearer"}}},
        "paths": path_map
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_platform_catalog_is_non_empty() {
        assert!(platform_catalog().len() >= 10);
    }

    #[test]
    fn test_stream_type_parser() {
        assert!(matches!(
            parse_stream_type(Some("studio")),
            Ok(StreamType::Studio)
        ));
        assert!(parse_stream_type(Some("unknown")).is_err());
    }

    #[test]
    fn test_external_url_validation_rejects_local_targets() {
        assert!(parse_http_url("https://example.test/hook", "url").is_ok());
        assert!(parse_http_url("http://127.0.0.1:8080/hook", "url").is_err());
        assert!(is_private_host(
            &url::Url::parse("rtmp://127.0.0.1:1935/live").unwrap()
        ));
        assert!(validate_media_input_url("rtmp://127.0.0.1:1935/live", "inputUrl").is_err());
    }

    #[test]
    fn test_openapi_has_core_paths() {
        let document = openapi_document();
        assert!(document["paths"]["/api/v1/events"].is_object());
        assert!(document["paths"]["/api/v1/oauth/{platform}/token"].is_object());
        assert!(document["paths"]["/api/v1/events/{id}/analytics/messages"].is_object());
        assert_eq!(document["openapi"], "3.1.0");
    }
}
