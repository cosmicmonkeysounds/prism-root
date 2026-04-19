//! CSRF protection — requires `X-Prism-CSRF: 1` on mutating requests.

use axum::{
    extract::Request,
    http::{Method, StatusCode},
    middleware::Next,
    response::Response,
};

pub async fn csrf_middleware(request: Request, next: Next) -> Result<Response, StatusCode> {
    let needs_csrf = matches!(
        *request.method(),
        Method::POST | Method::PUT | Method::DELETE
    ) && {
        let path = request.uri().path();
        path.starts_with("/api/")
            && !path.starts_with("/api/acme-challenge")
            && path != "/metrics"
            && !path.starts_with("/admin")
    };

    if needs_csrf {
        let has_header = request
            .headers()
            .get("x-prism-csrf")
            .is_some_and(|v| v == "1");
        if !has_header {
            return Err(StatusCode::FORBIDDEN);
        }
    }

    Ok(next.run(request).await)
}

#[cfg(test)]
mod tests {
    #[test]
    fn csrf_required_paths() {
        let needs = |path: &str| -> bool {
            path.starts_with("/api/")
                && !path.starts_with("/api/acme-challenge")
                && path != "/metrics"
                && !path.starts_with("/admin")
        };

        assert!(needs("/api/portals"));
        assert!(needs("/api/collections"));
        assert!(!needs("/api/acme-challenge/tok"));
        assert!(!needs("/admin/api/snapshot"));
        assert!(!needs("/portals/welcome"));
    }
}
