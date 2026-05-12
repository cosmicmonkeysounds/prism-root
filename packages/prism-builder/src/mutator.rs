//! `mutator` — Phase 4b of `docs/dev/dioxus-inspiration.md`.
//!
//! One declarative seam for every `Node::props` mutation. Replaces
//! the open-coded `Value::Object(ref mut map).insert(key, value)`
//! dance that lived in prefab materialisation, facet scalar
//! resolution, and facet variant rule evaluation. The builder takes
//! an optional [`DocumentBindings`] reference: when present, every
//! write reaches the matching per-NodeId reactive bag too, so block
//! `lower_ui` bodies subscribed via [`crate::ui_lower::LowerCtx::prop`]
//! re-fire automatically on the next dirty drain.
//!
//! ## Shape
//!
//! ```ignore
//! // Pure JSON write — same effect as the old `apply_prop_to_node`.
//! NodeMutator::new().write_at(&mut root, "card-title", "body", json!("Hi"));
//!
//! // Same write but also pokes the reactive bag, waking subscribers.
//! NodeMutator::with_bindings(&bindings)
//!     .write_at(&mut root, "card-title", "body", json!("Hi"));
//! ```
//!
//! Both shapes call into one private `set_object_key` so the
//! "promote Null/non-Object to an Object" branch lives exactly once.

use serde_json::Value;

use crate::document::Node;
use crate::reactive_props::DocumentBindings;

/// Single mutation seam for `Node::props`. The builder is `Copy` —
/// hold one across a hot loop without ceremony.
#[derive(Copy, Clone, Default)]
pub struct NodeMutator<'a> {
    bindings: Option<&'a DocumentBindings>,
}

impl<'a> NodeMutator<'a> {
    /// JSON-only mutator. Writes through to `Node::props` but does
    /// not notify reactive subscribers — appropriate for tests, the
    /// SSR path, and any flow that builds a fresh document tree.
    pub fn new() -> Self {
        Self { bindings: None }
    }

    /// Reactive mutator. Mirror every write into
    /// `bindings.props_for(node.id, ..).set(key, value)` so any
    /// subscribed `Effect` / `lower_ui` body wakes on the next dirty
    /// drain.
    pub fn with_bindings(bindings: &'a DocumentBindings) -> Self {
        Self {
            bindings: Some(bindings),
        }
    }

    /// Write `value` into `node.props[key]`. If `node.props` isn't
    /// an `Object` yet, promotes it to a single-key one.
    pub fn write(&self, node: &mut Node, key: &str, value: Value) {
        set_object_key(&mut node.props, key, value.clone());
        if let Some(b) = self.bindings {
            if !node.id.is_empty() {
                b.props_for(&node.id, &node.props).set(key, value);
            }
        }
    }

    /// Walk `root` looking for the first descendant (`root` included)
    /// with `id == target_id`; on hit, write `value` into its props.
    /// Returns `true` on hit, `false` when no matching node exists.
    pub fn write_at(&self, root: &mut Node, target_id: &str, key: &str, value: Value) -> bool {
        if root.id == target_id {
            self.write(root, key, value);
            return true;
        }
        for child in &mut root.children {
            if self.write_at(child, target_id, key, value.clone()) {
                return true;
            }
        }
        false
    }
}

/// Write `key = value` into a JSON `Object`-shaped slot. If the slot
/// is currently `Null` (default) or any non-`Object` shape, promotes
/// it to a single-key `Object` first. Internal — every mutation site
/// across `prism-builder` flows through this one branch.
fn set_object_key(slot: &mut Value, key: &str, value: Value) {
    if let Value::Object(map) = slot {
        map.insert(key.to_string(), value);
        return;
    }
    let mut map = serde_json::Map::new();
    map.insert(key.to_string(), value);
    *slot = Value::Object(map);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Node;
    use crate::reactive_props::DocumentBindings;
    use serde_json::json;

    fn node(id: &str, props: Value) -> Node {
        Node {
            id: id.into(),
            component: "container".into(),
            props,
            ..Default::default()
        }
    }

    #[test]
    fn write_updates_existing_object() {
        let m = NodeMutator::new();
        let mut n = node("a", json!({ "x": 1 }));
        m.write(&mut n, "x", json!(2));
        assert_eq!(n.props, json!({ "x": 2 }));
    }

    #[test]
    fn write_promotes_null_to_object() {
        let m = NodeMutator::new();
        let mut n = node("a", Value::Null);
        m.write(&mut n, "label", json!("Hi"));
        assert_eq!(n.props, json!({ "label": "Hi" }));
    }

    #[test]
    fn write_promotes_non_object_to_object() {
        let m = NodeMutator::new();
        let mut n = node("a", json!([1, 2, 3]));
        m.write(&mut n, "x", json!(1));
        assert_eq!(n.props, json!({ "x": 1 }));
    }

    #[test]
    fn write_at_finds_nested_target() {
        let m = NodeMutator::new();
        let mut root = node("root", json!({}));
        root.children.push(node("child", json!({ "body": "old" })));
        let hit = m.write_at(&mut root, "child", "body", json!("new"));
        assert!(hit);
        assert_eq!(root.children[0].props["body"], "new");
    }

    #[test]
    fn write_at_returns_false_on_miss() {
        let m = NodeMutator::new();
        let mut root = node("root", json!({}));
        let hit = m.write_at(&mut root, "nope", "x", json!(0));
        assert!(!hit);
    }

    #[test]
    fn write_with_bindings_pokes_reactive_bag() {
        // The whole point of Phase 4b: a write through this seam wakes
        // any subscriber attached to the matching reactive bag.
        let bindings = DocumentBindings::new();
        let bag = bindings.props_for("a", &json!({ "x": 1 }));
        let sig = bag.signal("x");

        let m = NodeMutator::with_bindings(&bindings);
        let mut n = node("a", json!({ "x": 1 }));
        m.write(&mut n, "x", json!(99));

        assert_eq!(n.props["x"], 99, "json mirrors the write");
        assert_eq!(sig.snapshot(), json!(99), "signal mirrors the write");
    }

    #[test]
    fn write_at_with_bindings_pokes_descendant_bag() {
        let bindings = DocumentBindings::new();
        let _ = bindings.props_for("child", &json!({}));
        let sig = bindings.props_for("child", &json!({})).signal("body");

        let m = NodeMutator::with_bindings(&bindings);
        let mut root = node("root", json!({}));
        root.children.push(node("child", json!({})));
        m.write_at(&mut root, "child", "body", json!("Hi"));

        assert_eq!(root.children[0].props["body"], "Hi");
        assert_eq!(sig.snapshot(), json!("Hi"));
    }
}
