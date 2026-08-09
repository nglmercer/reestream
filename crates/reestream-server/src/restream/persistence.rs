//! Encrypted state-file persistence for the product store.

use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::HashMap, io::Write, path::Path, sync::Arc};
use tracing::warn;
use uuid::Uuid;

use super::{ChannelSecret, OAuthConnection, RestreamData, now};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub(super) struct SensitiveState {
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

pub(super) fn encrypted_sensitive_state(data: &RestreamData, key: &[u8; 32]) -> Option<Vec<u8>> {
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

pub(super) fn load_or_create_state_key(path: &Path) -> Option<Arc<[u8; 32]>> {
    if let Ok(value) = std::env::var("RESTREAM_STATE_KEY") {
        if let Some(key) = decode_hex_key(&value) {
            return Some(Arc::new(key));
        }
        warn!(
            "RESTREAM_STATE_KEY must contain exactly 64 hexadecimal characters; using the sidecar key instead"
        );
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

pub(super) fn load_state(path: Option<&Path>) -> (RestreamData, Option<SensitiveState>) {
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

pub(super) fn apply_sensitive_state(data: &mut RestreamData, sensitive: SensitiveState) {
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

pub(super) fn load_sensitive_state(path: &Path, key: &[u8; 32]) -> Option<SensitiveState> {
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

pub(super) fn write_state_files(
    path: &Path,
    public: &[u8],
    sensitive: Option<&[u8]>,
) -> std::io::Result<()> {
    atomic_write(path, public, 0o600)?;
    if let Some(sensitive) = sensitive {
        atomic_write(&path.with_extension("secrets"), sensitive, 0o600)?;
    }
    Ok(())
}
