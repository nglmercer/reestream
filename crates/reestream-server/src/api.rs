use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct ApiResponse<T: Serialize> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<String>,
}

impl<T: Serialize> ApiResponse<T> {
    pub fn ok(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
        }
    }

    pub fn err(msg: impl Into<String>) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(msg.into()),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ServerStatus {
    pub version: String,
    pub uptime_seconds: u64,
    pub active_streams: u32,
    pub total_viewers: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AddPlatformRequest {
    pub name: String,
    pub url: String,
    pub key: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AddStreamRequest {
    pub name: String,
    pub input_url: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateConfigRequest {
    pub stream_key: Option<String>,
    pub platforms: Option<Vec<AddPlatformRequest>>,
}

pub const API_ROUTES: &[(&str, &str)] = &[
    ("GET", "/"),
    ("GET", "/dashboard"),
    ("GET", "/health"),
    ("GET", "/api/status"),
    ("GET", "/api/streams"),
    ("POST", "/api/streams"),
    ("DELETE", "/api/streams/:id"),
    ("GET", "/api/streams/:id/stats"),
    ("GET", "/api/config"),
    ("PUT", "/api/config"),
    ("POST", "/api/config/reload"),
    ("GET", "/api/platforms"),
    ("POST", "/api/platforms"),
    ("DELETE", "/api/platforms/:id"),
    ("PUT", "/api/platforms/:id/toggle"),
    ("GET", "/stream.m3u8"),
    ("GET", "/hls/:filename"),
    ("GET", "/stream.flv"),
    ("GET", "/metrics"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_response_ok() {
        let resp = ApiResponse::ok("test");
        assert!(resp.success);
        assert_eq!(resp.data.unwrap(), "test");
        assert!(resp.error.is_none());
    }

    #[test]
    fn test_api_response_err() {
        let resp: ApiResponse<()> = ApiResponse::err("something failed");
        assert!(!resp.success);
        assert!(resp.data.is_none());
        assert_eq!(resp.error.unwrap(), "something failed");
    }

    #[test]
    fn test_api_response_serialize() {
        let resp = ApiResponse::ok(42);
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("true"));
        assert!(json.contains("42"));
    }

    #[test]
    fn test_server_status_serialize() {
        let status = ServerStatus {
            version: "0.2.0".into(),
            uptime_seconds: 3600,
            active_streams: 2,
            total_viewers: 150,
        };
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("0.2.0"));
        assert!(json.contains("3600"));
    }

    #[test]
    fn test_add_platform_request_deserialize() {
        let json = r#"{"name":"Twitch","url":"rtmp://twitch.tv","key":"abc"}"#;
        let req: AddPlatformRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.name, "Twitch");
    }

    #[test]
    fn test_api_routes_count() {
        assert_eq!(API_ROUTES.len(), 19);
    }
}
