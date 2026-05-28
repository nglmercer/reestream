# Reestream Roadmap

## Current Status (v0.1.1)
- Basic RTMP relay server
- Multistream forwarding to multiple platforms
- TLS/RTMPS support
- Reconnection logic
- Configuration via TOML

---

## Build System: Feature Flags

```toml
[features]
default = ["core"]
core = []                          # RTMP relay + multistream (always included)
hls = ["axum", "tokio-stream"]     # HLS/HTTP server
api = ["axum", "serde_json"]       # REST API
ui = ["api", "rust-embed"]         # Web UI (requires api)
srt = ["srt-tokio"]                # SRT protocol
ffmpeg = []                        # FFmpeg process management
preview = ["hls"]                  # Stream preview (requires hls)
all = ["hls", "api", "ui", "srt", "ffmpeg", "preview"]
```

### Build targets
```bash
# Core only (RTMP relay, minimal binary ~3MB)
cargo build --release --no-default-features --features core

# Core + HLS server
cargo build --release --features hls

# Core + API + UI (full web features)
cargo build --release --features ui

# Everything
cargo build --release --features all
```

---

## Phase 0: Architecture Refactor (Foundation)
- [ ] Restructure as workspace with crates:
  - `reestream-core` — RTMP relay, multistream, config, error
  - `reestream-server` — HLS/HTTP server, REST API
  - `reestream-ffmpeg` — FFmpeg process manager
  - `reestream-ui` — Embedded web UI (compiled or downloaded)
  - `reestream` — Binary crate that composes all above
- [ ] Add Cargo feature flags for optional components
- [ ] Migrate config from TOML to TOML+JSON schema with validation
- [ ] Add `ConfigBuilder` pattern for programmatic config
- [ ] Define `StreamPipeline` trait (input → process → output abstraction)

---

## Phase 1: Testing Foundation (Completed)
- [x] Unit tests for `config.rs` (TOML parsing, validation)
- [x] Unit tests for `error.rs` (Display, From conversions)
- [x] Unit tests for `client.rs` (video/audio header detection)
- [x] Unit tests for `client/push.rs` (buffer logic, URL parsing)
- [x] Unit tests for `provider.rs` (serialization, error types)

---

## Phase 2: Integration Tests
- [ ] Add `tests/` directory for integration tests
- [ ] Test full RTMP handshake flow (mock server/client)
- [ ] Test config file loading from disk
- [ ] Test graceful shutdown on Ctrl+C
- [ ] Test reconnection logic with simulated disconnects
- [ ] Add test fixtures (sample RTMP packets, config files)

---

## Phase 3: FFmpeg Integration
- [ ] FFmpeg binary manager (`reestream-ffmpeg` crate)
  - [ ] Download correct FFmpeg binary per platform at startup
  - [ ] Binary registry: platform → URL mapping (JSON manifest)
  - [ ] Cache binaries in `~/.local/share/reestream/bin/` or `/data/bin/`
  - [ ] Verify binary checksums (SHA256)
  - [ ] Support user-provided FFmpeg path override
- [ ] FFmpeg process wrapper
  - [ ] Spawn FFmpeg as child process with stdin/stdout pipes
  - [ ] Monitor process health (PID, CPU, memory)
  - [ ] Auto-restart on crash with backoff
  - [ ] Graceful SIGTERM → SIGKILL escalation
- [ ] FFmpeg command builder
  - [ ] RTMP input → HLS output pipeline
  - [ ] RTMP input → FLV output pipeline
  - [ ] Transcoding profiles (passthrough, 1080p, 720p, 480p)
  - [ ] Hardware acceleration flags (VAAPI, NVENC, MMAL, VideoToolbox)
  - [ ] Audio-only mode
- [ ] Supported FFmpeg sources (prebuilt binaries)
  - [ ] Linux x86_64: https://johnvansickle.com/ffmpeg/
  - [ ] Linux aarch64: https://johnvansickle.com/ffmpeg/
  - [ ] Linux armv7/armv6: https://johnvansickle.com/ffmpeg/
  - [ ] macOS universal: https://evermeet.cx/ffmpeg/
  - [ ] Windows x86_64: https://www.gyan.dev/ffmpeg/builds/
  - [ ] Alternative: bundle via Nix (current approach for Docker)

---

## Phase 4: HLS/HTTP Server
- [ ] HTTP server using `axum` (feature-gated: `hls`)
- [ ] HLS segmenter
  - [ ] `.m3u8` playlist generation (live & VOD)
  - [ ] `.ts` segment writer with configurable duration (default 2s)
  - [ ] Segment cleanup (sliding window, configurable count)
  - [ ] Low-latency HLS (LL-HLS) with partial segments
- [ ] Serve HLS manifest and segments via HTTP
- [ ] CORS headers for cross-origin playback
- [ ] Configurable segment storage path
- [ ] FLV container support
  - [ ] FLV muxer for HTTP-FLV streaming
  - [ ] `/stream.flv` endpoint
  - [ ] Compatible with flv.js in browser
- [ ] Thumbnail/preview generation
  - [ ] Periodic JPEG snapshots from stream
  - [ ] `/stream/thumb.jpg` endpoint

---

## Phase 5: SRT Protocol
- [ ] SRT input listener (feature-gated: `srt`)
- [ ] SRT output push (multistream to SRT destinations)
- [ ] SRT latency and congestion control config
- [ ] SRT passphrase encryption
- [ ] Bridge: SRT input → RTMP relay → HLS output

---

## Phase 6: REST API
- [ ] HTTP API server (feature-gated: `api`)
- [ ] Endpoints:
  - [ ] `GET /api/status` — server health, uptime, version
  - [ ] `GET /api/streams` — list active streams
  - [ ] `POST /api/streams` — add platform destination
  - [ ] `DELETE /api/streams/:id` — remove platform destination
  - [ ] `GET /api/streams/:id/stats` — bitrate, viewers, uptime
  - [ ] `POST /api/config/reload` — hot-reload config
  - [ ] `GET /api/config` — current config (redacted keys)
  - [ ] `PUT /api/config` — update config via API
- [ ] Authentication
  - [ ] Bearer token auth
  - [ ] Basic auth
  - [ ] Configurable per-endpoint permissions
- [ ] WebSocket for real-time stats
- [ ] OpenAPI/Swagger spec generation

---

## Phase 7: Web UI
- [ ] UI build strategy (choose one):
  - [ ] Option A: Embed pre-built UI via `rust-embed` (compile-time)
  - [ ] Option B: Download UI assets at build time from GitHub releases
  - [ ] Option C: Serve UI from separate process/container
- [ ] UI framework: React or Leptos (Rust WASM)
- [ ] Pages:
  - [ ] Dashboard — stream status, viewer count, uptime
  - [ ] Stream setup wizard (like restreamer)
  - [ ] Platform management (add/remove/edit destinations)
  - [ ] FFmpeg process monitor (CPU, memory, frames)
  - [ ] Config editor (TOML with syntax highlighting)
  - [ ] Stream preview player (HLS.js or flv.js)
  - [ ] Log viewer (real-time streaming logs)
- [ ] i18n support (es, en, pt, fr, de minimum)
- [ ] Mobile-responsive layout

---

## Phase 8: Stream Processing Pipeline
- [ ] Input sources
  - [ ] RTMP ingest (current)
  - [ ] SRT ingest
  - [ ] File input (for offline/test)
  - [ ] RTSP input
  - [ ] USB/local device input (via FFmpeg)
- [ ] Processing chain
  - [ ] Passthrough (no transcoding, lowest CPU)
  - [ ] Transcode (via FFmpeg)
  - [ ] Resize/crop for platform-specific resolutions
  - [ ] Audio remix/mux (separate audio track)
  - [ ] Watermark overlay
  - [ ] Timestamp burn-in
- [ ] Output destinations
  - [ ] RTMP/RTMPS push (current)
  - [ ] SRT push
  - [ ] HLS local server
  - [ ] FLV HTTP stream
  - [ ] File recording (MP4/MKV)
  - [ ] WebRTC (future)

---

## Phase 9: Monitoring & Observability
- [ ] Metrics endpoint (Prometheus format)
  - [ ] `reestream_streams_total`
  - [ ] `reestream_viewers_gauge`
  - [ ] `reestream_bitrate_bytes`
  - [ ] `reestream_ffmpeg_cpu_usage`
  - [ ] `reestream_reconnects_total`
- [ ] Health check endpoint (`GET /health`)
- [ ] Structured logging (JSON output option)
- [ ] Log levels configurable per module
- [ ] Webhook notifications
  - [ ] Stream started
  - [ ] Stream ended
  - [ ] Platform disconnected
  - [ ] FFmpeg process crashed

---

## Phase 10: Multiplatform Build & Distribution
- [ ] Build matrix (via Nix, already partially done):
  - [x] Linux x86_64 (deb, rpm, tar.xz)
  - [x] Linux aarch64 (deb, rpm, tar.xz)
  - [x] Linux armv7 (tar.xz)
  - [x] Linux armv6 (tar.xz)
  - [ ] macOS x86_64 (dmg, tar.gz)
  - [ ] macOS aarch64 (dmg, tar.gz)
  - [ ] Windows x86_64 (msi, zip)
  - [ ] Windows aarch64 (msi, zip)
  - [ ] FreeBSD x86_64
- [ ] Docker images (already via Nix, improve):
  - [ ] `reestream/core` — minimal, RTMP relay only (~10MB)
  - [ ] `reestream/full` — with FFmpeg, HLS, UI (~80MB)
  - [ ] `reestream/cuda` — with NVIDIA GPU support
  - [ ] `reestream/vaapi` — with Intel GPU support
- [ ] FFmpeg binary bundling strategy:
  - [ ] Docker: FFmpeg installed in image layer
  - [ ] Standalone binary: download FFmpeg on first run
  - [ ] Nix bundle: FFmpeg included via Nix closure
- [ ] GitHub Actions CI
  - [ ] Lint + test on every PR
  - [ ] Cross-compile on tag push
  - [ ] Docker build + push to GHCR
  - [ ] Changelog generation (git-cliff)

---

## Phase 11: Production Hardening
- [ ] Graceful shutdown (drain in-flight packets)
- [ ] Rate limiting per connection
- [ ] Connection pool management
- [ ] Max viewer limit per stream
- [ ] Bandwidth limiting per stream
- [ ] Let's Encrypt auto-TLS (ACME)
- [ ] Config file watcher (hot-reload on change)
- [ ] Signal handlers (SIGHUP=reload, SIGTERM=shutdown)
- [ ] Memory leak detection (long-running soak tests)
- [ ] Fuzz testing for RTMP packet parsing
- [ ] Stress tests with 100+ concurrent streams

---

## Phase 12: Feature Parity with datarhei/restreamer

| Feature | restreamer | reestream target |
|---|---|---|
| RTMP/S ingest | ✅ | Phase 0 (current) |
| SRT ingest/output | ✅ | Phase 5 |
| HLS HTTP server | ✅ | Phase 4 |
| HTTP-FLV streaming | ❌ | Phase 4 |
| FFmpeg transcoding | ✅ | Phase 3 |
| HW accel (CUDA/VAAPI) | ✅ | Phase 3/8 |
| Web UI | ✅ | Phase 7 |
| REST API | ✅ | Phase 6 |
| Viewer monitoring | ✅ | Phase 9 |
| Bandwidth limits | ✅ | Phase 11 |
| Let's Encrypt | ✅ | Phase 11 |
| Docker multi-arch | ✅ | Phase 10 |
| Stream recording | ❌ | Phase 8 |
| Webhooks | ❌ | Phase 9 |
| Prometheus metrics | ✅ | Phase 9 |

---

## Testing Commands

```bash
# Run all tests
cargo test

# Run only core tests (no optional features)
cargo test --no-default-features --features core

# Run with output
cargo test -- --nocapture

# Run specific test module
cargo test config::tests

# Run clippy
cargo clippy --all-features

# Run with coverage (requires cargo-tarpaulin)
cargo tarpaulin --out Html --all-features

# Build minimal binary
cargo build --release --no-default-features --features core

# Build with everything
cargo build --release --features all
```

---

## Test Coverage Goals

| Module | Current | Target |
|--------|---------|--------|
| config.rs | Unit tests | 90% |
| error.rs | Unit tests | 95% |
| client.rs | Unit tests (helpers) | 70% |
| client/push.rs | Unit tests (partial) | 60% |
| provider.rs | Unit tests | 80% |
| server.rs | None | 50% |
| main.rs | None | 40% |
| hls (new) | — | 60% |
| api (new) | — | 70% |
| ffmpeg (new) | — | 50% |

---

## Contributing

When adding new features:
1. Write tests first (TDD encouraged)
2. Ensure `cargo test` passes
3. Ensure `cargo clippy` has no warnings
4. Update this roadmap if adding new test categories
5. New modules must be feature-gated and work independently
