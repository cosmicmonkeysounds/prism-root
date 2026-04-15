//! Portal data model.
//!
//! A [`Portal`] is the unit `prism-relay` serves: a published
//! document tree (via [`prism_builder::BuilderDocument`]) plus the
//! public metadata search engines and social previews read —
//! OpenGraph title/description, JSON-LD schema type, visibility
//! flag, created/updated timestamps. The legacy TypeScript relay
//! kept this in a JSON-file store saved every 5s; this module
//! gives us the same in-memory shape behind a lock so route
//! handlers can move without waiting on a persistence backend to
//! land.
//!
//! The store is intentionally simple — a `HashMap<PortalId,
//! Portal>` wrapped in an `RwLock`. Persistence, incremental CRDT
//! sync, and the federation peer graph are all follow-on phases;
//! the [`PortalStore`] trait boundary lives here so those phases
//! can swap the backend without touching any route handler.

use std::collections::HashMap;
use std::sync::RwLock;

use prism_builder::BuilderDocument;
use serde::{Deserialize, Serialize};

/// Stable identifier for a portal. The legacy relay used URL-safe
/// base32; we keep plain strings for now and tighten when the
/// persistence layer lands.
pub type PortalId = String;

/// Level of interactivity the portal opts into. Matches the legacy
/// relay's 4-level taxonomy so existing content maps cleanly when
/// we wire persistence:
///
/// * **L1** — static read-only snapshot. Pure SSR, no JS.
/// * **L2** — live incremental updates over WebSocket (future).
/// * **L3** — interactive forms + ephemeral DID auth (future).
/// * **L4** — full client-side hydration / bidirectional CRDT sync (future).
///
/// Today only L1 is served; the variants exist so the portal schema
/// is forward-compatible with the legacy on-disk format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PortalLevel {
    L1,
    L2,
    L3,
    L4,
}

/// Metadata the portal emits into `<head>` (title, description,
/// OpenGraph tags) and into the sitemap. Kept separate from the
/// document tree so cheap list endpoints (`GET /portals`, the
/// sitemap) don't have to walk every descendant.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortalMeta {
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_true")]
    pub public: bool,
    pub level: PortalLevel,
}

fn default_true() -> bool {
    true
}

/// A portal is metadata + a builder document. The document is what
/// the SSR walker turns into the body HTML; the metadata is what
/// goes into `<title>`, OpenGraph meta, and the sitemap entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Portal {
    pub id: PortalId,
    pub meta: PortalMeta,
    pub document: BuilderDocument,
}

/// In-memory portal store. Concurrent reads are cheap; writes take
/// a short exclusive lock. Persistence lives in a follow-on module
/// so route handlers don't care which backend is behind the trait.
#[derive(Debug, Default)]
pub struct PortalStore {
    inner: RwLock<HashMap<PortalId, Portal>>,
}

impl PortalStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert or replace a portal. Returns the previous entry if one
    /// existed so callers can diff on update.
    pub fn upsert(&self, portal: Portal) -> Option<Portal> {
        let mut guard = self.inner.write().expect("portal store lock poisoned");
        guard.insert(portal.id.clone(), portal)
    }

    /// Fetch a single portal by id. Returns a clone so the lock can
    /// be released before the caller does any rendering work.
    pub fn get(&self, id: &str) -> Option<Portal> {
        let guard = self.inner.read().expect("portal store lock poisoned");
        guard.get(id).cloned()
    }

    /// Every portal in the store, in arbitrary order. Used by the
    /// index route and the sitemap builder.
    pub fn list(&self) -> Vec<Portal> {
        let guard = self.inner.read().expect("portal store lock poisoned");
        guard.values().cloned().collect()
    }

    /// Portals flagged `public: true`, sorted by id for stable
    /// output. The sitemap + index route exclude non-public portals
    /// from search engines and the landing page.
    pub fn list_public(&self) -> Vec<Portal> {
        let mut out: Vec<Portal> = self.list().into_iter().filter(|p| p.meta.public).collect();
        out.sort_by(|a, b| a.id.cmp(&b.id));
        out
    }

    pub fn len(&self) -> usize {
        self.inner.read().expect("portal store lock poisoned").len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prism_builder::{BuilderDocument, Node};
    use serde_json::json;

    fn portal(id: &str, title: &str, public: bool) -> Portal {
        Portal {
            id: id.to_string(),
            meta: PortalMeta {
                title: title.to_string(),
                description: String::new(),
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
    fn upsert_replaces_existing() {
        let store = PortalStore::new();
        assert!(store.upsert(portal("a", "First", true)).is_none());
        let prev = store
            .upsert(portal("a", "Second", true))
            .expect("upsert should return previous");
        assert_eq!(prev.meta.title, "First");
        assert_eq!(store.get("a").unwrap().meta.title, "Second");
    }

    #[test]
    fn list_public_excludes_private() {
        let store = PortalStore::new();
        store.upsert(portal("a", "A", true));
        store.upsert(portal("b", "B", false));
        store.upsert(portal("c", "C", true));
        let public = store.list_public();
        assert_eq!(public.len(), 2);
        assert_eq!(public[0].id, "a");
        assert_eq!(public[1].id, "c");
    }

    #[test]
    fn list_public_sorted_by_id() {
        let store = PortalStore::new();
        store.upsert(portal("z", "Z", true));
        store.upsert(portal("m", "M", true));
        store.upsert(portal("a", "A", true));
        let public = store.list_public();
        let ids: Vec<_> = public.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids, vec!["a", "m", "z"]);
    }

    #[test]
    fn missing_portal_returns_none() {
        let store = PortalStore::new();
        assert!(store.get("nope").is_none());
    }
}
