//! Body size limit middleware.

use axum::{extract::Request, http::StatusCode, middleware::Next, response::Response};

pub async fn body_limit_middleware(request: Request, next: Next) -> Result<Response, StatusCode> {
    if let Some(content_length) = request
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<usize>().ok())
    {
        // Default 1MB limit — the actual limit comes from config
        if content_length > 1_048_576 {
            return Err(StatusCode::PAYLOAD_TOO_LARGE);
        }
    }
    Ok(next.run(request).await)
}
