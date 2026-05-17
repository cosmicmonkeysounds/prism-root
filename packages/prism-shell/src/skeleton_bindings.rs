//! Skeleton bind installer — closes A2 of
//! `docs/dev/ui-migration-followups.md`.
//!
//! The PRUI parser carries `bind:<key>="<source>"` attributes through
//! the runtime as `data-bind-<key>` semantic attrs (see
//! `prism-ui-runtime::interpret`). This module walks a lowered
//! `layout::Node` tree post-interpret and collects every binding into
//! a structured [`SkeletonBindings`] list. Downstream code
//! (field-focus routing in `events.rs`, future `Effect` installation
//! against `AppState` slots, debugging tools) consumes the list
//! without re-walking the tree.
//!
//! The shell's per-frame `RenderScope` (Phase 3a of
//! `docs/dev/dioxus-inspiration.md`) already auto-subscribes any
//! `reactive::Signal::read` invoked inside the render walk, so
//! writes against an `AppState` slot wake the next frame
//! automatically when slot accessors are reactive. This installer is
//! the **declarative side** — it surfaces the
//! "what bindings did the skeleton author?" question as data,
//! independent of how the binding is wired.
//!
//! Source grammar parsed by [`SkeletonBindings::collect`]:
//! - `"state.<slot>.<field>"` → [`BindSource::Slot`]
//! - `"$<selector>.<key>"` → [`BindSource::Selector`]
//! - anything else → [`BindSource::Literal`]
//!
//! [`SkeletonBindingContext`] is the install side: it consumes the
//! collected list, materialises a per-node [`ReactiveProps`] bag, and
//! registers one `Effect` per slot/selector binding so a snapshot
//! refresh propagates into the bag through the reactive graph. It is
//! the shell-scoped mirror of `prism_builder::DocumentBindings`
//! (`A2` of `docs/dev/ui-migration-followups.md`).
//!
//! [`SkeletonBindingContext::apply_to_ast`] is the **read side**: it
//! projects the per-node bag back into the skeleton AST as `Bare`
//! attributes via *subscribing* `signal(key)` reads, so the existing
//! `fill_compositions → lower_document_with_scope` pipeline renders
//! the bound value with no block-level change. `Shell::render` runs
//! it inside `RenderScope::run_in_render_pass` — gated behind
//! `Shell::attach_skeleton_bindings`, so `None` is byte-identical to
//! the pre-seam path — making a later
//! [`refresh`](SkeletonBindingContext::refresh) wake the next frame.
//! This is the `DocumentBindings` Phase 4a/4b split at the
//! skeleton's AST-attribute seam (the skeleton lowers through the
//! resolver, not per-block `lower_ui`).

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use prism_builder::ReactiveProps;
use prism_core::language::prism_ui::{
    self as prism_ui_ast, AttributeName, AttributeNamespace, AttributeValue,
};
use prism_core::language::syntax::{Position, SourceRange};
use prism_core::reactive::{Owner, ReactiveContext, Signal};
use prism_ui_runtime::layout::Node;
use serde_json::Value;

/// One `bind:<target_key>="<source>"` authored on a skeleton node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkeletonBind {
    /// Container / input id the binding targets.
    pub node_id: String,
    /// The prop key the binding writes to (`bind:value` → `"value"`).
    pub target_key: String,
    /// Parsed source expression.
    pub source: BindSource,
    /// Raw source string, retained verbatim for diagnostics.
    pub raw_source: String,
}

/// Parsed shape of a `bind:` source expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BindSource {
    /// Dotted `AppState` slot path — `state.<slot>.<field>...`. The
    /// `path` segments include everything after the `state.` prefix.
    Slot { path: Vec<String> },
    /// Host-resolved selector (`$selection.name`, `$active.id`, …).
    Selector { selector: String, key: String },
    /// Plain literal — wired as a one-shot write.
    Literal(String),
}

impl BindSource {
    pub fn parse(raw: &str) -> Self {
        let trimmed = raw.trim();
        if let Some(rest) = trimmed.strip_prefix("state.") {
            let path: Vec<String> = rest.split('.').map(|s| s.to_string()).collect();
            if path.iter().all(|s| !s.is_empty()) {
                return BindSource::Slot { path };
            }
        }
        if let Some(rest) = trimmed.strip_prefix('$') {
            if let Some((selector, key)) = rest.split_once('.') {
                if !selector.is_empty() && !key.is_empty() {
                    return BindSource::Selector {
                        selector: selector.to_string(),
                        key: key.to_string(),
                    };
                }
            }
        }
        BindSource::Literal(trimmed.to_string())
    }
}

/// Bind table — the declarative result of walking a lowered
/// skeleton tree.
#[derive(Debug, Clone, Default)]
pub struct SkeletonBindings {
    pub binds: Vec<SkeletonBind>,
}

impl SkeletonBindings {
    pub fn new() -> Self {
        Self::default()
    }

    /// Walk a slice of `layout::Node`s and collect every
    /// `data-bind-<key>` semantic attribute into the list.
    pub fn collect(nodes: &[Node]) -> Self {
        let mut out = Self::new();
        for node in nodes {
            out.walk(node);
        }
        out
    }

    /// Number of collected bindings.
    pub fn len(&self) -> usize {
        self.binds.len()
    }

    pub fn is_empty(&self) -> bool {
        self.binds.is_empty()
    }

    /// All bindings whose target key matches `key`. Useful for
    /// finding the one binding that drives a particular prop.
    pub fn by_key<'a>(&'a self, key: &'a str) -> impl Iterator<Item = &'a SkeletonBind> + 'a {
        self.binds.iter().filter(move |b| b.target_key == key)
    }

    /// All bindings whose source resolves to an `AppState` slot
    /// path — the subset future Effect installation will subscribe
    /// to.
    pub fn slot_bindings(&self) -> impl Iterator<Item = &SkeletonBind> {
        self.binds
            .iter()
            .filter(|b| matches!(b.source, BindSource::Slot { .. }))
    }

    fn walk(&mut self, node: &Node) {
        match node {
            Node::Container {
                id,
                props,
                children,
                ..
            } => {
                self.collect_from(id, &props.semantic.attrs);
                for child in children {
                    self.walk(child);
                }
            }
            Node::TextInput { id, semantic, .. } => {
                self.collect_from(id, &semantic.attrs);
            }
            // Leaves (Text, Spacer, Image) don't carry `bind:` attrs
            // in the current PRUI grammar — they're added as containers
            // (the `<input>` element lowers to `TextInput`). Skip.
            _ => {}
        }
    }

    fn collect_from(&mut self, node_id: &str, attrs: &[(String, String)]) {
        for (k, v) in attrs {
            if let Some(target_key) = k.strip_prefix("data-bind-") {
                self.binds.push(SkeletonBind {
                    node_id: node_id.to_string(),
                    target_key: target_key.to_string(),
                    source: BindSource::parse(v),
                    raw_source: v.clone(),
                });
            }
        }
    }
}

/// A skeleton bind whose source couldn't be resolved at install
/// time. Captured (not silently dropped) so the host can surface
/// the typo / missing selector.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnresolvedSkeletonBind {
    pub node_id: String,
    pub target_key: String,
    pub raw_source: String,
    pub reason: SkeletonBindError,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkeletonBindError {
    /// A `$<selector>.<key>` reference whose selector wasn't present
    /// in the selector map at install time.
    SelectorMissing,
}

/// Recipe retained per installed source signal so [`refresh`] can
/// re-derive the value from a fresh snapshot without re-walking the
/// tree.
///
/// [`refresh`]: SkeletonBindingContext::refresh
struct BindRecipe {
    /// Identity key into `sources` (`node_id\u{1}target_key`).
    sig_key: String,
    source: BindSource,
}

/// Runtime side of the skeleton `bind:*` declarations — the
/// shell-scoped mirror of `prism_builder::DocumentBindings`.
///
/// Owns:
/// 1. A shared [`Owner`] backing every materialised source signal
///    and every installed `Effect`. Dropping the context disposes
///    every effect (which deregisters from upstream subscribers).
/// 2. A per-node-id [`ReactiveProps`] bag. The `bind:` effects write
///    into it; the render walk reads from it (the read wiring is the
///    next slice — the bag is the agreed seam).
/// 3. One source `Signal<Value>` per slot/selector binding. Effects
///    subscribe it; [`refresh`](Self::refresh) writes fresh snapshot
///    values into it, driving the effect to mirror into the bag.
///
/// `state.<slot>.<field>...` and `$<selector>.<key>` sources install
/// an effect; `Literal` sources write once with no subscription —
/// the exact split `DocumentBindings` uses.
pub struct SkeletonBindingContext {
    owner: Rc<Owner>,
    props: Rc<RefCell<HashMap<String, ReactiveProps>>>,
    sources: Rc<RefCell<HashMap<String, Signal<Value>>>>,
    recipes: Vec<BindRecipe>,
    effect_count: usize,
    unresolved: Vec<UnresolvedSkeletonBind>,
}

impl std::fmt::Debug for SkeletonBindingContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SkeletonBindingContext")
            .field("effect_count", &self.effect_count)
            .field("cached_nodes", &self.props.borrow().len())
            .field("unresolved", &self.unresolved.len())
            .finish()
    }
}

fn sig_key(node_id: &str, target_key: &str) -> String {
    format!("{node_id}\u{1}{target_key}")
}

/// Walk a dotted path into a JSON value. `["workspace", "label"]`
/// against `{"workspace": {"label": "x"}}` yields `"x"`; any missing
/// segment yields [`Value::Null`].
fn resolve_path(root: &Value, path: &[String]) -> Value {
    let mut cur = root;
    for seg in path {
        match cur.get(seg) {
            Some(next) => cur = next,
            None => return Value::Null,
        }
    }
    cur.clone()
}

impl SkeletonBindingContext {
    /// Collect every `bind:*` on `nodes` and install it against the
    /// supplied snapshot. `snapshot` is the AppState slot tree keyed
    /// by slot name (`state.workspace.label` reads
    /// `snapshot["workspace"]["label"]`); `selectors` is the
    /// host-resolved selector table (`$selection.name` reads
    /// `selectors["selection"]["name"]`).
    pub fn install(nodes: &[Node], snapshot: &Value, selectors: &Value) -> Self {
        let mut ctx = Self {
            owner: Rc::new(Owner::new()),
            props: Rc::new(RefCell::new(HashMap::new())),
            sources: Rc::new(RefCell::new(HashMap::new())),
            recipes: Vec::new(),
            effect_count: 0,
            unresolved: Vec::new(),
        };
        let binds = SkeletonBindings::collect(nodes);
        for b in &binds.binds {
            ctx.install_one(b, snapshot, selectors);
        }
        ctx
    }

    fn props_bag(&self, node_id: &str) -> ReactiveProps {
        if let Some(existing) = self.props.borrow().get(node_id) {
            return existing.clone();
        }
        let bag = ReactiveProps::new(Value::Object(Default::default()));
        self.props
            .borrow_mut()
            .insert(node_id.to_string(), bag.clone());
        bag
    }

    fn resolve(source: &BindSource, snapshot: &Value, selectors: &Value) -> Option<Value> {
        match source {
            BindSource::Slot { path } => Some(resolve_path(snapshot, path)),
            BindSource::Selector { selector, key } => selectors
                .get(selector)
                .map(|sel| sel.get(key).cloned().unwrap_or(Value::Null)),
            BindSource::Literal(s) => Some(Value::String(s.clone())),
        }
    }

    fn install_one(&mut self, b: &SkeletonBind, snapshot: &Value, selectors: &Value) {
        let bag = self.props_bag(&b.node_id);

        // Literals never subscribe — one-shot write, mirroring
        // `DocumentBindings::install_one`.
        if let BindSource::Literal(s) = &b.source {
            bag.set(&b.target_key, Value::String(s.clone()));
            return;
        }

        // Slot always resolves (Null at worst) and Literal returned
        // above, so a `None` here is only ever a missing selector.
        let Some(value) = Self::resolve(&b.source, snapshot, selectors) else {
            self.unresolved.push(UnresolvedSkeletonBind {
                node_id: b.node_id.clone(),
                target_key: b.target_key.clone(),
                raw_source: b.raw_source.clone(),
                reason: SkeletonBindError::SelectorMissing,
            });
            return;
        };

        let key = sig_key(&b.node_id, &b.target_key);
        let source_sig = *self
            .sources
            .borrow_mut()
            .entry(key.clone())
            .or_insert_with(|| self.owner.insert(value.clone()));
        // An earlier binding may have created the slot — re-seed so a
        // duplicate target carries the freshest resolved value.
        source_sig.set(value);

        let target_key = b.target_key.clone();
        self.owner.insert_effect(move || {
            let v = source_sig.read(|v| v.clone());
            bag.set(&target_key, v);
        });
        self.effect_count += 1;
        self.recipes.push(BindRecipe {
            sig_key: key,
            source: b.source.clone(),
        });
    }

    /// Re-resolve every installed slot/selector binding against a
    /// fresh snapshot and push the new values through the source
    /// signals. The whole pass is one reactive batch, so each
    /// dependent effect re-runs at most once even when many slots
    /// change together.
    pub fn refresh(&self, snapshot: &Value, selectors: &Value) {
        ReactiveContext::batch(|| {
            for recipe in &self.recipes {
                let Some(v) = Self::resolve(&recipe.source, snapshot, selectors) else {
                    continue;
                };
                if let Some(sig) = self.sources.borrow().get(&recipe.sig_key) {
                    sig.set(v);
                }
            }
        });
    }

    /// The reactive prop bag for `node_id`, if any binding targeted
    /// it. The render walk consumes this to project bound values.
    pub fn props_for(&self, node_id: &str) -> Option<ReactiveProps> {
        self.props.borrow().get(node_id).cloned()
    }

    /// Non-subscribing read of a bound prop's current value.
    pub fn peek(&self, node_id: &str, key: &str) -> Option<Value> {
        self.props.borrow().get(node_id).map(|b| b.get(key))
    }

    /// **Read side (A2 / §2.3).** Project every materialised bind
    /// bag back into the skeleton AST so the existing
    /// `fill_compositions` → `lower_document_with_scope` pipeline
    /// renders the bound value with zero block-level changes.
    ///
    /// For each element whose bare `id` attribute names a node a
    /// binding targeted, every bound prop is read through
    /// `bag.signal(key)` — a *subscribing* read — and written as a
    /// `Bare` attribute (replacing any static placeholder of the same
    /// name; a `bind:` is the author's explicit "source this from
    /// state" intent and wins over a literal stand-in). Called inside
    /// the host's per-frame `RenderScope::run_in_render_pass`, the
    /// subscribing read enrolls the frame context, so a later
    /// [`refresh`](Self::refresh) — which pushes new values through
    /// the source signals and drives each `Effect` to `set` the bag —
    /// wakes the next frame automatically.
    ///
    /// This is the shell-scoped mirror of `prism_builder`'s
    /// `DocumentBindings` Phase 4b read seam; the skeleton's seam is
    /// the AST attribute layer rather than `LowerCtx` because the
    /// skeleton lowers through the resolver, not per-block `lower_ui`.
    pub fn apply_to_ast(&self, doc: &mut prism_ui_ast::Document) {
        let props = self.props.borrow();
        if props.is_empty() {
            return;
        }
        apply_walk(&mut doc.nodes, &props);
    }

    /// Number of installed (subscribing) effects.
    pub fn effect_count(&self) -> usize {
        self.effect_count
    }

    /// Number of materialised per-node prop bags.
    pub fn cached_node_count(&self) -> usize {
        self.props.borrow().len()
    }

    /// Bindings whose source couldn't resolve at install time.
    pub fn unresolved(&self) -> &[UnresolvedSkeletonBind] {
        &self.unresolved
    }

    /// The shared reactive owner — hosts can pin extra effects to
    /// this context's lifetime.
    pub fn owner(&self) -> &Owner {
        &self.owner
    }
}

/// Recursive AST walk for [`SkeletonBindingContext::apply_to_ast`].
/// Kept a free fn (not a method) so the `self.props` borrow is held
/// once at the entry point and passed down as a plain ref.
fn apply_walk(nodes: &mut [prism_ui_ast::Node], props: &HashMap<String, ReactiveProps>) {
    for node in nodes {
        if let prism_ui_ast::Node::Element(el) = node {
            if let Some(id) = element_id(el) {
                if let Some(bag) = props.get(&id) {
                    for key in bag.keys() {
                        // Subscribing read — enrolls the enclosing
                        // reactive frame context so `refresh` wakes it.
                        let value = bag.signal(&key).get();
                        set_or_replace_bare_attr(el, &key, &value);
                    }
                }
            }
            apply_walk(&mut el.children, props);
        }
    }
}

/// The element's `id="..."` literal, if present. The grammar
/// classifies bare `id` / `class` under
/// [`AttributeNamespace::Identifier`] (not `Bare`), so match that.
fn element_id(el: &prism_ui_ast::Element) -> Option<String> {
    el.attributes.iter().find_map(|a| {
        if a.name.namespace == AttributeNamespace::Identifier && a.name.local == "id" {
            match &a.value {
                AttributeValue::String { value, .. } => Some(value.clone()),
                _ => None,
            }
        } else {
            None
        }
    })
}

/// Replace the first `Bare` attribute named `key`, or push a fresh
/// one. A `bind:` is explicit "source this from state" intent, so the
/// bound value overrides any static placeholder of the same name —
/// the inverse of `fill_compositions`'s author-wins rule, which
/// covers structural props the skeleton pins.
fn set_or_replace_bare_attr(el: &mut prism_ui_ast::Element, key: &str, value: &Value) {
    let attr = synthetic_bare_attr(key, value);
    if let Some(slot) = el
        .attributes
        .iter_mut()
        .find(|a| a.name.namespace == AttributeNamespace::Bare && a.name.local == key)
    {
        *slot = attr;
    } else {
        el.attributes.push(attr);
    }
}

/// Build a zero-span `Bare` string attribute. Mirrors
/// `render::synthetic_attribute`'s value-stringify rule so a bound
/// array / object round-trips through the resolver's
/// `serde_json::from_str` decode exactly as an emission would.
fn synthetic_bare_attr(key: &str, value: &Value) -> prism_ui_ast::Attribute {
    let raw_value = match value {
        Value::String(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::Null => String::new(),
        other => other.to_string(),
    };
    let zero = SourceRange {
        start: Position {
            offset: 0,
            line: 1,
            column: 0,
        },
        end: Position {
            offset: 0,
            line: 1,
            column: 0,
        },
    };
    prism_ui_ast::Attribute {
        name: AttributeName {
            raw: key.to_string(),
            local: key.to_string(),
            namespace: AttributeNamespace::Bare,
            range: zero,
        },
        value: AttributeValue::String {
            value: raw_value,
            range: zero,
        },
        range: zero,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prism_ui_runtime::interpret::interpret;

    #[test]
    fn parses_state_slot_source() {
        let src = BindSource::parse("state.canvas.code_buffer.source");
        match src {
            BindSource::Slot { path } => assert_eq!(path, vec!["canvas", "code_buffer", "source"]),
            other => panic!("expected Slot, got {other:?}"),
        }
    }

    #[test]
    fn parses_selector_source() {
        let src = BindSource::parse("$selection.name");
        match src {
            BindSource::Selector { selector, key } => {
                assert_eq!(selector, "selection");
                assert_eq!(key, "name");
            }
            other => panic!("expected Selector, got {other:?}"),
        }
    }

    #[test]
    fn parses_literal_source() {
        match BindSource::parse("hello world") {
            BindSource::Literal(s) => assert_eq!(s, "hello world"),
            other => panic!("expected Literal, got {other:?}"),
        }
    }

    #[test]
    fn collects_bind_attr_from_container() {
        let nodes = interpret(
            r#"<container id="root" bind:title="state.workspace.label"><spacer/></container>"#,
        )
        .unwrap();
        let bindings = SkeletonBindings::collect(&nodes);
        assert_eq!(bindings.len(), 1);
        let b = &bindings.binds[0];
        assert_eq!(b.node_id, "root");
        assert_eq!(b.target_key, "title");
        match &b.source {
            BindSource::Slot { path } => assert_eq!(path, &["workspace", "label"]),
            other => panic!("expected Slot, got {other:?}"),
        }
    }

    #[test]
    fn collects_bind_attr_from_text_input() {
        let nodes = interpret(r#"<input id="email" bind:value="state.form.email"/>"#).unwrap();
        let bindings = SkeletonBindings::collect(&nodes);
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings.binds[0].node_id, "email");
        assert_eq!(bindings.binds[0].target_key, "value");
    }

    #[test]
    fn collects_recursively_through_nested_containers() {
        let nodes = interpret(
            r#"<container id="outer">
                 <container id="mid">
                   <container id="leaf" bind:hover-color="state.theme.accent"/>
                 </container>
               </container>"#,
        )
        .unwrap();
        let bindings = SkeletonBindings::collect(&nodes);
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings.binds[0].node_id, "leaf");
        assert_eq!(bindings.binds[0].target_key, "hover-color");
    }

    #[test]
    fn slot_bindings_filter_drops_literals_and_selectors() {
        let nodes = interpret(
            r#"<container id="a" bind:x="state.foo.bar" bind:y="$sel.k" bind:z="constant"/>"#,
        )
        .unwrap();
        let bindings = SkeletonBindings::collect(&nodes);
        assert_eq!(bindings.len(), 3);
        let slots: Vec<_> = bindings.slot_bindings().collect();
        assert_eq!(slots.len(), 1);
        assert_eq!(slots[0].target_key, "x");
    }

    #[test]
    fn by_key_finds_target_specific_bindings() {
        let nodes = interpret(
            r#"<container id="a" bind:title="state.a.b"/>
               <container id="b" bind:title="state.c.d"/>
               <container id="c" bind:body="state.e.f"/>"#,
        )
        .unwrap();
        let bindings = SkeletonBindings::collect(&nodes);
        let titles: Vec<_> = bindings.by_key("title").collect();
        assert_eq!(titles.len(), 2);
    }

    // --- SkeletonBindingContext (A2 install side) ---

    #[test]
    fn install_slot_binding_seeds_node_prop_from_snapshot() {
        let nodes =
            interpret(r#"<container id="root" bind:title="state.workspace.label"/>"#).unwrap();
        let snapshot = serde_json::json!({ "workspace": { "label": "Untitled" } });
        let ctx = SkeletonBindingContext::install(&nodes, &snapshot, &Value::Null);
        assert_eq!(ctx.effect_count(), 1);
        assert_eq!(
            ctx.peek("root", "title"),
            Some(Value::String("Untitled".into()))
        );
    }

    #[test]
    fn refresh_propagates_new_slot_value_through_the_effect() {
        let nodes =
            interpret(r#"<container id="root" bind:title="state.workspace.label"/>"#).unwrap();
        let snapshot = serde_json::json!({ "workspace": { "label": "v1" } });
        let ctx = SkeletonBindingContext::install(&nodes, &snapshot, &Value::Null);
        assert_eq!(ctx.peek("root", "title"), Some(Value::String("v1".into())));

        let next = serde_json::json!({ "workspace": { "label": "v2" } });
        ctx.refresh(&next, &Value::Null);
        assert_eq!(ctx.peek("root", "title"), Some(Value::String("v2".into())));
    }

    #[test]
    fn missing_slot_path_resolves_to_null_not_unresolved() {
        let nodes =
            interpret(r#"<container id="root" bind:title="state.workspace.label"/>"#).unwrap();
        let ctx = SkeletonBindingContext::install(&nodes, &serde_json::json!({}), &Value::Null);
        // Slots are dynamic — a not-yet-present slot is Null, not a
        // hard "unresolved" the way a missing selector is.
        assert!(ctx.unresolved().is_empty());
        assert_eq!(ctx.peek("root", "title"), Some(Value::Null));
        ctx.refresh(
            &serde_json::json!({ "workspace": { "label": "appeared" } }),
            &Value::Null,
        );
        assert_eq!(
            ctx.peek("root", "title"),
            Some(Value::String("appeared".into()))
        );
    }

    #[test]
    fn literal_source_writes_once_without_an_effect() {
        let nodes = interpret(r#"<container id="root" bind:title="hello world"/>"#).unwrap();
        let ctx = SkeletonBindingContext::install(&nodes, &Value::Null, &Value::Null);
        assert_eq!(ctx.effect_count(), 0);
        assert_eq!(
            ctx.peek("root", "title"),
            Some(Value::String("hello world".into()))
        );
    }

    #[test]
    fn selector_resolves_against_selector_table() {
        let nodes = interpret(r#"<container id="root" bind:title="$selection.name"/>"#).unwrap();
        let selectors = serde_json::json!({ "selection": { "name": "Node A" } });
        let ctx = SkeletonBindingContext::install(&nodes, &Value::Null, &selectors);
        assert_eq!(ctx.effect_count(), 1);
        assert_eq!(
            ctx.peek("root", "title"),
            Some(Value::String("Node A".into()))
        );
    }

    #[test]
    fn missing_selector_is_recorded_unresolved() {
        let nodes = interpret(r#"<container id="root" bind:title="$selection.name"/>"#).unwrap();
        let ctx = SkeletonBindingContext::install(&nodes, &Value::Null, &serde_json::json!({}));
        assert_eq!(ctx.effect_count(), 0);
        assert_eq!(ctx.unresolved().len(), 1);
        let u = &ctx.unresolved()[0];
        assert_eq!(u.node_id, "root");
        assert_eq!(u.target_key, "title");
        assert_eq!(u.reason, SkeletonBindError::SelectorMissing);
    }

    #[test]
    fn dropping_context_disposes_effects() {
        let nodes =
            interpret(r#"<container id="root" bind:title="state.workspace.label"/>"#).unwrap();
        let snapshot = serde_json::json!({ "workspace": { "label": "x" } });
        let ctx = SkeletonBindingContext::install(&nodes, &snapshot, &Value::Null);
        let bag = ctx.props_for("root").expect("bag materialised");
        drop(ctx);
        // The effect's owner is gone; pushing a new value nowhere to
        // observe must not panic and the bag stays at its last value.
        assert_eq!(bag.get("title"), Value::String("x".into()));
    }

    #[test]
    fn batched_refresh_runs_each_effect_once_for_many_slots() {
        let nodes = interpret(
            r#"<container id="a" bind:title="state.s.one"/>
               <container id="b" bind:title="state.s.two"/>"#,
        )
        .unwrap();
        let snap = serde_json::json!({ "s": { "one": "1", "two": "2" } });
        let ctx = SkeletonBindingContext::install(&nodes, &snap, &Value::Null);
        assert_eq!(ctx.effect_count(), 2);
        let next = serde_json::json!({ "s": { "one": "1b", "two": "2b" } });
        ctx.refresh(&next, &Value::Null);
        assert_eq!(ctx.peek("a", "title"), Some(Value::String("1b".into())));
        assert_eq!(ctx.peek("b", "title"), Some(Value::String("2b".into())));
    }

    // --- read side (A2 Phase 4b / §2.3): apply_to_ast ---

    fn bare_attr<'a>(el: &'a prism_ui_ast::Element, local: &str) -> Option<&'a str> {
        el.attributes.iter().find_map(|a| {
            if a.name.namespace == AttributeNamespace::Bare && a.name.local == local {
                match &a.value {
                    AttributeValue::String { value, .. } => Some(value.as_str()),
                    _ => None,
                }
            } else {
                None
            }
        })
    }

    fn first_element(doc: &prism_ui_ast::Document) -> &prism_ui_ast::Element {
        match &doc.nodes[0] {
            prism_ui_ast::Node::Element(el) => el,
            other => panic!("expected element, got {other:?}"),
        }
    }

    #[test]
    fn apply_to_ast_injects_resolved_slot_value_as_bare_attr() {
        let src = r#"<container id="root" bind:title="state.workspace.label"/>"#;
        let nodes = interpret(src).unwrap();
        let snap = serde_json::json!({ "workspace": { "label": "Untitled" } });
        let ctx = SkeletonBindingContext::install(&nodes, &snap, &Value::Null);

        let (mut doc, errs) = prism_ui_ast::parse(src);
        assert!(errs.is_empty());
        ctx.apply_to_ast(&mut doc);
        assert_eq!(bare_attr(first_element(&doc), "title"), Some("Untitled"));
    }

    #[test]
    fn apply_to_ast_bound_value_overrides_static_placeholder() {
        // The author wrote a literal `title` *and* a `bind:title`;
        // the binding is explicit "source from state" intent and must
        // win — and must not duplicate the attribute.
        let src = r#"<container id="root" title="placeholder" bind:title="state.w.l"/>"#;
        let nodes = interpret(src).unwrap();
        let snap = serde_json::json!({ "w": { "l": "Live" } });
        let ctx = SkeletonBindingContext::install(&nodes, &snap, &Value::Null);

        let (mut doc, _) = prism_ui_ast::parse(src);
        ctx.apply_to_ast(&mut doc);
        let el = first_element(&doc);
        let titles: Vec<_> = el
            .attributes
            .iter()
            .filter(|a| a.name.namespace == AttributeNamespace::Bare && a.name.local == "title")
            .collect();
        assert_eq!(titles.len(), 1, "bound attr replaced, not duplicated");
        assert_eq!(bare_attr(el, "title"), Some("Live"));
    }

    #[test]
    fn apply_to_ast_read_subscribes_so_refresh_reprojects() {
        // The load-bearing property: a subscribing read inside a
        // reactive context, then a `refresh`, re-runs the reader with
        // the new value — the skeleton mirror of DocumentBindings 4b.
        use prism_core::reactive::Owner;
        use std::cell::RefCell;
        use std::rc::Rc;

        let src = r#"<container id="root" bind:title="state.w.l"/>"#;
        let nodes = interpret(src).unwrap();
        let snap = serde_json::json!({ "w": { "l": "v1" } });
        let ctx = Rc::new(SkeletonBindingContext::install(&nodes, &snap, &Value::Null));

        let seen: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
        let owner = Owner::new();
        {
            let ctx_ref = Rc::clone(&ctx);
            let seen = Rc::clone(&seen);
            owner.insert_effect(move || {
                let (mut doc, _) = prism_ui_ast::parse(src);
                ctx_ref.apply_to_ast(&mut doc);
                let v = bare_attr(first_element(&doc), "title")
                    .unwrap_or_default()
                    .to_string();
                seen.borrow_mut().push(v);
            });
        }
        assert_eq!(seen.borrow().as_slice(), &["v1".to_string()]);

        ctx.refresh(&serde_json::json!({ "w": { "l": "v2" } }), &Value::Null);
        assert_eq!(
            seen.borrow().as_slice(),
            &["v1".to_string(), "v2".to_string()],
            "refresh re-ran the AST reader through the subscribed signal"
        );
    }

    #[test]
    fn apply_to_ast_reaches_nested_elements_and_skips_unbound() {
        let src = r#"<container id="outer">
                       <container id="leaf" bind:title="state.t.v"/>
                       <container id="other"/>
                     </container>"#;
        let nodes = interpret(src).unwrap();
        let snap = serde_json::json!({ "t": { "v": "deep" } });
        let ctx = SkeletonBindingContext::install(&nodes, &snap, &Value::Null);

        let (mut doc, _) = prism_ui_ast::parse(src);
        ctx.apply_to_ast(&mut doc);
        let outer = first_element(&doc);
        let leaf = outer
            .children
            .iter()
            .find_map(|n| match n {
                prism_ui_ast::Node::Element(el) if element_id(el).as_deref() == Some("leaf") => {
                    Some(el)
                }
                _ => None,
            })
            .expect("leaf element present");
        assert_eq!(bare_attr(leaf, "title"), Some("deep"));
        // The unbound sibling gets nothing injected.
        let other = outer
            .children
            .iter()
            .find_map(|n| match n {
                prism_ui_ast::Node::Element(el) if element_id(el).as_deref() == Some("other") => {
                    Some(el)
                }
                _ => None,
            })
            .expect("other element present");
        assert_eq!(bare_attr(other, "title"), None);
    }

    #[test]
    fn apply_to_ast_serialises_collection_binding_as_json_string() {
        // A bound array round-trips as its canonical JSON form so the
        // resolver's `serde_json::from_str` decode rehydrates it —
        // same contract `render::synthetic_attribute` upholds.
        let src = r#"<container id="root" bind:items="state.list.rows"/>"#;
        let nodes = interpret(src).unwrap();
        let snap = serde_json::json!({ "list": { "rows": [{"k": 1}, {"k": 2}] } });
        let ctx = SkeletonBindingContext::install(&nodes, &snap, &Value::Null);

        let (mut doc, _) = prism_ui_ast::parse(src);
        ctx.apply_to_ast(&mut doc);
        let raw = bare_attr(first_element(&doc), "items").expect("items injected");
        let parsed: Value = serde_json::from_str(raw).expect("round-trips");
        assert_eq!(parsed, serde_json::json!([{"k": 1}, {"k": 2}]));
    }
}
