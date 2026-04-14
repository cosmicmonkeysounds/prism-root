//! HTTP transport — an axum router that exposes [`DaemonKernel::invoke`]
//! as a single REST endpoint:
//!
//! ```text
//! POST /invoke/<command>
//! Content-Type: application/json
//!
//! { …command payload… }
//! ```
//!
//! Success returns `200 OK` with the handler's JSON result.
//! `CommandError::NotFound` maps to `404`, `Handler` to `500`, all others
//! to `500` with the daemon's error string in the body.
//!
//! Two convenience routes round things out:
//!
//! ```text
//! GET /capabilities  -> ["build.run_step", "crdt.read", …]
//! GET /healthz       -> "ok"
//! ```
//!
//! ### Why blocking inside async
//!
//! The kernel is intentionally synchronous so it can be embedded behind
//! Tauri, FFI, stdio, and emscripten without dragging tokio along. axum
//! is async, so every handler hops onto [`tokio::task::spawn_blocking`]
//! before calling `kernel.invoke`. The kernel itself never sees a tokio
//! reactor — it just runs a closure on a blocking-pool thread.

use crate::kernel::DaemonKernel;
use crate::registry::CommandError;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde_json::{json, Value as JsonValue};
use std::sync::Arc;

/// State plugged into the axum router. The kernel is `Clone`, so multiple
/// adapters can share the same kernel — but we wrap it in `Arc` anyway so
/// the closure cost on each request is negligible.
#[derive(Clone)]
pub struct HttpState {
    pub kernel: Arc<DaemonKernel>,
}

/// Build the axum router that exposes the daemon kernel.
///
/// The returned router is a plain `axum::Router<()>` — already merged with
/// its state — so callers can `.merge` or `.nest` it under any prefix
/// before serving.
pub fn router(kernel: Arc<DaemonKernel>) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/capabilities", get(capabilities))
        .route("/invoke/:command", post(invoke))
        .route("/admin", get(admin_html))
        .route("/admin/api/snapshot", get(admin_snapshot))
        .with_state(HttpState { kernel })
}

async fn healthz() -> &'static str {
    "ok"
}

async fn capabilities(State(state): State<HttpState>) -> Json<Vec<String>> {
    Json(state.kernel.capabilities())
}

async fn invoke(
    State(state): State<HttpState>,
    Path(command): Path<String>,
    Json(payload): Json<JsonValue>,
) -> Response {
    let kernel = state.kernel.clone();
    let cmd = command.clone();

    // Hop the (potentially blocking) sync invoke onto the blocking pool
    // so we don't stall the async runtime. JoinError is treated as a 500.
    let result = tokio::task::spawn_blocking(move || kernel.invoke(&cmd, payload)).await;

    match result {
        Ok(Ok(value)) => (StatusCode::OK, Json(value)).into_response(),
        Ok(Err(err)) => command_error_to_response(&command, err),
        Err(join_err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "error": "transport_join_error",
                "command": command,
                "message": join_err.to_string(),
            })),
        )
            .into_response(),
    }
}

/// Admin dashboard — self-contained HTML page that polls `/admin/api/snapshot`.
async fn admin_html(State(state): State<HttpState>) -> Response {
    let kernel = state.kernel.clone();

    let result =
        tokio::task::spawn_blocking(move || kernel.invoke("daemon.admin", serde_json::json!({})))
            .await;

    // Build a minimal seed JSON for the HTML page
    let seed_json = match result {
        Ok(Ok(snap)) => {
            let wrapped = json!({
                "sourceId": "daemon",
                "sourceLabel": "Prism Daemon",
                "capturedAt": chrono_now_iso(),
                "health": snap["health"],
                "uptimeSeconds": snap["uptimeSeconds"],
                "metrics": snap["metrics"],
                "services": snap["services"],
                "activity": snap["activity"],
            });
            serde_json::to_string(&wrapped).unwrap_or_default()
        }
        _ => String::new(),
    };

    let html = admin_html_template(&seed_json);

    (
        StatusCode::OK,
        [("content-type", "text/html; charset=utf-8")],
        html,
    )
        .into_response()
}

/// Admin snapshot JSON — polled by the admin HTML page.
async fn admin_snapshot(State(state): State<HttpState>) -> Response {
    let kernel = state.kernel.clone();

    let result =
        tokio::task::spawn_blocking(move || kernel.invoke("daemon.admin", serde_json::json!({})))
            .await;

    match result {
        Ok(Ok(admin_data)) => {
            let snapshot = json!({
                "sourceId": "daemon",
                "sourceLabel": "Prism Daemon",
                "capturedAt": chrono_now_iso(),
                "health": admin_data["health"],
                "uptimeSeconds": admin_data["uptimeSeconds"],
                "metrics": admin_data["metrics"],
                "services": admin_data["services"],
                "activity": admin_data["activity"],
            });
            (StatusCode::OK, Json(snapshot)).into_response()
        }
        Ok(Err(err)) => command_error_to_response("daemon.admin", err),
        Err(join_err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "error": "transport_join_error",
                "message": join_err.to_string(),
            })),
        )
            .into_response(),
    }
}

/// Build the admin HTML page, injecting the seed snapshot JSON.
/// Uses string concatenation instead of format!() to avoid issues with
/// `#` in CSS hex colors conflicting with raw string delimiters.
fn admin_html_template(seed_json: &str) -> String {
    let seed = if seed_json.is_empty() {
        "null"
    } else {
        seed_json
    };
    let mut html = String::with_capacity(8192);
    html.push_str(include_str!("admin_template_head.html"));
    html.push_str(seed);
    html.push_str(include_str!("admin_template_tail.html"));
    html
}

/// Return the current time in ISO 8601 format (no chrono dependency).
fn chrono_now_iso() -> String {
    // Use the system time to produce an ISO timestamp.
    let now = std::time::SystemTime::now();
    let since_epoch = now
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = since_epoch.as_secs();
    // Simple UTC timestamp: YYYY-MM-DDTHH:MM:SSZ
    let days_since_epoch = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;
    // Compute year/month/day from days since epoch (1970-01-01)
    let (year, month, day) = days_to_ymd(days_since_epoch);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        year, month, day, hours, minutes, seconds
    )
}

/// Convert days since Unix epoch to (year, month, day).
fn days_to_ymd(days: u64) -> (u64, u64, u64) {
    // Civil calendar algorithm (adapted from Howard Hinnant)
    let z = days + 719468;
    let era = z / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

fn command_error_to_response(command: &str, err: CommandError) -> Response {
    let (code, kind) = match &err {
        CommandError::NotFound(_) => (StatusCode::NOT_FOUND, "not_found"),
        CommandError::AlreadyRegistered { .. } => (StatusCode::CONFLICT, "already_registered"),
        CommandError::Handler { .. } => (StatusCode::INTERNAL_SERVER_ERROR, "handler_error"),
        CommandError::LockPoisoned => (StatusCode::INTERNAL_SERVER_ERROR, "lock_poisoned"),
    };
    (
        code,
        Json(json!({
            "error": kind,
            "command": command,
            "message": err.to_string(),
        })),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builder::DaemonBuilder;
    use crate::module::DaemonModule;
    use axum::body::{to_bytes, Body};
    use axum::http::{Request, StatusCode};
    use serde_json::json;
    use tower::ServiceExt; // for `oneshot`

    /// A tiny module so the HTTP test suite doesn't have to depend on the
    /// full feature matrix being on. Just registers `echo.ping` and
    /// `echo.boom`.
    struct EchoModule;
    impl DaemonModule for EchoModule {
        fn id(&self) -> &str {
            "echo"
        }
        fn install(&self, builder: &mut crate::builder::DaemonBuilder) -> Result<(), CommandError> {
            builder
                .registry()
                .register("echo.ping", |payload| Ok(json!({ "echoed": payload })))?;
            builder.registry().register("echo.boom", |_| {
                Err(CommandError::handler("echo.boom", "kaboom"))
            })?;
            Ok(())
        }
    }

    fn build_kernel() -> Arc<DaemonKernel> {
        let kernel = DaemonBuilder::new()
            .with_module(EchoModule)
            .build()
            .unwrap();
        Arc::new(kernel)
    }

    async fn read_body(resp: Response) -> (StatusCode, JsonValue) {
        let status = resp.status();
        let bytes = to_bytes(resp.into_body(), 64 * 1024).await.unwrap();
        let value: JsonValue = if bytes.is_empty() {
            JsonValue::Null
        } else {
            serde_json::from_slice(&bytes).unwrap_or(JsonValue::Null)
        };
        (status, value)
    }

    #[tokio::test]
    async fn healthz_returns_ok_text() {
        let app = router(build_kernel());
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/healthz")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = to_bytes(resp.into_body(), 1024).await.unwrap();
        assert_eq!(&bytes[..], b"ok");
    }

    #[tokio::test]
    async fn capabilities_lists_registered_commands() {
        let app = router(build_kernel());
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/capabilities")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let (status, body) = read_body(resp).await;
        assert_eq!(status, StatusCode::OK);
        let names: Vec<String> = serde_json::from_value(body).unwrap();
        assert!(names.contains(&"echo.ping".to_string()));
        assert!(names.contains(&"echo.boom".to_string()));
    }

    #[tokio::test]
    async fn invoke_echo_roundtrips_json_payload() {
        let app = router(build_kernel());
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/invoke/echo.ping")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"hello":"world"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        let (status, body) = read_body(resp).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, json!({ "echoed": { "hello": "world" } }));
    }

    #[tokio::test]
    async fn invoke_unknown_command_returns_404() {
        let app = router(build_kernel());
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/invoke/nope.nada")
                    .header("content-type", "application/json")
                    .body(Body::from("null"))
                    .unwrap(),
            )
            .await
            .unwrap();
        let (status, body) = read_body(resp).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["error"], "not_found");
        assert_eq!(body["command"], "nope.nada");
    }

    #[tokio::test]
    async fn admin_html_returns_html_page() {
        let kernel = DaemonBuilder::new()
            .with_module(EchoModule)
            .with_module(crate::modules::admin_module::AdminModule)
            .build()
            .unwrap();
        let app = router(Arc::new(kernel));
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/admin")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let ct = resp
            .headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap();
        assert!(ct.contains("text/html"));
        let bytes = to_bytes(resp.into_body(), 256 * 1024).await.unwrap();
        let body = String::from_utf8_lossy(&bytes);
        assert!(body.contains("Prism Daemon Admin"));
        assert!(body.contains("admin-shell"));
    }

    #[tokio::test]
    async fn admin_snapshot_returns_json() {
        let kernel = DaemonBuilder::new()
            .with_module(EchoModule)
            .with_module(crate::modules::admin_module::AdminModule)
            .build()
            .unwrap();
        let app = router(Arc::new(kernel));
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/admin/api/snapshot")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let (status, body) = read_body(resp).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["sourceId"], "daemon");
        assert_eq!(body["sourceLabel"], "Prism Daemon");
        assert_eq!(body["health"]["level"], "ok");
        assert!(body["uptimeSeconds"].is_u64());
        assert!(body["metrics"].is_array());
        assert!(body["services"].is_array());
    }

    #[tokio::test]
    async fn invoke_handler_error_maps_to_500_with_message() {
        let app = router(build_kernel());
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/invoke/echo.boom")
                    .header("content-type", "application/json")
                    .body(Body::from("{}"))
                    .unwrap(),
            )
            .await
            .unwrap();
        let (status, body) = read_body(resp).await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(body["error"], "handler_error");
        assert_eq!(body["command"], "echo.boom");
        assert!(body["message"].as_str().unwrap().contains("kaboom"));
    }
}
