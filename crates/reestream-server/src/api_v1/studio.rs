//! Studio sessions and studio asset handlers for the versioned API.

use super::*;

pub(super) async fn list_studio_sessions(State(state): State<AppState>) -> Response {
    ok(state.restream.list_studio_sessions().await)
}

pub(super) async fn create_studio_session(
    State(state): State<AppState>,
    Json(request): Json<StudioSessionRequest>,
) -> Response {
    match state
        .restream
        .get_or_create_studio_session(&request.event_id)
        .await
    {
        Some(session) => {
            if request.layout.is_some() || request.settings.is_some() {
                let _ = state
                    .restream
                    .update_studio_session(&session.id, request.layout, request.settings, None)
                    .await;
            }
            match state
                .restream
                .get_or_create_studio_session(&request.event_id)
                .await
            {
                Some(session) => created(session),
                None => not_found("event"),
            }
        }
        None => not_found("event"),
    }
}

pub(super) async fn get_studio_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    match state
        .restream
        .list_studio_sessions()
        .await
        .into_iter()
        .find(|session| session.id == id)
    {
        Some(session) => ok(session),
        None => not_found("Studio session"),
    }
}

pub(super) async fn update_studio_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<StudioSessionPatch>,
) -> Response {
    match state
        .restream
        .update_studio_session(&id, request.layout, request.settings, request.status)
        .await
    {
        Some(session) => ok(session),
        None => not_found("Studio session"),
    }
}

pub(super) async fn start_studio_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    let Some(session) = state
        .restream
        .list_studio_sessions()
        .await
        .into_iter()
        .find(|session| session.id == id)
    else {
        return not_found("Studio session");
    };
    let Some(event) = state.restream.get_event(&session.event_id).await else {
        return not_found("event");
    };
    let event = match state.restream.set_event_live(&event.id).await {
        Some(event) => event,
        None => {
            return error(
                StatusCode::CONFLICT,
                "event_not_startable",
                "event cannot go live",
            );
        }
    };
    let _ = start_event_recording(&state.recording_manager, &state.restream, &event).await;
    if let Err(message) =
        start_event_playback(&state.playback_manager, &state.restream, &event).await
    {
        finish_event_recording(&state.recording_manager, &state.restream, &event).await;
        let _ = state.restream.cancel_event(&event.id).await;
        return error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "playback_unavailable",
            message,
        );
    }
    match state
        .restream
        .update_studio_session(&id, None, None, Some("live".into()))
        .await
    {
        Some(session) => ok(session),
        None => not_found("Studio session"),
    }
}

pub(super) async fn end_studio_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    let session = state
        .restream
        .list_studio_sessions()
        .await
        .into_iter()
        .find(|session| session.id == id);
    if let Some(session) = session
        && let Some(event) = state.restream.end_event(&session.event_id).await
    {
        finish_event_recording(&state.recording_manager, &state.restream, &event).await;
        finish_event_playback(&state.playback_manager, &event.id).await;
    }
    match state
        .restream
        .update_studio_session(&id, None, None, Some("ended".into()))
        .await
    {
        Some(session) => ok(session),
        None => not_found("Studio session"),
    }
}

pub(super) async fn add_guest(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<GuestRequest>,
) -> Response {
    match state
        .restream
        .add_guest(
            &id,
            request.name,
            request.role.unwrap_or_else(|| "guest".into()),
        )
        .await
    {
        Some(guest) => created(guest),
        None => not_found("Studio session"),
    }
}

pub(super) async fn remove_guest(
    State(state): State<AppState>,
    Path((id, guest_id)): Path<(String, String)>,
) -> Response {
    if state.restream.remove_guest(&id, &guest_id).await {
        ok(json!({"deleted": true, "id": guest_id}))
    } else {
        not_found("guest")
    }
}

pub(super) async fn add_scene(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<SceneRequest>,
) -> Response {
    match state
        .restream
        .add_scene(
            &id,
            request.name,
            request.layout.unwrap_or_else(|| "grid".into()),
            request.source_ids.unwrap_or_default(),
        )
        .await
    {
        Some(scene) => created(scene),
        None => not_found("Studio session"),
    }
}

pub(super) async fn update_scene(
    State(state): State<AppState>,
    Path((id, scene_id)): Path<(String, String)>,
    Json(request): Json<ScenePatch>,
) -> Response {
    match state
        .restream
        .update_scene(
            &id,
            &scene_id,
            request.name,
            request.layout,
            request.source_ids,
            request.active,
        )
        .await
    {
        Some(scene) => ok(scene),
        None => not_found("scene"),
    }
}

fn brand_from_value(value: Value) -> Brand {
    let timestamp = now();
    Brand {
        id: String::new(),
        name: value
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("New brand")
            .into(),
        logo_url: value.get("logoUrl").and_then(Value::as_str).map(Into::into),
        primary_color: value
            .get("primaryColor")
            .and_then(Value::as_str)
            .unwrap_or("#ffffff")
            .into(),
        secondary_color: value
            .get("secondaryColor")
            .and_then(Value::as_str)
            .unwrap_or("#000000")
            .into(),
        font_family: value
            .get("fontFamily")
            .and_then(Value::as_str)
            .unwrap_or("Inter")
            .into(),
        created_at: timestamp,
        updated_at: timestamp,
    }
}

pub(super) async fn list_brands(State(state): State<AppState>) -> Response {
    ok(state.restream.list_brands().await)
}

pub(super) async fn create_brand(
    State(state): State<AppState>,
    Json(value): Json<Value>,
) -> Response {
    created(state.restream.create_brand(brand_from_value(value)).await)
}

pub(super) async fn update_brand(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(value): Json<Value>,
) -> Response {
    match state.restream.update_brand(&id, value).await {
        Some(brand) => ok(brand),
        None => not_found("brand"),
    }
}

pub(super) async fn delete_brand(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    if state.restream.delete_brand(&id).await {
        ok(json!({"deleted": true, "id": id}))
    } else {
        not_found("brand")
    }
}

fn caption_from_value(value: Value) -> Caption {
    let timestamp = now();
    Caption {
        id: String::new(),
        name: value
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("Captions")
            .into(),
        language: value
            .get("language")
            .and_then(Value::as_str)
            .unwrap_or("en")
            .into(),
        style: value.get("style").cloned().unwrap_or_else(|| json!({})),
        enabled: value
            .get("enabled")
            .and_then(Value::as_bool)
            .unwrap_or(true),
        created_at: timestamp,
        updated_at: timestamp,
    }
}

pub(super) async fn list_captions(State(state): State<AppState>) -> Response {
    ok(state.restream.list_captions().await)
}

pub(super) async fn create_caption(
    State(state): State<AppState>,
    Json(value): Json<Value>,
) -> Response {
    created(
        state
            .restream
            .create_caption(caption_from_value(value))
            .await,
    )
}

pub(super) async fn update_caption(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(value): Json<Value>,
) -> Response {
    match state.restream.update_caption(&id, value).await {
        Some(caption) => ok(caption),
        None => not_found("caption"),
    }
}

pub(super) async fn delete_caption(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    if state.restream.delete_caption(&id).await {
        ok(json!({"deleted": true, "id": id}))
    } else {
        not_found("caption")
    }
}

fn qr_from_value(value: Value) -> QrCode {
    let timestamp = now();
    QrCode {
        id: String::new(),
        name: value
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("QR code")
            .into(),
        data: value
            .get("data")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .into(),
        foreground: value
            .get("foreground")
            .and_then(Value::as_str)
            .unwrap_or("#000000")
            .into(),
        background: value
            .get("background")
            .and_then(Value::as_str)
            .unwrap_or("#ffffff")
            .into(),
        enabled: value
            .get("enabled")
            .and_then(Value::as_bool)
            .unwrap_or(true),
        position: value
            .get("position")
            .and_then(Value::as_str)
            .unwrap_or("bottom-right")
            .into(),
        created_at: timestamp,
        updated_at: timestamp,
    }
}

pub(super) async fn list_qr_codes(State(state): State<AppState>) -> Response {
    ok(state.restream.list_qr_codes().await)
}

pub(super) async fn create_qr_code(
    State(state): State<AppState>,
    Json(value): Json<Value>,
) -> Response {
    created(state.restream.create_qr_code(qr_from_value(value)).await)
}

pub(super) async fn update_qr_code(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(value): Json<Value>,
) -> Response {
    match state.restream.update_qr_code(&id, value).await {
        Some(qr_code) => ok(qr_code),
        None => not_found("QR code"),
    }
}

pub(super) async fn delete_qr_code(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    if state.restream.delete_qr_code(&id).await {
        ok(json!({"deleted": true, "id": id}))
    } else {
        not_found("QR code")
    }
}

pub(super) async fn reorder_qr_codes(
    State(state): State<AppState>,
    Json(request): Json<ReorderRequest>,
) -> Response {
    ok(state.restream.reorder_qr_codes(&request.ids).await)
}

fn ticker_from_value(value: Value) -> Ticker {
    let timestamp = now();
    Ticker {
        id: String::new(),
        text: value
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .into(),
        speed: value.get("speed").and_then(Value::as_u64).unwrap_or(40) as u32,
        color: value
            .get("color")
            .and_then(Value::as_str)
            .unwrap_or("#ffffff")
            .into(),
        background_color: value
            .get("backgroundColor")
            .and_then(Value::as_str)
            .unwrap_or("#000000")
            .into(),
        enabled: value
            .get("enabled")
            .and_then(Value::as_bool)
            .unwrap_or(true),
        order: value.get("order").and_then(Value::as_u64).unwrap_or(0) as u32,
        created_at: timestamp,
        updated_at: timestamp,
    }
}

pub(super) async fn list_tickers(State(state): State<AppState>) -> Response {
    ok(state.restream.list_tickers().await)
}

pub(super) async fn create_ticker(
    State(state): State<AppState>,
    Json(value): Json<Value>,
) -> Response {
    created(state.restream.create_ticker(ticker_from_value(value)).await)
}

pub(super) async fn update_ticker(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(value): Json<Value>,
) -> Response {
    match state.restream.update_ticker(&id, value).await {
        Some(ticker) => ok(ticker),
        None => not_found("ticker"),
    }
}

pub(super) async fn delete_ticker(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    if state.restream.delete_ticker(&id).await {
        ok(json!({"deleted": true, "id": id}))
    } else {
        not_found("ticker")
    }
}

pub(super) async fn reorder_tickers(
    State(state): State<AppState>,
    Json(request): Json<ReorderRequest>,
) -> Response {
    ok(state.restream.reorder_tickers(&request.ids).await)
}

pub(super) async fn list_fonts() -> Response {
    ok(vec!["Inter", "Roboto", "Open Sans", "Montserrat", "Arial"])
}

pub(super) async fn list_countdown_audio(State(state): State<AppState>) -> Response {
    let files = state
        .restream
        .list_files(None)
        .await
        .into_iter()
        .filter(|file| {
            file.labels
                .iter()
                .any(|label| label.eq_ignore_ascii_case("countdown"))
        })
        .map(public_file)
        .collect::<Vec<_>>();
    ok(files)
}

pub(super) async fn list_background_audio(State(state): State<AppState>) -> Response {
    let files = state
        .restream
        .list_files(None)
        .await
        .into_iter()
        .filter(|file| {
            file.labels
                .iter()
                .any(|label| label.eq_ignore_ascii_case("background"))
        })
        .map(public_file)
        .collect::<Vec<_>>();
    ok(files)
}
