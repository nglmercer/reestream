# Future web rebuild guide

This document describes the smallest client architecture that can consume the
Reestream backend without coupling UI state to RTMP implementation details.

## Suggested client state

```text
session
  profile
  accessToken / refreshToken
home
  drafts
  upcomingEvents
  liveEvents
  pastEvents
channels
  catalog
  connectedChannels
  oauthConnections
studio
  session
  scenes / guests / brands / overlays
event
  details
  credentials (short-lived view state only)
  chatHistory + chatSocket
  analytics
storage
  files
  clipProjects
```

The server's product store is the control plane. The media player should use
the existing HLS/FLV URLs and never infer stream state from the player alone.

## Home screen flow

1. Load `/me`, `/channels`, `/streams`, `/events/upcoming`, and `/events/live`.
2. Render drafts as reusable cards; an event card is a specific run.
3. For “New stream”, create a draft first, then create an event from its
   fields or create an event directly.
4. For “Go live”, call `POST /events/{id}/go-live`, then subscribe to
   `/streaming/ws`.
5. For “End stream”, call `POST /events/{id}/end`; refetch analytics and
   recordings after the response.
6. Scheduled events are promoted by the server; reconcile from
   `/streaming/ws` after reconnect instead of maintaining a browser timer.

## Channel setup flow

1. Load `/platforms` for the catalog and capability flags.
2. For a configured OAuth provider, request `/oauth/{platform}/authorize` with
   a random `state`, redirect the browser, then send the returned code to
   `/oauth/{platform}/token`. Store only the returned connection summary.
   Provider-specific channel discovery may then create `/channels` records.
3. For Custom RTMP/SRT, collect URL/key in the UI and `POST /channels`.
4. Refresh `/channels` after every mutation. Credentials are read only from
   `/channels/{id}/credentials` when the setup panel is open.

## Studio flow

1. Create an event with `streamType: "studio"`.
2. `POST /studio/sessions` with the event ID.
3. Load session scenes and guests; persist edits with PATCH/POST calls.
4. Use `activeSceneId` and `layout` for the local renderer.
5. Start/end the session through the session endpoints; these also transition
   the associated event and keep event state synchronized through
   `/streaming/ws`.
6. Label uploaded audio files `countdown` or `background` to make them appear
   in the Studio audio catalogs.

## Chat flow

```text
GET event history ──┐
                    ├─ render ordered messages
WebSocket updates ──┘
```

Reconnect with exponential backoff. After reconnect, fetch history again and
deduplicate by message ID. Sending a message is an HTTP POST; the WebSocket
is the update channel, not the write channel.

## Upload flow

```text
select file
  -> POST multipart /storage/files
  -> store returned file.id
  -> create event { streamType: "file", fileId }
  -> schedule event
```

For a `file` or `playlist` event, the server validates that the source is in
managed storage, starts FFmpeg at go-live (or at its scheduled time), and
stops playback when the event ends. Recording files are linked to the event;
use `/events/{id}/recordings` and `/events/{id}/recordings/transcriptions` to
refresh the post-event media panel.

For large files, the current API is a single multipart request. The client
should isolate the uploader behind an adapter so resumable uploads can be
added later without changing event screens.

## Error handling

Use HTTP status and `error.code` together:

| Status | Client behavior |
| --- | --- |
| `400` | Show field-level validation |
| `401` | Refresh once, then redirect to login |
| `404` | Remove stale resource from local cache |
| `409` | Refetch the resource and show a lifecycle conflict |
| `422` | Show a domain validation message |
| `502` | Keep webhook/provider action retryable |

For recording/file playback, also handle `409`/`422` as a lifecycle or media
availability failure and refetch the event before retrying.

## Media URLs

The existing local media endpoints are:

- `GET /stream.m3u8` for HLS;
- `GET /stream.flv` for low-latency FLV;
- `GET /hls/{filename}` for HLS segments.

Use HLS.js where Media Source Extensions are available and fall back to the
native HLS player on Safari. Keep the player in a separate component from the
event/chat/analytics state.
