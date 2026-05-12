//! `ssr_cache` — Phase 8 of the Dioxus-inspired reactive overhaul
//! (`docs/dev/dioxus-inspiration.md`).
//!
//! The plan: each cacheable HTML fragment is an `Effect` owned by a
//! per-route `Owner`. The effect's body produces the fragment's
//! HTML. When subscribed signals change, the effect re-fires and
//! stores fresh HTML; the next request returns the cached output
//! without re-walking the document.
//!
//! ## What's here
//!
//! * [`CachedFragment`] — one Effect-backed HTML cache. The
//!   constructor takes an `FnMut() -> String` that performs the
//!   render *and* reads reactive signals along the way; the effect
//!   subscribes to whatever the body touches, and every subsequent
//!   write to one of those signals invalidates the cache and the
//!   effect re-fires.
//! * [`SsrCache`] — `IndexMap<route, CachedFragment>` plus an
//!   `Owner` so the per-route effects share their lifetime with
//!   the cache.
//!
//! ## What's not here (yet)
//!
//! Reactive primitives use `UnsyncStorage` and aren't `Send` —
//! axum's multi-threaded executor can't share a `CachedFragment`
//! across worker threads as-is. The pragmatic integration is a
//! single-threaded "SSR renderer worker" that drains a queue of
//! requests, executes the effect-backed renders on its own thread,
//! and ships the rendered HTML back. That landing is a
//! `prism-relay` follow-up; this module provides the cache
//! primitive itself, fully tested against the reactive substrate.
//!
//! `FederatedSignal<T>` / `RelaySignal<T>` reads inside a cached
//! fragment's body give cross-fleet cache invalidation for free —
//! the same federation bus that updates the underlying data
//! signal also re-fires every relay's local cache. That's the
//! "scope ladder payoff" §6 Phase 8 of the plan describes.

use std::cell::RefCell;
use std::rc::Rc;

use indexmap::IndexMap;
use prism_core::reactive::{Effect, Owner};

/// One Effect-backed HTML cache. Cloning is *not* supported — the
/// fragment owns an `Effect` whose lifetime is the cache entry's.
pub struct CachedFragment {
    /// Live storage the effect writes to. Reads see the most
    /// recent computation; `RefCell` rather than `Cell` so callers
    /// can borrow the `String` without cloning.
    body: Rc<RefCell<String>>,
    /// Generation counter — bumped each time the effect fires.
    /// Surfaced for diagnostics + a hook for the worker queue's
    /// "did this change since I last looked?" check.
    gen: Rc<std::cell::Cell<u64>>,
    /// The effect. Dropping the fragment disposes it.
    #[allow(dead_code)]
    effect: Effect,
}

impl CachedFragment {
    /// Build a fragment whose body re-runs whenever any reactive
    /// signal it reads changes. The body executes immediately to
    /// capture the initial dependency set + the initial HTML.
    pub fn new<F>(mut compute: F) -> Self
    where
        F: FnMut() -> String + 'static,
    {
        let body = Rc::new(RefCell::new(String::new()));
        let gen = Rc::new(std::cell::Cell::new(0_u64));
        let body_for = Rc::clone(&body);
        let gen_for = Rc::clone(&gen);
        let effect = Effect::new(move || {
            *body_for.borrow_mut() = compute();
            gen_for.set(gen_for.get() + 1);
        });
        Self { body, gen, effect }
    }

    /// Snapshot the most recent rendered HTML.
    pub fn render(&self) -> String {
        self.body.borrow().clone()
    }

    /// Borrow the cached HTML in place. Use to avoid the clone in
    /// `render` when the caller copies straight into a response
    /// body.
    pub fn body(&self) -> std::cell::Ref<'_, String> {
        self.body.borrow()
    }

    /// Generation counter — incremented on every re-fire. Useful
    /// for staleness checks in a worker queue: if `last_seen` ==
    /// `generation()`, the fragment hasn't changed since.
    pub fn generation(&self) -> u64 {
        self.gen.get()
    }
}

/// A keyed map of [`CachedFragment`]s. The `Owner` is the lifetime
/// authority; dropping the `SsrCache` drops every fragment's
/// effect.
pub struct SsrCache {
    fragments: IndexMap<String, CachedFragment>,
    /// Owns nothing today (the effects are stored inside each
    /// `CachedFragment`), but kept as the natural extension point
    /// for `Owner::insert_effect`-style per-cache effects (a
    /// "warm-on-mount" hook, an invalidation-counter effect, etc.).
    /// Phase 3-style scope wiring.
    #[allow(dead_code)]
    owner: Owner,
}

impl SsrCache {
    pub fn new() -> Self {
        Self {
            fragments: IndexMap::new(),
            owner: Owner::new(),
        }
    }

    /// Insert (or replace) a fragment keyed by `route`. Replacing
    /// disposes the previous fragment's effect.
    pub fn insert<F>(&mut self, route: impl Into<String>, compute: F)
    where
        F: FnMut() -> String + 'static,
    {
        self.fragments
            .insert(route.into(), CachedFragment::new(compute));
    }

    /// Drop a fragment by route id. Returns `true` if a fragment
    /// existed.
    pub fn remove(&mut self, route: &str) -> bool {
        self.fragments.shift_remove(route).is_some()
    }

    pub fn contains(&self, route: &str) -> bool {
        self.fragments.contains_key(route)
    }

    pub fn len(&self) -> usize {
        self.fragments.len()
    }

    pub fn is_empty(&self) -> bool {
        self.fragments.is_empty()
    }

    /// Snapshot a route's rendered HTML, or `None` if no fragment
    /// is registered.
    pub fn render(&self, route: &str) -> Option<String> {
        self.fragments.get(route).map(CachedFragment::render)
    }

    /// Generation counter for the named route. Returns `None` if
    /// the route isn't registered. Bumped every time the fragment's
    /// effect re-fires (i.e., every reactive invalidation).
    pub fn generation(&self, route: &str) -> Option<u64> {
        self.fragments.get(route).map(CachedFragment::generation)
    }
}

impl Default for SsrCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prism_core::reactive::Owner as CoreOwner;

    #[test]
    fn cached_fragment_renders_initial_body() {
        let f = CachedFragment::new(|| String::from("<p>hello</p>"));
        assert_eq!(f.render(), "<p>hello</p>");
        assert_eq!(f.generation(), 1, "fires once on construction");
    }

    #[test]
    fn cached_fragment_re_renders_when_signal_changes() {
        // The cache's whole point: signal writes invalidate the
        // rendered HTML, so a subsequent `render()` returns the new
        // body without re-walking the source document.
        let owner = CoreOwner::new();
        let title = owner.insert(String::from("Alpha"));
        let f = CachedFragment::new(move || {
            let t = title.get();
            format!("<h1>{t}</h1>")
        });
        assert_eq!(f.render(), "<h1>Alpha</h1>");
        assert_eq!(f.generation(), 1);

        title.set("Beta".into());
        assert_eq!(f.render(), "<h1>Beta</h1>");
        assert_eq!(f.generation(), 2);

        title.set("Gamma".into());
        assert_eq!(f.render(), "<h1>Gamma</h1>");
        assert_eq!(f.generation(), 3);
    }

    #[test]
    fn cached_fragment_does_not_re_render_on_unrelated_signal() {
        let owner = CoreOwner::new();
        let title = owner.insert(String::from("Alpha"));
        let unrelated = owner.insert(0_i32);
        let f = CachedFragment::new(move || {
            let t = title.get();
            format!("<h1>{t}</h1>")
        });
        let gen0 = f.generation();
        unrelated.set(99);
        assert_eq!(f.generation(), gen0, "unrelated signal did not invalidate");
    }

    #[test]
    fn body_borrow_avoids_clone() {
        let f = CachedFragment::new(|| "<div></div>".into());
        let borrowed = f.body();
        assert_eq!(&*borrowed, "<div></div>");
    }

    #[test]
    fn ssr_cache_insert_and_render_roundtrip() {
        let mut cache = SsrCache::new();
        cache.insert("/", || "<html><body>root</body></html>".into());
        cache.insert("/about", || "<p>about</p>".into());
        assert!(cache.contains("/"));
        assert_eq!(cache.len(), 2);
        assert_eq!(
            cache.render("/").as_deref(),
            Some("<html><body>root</body></html>")
        );
        assert_eq!(cache.render("/about").as_deref(), Some("<p>about</p>"));
        assert!(cache.render("/missing").is_none());
    }

    #[test]
    fn ssr_cache_per_route_invalidation_is_independent() {
        let owner = CoreOwner::new();
        let root_text = owner.insert(String::from("root-1"));
        let about_text = owner.insert(String::from("about-1"));

        let mut cache = SsrCache::new();
        cache.insert("/", move || {
            let t = root_text.get();
            format!("<body>{t}</body>")
        });
        cache.insert("/about", move || {
            let t = about_text.get();
            format!("<p>{t}</p>")
        });

        let root_gen0 = cache.generation("/").unwrap();
        let about_gen0 = cache.generation("/about").unwrap();

        // Mutating root_text invalidates only the root fragment.
        root_text.set("root-2".into());
        assert!(cache.generation("/").unwrap() > root_gen0);
        assert_eq!(
            cache.generation("/about"),
            Some(about_gen0),
            "/about wasn't subscribed to root_text"
        );
        assert_eq!(cache.render("/").as_deref(), Some("<body>root-2</body>"));

        // Mutating about_text invalidates only /about.
        let root_gen1 = cache.generation("/").unwrap();
        about_text.set("about-2".into());
        assert_eq!(cache.generation("/"), Some(root_gen1));
        assert_eq!(cache.render("/about").as_deref(), Some("<p>about-2</p>"));
    }

    #[test]
    fn ssr_cache_insert_replaces_existing_fragment() {
        let mut cache = SsrCache::new();
        cache.insert("/", || "v1".into());
        assert_eq!(cache.render("/").as_deref(), Some("v1"));
        cache.insert("/", || "v2".into());
        assert_eq!(cache.render("/").as_deref(), Some("v2"));
        assert_eq!(cache.len(), 1, "replace, not add");
    }

    #[test]
    fn ssr_cache_remove_disposes_fragment() {
        let mut cache = SsrCache::new();
        cache.insert("/", || "x".into());
        assert!(cache.remove("/"));
        assert!(!cache.contains("/"));
        assert!(!cache.remove("/"), "remove of missing returns false");
    }

    #[test]
    fn cache_drop_disposes_effects_no_panic() {
        // After the cache drops, mutating a foreign signal that the
        // fragment was subscribed to must not panic (the effect's
        // context has been disposed).
        let foreign_owner = CoreOwner::new();
        let sig = foreign_owner.insert(0_i32);
        {
            let mut cache = SsrCache::new();
            let sig_for = sig;
            cache.insert("/", move || {
                let v = sig_for.get();
                format!("{v}")
            });
            sig.set(1);
            assert_eq!(cache.render("/").as_deref(), Some("1"));
        } // cache drops — every fragment's effect disposed
        sig.set(2);
        sig.set(3);
    }
}
