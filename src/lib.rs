//! # reestream
//!
//! A multi-stream RTMP relay that forwards incoming RTMP streams to multiple destinations.
//!
//! This library provides the core functionality for the RTMP relay system, including:
//! - Configuration management
//! - Server-side RTMP handling
//! - Client-side push functionality
//! - Error types and providers

pub mod client;
pub mod config;
pub mod error;
pub mod provider;
pub mod server;

// Re-export commonly used types for convenience
pub use client::PushClient;
pub use config::{Config, Orientation, Platform};
pub use error::{RelayError, Result};

// Async stream type alias
use tokio::io::{AsyncRead, AsyncWrite};

/// Dynamic stream type that can be either plain TCP or TLS
pub trait AsyncReadWrite: AsyncRead + AsyncWrite + Send + Unpin {}

impl<T: AsyncRead + AsyncWrite + Send + Unpin> AsyncReadWrite for T {}

pub type DynStream = Box<dyn AsyncReadWrite + 'static>;
