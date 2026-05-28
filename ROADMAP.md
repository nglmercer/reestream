# Reestream Roadmap

## Current Status (v0.3.0)
- Workspace architecture with **5 crates** (core, ffmpeg, server, srt, root)
- RTMP relay server with multistream forwarding
- SRT protocol support (input listener, output sender, encryption)
- TLS/RTMPS support with reconnection logic
- Configuration via TOML with ConfigBuilder pattern
- FFmpeg integration (binary resolver, command builder, process supervisor, **download**)
- HLS segmenter with playlist generation
- REST API with stream/platform management (**19 routes**)
- HTTP server with axum (HLS serving, metrics, health check, FLV streaming)
- Web UI dashboard (Vite 8 + Preact + TypeScript + Tailwind CSS 4)
- Production hardening (graceful shutdown, rate limiting, connection pool, signal handlers)
- Structured JSON logging
- Webhook notifications
- Concrete pipeline implementations (RTMP, SRT, File)
- **263 tests passing**, clippy clean, cargo fmt clean

---

## Build System: Feature Flags

```toml
[features]
default = ["core"]
core = ["dep:reestream-core"]       # RTMP relay + multistream
hls = ["dep:reestream-server", "reestream-server/hls"]  # HLS/HTTP server
api = ["dep:reestream-server", "reestream-server/api"]  # REST API
srt = ["dep:reestream-srt"]         # SRT protocol
ffmpeg = ["dep:reestream-ffmpeg"]    # FFmpeg process management
preview = ["hls"]                    # Stream preview alias
webhook = ["dep:reestream-server", "reestream-server/api"]  # Webhook notifications
all = ["hls", "api", "ffmpeg", "preview", "srt", "webhook"]
```

### Build targets
```bash
# Core only (RTMP relay, minimal binary)
cargo build --release --no-default-features --features core

# Core + HLS server
cargo build --release --features core,hls

# Core + SRT
cargo build --release --features core,srt

# Core + API
cargo build --release --features core,api

# Everything
cargo build --release --features all
```

---

## Phase 0: Architecture Refactor ✅ DONE
- [x] Workspace restructure (reestream-core, reestream-ffmpeg, reestream-server, reestream-srt)
- [x] Cargo feature flags for optional components
- [x] ConfigBuilder pattern for programmatic config
- [x] StreamPipeline trait (input → process → output abstraction)
- [x] PipelineManager trait for managing multiple pipelines
- [x] Config validation and TOML serialization

---

## Phase 1: Testing Foundation ✅ DONE
- [x] Unit tests for `config.rs` (TOML parsing, validation, ConfigBuilder)
- [x] Unit tests for `error.rs` (Display, From conversions)
- [x] Unit tests for `client.rs` (video/audio header detection)
- [x] Unit tests for `client/push.rs` (buffer logic, URL parsing)
- [x] Unit tests for `provider.rs` (serialization, error types)
- [x] Unit tests for `pipeline.rs` (status, stats, events)

---

## Phase 2: Integration Tests ✅ DONE
- [x] `tests/` directory with integration tests
- [x] Full RTMP handshake flow (mock server/client)
- [x] Config file loading from disk
- [x] Graceful shutdown simulation
- [x] Reconnection logic with simulated disconnects
- [x] Test fixtures (sample RTMP packets, config files)

---

## Phase 3: Test Infrastructure ✅ DONE
- [x] `test-utils` feature flag for test helpers
- [x] Mock RTMP server for integration tests
- [x] Mock RTMP client for testing PushClient
- [x] `cargo-tarpaulin` coverage in CI
- [x] Property-based tests with `proptest`
- [x] Stress tests with concurrent connections
- [x] Network timeout simulation tests

---

## Phase 4: FFmpeg Integration ✅ DONE
- [x] FFmpeg binary resolver (platform → URL mapping)
- [x] Binary cache in `~/.local/share/reestream/bin/`
- [x] User-provided FFmpeg path override
- [x] FFmpeg binary download with checksum verification
- [x] FFmpeg command builder (passthrough, HLS, transcode, HW accel)
- [x] Hardware acceleration flags (VAAPI, NVENC, VideoToolbox, MMAL)
- [x] FFmpeg process wrapper with kill/stderr
- [x] Auto-restart supervisor with backoff
- [x] Transcoding profiles (1080p, 720p, 480p)

---

## Phase 5: HLS/HTTP Server ✅ DONE
- [x] HTTP server using `axum`
- [x] HLS segmenter with `.m3u8` playlist generation (live & VOD)
- [x] Segment cleanup (sliding window, configurable count)
- [x] CORS headers for cross-origin playback
- [x] Configurable segment storage path
- [x] Serve HLS manifest at `/stream.m3u8`
- [x] Serve segments at `/hls/{filename}`

---

## Phase 6: REST API ✅ DONE
- [x] HTTP API server
- [x] `GET /health` — health check
- [x] `GET /api/status` — server health, uptime, version
- [x] `GET /api/streams` — list active streams
- [x] `POST /api/streams` — add stream
- [x] `DELETE /api/streams/{id}` — remove stream
- [x] `GET /api/streams/{id}/stats` — stream statistics
- [x] `GET /api/config` — get config
- [x] `PUT /api/config` — update config
- [x] `POST /api/config/reload` — trigger config reload
- [x] `GET /api/platforms` — list platforms
- [x] `POST /api/platforms` — add platform
- [x] `DELETE /api/platforms/{id}` — remove platform
- [x] `PUT /api/platforms/{id}/toggle` — toggle platform
- [x] `GET /stream.m3u8` — HLS playlist
- [x] `GET /hls/{filename}` — HLS segments
- [x] `GET /stream.flv` — FLV live stream
- [x] `GET /metrics` — Prometheus-format metrics
- [x] `GET /` — Web UI dashboard

---

## Phase 7: SRT Protocol ✅ DONE
- [x] SRT input listener (feature-gated: `srt`)
- [x] SRT output push (multistream to SRT destinations)
- [x] SRT latency and congestion control config
- [x] SRT passphrase encryption (AES-128)
- [x] SRT configuration validation
- [ ] Bridge: SRT input → RTMP relay → HLS output (runtime wiring)

---

## Phase 8: Web UI ✅ DONE
- [x] UI build strategy (Vite 8 + Preact + TypeScript, compiled to static assets)
- [x] Tailwind CSS 4 styling
- [x] Dashboard — stream status, viewer count, uptime
- [x] Platform management (list, toggle enabled/disabled)
- [x] Log viewer (real-time in-browser logs)
- [x] Auto-refresh polling (5s status, 10s streams, 15s platforms)
- [x] Embedded via `rust-embed` (compiled into binary)
- [ ] Stream setup wizard
- [ ] Stream preview player (HLS.js or flv.js)
- [ ] i18n support

---

## Phase 9: Stream Processing Pipeline ✅ DONE
- [x] Input sources: RTMP, SRT, File
- [x] Concrete pipeline implementations (`RtmpPipeline`, `SrtPipeline`, `FilePipeline`)
- [x] `DefaultPipelineManager` with auto-detection of input type
- [x] Pipeline status/stats lifecycle
- [x] FLV container support (`/stream.flv` endpoint)
- [x] Output: RTMP, HLS, FLV
- [ ] Processing: transcode, resize, watermark
- [ ] Thumbnail/preview generation
- [ ] Input sources: RTSP, USB

---

## Phase 10: Monitoring & Observability ✅ DONE
- [x] Health check endpoint (`GET /health`)
- [x] Metrics endpoint (`GET /metrics`, Prometheus format)
- [x] `reestream_uptime_seconds`
- [x] `reestream_streams_total`
- [x] `reestream_viewers_total`
- [x] `reestream_stream_status` per stream
- [x] `reestream_stream_bitrate_kbps` per stream
- [x] Structured logging (JSON output option via `--json-log`)
- [x] Configurable log level (`--log-level`)
- [x] Webhook notifications (stream start/end/error, viewer connect/disconnect)
- [x] Webhook secret header authentication
- [x] Webhook configurable timeout

---

## Phase 11: Multiplatform Build & Distribution (PARTIAL)
- [x] Linux x86_64 (deb, rpm, tar.xz)
- [x] Linux aarch64 (deb, rpm, tar.xz)
- [x] Linux armv7 (tar.xz)
- [x] Linux armv6 (tar.xz)
- [x] Docker via Nix
- [ ] macOS x86_64/aarch64
- [ ] Windows x86_64
- [ ] Docker: `reestream/core`, `reestream/full`, `reestream/cuda`

---

## Phase 12: Production Hardening ✅ DONE
- [x] Graceful shutdown (drain in-flight packets with configurable timeout)
- [x] Rate limiting per connection (per-second connection rate limiter)
- [x] Connection pool management (max concurrent connections with RAII guard)
- [x] Max viewer limit per stream
- [x] Bandwidth limiting per stream
- [x] Config file watcher (hot-reload on change detection)
- [x] Signal handlers (SIGTERM=shutdown, SIGINT=shutdown, SIGHUP=reload)
- [ ] Let's Encrypt auto-TLS (ACME)
- [ ] Fuzz testing for RTMP packet parsing
- [ ] Stress tests with 100+ concurrent streams

---

## Feature Parity with datarhei/restreamer

| Feature | restreamer | reestream |
|---|---|---|
| RTMP/S ingest | ✅ | ✅ |
| SRT ingest/output | ✅ | ✅ |
| HLS HTTP server | ✅ | ✅ |
| HTTP-FLV streaming | ❌ | ✅ |
| FFmpeg transcoding | ✅ | ✅ |
| HW accel (CUDA/VAAPI) | ✅ | ✅ |
| Web UI | ✅ | ✅ |
| REST API | ✅ | ✅ |
| Viewer monitoring | ✅ | ✅ |
| Health check | ✅ | ✅ |
| Prometheus metrics | ✅ | ✅ |
| Docker multi-arch | ✅ | ✅ (Linux) |
| Stream recording | ❌ | TODO Phase 9 |
| Webhooks | ❌ | ✅ |
| Structured logging | ✅ | ✅ |
| Graceful shutdown | ✅ | ✅ |

---

## Testing Commands

```bash
# Run all tests
cargo test --workspace

# Run with all features
cargo test --workspace --all-features

# Run with output
cargo test --workspace -- --nocapture

# Run specific crate tests
cargo test -p reestream-core
cargo test -p reestream-ffmpeg
cargo test -p reestream-server
cargo test -p reestream-srt

# Run clippy (all features)
cargo clippy --workspace --all-targets --all-features

# Check formatting
cargo fmt --all -- --check

# Run with coverage
cargo tarpaulin --workspace --out Html

# Build minimal binary
cargo build --release --no-default-features --features core

# Build with everything
cargo build --release --features all

# Build dashboard
cd dashboard && bun run build
```

---

## Test Coverage

| Module | Tests |
|--------|-------|
| reestream-core | 123 |
| reestream-ffmpeg | 23 |
| reestream-server | 44 |
| reestream-srt | 23 |
| reestream (root) | 19 |
| integration tests | 31 |
| **Total** | **263** |

---

## Crate Architecture

```
reestream/                          # Root binary crate
├── crates/
│   ├── reestream-core/             # RTMP relay, config, pipeline traits, hardening
│   ├── reestream-ffmpeg/           # FFmpeg binary resolver, command builder, supervisor
│   ├── reestream-server/           # HTTP server, HLS, REST API, webhooks, dashboard
│   └── reestream-srt/              # SRT listener, sender, config
└── dashboard/                      # Vite 8 + Preact + TypeScript + Tailwind
    └── src/
        ├── api/                    # Type-safe API client
        ├── hooks/                  # usePolling hook
        └── components/             # Header, StatsCards, StreamsTable, PlatformsTable, LogViewer
```
