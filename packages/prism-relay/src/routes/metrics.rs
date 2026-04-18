//! Prometheus metrics endpoint.

use crate::relay_state::FullRelayState;
use axum::{
    extract::State,
    http::{header, HeaderValue},
    response::{IntoResponse, Response},
};
use std::sync::Arc;

pub async fn prometheus_metrics(State(state): State<Arc<FullRelayState>>) -> Response {
    let body = state.metrics.render_prometheus();
    let mut resp = body.into_response();
    resp.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    resp
}
