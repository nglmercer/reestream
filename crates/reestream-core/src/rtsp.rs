use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RtspConfig {
    pub enabled: bool,
    pub listen_port: u16,
    pub listen_addr: String,
    pub auth: Option<RtspAuth>,
    pub transport: RtspTransport,
}

impl Default for RtspConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            listen_port: 8554,
            listen_addr: "0.0.0.0".into(),
            auth: None,
            transport: RtspTransport::Tcp,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RtspAuth {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RtspTransport {
    Tcp,
    Udp,
}

impl RtspConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self.listen_port == 0 {
            return Err("RTSP port cannot be 0".into());
        }
        Ok(())
    }
}

pub struct RtspInput {
    config: RtspConfig,
}

impl RtspInput {
    pub fn new(config: RtspConfig) -> Self {
        Self { config }
    }

    pub fn build_ffmpeg_input_args(&self, url: &str) -> Vec<String> {
        let mut args = vec!["-rtsp_transport".into()];
        match self.config.transport {
            RtspTransport::Tcp => args.push("tcp".into()),
            RtspTransport::Udp => args.push("udp".into()),
        }
        args.extend(["-i".into(), url.into()]);
        args
    }

    pub fn build_restream_args(&self, input_url: &str, output_url: &str) -> Vec<String> {
        let mut args = self.build_ffmpeg_input_args(input_url);
        args.extend([
            "-c".into(),
            "copy".into(),
            "-f".into(),
            "flv".into(),
            output_url.into(),
        ]);
        args
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rtsp_config_default() {
        let config = RtspConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.listen_port, 8554);
    }

    #[test]
    fn test_rtsp_config_validate() {
        let config = RtspConfig::default();
        assert!(config.validate().is_ok());

        let bad = RtspConfig {
            listen_port: 0,
            ..Default::default()
        };
        assert!(bad.validate().is_err());
    }

    #[test]
    fn test_rtsp_input_build_args() {
        let input = RtspInput::new(RtspConfig::default());
        let args = input.build_ffmpeg_input_args("rtsp://camera:554/stream");
        assert!(args.contains(&"-rtsp_transport".to_string()));
        assert!(args.contains(&"tcp".to_string()));
        assert!(args.contains(&"rtsp://camera:554/stream".to_string()));
    }

    #[test]
    fn test_rtsp_input_restream_args() {
        let input = RtspInput::new(RtspConfig::default());
        let args = input.build_restream_args("rtsp://cam:554/live", "rtmp://server/live/key");
        assert!(args.contains(&"-c".to_string()));
        assert!(args.contains(&"copy".to_string()));
        assert!(args.contains(&"rtmp://server/live/key".to_string()));
    }
}
