# Reestream Roadmap

## Current Status (v0.2.0, verified 2026-08-09)

| Metric | Value |
|--------|-------|
| Crates | 5 (core, ffmpeg, server, srt, root) |
| Rust source files | 60 |
| Tests | 411 (cargo test --workspace --all-features) |
| API routes | 130+ (legacy + `/api/v1`) |
| Feature flags | 8 |
| Dashboard components | 18 |

### What's built

**Core**
- RTMP relay with multistream forwarding and RTMPS output support
- SRT protocol (optional encrypted input listener and output sender)
- SRT bridge (SRT→RTMP→HLS pipeline with stats)
- RTSP input support (TCP/UDP transport, FFmpeg restream)
- TLS/RTMPS support with reconnection logic
- Configuration via TOML with ConfigBuilder pattern
- Concrete pipeline implementations (RTMP, SRT, File)

**FFmpeg**
- Binary resolver (platform URL mapping, download, checksum)
- Command builder (passthrough, HLS, transcode, HW accel)
- Process supervisor with auto-restart and backoff
- Hardware acceleration (VAAPI, NVENC, VideoToolbox, MMAL)
- Stream processing (transcode profiles, resize, watermark, thumbnail)
- Recording (FFmpeg-based, MP4/FLV/MKV/TS, scheduled, rotation)

**Server**
- HTTP server using axum
- HLS segmenter with live `.m3u8` playlist
- HTTP-FLV live streaming (`/stream.flv`)
- Legacy REST API plus versioned product API for channels, drafts, events,
  Studio, chat, analytics, storage, clips, webhooks, private ingest, OAuth,
  and provider-compatible event subresources
- Scheduled event worker with file/playlist FFmpeg playback and recording-to-
  storage lifecycle linkage
- Optional command-backed transcription lifecycle with explicit unavailable
  (`Unknown`) status when no speech-to-text binary is configured
- Prometheus metrics (uptime, streams, viewers, bitrate)
- Webhook notifications (stream start/end/error, viewer connect/disconnect)
- DVR/timeshift buffer
- WebRTC config (ICE servers)
- Adaptive bitrate (ABR) for HLS (master playlist generation)

**Dashboard**
- Vite 8 + Preact + TypeScript + Tailwind CSS 4
- Video preview (FLV/HLS player with flv.js, latency monitor)
- First-time setup wizard (CLI `--setup` + web wizard)
- Settings panel (stream key reveal/reset, server endpoints, OBS guide)
- Platform management (add/remove with presets)
- Recording controls (start/stop/delete)
- Stream and platform tables
- Log viewer
- Auto-refresh polling

**Security**
- Optional bearer authentication with login throttling, refresh-token rotation,
  in-memory sessions, and protected legacy control routes
- AES-256-GCM encrypted state sidecar for channel/event/OAuth/webhook secrets
- IP allowlist/blocklist (CIDR support)
- Per-platform stream key validation
- Rate limiting per IP
- TLS-compatible outbound RTMPS; terminate HTTPS/RTMPS at a trusted proxy

**Production**
- Graceful shutdown (drain in-flight, configurable timeout)
- Rate limiting per connection
- Connection pool (max concurrent, RAII guard)
- Max viewer limit per stream
- Bandwidth limiting per stream
- Config file watcher (change detection; listener changes require restart)
- Signal handlers (SIGTERM, SIGINT, SIGHUP)
- Fuzz tests (RTMP packet parsing, config, FLV tags, IP matching)
- Stress tests (50 concurrent, rapid connect/disconnect, contention)
- ACME/Let's Encrypt config (auto-TLS)

**Structured Logging**
- JSON output (`--json-log`)
- Configurable level (`--log-level`)

---

## Build System

```toml
[features]
default = ["core"]
core = ["dep:reestream-core"]
hls = ["dep:reestream-server", "reestream-server/hls"]
api = ["dep:reestream-server", "reestream-server/api"]
srt = ["dep:reestream-srt"]
ffmpeg = ["dep:reestream-ffmpeg"]
preview = ["hls"]
webhook = ["dep:reestream-server", "reestream-server/api"]
all = ["hls", "api", "ffmpeg", "preview", "srt", "webhook"]
```

```bash
cargo build --release --features all
```

---

## TODO: Future Features

The current product API and local runtime cover the client-facing control plane.
The remaining work below is optional production depth behind provider-specific
adapters and external media services, not missing route design.

### 1. Web UI Enhancements
- [ ] i18n (internationalization)
- [ ] Stream analytics charts (bitrate/viewers over time)
- [ ] Dark/light theme toggle
- [ ] Keyboard shortcuts
- [ ] Mobile-responsive improvements

### 2. Advanced Streaming
- [ ] WebRTC output (low-latency viewer playback)
- [ ] Multi-language audio track support

### 3. Advanced Recording
- [ ] Recording upload to S3/R2/MinIO

---

## API Endpoints

The table below is the legacy compatibility surface. The complete versioned
client contract is documented in [docs/API.md](docs/API.md); use `/api/v1` for
new web work.

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

## Test Coverage (latest full-feature run)

| Module | Tests |
|--------|------:|
| reestream-core | 153 |
| reestream-ffmpeg | 33 |
| reestream-server | 106 |
| reestream-srt | 28 |
| reestream + root integration tests | 91 |
| **Total** | **411** |

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
│   │       ├── rtsp.rs             # RTSP input config and FFmpeg args
│   │       ├── security.rs         # IP filter, API token, ACME config
│   │       ├── server.rs           # RTMP handshake
│   │       └── setup.rs            # First-run detection, CLI wizard, setup API
│   ├── reestream-ffmpeg/
│   │   └── src/
│   │       ├── command.rs          # Command builder
│   │       ├── error.rs            # FfmpegError
│   │       ├── process.rs          # Process wrapper, supervisor
│   │       ├── processing.rs       # Transcode, watermark, thumbnail, resize
│   │       └── resolver.rs         # Binary resolver, download
│   ├── reestream-server/
│   │   ├── static/                 # Compiled dashboard (rust-embed)
│   │   └── src/
│   │       ├── api.rs              # API types, route definitions
│   │       ├── dashboard.rs        # Static file serving
│   │       ├── dvr.rs              # DVR/timeshift buffer
│   │       ├── flv.rs              # FLV container builder
│   │       ├── hls.rs              # HLS segmenter
│   │       ├── http.rs             # Axum router, all handlers
│   │       ├── recording.rs        # FFmpeg recording manager
│   │       ├── recording_ext.rs    # Scheduled, rotation, S3, format convert
│   │       ├── stream.rs           # StreamManager CRUD
│   │       ├── webhook.rs          # Webhook sender
│   │       └── webrtc.rs           # WebRTC config, ABR, master playlist
│   └── reestream-srt/
│       └── src/
│           ├── bridge.rs           # SRT→RTMP bridge with stats
│           ├── config.rs           # SRT config
│           ├── error.rs            # SrtError
│           ├── listener.rs         # SRT input
│           └── sender.rs           # SRT output
├── dashboard/                      # Vite 8 + Preact + TypeScript + Tailwind
│   └── src/
│       ├── api/                    # Type-safe API client
│       ├── hooks/                  # usePolling, useVideoPlayer
│       └── components/             # 10 components
└── tests/
    ├── fuzz.rs                     # Property-based fuzz tests
    ├── stress_heavy.rs             # Heavy stress tests
    └── *.rs                        # 57 integration tests
```
