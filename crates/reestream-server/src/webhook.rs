use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::{error, info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookConfig {
    pub enabled: bool,
    pub url: String,
    pub secret: Option<String>,
    pub timeout_secs: u64,
    pub on_stream_start: bool,
    pub on_stream_end: bool,
    pub on_stream_error: bool,
    pub on_viewer_connect: bool,
    pub on_viewer_disconnect: bool,
}

impl Default for WebhookConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            url: String::new(),
            secret: None,
            timeout_secs: 10,
            on_stream_start: true,
            on_stream_end: true,
            on_stream_error: true,
            on_viewer_connect: false,
            on_viewer_disconnect: false,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct WebhookPayload {
    pub event: WebhookEvent,
    pub stream_id: String,
    pub timestamp: u64,
    pub data: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum WebhookEvent {
    StreamStart,
    StreamEnd,
    StreamError,
    ViewerConnect,
    ViewerDisconnect,
}

pub struct WebhookSender {
    config: WebhookConfig,
}

impl WebhookSender {
    pub fn new(config: WebhookConfig) -> Self {
        Self { config }
    }

    pub fn should_send(&self, event: &WebhookEvent) -> bool {
        if !self.config.enabled {
            return false;
        }
        match event {
            WebhookEvent::StreamStart => self.config.on_stream_start,
            WebhookEvent::StreamEnd => self.config.on_stream_end,
            WebhookEvent::StreamError => self.config.on_stream_error,
            WebhookEvent::ViewerConnect => self.config.on_viewer_connect,
            WebhookEvent::ViewerDisconnect => self.config.on_viewer_disconnect,
        }
    }

    pub async fn send(&self, payload: &WebhookPayload) -> Result<(), String> {
        if !self.should_send(&payload.event) {
            return Ok(());
        }

        let client = crate::restream::build_safe_http_client(
            &self.config.url,
            Duration::from_secs(self.config.timeout_secs.max(1)),
        )
        .await?;

        let mut request = client
            .post(&self.config.url)
            .json(payload)
            .header("Content-Type", "application/json");

        if let Some(ref secret) = self.config.secret {
            request = request.header("X-Webhook-Secret", secret);
        }

        match request.send().await {
            Ok(response) => {
                if response.status().is_success() {
                    info!(
                        "Webhook sent successfully: {:?} for stream {}",
                        payload.event, payload.stream_id
                    );
                    Ok(())
                } else {
                    let status = response.status();
                    warn!(
                        "Webhook returned non-success status: {} for event {:?}",
                        status, payload.event
                    );
                    Err(format!("Webhook returned status {status}"))
                }
            }
            Err(e) => {
                error!("Webhook send failed: {}", e);
                Err(format!("Webhook send failed: {e}"))
            }
        }
    }
}

pub fn create_payload(
    event: WebhookEvent,
    stream_id: String,
    data: serde_json::Value,
) -> WebhookPayload {
    WebhookPayload {
        event,
        stream_id,
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        data,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_webhook_config_default() {
        let config = WebhookConfig::default();
        assert!(!config.enabled);
        assert!(config.url.is_empty());
        assert_eq!(config.timeout_secs, 10);
        assert!(config.on_stream_start);
        assert!(config.on_stream_end);
        assert!(config.on_stream_error);
        assert!(!config.on_viewer_connect);
        assert!(!config.on_viewer_disconnect);
    }

    #[test]
    fn test_webhook_sender_should_send_disabled() {
        let config = WebhookConfig::default();
        let sender = WebhookSender::new(config);
        assert!(!sender.should_send(&WebhookEvent::StreamStart));
    }

    #[test]
    fn test_webhook_sender_should_send_enabled() {
        let config = WebhookConfig {
            enabled: true,
            url: "http://example.com".into(),
            ..Default::default()
        };
        let sender = WebhookSender::new(config);
        assert!(sender.should_send(&WebhookEvent::StreamStart));
        assert!(sender.should_send(&WebhookEvent::StreamEnd));
        assert!(sender.should_send(&WebhookEvent::StreamError));
        assert!(!sender.should_send(&WebhookEvent::ViewerConnect));
    }

    #[test]
    fn test_create_payload() {
        let payload = create_payload(
            WebhookEvent::StreamStart,
            "test-stream".into(),
            serde_json::json!({"name": "test"}),
        );
        assert_eq!(payload.event, WebhookEvent::StreamStart);
        assert_eq!(payload.stream_id, "test-stream");
        assert!(payload.timestamp > 0);
    }

    #[test]
    fn test_webhook_event_serialize() {
        let event = WebhookEvent::StreamStart;
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("StreamStart"));
    }

    #[test]
    fn test_webhook_config_serialize() {
        let config = WebhookConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("enabled"));
        assert!(json.contains("timeout_secs"));
    }

    #[test]
    fn test_webhook_payload_serialize() {
        let payload = create_payload(
            WebhookEvent::StreamEnd,
            "stream-1".into(),
            serde_json::json!({}),
        );
        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("StreamEnd"));
        assert!(json.contains("stream-1"));
    }
}
