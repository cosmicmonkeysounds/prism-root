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
//! This module lands two cooperating pieces:
//!
//! 1. [`ReactiveProps`] — the per-Node reactive prop bag (lazy-
//!    materialised `Signal<Value>` per key + canonical JSON store).
//! 2. [`DocumentBindings`] — the per-Document binding context that
//!    `BuilderDocument::install_bindings` returns. Holds a
//!    `HashMap<NodeId, ReactiveProps>` (one bag per referenced node)
//!    and a `Vec<Effect>` of installed `ActionKind::Bind` effects.
//!    Dropping the `DocumentBindings` disposes every installed
//!    effect.
//!
//! The two coexist with the canonical `Node::props: Value` —
//! `DocumentBindings::props_for(node_id)` lazily mirrors a node's
//! props into a `ReactiveProps` keyed by NodeId, and a `Bind`
//! connection's effect reads from one bag and writes to another.
//! Phase 4b (`LowerCtx::with_bindings` + `NodeMutator`) wires these
//! bags into the render walk + the prop-write seam; the on-disk
//! `Node::props: Value` shape is unchanged.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use indexmap::IndexMap;
use prism_core::reactive::{Owner, Signal};
use serde_json::Value;

use crate::document::{BuilderDocument, Node, NodeId};
use crate::signal::{ActionKind, Connection};

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

    /// Every key currently present in the canonical JSON store —
    /// every prop a writer has `set`. Used by the skeleton read
    /// consumer to project the whole bag back into the tree without
    /// re-walking the bind list. Non-subscribing (the caller takes a
    /// per-key subscribing read via [`signal`](Self::signal) for the
    /// ones it actually projects).
    pub fn keys(&self) -> Vec<String> {
        self.inner
            .json
            .borrow()
            .as_object()
            .map(|o| o.keys().cloned().collect())
            .unwrap_or_default()
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

// Wave 8.1 — `node:props():read("k") / :write("k", v) / :signal("k")`
// mirror of `ReactiveProps::{get, set, signal}` on the Rust side. The
// surface matches one method per side so a Luau-authored component or
// modifier reaches for props the same way Rust does. `:signal(k)`
// returns the `Signal<Value>` userdata from
// `prism_core::luau_reactive`, so the full reactive vocabulary
// (`read` / `peek` / `track` / `write` / `set`) is available through
// the existing impl.
#[cfg(feature = "luau")]
impl mlua::UserData for ReactiveProps {
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("read", |lua, this, key: String| {
            let v = this.get(&key);
            mlua::LuaSerdeExt::to_value(lua, &v)
        });
        methods.add_method("write", |lua, this, (key, value): (String, mlua::Value)| {
            let v: Value = mlua::LuaSerdeExt::from_value(lua, value)?;
            this.set(&key, v);
            Ok(())
        });
        methods.add_method("signal", |_, this, key: String| Ok(this.signal(&key)));
    }
}

/// Per-document binding context — the runtime side of an authored
/// `ActionKind::Bind` connection. Owns:
///
/// 1. A per-NodeId [`ReactiveProps`] cache. The first
///    [`DocumentBindings::props_for`] call for a NodeId snapshots
///    that node's `Value` props into a fresh `ReactiveProps`; later
///    calls return the cached one. Writers (the Bind effects) and
///    readers (block lower bodies, eventually) share the same bag.
/// 2. A `Vec<Effect>` of installed `Bind` effects. Effects subscribe
///    the source node's prop signal and write the resolved value
///    into the target node's prop signal. Dropping the
///    `DocumentBindings` disposes every effect.
///
/// The source grammar accepted today is `"<source_node_id>.<source_key>"`
/// — a literal NodeId + dot + prop key. `$<keyword>` selector
/// references (`$selection.name`) parse into [`SourceRef::Selection`]
/// but aren't resolved at install time yet; the effect is registered
/// and the source is recorded in [`DocumentBindings::unresolved`] for
/// the host to walk later. Plain literals (`"hello"`) parse as
/// [`SourceRef::Literal`] — the effect fires once and writes the
/// literal, no subscription.
#[derive(Clone)]
pub struct DocumentBindings {
    /// Reactive owner backing every materialised prop signal *and*
    /// every installed Bind effect — all reactive state owned by
    /// this document binding context. Dropping the owner disposes
    /// every retained effect (which in turn deregisters from
    /// upstream signal subscribers).
    owner: Rc<Owner>,
    /// Per-NodeId materialised reactive prop bag.
    props: Rc<RefCell<HashMap<NodeId, ReactiveProps>>>,
    /// Count of installed Bind effects. The effects themselves live
    /// on `owner.retained` — incrementing this counter tracks how
    /// many are pinned.
    effect_count: usize,
    /// Bind connections whose `source` couldn't be resolved at
    /// install time (selector references, malformed grammar, missing
    /// source nodes). The host can iterate these later to wire up
    /// alternative resolution paths.
    unresolved: Vec<UnresolvedBind>,
}

impl std::fmt::Debug for DocumentBindings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DocumentBindings")
            .field("effect_count", &self.effect_count)
            .field("cached_nodes", &self.props.borrow().len())
            .field("unresolved", &self.unresolved.len())
            .finish()
    }
}

/// A `Bind` connection whose `source` didn't resolve at install
/// time. Captured for host follow-up so the connection isn't
/// silently dropped.
#[derive(Debug, Clone)]
pub struct UnresolvedBind {
    pub connection_id: String,
    pub target_node: NodeId,
    pub target_key: String,
    pub source: String,
    pub reason: BindResolveError,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BindResolveError {
    /// `source` grammar didn't match any supported shape.
    GrammarUnrecognised,
    /// `source` referenced a NodeId that isn't in the document.
    SourceNodeMissing,
    /// `source` references a host-resolved selector
    /// (`$selection.name`) — the host wires these up separately.
    SelectorReference,
}

/// Parsed source expression. Today we recognise three shapes:
/// - `node_id.key` — read a node prop signal
/// - `$selector.key` — host-resolved selector (deferred)
/// - any other literal — a constant string (no subscription)
#[derive(Debug, Clone, PartialEq, Eq)]
enum SourceRef {
    NodeProp { node: NodeId, key: String },
    Selector { selector: String, key: String },
    Literal(String),
}

fn parse_source(raw: &str) -> SourceRef {
    let trimmed = raw.trim();
    if let Some((head, key)) = trimmed.split_once('.') {
        if let Some(selector) = head.strip_prefix('$') {
            return SourceRef::Selector {
                selector: selector.to_string(),
                key: key.to_string(),
            };
        }
        // Bare identifier left of dot → treat as NodeId.
        if !head.is_empty() && !key.is_empty() {
            return SourceRef::NodeProp {
                node: head.to_string(),
                key: key.to_string(),
            };
        }
    }
    SourceRef::Literal(trimmed.to_string())
}

impl DocumentBindings {
    /// Build an empty binding context with a fresh `Owner`. Hosts
    /// generally call [`BuilderDocument::install_bindings`] instead,
    /// which goes through this constructor and then walks the
    /// document's `connections` to install effects.
    pub fn new() -> Self {
        Self {
            owner: Rc::new(Owner::new()),
            props: Rc::new(RefCell::new(HashMap::new())),
            effect_count: 0,
            unresolved: Vec::new(),
        }
    }

    /// Get-or-create the reactive prop bag for a NodeId. The first
    /// call seeds the bag from `initial_props`; later calls ignore
    /// `initial_props` (the cached bag is the canonical source).
    pub fn props_for(&self, node_id: &str, initial_props: &Value) -> ReactiveProps {
        if let Some(existing) = self.props.borrow().get(node_id) {
            return existing.clone();
        }
        let bag = ReactiveProps::new(initial_props.clone());
        self.props
            .borrow_mut()
            .insert(node_id.to_string(), bag.clone());
        bag
    }

    /// Number of installed `Bind` effects.
    pub fn effect_count(&self) -> usize {
        self.effect_count
    }

    /// Bindings that couldn't resolve at install time.
    pub fn unresolved(&self) -> &[UnresolvedBind] {
        &self.unresolved
    }

    /// Number of materialised per-NodeId reactive prop bags.
    pub fn cached_node_count(&self) -> usize {
        self.props.borrow().len()
    }

    /// The shared reactive owner. Hosts that want to install
    /// additional effects backed by this binding context's lifetime
    /// can use it directly.
    pub fn owner(&self) -> &Owner {
        &self.owner
    }

    /// Read the current value of a NodeId's prop key (non-subscribing).
    /// Returns `None` when the NodeId has no materialised bag.
    pub fn peek(&self, node_id: &str, key: &str) -> Option<Value> {
        self.props.borrow().get(node_id).map(|bag| bag.get(key))
    }

    /// Install effects for every `ActionKind::Bind` connection in
    /// the document. Source nodes are looked up in the document
    /// tree; literals resolve immediately; selector references are
    /// recorded as [`unresolved`](DocumentBindings::unresolved).
    pub fn install_for(&mut self, document: &BuilderDocument) {
        // First, snapshot every Node in the document into a Map so
        // sources can be resolved by NodeId in O(1).
        let mut nodes_by_id: HashMap<NodeId, &Node> = HashMap::new();
        if let Some(root) = document.root.as_ref() {
            collect_nodes(root, &mut nodes_by_id);
        }
        for (_, zone_nodes) in document.zones.iter() {
            for n in zone_nodes {
                collect_nodes(n, &mut nodes_by_id);
            }
        }

        for conn in &document.connections {
            let ActionKind::Bind { target_key, source } = &conn.action else {
                continue;
            };
            self.install_one(conn, target_key, source, &nodes_by_id);
        }
    }

    fn install_one(
        &mut self,
        conn: &Connection,
        target_key: &str,
        source: &str,
        nodes_by_id: &HashMap<NodeId, &Node>,
    ) {
        let target_node = match nodes_by_id.get(&conn.target_node) {
            Some(n) => n,
            None => {
                self.unresolved.push(UnresolvedBind {
                    connection_id: conn.id.clone(),
                    target_node: conn.target_node.clone(),
                    target_key: target_key.to_string(),
                    source: source.to_string(),
                    reason: BindResolveError::SourceNodeMissing,
                });
                return;
            }
        };
        let target_props = self.props_for(&target_node.id, &target_node.props);
        let target_key_owned = target_key.to_string();

        match parse_source(source) {
            SourceRef::Literal(lit) => {
                // No subscription needed — write once.
                target_props.set(&target_key_owned, Value::String(lit));
            }
            SourceRef::NodeProp { node, key } => {
                let Some(src_node) = nodes_by_id.get(&node) else {
                    self.unresolved.push(UnresolvedBind {
                        connection_id: conn.id.clone(),
                        target_node: conn.target_node.clone(),
                        target_key: target_key_owned,
                        source: source.to_string(),
                        reason: BindResolveError::SourceNodeMissing,
                    });
                    return;
                };
                let source_props = self.props_for(&src_node.id, &src_node.props);
                let source_sig = source_props.signal(&key);
                let target_props_clone = target_props.clone();
                self.owner.insert_effect(move || {
                    let value = source_sig.snapshot_after_subscribe();
                    target_props_clone.set(&target_key_owned, value);
                });
                self.effect_count += 1;
            }
            SourceRef::Selector { .. } => {
                self.unresolved.push(UnresolvedBind {
                    connection_id: conn.id.clone(),
                    target_node: conn.target_node.clone(),
                    target_key: target_key_owned,
                    source: source.to_string(),
                    reason: BindResolveError::SelectorReference,
                });
            }
        }
    }
}

impl Default for DocumentBindings {
    fn default() -> Self {
        Self::new()
    }
}

fn collect_nodes<'a>(node: &'a Node, out: &mut HashMap<NodeId, &'a Node>) {
    out.insert(node.id.clone(), node);
    for child in &node.children {
        collect_nodes(child, out);
    }
}

impl BuilderDocument {
    /// **Phase 4** of `docs/dev/dioxus-inspiration.md`: install
    /// `ActionKind::Bind` connections as `Effect`s. Returns a
    /// [`DocumentBindings`] that owns the per-Node reactive prop
    /// bags + every installed effect. Drop the result to tear down
    /// the effects (and their upstream subscriptions).
    pub fn install_bindings(&self) -> DocumentBindings {
        let mut bindings = DocumentBindings::new();
        bindings.install_for(self);
        bindings
    }
}

// `Signal::snapshot()` is the non-subscribing read. Inside an
// `Effect`, we want a subscribing read so the effect re-fires when
// the source signal changes. `Signal::read(|v| v.clone())` subscribes
// but takes a closure; this little helper hides the closure plumbing
// in one place so the install path reads linearly.
trait SignalValueExt {
    fn snapshot_after_subscribe(&self) -> Value;
}

impl SignalValueExt for Signal<Value> {
    fn snapshot_after_subscribe(&self) -> Value {
        self.read(|v| v.clone())
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
    fn keys_reports_every_set_prop_including_post_construction_writes() {
        let p = props(serde_json::json!({"a": 1, "b": 2}));
        let mut k = p.keys();
        k.sort();
        assert_eq!(k, vec!["a".to_string(), "b".to_string()]);
        p.set("c", serde_json::json!(3));
        let mut k = p.keys();
        k.sort();
        assert_eq!(k, vec!["a".to_string(), "b".to_string(), "c".to_string()]);
        // Null bag → no keys (the coerced-empty-object case).
        assert!(ReactiveProps::new(Value::Null).keys().is_empty());
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

    // -------------- DocumentBindings tests (Phase 4a) --------------

    use crate::document::{BuilderDocument, Node, NodeId};
    use crate::signal::{ActionKind, Connection};

    fn node(id: &str, props: Value) -> Node {
        Node {
            id: id.to_string(),
            component: "container".into(),
            props,
            ..Default::default()
        }
    }

    fn bind(
        id: &str,
        source_node: &str,
        target: NodeId,
        target_key: &str,
        source: &str,
    ) -> Connection {
        Connection {
            id: id.to_string(),
            source_node: source_node.to_string(),
            signal: "value-changed".into(),
            target_node: target,
            action: ActionKind::Bind {
                target_key: target_key.to_string(),
                source: source.to_string(),
            },
            params: Value::Null,
        }
    }

    #[test]
    fn parse_source_recognises_node_prop_form() {
        assert_eq!(
            parse_source("nodeA.title"),
            SourceRef::NodeProp {
                node: "nodeA".into(),
                key: "title".into()
            }
        );
    }

    #[test]
    fn parse_source_recognises_selector_form() {
        assert_eq!(
            parse_source("$selection.name"),
            SourceRef::Selector {
                selector: "selection".into(),
                key: "name".into()
            }
        );
    }

    #[test]
    fn parse_source_falls_back_to_literal() {
        assert_eq!(parse_source("hello"), SourceRef::Literal("hello".into()));
    }

    #[test]
    fn install_bindings_wires_node_to_node_prop_bind() {
        // Phase 4a end-to-end: an `ActionKind::Bind` connection
        // compiles to an `Effect` that mirrors source.key → target.key.
        let mut root = node("root", serde_json::json!({}));
        root.children = vec![
            node("src", serde_json::json!({ "title": "hello" })),
            node("dst", serde_json::json!({ "title": "" })),
        ];
        let mut doc = BuilderDocument {
            root: Some(root),
            ..Default::default()
        };
        doc.connections
            .push(bind("c1", "src", "dst".into(), "title", "src.title"));

        let bindings = doc.install_bindings();
        assert_eq!(bindings.effect_count(), 1);
        assert!(bindings.unresolved().is_empty());

        // The effect's initial run wrote the source value into the
        // target. The target's reactive props bag now reflects it.
        let dst = bindings.peek("dst", "title").expect("dst materialised");
        assert_eq!(dst, serde_json::json!("hello"));

        // Mutating the source signal re-fires the effect.
        let src_props = bindings.props_for("src", &serde_json::json!({}));
        src_props.set("title", serde_json::json!("world"));
        let dst = bindings.peek("dst", "title").expect("dst materialised");
        assert_eq!(dst, serde_json::json!("world"));
    }

    #[test]
    fn install_bindings_records_missing_source_node() {
        let mut root = node("root", serde_json::json!({}));
        root.children = vec![node("dst", serde_json::json!({}))];
        let mut doc = BuilderDocument {
            root: Some(root),
            ..Default::default()
        };
        doc.connections
            .push(bind("c1", "missing", "dst".into(), "x", "missing.value"));

        let bindings = doc.install_bindings();
        assert_eq!(bindings.effect_count(), 0);
        assert_eq!(bindings.unresolved().len(), 1);
        assert_eq!(
            bindings.unresolved()[0].reason,
            BindResolveError::SourceNodeMissing
        );
    }

    #[test]
    fn install_bindings_records_selector_reference() {
        let mut root = node("root", serde_json::json!({}));
        root.children = vec![node("dst", serde_json::json!({}))];
        let mut doc = BuilderDocument {
            root: Some(root),
            ..Default::default()
        };
        doc.connections
            .push(bind("c1", "any", "dst".into(), "x", "$selection.name"));

        let bindings = doc.install_bindings();
        assert_eq!(bindings.effect_count(), 0);
        assert_eq!(bindings.unresolved().len(), 1);
        assert_eq!(
            bindings.unresolved()[0].reason,
            BindResolveError::SelectorReference
        );
    }

    #[test]
    fn install_bindings_writes_literal_once_without_subscription() {
        let mut root = node("root", serde_json::json!({}));
        root.children = vec![node("dst", serde_json::json!({}))];
        let mut doc = BuilderDocument {
            root: Some(root),
            ..Default::default()
        };
        doc.connections
            .push(bind("c1", "any", "dst".into(), "label", "Hello"));

        let bindings = doc.install_bindings();
        // No subscription installed — literal is a one-shot write.
        assert_eq!(bindings.effect_count(), 0);
        let dst = bindings.peek("dst", "label").expect("dst materialised");
        assert_eq!(dst, serde_json::json!("Hello"));
    }

    #[test]
    fn install_bindings_drop_disposes_effects() {
        let mut root = node("root", serde_json::json!({}));
        root.children = vec![
            node("src", serde_json::json!({ "v": "a" })),
            node("dst", serde_json::json!({ "v": "" })),
        ];
        let mut doc = BuilderDocument {
            root: Some(root),
            ..Default::default()
        };
        doc.connections
            .push(bind("c1", "src", "dst".into(), "v", "src.v"));

        let bindings = doc.install_bindings();
        let src_bag = bindings.props_for("src", &serde_json::json!({}));
        // Keep the source bag alive after dropping the bindings so the
        // writer can still mutate it; the effect should already be torn
        // down via DocumentBindings::owner::drop.
        drop(bindings);
        // No panic, no observable side effect (the effect is gone).
        src_bag.set("v", serde_json::json!("changed"));
    }

    #[test]
    fn props_for_returns_cached_bag_on_subsequent_calls() {
        let bindings = DocumentBindings::new();
        let bag1 = bindings.props_for("n1", &serde_json::json!({"a": 1}));
        let bag2 = bindings.props_for("n1", &serde_json::json!({"a": 999}));
        // Second call ignores its initial_props arg — bag1 is canonical.
        assert_eq!(bag1.get("a"), serde_json::json!(1));
        assert_eq!(bag2.get("a"), serde_json::json!(1));
        // Mutation is shared.
        bag1.set("a", serde_json::json!(2));
        assert_eq!(bag2.get("a"), serde_json::json!(2));
    }
}
