//! Integration tests that drive the real axum router end-to-end
//! via `tower::ServiceExt::oneshot`. No TCP socket is opened — the
//! router receives synthetic `http::Request`s and we assert on the
//! resulting `http::Response`s. This is the same pattern the axum
//! docs recommend for handler testing and matches what
//! `prism-daemon`'s `transport-http` feature tests exercise.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use prism_relay::{build_router, AppState};
use tower::ServiceExt;

async fn body_string(resp: axum::response::Response) -> String {
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8(body.to_vec()).unwrap()
}

fn app() -> axum::Router {
    build_router(Arc::new(AppState::with_sample_portals()))
}

#[tokio::test]
async fn healthz_returns_ok() {
    let resp = app()
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(body_string(resp).await, "ok");
}

#[tokio::test]
async fn index_shows_welcome_portal_link() {
    let resp = app()
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_string(resp).await;
    assert!(body.contains("<!doctype html>"));
    assert!(body.contains(r#"<a href="/portals/welcome">Welcome to Prism</a>"#));
    // The private draft portal must not appear.
    assert!(!body.contains("draft"));
}

#[tokio::test]
async fn portals_alias_matches_index() {
    let resp = app()
        .oneshot(
            Request::builder()
                .uri("/portals")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_string(resp).await;
    assert!(body.contains("Sovereign Portals"));
}

#[tokio::test]
async fn portal_detail_renders_welcome_page() {
    let resp = app()
        .oneshot(
            Request::builder()
                .uri("/portals/welcome")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let content_type = resp
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert!(content_type.starts_with("text/html"));
    let body = body_string(resp).await;
    assert!(body.contains("<title>Welcome to Prism</title>"));
    assert!(body.contains(r#"<meta property="og:title" content="Welcome to Prism">"#));
    // The body must have been walked through the component
    // registry — so the heading and container survive.
    assert!(body.contains("<h1>Welcome to Prism</h1>"));
    assert!(body.contains("<section>"));
    assert!(body.contains(r#"<a href="/portals">See all portals</a>"#));
}

#[tokio::test]
async fn private_portal_returns_404() {
    let resp = app()
        .oneshot(
            Request::builder()
                .uri("/portals/draft")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn unknown_portal_returns_404() {
    let resp = app()
        .oneshot(
            Request::builder()
                .uri("/portals/does-not-exist")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn sitemap_lists_public_portals() {
    let resp = app()
        .oneshot(
            Request::builder()
                .uri("/sitemap.xml")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let content_type = resp
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert!(content_type.starts_with("application/xml"));
    let body = body_string(resp).await;
    assert!(body.contains("<loc>/portals/welcome</loc>"));
    assert!(!body.contains("draft"));
}

#[tokio::test]
async fn robots_allows_welcome_and_points_at_sitemap() {
    let resp = app()
        .oneshot(
            Request::builder()
                .uri("/robots.txt")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_string(resp).await;
    assert!(body.contains("Allow: /portals/welcome"));
    assert!(body.contains("Sitemap: /sitemap.xml"));
    assert!(!body.contains("Allow: /portals/draft"));
}
