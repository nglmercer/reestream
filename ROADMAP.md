# Reestream Roadmap

## Current Status (v0.2.0)
- Workspace architecture with 3 crates
- Basic RTMP relay server with multistream forwarding
- TLS/RTMPS support with reconnection logic
- Configuration via TOML with ConfigBuilder pattern
- FFmpeg integration (binary resolver, command builder, process supervisor)
- HLS segmenter with playlist generation
- REST API with stream/platform management
- HTTP server with axum (HLS serving, metrics, health check)
- 196 tests passing, clippy clean

---

## Build System: Feature Flags

```toml
[features]
default = ["core"]
core = []                          # RTMP relay + multistream
hls = []                           # HLS/HTTP server
api = ["serde_json"]               # REST API
ffmpeg = []                        # FFmpeg process management
all = ["hls", "api", "ffmpeg"]
```

### Build targets
```bash
# Core only (RTMP relay, minimal binary)
cargo build --release --no-default-features --features core

# Core + HLS server
cargo build --release --features core,hls

# Core + API
cargo build --release --features core,api

# Everything
cargo build --release --features all
```

---

## Phase 0: Architecture Refactor ✅ DONE
- [x] Workspace restructure (reestream-core, reestream-ffmpeg, reestream-server)
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
- [x] Serve segments at `/hls/:filename`

---

## Phase 6: REST API ✅ DONE
- [x] HTTP API server
- [x] `GET /health` — health check
- [x] `GET /api/status` — server health, uptime, version
- [x] `GET /api/streams` — list active streams
- [x] `POST /api/streams` — add stream
- [x] `DELETE /api/streams/:id` — remove stream
- [x] `GET /api/platforms` — list platforms
- [x] `POST /api/platforms` — add platform
- [x] `DELETE /api/platforms/:id` — remove platform
- [x] `PUT /api/platforms/:id/toggle` — toggle platform
- [x] `GET /metrics` — Prometheus-format metrics

---

## Phase 7: SRT Protocol (TODO)
- [ ] SRT input listener (feature-gated: `srt`)
- [ ] SRT output push (multistream to SRT destinations)
- [ ] SRT latency and congestion control config
- [ ] SRT passphrase encryption
- [ ] Bridge: SRT input → RTMP relay → HLS output

---

## Phase 8: Web UI (TODO)
- [ ] UI build strategy (embed pre-built or download)
- [ ] Dashboard — stream status, viewer count, uptime
- [ ] Stream setup wizard
- [ ] Platform management (add/remove/edit destinations)
- [ ] Stream preview player (HLS.js or flv.js)
- [ ] Log viewer (real-time streaming logs)
- [ ] i18n support

---

## Phase 9: Stream Processing Pipeline (TODO)
- [ ] Input sources: RTMP, SRT, File, RTSP, USB
- [ ] Processing: passthrough, transcode, resize, watermark
- [ ] Output: RTMP, SRT, HLS, FLV, File recording
- [ ] FLV container support (`/stream.flv` endpoint)
- [ ] Thumbnail/preview generation

---

## Phase 10: Monitoring & Observability ✅ DONE (partial)
- [x] Health check endpoint (`GET /health`)
- [x] Metrics endpoint (`GET /metrics`, Prometheus format)
- [x] `reestream_uptime_seconds`
- [x] `reestream_streams_total`
- [x] `reestream_viewers_total`
- [x] `reestream_stream_status` per stream
- [x] `reestream_stream_bitrate_kbps` per stream
- [ ] Structured logging (JSON output option)
- [ ] Webhook notifications (stream start/end/disconnect)

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

## Phase 12: Production Hardening (TODO)
- [ ] Graceful shutdown (drain in-flight packets)
- [ ] Rate limiting per connection
- [ ] Connection pool management
- [ ] Max viewer limit per stream
- [ ] Bandwidth limiting per stream
- [ ] Let's Encrypt auto-TLS (ACME)
- [ ] Config file watcher (hot-reload on change)
- [ ] Signal handlers (SIGHUP=reload, SIGTERM=shutdown)
- [ ] Fuzz testing for RTMP packet parsing
- [ ] Stress tests with 100+ concurrent streams

---

## Feature Parity with datarhei/restreamer

| Feature | restreamer | reestream |
|---|---|---|
| RTMP/S ingest | ✅ | ✅ |
| SRT ingest/output | ✅ | TODO Phase 7 |
| HLS HTTP server | ✅ | ✅ |
| HTTP-FLV streaming | ❌ | TODO Phase 9 |
| FFmpeg transcoding | ✅ | ✅ |
| HW accel (CUDA/VAAPI) | ✅ | ✅ |
| Web UI | ✅ | TODO Phase 8 |
| REST API | ✅ | ✅ |
| Viewer monitoring | ✅ | ✅ |
| Health check | ✅ | ✅ |
| Prometheus metrics | ✅ | ✅ |
| Docker multi-arch | ✅ | ✅ (Linux) |
| Stream recording | ❌ | TODO Phase 9 |
| Webhooks | ❌ | TODO Phase 10 |

---

## Testing Commands

```bash
# Run all tests
cargo test --workspace

# Run with output
cargo test --workspace -- --nocapture

# Run specific crate tests
cargo test -p reestream-core
cargo test -p reestream-ffmpeg
cargo test -p reestream-server

# Run clippy
cargo clippy --workspace --all-targets

# Run with coverage
cargo tarpaulin --workspace --out Html

# Build minimal binary
cargo build --release --no-default-features --features core

# Build with everything
cargo build --release --features all
```

---

## Test Coverage

| Module | Tests |
|--------|-------|
| reestream-core | 99 |
| reestream-ffmpeg | 23 |
| reestream-server | 9 |
| reestream (root) | 34 |
| integration tests | 31 |
| **Total** | **196** |
