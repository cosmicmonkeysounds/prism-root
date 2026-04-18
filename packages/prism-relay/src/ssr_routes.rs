//! Axum router + HTTP handlers for `prism-relay`.
//!
//! Routes today:
//!
//! | Method | Path              | Purpose                                         |
//! |--------|-------------------|-------------------------------------------------|
//! | GET    | `/healthz`        | Liveness probe (`"ok"`)                         |
//! | GET    | `/`               | Landing page — lists all public portals         |
//! | GET    | `/portals`        | Alias for `/`                                   |
//! | GET    | `/portals/:id`    | Render one portal as semantic HTML              |
//! | GET    | `/sitemap.xml`    | `<urlset>` of every public portal               |
//! | GET    | `/robots.txt`     | `Allow: /portals/…` for every public portal     |
//!
//! Everything beyond this (portal submissions, WebSocket relay,
//! federation peers, hashcash proofs, ACME, escrow, …) is a
//! follow-on phase. The spine here is the same one every
//! follow-on plugs into: an `Arc<AppState>` passed by
//! `Router::with_state`, handlers that walk the portal store and
//! call [`prism_builder::render_document_html`] for the body.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use prism_builder::{render_document_html, Html, RenderError};

use crate::portal::Portal;
use crate::state::AppState;

/// Build the axum router with every route wired to its handler.
/// The caller owns `state` and decides how long-lived it is; axum
/// clones the `Arc` per request.
pub fn build_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/", get(index))
        .route("/portals", get(index))
        .route("/portals/:id", get(portal_detail))
        .route("/sitemap.xml", get(sitemap))
        .route("/robots.txt", get(robots))
        .with_state(state)
}

async fn healthz() -> &'static str {
    "ok"
}

/// Landing page. Lists every public portal as a navigable anchor
/// under an `<h1>`. Non-public portals never appear.
async fn index(State(state): State<Arc<AppState>>) -> Response {
    let portals = state.portals.list_public();
    let body = render_index_page(&portals);
    html_response(body)
}

/// Portal detail page. Walks the document tree through the SSR
/// pipeline and wraps the returned fragment in a full HTML
/// document with an OpenGraph-friendly `<head>`.
async fn portal_detail(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Response {
    let Some(portal) = state.portals.get(&id) else {
        return not_found();
    };
    if !portal.meta.public {
        // Non-public portals behave as if they don't exist to
        // unauthenticated visitors — the capability-token path
        // that unlocks them lands in a follow-on phase.
        return not_found();
    }

    let body = match render_document_html(&portal.document, &state.registry, &state.tokens) {
        Ok(b) => b,
        Err(err) => return render_error_response(err),
    };
    let page = wrap_portal_page(&portal, &body);
    html_response(page)
}

/// XML sitemap. One `<url>` entry per public portal, pointing at
/// `/portals/{id}` relative to the serving host — callers are
/// expected to rewrite the `<loc>` prefix via a reverse proxy.
async fn sitemap(State(state): State<Arc<AppState>>) -> Response {
    let portals = state.portals.list_public();
    let body = render_sitemap(&portals);
    xml_response(body)
}

/// Minimal `robots.txt` — allow the index and every public portal
/// detail path, disallow everything else by convention.
async fn robots(State(state): State<Arc<AppState>>) -> Response {
    let portals = state.portals.list_public();
    let body = render_robots(&portals);
    text_response(body)
}

// ── Page builders ───────────────────────────────────────────────

/// Build the landing page body — `<!doctype html><html>…</html>`.
pub fn render_index_page(portals: &[Portal]) -> String {
    let mut h = Html::with_capacity(512);
    h.doctype();
    h.open("html");
    render_head(&mut h, "Prism — Sovereign Portals", "");
    h.open("body");
    h.open("h1");
    h.text("Sovereign Portals");
    h.close("h1");
    if portals.is_empty() {
        h.open("p");
        h.text("No portals have been published yet.");
        h.close("p");
    } else {
        h.open("ul");
        for p in portals {
            h.open("li");
            h.open_attrs("a", &[("href", &format!("/portals/{}", p.id))]);
            h.text(&p.meta.title);
            h.close("a");
            h.close("li");
        }
        h.close("ul");
    }
    h.close("body");
    h.close("html");
    h.into_string()
}

/// Wrap a portal body fragment (already HTML) in a full page with
/// an OpenGraph-ready `<head>`. The fragment is inserted raw —
/// it's already been escaped by the component walker.
pub fn wrap_portal_page(portal: &Portal, body_html: &str) -> String {
    let mut h = Html::with_capacity(body_html.len() + 512);
    h.doctype();
    h.open("html");
    render_head(&mut h, &portal.meta.title, &portal.meta.description);
    h.open("body");
    h.raw(body_html);
    h.close("body");
    h.close("html");
    h.into_string()
}

/// Render the sitemap XML for a list of public portals.
pub fn render_sitemap(portals: &[Portal]) -> String {
    let mut out = String::with_capacity(256);
    out.push_str(r#"<?xml version="1.0" encoding="UTF-8"?>"#);
    out.push_str(r#"<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">"#);
    for p in portals {
        out.push_str("<url><loc>/portals/");
        // URL path segments are HTML-safe but let's run the same
        // escape to keep `&` correct if an id ever carries one.
        out.push_str(&prism_builder::escape_attr(&p.id));
        out.push_str("</loc></url>");
    }
    out.push_str("</urlset>");
    out
}

/// Render a minimal `robots.txt`.
pub fn render_robots(portals: &[Portal]) -> String {
    let mut out = String::with_capacity(64 + portals.len() * 24);
    out.push_str("User-agent: *\n");
    out.push_str("Allow: /\n");
    out.push_str("Allow: /portals\n");
    for p in portals {
        out.push_str("Allow: /portals/");
        out.push_str(&p.id);
        out.push('\n');
    }
    out.push_str("Sitemap: /sitemap.xml\n");
    out
}

fn render_head(h: &mut Html, title: &str, description: &str) {
    h.open("head");
    h.open_attrs("meta", &[("charset", "utf-8")]);
    h.open_attrs(
        "meta",
        &[
            ("name", "viewport"),
            ("content", "width=device-width, initial-scale=1"),
        ],
    );
    h.open("title");
    h.text(title);
    h.close("title");
    if !description.is_empty() {
        h.open_attrs("meta", &[("name", "description"), ("content", description)]);
        h.open_attrs(
            "meta",
            &[("property", "og:description"), ("content", description)],
        );
    }
    h.open_attrs("meta", &[("property", "og:title"), ("content", title)]);
    h.open_attrs("meta", &[("property", "og:type"), ("content", "article")]);
    h.close("head");
}

// ── Response helpers ────────────────────────────────────────────

fn html_response(body: String) -> Response {
    let mut resp = body.into_response();
    resp.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    resp
}

fn xml_response(body: String) -> Response {
    let mut resp = body.into_response();
    resp.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/xml; charset=utf-8"),
    );
    resp
}

fn text_response(body: String) -> Response {
    let mut resp = body.into_response();
    resp.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    resp
}

fn not_found() -> Response {
    let body = "<!doctype html><html><head><title>404 — Not Found</title></head><body><h1>Not Found</h1><p>No portal by that name.</p></body></html>";
    let mut resp = (StatusCode::NOT_FOUND, body).into_response();
    resp.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    resp
}

fn render_error_response(err: RenderError) -> Response {
    let body = format!(
        "<!doctype html><html><head><title>500 — Render Error</title></head><body><h1>Render Error</h1><pre>{}</pre></body></html>",
        prism_builder::escape_text(&err.to_string())
    );
    let mut resp = (StatusCode::INTERNAL_SERVER_ERROR, body).into_response();
    resp.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    resp
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::portal::{PortalLevel, PortalMeta};
    use prism_builder::{BuilderDocument, Node};
    use serde_json::json;

    fn portal(id: &str, title: &str, public: bool) -> Portal {
        Portal {
            id: id.to_string(),
            meta: PortalMeta {
                title: title.to_string(),
                description: "desc".into(),
                public,
                level: PortalLevel::L1,
            },
            document: BuilderDocument {
                root: Some(Node {
                    id: "root".into(),
                    component: "heading".into(),
                    props: json!({ "text": title }),
                    children: vec![],
                }),
                zones: Default::default(),
            },
        }
    }

    #[test]
    fn index_page_lists_public_portals() {
        let portals = vec![portal("alpha", "Alpha", true), portal("beta", "Beta", true)];
        let html = render_index_page(&portals);
        assert!(html.contains("<!doctype html>"));
        assert!(html.contains("<h1>Sovereign Portals</h1>"));
        assert!(html.contains(r#"<a href="/portals/alpha">Alpha</a>"#));
        assert!(html.contains(r#"<a href="/portals/beta">Beta</a>"#));
    }

    #[test]
    fn index_page_empty_shows_placeholder() {
        let html = render_index_page(&[]);
        assert!(html.contains("No portals have been published yet."));
    }

    #[test]
    fn portal_page_head_carries_title_and_description() {
        let p = portal("alpha", "Alpha Title", true);
        let page = wrap_portal_page(&p, "<h1>Body</h1>");
        assert!(page.contains("<title>Alpha Title</title>"));
        assert!(page.contains(r#"<meta name="description" content="desc">"#));
        assert!(page.contains(r#"<meta property="og:title" content="Alpha Title">"#));
        assert!(page.contains("<h1>Body</h1>"));
    }

    #[test]
    fn portal_page_title_is_escaped() {
        let mut p = portal("x", "<script>x</script>", true);
        p.meta.description = "".into();
        let page = wrap_portal_page(&p, "");
        assert!(page.contains("<title>&lt;script&gt;x&lt;/script&gt;</title>"));
        assert!(!page.contains("<script>x</script>"));
    }

    #[test]
    fn sitemap_only_lists_provided_portals() {
        let xml = render_sitemap(&[portal("a", "A", true), portal("b", "B", true)]);
        assert!(xml.starts_with(r#"<?xml version="1.0" encoding="UTF-8"?>"#));
        assert!(xml.contains("<loc>/portals/a</loc>"));
        assert!(xml.contains("<loc>/portals/b</loc>"));
    }

    #[test]
    fn robots_allows_each_public_portal() {
        let txt = render_robots(&[portal("alpha", "A", true)]);
        assert!(txt.contains("User-agent: *"));
        assert!(txt.contains("Allow: /portals/alpha"));
        assert!(txt.contains("Sitemap: /sitemap.xml"));
    }
}
