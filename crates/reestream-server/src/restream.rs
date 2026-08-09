//! Product-domain state used by the versioned REST API.
//!
//! The relay itself is intentionally kept separate from this module.  A
//! relay can be fed by RTMP, SRT, a Studio session, or a stored video, while
//! the web application needs one consistent model for channels, drafts,
//! events, chat, analytics, and assets.  This store is the small local
//! control-plane database for those resources.

use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit},
};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::Sha256;
use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, RwLock, broadcast};
use tracing::warn;
use uuid::Uuid;

use reestream_core::config::{Platform, PlatformEvent, platform_id_from};

pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn scheduled_epoch(value: &str) -> Option<u64> {
    let value = value.trim();
    if let Ok(epoch) = value.parse::<u64>() {
        return Some(epoch);
    }
    let (date, clock) = value.split_once('T').or_else(|| value.split_once(' '))?;
    let mut date_parts = date.split('-');
    let year = date_parts.next()?.parse::<i64>().ok()?;
    let month = date_parts.next()?.parse::<i64>().ok()?;
    let day = date_parts.next()?.parse::<i64>().ok()?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }

    let zone_start = clock.find(['Z', '+', '-']);
    let (clock, zone) = match zone_start {
        Some(index) => (&clock[..index], &clock[index..]),
        None => (clock, "Z"),
    };
    let mut clock_parts = clock.split(':');
    let hour = clock_parts.next()?.parse::<i64>().ok()?;
    let minute = clock_parts.next()?.parse::<i64>().ok()?;
    let second = clock_parts.next()?.split('.').next()?.parse::<i64>().ok()?;
    if hour > 23 || minute > 59 || second > 59 {
        return None;
    }

    let offset_seconds = if zone == "Z" || zone.is_empty() {
        0
    } else {
        let sign = if zone.starts_with('-') { -1 } else { 1 };
        let offset = zone.trim_start_matches(['+', '-']);
        let mut parts = offset.split(':');
        let hours = parts.next()?.parse::<i64>().ok()?;
        let minutes = parts.next().unwrap_or("0").parse::<i64>().ok()?;
        sign * (hours * 3600 + minutes * 60)
    };

    // Days from civil (Gregorian) date, relative to 1970-01-01.
    let adjusted_year = year - i64::from(month <= 2);
    let era = if adjusted_year >= 0 {
        adjusted_year / 400
    } else {
        (adjusted_year - 399) / 400
    };
    let year_of_era = adjusted_year - era * 400;
    let month_prime = month + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * month_prime + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    let days = era * 146_097 + day_of_era - 719_468;
    let timestamp = days * 86_400 + hour * 3_600 + minute * 60 + second - offset_seconds;
    (timestamp >= 0).then_some(timestamp as u64)
}

pub fn is_valid_scheduled_for(value: &str) -> bool {
    scheduled_epoch(value).is_some()
}

fn id() -> String {
    Uuid::new_v4().to_string()
}

fn ffmpeg_command() -> PathBuf {
    std::env::var_os("RESTREAM_FFMPEG_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("ffmpeg"))
}

fn default_true() -> bool {
    true
}

fn default_username() -> String {
    "local-user".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformCatalogEntry {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub url: String,
    pub image: PlatformImages,
    pub capabilities: PlatformCapabilities,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformImages {
    pub png: String,
    pub svg: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformCapabilities {
    pub stream: bool,
    pub oauth: bool,
    pub chat_read: bool,
    pub chat_write: bool,
    pub chat_relay: bool,
    pub scheduling: bool,
    pub analytics: bool,
}

fn platform(id: &str, name: &str, slug: &str, url: &str, chat: bool) -> PlatformCatalogEntry {
    PlatformCatalogEntry {
        id: id.into(),
        name: name.into(),
        slug: slug.into(),
        url: url.into(),
        image: PlatformImages {
            png: format!("/assets/platforms/{slug}.png"),
            svg: format!("/assets/platforms/{slug}.svg"),
        },
        capabilities: PlatformCapabilities {
            stream: true,
            oauth: !slug.starts_with("custom-"),
            chat_read: chat,
            chat_write: chat,
            chat_relay: chat,
            scheduling: !matches!(slug, "custom-rtmp" | "custom-srt"),
            analytics: chat,
        },
    }
}

/// Platform metadata is deliberately local and editable in one place.  A
/// real provider integration can use the same IDs without changing the web
/// contract.
pub fn platform_catalog() -> Vec<PlatformCatalogEntry> {
    vec![
        platform("twitch", "Twitch", "twitch", "https://twitch.tv", true),
        platform("youtube", "YouTube", "youtube", "https://youtube.com", true),
        platform(
            "facebook",
            "Facebook",
            "facebook",
            "https://facebook.com",
            true,
        ),
        platform(
            "linkedin",
            "LinkedIn",
            "linkedin",
            "https://linkedin.com",
            true,
        ),
        platform(
            "instagram",
            "Instagram",
            "instagram",
            "https://instagram.com",
            false,
        ),
        platform("tiktok", "TikTok", "tiktok", "https://tiktok.com", false),
        platform("x", "X", "x", "https://x.com", true),
        platform("kick", "Kick", "kick", "https://kick.com", true),
        platform("rumble", "Rumble", "rumble", "https://rumble.com", true),
        platform("discord", "Discord", "discord", "https://discord.com", true),
        platform("custom-rtmp", "Custom RTMP", "custom-rtmp", "", false),
    ]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IngestServer {
    pub id: String,
    pub name: String,
    pub url: String,
    pub rtmp_url: String,
    pub latitude: f64,
    pub longitude: f64,
    pub recommended: bool,
}

pub fn ingest_servers() -> Vec<IngestServer> {
    ingest_servers_for_url("rtmp://localhost:1935/live")
}

pub fn ingest_servers_for_url(rtmp_url: &str) -> Vec<IngestServer> {
    vec![
        IngestServer {
            id: "autodetect".into(),
            name: "Autodetect".into(),
            url: "localhost".into(),
            rtmp_url: rtmp_url.into(),
            latitude: 0.0,
            longitude: 0.0,
            recommended: true,
        },
        IngestServer {
            id: "local".into(),
            name: "Local Reestream".into(),
            url: "localhost".into(),
            rtmp_url: rtmp_url.into(),
            latitude: 0.0,
            longitude: 0.0,
            recommended: true,
        },
    ]
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum StreamType {
    Studio,
    #[default]
    Encoder,
    File,
    Playlist,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum EventStatus {
    #[default]
    Draft,
    Scheduled,
    Live,
    Ended,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IngestCredentials {
    pub server_url: String,
    #[serde(skip_serializing, default)]
    pub stream_key: String,
    pub backup_server_url: Option<String>,
    pub protocol: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Channel {
    pub id: String,
    pub platform_id: String,
    pub display_name: String,
    pub channel_url: Option<String>,
    pub stream_url: String,
    #[serde(skip_serializing, default)]
    pub stream_key: String,
    #[serde(skip_serializing, default)]
    pub rtmp_username: Option<String>,
    #[serde(skip_serializing, default)]
    pub rtmp_password: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub status: String,
    pub created_at: u64,
    pub updated_at: u64,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Draft {
    pub id: String,
    pub name: String,
    pub stream_type: StreamType,
    pub title: String,
    pub description: String,
    pub destination_ids: Vec<String>,
    pub brand_id: Option<String>,
    pub studio_session_id: Option<String>,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    pub id: String,
    pub draft_id: Option<String>,
    pub stream_type: StreamType,
    pub title: String,
    pub description: String,
    pub status: EventStatus,
    pub scheduled_for: Option<String>,
    pub created_at: u64,
    pub updated_at: u64,
    pub started_at: Option<u64>,
    pub ended_at: Option<u64>,
    pub duration_seconds: Option<u64>,
    pub destination_ids: Vec<String>,
    pub source_file_id: Option<String>,
    pub loops_count: u8,
    pub guest_link: Option<String>,
    pub ingest: IngestCredentials,
    pub recording_file_id: Option<String>,
    pub current_viewers: u32,
    pub peak_viewers: u32,
    pub chat_message_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageFile {
    pub id: String,
    pub name: String,
    pub mime_type: String,
    pub size_bytes: u64,
    pub duration_seconds: Option<u64>,
    pub status: String,
    pub path: String,
    pub labels: Vec<String>,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Transcription {
    pub id: String,
    pub event_id: String,
    pub file_id: String,
    pub file_name: String,
    pub status: String,
    pub language: Option<String>,
    pub download_url: Option<String>,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessage {
    pub id: String,
    pub event_id: String,
    pub destination_id: Option<String>,
    pub author_name: String,
    pub author_id: Option<String>,
    pub message: String,
    pub kind: String,
    pub reply_to: Option<String>,
    pub created_at: u64,
    pub deleted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewerSample {
    pub event_id: String,
    pub timestamp: u64,
    pub viewers: u32,
    pub bitrate_kbps: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalyticsReport {
    pub event_id: String,
    pub title: String,
    pub status: EventStatus,
    pub views: u64,
    pub peak_concurrent_viewers: u32,
    pub average_concurrent_viewers: u32,
    pub chat_messages: u64,
    pub duration_seconds: u64,
    pub destinations: Vec<DestinationAnalytics>,
    pub timeseries: Vec<ViewerSample>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DestinationAnalytics {
    pub destination_id: String,
    pub views: u64,
    pub peak_viewers: u32,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalyticsOverview {
    pub from: Option<String>,
    pub to: Option<String>,
    pub total_views: u64,
    pub total_streams: u64,
    pub total_chat_messages: u64,
    pub average_duration_seconds: u64,
    pub streams: Vec<AnalyticsReport>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipProject {
    pub id: String,
    pub event_id: String,
    pub source_file_id: Option<String>,
    pub name: String,
    pub start_seconds: u64,
    pub end_seconds: u64,
    pub status: String,
    pub output_file_id: Option<String>,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Guest {
    pub id: String,
    pub name: String,
    pub role: String,
    pub join_url: String,
    pub status: String,
    pub created_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Scene {
    pub id: String,
    pub name: String,
    pub layout: String,
    pub source_ids: Vec<String>,
    pub active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StudioSession {
    pub id: String,
    pub event_id: String,
    pub status: String,
    pub layout: String,
    pub settings: Value,
    pub guest_link: String,
    pub guests: Vec<Guest>,
    pub scenes: Vec<Scene>,
    pub active_scene_id: Option<String>,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Brand {
    pub id: String,
    pub name: String,
    pub logo_url: Option<String>,
    pub primary_color: String,
    pub secondary_color: String,
    pub font_family: String,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Caption {
    pub id: String,
    pub name: String,
    pub language: String,
    pub style: Value,
    pub enabled: bool,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QrCode {
    pub id: String,
    pub name: String,
    pub data: String,
    pub foreground: String,
    pub background: String,
    pub enabled: bool,
    pub position: String,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Ticker {
    pub id: String,
    pub text: String,
    pub speed: u32,
    pub color: String,
    pub background_color: String,
    pub enabled: bool,
    pub order: u32,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebhookSubscription {
    pub id: String,
    pub url: String,
    #[serde(skip_serializing)]
    pub secret: Option<String>,
    pub events: Vec<String>,
    pub enabled: bool,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub id: String,
    #[serde(default = "default_username")]
    pub username: String,
    pub email: String,
    pub display_name: String,
    pub timezone: String,
    pub avatar_url: Option<String>,
    pub created_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AuthSession {
    access_token: String,
    refresh_token: String,
    expires_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct LoginThrottle {
    failures: u8,
    blocked_until: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct ChannelSecret {
    stream_key: String,
    rtmp_username: Option<String>,
    rtmp_password: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthTokens {
    pub access_token: String,
    pub refresh_token: String,
    pub token_type: String,
    pub expires_in: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionSummary {
    pub id: String,
    pub platform_id: String,
    pub status: String,
    pub scopes: Vec<String>,
    pub connected_at: u64,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OAuthConnection {
    id: String,
    platform_id: String,
    access_token: String,
    refresh_token: Option<String>,
    token_type: String,
    expires_at: Option<u64>,
    scopes: Vec<String>,
    connected_at: u64,
    updated_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventNotification {
    pub event: String,
    pub event_id: String,
    pub payload: Value,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
struct RestreamData {
    channels: Vec<Channel>,
    #[serde(skip)]
    channel_secrets: HashMap<String, ChannelSecret>,
    drafts: Vec<Draft>,
    events: Vec<Event>,
    #[serde(skip)]
    event_stream_keys: HashMap<String, String>,
    files: Vec<StorageFile>,
    #[serde(default)]
    transcriptions: Vec<Transcription>,
    #[serde(skip)]
    oauth_connections: Vec<OAuthConnection>,
    chat: Vec<ChatMessage>,
    viewer_samples: Vec<ViewerSample>,
    clips: Vec<ClipProject>,
    studio_sessions: Vec<StudioSession>,
    brands: Vec<Brand>,
    captions: Vec<Caption>,
    qr_codes: Vec<QrCode>,
    tickers: Vec<Ticker>,
    webhooks: Vec<WebhookSubscription>,
    profile: Profile,
    #[serde(skip)]
    sessions: Vec<AuthSession>,
}

impl Default for RestreamData {
    fn default() -> Self {
        let timestamp = now();
        Self {
            channels: Vec::new(),
            channel_secrets: HashMap::new(),
            drafts: Vec::new(),
            events: Vec::new(),
            event_stream_keys: HashMap::new(),
            files: Vec::new(),
            transcriptions: Vec::new(),
            oauth_connections: Vec::new(),
            chat: Vec::new(),
            viewer_samples: Vec::new(),
            clips: Vec::new(),
            studio_sessions: Vec::new(),
            brands: Vec::new(),
            captions: Vec::new(),
            qr_codes: Vec::new(),
            tickers: Vec::new(),
            webhooks: Vec::new(),
            profile: Profile {
                id: "local-user".into(),
                username: "local-user".into(),
                email: std::env::var("RESTREAM_ADMIN_EMAIL")
                    .unwrap_or_else(|_| "admin@localhost".into()),
                display_name: "Reestream user".into(),
                timezone: "UTC".into(),
                avatar_url: None,
                created_at: timestamp,
            },
            sessions: Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
struct AuthConfig {
    required: bool,
    email: String,
    password: Option<String>,
}

impl AuthConfig {
    fn from_env() -> Self {
        let explicit_required = std::env::var("RESTREAM_AUTH_REQUIRED")
            .map(|value| matches!(value.as_str(), "1" | "true" | "yes"))
            .unwrap_or(false);
        let password = std::env::var("RESTREAM_ADMIN_PASSWORD")
            .ok()
            .filter(|value| !value.trim().is_empty());
        let required = explicit_required || password.is_some();
        Self {
            required,
            email: std::env::var("RESTREAM_ADMIN_EMAIL")
                .unwrap_or_else(|_| "admin@localhost".into()),
            password,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RuntimeIngestConfig {
    pub listen_addr: String,
    pub listen_port: u16,
    pub advertised_host: String,
    pub stream_key: String,
}

type OAuthState = (String, String, u64, String);

impl Default for RuntimeIngestConfig {
    fn default() -> Self {
        Self {
            listen_addr: "0.0.0.0".into(),
            listen_port: 1935,
            advertised_host: std::env::var("RESTREAM_PUBLIC_HOST")
                .unwrap_or_else(|_| "localhost".into()),
            stream_key: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct SensitiveState {
    channel_secrets: HashMap<String, ChannelSecret>,
    event_stream_keys: HashMap<String, String>,
    oauth_connections: Vec<OAuthConnection>,
    webhook_secrets: HashMap<String, String>,
}

fn sensitive_state(data: &RestreamData) -> SensitiveState {
    SensitiveState {
        channel_secrets: data.channel_secrets.clone(),
        event_stream_keys: data.event_stream_keys.clone(),
        oauth_connections: data.oauth_connections.clone(),
        webhook_secrets: data
            .webhooks
            .iter()
            .filter_map(|webhook| {
                webhook
                    .secret
                    .as_ref()
                    .map(|secret| (webhook.id.clone(), secret.clone()))
            })
            .collect(),
    }
}

fn encrypted_sensitive_state(data: &RestreamData, key: &[u8; 32]) -> Option<Vec<u8>> {
    let encoded = serde_json::to_vec(&sensitive_state(data)).ok()?;
    encrypt_sensitive_state(key, &encoded).ok()
}

fn decode_hex_key(value: &str) -> Option<[u8; 32]> {
    let value = value.trim();
    if value.len() != 64 {
        return None;
    }
    let mut key = [0u8; 32];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        key[index] = u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok()?;
    }
    Some(key)
}

fn load_or_create_state_key(path: &Path) -> Option<Arc<[u8; 32]>> {
    if let Ok(value) = std::env::var("RESTREAM_STATE_KEY") {
        if let Some(key) = decode_hex_key(&value) {
            return Some(Arc::new(key));
        }
        warn!("RESTREAM_STATE_KEY must contain exactly 64 hexadecimal characters; using the sidecar key instead");
    }

    let key_path = path.with_extension("key");
    if let Ok(key) = std::fs::read(&key_path) {
        if key.len() == 32 {
            let mut bytes = [0u8; 32];
            bytes.copy_from_slice(&key);
            return Some(Arc::new(bytes));
        }
        warn!(path = %key_path.display(), "ignoring invalid Reestream state key file");
    }

    let mut key = [0u8; 32];
    key[..16].copy_from_slice(Uuid::new_v4().as_bytes());
    key[16..].copy_from_slice(Uuid::new_v4().as_bytes());
    if let Err(error) = atomic_write(&key_path, &key, 0o600) {
        warn!(path = %key_path.display(), %error, "failed to create Reestream state key");
        return None;
    }
    Some(Arc::new(key))
}

fn load_state(path: Option<&Path>) -> (RestreamData, Option<SensitiveState>) {
    let Some(path) = path else {
        return (RestreamData::default(), None);
    };
    let contents = match std::fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return (RestreamData::default(), None);
        }
        Err(error) => {
            warn!(path = %path.display(), %error, "failed to read Reestream state; using empty state");
            return (RestreamData::default(), None);
        }
    };
    let raw: Value = match serde_json::from_str(&contents) {
        Ok(value) => value,
        Err(error) => {
            warn!(path = %path.display(), %error, "Reestream state is invalid; preserving it and using empty state");
            let backup = path.with_extension(format!("corrupt.{}", now()));
            let _ = std::fs::rename(path, backup);
            return (RestreamData::default(), None);
        }
    };
    let data = match serde_json::from_value(raw.clone()) {
        Ok(data) => data,
        Err(error) => {
            warn!(path = %path.display(), %error, "Reestream state could not be decoded; preserving it and using empty state");
            let backup = path.with_extension(format!("invalid.{}", now()));
            let _ = std::fs::rename(path, backup);
            return (RestreamData::default(), None);
        }
    };

    // Older releases stored secrets directly in the main JSON file.  Read
    // them once so the next mutation can migrate them to the encrypted sidecar.
    let legacy = SensitiveState {
        channel_secrets: raw
            .get("channel_secrets")
            .and_then(|value| serde_json::from_value(value.clone()).ok())
            .unwrap_or_default(),
        event_stream_keys: raw
            .get("event_stream_keys")
            .and_then(|value| serde_json::from_value(value.clone()).ok())
            .unwrap_or_default(),
        oauth_connections: raw
            .get("oauth_connections")
            .and_then(|value| serde_json::from_value(value.clone()).ok())
            .unwrap_or_default(),
        webhook_secrets: HashMap::new(),
    };
    let has_legacy = !legacy.channel_secrets.is_empty()
        || !legacy.event_stream_keys.is_empty()
        || !legacy.oauth_connections.is_empty();
    (data, has_legacy.then_some(legacy))
}

fn apply_sensitive_state(data: &mut RestreamData, sensitive: SensitiveState) {
    data.channel_secrets = sensitive.channel_secrets;
    data.event_stream_keys = sensitive.event_stream_keys;
    data.oauth_connections = sensitive.oauth_connections;
    for webhook in &mut data.webhooks {
        webhook.secret = sensitive.webhook_secrets.get(&webhook.id).cloned();
    }
}

fn encrypt_sensitive_state(key: &[u8; 32], plaintext: &[u8]) -> Result<Vec<u8>, String> {
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|error| error.to_string())?;
    let uuid = Uuid::new_v4();
    let nonce_bytes = &uuid.as_bytes()[..12];
    let ciphertext = cipher
        .encrypt(Nonce::from_slice(nonce_bytes), plaintext)
        .map_err(|_| "failed to encrypt state secrets".to_string())?;
    let mut encoded = nonce_bytes.to_vec();
    encoded.extend(ciphertext);
    Ok(encoded)
}

fn load_sensitive_state(path: &Path, key: &[u8; 32]) -> Option<SensitiveState> {
    let encrypted_path = path.with_extension("secrets");
    let encoded = std::fs::read(encrypted_path).ok()?;
    if encoded.len() <= 12 {
        return None;
    }
    let cipher = Aes256Gcm::new_from_slice(key).ok()?;
    let plaintext = cipher
        .decrypt(Nonce::from_slice(&encoded[..12]), &encoded[12..])
        .ok()?;
    serde_json::from_slice(&plaintext).ok()
}

fn atomic_write(path: &Path, bytes: &[u8], mode: u32) -> std::io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| std::io::Error::other("state path has no parent"))?;
    std::fs::create_dir_all(parent)?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("state");
    let temporary = parent.join(format!(".{file_name}.{}.tmp", Uuid::new_v4()));
    let result = (|| {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(mode);
        }
        let mut file = options.open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = file.metadata()?.permissions();
            permissions.set_mode(mode);
            std::fs::set_permissions(&temporary, permissions)?;
        }
        std::fs::rename(&temporary, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

fn write_state_files(path: &Path, public: &[u8], sensitive: Option<&[u8]>) -> std::io::Result<()> {
    atomic_write(path, public, 0o600)?;
    if let Some(sensitive) = sensitive {
        atomic_write(&path.with_extension("secrets"), sensitive, 0o600)?;
    }
    Ok(())
}

#[derive(Clone)]
pub struct RestreamStore {
    data: Arc<RwLock<RestreamData>>,
    state_path: Option<PathBuf>,
    state_key: Option<Arc<[u8; 32]>>,
    storage_root: Arc<PathBuf>,
    persist_lock: Arc<Mutex<()>>,
    recording_sessions: Arc<RwLock<HashMap<String, String>>>,
    oauth_states: Arc<RwLock<HashMap<String, OAuthState>>>,
    login_throttle: Arc<Mutex<HashMap<String, LoginThrottle>>>,
    auth: Arc<AuthConfig>,
    runtime_config: Arc<RwLock<RuntimeIngestConfig>>,
    runtime_platforms: Option<Arc<RwLock<Vec<Platform>>>>,
    event_tx: broadcast::Sender<EventNotification>,
    chat_tx: broadcast::Sender<ChatMessage>,
    platform_event_tx: broadcast::Sender<PlatformEvent>,
}

impl Default for RestreamStore {
    fn default() -> Self {
        Self::new()
    }
}

impl RestreamStore {
    pub fn new() -> Self {
        Self::from_parts(None, None, RuntimeIngestConfig::default(), None)
    }

    pub fn with_state_path(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let storage_root = path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("storage");
        Self::from_parts(
            Some(path),
            Some(storage_root),
            RuntimeIngestConfig::default(),
            None,
        )
    }

    pub fn with_runtime_config(
        path: impl Into<PathBuf>,
        listen_addr: impl Into<String>,
        listen_port: u16,
        stream_key: impl Into<String>,
        platforms: Arc<RwLock<Vec<Platform>>>,
    ) -> Self {
        let path = path.into();
        let storage_root = path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("storage");
        let runtime_config = RuntimeIngestConfig {
            listen_addr: listen_addr.into(),
            listen_port,
            advertised_host: std::env::var("RESTREAM_PUBLIC_HOST")
                .unwrap_or_else(|_| "localhost".into()),
            stream_key: stream_key.into(),
        };
        Self::from_parts(
            Some(path),
            Some(storage_root),
            runtime_config,
            Some(platforms),
        )
    }

    fn from_parts(
        state_path: Option<PathBuf>,
        storage_root: Option<PathBuf>,
        runtime_config: RuntimeIngestConfig,
        runtime_platforms: Option<Arc<RwLock<Vec<Platform>>>>,
    ) -> Self {
        let state_key = state_path
            .as_ref()
            .and_then(|path| load_or_create_state_key(path));
        let (mut data, legacy_sensitive) = load_state(state_path.as_deref());
        let mut migrated_legacy = false;
        if let (Some(path), Some(key)) = (state_path.as_deref(), state_key.as_deref()) {
            if let Some(sensitive) = load_sensitive_state(path, key) {
                apply_sensitive_state(&mut data, sensitive);
            } else if let Some(legacy_sensitive) = legacy_sensitive {
                warn!(path = %path.display(), "migrating plaintext state secrets to encrypted storage");
                apply_sensitive_state(&mut data, legacy_sensitive);
                migrated_legacy = true;
            }
        } else if let Some(legacy_sensitive) = legacy_sensitive {
            apply_sensitive_state(&mut data, legacy_sensitive);
        }
        let migration_snapshot = migrated_legacy.then(|| data.clone());
        let (event_tx, _) = broadcast::channel(512);
        let (chat_tx, _) = broadcast::channel(512);
        let (platform_event_tx, _) = broadcast::channel(256);
        let store = Self {
            data: Arc::new(RwLock::new(data)),
            state_path,
            state_key,
            storage_root: Arc::new(
                storage_root.unwrap_or_else(|| std::env::temp_dir().join("reestream-storage")),
            ),
            persist_lock: Arc::new(Mutex::new(())),
            recording_sessions: Arc::new(RwLock::new(HashMap::new())),
            oauth_states: Arc::new(RwLock::new(HashMap::new())),
            login_throttle: Arc::new(Mutex::new(HashMap::new())),
            auth: Arc::new(AuthConfig::from_env()),
            runtime_config: Arc::new(RwLock::new(runtime_config)),
            runtime_platforms,
            event_tx,
            chat_tx,
            platform_event_tx,
        };
        if let (Some(snapshot), Some(path), Some(key)) = (
            migration_snapshot,
            store.state_path.as_ref(),
            store.state_key.as_deref(),
        ) {
            match serde_json::to_vec_pretty(&snapshot)
                .ok()
                .zip(encrypted_sensitive_state(&snapshot, key))
            {
                Some((public, sensitive)) => {
                    if let Err(error) = write_state_files(path, &public, Some(&sensitive)) {
                        warn!(%error, "failed to finish plaintext state migration")
                    }
                }
                None => warn!("failed to encode migrated Reestream state"),
            }
        }
        store
    }

    async fn persist(&self, data: &RestreamData) {
        let Some(path) = &self.state_path else {
            return;
        };
        let public = match serde_json::to_vec_pretty(data) {
            Ok(encoded) => encoded,
            Err(error) => {
                warn!(%error, "failed to encode Reestream state");
                return;
            }
        };
        let sensitive = self
            .state_key
            .as_deref()
            .and_then(|key| encrypted_sensitive_state(data, key));
        let path = path.clone();
        let result = tokio::task::spawn_blocking(move || {
            write_state_files(&path, &public, sensitive.as_deref())
        })
        .await
        .unwrap_or_else(|error| Err(std::io::Error::other(error.to_string())));
        match result {
            Ok(()) => {}
            Err(error) => warn!(%error, "failed to persist Reestream state"),
        }
    }

    async fn mutate<R>(&self, operation: impl FnOnce(&mut RestreamData) -> R) -> R {
        let _persist_guard = self.persist_lock.lock().await;
        let (result, snapshot) = {
            let mut data = self.data.write().await;
            let result = operation(&mut data);
            (result, data.clone())
        };
        self.persist(&snapshot).await;
        result
    }

    pub fn auth_required(&self) -> bool {
        self.auth.required
    }

    pub async fn runtime_ingest_config(&self) -> RuntimeIngestConfig {
        self.runtime_config.read().await.clone()
    }

    pub async fn runtime_ingest_url(&self) -> String {
        let config = self.runtime_ingest_config().await;
        format!(
            "rtmp://{}:{}/live",
            config.advertised_host, config.listen_port
        )
    }

    pub async fn runtime_stream_key(&self) -> String {
        self.runtime_config.read().await.stream_key.clone()
    }

    pub async fn accepts_stream_key(&self, stream_key: &str) -> bool {
        if stream_key.is_empty() {
            return false;
        }
        if self.runtime_stream_key().await == stream_key {
            return true;
        }
        self.data
            .read()
            .await
            .event_stream_keys
            .values()
            .any(|key| key == stream_key)
    }

    pub async fn set_runtime_stream_key(&self, stream_key: impl Into<String>) {
        self.runtime_config.write().await.stream_key = stream_key.into();
    }

    pub async fn set_runtime_platforms(&self, platforms: Vec<Platform>) {
        if let Some(runtime_platforms) = &self.runtime_platforms {
            *runtime_platforms.write().await = platforms;
        }
    }

    pub async fn set_channel_status(
        &self,
        destination_id: &str,
        status: impl Into<String>,
        last_error: Option<String>,
    ) {
        let status = status.into();
        self.mutate(|data| {
            let secrets = data.channel_secrets.clone();
            for channel in &mut data.channels {
                let stream_key = secrets
                    .get(&channel.id)
                    .map(|secret| secret.stream_key.as_str())
                    .unwrap_or(channel.stream_key.as_str());
                if platform_id_from(&channel.stream_url, stream_key) == destination_id {
                    channel.status = status.clone();
                    channel.last_error = last_error.clone();
                    channel.updated_at = now();
                }
            }
        })
        .await;
    }

    pub async fn set_runtime_ingest_config(
        &self,
        listen_addr: impl Into<String>,
        listen_port: u16,
        stream_key: impl Into<String>,
    ) {
        let mut config = self.runtime_config.write().await;
        config.listen_addr = listen_addr.into();
        config.listen_port = listen_port;
        config.stream_key = stream_key.into();
    }

    pub fn subscribe_events(&self) -> broadcast::Receiver<EventNotification> {
        self.event_tx.subscribe()
    }

    pub fn subscribe_chat(&self) -> broadcast::Receiver<ChatMessage> {
        self.chat_tx.subscribe()
    }

    pub fn subscribe_platform_events(&self) -> broadcast::Receiver<PlatformEvent> {
        self.platform_event_tx.subscribe()
    }

    pub async fn profile(&self) -> Profile {
        self.data.read().await.profile.clone()
    }

    pub async fn update_profile(
        &self,
        display_name: Option<String>,
        timezone: Option<String>,
        avatar_url: Option<String>,
    ) -> Profile {
        self.mutate(|data| {
            if let Some(value) = display_name {
                data.profile.display_name = value;
            }
            if let Some(value) = timezone {
                data.profile.timezone = value;
            }
            if let Some(value) = avatar_url {
                data.profile.avatar_url = Some(value);
            }
            data.profile.clone()
        })
        .await
    }

    pub async fn login(&self, email: &str, password: &str) -> Result<AuthTokens, String> {
        let throttle_key = email.trim().to_ascii_lowercase();
        {
            let throttles = self.login_throttle.lock().await;
            if let Some(throttle) = throttles.get(&throttle_key)
                && throttle.blocked_until > now()
            {
                return Err("invalid credentials".into());
            }
        }

        let valid = self
            .auth
            .password
            .as_deref()
            .is_some_and(|configured| email == self.auth.email && password == configured);
        if !valid {
            let mut throttles = self.login_throttle.lock().await;
            let throttle = throttles.entry(throttle_key).or_default();
            throttle.failures = throttle.failures.saturating_add(1);
            if throttle.failures >= 5 {
                throttle.blocked_until = now().saturating_add(60);
            }
            return Err("invalid credentials".into());
        }
        self.login_throttle.lock().await.remove(&throttle_key);
        self.create_session().await
    }

    pub async fn create_session(&self) -> Result<AuthTokens, String> {
        let access_token = format!("rst_{}", id().replace('-', ""));
        let refresh_token = format!("rsr_{}", id().replace('-', ""));
        let expires_in = 60 * 60 * 24 * 30;
        let session = AuthSession {
            access_token: access_token.clone(),
            refresh_token: refresh_token.clone(),
            expires_at: now() + expires_in,
        };
        self.mutate(|data| data.sessions.push(session)).await;
        Ok(AuthTokens {
            access_token,
            refresh_token,
            token_type: "Bearer".into(),
            expires_in,
        })
    }

    pub async fn refresh_session(&self, refresh_token: &str) -> Result<AuthTokens, String> {
        let expires_in = 60 * 60 * 24 * 30;
        let access_token = format!("rst_{}", id().replace('-', ""));
        let next_refresh_token = format!("rsr_{}", id().replace('-', ""));
        let next_session = AuthSession {
            access_token: access_token.clone(),
            refresh_token: next_refresh_token.clone(),
            expires_at: now() + expires_in,
        };
        let refreshed = self
            .mutate(|data| {
                let index = data.sessions.iter().position(|session| {
                    session.refresh_token == refresh_token && session.expires_at > now()
                })?;
                data.sessions[index] = next_session;
                Some(())
            })
            .await;
        if refreshed.is_none() {
            return Err("invalid refresh token".into());
        }
        Ok(AuthTokens {
            access_token,
            refresh_token: next_refresh_token,
            token_type: "Bearer".into(),
            expires_in,
        })
    }

    pub async fn revoke_session(&self, access_token: &str) {
        self.mutate(|data| {
            data.sessions
                .retain(|session| session.access_token != access_token)
        })
        .await;
    }

    pub async fn validate_access_token(&self, token: &str) -> bool {
        if !self.auth.required {
            return true;
        }
        self.data
            .read()
            .await
            .sessions
            .iter()
            .any(|session| session.access_token == token && session.expires_at > now())
    }

    pub async fn list_connections(&self) -> Vec<ConnectionSummary> {
        self.data
            .read()
            .await
            .oauth_connections
            .iter()
            .map(connection_summary)
            .collect()
    }

    pub async fn save_oauth_connection(
        &self,
        platform_id: String,
        access_token: String,
        refresh_token: Option<String>,
        token_type: String,
        expires_at: Option<u64>,
        scopes: Vec<String>,
    ) -> ConnectionSummary {
        let timestamp = now();
        let connection = self
            .mutate(|data| {
                if let Some(connection) = data
                    .oauth_connections
                    .iter_mut()
                    .find(|connection| connection.platform_id == platform_id)
                {
                    connection.access_token = access_token;
                    connection.refresh_token = refresh_token;
                    connection.token_type = token_type;
                    connection.expires_at = expires_at;
                    connection.scopes = scopes;
                    connection.updated_at = timestamp;
                    return connection.clone();
                }
                let connection = OAuthConnection {
                    id: id(),
                    platform_id,
                    access_token,
                    refresh_token,
                    token_type,
                    expires_at,
                    scopes,
                    connected_at: timestamp,
                    updated_at: timestamp,
                };
                data.oauth_connections.push(connection.clone());
                connection
            })
            .await;
        connection_summary(&connection)
    }

    pub async fn delete_connection(&self, connection_id: &str) -> bool {
        self.mutate(|data| {
            let before = data.oauth_connections.len();
            data.oauth_connections
                .retain(|connection| connection.id != connection_id);
            data.oauth_connections.len() != before
        })
        .await
    }

    pub async fn save_oauth_state(
        &self,
        state: String,
        platform_id: String,
        redirect_uri: String,
        owner_token: String,
    ) {
        self.oauth_states.write().await.insert(
            state,
            (
                platform_id,
                redirect_uri,
                now().saturating_add(600),
                owner_token,
            ),
        );
    }

    pub async fn consume_oauth_state(
        &self,
        state: &str,
        platform_id: &str,
        redirect_uri: &str,
        owner_token: &str,
    ) -> bool {
        let Some((stored_platform, stored_redirect, expires_at, stored_owner)) =
            self.oauth_states.write().await.remove(state)
        else {
            return false;
        };
        stored_platform == platform_id
            && stored_redirect == redirect_uri
            && stored_owner == owner_token
            && expires_at > now()
    }

    pub async fn list_channels(&self) -> Vec<Channel> {
        let data = self.data.read().await;
        data.channels
            .iter()
            .cloned()
            .map(|mut channel| {
                hydrate_channel(&data, &mut channel);
                channel
            })
            .collect()
    }

    pub async fn get_channel(&self, channel_id: &str) -> Option<Channel> {
        let data = self.data.read().await;
        let mut channel = data
            .channels
            .iter()
            .find(|channel| channel.id == channel_id)
            .cloned()?;
        hydrate_channel(&data, &mut channel);
        Some(channel)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn create_channel(
        &self,
        platform_id: String,
        display_name: String,
        channel_url: Option<String>,
        stream_url: String,
        stream_key: String,
        rtmp_username: Option<String>,
        rtmp_password: Option<String>,
    ) -> Channel {
        let timestamp = now();
        let channel = Channel {
            id: id(),
            platform_id,
            display_name,
            channel_url,
            stream_url,
            stream_key,
            rtmp_username,
            rtmp_password,
            enabled: true,
            status: "disconnected".into(),
            created_at: timestamp,
            updated_at: timestamp,
            last_error: None,
        };
        self.mutate(|data| {
            data.channel_secrets.insert(
                channel.id.clone(),
                ChannelSecret {
                    stream_key: channel.stream_key.clone(),
                    rtmp_username: channel.rtmp_username.clone(),
                    rtmp_password: channel.rtmp_password.clone(),
                },
            );
            data.channels.push(channel.clone());
        })
        .await;
        let _ = self.platform_event_tx.send(PlatformEvent::Added {
            platform_id: platform_id_from(&channel.stream_url, &channel.stream_key),
            url: channel.stream_url.clone(),
            key: channel.stream_key.clone(),
        });
        channel
    }

    pub async fn update_channel(
        &self,
        channel_id: &str,
        display_name: Option<String>,
        channel_url: Option<String>,
        stream_url: Option<String>,
        stream_key: Option<String>,
        enabled: Option<bool>,
    ) -> Option<Channel> {
        let previous = self.get_channel(channel_id).await;
        let updated = self
            .mutate(|data| {
                let channel_index = data
                    .channels
                    .iter()
                    .position(|channel| channel.id == channel_id)?;
                let mut channel = data.channels[channel_index].clone();
                hydrate_channel(data, &mut channel);
                if let Some(value) = display_name {
                    channel.display_name = value;
                }
                if let Some(value) = channel_url {
                    channel.channel_url = Some(value);
                }
                if let Some(value) = stream_url {
                    channel.stream_url = value;
                }
                if let Some(value) = stream_key {
                    channel.stream_key = value;
                }
                if let Some(value) = enabled {
                    channel.enabled = value;
                }
                channel.updated_at = now();
                data.channels[channel_index] = channel.clone();
                data.channel_secrets.insert(
                    channel.id.clone(),
                    ChannelSecret {
                        stream_key: channel.stream_key.clone(),
                        rtmp_username: channel.rtmp_username.clone(),
                        rtmp_password: channel.rtmp_password.clone(),
                    },
                );
                Some(channel)
            })
            .await;
        if let Some(ref channel) = updated {
            if let Some(previous) = previous
                && (previous.stream_url != channel.stream_url
                    || previous.stream_key != channel.stream_key)
            {
                let _ = self.platform_event_tx.send(PlatformEvent::Removed {
                    platform_id: platform_id_from(&previous.stream_url, &previous.stream_key),
                });
            }
            let _ = self.platform_event_tx.send(PlatformEvent::Added {
                platform_id: platform_id_from(&channel.stream_url, &channel.stream_key),
                url: channel.stream_url.clone(),
                key: channel.stream_key.clone(),
            });
            if !channel.enabled {
                let _ = self.platform_event_tx.send(PlatformEvent::Toggled {
                    platform_id: platform_id_from(&channel.stream_url, &channel.stream_key),
                    url: channel.stream_url.clone(),
                    key: channel.stream_key.clone(),
                    enabled: false,
                });
            }
        }
        updated
    }

    pub async fn delete_channel(&self, channel_id: &str) -> bool {
        let removed_channel = self.get_channel(channel_id).await;
        let removed = self
            .mutate(|data| {
                let before = data.channels.len();
                data.channels.retain(|channel| channel.id != channel_id);
                data.channel_secrets.remove(channel_id);
                data.channels.len() != before
            })
            .await;
        if removed && let Some(channel) = removed_channel {
            let _ = self.platform_event_tx.send(PlatformEvent::Removed {
                platform_id: platform_id_from(&channel.stream_url, &channel.stream_key),
            });
        }
        removed
    }

    pub async fn list_drafts(&self) -> Vec<Draft> {
        self.data.read().await.drafts.clone()
    }

    pub async fn get_draft(&self, draft_id: &str) -> Option<Draft> {
        self.data
            .read()
            .await
            .drafts
            .iter()
            .find(|draft| draft.id == draft_id)
            .cloned()
    }

    pub async fn create_draft(
        &self,
        name: String,
        stream_type: StreamType,
        title: String,
        description: String,
        destination_ids: Vec<String>,
        brand_id: Option<String>,
    ) -> Draft {
        let timestamp = now();
        let draft = Draft {
            id: id(),
            name,
            stream_type,
            title,
            description,
            destination_ids,
            brand_id,
            studio_session_id: None,
            created_at: timestamp,
            updated_at: timestamp,
        };
        self.mutate(|data| data.drafts.push(draft.clone())).await;
        draft
    }

    pub async fn update_draft(
        &self,
        draft_id: &str,
        name: Option<String>,
        title: Option<String>,
        description: Option<String>,
        destination_ids: Option<Vec<String>>,
        brand_id: Option<String>,
    ) -> Option<Draft> {
        self.mutate(|data| {
            let draft = data.drafts.iter_mut().find(|draft| draft.id == draft_id)?;
            if let Some(value) = name {
                draft.name = value;
            }
            if let Some(value) = title {
                draft.title = value;
            }
            if let Some(value) = description {
                draft.description = value;
            }
            if let Some(value) = destination_ids {
                draft.destination_ids = value;
            }
            if let Some(value) = brand_id {
                draft.brand_id = Some(value);
            }
            draft.updated_at = now();
            Some(draft.clone())
        })
        .await
    }

    pub async fn delete_draft(&self, draft_id: &str) -> bool {
        self.mutate(|data| {
            let before = data.drafts.len();
            data.drafts.retain(|draft| draft.id != draft_id);
            data.drafts.len() != before
        })
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn create_event(
        &self,
        draft_id: Option<String>,
        stream_type: StreamType,
        title: String,
        description: String,
        scheduled_for: Option<String>,
        destination_ids: Vec<String>,
        source_file_id: Option<String>,
        loops_count: u8,
    ) -> Event {
        let timestamp = now();
        let event_id = id();
        let stream_key = format!("event-{}", event_id.replace('-', ""));
        let server_url = self.runtime_ingest_url().await;
        let event = Event {
            id: event_id.clone(),
            draft_id,
            stream_type,
            title,
            description,
            status: if scheduled_for.is_some() {
                EventStatus::Scheduled
            } else {
                EventStatus::Draft
            },
            scheduled_for,
            created_at: timestamp,
            updated_at: timestamp,
            started_at: None,
            ended_at: None,
            duration_seconds: None,
            destination_ids,
            source_file_id,
            loops_count,
            guest_link: Some(format!("/studio/guest/{event_id}")),
            ingest: IngestCredentials {
                server_url,
                stream_key,
                backup_server_url: None,
                protocol: "rtmp".into(),
            },
            recording_file_id: None,
            current_viewers: 0,
            peak_viewers: 0,
            chat_message_count: 0,
        };
        self.mutate(|data| {
            data.event_stream_keys
                .insert(event.id.clone(), event.ingest.stream_key.clone());
            data.events.push(event.clone());
        })
        .await;
        self.publish_event("event.created", &event).await;
        event
    }

    pub async fn list_events(&self, status: Option<EventStatus>) -> Vec<Event> {
        let data = self.data.read().await;
        let mut events = data
            .events
            .iter()
            .cloned()
            .map(|mut event| {
                hydrate_event(&data, &mut event);
                event
            })
            .collect::<Vec<_>>();
        if let Some(status) = status {
            events.retain(|event| event.status == status);
        }
        events.sort_by_key(|event| std::cmp::Reverse(event.updated_at));
        events
    }

    pub async fn get_event(&self, event_id: &str) -> Option<Event> {
        let data = self.data.read().await;
        let mut event = data
            .events
            .iter()
            .find(|event| event.id == event_id)
            .cloned()?;
        hydrate_event(&data, &mut event);
        Some(event)
    }

    pub async fn update_event(
        &self,
        event_id: &str,
        title: Option<String>,
        description: Option<String>,
        scheduled_for: Option<Option<String>>,
        destination_ids: Option<Vec<String>>,
    ) -> Option<Event> {
        let event = self
            .mutate(|data| {
                let event = data.events.iter_mut().find(|event| event.id == event_id)?;
                if let Some(value) = title {
                    event.title = value;
                }
                if let Some(value) = description {
                    event.description = value;
                }
                if let Some(value) = scheduled_for {
                    event.scheduled_for = value;
                    if event.status == EventStatus::Draft && event.scheduled_for.is_some() {
                        event.status = EventStatus::Scheduled;
                    } else if event.status == EventStatus::Scheduled
                        && event.scheduled_for.is_none()
                    {
                        event.status = EventStatus::Draft;
                    }
                }
                if let Some(value) = destination_ids {
                    event.destination_ids = value;
                }
                event.updated_at = now();
                Some(event.clone())
            })
            .await;
        if let Some(ref event) = event {
            self.publish_event("event.updated", event).await;
        }
        event
    }

    pub async fn delete_event(&self, event_id: &str) -> bool {
        let deleted = self
            .mutate(|data| {
                let before = data.events.len();
                data.events.retain(|event| event.id != event_id);
                data.events.len() != before
            })
            .await;
        if deleted {
            self.publish_notification("event.deleted", event_id, Value::Null)
                .await;
        }
        deleted
    }

    pub async fn set_event_live(&self, event_id: &str) -> Option<Event> {
        let event = self
            .mutate(|data| {
                let event = data.events.iter_mut().find(|event| event.id == event_id)?;
                if matches!(event.status, EventStatus::Ended | EventStatus::Cancelled) {
                    return None;
                }
                if event.status == EventStatus::Live {
                    return Some(event.clone());
                }
                let timestamp = now();
                event.status = EventStatus::Live;
                event.started_at = Some(timestamp);
                event.updated_at = timestamp;
                Some(event.clone())
            })
            .await;
        if let Some(ref event) = event {
            self.publish_event("event.started", event).await;
        }
        event
    }

    pub async fn promote_due_events(&self) -> Vec<Event> {
        let timestamp = now();
        let due = self
            .mutate(|data| {
                let mut due = Vec::new();
                for event in &mut data.events {
                    let scheduled = event.scheduled_for.as_deref().and_then(scheduled_epoch);
                    if event.status == EventStatus::Scheduled
                        && scheduled.is_some_and(|scheduled| scheduled <= timestamp)
                    {
                        event.status = EventStatus::Live;
                        event.started_at = Some(timestamp);
                        event.updated_at = timestamp;
                        due.push(event.clone());
                    }
                }
                due
            })
            .await;
        for event in &due {
            self.publish_event("event.started", event).await;
        }
        due
    }

    pub async fn end_event(&self, event_id: &str) -> Option<Event> {
        let event = self
            .mutate(|data| {
                let event = data.events.iter_mut().find(|event| event.id == event_id)?;
                if event.status != EventStatus::Live {
                    return None;
                }
                let timestamp = now();
                event.status = EventStatus::Ended;
                event.ended_at = Some(timestamp);
                event.updated_at = timestamp;
                event.duration_seconds = event
                    .started_at
                    .map(|started| timestamp.saturating_sub(started));
                event.current_viewers = 0;
                Some(event.clone())
            })
            .await;
        if let Some(ref event) = event {
            self.publish_event("event.ended", event).await;
        }
        event
    }

    pub async fn cancel_event(&self, event_id: &str) -> Option<Event> {
        let event = self
            .mutate(|data| {
                let event = data.events.iter_mut().find(|event| event.id == event_id)?;
                event.status = EventStatus::Cancelled;
                event.updated_at = now();
                Some(event.clone())
            })
            .await;
        if let Some(ref event) = event {
            self.publish_event("event.cancelled", event).await;
        }
        event
    }

    pub async fn duplicate_event(&self, event_id: &str) -> Option<Draft> {
        let source = self.get_event(event_id).await?;
        Some(
            self.create_draft(
                format!("Copy of {}", source.title),
                source.stream_type,
                source.title,
                source.description,
                source.destination_ids,
                None,
            )
            .await,
        )
    }

    pub async fn event_credentials(&self, event_id: &str) -> Option<IngestCredentials> {
        self.get_event(event_id).await.map(|event| event.ingest)
    }

    pub async fn add_event_destination(&self, event_id: &str, channel_id: &str) -> Option<Event> {
        let exists = self.get_channel(channel_id).await.is_some();
        if !exists {
            return None;
        }
        self.mutate(|data| {
            let event = data.events.iter_mut().find(|event| event.id == event_id)?;
            if !event.destination_ids.iter().any(|id| id == channel_id) {
                event.destination_ids.push(channel_id.to_string());
            }
            event.updated_at = now();
            Some(event.clone())
        })
        .await
    }

    pub async fn remove_event_destination(
        &self,
        event_id: &str,
        channel_id: &str,
    ) -> Option<Event> {
        self.mutate(|data| {
            let event = data.events.iter_mut().find(|event| event.id == event_id)?;
            event.destination_ids.retain(|id| id != channel_id);
            event.updated_at = now();
            Some(event.clone())
        })
        .await
    }

    pub async fn list_files(&self, query: Option<&str>) -> Vec<StorageFile> {
        let mut files = self.data.read().await.files.clone();
        if let Some(query) = query.filter(|query| !query.trim().is_empty()) {
            let query = query.to_lowercase();
            files.retain(|file| {
                file.name.to_lowercase().contains(&query)
                    || file
                        .labels
                        .iter()
                        .any(|label| label.to_lowercase().contains(&query))
            });
        }
        files.sort_by_key(|file| std::cmp::Reverse(file.updated_at));
        files
    }

    pub async fn get_file(&self, file_id: &str) -> Option<StorageFile> {
        self.data
            .read()
            .await
            .files
            .iter()
            .find(|file| file.id == file_id)
            .cloned()
    }

    pub async fn create_file_metadata(
        &self,
        name: String,
        mime_type: String,
        size_bytes: u64,
        duration_seconds: Option<u64>,
        labels: Vec<String>,
        path: String,
    ) -> StorageFile {
        self.create_file_metadata_with_status(
            name,
            mime_type,
            size_bytes,
            duration_seconds,
            labels,
            path,
            "ready".into(),
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn create_file_metadata_with_status(
        &self,
        name: String,
        mime_type: String,
        size_bytes: u64,
        duration_seconds: Option<u64>,
        labels: Vec<String>,
        path: String,
        status: String,
    ) -> StorageFile {
        let timestamp = now();
        let file = StorageFile {
            id: id(),
            name,
            mime_type,
            size_bytes,
            duration_seconds,
            status,
            path,
            labels,
            created_at: timestamp,
            updated_at: timestamp,
        };
        self.mutate(|data| data.files.push(file.clone())).await;
        file
    }

    pub async fn update_file_state(
        &self,
        file_id: &str,
        size_bytes: u64,
        status: String,
    ) -> Option<StorageFile> {
        self.mutate(|data| {
            let file = data.files.iter_mut().find(|file| file.id == file_id)?;
            file.size_bytes = size_bytes;
            file.status = status;
            file.updated_at = now();
            Some(file.clone())
        })
        .await
    }

    pub async fn list_transcriptions(&self, event_id: &str) -> Option<Vec<Transcription>> {
        self.get_event(event_id).await.as_ref()?;
        Some(
            self.data
                .read()
                .await
                .transcriptions
                .iter()
                .filter(|transcription| transcription.event_id == event_id)
                .cloned()
                .collect(),
        )
    }

    pub async fn ensure_transcription(&self, event_id: &str) -> Option<Transcription> {
        let event = self.get_event(event_id).await?;
        let file_id = event.recording_file_id?;
        let file = self.get_file(&file_id).await?;
        if let Some(existing) = self
            .data
            .read()
            .await
            .transcriptions
            .iter()
            .find(|transcription| transcription.event_id == event_id)
            .cloned()
        {
            return Some(existing);
        }

        let timestamp = now();
        let transcription = Transcription {
            id: id(),
            event_id: event_id.to_string(),
            file_id: file.id.clone(),
            file_name: file.name.clone(),
            status: if std::env::var("RESTREAM_TRANSCRIBER_BIN").is_ok() {
                "InProgress".into()
            } else {
                "Unknown".into()
            },
            language: std::env::var("RESTREAM_TRANSCRIBER_LANGUAGE").ok(),
            download_url: None,
            created_at: timestamp,
            updated_at: timestamp,
        };
        self.mutate(|data| data.transcriptions.push(transcription.clone()))
            .await;

        if let Ok(binary) = std::env::var("RESTREAM_TRANSCRIBER_BIN") {
            let store = self.clone();
            let source_path = file.path.clone();
            let transcription_id = transcription.id.clone();
            let event_id_owned = event_id.to_string();
            let output_path = self
                .storage_path(&transcription.id, "transcript.txt")
                .to_string_lossy()
                .into_owned();
            tokio::spawn(async move {
                let _ = tokio::fs::create_dir_all(store.storage_root_path()).await;
                let result = tokio::process::Command::new(binary)
                    .arg(&source_path)
                    .arg(&output_path)
                    .output()
                    .await;
                let success = result.is_ok_and(|output| output.status.success())
                    && tokio::fs::metadata(&output_path)
                        .await
                        .map(|metadata| metadata.is_file())
                        .unwrap_or(false);
                if success {
                    let file = store
                        .create_file_metadata(
                            format!("{}.txt", transcription_id),
                            "text/plain".into(),
                            tokio::fs::metadata(&output_path)
                                .await
                                .map(|metadata| metadata.len())
                                .unwrap_or(0),
                            None,
                            vec!["transcription".into(), event_id_owned],
                            output_path.clone(),
                        )
                        .await;
                    store
                        .update_transcription(
                            &transcription_id,
                            "Completed".into(),
                            Some(format!("/api/v1/storage/files/{}/download", file.id)),
                        )
                        .await;
                } else {
                    store
                        .update_transcription(&transcription_id, "Failed".into(), None)
                        .await;
                }
            });
        }
        Some(transcription)
    }

    async fn update_transcription(
        &self,
        transcription_id: &str,
        status: String,
        download_url: Option<String>,
    ) -> Option<Transcription> {
        self.mutate(|data| {
            let transcription = data
                .transcriptions
                .iter_mut()
                .find(|transcription| transcription.id == transcription_id)?;
            transcription.status = status;
            transcription.download_url = download_url;
            transcription.updated_at = now();
            Some(transcription.clone())
        })
        .await
    }

    pub async fn link_event_recording(&self, event_id: &str, file_id: &str) -> Option<Event> {
        self.get_file(file_id).await.as_ref()?;
        let event = self
            .mutate(|data| {
                let event = data.events.iter_mut().find(|event| event.id == event_id)?;
                event.recording_file_id = Some(file_id.to_string());
                event.updated_at = now();
                Some(event.clone())
            })
            .await;
        if let Some(ref event) = event {
            self.publish_event("event.updated", event).await;
        }
        event
    }

    pub async fn set_recording_session(&self, event_id: &str, recording_id: &str) {
        self.recording_sessions
            .write()
            .await
            .insert(event_id.to_string(), recording_id.to_string());
    }

    pub async fn take_recording_session(&self, event_id: &str) -> Option<String> {
        self.recording_sessions.write().await.remove(event_id)
    }

    pub async fn recording_session(&self, event_id: &str) -> Option<String> {
        self.recording_sessions.read().await.get(event_id).cloned()
    }

    pub fn storage_path(&self, file_id: &str, name: &str) -> PathBuf {
        let safe_name: String = name
            .chars()
            .map(|character| {
                if character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-') {
                    character
                } else {
                    '_'
                }
            })
            .collect();
        self.storage_root.join(format!("{file_id}-{safe_name}"))
    }

    pub fn storage_root_path(&self) -> PathBuf {
        self.storage_root.as_ref().clone()
    }

    pub fn is_managed_storage_path(&self, path: &str) -> bool {
        let Ok(root) = std::fs::canonicalize(self.storage_root.as_ref()) else {
            return false;
        };
        let Ok(candidate) = std::fs::canonicalize(path) else {
            return false;
        };
        candidate.starts_with(root)
    }

    pub async fn update_file(
        &self,
        file_id: &str,
        name: Option<String>,
        labels: Option<Vec<String>>,
    ) -> Option<StorageFile> {
        self.mutate(|data| {
            let file = data.files.iter_mut().find(|file| file.id == file_id)?;
            if let Some(value) = name {
                file.name = value;
            }
            if let Some(value) = labels {
                file.labels = value;
            }
            file.updated_at = now();
            Some(file.clone())
        })
        .await
    }

    pub async fn delete_file(&self, file_id: &str) -> Option<StorageFile> {
        let file = self
            .mutate(|data| {
                let index = data.files.iter().position(|file| file.id == file_id)?;
                Some(data.files.remove(index))
            })
            .await;
        if let Some(ref file) = file
            && self.is_managed_storage_path(&file.path)
        {
            let _ = tokio::fs::remove_file(&file.path).await;
        }
        file
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn add_chat_message(
        &self,
        event_id: String,
        destination_id: Option<String>,
        author_name: String,
        author_id: Option<String>,
        message: String,
        kind: String,
        reply_to: Option<String>,
    ) -> Result<ChatMessage, String> {
        if self.get_event(&event_id).await.is_none() {
            return Err("event not found".into());
        }
        let chat_message = ChatMessage {
            id: id(),
            event_id: event_id.clone(),
            destination_id,
            author_name,
            author_id,
            message,
            kind,
            reply_to,
            created_at: now(),
            deleted: false,
        };
        self.mutate(|data| {
            data.chat.push(chat_message.clone());
            if let Some(event) = data.events.iter_mut().find(|event| event.id == event_id) {
                event.chat_message_count += 1;
                event.updated_at = now();
            }
        })
        .await;
        let _ = self.chat_tx.send(chat_message.clone());
        self.publish_notification(
            "chat.message",
            &chat_message.id,
            serde_json::to_value(&chat_message).unwrap_or(Value::Null),
        )
        .await;
        Ok(chat_message)
    }

    pub async fn list_chat(&self, event_id: Option<&str>) -> Vec<ChatMessage> {
        let mut messages = self.data.read().await.chat.clone();
        if let Some(event_id) = event_id {
            messages.retain(|message| message.event_id == event_id);
        }
        messages.retain(|message| !message.deleted);
        messages.sort_by_key(|message| message.created_at);
        messages
    }

    pub async fn delete_chat_message(&self, message_id: &str) -> Option<ChatMessage> {
        self.mutate(|data| {
            let message = data
                .chat
                .iter_mut()
                .find(|message| message.id == message_id)?;
            message.deleted = true;
            Some(message.clone())
        })
        .await
    }

    pub async fn record_viewers(&self, event_id: &str, viewers: u32, bitrate_kbps: u64) -> bool {
        let timestamp = now();
        self.mutate(|data| {
            let Some(event) = data.events.iter_mut().find(|event| event.id == event_id) else {
                return false;
            };
            event.current_viewers = viewers;
            event.peak_viewers = event.peak_viewers.max(viewers);
            event.updated_at = timestamp;
            data.viewer_samples.push(ViewerSample {
                event_id: event_id.to_string(),
                timestamp,
                viewers,
                bitrate_kbps,
            });
            true
        })
        .await
    }

    pub async fn analytics(&self, event_id: &str) -> Option<AnalyticsReport> {
        let data = self.data.read().await;
        let event = data.events.iter().find(|event| event.id == event_id)?;
        let samples: Vec<ViewerSample> = data
            .viewer_samples
            .iter()
            .filter(|sample| sample.event_id == event_id)
            .cloned()
            .collect();
        let average = if samples.is_empty() {
            0
        } else {
            samples
                .iter()
                .map(|sample| sample.viewers as u64)
                .sum::<u64>()
                / samples.len() as u64
        } as u32;
        let duration = event
            .duration_seconds
            .or_else(|| {
                event
                    .started_at
                    .map(|started| now().saturating_sub(started))
            })
            .unwrap_or(0);
        Some(AnalyticsReport {
            event_id: event.id.clone(),
            title: event.title.clone(),
            status: event.status.clone(),
            views: samples.iter().map(|sample| sample.viewers as u64).sum(),
            peak_concurrent_viewers: event.peak_viewers,
            average_concurrent_viewers: average,
            chat_messages: event.chat_message_count,
            duration_seconds: duration,
            destinations: event
                .destination_ids
                .iter()
                .map(|destination_id| DestinationAnalytics {
                    destination_id: destination_id.clone(),
                    views: 0,
                    peak_viewers: 0,
                    status: "unknown".into(),
                })
                .collect(),
            timeseries: samples,
        })
    }

    pub async fn analytics_overview(
        &self,
        from: Option<String>,
        to: Option<String>,
    ) -> AnalyticsOverview {
        let from_epoch = from.as_deref().and_then(scheduled_epoch);
        let to_epoch = to.as_deref().and_then(scheduled_epoch);
        let mut events = self.list_events(Some(EventStatus::Ended)).await;
        events.retain(|event| {
            let timestamp = event.ended_at.unwrap_or(event.updated_at);
            from_epoch.is_none_or(|from| timestamp >= from)
                && to_epoch.is_none_or(|to| timestamp <= to)
        });
        let mut streams = Vec::new();
        for event in &events {
            if let Some(report) = self.analytics(&event.id).await {
                streams.push(report);
            }
        }
        let total_duration = streams
            .iter()
            .map(|stream| stream.duration_seconds)
            .sum::<u64>();
        AnalyticsOverview {
            from,
            to,
            total_views: streams.iter().map(|stream| stream.views).sum(),
            total_streams: streams.len() as u64,
            total_chat_messages: streams.iter().map(|stream| stream.chat_messages).sum(),
            average_duration_seconds: if streams.is_empty() {
                0
            } else {
                total_duration / streams.len() as u64
            },
            streams,
        }
    }

    pub async fn create_clip(
        &self,
        event_id: String,
        name: String,
        start_seconds: u64,
        end_seconds: u64,
    ) -> Option<ClipProject> {
        let event = self.get_event(&event_id).await?;
        let source_file_id = event.recording_file_id.or(event.source_file_id);
        let source_file = match source_file_id.as_deref() {
            Some(file_id) => self.get_file(file_id).await,
            None => None,
        };
        let clip_id = id();
        let timestamp = now();
        let clip = ClipProject {
            id: clip_id.clone(),
            event_id,
            source_file_id,
            name,
            start_seconds,
            end_seconds,
            status: if source_file
                .as_ref()
                .is_some_and(|file| self.is_managed_storage_path(&file.path))
            {
                "processing"
            } else {
                "failed"
            }
            .into(),
            output_file_id: None,
            created_at: timestamp,
            updated_at: timestamp,
        };
        self.mutate(|data| data.clips.push(clip.clone())).await;

        if let Some(source_file) = source_file
            && self.is_managed_storage_path(&source_file.path)
        {
            let store = self.clone();
            let clip_id = clip.id.clone();
            let output_name = format!("{}-clip.mp4", clip.name);
            let output_path = self.storage_path(&clip_id, &output_name);
            let duration = end_seconds.saturating_sub(start_seconds);
            tokio::spawn(async move {
                let Some(parent) = output_path.parent() else {
                    return;
                };
                if tokio::fs::create_dir_all(parent).await.is_err() {
                    store.mark_clip_failed(&clip_id).await;
                    return;
                }
                let succeeded = tokio::process::Command::new(ffmpeg_command())
                    .args([
                        "-y",
                        "-ss",
                        &start_seconds.to_string(),
                        "-i",
                        &source_file.path,
                        "-t",
                        &duration.to_string(),
                        "-c",
                        "copy",
                    ])
                    .arg(&output_path)
                    .status()
                    .await
                    .map(|status| status.success())
                    .unwrap_or(false);
                if !succeeded {
                    store.mark_clip_failed(&clip_id).await;
                    return;
                }
                let size_bytes = tokio::fs::metadata(&output_path)
                    .await
                    .map(|metadata| metadata.len())
                    .unwrap_or(0);
                let file = store
                    .create_file_metadata(
                        output_name,
                        "video/mp4".into(),
                        size_bytes,
                        Some(duration),
                        vec!["clip".into()],
                        output_path.to_string_lossy().into(),
                    )
                    .await;
                store
                    .mutate(|data| {
                        if let Some(clip) = data.clips.iter_mut().find(|clip| clip.id == clip_id) {
                            clip.status = "ready".into();
                            clip.output_file_id = Some(file.id.clone());
                            clip.updated_at = now();
                        }
                    })
                    .await;
            });
        }
        Some(clip)
    }

    async fn mark_clip_failed(&self, clip_id: &str) {
        self.mutate(|data| {
            if let Some(clip) = data.clips.iter_mut().find(|clip| clip.id == clip_id) {
                clip.status = "failed".into();
                clip.updated_at = now();
            }
        })
        .await;
    }

    pub async fn list_clips(&self, event_id: Option<&str>) -> Vec<ClipProject> {
        let mut clips = self.data.read().await.clips.clone();
        if let Some(event_id) = event_id {
            clips.retain(|clip| clip.event_id == event_id);
        }
        clips.sort_by_key(|clip| std::cmp::Reverse(clip.updated_at));
        clips
    }

    pub async fn get_clip(&self, clip_id: &str) -> Option<ClipProject> {
        self.data
            .read()
            .await
            .clips
            .iter()
            .find(|clip| clip.id == clip_id)
            .cloned()
    }

    pub async fn delete_clip(&self, clip_id: &str) -> bool {
        self.mutate(|data| {
            let before = data.clips.len();
            data.clips.retain(|clip| clip.id != clip_id);
            data.clips.len() != before
        })
        .await
    }

    pub async fn get_or_create_studio_session(&self, event_id: &str) -> Option<StudioSession> {
        if let Some(session) = self
            .data
            .read()
            .await
            .studio_sessions
            .iter()
            .find(|session| session.event_id == event_id)
            .cloned()
        {
            return Some(session);
        }
        self.get_event(event_id).await.as_ref()?;
        let timestamp = now();
        let session = StudioSession {
            id: id(),
            event_id: event_id.into(),
            status: "idle".into(),
            layout: "grid".into(),
            settings: Value::Object(Default::default()),
            guest_link: format!("/studio/guest/{event_id}"),
            guests: Vec::new(),
            scenes: vec![Scene {
                id: id(),
                name: "Main scene".into(),
                layout: "grid".into(),
                source_ids: Vec::new(),
                active: true,
            }],
            active_scene_id: None,
            created_at: timestamp,
            updated_at: timestamp,
        };
        self.mutate(|data| data.studio_sessions.push(session.clone()))
            .await;
        Some(session)
    }

    pub async fn update_studio_session(
        &self,
        session_id: &str,
        layout: Option<String>,
        settings: Option<Value>,
        status: Option<String>,
    ) -> Option<StudioSession> {
        self.mutate(|data| {
            let session = data
                .studio_sessions
                .iter_mut()
                .find(|session| session.id == session_id)?;
            if let Some(value) = layout {
                session.layout = value;
            }
            if let Some(value) = settings {
                session.settings = value;
            }
            if let Some(value) = status {
                session.status = value;
            }
            session.updated_at = now();
            Some(session.clone())
        })
        .await
    }

    pub async fn add_guest(&self, session_id: &str, name: String, role: String) -> Option<Guest> {
        let guest = Guest {
            id: id(),
            name,
            role,
            join_url: format!("/studio/guest/{}", id()),
            status: "invited".into(),
            created_at: now(),
        };
        self.mutate(|data| {
            let session = data
                .studio_sessions
                .iter_mut()
                .find(|session| session.id == session_id)?;
            session.guests.push(guest.clone());
            session.updated_at = now();
            Some(guest.clone())
        })
        .await
    }

    pub async fn remove_guest(&self, session_id: &str, guest_id: &str) -> bool {
        self.mutate(|data| {
            let Some(session) = data
                .studio_sessions
                .iter_mut()
                .find(|session| session.id == session_id)
            else {
                return false;
            };
            let before = session.guests.len();
            session.guests.retain(|guest| guest.id != guest_id);
            before != session.guests.len()
        })
        .await
    }

    pub async fn add_scene(
        &self,
        session_id: &str,
        name: String,
        layout: String,
        source_ids: Vec<String>,
    ) -> Option<Scene> {
        let scene = Scene {
            id: id(),
            name,
            layout,
            source_ids,
            active: false,
        };
        self.mutate(|data| {
            let session = data
                .studio_sessions
                .iter_mut()
                .find(|session| session.id == session_id)?;
            session.scenes.push(scene.clone());
            session.updated_at = now();
            Some(scene)
        })
        .await
    }

    pub async fn update_scene(
        &self,
        session_id: &str,
        scene_id: &str,
        name: Option<String>,
        layout: Option<String>,
        source_ids: Option<Vec<String>>,
        active: Option<bool>,
    ) -> Option<Scene> {
        self.mutate(|data| {
            let session = data
                .studio_sessions
                .iter_mut()
                .find(|session| session.id == session_id)?;
            let scene_index = session
                .scenes
                .iter()
                .position(|scene| scene.id == scene_id)?;
            let scene = &mut session.scenes[scene_index];
            if let Some(value) = name {
                scene.name = value;
            }
            if let Some(value) = layout {
                scene.layout = value;
            }
            if let Some(value) = source_ids {
                scene.source_ids = value;
            }
            if let Some(value) = active {
                scene.active = value;
                if value {
                    for (index, other) in session.scenes.iter_mut().enumerate() {
                        if index != scene_index {
                            other.active = false;
                        }
                    }
                    session.active_scene_id = Some(scene_id.into());
                }
            }
            session.updated_at = now();
            Some(session.scenes[scene_index].clone())
        })
        .await
    }

    pub async fn list_studio_sessions(&self) -> Vec<StudioSession> {
        self.data.read().await.studio_sessions.clone()
    }

    pub async fn list_brands(&self) -> Vec<Brand> {
        self.data.read().await.brands.clone()
    }

    pub async fn create_brand(&self, mut brand: Brand) -> Brand {
        brand.id = id();
        brand.created_at = now();
        brand.updated_at = brand.created_at;
        self.mutate(|data| data.brands.push(brand.clone())).await;
        brand
    }

    pub async fn update_brand(&self, brand_id: &str, patch: Value) -> Option<Brand> {
        self.mutate(|data| {
            let brand = data.brands.iter_mut().find(|brand| brand.id == brand_id)?;
            merge_value(brand, patch);
            brand.updated_at = now();
            Some(brand.clone())
        })
        .await
    }

    pub async fn delete_brand(&self, brand_id: &str) -> bool {
        self.mutate(|data| {
            let before = data.brands.len();
            data.brands.retain(|brand| brand.id != brand_id);
            before != data.brands.len()
        })
        .await
    }

    pub async fn list_captions(&self) -> Vec<Caption> {
        self.data.read().await.captions.clone()
    }

    pub async fn create_caption(&self, mut caption: Caption) -> Caption {
        caption.id = id();
        caption.created_at = now();
        caption.updated_at = caption.created_at;
        self.mutate(|data| data.captions.push(caption.clone()))
            .await;
        caption
    }

    pub async fn update_caption(&self, caption_id: &str, patch: Value) -> Option<Caption> {
        self.mutate(|data| {
            let caption = data
                .captions
                .iter_mut()
                .find(|caption| caption.id == caption_id)?;
            merge_value(caption, patch);
            caption.updated_at = now();
            Some(caption.clone())
        })
        .await
    }

    pub async fn delete_caption(&self, caption_id: &str) -> bool {
        self.mutate(|data| {
            let before = data.captions.len();
            data.captions.retain(|caption| caption.id != caption_id);
            before != data.captions.len()
        })
        .await
    }

    pub async fn list_qr_codes(&self) -> Vec<QrCode> {
        self.data.read().await.qr_codes.clone()
    }

    pub async fn create_qr_code(&self, mut qr_code: QrCode) -> QrCode {
        qr_code.id = id();
        qr_code.created_at = now();
        qr_code.updated_at = qr_code.created_at;
        self.mutate(|data| data.qr_codes.push(qr_code.clone()))
            .await;
        qr_code
    }

    pub async fn update_qr_code(&self, qr_id: &str, patch: Value) -> Option<QrCode> {
        self.mutate(|data| {
            let qr_code = data
                .qr_codes
                .iter_mut()
                .find(|qr_code| qr_code.id == qr_id)?;
            merge_value(qr_code, patch);
            qr_code.updated_at = now();
            Some(qr_code.clone())
        })
        .await
    }

    pub async fn delete_qr_code(&self, qr_id: &str) -> bool {
        self.mutate(|data| {
            let before = data.qr_codes.len();
            data.qr_codes.retain(|qr_code| qr_code.id != qr_id);
            before != data.qr_codes.len()
        })
        .await
    }

    pub async fn reorder_qr_codes(&self, ids: &[String]) -> Vec<QrCode> {
        self.mutate(|data| {
            let positions = ids
                .iter()
                .enumerate()
                .map(|(position, id)| (id.as_str(), position))
                .collect::<HashMap<_, _>>();
            data.qr_codes.sort_by_key(|qr_code| {
                positions
                    .get(qr_code.id.as_str())
                    .copied()
                    .unwrap_or(ids.len())
            });
            data.qr_codes.clone()
        })
        .await
    }

    pub async fn list_tickers(&self) -> Vec<Ticker> {
        self.data.read().await.tickers.clone()
    }

    pub async fn create_ticker(&self, mut ticker: Ticker) -> Ticker {
        ticker.id = id();
        ticker.created_at = now();
        ticker.updated_at = ticker.created_at;
        self.mutate(|data| data.tickers.push(ticker.clone())).await;
        ticker
    }

    pub async fn update_ticker(&self, ticker_id: &str, patch: Value) -> Option<Ticker> {
        self.mutate(|data| {
            let ticker = data
                .tickers
                .iter_mut()
                .find(|ticker| ticker.id == ticker_id)?;
            merge_value(ticker, patch);
            ticker.updated_at = now();
            Some(ticker.clone())
        })
        .await
    }

    pub async fn delete_ticker(&self, ticker_id: &str) -> bool {
        self.mutate(|data| {
            let before = data.tickers.len();
            data.tickers.retain(|ticker| ticker.id != ticker_id);
            before != data.tickers.len()
        })
        .await
    }

    pub async fn reorder_tickers(&self, ids: &[String]) -> Vec<Ticker> {
        self.mutate(|data| {
            let positions = ids
                .iter()
                .enumerate()
                .map(|(position, id)| (id.as_str(), position))
                .collect::<HashMap<_, _>>();
            data.tickers.sort_by_key(|ticker| {
                positions
                    .get(ticker.id.as_str())
                    .copied()
                    .unwrap_or(ids.len())
            });
            for (order, ticker) in data.tickers.iter_mut().enumerate() {
                ticker.order = order as u32;
                ticker.updated_at = now();
            }
            data.tickers.clone()
        })
        .await
    }

    pub async fn list_webhooks(&self) -> Vec<WebhookSubscription> {
        self.data.read().await.webhooks.clone()
    }

    pub async fn create_webhook(&self, mut webhook: WebhookSubscription) -> WebhookSubscription {
        webhook.id = id();
        webhook.created_at = now();
        webhook.updated_at = webhook.created_at;
        self.mutate(|data| data.webhooks.push(webhook.clone()))
            .await;
        webhook
    }

    pub async fn update_webhook(
        &self,
        webhook_id: &str,
        url: Option<String>,
        secret: Option<String>,
        events: Option<Vec<String>>,
        enabled: Option<bool>,
    ) -> Option<WebhookSubscription> {
        self.mutate(|data| {
            let webhook = data
                .webhooks
                .iter_mut()
                .find(|webhook| webhook.id == webhook_id)?;
            if let Some(value) = url {
                webhook.url = value;
            }
            if let Some(value) = secret {
                webhook.secret = Some(value);
            }
            if let Some(value) = events {
                webhook.events = value;
            }
            if let Some(value) = enabled {
                webhook.enabled = value;
            }
            webhook.updated_at = now();
            Some(webhook.clone())
        })
        .await
    }

    pub async fn delete_webhook(&self, webhook_id: &str) -> bool {
        self.mutate(|data| {
            let before = data.webhooks.len();
            data.webhooks.retain(|webhook| webhook.id != webhook_id);
            before != data.webhooks.len()
        })
        .await
    }

    async fn publish_event(&self, event_name: &str, event: &Event) {
        self.publish_notification(
            event_name,
            &event.id,
            serde_json::to_value(event).unwrap_or(Value::Null),
        )
        .await;
    }

    async fn publish_notification(&self, event_name: &str, event_id: &str, payload: Value) {
        let timestamp = now();
        let _ = self.event_tx.send(EventNotification {
            event: event_name.into(),
            event_id: event_id.into(),
            payload: payload.clone(),
            timestamp,
        });

        let webhooks = self
            .data
            .read()
            .await
            .webhooks
            .iter()
            .filter(|webhook| {
                webhook.enabled
                    && (webhook.events.is_empty()
                        || webhook.events.iter().any(|event| event == event_name))
            })
            .cloned()
            .collect::<Vec<_>>();
        for webhook in webhooks {
            let event_name = event_name.to_string();
            let event_id = event_id.to_string();
            let body = serde_json::json!({
                "event": event_name,
                "eventId": event_id,
                "timestamp": timestamp,
                "data": payload.clone(),
            });
            let body_bytes = serde_json::to_vec(&body).unwrap_or_default();
            let signature = webhook.secret.as_deref().and_then(|secret| {
                let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(secret.as_bytes()).ok()?;
                mac.update(&body_bytes);
                let digest = mac.finalize().into_bytes();
                let encoded = digest
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect::<String>();
                Some(format!("sha256={encoded}"))
            });
            tokio::spawn(async move {
                let client = match reqwest::Client::builder()
                    .timeout(Duration::from_secs(10))
                    .redirect(reqwest::redirect::Policy::none())
                    .build()
                {
                    Ok(client) => client,
                    Err(error) => {
                        warn!(webhook_id = %webhook.id, %error, "webhook client creation failed");
                        return;
                    }
                };
                for attempt in 0..3 {
                    let mut request = client
                        .post(&webhook.url)
                        .header("x-reestream-event", &event_name)
                        .header("x-reestream-event-id", &event_id)
                        .header("content-type", "application/json")
                        .body(body_bytes.clone());
                    if let Some(signature) = signature.as_deref() {
                        request = request.header("x-reestream-signature", signature);
                    }
                    match request.send().await {
                        Ok(response) if response.status().is_success() => return,
                        Ok(response) => {
                            if attempt == 2 {
                                warn!(webhook_id = %webhook.id, status = %response.status(), "webhook delivery returned an error");
                            }
                        }
                        Err(error) if attempt == 2 => {
                            warn!(webhook_id = %webhook.id, %error, "webhook delivery failed");
                        }
                        Err(_) => {}
                    }
                    tokio::time::sleep(Duration::from_millis(250 * (attempt + 1))).await;
                }
            });
        }
    }
}

#[async_trait::async_trait]
impl reestream_core::client::PublishKeyResolver for RestreamStore {
    async fn platforms_for_key(
        &self,
        stream_key: &str,
    ) -> Option<Vec<reestream_core::config::Platform>> {
        let runtime_key = self.runtime_stream_key().await;
        if !runtime_key.is_empty() && runtime_key == stream_key {
            let platforms = match &self.runtime_platforms {
                Some(platforms) => platforms.read().await.clone(),
                None => Vec::new(),
            };
            return Some(
                platforms
                    .into_iter()
                    .filter(|platform| platform.enabled)
                    .collect(),
            );
        }
        let data = self.data.read().await;
        let event_id = data
            .event_stream_keys
            .iter()
            .find_map(|(event_id, key)| (key == stream_key).then_some(event_id))?;
        let event = data.events.iter().find(|event| &event.id == event_id)?;
        let mut platforms = Vec::new();
        for channel_id in &event.destination_ids {
            let mut channel = data
                .channels
                .iter()
                .find(|channel| &channel.id == channel_id)
                .cloned()?;
            hydrate_channel(&data, &mut channel);
            if !channel.enabled {
                continue;
            }
            let url = url::Url::parse(&channel.stream_url).ok()?;
            platforms.push(reestream_core::config::Platform {
                url,
                key: channel.stream_key,
                enabled: true,
                orientation: reestream_core::config::Orientation::Horizontal,
            });
        }
        Some(platforms)
    }

    async fn input_url_for_key(&self, stream_key: &str) -> Option<String> {
        Some(format!(
            "{}/{}",
            self.runtime_ingest_url().await.trim_end_matches('/'),
            stream_key
        ))
    }
}

#[async_trait::async_trait]
impl reestream_core::client::DestinationStatusReporter for RestreamStore {
    async fn set_destination_status(
        &self,
        destination_id: &str,
        status: &str,
        last_error: Option<String>,
    ) {
        self.set_channel_status(destination_id, status, last_error)
            .await;
    }
}

fn connection_summary(connection: &OAuthConnection) -> ConnectionSummary {
    ConnectionSummary {
        id: connection.id.clone(),
        platform_id: connection.platform_id.clone(),
        status: if connection
            .expires_at
            .is_some_and(|expires_at| expires_at <= now())
        {
            "expired".into()
        } else {
            "connected".into()
        },
        scopes: connection.scopes.clone(),
        connected_at: connection.connected_at,
        updated_at: connection.updated_at,
    }
}

fn hydrate_channel(data: &RestreamData, channel: &mut Channel) {
    if let Some(secret) = data.channel_secrets.get(&channel.id) {
        channel.stream_key = secret.stream_key.clone();
        channel.rtmp_username = secret.rtmp_username.clone();
        channel.rtmp_password = secret.rtmp_password.clone();
    }
}

fn hydrate_event(data: &RestreamData, event: &mut Event) {
    if let Some(stream_key) = data.event_stream_keys.get(&event.id) {
        event.ingest.stream_key = stream_key.clone();
    }
}

fn merge_value<T>(target: &mut T, patch: Value)
where
    T: for<'de> Deserialize<'de> + Serialize,
{
    let Ok(mut current) = serde_json::to_value(&*target) else {
        return;
    };
    if let (Some(current), Some(patch)) = (current.as_object_mut(), patch.as_object()) {
        for (key, value) in patch {
            current.insert(key.clone(), value.clone());
        }
    }
    if let Ok(updated) = serde_json::from_value(current) {
        *target = updated;
    }
}

/// Keep this import visible to consumers that build a local API store from a
/// HashMap-backed fixture.  It also documents that IDs are opaque strings.
pub type JsonMap = HashMap<String, Value>;
