//! Shared application state passed into every axum handler.
//!
//! Holds the portal store, the component registry, and the design
//! tokens — everything a request handler needs to render a portal
//! to HTML. Wrapped in `Arc` by the caller so clones stay cheap and
//! the HTTP server can scale across tokio worker threads without
//! contention on a single owner.

use std::sync::Arc;

use prism_builder::{starter::register_builtins, BuilderDocument, ComponentRegistry, Node};
use prism_core::design_tokens::{DesignTokens, DEFAULT_TOKENS};
use serde_json::json;

use crate::portal::{Portal, PortalLevel, PortalMeta, PortalStore};
use crate::ssr_worker::SsrWorker;

/// Everything a relay route handler needs. Construct once at boot,
/// stuff into an `Arc`, hand to `axum::Router::with_state`.
pub struct AppState {
    pub portals: PortalStore,
    pub registry: ComponentRegistry,
    pub tokens: DesignTokens,
    /// **Phase 8** of `docs/dev/dioxus-inspiration.md`: the
    /// single-threaded SSR renderer worker. Lazy-installed per
    /// portal-id; handlers call `ssr.render("/portals/{id}").await`
    /// for cached HTML, falling back to direct
    /// `lower_semantic_html` + a one-shot insert on first miss.
    /// `Arc` so `Drop` doesn't fire just because a handler clone
    /// goes out of scope.
    pub ssr: Arc<SsrWorker>,
}

impl AppState {
    /// Fresh state with the built-in component catalog registered
    /// and an empty portal store. The caller is responsible for
    /// upserting portals before the server is useful.
    pub fn new() -> Self {
        let mut registry = ComponentRegistry::new();
        register_builtins(&mut registry).expect("builtin components must register");
        prism_builder::register_core_widgets(&mut registry).expect("core widgets must register");
        Self {
            portals: PortalStore::new(),
            registry,
            tokens: DEFAULT_TOKENS,
            ssr: Arc::new(SsrWorker::spawn()),
        }
    }

    /// Fresh state with two sample portals seeded — a public
    /// "welcome" L1 portal and a non-public "draft". Used by the
    /// dev bin and by route integration tests so we can exercise
    /// index / detail / sitemap paths without any wire protocol.
    pub fn with_sample_portals() -> Self {
        let state = Self::new();

        let welcome = Portal {
            id: "welcome".into(),
            meta: PortalMeta {
                title: "Welcome to Prism".into(),
                description: "The distributed visual OS.".into(),
                public: true,
                level: PortalLevel::L1,
            },
            document: BuilderDocument {
                root: Some(Node {
                    id: "root".into(),
                    component: "container".into(),
                    props: json!({}),
                    children: vec![
                        Node {
                            id: "h".into(),
                            component: "text".into(),
                            props: json!({ "body": "Welcome to Prism", "level": "h1" }),
                            children: vec![],
                            ..Default::default()
                        },
                        Node {
                            id: "t".into(),
                            component: "text".into(),
                            props: json!({
                                "body": "You're viewing a Sovereign Portal — a server-rendered snapshot of a Prism document."
                            }),
                            children: vec![],
                            ..Default::default()
                        },
                        Node {
                            id: "l".into(),
                            component: "text".into(),
                            props: json!({ "body": "See all portals", "href": "/portals" }),
                            children: vec![],
                            ..Default::default()
                        },
                    ],
                    ..Default::default()
                }),
                ..Default::default()
            },
        };

        let draft = Portal {
            id: "draft".into(),
            meta: PortalMeta {
                title: "Draft (private)".into(),
                description: String::new(),
                public: false,
                level: PortalLevel::L1,
            },
            document: BuilderDocument {
                root: Some(Node {
                    id: "root".into(),
                    component: "text".into(),
                    props: json!({ "body": "This portal is private", "level": "h2" }),
                    children: vec![],
                    ..Default::default()
                }),
                ..Default::default()
            },
        };

        state.portals.upsert(welcome);
        state.portals.upsert(draft);
        state
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_registers_builtins() {
        let state = AppState::new();
        let core_widget_count = prism_builder::collect_all_contributions().len();
        assert_eq!(state.registry.len(), 16 + core_widget_count);
        for id in [
            "text",
            "image",
            "container",
            "form",
            "input",
            "button",
            "card",
            "code",
            "divider",
            "spacer",
            "columns",
            "list",
            "table",
            "tabs",
            "accordion",
        ] {
            assert!(state.registry.get(id).is_some(), "missing builtin: {id}");
        }
    }

    #[test]
    fn sample_state_has_public_and_private_portals() {
        let state = AppState::with_sample_portals();
        assert_eq!(state.portals.len(), 2);
        assert_eq!(state.portals.list_public().len(), 1);
        assert_eq!(state.portals.list_public()[0].id, "welcome");
    }
}
