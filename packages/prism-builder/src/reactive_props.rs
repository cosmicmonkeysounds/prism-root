//! `reactive_props` — Phase 4 of the Dioxus-inspired reactive
//! overhaul (`docs/dev/dioxus-inspiration.md`).
//!
//! The plan: `Node::props` is still serializable JSON on disk, but
//! the in-memory representation is a per-key `IndexMap<String,
//! Signal<Value>>` materialised lazily on first read. Block
//! authors get typed accessors; the `Connection` system's new
//! `ActionKind::Bind` variant (declared alongside this module in
//! [`crate::signal::ActionKind`]) is compiled to an `Effect` on
//! document load.
//!
//! Today this module lands the materialisation primitive itself —
//! [`ReactiveProps`] — without yet replacing `Node::props`. The two
//! representations coexist: legacy code stays on the `Value` field;
//! reactive-aware code constructs a `ReactiveProps` from that
//! `Value`, reads through it (`signal(key)` for the per-key
//! reactive cell), and writes back to canonical JSON via
//! [`ReactiveProps::to_value`] when serialising.
//!
//! The follow-up is to make `Node::props` *be* a `ReactiveProps` —
//! that touches every consumer of `Node::props[..]` (the four
//! visible call sites in `luau_component`, `prefab`, `ui_resolver`,
//! `facet`) and is the right scope for a dedicated PR. The
//! primitive here is unblocking: `ActionKind::Bind` can be parsed,
//! dispatched, and tested against a `ReactiveProps` standing in
//! for a node's prop bag.

use std::cell::RefCell;
use std::rc::Rc;

use indexmap::IndexMap;
use prism_core::reactive::{Owner, Signal};
use serde_json::Value;

struct Inner {
    /// Source-of-truth JSON. Kept in sync with materialised
    /// signals — any write through [`ReactiveProps::set`] updates
    /// both. Stays at the `Value::Object` level (or `Value::Null`
    /// for the empty case) so [`to_value`] is a constant-time
    /// borrow.
    json: RefCell<Value>,
    /// Lazily-materialised signal cells. The first
    /// [`signal`](ReactiveProps::signal) call for a key reads from
    /// `json`, allocates a `Signal<Value>` in [`owner`], and
    /// inserts it here. Subsequent calls reuse the cached signal.
    signals: RefCell<IndexMap<String, Signal<Value>>>,
    /// Lifetime authority for the signal slots. Cloning a
    /// [`ReactiveProps`] shares this owner; dropping the last
    /// clone reclaims every materialised signal slot.
    #[allow(dead_code)]
    owner: Rc<Owner>,
}

/// A reactive prop bag. `Clone`-cheap (shares its `Inner` via an
/// `Rc`); all mutating methods take `&self` so handlers and effects
/// can share the same bag.
#[derive(Clone)]
pub struct ReactiveProps {
    inner: Rc<Inner>,
}

impl ReactiveProps {
    /// Build a fresh prop bag seeded by `json`. `json` must be a
    /// `Value::Object` or `Value::Null`; anything else gets coerced
    /// to an empty object (logically equivalent — no keys).
    pub fn new(json: Value) -> Self {
        let json = if matches!(json, Value::Object(_) | Value::Null) {
            json
        } else {
            Value::Object(Default::default())
        };
        Self {
            inner: Rc::new(Inner {
                json: RefCell::new(json),
                signals: RefCell::new(IndexMap::new()),
                owner: Rc::new(Owner::new()),
            }),
        }
    }

    /// Get-or-create the per-key signal. Lazily materialises from
    /// the underlying JSON store. The returned [`Signal`] is
    /// `Copy + 'static` and subscribes the current reactive context
    /// on `read` / `get`.
    pub fn signal(&self, key: &str) -> Signal<Value> {
        if let Some(existing) = self.inner.signals.borrow().get(key) {
            return *existing;
        }
        let initial = self.read_json_key(key);
        let signal = self.inner.owner.insert(initial);
        self.inner
            .signals
            .borrow_mut()
            .insert(key.to_string(), signal);
        signal
    }

    /// Read the current value (non-subscribing). Pulls from the
    /// signal if one was materialised, otherwise from the JSON.
    pub fn get(&self, key: &str) -> Value {
        if let Some(signal) = self.inner.signals.borrow().get(key) {
            return signal.snapshot();
        }
        self.read_json_key(key)
    }

    /// Write a value back into the bag. Updates the canonical JSON
    /// store unconditionally; if a signal was materialised for
    /// `key`, also writes the new value through it (firing reactive
    /// subscribers).
    pub fn set(&self, key: &str, value: Value) {
        self.write_json_key(key, value.clone());
        if let Some(signal) = self.inner.signals.borrow().get(key) {
            signal.set(value);
        }
    }

    /// Number of keys currently present in the bag.
    pub fn len(&self) -> usize {
        self.inner.json.borrow().as_object().map_or(0, |o| o.len())
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Snapshot the canonical JSON. Reflects the most recent writes
    /// through [`ReactiveProps::set`]. Use this when serialising the
    /// owning `Node` back to disk.
    pub fn to_value(&self) -> Value {
        self.inner.json.borrow().clone()
    }

    /// How many signals have been materialised. Exposed for tests
    /// and diagnostics.
    pub fn materialised_count(&self) -> usize {
        self.inner.signals.borrow().len()
    }

    /// Whether a specific key has had its signal materialised.
    pub fn is_materialised(&self, key: &str) -> bool {
        self.inner.signals.borrow().contains_key(key)
    }

    fn read_json_key(&self, key: &str) -> Value {
        self.inner
            .json
            .borrow()
            .as_object()
            .and_then(|o| o.get(key).cloned())
            .unwrap_or(Value::Null)
    }

    fn write_json_key(&self, key: &str, value: Value) {
        let mut json = self.inner.json.borrow_mut();
        match &mut *json {
            Value::Object(map) => {
                map.insert(key.to_string(), value);
            }
            other => {
                // Was Null (or coerced from a non-object initial);
                // promote to a single-key object.
                let mut map = serde_json::Map::new();
                map.insert(key.to_string(), value);
                *other = Value::Object(map);
            }
        }
    }
}

impl Default for ReactiveProps {
    fn default() -> Self {
        Self::new(Value::Object(Default::default()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prism_core::reactive::Effect;
    use std::cell::Cell;

    fn props(json: serde_json::Value) -> ReactiveProps {
        ReactiveProps::new(json)
    }

    #[test]
    fn new_with_object_preserves_keys() {
        let p = props(serde_json::json!({"a": 1, "b": "x"}));
        assert_eq!(p.len(), 2);
        assert_eq!(p.get("a"), serde_json::json!(1));
        assert_eq!(p.get("b"), serde_json::json!("x"));
    }

    #[test]
    fn new_with_null_starts_empty() {
        let p = ReactiveProps::new(Value::Null);
        assert!(p.is_empty());
        assert_eq!(p.get("missing"), Value::Null);
    }

    #[test]
    fn new_with_non_object_coerces_to_empty() {
        let p = ReactiveProps::new(serde_json::json!([1, 2, 3]));
        assert!(p.is_empty());
    }

    #[test]
    fn signal_is_lazy_and_idempotent() {
        let p = props(serde_json::json!({"title": "hello"}));
        assert_eq!(p.materialised_count(), 0);
        let s1 = p.signal("title");
        let s2 = p.signal("title");
        // Identity check on the underlying generational-box handle:
        // both calls must return the same Signal (the cached one).
        assert!(p.is_materialised("title"));
        assert_eq!(p.materialised_count(), 1);
        assert_eq!(s1.snapshot(), serde_json::json!("hello"));
        assert_eq!(s2.snapshot(), serde_json::json!("hello"));
    }

    #[test]
    fn signal_for_missing_key_is_null() {
        let p = props(serde_json::json!({}));
        let s = p.signal("missing");
        assert_eq!(s.snapshot(), Value::Null);
    }

    #[test]
    fn set_updates_canonical_json_and_signal() {
        let p = props(serde_json::json!({"title": "old"}));
        let s = p.signal("title");

        p.set("title", serde_json::json!("new"));
        assert_eq!(s.snapshot(), serde_json::json!("new"));
        assert_eq!(
            p.to_value(),
            serde_json::json!({"title": "new"}),
            "canonical json reflects write"
        );
    }

    #[test]
    fn set_on_unmaterialised_key_updates_json_only() {
        let p = props(serde_json::json!({"a": 1}));
        p.set("b", serde_json::json!(2));
        assert_eq!(p.materialised_count(), 0);
        assert_eq!(
            p.to_value(),
            serde_json::json!({"a": 1, "b": 2}),
            "json absorbs the write"
        );
        // First signal() now reads the new value.
        assert_eq!(p.signal("b").snapshot(), serde_json::json!(2));
    }

    #[test]
    fn set_after_signal_read_wakes_effect() {
        // The point of Phase 4: a `bind` in an authored connection
        // compiles to an `Effect` reading a prop signal, and a
        // later write to the prop wakes the effect.
        let p = props(serde_json::json!({"count": 0}));
        let sig = p.signal("count");

        let fires = Rc::new(Cell::new(0));
        let fc = Rc::clone(&fires);
        let _e = Effect::new(move || {
            sig.read(|_| {});
            fc.set(fc.get() + 1);
        });
        assert_eq!(fires.get(), 1, "initial run");

        p.set("count", serde_json::json!(1));
        assert_eq!(fires.get(), 2);

        p.set("count", serde_json::json!(2));
        assert_eq!(fires.get(), 3);
    }

    #[test]
    fn set_with_same_value_does_not_skip_notify_today() {
        // Signal::set is unconditional — re-setting to the same
        // value still notifies subscribers. Block authors that need
        // PartialEq gating must use `Memo` over the prop signal.
        // This test documents today's behaviour so a future change
        // to add gating here trips the test.
        let p = props(serde_json::json!({"x": 1}));
        let sig = p.signal("x");

        let fires = Rc::new(Cell::new(0));
        let fc = Rc::clone(&fires);
        let _e = Effect::new(move || {
            sig.read(|_| {});
            fc.set(fc.get() + 1);
        });
        assert_eq!(fires.get(), 1);
        p.set("x", serde_json::json!(1));
        assert_eq!(fires.get(), 2, "equal write still wakes effect");
    }

    #[test]
    fn clones_share_state() {
        let p = props(serde_json::json!({"a": 1}));
        let cloned = p.clone();
        p.set("a", serde_json::json!(99));
        assert_eq!(cloned.get("a"), serde_json::json!(99));
    }

    #[test]
    fn to_value_is_canonical_after_set_chain() {
        let p = props(serde_json::json!({"a": 1}));
        let _ = p.signal("a"); // materialise
        p.set("a", serde_json::json!(2));
        p.set("b", serde_json::json!("hello"));
        p.set("c", serde_json::json!([1, 2, 3]));
        let v = p.to_value();
        let obj = v.as_object().unwrap();
        assert_eq!(obj.len(), 3);
        assert_eq!(obj["a"], serde_json::json!(2));
        assert_eq!(obj["b"], serde_json::json!("hello"));
        assert_eq!(obj["c"], serde_json::json!([1, 2, 3]));
    }
}
