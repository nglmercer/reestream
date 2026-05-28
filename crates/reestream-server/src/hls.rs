use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

#[derive(Debug, Clone)]
pub struct HlsConfig {
    pub segment_duration: u32,
    pub max_segments: usize,
    pub segment_dir: PathBuf,
    pub playlist_path: PathBuf,
    pub http_port: u16,
    pub http_addr: String,
}

impl Default for HlsConfig {
    fn default() -> Self {
        Self {
            segment_duration: 2,
            max_segments: 10,
            segment_dir: PathBuf::from("/tmp/reestream/hls"),
            playlist_path: PathBuf::from("/tmp/reestream/hls/stream.m3u8"),
            http_port: 8080,
            http_addr: "0.0.0.0".into(),
        }
    }
}

pub struct HlsSegmenter {
    config: HlsConfig,
    segments: Arc<RwLock<Vec<Segment>>>,
}

#[derive(Debug, Clone)]
pub struct Segment {
    pub index: u32,
    pub filename: String,
    pub duration: f64,
    pub byte_offset: u64,
    pub byte_length: u64,
}

impl HlsSegmenter {
    pub fn new(config: HlsConfig) -> Self {
        Self {
            config,
            segments: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub fn config(&self) -> &HlsConfig {
        &self.config
    }

    pub fn generate_playlist(&self, segments: &[Segment], is_live: bool) -> String {
        let mut playlist = String::new();
        playlist.push_str("#EXTM3U\n");
        playlist.push_str("#EXT-X-VERSION:3\n");
        playlist.push_str(&format!(
            "#EXT-X-TARGETDURATION:{}\n",
            self.config.segment_duration
        ));
        playlist.push_str("#EXT-X-MEDIA-SEQUENCE:0\n");

        if !is_live {
            playlist.push_str("#EXT-X-ENDLIST\n");
        }

        for segment in segments {
            playlist.push_str(&format!("#EXTINF:{:.3},\n", segment.duration));
            playlist.push_str(&format!("{}\n", segment.filename));
        }

        playlist
    }

    pub async fn add_segment(&self, segment: Segment) {
        let mut segments = self.segments.write().await;
        segments.push(segment);

        // Trim old segments
        if segments.len() > self.config.max_segments {
            let excess = segments.len() - self.config.max_segments;
            segments.drain(..excess);
        }
    }

    pub async fn get_segments(&self) -> Vec<Segment> {
        self.segments.read().await.clone()
    }

    pub async fn clear(&self) {
        self.segments.write().await.clear();
        info!("Cleared all HLS segments");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hls_config_default() {
        let config = HlsConfig::default();
        assert_eq!(config.segment_duration, 2);
        assert_eq!(config.max_segments, 10);
        assert_eq!(config.http_port, 8080);
    }

    #[test]
    fn test_generate_live_playlist() {
        let config = HlsConfig::default();
        let segmenter = HlsSegmenter::new(config);

        let segments = vec![
            Segment {
                index: 0,
                filename: "seg0.ts".into(),
                duration: 2.0,
                byte_offset: 0,
                byte_length: 1024,
            },
            Segment {
                index: 1,
                filename: "seg1.ts".into(),
                duration: 2.0,
                byte_offset: 1024,
                byte_length: 1024,
            },
        ];

        let playlist = segmenter.generate_playlist(&segments, true);
        assert!(playlist.contains("#EXTM3U"));
        assert!(playlist.contains("#EXT-X-TARGETDURATION:2"));
        assert!(playlist.contains("#EXTINF:2.000,"));
        assert!(playlist.contains("seg0.ts"));
        assert!(playlist.contains("seg1.ts"));
        assert!(!playlist.contains("#EXT-X-ENDLIST"));
    }

    #[test]
    fn test_generate_vod_playlist() {
        let config = HlsConfig::default();
        let segmenter = HlsSegmenter::new(config);

        let segments = vec![Segment {
            index: 0,
            filename: "seg0.ts".into(),
            duration: 2.5,
            byte_offset: 0,
            byte_length: 1024,
        }];

        let playlist = segmenter.generate_playlist(&segments, false);
        assert!(playlist.contains("#EXT-X-ENDLIST"));
        assert!(playlist.contains("#EXTINF:2.500,"));
    }

    #[tokio::test]
    async fn test_add_and_get_segments() {
        let config = HlsConfig::default();
        let segmenter = HlsSegmenter::new(config);

        segmenter
            .add_segment(Segment {
                index: 0,
                filename: "seg0.ts".into(),
                duration: 2.0,
                byte_offset: 0,
                byte_length: 1024,
            })
            .await;

        let segments = segmenter.get_segments().await;
        assert_eq!(segments.len(), 1);
    }

    #[tokio::test]
    async fn test_trim_old_segments() {
        let mut config = HlsConfig::default();
        config.max_segments = 3;
        let segmenter = HlsSegmenter::new(config);

        for i in 0..5 {
            segmenter
                .add_segment(Segment {
                    index: i,
                    filename: format!("seg{}.ts", i),
                    duration: 2.0,
                    byte_offset: 0,
                    byte_length: 1024,
                })
                .await;
        }

        let segments = segmenter.get_segments().await;
        assert_eq!(segments.len(), 3);
        assert_eq!(segments[0].index, 2); // First two trimmed
    }

    #[tokio::test]
    async fn test_clear_segments() {
        let config = HlsConfig::default();
        let segmenter = HlsSegmenter::new(config);

        segmenter
            .add_segment(Segment {
                index: 0,
                filename: "seg0.ts".into(),
                duration: 2.0,
                byte_offset: 0,
                byte_length: 1024,
            })
            .await;

        segmenter.clear().await;
        assert!(segmenter.get_segments().await.is_empty());
    }
}
