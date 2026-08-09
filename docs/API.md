# Reestream client API

This is the contract for the web dashboard. The versioned product API is
available under `/api/v1`. The older `/api/*` routes remain available as a
backward-compatible server interface; the dashboard uses only the versioned
routes.

The API is designed around the same product areas a multistream application
needs: destinations (channels), reusable stream drafts, scheduled events,
Studio sessions, unified chat, analytics, video storage, clips, and webhooks.
The API also exposes a provider-neutral OAuth contract. Provider credentials
are configured on the server; the browser receives an authorization URL and a
redacted connection summary, never provider access tokens.

## Conventions

Default local base URL:

```text
http://localhost:8080/api/v1
```

JSON responses use one envelope:

```json
{
  "success": true,
  "data": {},
  "meta": { "page": 1, "limit": 50, "total": 1 }
}
```

`meta` is present on list endpoints. Errors use HTTP status plus a stable
machine-readable code:

```json
{
  "success": false,
  "data": null,
  "error": {
    "code": "invalid_request",
    "message": "streamUrl is required"
  }
}
```

List parameters:

| Parameter | Meaning | Default |
| --- | --- | --- |
| `page` | 1-based page number | `1` |
| `limit` | Page size, capped at 200 | `50` |
| `q` | Name/label search where supported | none |
| `status` | Resource-specific status filter | none |
| `from`, `to` | ISO-8601 analytics range | none |

All JSON fields use `camelCase`. IDs are opaque strings and must not be parsed
as numeric values by the client.

## Authentication

Local development has authentication disabled unless either
`RESTREAM_AUTH_REQUIRED=true` or `RESTREAM_ADMIN_PASSWORD` is set. Production
deployments should enable it and set:

```bash
export RESTREAM_AUTH_REQUIRED=true
export RESTREAM_ADMIN_EMAIL=owner@example.com
export RESTREAM_ADMIN_PASSWORD='use-a-secret-password'
```

Set both RESTREAM_AUTH_REQUIRED=true and RESTREAM_ADMIN_PASSWORD; enabling
authentication without a password intentionally fails closed, so no login can
succeed. GET /status, health, platform catalog, ingest catalog, and the
one-time setup endpoints remain public for dashboard bootstrap. Private
versioned resources, legacy control routes, WebSockets, and metrics require a
valid bearer token.

### `POST /auth/login`

Request:

```json
{ "email": "owner@example.com", "password": "..." }
```

Response data contains `accessToken`, `refreshToken`, `tokenType`, and
`expiresIn`. Send the access token on private requests:

```http
Authorization: Bearer <accessToken>
```

### `POST /auth/refresh`

```json
{ "refreshToken": "..." }
```

### `POST /auth/logout`

Revokes the Bearer access token when one is supplied.

### `GET /me`, `GET /profile`, `PATCH /me`

Profile data includes `id`, `username`, `email`, `displayName`, `timezone`, and
`avatarUrl`. The profile patch accepts `displayName`, `timezone`, and
`avatarUrl`.
`/profile` and `/user/profile` are equivalent aliases. `/ingest` and
`/user/ingest` expose the selected local ingest, while `/stream-key` and
`/user/streamKey` expose the protected global encoder credentials.

### Provider OAuth

| Method | Path | Purpose |
| --- | --- | --- |
| `GET` | `/connections` | List redacted connected provider accounts |
| `DELETE` | `/connections/{id}` | Remove a stored provider connection |
| `GET` | `/oauth/{platform}/authorize` | Build a CSRF-bound provider authorize URL |
| `POST` | `/oauth/{platform}/token` | Exchange an authorization or refresh token |

Configure a provider with environment variables named after its platform ID:

```bash
RESTREAM_OAUTH_YOUTUBE_CLIENT_ID=...
RESTREAM_OAUTH_YOUTUBE_CLIENT_SECRET=...
RESTREAM_OAUTH_YOUTUBE_AUTHORIZE_URL=https://provider.example/authorize
RESTREAM_OAUTH_YOUTUBE_TOKEN_URL=https://provider.example/token
RESTREAM_OAUTH_YOUTUBE_SCOPES='stream chat read'
```

The code exchange body includes `code`, `redirectUri`, and the same `state`
returned by the authorize endpoint. Refresh exchanges use `refreshToken` and
do not require the one-time state. The token endpoint uses an OAuth
authorization-code or refresh-token form and HTTP Basic client authentication.
A successful exchange stores only in the
local service state and returns a connection summary. Provider-specific
stream-key/channel discovery remains behind the destination adapter; custom
RTMP/SRT channels are fully usable without OAuth.

## Public reference data

| Method | Path | Purpose |
| --- | --- | --- |
| `GET` | `/health` | API health and version |
| `GET` | `/openapi.json` | Lightweight machine-readable route document |
| `GET` | `/platforms` | Supported destination catalog and capabilities |
| `GET` | `/ingest-servers` | Available RTMP ingest servers |
| `GET` | `/servers` | Alias for the ingest-server catalog |

`/platforms` is catalog metadata, not the user's connected channels. Use
`/channels` for destinations configured by the current account. The local
state file contains the credentials needed by the relay and should be owned
and readable only by the service account.

## Channels (destinations)

| Method | Path | Purpose |
| --- | --- | --- |
| `GET` | `/channels` | List connected destinations |
| `POST` | `/channels` | Add a manually configured destination |
| `GET` | `/channels/{id}` | Read destination metadata |
| `PATCH` | `/channels/{id}` | Change destination metadata or enabled state |
| `DELETE` | `/channels/{id}` | Remove a destination |
| `GET` | `/channels/{id}/credentials` | Read stream URL/key for the setup UI |

Create a custom RTMP destination:

```json
{
  "platformId": "custom-rtmp",
  "displayName": "My YouTube backup",
  "channelUrl": "https://youtube.com/@me",
  "streamUrl": "rtmp://a.rtmp.youtube.com/live2",
  "streamKey": "provider-secret"
}
```

The response never serializes `streamKey`, `rtmpUsername`, or
`rtmpPassword`. Fetch credentials only when the user explicitly opens the
setup view. A channel mutation is immediately bridged to the running relay.

## Reusable streams and events

There are two related resources:

1. A **stream draft** is a reusable setup (name, stream type, title,
   destinations, brand).
2. An **event** is one scheduled or live run of a setup.

### Drafts

| Method | Path | Purpose |
| --- | --- | --- |
| `GET` | `/streams` | List reusable drafts |
| `POST` | `/streams` | Create a draft |
| `GET` | `/streams/{id}` | Read a draft |
| `PATCH` | `/streams/{id}` | Update a draft |
| `DELETE` | `/streams/{id}` | Delete a draft |
| `POST` | `/streams/{id}/duplicate` | Copy a draft |

`streamType` is one of `studio`, `encoder`, `file`, or `playlist`.

### Events

| Method | Path | Purpose |
| --- | --- | --- |
| `GET` | `/events` | List events; accepts `status` |
| `POST` | `/events` | Create an instant, scheduled, or file event |
| `GET` | `/events/upcoming` | Scheduled events |
| `GET` | `/events/live` | In-progress events |
| `GET` | `/events/history` | Ended events |
| `GET` | `/events/{id}` | Event details |
| `PATCH` | `/events/{id}` | Update title, description, time, destinations |
| `DELETE` | `/events/{id}` | Delete/cancel an event |
| `POST` | `/events/{id}/destinations` | Attach a channel |
| `DELETE` | `/events/{id}/destinations/{channelId}` | Detach a channel |
| `GET` | `/events/{id}/stream-key` | Encoder ingest credentials |
| `GET` | `/events/{id}/srt-keys` | SRT ingest credentials |
| `POST` | `/events/{id}/go-live` | Transition to `live` |
| `POST` | `/events/{id}/end` | Transition to `ended` |
| `POST` | `/events/{id}/viewers` | Record a viewer/bitrate sample |
| `GET` | `/events/{id}/recordings` | Files linked to the event |
| `POST` | `/events/{id}/recordings/start` | Start recording while the event is live |
| `POST` | `/events/{id}/recordings/stop` | Stop the active event recording |
| `POST` | `/events/{id}/recordings/download-url` | Resolve a recording by `fileName` |
| `GET/POST` | `/events/{id}/recordings/transcriptions` | Read or request transcription work |
| `GET` | `/events/{id}/chat` | Event chat history |
| `POST` | `/events/{id}/chat/history/download-url` | Resolve the CSV chat export |
| `GET` | `/events/{id}/analytics` | Event analytics and time series |
| `GET` | `/events/{id}/analytics/viewers` | Restream-style viewer totals/by-channel |
| `GET` | `/events/{id}/analytics/messages` | Restream-style chat totals/by-channel |
| `GET/POST` | `/events/{id}/viewers` | Read or record viewer samples |
| `GET/POST` | `/events/{id}/transcriptions` | Alias for recording transcriptions |
| `GET` | `/events/{id}/chat-export` | CSV export of chat history |

Create a scheduled encoder event:

```json
{
  "streamType": "encoder",
  "title": "Weekly show",
  "description": "Episode 12",
  "scheduledFor": "2026-08-20T15:00:00Z",
  "destinationIds": ["channel-id"]
}
```

Create a pre-recorded event by setting `streamType` to `file` or `playlist`,
providing `fileId`, and optionally `loopsCount` from 0 to 9.

An event response intentionally omits the ingest stream key. The setup screen
uses `/events/{id}/stream-key` after the user chooses an encoder.

Lifecycle values are `draft`, `scheduled`, `live`, `ended`, and `cancelled`.
The server promotes due `scheduled` events once per second. File and playlist
events are played through FFmpeg into their event ingest key; encoder events
wait for an RTMP/SRT publisher. When the server recording manager is enabled,
live events create a `recording` storage file and ending the event finalizes
its status and size. By default recordings consume the local HTTP-FLV preview;
set `RESTREAM_RECORDING_INPUT_URL` when the deployment uses another ingest
source. Set `RESTREAM_RECORDING_ENABLED=false` to disable automatic event
recording while retaining the manual legacy recording API.

When built with the `srt` feature, SRT ingest is disabled unless
RESTREAM_SRT_ENABLED=true and RESTREAM_SRT_PASSPHRASE are both set. It listens
on RESTREAM_SRT_PORT (default 3000) and forwards MPEG-TS packets to the event
key encoded in the SRT `streamid` query. The passphrase shown by
`/events/{id}/srt-keys` must contain at least 10 characters.

RTMPS is exposed only when configured. Set `RESTREAM_RTMPS_URL` to an
`rtmps://` endpoint supplied by a TLS terminator or external ingest service;
the dashboard then shows it as the backup protocol. Destination URLs using
`rtmps://` are also supported by the outbound relay.

## Studio

| Method | Path | Purpose |
| --- | --- | --- |
| `GET` | `/studio/sessions` | List Studio sessions |
| `POST` | `/studio/sessions` | Create/get a session for an event |
| `GET` | `/studio/sessions/{id}` | Read Studio state |
| `PATCH` | `/studio/sessions/{id}` | Update layout/settings/status |
| `POST` | `/studio/sessions/{id}/start` | Start the Studio session |
| `POST` | `/studio/sessions/{id}/end` | End the Studio session |
| `POST` | `/studio/sessions/{id}/guests` | Invite a guest |
| `DELETE` | `/studio/sessions/{id}/guests/{guestId}` | Remove a guest |
| `POST` | `/studio/sessions/{id}/scenes` | Add a scene |
| `PATCH` | `/studio/sessions/{id}/scenes/{sceneId}` | Update/activate a scene |
| `GET/POST` | `/studio/brands` | List/create branding presets |
| `PATCH/DELETE` | `/studio/brands/{id}` | Update/delete a brand |
| `GET/POST` | `/studio/captions` | List/create caption presets |
| `PATCH/DELETE` | `/studio/captions/{id}` | Update/delete captions |
| `GET/POST` | `/studio/qr-codes` | List/create QR overlays |
| `PATCH/DELETE` | `/studio/qr-codes/{id}` | Update/delete QR overlays |
| `PATCH` | `/studio/qr-codes/reorder` | Persist UI ordering request |
| `GET/POST` | `/studio/tickers` | List/create ticker overlays |
| `PATCH/DELETE` | `/studio/tickers/{id}` | Update/delete tickers |
| `PATCH` | `/studio/tickers/reorder` | Persist UI ordering request |
| `GET` | `/studio/fonts` | Font catalog |
| `GET` | `/studio/audio/countdown` | Storage files labeled `countdown` |
| `GET` | `/studio/audio/backgrounds` | Storage files labeled `background` |

Studio guest links are safe to share with guests and are separate from the
authenticated host API.

## Chat

### REST

| Method | Path | Purpose |
| --- | --- | --- |
| `GET` | `/chat/messages?eventId={id}` | Read unified chat |
| `POST` | `/chat/messages` | Send a destination-specific message |
| `DELETE` | `/chat/messages/{id}` | Soft-delete a message |
| `GET` | `/chat/sources` | List chat-capable channel sources |
| `GET` | `/chat/actions` | List supported chat actions |
| `GET` | `/chat/connections` | List channel chat connection status |
| `GET` | `/chat/events` | List events available to chat |
| `POST` | `/chat/reply` | Send a reply; same body as a message |
| `POST` | `/chat/relay` | Send a relay-bot message to the event |
| `GET` | `/events/{id}/chat` | Event-scoped history |

Message body:

```json
{
  "eventId": "event-id",
  "destinationId": "optional-channel-id",
  "authorName": "Host",
  "message": "Welcome!",
  "replyTo": "optional-message-id"
}
```

Omitting `destinationId` means the message is broadcast by the local chat
plane. A provider adapter can fan it out to the connected platforms.

### WebSocket

Connect to `/chat/ws?eventId={id}` with the same Bearer token. Prefer the
`Sec-WebSocket-Protocol: reestream-bearer-<access-token>` handshake header for
browser clients; the older `access_token` query parameter remains supported
for compatibility but can expose tokens in URL logs. The first frame:

```json
{ "type": "init", "messages": [] }
```

New frames:

```json
{ "type": "message", "message": { "id": "...", "message": "Hi" } }
```

`/streaming/ws` uses the same pattern with `events` in the initial frame and
`notification` frames for event lifecycle changes.

## Analytics

| Method | Path | Purpose |
| --- | --- | --- |
| `GET` | `/analytics/overview?from=&to=` | Account-level totals and stream reports |
| `GET` | `/analytics/timeseries?eventId=` | Viewer/bitrate samples |
| `GET` | `/events/{id}/analytics` | One event's full report |
| `GET` | `/events/{id}/analytics/viewers` | Mean/max/views/watched-time totals |
| `GET` | `/events/{id}/analytics/messages` | Message/chatter totals and rates |

Reports contain views, peak and average concurrent viewers, chat count,
duration, destination breakdown, and a `timeseries` array.

## Storage and clips

| Method | Path | Purpose |
| --- | --- | --- |
| `GET` | `/storage/files` | Search/list stored files |
| `POST` | `/storage/files` | Multipart upload (`file`, optional `name`, `labels`) |
| `POST` | `/storage/metadata` | Register metadata for an existing local file |
| `GET` | `/storage/files/{id}` | File metadata |
| `PATCH` | `/storage/files/{id}` | Rename/update labels |
| `DELETE` | `/storage/files/{id}` | Delete metadata and local file |
| `GET` | `/storage/files/{id}/download` | Download local file |
| `POST` | `/storage/files/{id}/download-url` | Get `downloadUrl`/`url` for a client download |
| `GET` | `/clips/projects` | List clip projects |
| `POST` | `/clips/projects` | Create a clip from an event time range |
| `GET` | `/clips/projects/{id}` | Read clip status |
| `DELETE` | `/clips/projects/{id}` | Delete a clip project |
| `GET` | `/clips/projects/{id}/download` | Download a ready clip |

Multipart uploads stream directly to the storage root and are capped at 2 GiB
by default. Set RESTREAM_MAX_UPLOAD_BYTES to change the limit. Download paths
are constrained to files managed below the storage root; filenames are
sanitized for response headers.

Clip request:

```json
{
  "eventId": "event-id",
  "name": "Best moment",
  "startSeconds": 120,
  "endSeconds": 155
}
```

Clip creation returns `processing` when the event has a local source file and
FFmpeg work has started, `ready` after the output is persisted, or `failed`
when no usable source is available. Downloading a processing clip returns
`409`; downloading a failed clip returns `422`.

### Transcriptions

Transcription records use the statuses `InProgress`, `Completed`, `Failed`,
and `Unknown`. Without a configured speech-to-text executable, a request is
stored as `Unknown` rather than pretending that transcription completed. To
enable the local worker, configure a command that accepts the source media
path followed by a destination text path:

```bash
RESTREAM_TRANSCRIBER_BIN=/usr/local/bin/my-transcriber
RESTREAM_TRANSCRIBER_LANGUAGE=en
```

The worker creates a text storage file and exposes its protected download URL
when the command exits successfully.

## Webhooks

| Method | Path | Purpose |
| --- | --- | --- |
| `GET` | `/webhooks` | List subscriptions |
| `POST` | `/webhooks` | Create a subscription |
| `PATCH` | `/webhooks/{id}` | Update URL/events/enabled |
| `DELETE` | `/webhooks/{id}` | Delete a subscription |
| `POST` | `/webhooks/{id}/test` | Deliver a test request |

Example:

```json
{
  "url": "https://example.com/reestream-events",
  "secret": "shared-secret",
  "events": ["event.started", "event.ended", "chat.message"],
  "enabled": true
}
```

Webhook payloads include `event`, `eventId`, `timestamp`, and `data`. The
same lifecycle messages are also available through `/streaming/ws`.
When `secret` is configured, verify the `X-Reestream-Signature` header as
`sha256=<HMAC-SHA256 of the raw JSON body>`.

## Legacy compatibility routes

The server still exposes the older `/api/*` and `/ws/streams` routes for
external clients. The dashboard no longer depends on them. The media routes
`/stream.m3u8`, `/hls/{filename}`, and `/stream.flv` remain active because the
preview player consumes the media stream directly rather than through the
JSON API.

## Runtime and security notes

RESTREAM_HTTP_ADDR and RESTREAM_HTTP_PORT control the HTTP listener;
RESTREAM_PUBLIC_HOST controls advertised ingest URLs. Listener changes made
through setup/config APIs return restartRequired: true. Runtime stream keys
and configured output destinations are updated immediately, while the process
must be restarted to move a bound listener.

The main state file is config.state.json; credentials are encrypted in the
adjacent config.state.secrets file using RESTREAM_STATE_KEY or the private
generated config.state.key. Do not commit these files. Webhook URLs reject
literal localhost/private targets, and media inputs reject private targets
unless RESTREAM_ALLOW_PRIVATE_MEDIA_INPUTS=true is explicitly set. Deploy DNS
and network egress controls as an additional defense against DNS rebinding.

## Client implementation guidance

- Keep access tokens in memory where possible; use refresh tokens only in a
  secure, same-origin session mechanism.
- Treat `event.status` as the source of truth and update cards from
  `/streaming/ws`, then reconcile with `GET /events` after reconnect.
- Never render or log `streamKey`, `rtmpPassword`, or `secret` values.
- Use optimistic UI only for cosmetic edits. For lifecycle transitions, wait
  for the server response and handle 409/422 responses explicitly.
- Use `GET /events/{id}/chat` for history, then switch to the WebSocket for
  live updates.
- Use `GET /openapi.json` as a route-discovery aid; this document is the
  authoritative request/response behavior reference.
