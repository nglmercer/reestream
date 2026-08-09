//! Storage and multipart upload handlers for the versioned API.

use super::*;

use axum::{
    body::Body,
    extract::{Multipart, Path, Query, State, multipart::Field},
    http::{HeaderMap, HeaderValue, header},
    response::IntoResponse,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use uuid::Uuid;

fn max_upload_bytes() -> u64 {
    std::env::var("RESTREAM_MAX_UPLOAD_BYTES")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(2 * 1024 * 1024 * 1024)
}

async fn multipart_text(field: &mut Field<'_>, max_bytes: usize) -> Result<String, String> {
    let mut value = Vec::new();
    while let Some(chunk) = field.chunk().await.map_err(|error| error.to_string())? {
        if value.len().saturating_add(chunk.len()) > max_bytes {
            return Err(format!(
                "multipart text field exceeds the {max_bytes} byte limit"
            ));
        }
        value.extend_from_slice(&chunk);
    }
    String::from_utf8(value).map_err(|_| "multipart text field must be UTF-8".into())
}

fn safe_download_filename(value: &str) -> String {
    let safe: String = value
        .chars()
        .take(200)
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-') {
                character
            } else {
                '_'
            }
        })
        .collect();
    if safe.is_empty() {
        "download".into()
    } else {
        safe
    }
}

pub(super) async fn list_files(State(state): State<AppState>, query: Query<ListQuery>) -> Response {
    list(
        state
            .restream
            .list_files(query.q.as_deref())
            .await
            .into_iter()
            .map(public_file)
            .collect(),
        &query,
    )
}

pub(super) async fn upload_file(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Response {
    let mut name = None;
    let mut mime_type = "application/octet-stream".to_string();
    let mut labels = Vec::new();
    let upload_id = Uuid::new_v4().to_string();
    let temporary_path = state.restream.storage_path(&upload_id, "upload.part");
    let mut total_bytes = 0u64;
    let mut received_file = false;
    loop {
        let Some(mut field) = (match multipart.next_field().await {
            Ok(field) => field,
            Err(error) => {
                let _ = tokio::fs::remove_file(&temporary_path).await;
                return bad_request(format!("unable to read multipart upload: {error}"));
            }
        }) else {
            break;
        };
        let field_name = field.name().unwrap_or_default().to_string();
        if field_name == "file" {
            if received_file {
                let _ = tokio::fs::remove_file(&temporary_path).await;
                return bad_request("only one file field is supported");
            }
            received_file = true;
            if let Some(content_type) = field.content_type() {
                mime_type = content_type.to_string();
            }
            if name.is_none() {
                name = field.file_name().map(safe_download_filename);
            }
            if let Some(parent) = temporary_path.parent()
                && let Err(io_error) = tokio::fs::create_dir_all(parent).await
            {
                let _ = tokio::fs::remove_file(&temporary_path).await;
                return error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "storage_error",
                    io_error.to_string(),
                );
            }
            let mut file = match tokio::fs::File::create(&temporary_path).await {
                Ok(file) => file,
                Err(io_error) => {
                    let _ = tokio::fs::remove_file(&temporary_path).await;
                    return error(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "storage_error",
                        io_error.to_string(),
                    );
                }
            };
            loop {
                match field.chunk().await {
                    Ok(Some(chunk)) => {
                        total_bytes = total_bytes.saturating_add(chunk.len() as u64);
                        if total_bytes > max_upload_bytes() {
                            let _ = tokio::fs::remove_file(&temporary_path).await;
                            return error(
                                StatusCode::PAYLOAD_TOO_LARGE,
                                "upload_too_large",
                                format!(
                                    "file exceeds the {} byte upload limit",
                                    max_upload_bytes()
                                ),
                            );
                        }
                        if let Err(io_error) = file.write_all(&chunk).await {
                            let _ = tokio::fs::remove_file(&temporary_path).await;
                            return error(
                                StatusCode::INTERNAL_SERVER_ERROR,
                                "storage_error",
                                io_error.to_string(),
                            );
                        }
                    }
                    Ok(None) => break,
                    Err(error) => {
                        let _ = tokio::fs::remove_file(&temporary_path).await;
                        return bad_request(format!("unable to read upload: {error}"));
                    }
                }
            }
        } else if field_name == "name" {
            match multipart_text(&mut field, 255).await {
                Ok(value) => name = Some(value),
                Err(error) => {
                    let _ = tokio::fs::remove_file(&temporary_path).await;
                    return bad_request(format!("invalid name: {error}"));
                }
            }
        } else if field_name == "labels"
            && let Ok(value) = multipart_text(&mut field, 4096).await
        {
            labels = value
                .split(',')
                .map(str::trim)
                .filter(|label| !label.is_empty())
                .map(ToOwned::to_owned)
                .collect();
        }
    }
    let name = match name.filter(|value| !value.trim().is_empty()) {
        Some(value) => value,
        None => {
            let _ = tokio::fs::remove_file(&temporary_path).await;
            return bad_request("multipart field `file` is required");
        }
    };
    if !received_file {
        let _ = tokio::fs::remove_file(&temporary_path).await;
        return bad_request("multipart field `file` is required");
    }
    let path = state.restream.storage_path(&upload_id, &name);
    if let Err(io_error) = tokio::fs::rename(&temporary_path, &path).await {
        let _ = tokio::fs::remove_file(&temporary_path).await;
        return error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "storage_error",
            io_error.to_string(),
        );
    }
    let file = state
        .restream
        .create_file_metadata(
            name,
            mime_type,
            total_bytes,
            None,
            labels,
            path.to_string_lossy().into(),
        )
        .await;
    created(public_file(file))
}

pub(super) async fn create_storage_metadata(
    State(state): State<AppState>,
    Json(request): Json<CreateStorageMetadataRequest>,
) -> Response {
    let name = match required(&request.name, "name") {
        Ok(value) => value,
        Err(response) => return response,
    };
    if request.size_bytes.unwrap_or(0) > max_upload_bytes() {
        return error(
            StatusCode::PAYLOAD_TOO_LARGE,
            "upload_too_large",
            format!("file exceeds the {} byte upload limit", max_upload_bytes()),
        );
    }
    let path = request.path.unwrap_or_default();
    if !path.is_empty() && !state.restream.is_managed_storage_path(&path) {
        return bad_request("path must point to an existing file inside the storage root");
    }
    let file = state
        .restream
        .create_file_metadata(
            name,
            request
                .mime_type
                .unwrap_or_else(|| "application/octet-stream".into()),
            request.size_bytes.unwrap_or(0),
            request.duration_seconds,
            request.labels.unwrap_or_default(),
            path,
        )
        .await;
    created(public_file(file))
}

pub(super) async fn get_file(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    match state.restream.get_file(&id).await {
        Some(file) => ok(public_file(file)),
        None => not_found("storage file"),
    }
}

pub(super) async fn update_file(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<UpdateStorageRequest>,
) -> Response {
    match state
        .restream
        .update_file(&id, request.name, request.labels)
        .await
    {
        Some(file) => ok(public_file(file)),
        None => not_found("storage file"),
    }
}

pub(super) async fn delete_file(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    if state.restream.delete_file(&id).await.is_some() {
        ok(json!({"deleted": true, "id": id}))
    } else {
        not_found("storage file")
    }
}

pub(super) async fn download_file(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    let Some(file) = state.restream.get_file(&id).await else {
        return not_found("storage file");
    };
    if !state.restream.is_managed_storage_path(&file.path) {
        return error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "file_not_downloadable",
            "file contents are not available in the local storage root",
        );
    }
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(&file.mime_type)
            .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream")),
    );
    if let Ok(value) = HeaderValue::from_str(&format!(
        "attachment; filename=\"{}\"",
        safe_download_filename(&file.name)
    )) {
        headers.insert(header::CONTENT_DISPOSITION, value);
    }
    let path = file.path.clone();
    let stream = async_stream::stream! {
        let mut input = match tokio::fs::File::open(path).await {
            Ok(input) => input,
            Err(error) => {
                yield Err::<bytes::Bytes, std::io::Error>(error);
                return;
            }
        };
        let mut buffer = vec![0u8; 64 * 1024];
        loop {
            let read = match input.read(&mut buffer).await {
                Ok(read) => read,
                Err(error) => {
                    yield Err::<bytes::Bytes, std::io::Error>(error);
                    return;
                }
            };
            if read == 0 {
                break;
            }
            yield Ok::<bytes::Bytes, std::io::Error>(bytes::Bytes::copy_from_slice(&buffer[..read]));
        }
    };
    (StatusCode::OK, headers, Body::from_stream(stream)).into_response()
}

pub(super) async fn file_download_url(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    if state.restream.get_file(&id).await.is_none() {
        return not_found("storage file");
    }
    let download_url = format!("/api/v1/storage/files/{id}/download");
    ok(
        json!({"url": download_url, "downloadUrl": format!("/api/v1/storage/files/{id}/download"), "expiresIn": 3600}),
    )
}
