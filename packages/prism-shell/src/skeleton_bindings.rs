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
//! Effect installation against AppState slots is the next slice; the
//! collected list is the seam every consumer agrees on.

use prism_ui_runtime::layout::Node;

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
}
