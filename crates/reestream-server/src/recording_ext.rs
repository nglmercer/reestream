use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ScheduleConfig {
    pub enabled: bool,
    pub schedules: Vec<ScheduledRecording>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledRecording {
    pub id: String,
    pub name: String,
    pub input_url: String,
    pub start_time: Option<String>,
    pub duration_secs: Option<u64>,
    pub cron: Option<String>,
    pub output_dir: PathBuf,
    pub format: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RotationConfig {
    pub enabled: bool,
    pub max_duration_secs: u64,
    pub max_size_mb: u64,
    pub max_files: usize,
    pub output_dir: PathBuf,
}

impl Default for RotationConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            max_duration_secs: 3600,
            max_size_mb: 2048,
            max_files: 10,
            output_dir: PathBuf::from("/tmp/reestream/recordings"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct S3Config {
    pub enabled: bool,
    pub bucket: String,
    pub region: String,
    pub endpoint: Option<String>,
    pub access_key: String,
    pub secret_key: String,
    pub prefix: String,
    pub auto_upload: bool,
}

impl Default for S3Config {
    fn default() -> Self {
        Self {
            enabled: false,
            bucket: String::new(),
            region: "us-east-1".into(),
            endpoint: None,
            access_key: String::new(),
            secret_key: String::new(),
            prefix: "recordings/".into(),
            auto_upload: false,
        }
    }
}

impl S3Config {
    pub fn validate(&self) -> Result<(), String> {
        if self.bucket.is_empty() {
            return Err("S3 bucket cannot be empty".into());
        }
        if self.access_key.is_empty() || self.secret_key.is_empty() {
            return Err("S3 credentials cannot be empty".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormatConvertConfig {
    pub input_format: String,
    pub output_format: String,
    pub output_dir: PathBuf,
    pub delete_original: bool,
}

impl Default for FormatConvertConfig {
    fn default() -> Self {
        Self {
            input_format: "flv".into(),
            output_format: "mp4".into(),
            output_dir: PathBuf::from("/tmp/reestream/converted"),
            delete_original: false,
        }
    }
}

impl FormatConvertConfig {
    pub fn to_ffmpeg_args(&self, input: &str, output: &str) -> Vec<String> {
        vec![
            "-i".into(),
            input.into(),
            "-c".into(),
            "copy".into(),
            "-movflags".into(),
            "+faststart".into(),
            output.into(),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rotation_config_default() {
        let config = RotationConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.max_duration_secs, 3600);
        assert_eq!(config.max_files, 10);
    }

    #[test]
    fn test_s3_config_default() {
        let config = S3Config::default();
        assert!(!config.enabled);
        assert_eq!(config.region, "us-east-1");
    }

    #[test]
    fn test_s3_config_validate_empty_bucket() {
        let config = S3Config::default();
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_s3_config_validate_ok() {
        let config = S3Config {
            bucket: "my-bucket".into(),
            access_key: "AKIA...".into(),
            secret_key: "secret".into(),
            ..Default::default()
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_format_convert_default() {
        let config = FormatConvertConfig::default();
        assert_eq!(config.input_format, "flv");
        assert_eq!(config.output_format, "mp4");
        assert!(!config.delete_original);
    }

    #[test]
    fn test_format_convert_ffmpeg_args() {
        let config = FormatConvertConfig::default();
        let args = config.to_ffmpeg_args("/tmp/input.flv", "/tmp/output.mp4");
        assert!(args.contains(&"-c".to_string()));
        assert!(args.contains(&"copy".to_string()));
        assert!(args.contains(&"-movflags".to_string()));
    }

    #[test]
    fn test_schedule_config_default() {
        let config = ScheduleConfig::default();
        assert!(!config.enabled);
        assert!(config.schedules.is_empty());
    }
}
