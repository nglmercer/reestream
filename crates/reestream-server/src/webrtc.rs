use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebRtcConfig {
    pub enabled: bool,
    pub port: u16,
    pub ice_servers: Vec<IceServer>,
    pub max_viewers: u32,
}

impl Default for WebRtcConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            port: 8443,
            ice_servers: vec![IceServer {
                urls: vec!["stun:stun.l.google.com:19302".into()],
                username: None,
                credential: None,
            }],
            max_viewers: 100,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IceServer {
    pub urls: Vec<String>,
    pub username: Option<String>,
    pub credential: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AbrConfig {
    pub enabled: bool,
    pub variants: Vec<AbrVariant>,
    pub segment_count: usize,
    pub segment_duration_secs: u32,
}

impl Default for AbrConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            variants: vec![
                AbrVariant::new("1080p", 1920, 1080, 5000, 60),
                AbrVariant::new("720p", 1280, 720, 2500, 30),
                AbrVariant::new("480p", 854, 480, 1000, 30),
                AbrVariant::new("360p", 640, 360, 500, 24),
            ],
            segment_count: 5,
            segment_duration_secs: 6,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AbrVariant {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub bitrate_kbps: u32,
    pub fps: u32,
}

impl AbrVariant {
    pub fn new(name: &str, width: u32, height: u32, bitrate_kbps: u32, fps: u32) -> Self {
        Self {
            name: name.to_string(),
            width,
            height,
            bitrate_kbps,
            fps,
        }
    }

    pub fn to_ffmpeg_args(&self, input: &str, output_dir: &str) -> Vec<String> {
        vec![
            "-i".into(),
            input.into(),
            "-c:v".into(),
            "libx264".into(),
            "-preset".into(),
            "veryfast".into(),
            "-b:v".into(),
            format!("{}k", self.bitrate_kbps),
            "-maxrate".into(),
            format!("{}k", self.bitrate_kbps),
            "-bufsize".into(),
            format!("{}k", self.bitrate_kbps * 2),
            "-vf".into(),
            format!("scale={}x{}", self.width, self.height),
            "-r".into(),
            self.fps.to_string(),
            "-c:a".into(),
            "aac".into(),
            "-b:a".into(),
            "128k".into(),
            "-f".into(),
            "hls".into(),
            "-hls_time".into(),
            "6".into(),
            "-hls_list_size".into(),
            "5".into(),
            format!("{}/{}.m3u8", output_dir, self.name),
        ]
    }
}

pub fn generate_master_playlist(variants: &[AbrVariant], base_url: &str) -> String {
    let mut playlist = String::from("#EXTM3U\n#EXT-X-VERSION:3\n");

    for variant in variants {
        playlist.push_str(&format!(
            "#EXT-X-STREAM-INF:BANDWIDTH={},RESOLUTION={}x{},NAME=\"{}\"\n",
            variant.bitrate_kbps * 1000,
            variant.width,
            variant.height,
            variant.name
        ));
        playlist.push_str(&format!("{}/{}.m3u8\n", base_url, variant.name));
    }

    playlist
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_webrtc_config_default() {
        let config = WebRtcConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.port, 8443);
        assert_eq!(config.ice_servers.len(), 1);
    }

    #[test]
    fn test_abr_config_default() {
        let config = AbrConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.variants.len(), 4);
    }

    #[test]
    fn test_abr_variant_ffmpeg_args() {
        let variant = AbrVariant::new("720p", 1280, 720, 2500, 30);
        let args = variant.to_ffmpeg_args("rtmp://input", "/tmp/hls");
        assert!(args.iter().any(|a| a.contains("1280x720")));
        assert!(args.contains(&"2500k".to_string()));
        assert!(args.iter().any(|a| a.contains("720p.m3u8")));
    }

    #[test]
    fn test_master_playlist_generation() {
        let variants = vec![
            AbrVariant::new("1080p", 1920, 1080, 5000, 60),
            AbrVariant::new("720p", 1280, 720, 2500, 30),
        ];
        let playlist = generate_master_playlist(&variants, "/hls");
        assert!(playlist.contains("#EXTM3U"));
        assert!(playlist.contains("BANDWIDTH=5000000"));
        assert!(playlist.contains("1080p.m3u8"));
        assert!(playlist.contains("720p.m3u8"));
    }
}
