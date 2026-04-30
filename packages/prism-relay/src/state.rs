//! Shared application state passed into every axum handler.
//!
//! Holds the portal store, the component registry, and the design
//! tokens — everything a request handler needs to render a portal
//! to HTML. Wrapped in `Arc` by the caller so clones stay cheap and
//! the HTTP server can scale across tokio worker threads without
//! contention on a single owner.

use prism_builder::{
    html_starter::register_html_builtins, starter::register_builtins, BuilderDocument,
    ComponentRegistry, HtmlRegistry, Node,
};
use prism_core::design_tokens::{DesignTokens, DEFAULT_TOKENS};
use serde_json::json;

use crate::portal::{Portal, PortalLevel, PortalMeta, PortalStore};

/// Everything a relay route handler needs. Construct once at boot,
/// stuff into an `Arc`, hand to `axum::Router::with_state`.
pub struct AppState {
    pub portals: PortalStore,
    pub registry: ComponentRegistry,
    pub html_registry: HtmlRegistry,
    pub tokens: DesignTokens,
}

impl AppState {
    /// Fresh state with the built-in component catalog registered
    /// and an empty portal store. The caller is responsible for
    /// upserting portals before the server is useful.
    pub fn new() -> Self {
        let mut registry = ComponentRegistry::new();
        register_builtins(&mut registry).expect("builtin components must register");
        let mut html_registry = HtmlRegistry::new();
        register_html_builtins(&mut html_registry).expect("html builtins must register");
        Self {
            portals: PortalStore::new(),
            registry,
            html_registry,
            tokens: DEFAULT_TOKENS,
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
        assert_eq!(state.registry.len(), 16);
        assert_eq!(state.html_registry.len(), 16);
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
            assert!(
                state.html_registry.get(id).is_some(),
                "missing html builtin: {id}"
            );
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
