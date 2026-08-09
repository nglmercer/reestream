//! Webhook subscription handlers for the versioned API.

use super::*;
use crate::restream::WebhookSubscription;

fn webhook_from_request(request: WebhookRequest) -> WebhookSubscription {
    WebhookSubscription {
        id: String::new(),
        url: request.url.unwrap_or_default(),
        secret: request.secret,
        events: request
            .events
            .unwrap_or_else(|| vec!["event.started".into(), "event.ended".into()]),
        enabled: request.enabled.unwrap_or(true),
        created_at: 0,
        updated_at: 0,
    }
}

pub(super) async fn list_webhooks(State(state): State<AppState>) -> Response {
    let hooks = state.restream.list_webhooks().await;
    ok(hooks
        .into_iter()
        .map(|hook| {
            json!({"id": hook.id, "url": hook.url, "events": hook.events, "enabled": hook.enabled, "createdAt": hook.created_at, "updatedAt": hook.updated_at})
        })
        .collect::<Vec<_>>())
}

pub(super) async fn create_webhook(
    State(state): State<AppState>,
    Json(request): Json<WebhookRequest>,
) -> Response {
    let Some(url) = request.url.as_deref() else {
        return bad_request("url is required");
    };
    if let Err(response) = parse_http_url(url, "url") {
        return response;
    }
    let hook = state
        .restream
        .create_webhook(webhook_from_request(request))
        .await;
    ok(
        json!({"id": hook.id, "url": hook.url, "events": hook.events, "enabled": hook.enabled, "createdAt": hook.created_at, "updatedAt": hook.updated_at}),
    )
}

pub(super) async fn update_webhook(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<WebhookRequest>,
) -> Response {
    if let Some(ref url) = request.url
        && let Err(response) = parse_http_url(url, "url")
    {
        return response;
    }
    match state
        .restream
        .update_webhook(
            &id,
            request.url,
            request.secret,
            request.events,
            request.enabled,
        )
        .await
    {
        Some(hook) => ok(
            json!({"id": hook.id, "url": hook.url, "events": hook.events, "enabled": hook.enabled, "createdAt": hook.created_at, "updatedAt": hook.updated_at}),
        ),
        None => not_found("webhook"),
    }
}

pub(super) async fn delete_webhook(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    if state.restream.delete_webhook(&id).await {
        ok(json!({"deleted": true, "id": id}))
    } else {
        not_found("webhook")
    }
}

pub(super) async fn test_webhook(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    let hook = state
        .restream
        .list_webhooks()
        .await
        .into_iter()
        .find(|hook| hook.id == id);
    let Some(hook) = hook else {
        return not_found("webhook");
    };
    let payload = json!({"event": "webhook.test", "timestamp": now(), "data": {"healthy": true}});
    let client = match crate::restream::build_safe_http_client(
        &hook.url,
        std::time::Duration::from_secs(10),
    )
    .await
    {
        Ok(client) => client,
        Err(client_error) => {
            return error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "webhook_client",
                client_error.to_string(),
            );
        }
    };
    match client.post(&hook.url).json(&payload).send().await {
        Ok(response) => ok(
            json!({"delivered": response.status().is_success(), "status": response.status().as_u16()}),
        ),
        Err(request_error) => error(
            StatusCode::BAD_GATEWAY,
            "webhook_failed",
            request_error.to_string(),
        ),
    }
}
