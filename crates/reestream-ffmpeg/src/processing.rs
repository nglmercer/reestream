use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessConfig {
    pub enabled: bool,
    pub profiles: Vec<TranscodeProfile>,
    pub watermark: Option<WatermarkConfig>,
    pub thumbnail: Option<ThumbnailConfig>,
}

impl Default for ProcessConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            profiles: vec![
                TranscodeProfile::new("720p", "1280x720", "2500k", "128k"),
                TranscodeProfile::new("480p", "854x480", "1000k", "96k"),
            ],
            watermark: None,
            thumbnail: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscodeProfile {
    pub name: String,
    pub resolution: String,
    pub video_bitrate: String,
    pub audio_bitrate: String,
    pub codec: String,
    pub preset: String,
}

impl TranscodeProfile {
    pub fn new(name: &str, resolution: &str, video_bitrate: &str, audio_bitrate: &str) -> Self {
        Self {
            name: name.to_string(),
            resolution: resolution.to_string(),
            video_bitrate: video_bitrate.to_string(),
            audio_bitrate: audio_bitrate.to_string(),
            codec: "libx264".to_string(),
            preset: "veryfast".to_string(),
        }
    }

    pub fn to_ffmpeg_args(&self, input: &str, output: &str) -> Vec<String> {
        vec![
            "-i".into(),
            input.into(),
            "-c:v".into(),
            self.codec.clone(),
            "-preset".into(),
            self.preset.clone(),
            "-b:v".into(),
            self.video_bitrate.clone(),
            "-maxrate".into(),
            self.video_bitrate.clone(),
            "-bufsize".into(),
            self.video_bitrate.clone(),
            "-vf".into(),
            format!("scale={}", self.resolution),
            "-c:a".into(),
            "aac".into(),
            "-b:a".into(),
            self.audio_bitrate.clone(),
            "-f".into(),
            "flv".into(),
            output.into(),
        ]
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatermarkConfig {
    pub image_path: PathBuf,
    pub position: WatermarkPosition,
    pub opacity: f32,
    pub scale: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WatermarkPosition {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    Center,
}

impl WatermarkPosition {
    pub fn to_overlay(&self, margin: u32) -> String {
        match self {
            Self::TopLeft => format!("{margin}:{margin}"),
            Self::TopRight => format!("main_w-overlay_w-{margin}:{margin}"),
            Self::BottomLeft => format!("{margin}:main_h-overlay_h-{margin}"),
            Self::BottomRight => {
                format!("main_w-overlay_w-{margin}:main_h-overlay_h-{margin}")
            }
            Self::Center => "(main_w-overlay_w)/2:(main_h-overlay_h)/2".to_string(),
        }
    }
}

impl WatermarkConfig {
    pub fn to_filter(&self) -> String {
        let overlay = self.position.to_overlay(10);
        format!(
            "movie={}[wm];[in][wm]overlay={}:format=auto",
            self.image_path.display(),
            overlay
        )
    }

    pub fn to_ffmpeg_args(&self, input: &str, output: &str) -> Vec<String> {
        vec![
            "-i".into(),
            input.into(),
            "-i".into(),
            self.image_path.to_string_lossy().to_string(),
            "-filter_complex".into(),
            format!(
                "[1:v]scale=iw*{}:ih*{},format=rgba,colorchannelmixer=aa={}[wm];[0:v][wm]overlay={}",
                self.scale,
                self.scale,
                self.opacity,
                self.position.to_overlay(10)
            ),
            "-c:v".into(),
            "libx264".into(),
            "-preset".into(),
            "veryfast".into(),
            "-c:a".into(),
            "copy".into(),
            "-f".into(),
            "flv".into(),
            output.into(),
        ]
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThumbnailConfig {
    pub interval_secs: u32,
    pub output_dir: PathBuf,
    pub width: u32,
    pub height: u32,
    pub quality: u32,
}

impl Default for ThumbnailConfig {
    fn default() -> Self {
        Self {
            interval_secs: 10,
            output_dir: PathBuf::from("/tmp/reestream/thumbnails"),
            width: 320,
            height: 180,
            quality: 2,
        }
    }
}

impl ThumbnailConfig {
    pub fn to_ffmpeg_args(&self, input: &str) -> Vec<String> {
        vec![
            "-i".into(),
            input.into(),
            "-vf".into(),
            format!(
                "fps=1/{},scale={}:{}",
                self.interval_secs, self.width, self.height
            ),
            "-q:v".into(),
            self.quality.to_string(),
            self.output_dir
                .join("thumb_%04d.jpg")
                .to_string_lossy()
                .to_string(),
        ]
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResizeConfig {
    pub width: u32,
    pub height: u32,
    pub maintain_aspect: bool,
}

impl ResizeConfig {
    pub fn to_filter(&self) -> String {
        if self.maintain_aspect {
            format!(
                "scale={}:{}:force_original_aspect_ratio=decrease",
                self.width, self.height
            )
        } else {
            format!("scale={}:{}", self.width, self.height)
        }
    }
}

pub struct StreamProcessor {
    config: ProcessConfig,
}

impl StreamProcessor {
    pub fn new(config: ProcessConfig) -> Self {
        Self { config }
    }

    pub fn build_transcode_args(
        &self,
        input: &str,
        output: &str,
        profile_name: &str,
    ) -> Option<Vec<String>> {
        let profile = self
            .config
            .profiles
            .iter()
            .find(|p| p.name == profile_name)?;
        Some(profile.to_ffmpeg_args(input, output))
    }

    pub fn build_watermark_args(&self, input: &str, output: &str) -> Option<Vec<String>> {
        let wm = self.config.watermark.as_ref()?;
        Some(wm.to_ffmpeg_args(input, output))
    }

    pub fn build_thumbnail_args(&self, input: &str) -> Option<Vec<String>> {
        let thumb = self.config.thumbnail.as_ref()?;
        Some(thumb.to_ffmpeg_args(input))
    }

    pub fn list_profiles(&self) -> &[TranscodeProfile] {
        &self.config.profiles
    }

    pub fn has_watermark(&self) -> bool {
        self.config.watermark.is_some()
    }

    pub fn has_thumbnail(&self) -> bool {
        self.config.thumbnail.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transcode_profile_default() {
        let p = TranscodeProfile::new("720p", "1280x720", "2500k", "128k");
        assert_eq!(p.name, "720p");
        assert_eq!(p.resolution, "1280x720");
        assert_eq!(p.codec, "libx264");
    }

    #[test]
    fn test_transcode_profile_args() {
        let p = TranscodeProfile::new("480p", "854x480", "1000k", "96k");
        let args = p.to_ffmpeg_args("rtmp://input", "rtmp://output");
        assert!(args.contains(&"-c:v".to_string()));
        assert!(args.contains(&"libx264".to_string()));
        assert!(args.iter().any(|a| a.contains("854x480")));
    }

    #[test]
    fn test_watermark_position_overlay() {
        assert_eq!(WatermarkPosition::TopLeft.to_overlay(10), "10:10");
        assert_eq!(
            WatermarkPosition::Center.to_overlay(10),
            "(main_w-overlay_w)/2:(main_h-overlay_h)/2"
        );
    }

    #[test]
    fn test_watermark_config_filter() {
        let wm = WatermarkConfig {
            image_path: PathBuf::from("/tmp/logo.png"),
            position: WatermarkPosition::BottomRight,
            opacity: 0.8,
            scale: 0.5,
        };
        let filter = wm.to_filter();
        assert!(filter.contains("logo.png"));
        assert!(filter.contains("overlay="));
    }

    #[test]
    fn test_thumbnail_config_default() {
        let config = ThumbnailConfig::default();
        assert_eq!(config.interval_secs, 10);
        assert_eq!(config.width, 320);
    }

    #[test]
    fn test_thumbnail_ffmpeg_args() {
        let config = ThumbnailConfig::default();
        let args = config.to_ffmpeg_args("rtmp://input");
        assert!(args.contains(&"-vf".to_string()));
        assert!(args.iter().any(|a| a.contains("thumb_")));
    }

    #[test]
    fn test_resize_config_filter() {
        let resize = ResizeConfig {
            width: 1280,
            height: 720,
            maintain_aspect: true,
        };
        assert!(resize.to_filter().contains("force_original_aspect_ratio"));

        let resize2 = ResizeConfig {
            width: 1920,
            height: 1080,
            maintain_aspect: false,
        };
        assert!(!resize2.to_filter().contains("force_original_aspect_ratio"));
    }

    #[test]
    fn test_process_config_default() {
        let config = ProcessConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.profiles.len(), 2);
        assert!(config.watermark.is_none());
    }

    #[test]
    fn test_stream_processor_profiles() {
        let processor = StreamProcessor::new(ProcessConfig::default());
        assert_eq!(processor.list_profiles().len(), 2);
        assert!(!processor.has_watermark());
    }

    #[test]
    fn test_stream_processor_transcode() {
        let processor = StreamProcessor::new(ProcessConfig::default());
        let args = processor.build_transcode_args("rtmp://in", "rtmp://out", "720p");
        assert!(args.is_some());
        let args = processor.build_transcode_args("rtmp://in", "rtmp://out", "nonexistent");
        assert!(args.is_none());
    }
}
