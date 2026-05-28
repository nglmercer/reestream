use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct FfmpegCommand {
    pub ffmpeg_path: PathBuf,
    pub input: InputSource,
    pub outputs: Vec<Output>,
    pub global_args: Vec<String>,
    pub hw_accel: Option<HardwareAccel>,
}

#[derive(Debug, Clone)]
pub enum InputSource {
    Rtmp { url: String },
    File { path: PathBuf },
    Pipe,
}

#[derive(Debug, Clone)]
pub struct Output {
    pub destination: OutputDestination,
    pub codec_args: Vec<String>,
    pub format_args: Vec<String>,
}

#[derive(Debug, Clone)]
pub enum OutputDestination {
    Hls {
        segment_path: PathBuf,
        playlist_path: PathBuf,
    },
    File {
        path: PathBuf,
    },
    Rtmp {
        url: String,
    },
    FlvHttp {
        endpoint: String,
    },
    Pipe,
}

#[derive(Debug, Clone)]
pub enum HardwareAccel {
    Vaapi,
    Nvenc,
    VideoToolbox,
    Mmal,
}

impl FfmpegCommand {
    pub fn new(ffmpeg_path: PathBuf, input: InputSource) -> Self {
        Self {
            ffmpeg_path,
            input,
            outputs: Vec::new(),
            global_args: Vec::new(),
            hw_accel: None,
        }
    }

    pub fn global_arg(mut self, arg: impl Into<String>) -> Self {
        self.global_args.push(arg.into());
        self
    }

    pub fn hw_accel(mut self, accel: HardwareAccel) -> Self {
        self.hw_accel = Some(accel);
        self
    }

    pub fn add_output(mut self, output: Output) -> Self {
        self.outputs.push(output);
        self
    }

    pub fn passthrough_to_rtmp(self, url: &str) -> Self {
        self.add_output(Output {
            destination: OutputDestination::Rtmp { url: url.to_string() },
            codec_args: vec!["-c", "copy"].into_iter().map(String::from).collect(),
            format_args: vec!["-f", "flv"].into_iter().map(String::from).collect(),
        })
    }

    pub fn to_hls(self, segment_path: PathBuf, playlist_path: PathBuf) -> Self {
        self.add_output(Output {
            destination: OutputDestination::Hls {
                segment_path,
                playlist_path,
            },
            codec_args: vec!["-c", "copy"].into_iter().map(String::from).collect(),
            format_args: vec![
                "-f", "hls",
                "-hls_time", "2",
                "-hls_list_size", "10",
                "-hls_flags", "delete_segments",
            ]
            .into_iter()
            .map(String::from)
            .collect(),
        })
    }

    pub fn to_flv_http(self, endpoint: &str) -> Self {
        self.add_output(Output {
            destination: OutputDestination::FlvHttp {
                endpoint: endpoint.to_string(),
            },
            codec_args: vec!["-c", "copy"].into_iter().map(String::from).collect(),
            format_args: vec!["-f", "flv"].into_iter().map(String::from).collect(),
        })
    }

    pub fn transcode(
        self,
        output: OutputDestination,
        resolution: &str,
        bitrate: &str,
    ) -> Self {
        self.add_output(Output {
            destination: output,
            codec_args: vec![
                "-c:v", "libx264",
                "-preset", "veryfast",
                "-b:v", bitrate,
                "-maxrate", bitrate,
                "-bufsize", bitrate,
                "-vf", &format!("scale={resolution}"),
                "-c:a", "aac",
                "-b:a", "128k",
            ]
            .into_iter()
            .map(String::from)
            .collect(),
            format_args: vec![],
        })
    }

    pub fn build_args(&self) -> Vec<String> {
        let mut args: Vec<String> = Vec::new();

        // Global args
        args.extend(self.global_args.clone());

        // Hardware acceleration
        match &self.hw_accel {
            Some(HardwareAccel::Vaapi) => {
                args.extend(["-vaapi_device".into(), "/dev/dri/renderD128".into()]);
                args.extend(["-hwaccel".into(), "vaapi".into()]);
                args.extend(["-hwaccel_output_format".into(), "vaapi".into()]);
            }
            Some(HardwareAccel::Nvenc) => {
                args.extend(["-hwaccel".into(), "cuda".into()]);
            }
            Some(HardwareAccel::VideoToolbox) => {
                args.extend(["-hwaccel".into(), "videotoolbox".into()]);
            }
            Some(HardwareAccel::Mmal) => {
                args.extend(["-hwaccel".into(), "mmal".into()]);
            }
            None => {}
        }

        // Input
        match &self.input {
            InputSource::Rtmp { url } => {
                args.extend([
                    "-listen".into(),
                    "1".into(),
                    "-i".into(),
                    url.clone(),
                ]);
            }
            InputSource::File { path } => {
                args.extend(["-i".into(), path.to_string_lossy().to_string()]);
            }
            InputSource::Pipe => {
                args.extend(["-i".into(), "pipe:0".into()]);
            }
        }

        // Outputs
        for output in &self.outputs {
            args.extend(output.codec_args.clone());

            match &output.destination {
                OutputDestination::Hls {
                    segment_path: _,
                    playlist_path,
                } => {
                    args.extend(output.format_args.clone());
                    args.push(playlist_path.to_string_lossy().to_string());
                    // HLS segment pattern is derived from playlist path
                }
                OutputDestination::File { path } => {
                    args.extend(output.format_args.clone());
                    args.push(path.to_string_lossy().to_string());
                }
                OutputDestination::Rtmp { url } => {
                    args.extend(output.format_args.clone());
                    args.push(url.clone());
                }
                OutputDestination::FlvHttp { endpoint } => {
                    args.extend(output.format_args.clone());
                    args.push(endpoint.clone());
                }
                OutputDestination::Pipe => {
                    args.extend(output.format_args.clone());
                    args.push("pipe:1".into());
                }
            }
        }

        args
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_passthrough_command() {
        let cmd = FfmpegCommand::new(
            PathBuf::from("ffmpeg"),
            InputSource::Rtmp {
                url: "rtmp://0.0.0.0:1935/live".into(),
            },
        )
        .passthrough_to_rtmp("rtmp://live.twitch.tv/app/key");

        let args = cmd.build_args();
        assert!(args.contains(&"-i".to_string()));
        assert!(args.contains(&"-c".to_string()));
        assert!(args.contains(&"copy".to_string()));
        assert!(args.contains(&"-f".to_string()));
        assert!(args.contains(&"flv".to_string()));
        assert!(args.contains(&"rtmp://live.twitch.tv/app/key".to_string()));
    }

    #[test]
    fn test_build_hls_command() {
        let cmd = FfmpegCommand::new(
            PathBuf::from("ffmpeg"),
            InputSource::Rtmp {
                url: "rtmp://0.0.0.0:1935/live".into(),
            },
        )
        .to_hls(
            PathBuf::from("/tmp/segments"),
            PathBuf::from("/tmp/playlist.m3u8"),
        );

        let args = cmd.build_args();
        assert!(args.contains(&"-f".to_string()));
        assert!(args.contains(&"hls".to_string()));
        assert!(args.contains(&"-hls_time".to_string()));
        assert!(args.contains(&"2".to_string()));
    }

    #[test]
    fn test_build_transcode_command() {
        let cmd = FfmpegCommand::new(
            PathBuf::from("ffmpeg"),
            InputSource::Rtmp {
                url: "rtmp://0.0.0.0:1935/live".into(),
            },
        )
        .transcode(
            OutputDestination::File {
                path: PathBuf::from("/tmp/output.mp4"),
            },
            "1280x720",
            "2500k",
        );

        let args = cmd.build_args();
        assert!(args.contains(&"-c:v".to_string()));
        assert!(args.contains(&"libx264".to_string()));
        assert!(args.contains(&"-b:v".to_string()));
        assert!(args.contains(&"2500k".to_string()));
    }

    #[test]
    fn test_build_hwaccel_nvenc() {
        let cmd = FfmpegCommand::new(
            PathBuf::from("ffmpeg"),
            InputSource::Rtmp {
                url: "rtmp://0.0.0.0:1935/live".into(),
            },
        )
        .hw_accel(HardwareAccel::Nvenc)
        .passthrough_to_rtmp("rtmp://output/app");

        let args = cmd.build_args();
        assert!(args.contains(&"-hwaccel".to_string()));
        assert!(args.contains(&"cuda".to_string()));
    }

    #[test]
    fn test_build_hwaccel_vaapi() {
        let cmd = FfmpegCommand::new(
            PathBuf::from("ffmpeg"),
            InputSource::Rtmp {
                url: "rtmp://0.0.0.0:1935/live".into(),
            },
        )
        .hw_accel(HardwareAccel::Vaapi)
        .passthrough_to_rtmp("rtmp://output/app");

        let args = cmd.build_args();
        assert!(args.contains(&"-vaapi_device".to_string()));
        assert!(args.contains(&"/dev/dri/renderD128".to_string()));
    }

    #[test]
    fn test_build_multiple_outputs() {
        let cmd = FfmpegCommand::new(
            PathBuf::from("ffmpeg"),
            InputSource::Rtmp {
                url: "rtmp://0.0.0.0:1935/live".into(),
            },
        )
        .passthrough_to_rtmp("rtmp://twitch.tv/app/key1")
        .passthrough_to_rtmp("rtmp://youtube.com/live2/key2");

        let args = cmd.build_args();
        assert!(args.contains(&"rtmp://twitch.tv/app/key1".to_string()));
        assert!(args.contains(&"rtmp://youtube.com/live2/key2".to_string()));
    }

    #[test]
    fn test_build_global_args() {
        let cmd = FfmpegCommand::new(
            PathBuf::from("ffmpeg"),
            InputSource::Rtmp {
                url: "rtmp://0.0.0.0:1935/live".into(),
            },
        )
        .global_arg("-loglevel")
        .global_arg("warning")
        .passthrough_to_rtmp("rtmp://output/app");

        let args = cmd.build_args();
        assert_eq!(args[0], "-loglevel");
        assert_eq!(args[1], "warning");
    }

    #[test]
    fn test_input_source_variants() {
        let rtmp = InputSource::Rtmp {
            url: "rtmp://test".into(),
        };
        let file = InputSource::File {
            path: PathBuf::from("/tmp/test.mp4"),
        };
        let pipe = InputSource::Pipe;

        assert!(matches!(rtmp, InputSource::Rtmp { .. }));
        assert!(matches!(file, InputSource::File { .. }));
        assert!(matches!(pipe, InputSource::Pipe));
    }

    #[test]
    fn test_output_destination_variants() {
        let hls = OutputDestination::Hls {
            segment_path: PathBuf::from("/tmp/seg"),
            playlist_path: PathBuf::from("/tmp/playlist.m3u8"),
        };
        let file = OutputDestination::File {
            path: PathBuf::from("/tmp/out.mp4"),
        };
        let rtmp = OutputDestination::Rtmp {
            url: "rtmp://test".into(),
        };
        let flv = OutputDestination::FlvHttp {
            endpoint: "/stream.flv".into(),
        };
        let pipe = OutputDestination::Pipe;

        assert!(matches!(hls, OutputDestination::Hls { .. }));
        assert!(matches!(file, OutputDestination::File { .. }));
        assert!(matches!(rtmp, OutputDestination::Rtmp { .. }));
        assert!(matches!(flv, OutputDestination::FlvHttp { .. }));
        assert!(matches!(pipe, OutputDestination::Pipe));
    }
}
