pub mod client;
pub mod config;
pub mod error;
pub mod hardening;
pub mod pipeline;
pub mod pipeline_impl;
pub mod provider;
pub mod rtsp;
pub mod security;
pub mod server;
pub mod setup;

use tokio::io::{AsyncRead, AsyncWrite};

pub trait AsyncReadWrite: AsyncRead + AsyncWrite + Send + Unpin {}

impl<T: AsyncRead + AsyncWrite + Send + Unpin> AsyncReadWrite for T {}

pub type DynStream = Box<dyn AsyncReadWrite + 'static>;
