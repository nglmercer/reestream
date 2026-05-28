use axum::{extract::Path, http::StatusCode, response::IntoResponse};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "static"]
struct DashboardAssets;

pub async fn serve_index() -> impl IntoResponse {
    match DashboardAssets::get("index.html") {
        Some(content) => {
            let body = String::from_utf8_lossy(content.data.as_ref()).to_string();
            (
                StatusCode::OK,
                [("content-type", "text/html; charset=utf-8")],
                body,
            )
                .into_response()
        }
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

pub async fn serve_assets(Path(path): Path<String>) -> impl IntoResponse {
    let full_path = format!("assets/{path}");
    match DashboardAssets::get(&full_path) {
        Some(content) => {
            let mime = mime_guess(&full_path);
            let body = content.data.to_vec();
            (StatusCode::OK, [("content-type", mime)], body).into_response()
        }
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

pub async fn serve_favicon() -> impl IntoResponse {
    match DashboardAssets::get("favicon.svg") {
        Some(content) => {
            let body = content.data.to_vec();
            (StatusCode::OK, [("content-type", "image/svg+xml")], body).into_response()
        }
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

fn mime_guess(path: &str) -> &'static str {
    if path.ends_with(".js") || path.ends_with(".mjs") {
        "application/javascript"
    } else if path.ends_with(".css") {
        "text/css"
    } else if path.ends_with(".svg") {
        "image/svg+xml"
    } else if path.ends_with(".png") {
        "image/png"
    } else if path.ends_with(".jpg") || path.ends_with(".jpeg") {
        "image/jpeg"
    } else if path.ends_with(".woff2") {
        "font/woff2"
    } else if path.ends_with(".woff") {
        "font/woff"
    } else {
        "application/octet-stream"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mime_guess_js() {
        assert_eq!(mime_guess("app.js"), "application/javascript");
    }

    #[test]
    fn test_mime_guess_css() {
        assert_eq!(mime_guess("style.css"), "text/css");
    }

    #[test]
    fn test_mime_guess_svg() {
        assert_eq!(mime_guess("icon.svg"), "image/svg+xml");
    }

    #[test]
    fn test_mime_guess_unknown() {
        assert_eq!(mime_guess("file.xyz"), "application/octet-stream");
    }
}
