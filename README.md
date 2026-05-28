# Reestream

RTMP/SRT multistream relay server with HLS, HTTP-FLV, FFmpeg transcoding, REST API, and web dashboard.

## Features

- **RTMP relay** — receive one stream, forward to multiple platforms simultaneously
- **SRT protocol** — low-latency input/output with AES-128 encryption
- **HLS server** — live `.m3u8` playlist and `.ts` segment serving
- **HTTP-FLV** — zero-copy FLV live streaming at `/stream.flv`
- **FFmpeg integration** — binary resolver, command builder, supervisor, download, hardware acceleration
- **REST API** — 25 endpoints for stream/platform/config/setup/recording management
- **Web dashboard** — Vite 8 + Preact + TypeScript + Tailwind CSS 4
- **Video preview** — FLV/HLS player with latency monitor (flv.js)
- **First-time setup** — CLI `--setup` wizard + dashboard web wizard
- **Settings panel** — stream key reveal/reset, server endpoints, OBS setup guide
- **Platform management** — add/remove with presets (Twitch, YouTube, Facebook, Instagram, Kick, TikTok)
- **Stream recording** — FFmpeg-based recording to MP4/FLV/MKV/TS
- **Prometheus metrics** — uptime, streams, viewers, per-stream status and bitrate
- **Webhooks** — notifications for stream start/end/error, viewer connect/disconnect
- **Production hardening** — graceful shutdown, rate limiting, connection pool, signal handlers, config watcher
- **Structured logging** — JSON output option with configurable log level

## Quick Start

```bash
# Build with all features
cargo build --release --features all

# Create config
cat > config.toml <<EOF
rtmp_addr = "0.0.0.0"
rtmp_port = 1935
stream_key = "my-secret-key"

[[platform]]
url = "rtmp://live.twitch.tv/app"
key = "twitch-stream-key"
orientation = "horizontal"

[[platform]]
url = "rtmps://live-api-s.facebook.com:443/rtmp/"
key = "facebook-stream-key"
orientation = "vertical"
EOF

# Run
./target/release/reestream --config config.toml
```

## CLI Options

```
reestream [OPTIONS]

Options:
  -c, --config <PATH>      Config file path [default: config.toml]
      --json-log           Enable JSON structured logging
      --log-level <LEVEL>  Log level: trace, debug, info, warn, error [default: info]
      --setup              Run interactive first-time setup wizard
```

### First Run

```bash
# Option 1: CLI wizard
reestream --setup

# Option 2: Auto-detect → start server → open browser
reestream
# Shows: "No config file found. Run with --setup or open http://localhost:8080"
# Opens dashboard setup wizard automatically
```

## Configuration

### config.toml

```toml
rtmp_addr = "0.0.0.0"          # Bind address
rtmp_port = 1935                # RTMP port
stream_key = "publisher-key"    # Required stream key for publishing

# Optional: output platforms
[[platform]]
url = "rtmp://live.twitch.tv/app"
key = "twitch-key"
orientation = "horizontal"      # "horizontal" (default) or "vertical"

[[platform]]
url = "rtmps://live-api-s.facebook.com:443/rtmp/"
key = "facebook-key"
orientation = "vertical"
```

### Programmatic (Rust)

```rust
use reestream::config::{Config, ConfigBuilder, Orientation};
use url::Url;

let config = Config::builder()
    .addr("0.0.0.0")
    .port(1935)
    .stream_key("my-key")
    .add_platform(
        Url::parse("rtmp://live.twitch.tv/app").unwrap(),
        "twitch-key",
        Orientation::Horizontal,
    )
    .build();

config.validate().unwrap();
let toml = config.to_toml().unwrap();
```

## Services

When running with `--features all`, three services start:

| Service | Default Port | Description |
|---------|-------------|-------------|
| RTMP relay | 1935 | Accepts RTMP/RTMPS publish connections |
| SRT listener | 3000 | Accepts SRT input streams |
| HTTP server | 8080 | Dashboard, API, HLS, FLV, metrics |

## API Endpoints

### Health & Status

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/health` | Health check (200 OK) |
| `GET` | `/api/status` | Version, uptime, active streams, viewers |
| `GET` | `/metrics` | Prometheus-format metrics |

### Streams

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/streams` | List all streams |
| `POST` | `/api/streams` | Add stream `{name, input_url}` |
| `DELETE` | `/api/streams/{id}` | Remove stream |
| `GET` | `/api/streams/{id}/stats` | Stream statistics |

### Platforms

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/platforms` | List all platforms |
| `POST` | `/api/platforms` | Add platform `{name, url, key}` |
| `DELETE` | `/api/platforms/{id}` | Remove platform |
| `PUT` | `/api/platforms/{id}/toggle` | Toggle enabled/disabled |

### Config

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/config` | Get current config |
| `PUT` | `/api/config` | Update config |
| `POST` | `/api/config/reload` | Trigger hot-reload |

### Setup (First Run)

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/setup/status` | First-run detection |
| `POST` | `/api/setup/save` | Save config from wizard |
| `GET` | `/api/setup/info` | Server endpoints, hostname, ports |
| `GET` | `/api/setup/key` | Reveal stream key |
| `POST` | `/api/setup/key` | Reset stream key (generates new UUID) |

### Recordings

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/recordings` | List all recordings |
| `POST` | `/api/recordings/start` | Start recording `{stream_id, input_url}` |
| `POST` | `/api/recordings/{id}/stop` | Stop recording |
| `DELETE` | `/api/recordings/{id}` | Delete recording + file |

### Streaming

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/stream.m3u8` | HLS playlist (live) |
| `GET` | `/hls/{filename}` | HLS segment file |
| `GET` | `/stream.flv` | FLV live stream |

### Dashboard

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/` | Web UI dashboard |
| `GET` | `/dashboard` | Web UI dashboard (alias) |
| `GET` | `/assets/{*path}` | Static assets (JS, CSS) |
| `GET` | `/favicon.svg` | Favicon |

### Response Format

All API responses use a consistent wrapper:

```json
{
  "success": true,
  "data": { ... }
}
```

```json
{
  "success": false,
  "error": "error message"
}
```

## Prometheus Metrics

```
# HELP reestream_uptime_seconds Server uptime
# TYPE reestream_uptime_seconds gauge
reestream_uptime_seconds 3600

# HELP reestream_streams_total Number of streams
# TYPE reestream_streams_total gauge
reestream_streams_total 2

# HELP reestream_viewers_total Total viewers
# TYPE reestream_viewers_total gauge
reestream_viewers_total 150

# HELP reestream_stream_status Stream status (1=Live)
# TYPE reestream_stream_status gauge
reestream_stream_status{id="abc-123",name="main"} 1

# HELP reestream_stream_bitrate_kbps Stream bitrate
# TYPE reestream_stream_bitrate_kbps gauge
reestream_stream_bitrate_kbps{id="abc-123"} 5000
```

## Feature Flags

```toml
[features]
default = ["core"]
core = ["dep:reestream-core"]       # RTMP relay + multistream
hls = ["dep:reestream-server", "reestream-server/hls"]  # HLS/HTTP server
api = ["dep:reestream-server", "reestream-server/api"]  # REST API
srt = ["dep:reestream-srt"]         # SRT protocol
ffmpeg = ["dep:reestream-ffmpeg"]    # FFmpeg management
preview = ["hls"]                    # Stream preview
webhook = ["dep:reestream-server", "reestream-server/api"]  # Webhooks
all = ["hls", "api", "ffmpeg", "preview", "srt", "webhook"]
```

### Build Targets

```bash
# Minimal (RTMP relay only)
cargo build --release --no-default-features --features core

# With HLS
cargo build --release --features core,hls

# With SRT
cargo build --release --features core,srt

# With API
cargo build --release --features core,api

# Everything
cargo build --release --features all
```

## Architecture

```
reestream/                          # Root binary crate
├── src/
│   ├── main.rs                     # CLI, signal handlers, service startup
│   └── lib.rs                      # Re-exports from workspace crates
├── crates/
│   ├── reestream-core/             # RTMP relay, config, pipeline, hardening
│   │   └── src/
│   │       ├── client.rs           # RTMP publisher handler
│   │       ├── client/push.rs      # Push client with reconnection
│   │       ├── config.rs           # TOML config, ConfigBuilder
│   │       ├── error.rs            # RelayError enum
│   │       ├── hardening.rs        # Graceful shutdown, rate limiter, connection pool
│   │       ├── pipeline.rs         # StreamPipeline/PipelineManager traits
│   │       ├── pipeline_impl.rs    # RTMP/SRT/File pipeline implementations
│   │       ├── provider.rs         # OAuth2 stream key provider
│   │       └── server.rs           # RTMP handshake
│   ├── reestream-ffmpeg/           # FFmpeg binary management
│   │   └── src/
│   │       ├── command.rs          # Command builder (passthrough, HLS, transcode, HW accel)
│   │       ├── error.rs            # FfmpegError enum
│   │       ├── process.rs          # Process wrapper, supervisor with auto-restart
│   │       └── resolver.rs         # Binary resolver, platform URLs, download
│   ├── reestream-server/           # HTTP server, API, HLS, FLV, dashboard
│   │   ├── static/                 # Compiled dashboard (Vite output, embedded via rust-embed)
│   │   └── src/
│   │       ├── api.rs              # API types and route definitions
│   │       ├── dashboard.rs        # Static file serving (rust-embed)
│   │       ├── flv.rs              # FLV container builder and streaming
│   │       ├── hls.rs              # HLS segmenter and playlist generation
│   │       ├── http.rs             # Axum router, all endpoint handlers
│   │       ├── stream.rs           # StreamManager (CRUD for streams/platforms)
│   │       └── webhook.rs          # Webhook sender with event filtering
│   └── reestream-srt/              # SRT protocol support
│       └── src/
│           ├── config.rs           # SRT config (latency, encryption, bandwidth)
│           ├── error.rs            # SrtError enum
│           ├── listener.rs         # SRT input listener
│           └── sender.rs           # SRT output sender
├── dashboard/                      # Vite 8 + Preact + TypeScript + Tailwind
│   └── src/
│       ├── api/                    # Type-safe API client
│       ├── hooks/                  # usePolling, useVideoPlayer
│       └── components/             # Header, StatsCards, VideoPreview, StreamsTable, etc.
└── tests/                          # Integration tests
    ├── common/mock_rtmp.rs         # Mock RTMP server/client
    └── *.rs                        # 58 integration tests
```

## Web Dashboard

The dashboard is a single-page app built with Vite 8, Preact, TypeScript, and Tailwind CSS 4. It's compiled to static assets and embedded into the binary via `rust-embed`.

### Features

- Real-time stats (uptime, streams, viewers)
- Stream and platform management tables
- Live video preview with FLV.js (low-latency) or native HLS
- Source toggle (FLV/HLS), latency monitor, player controls
- Log viewer with in-browser log streaming
- Auto-refresh polling (5s/10s/15s)

### Building the Dashboard

```bash
cd dashboard
bun install
bun run build    # outputs to ../crates/reestream-server/static/
```

Then rebuild the Rust binary to embed the new assets:

```bash
cargo build --release --features all
```

## FFmpeg Integration

### Binary Resolution Order

1. Custom path (if set)
2. Local cache at `~/.local/share/reestream/bin/ffmpeg`
3. System PATH
4. Auto-download from platform-specific URL

### Hardware Acceleration

| Accelerator | Flag | Platform |
|-------------|------|----------|
| VAAPI | `HardwareAccel::Vaapi` | Linux (Intel/AMD) |
| NVENC | `HardwareAccel::Nvenc` | Linux/Windows (NVIDIA) |
| VideoToolbox | `HardwareAccel::VideoToolbox` | macOS |
| MMAL | `HardwareAccel::Mmal` | Raspberry Pi |

### Command Builder

```rust
use reestream::ffmpeg::{FfmpegCommand, InputSource, OutputDestination, HardwareAccel};
use std::path::PathBuf;

let cmd = FfmpegCommand::new(PathBuf::from("ffmpeg"), InputSource::Pipe)
    .hw_accel(HardwareAccel::Nvenc)
    .passthrough_to_rtmp("rtmp://live.twitch.tv/app/key")
    .to_hls(PathBuf::from("/tmp/segments"), PathBuf::from("/tmp/playlist.m3u8"));

let args = cmd.build_args();
```

## SRT Protocol

### Listener (Input)

```rust
use reestream::srt::{SrtConfig, SrtListener};

let config = SrtConfig {
    enabled: true,
    listen_addr: "0.0.0.0".into(),
    listen_port: 3000,
    latency_ms: 200,
    passphrase: Some("my-encryption-passphrase".into()),
    ..Default::default()
};

let listener = SrtListener::new(config);
listener.run().await?;
```

### Sender (Output)

```rust
use reestream::srt::SrtSender;
use url::Url;

let mut sender = SrtSender::new(
    Url::parse("srt://output-server:3000").unwrap(),
    200,
    Some("encryption-passphrase".into()),
);
sender.connect().await?;
sender.send(data).await?;
```

## Webhooks

### Configuration

```rust
use reestream::http_server::webhook::{WebhookConfig, WebhookSender, WebhookEvent, create_payload};
use serde_json::json;

let config = WebhookConfig {
    enabled: true,
    url: "https://hooks.example.com/reestream".into(),
    secret: Some("webhook-secret".into()),
    on_stream_start: true,
    on_stream_end: true,
    on_stream_error: true,
    ..Default::default()
};

let sender = WebhookSender::new(config);
let payload = create_payload(
    WebhookEvent::StreamStart,
    "stream-id".into(),
    json!({"name": "my-stream"}),
);
sender.send(&payload).await?;
```

## Production Hardening

### Graceful Shutdown

```rust
use reestream::hardening::{GracefulShutdown, setup_signal_handlers};
use std::sync::Arc;

let shutdown = Arc::new(GracefulShutdown::new());
setup_signal_handlers(shutdown.clone()).await;

// In your main loop:
tokio::select! {
    _ = shutdown.wait_for_shutdown() => {
        shutdown.drain_timeout(Duration::from_secs(30)).await;
        break;
    }
    // ... other branches
}
```

### Rate Limiting & Connection Pool

```rust
use reestream::hardening::{RateLimiter, ConnectionPool};

let rate_limiter = RateLimiter::new(100); // 100 connections/sec
let pool = ConnectionPool::new(1000);     // max 1000 concurrent

if rate_limiter.try_acquire().await {
    if let Some(guard) = pool.try_acquire().await {
        // handle connection
        // guard dropped automatically on scope exit
    }
}
```

### Config Watcher

```rust
use reestream::hardening::ConfigWatcher;

ConfigWatcher::watch_loop(
    PathBuf::from("config.toml"),
    Duration::from_secs(5),
    || println!("Config changed, reloading..."),
).await;
```

## Testing

```bash
# All tests
cargo test --workspace --all-features

# Specific crate
cargo test -p reestream-core
cargo test -p reestream-ffmpeg
cargo test -p reestream-server
cargo test -p reestream-srt

# With output
cargo test --workspace -- --nocapture

# Clippy
cargo clippy --workspace --all-targets --all-features

# Formatting
cargo fmt --all -- --check

# Coverage
cargo tarpaulin --workspace --out Html
```

## Low Latency Configuration

- **RTMP chunk size:** 128 bytes (smaller chunks, lower per-chunk latency)
- **ACK window:** 256KB (more frequent acknowledgments)
- **TCP_NODELAY:** enabled on all sockets
- **FLV player:** `enableStashBuffer: false`, `stashInitialSize: 128`
- **SRT default latency:** 200ms

## Supported Protocols

| Protocol | Input | Output |
|----------|-------|--------|
| RTMP | ✅ | ✅ |
| RTMPS | ✅ | ✅ |
| SRT | ✅ | ✅ |
| HLS | — | ✅ |
| HTTP-FLV | — | ✅ |

## License

MIT OR Apache-2.0
