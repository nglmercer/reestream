mod command;
mod error;
mod process;
pub mod processing;
mod resolver;

pub use command::{FfmpegCommand, HardwareAccel, InputSource, Output, OutputDestination};
pub use error::FfmpegError;
pub use process::{FfmpegProcess, FfmpegSupervisor};
pub use processing::{
    ProcessConfig, ResizeConfig, StreamProcessor, ThumbnailConfig, TranscodeProfile,
    WatermarkConfig, WatermarkPosition,
};
pub use resolver::{BinaryResolver, PlatformBinaries};
