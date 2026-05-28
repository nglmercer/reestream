# Reestream Roadmap

## Current Status (v0.3.0)

| Metric | Value |
|--------|-------|
| Crates | 5 (core, ffmpeg, server, srt, root) |
| Rust source files | 28 |
| Rust lines of code | ~6,000 |
| Tests | 294 |
| API endpoints | 25 |
| Feature flags | 8 |
| Dashboard components | 10 |

### What's built
- RTMP relay with multistream forwarding (RTMP/RTMPS)
- SRT protocol (input listener, output sender, AES-128 encryption)
- HLS segmenter with live `.m3u8` playlist
- HTTP-FLV live streaming (`/stream.flv`)
- FFmpeg integration (binary resolver, command builder, supervisor, download, HW accel)
- REST API (25 endpoints: streams, platforms, config, setup, recordings, metrics)
- Web dashboard (Vite 8 + Preact + TypeScript + Tailwind 4 + flv.js)
- Video preview (FLV/HLS toggle, latency monitor)
- First-time setup (CLI `--setup` wizard + dashboard web wizard)
- Settings panel (stream key reveal/reset, server endpoints, OBS guide)
- Platform management (add/remove with presets: Twitch, YouTube, Facebook, Instagram, Kick, TikTok)
- Stream recording (FFmpeg-based, MP4/FLV/MKV/TS, dashboard controls)
- Webhook notifications (stream start/end/error, viewer connect/disconnect)
- Structured JSON logging (`--json-log`, `--log-level`)
- Production hardening (graceful shutdown, rate limiting, connection pool, signal handlers, config watcher)
- Prometheus metrics (uptime, streams, viewers, per-stream status/bitrate)
- Concrete pipeline implementations (RTMP, SRT, File)

---

## Build System

```toml
[features]
default = ["core"]
core = ["dep:reestream-core"]       # RTMP relay + multistream
hls = ["dep:reestream-server", "reestream-server/hls"]  # HLS/HTTP server
api = ["dep:reestream-server", "reestream-server/api"]  # REST API
srt = ["dep:reestream-srt"]         # SRT protocol
ffmpeg = ["dep:reestream-ffmpeg"]    # FFmpeg process management
preview = ["hls"]                    # Stream preview
webhook = ["dep:reestream-server", "reestream-server/api"]  # Webhooks
all = ["hls", "api", "ffmpeg", "preview", "srt", "webhook"]
```

```bash
cargo build --release --features all
```

---

## TODO: Future Features

### 1. SRT Bridge (runtime wiring)
- [ ] SRT input → RTMP relay → HLS output pipeline
- [ ] Auto-detect SRT publish and route to RTMP clients

### 2. Stream Processing
- [ ] Transcode via FFmpeg (resolution/bitrate/codec conversion)
- [ ] Resize / scale filters
- [ ] Watermark overlay (image or text)
- [ ] Thumbnail / preview frame generation
- [ ] Input sources: RTSP, USB capture

### 3. Web UI Enhancements
- [ ] i18n (internationalization support)
- [ ] Stream analytics charts (bitrate, viewers over time)
- [ ] Dark/light theme toggle
- [ ] Keyboard shortcuts
- [ ] Mobile-responsive improvements

### 4. Multiplatform Distribution
- [ ] macOS builds (x86_64, aarch64)
- [ ] Windows builds (x86_64)
- [ ] Docker images: `reestream/core`, `reestream/full`, `reestream/cuda`
- [ ] GitHub Actions CI/CD pipeline

### 5. Production Hardening
- [ ] Let's Encrypt auto-TLS (ACME integration)
- [ ] Fuzz testing for RTMP packet parsing
- [ ] Stress tests with 100+ concurrent streams
- [ ] Connection draining on config reload

### 6. Advanced Recording
- [ ] Scheduled recordings (start/stop at specific times)
- [ ] Recording rotation (auto-split by duration or size)
- [ ] Recording upload to S3/R2/MinIO
- [ ] Recording format conversion post-capture

### 7. Advanced Streaming
- [ ] RTSP input/output support
- [ ] WebRTC output (low-latency viewer playback)
- [ ] Adaptive bitrate (ABR) for HLS
- [ ] DVR / timeshift (rewind live stream)
- [ ] Multi-language audio track support

### 8. Observability
- [ ] OpenTelemetry tracing export
- [ ] Grafana dashboard JSON template
- [ ] Alerting webhooks (configurable thresholds)
- [ ] Log file rotation and archival

### 9. Security
- [ ] RTMP stream key validation per-platform
- [ ] IP allowlist/blocklist for publishing
- [ ] Rate limiting per stream key
- [ ] HTTPS for dashboard (auto-TLS or manual cert)
- [ ] API authentication (token-based)

---

## API Endpoints (25)

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/` | Dashboard |
| `GET` | `/dashboard` | Dashboard (alias) |
| `GET` | `/assets/{*path}` | Static assets |
| `GET` | `/favicon.svg` | Favicon |
| `GET` | `/health` | Health check |
| `GET` | `/api/status` | Server status |
| `GET` | `/api/streams` | List streams |
| `POST` | `/api/streams` | Add stream |
| `DELETE` | `/api/streams/{id}` | Remove stream |
| `GET` | `/api/streams/{id}/stats` | Stream stats |
| `GET` | `/api/config` | Get config |
| `PUT` | `/api/config` | Update config |
| `POST` | `/api/config/reload` | Reload config |
| `GET` | `/api/setup/status` | First-run detection |
| `POST` | `/api/setup/save` | Save setup config |
| `GET` | `/api/setup/info` | Server endpoints/URLs |
| `GET` | `/api/setup/key` | Reveal stream key |
| `POST` | `/api/setup/key` | Reset stream key |
| `GET` | `/api/platforms` | List platforms |
| `POST` | `/api/platforms` | Add platform |
| `DELETE` | `/api/platforms/{id}` | Remove platform |
| `PUT` | `/api/platforms/{id}/toggle` | Toggle platform |
| `GET` | `/api/recordings` | List recordings |
| `POST` | `/api/recordings/start` | Start recording |
| `POST` | `/api/recordings/{id}/stop` | Stop recording |
| `DELETE` | `/api/recordings/{id}` | Delete recording |
| `GET` | `/stream.m3u8` | HLS playlist |
| `GET` | `/hls/{filename}` | HLS segment |
| `GET` | `/stream.flv` | FLV live stream |
| `GET` | `/metrics` | Prometheus metrics |

---

## CLI

```
reestream [OPTIONS]

Options:
  -c, --config <PATH>      Config file path [default: config.toml]
      --json-log           Enable JSON structured logging
      --log-level <LEVEL>  Log level [default: info]
      --setup              Run interactive first-time setup wizard
```

---

## Test Coverage

| Module | Tests |
|--------|------:|
| reestream-core | 131 |
| reestream-ffmpeg | 23 |
| reestream-server | 51 |
| reestream-srt | 23 |
| reestream (root) | 10 |
| integration tests | 56 |
| **Total** | **294** |

---

## Crate Architecture

```
reestream/
├── crates/
│   ├── reestream-core/
│   │   └── src/
│   │       ├── client.rs           # RTMP publisher handler
│   │       ├── client/push.rs      # Push client with reconnection
│   │       ├── config.rs           # TOML config, ConfigBuilder
│   │       ├── error.rs            # RelayError
│   │       ├── hardening.rs        # Shutdown, rate limiter, pool, signals, watcher
│   │       ├── pipeline.rs         # StreamPipeline/PipelineManager traits
│   │       ├── pipeline_impl.rs    # RTMP/SRT/File pipelines
│   │       ├── provider.rs         # OAuth2 stream key provider
│   │       ├── server.rs           # RTMP handshake
│   │       └── setup.rs            # First-run detection, CLI wizard, setup API
│   ├── reestream-ffmpeg/
│   │   └── src/
│   │       ├── command.rs          # Command builder
│   │       ├── error.rs            # FfmpegError
│   │       ├── process.rs          # Process wrapper, supervisor
│   │       └── resolver.rs         # Binary resolver, download
│   ├── reestream-server/
│   │   ├── static/                 # Compiled dashboard (rust-embed)
│   │   └── src/
│   │       ├── api.rs              # API types, route definitions
│   │       ├── dashboard.rs        # Static file serving
│   │       ├── flv.rs              # FLV container builder
│   │       ├── hls.rs              # HLS segmenter
│   │       ├── http.rs             # Axum router, all handlers
│   │       ├── recording.rs        # FFmpeg recording manager
│   │       ├── stream.rs           # StreamManager CRUD
│   │       └── webhook.rs          # Webhook sender
│   └── reestream-srt/
│       └── src/
│           ├── config.rs           # SRT config
│           ├── error.rs            # SrtError
│           ├── listener.rs         # SRT input
│           └── sender.rs           # SRT output
├── dashboard/
│   └── src/
│       ├── api/                    # Type-safe API client
│       ├── hooks/                  # usePolling, useVideoPlayer
│       └── components/             # 10 components
└── tests/                          # Integration tests
```
