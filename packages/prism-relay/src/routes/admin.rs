//! Admin dashboard and snapshot.

use std::sync::Arc;

use axum::{extract::State, response::IntoResponse, Json};
use serde_json::json;

use crate::relay_state::FullRelayState;

pub async fn admin_dashboard(State(state): State<Arc<FullRelayState>>) -> impl IntoResponse {
    let html = format!(
        r#"<!doctype html><html><head><title>Prism Relay Admin</title></head>
<body>
<h1>Prism Relay Admin</h1>
<p>DID: {}</p>
<p>Modules: {}</p>
<p>Uptime: {}s</p>
<p>Requests: {}</p>
<script>
setInterval(async () => {{
  const r = await fetch('/admin/api/snapshot');
  const d = await r.json();
  document.querySelector('#metrics').textContent = JSON.stringify(d, null, 2);
}}, 5000);
</script>
<pre id="metrics"></pre>
</body></html>"#,
        state.relay_did,
        state.relay.modules().len(),
        state.metrics.uptime_seconds(),
        state
            .metrics
            .requests_total
            .load(std::sync::atomic::Ordering::Relaxed),
    );
    axum::response::Html(html)
}

pub async fn admin_snapshot(State(state): State<Arc<FullRelayState>>) -> impl IntoResponse {
    Json(json!({
        "health": {
            "running": true,
            "did": state.relay_did,
            "uptime": state.metrics.uptime_seconds(),
        },
        "metrics": {
            "requests": state.metrics.requests_total.load(std::sync::atomic::Ordering::Relaxed),
        },
        "modules": state.relay.modules(),
        "peers": state.federation().get_peers().len(),
    }))
}
