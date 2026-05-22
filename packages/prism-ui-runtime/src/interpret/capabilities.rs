//! Phase 12 — typed host capabilities. See
//! `docs/dev/prui-expressiveness-roadmap.md` §7.9.
//!
//! A component declares the host services it needs via
//! `<requires>` body statements (parsed by the canonical reader
//! per §6.15). At instantiation time the runtime looks the names
//! up against the active [`CapabilityRegistry`] installed on the
//! [`LowerScope`]; resolved values become regular scope bindings
//! reachable via `{<name>}` interpolation. Missing **required**
//! capabilities surface a render-side diagnostic but don't halt
//! the document — graceful degradation matches the rest of the
//! lowering pipeline (unknown tags fall through, unknown mixins
//! drop silently, etc.). The `prism-cli` lint phase (Phase 17)
//! upgrades the diagnostic to a parse-time hard error.
//!
//! ## Surface
//!
//! - [`CapabilityDef`] — one declared `requires name: Type[?]`
//!   row. `optional` mirrors the `Type?` shape from §6.15.
//! - [`parse_requires_line`] — parse one canonical body line
//!   (`clipboard: Clipboard`, `network: Network?`, multi-comma
//!   shape `a: A, b: B`) into a list of capability rows.
//! - [`CapabilityRegistry`] — name → JSON instance map (clone-
//!   cheap `Arc<HashMap>`); installed on [`LowerScope`] via
//!   `with_capability` / `with_capability_registry`.
//! - [`harvest_requires`] — pull every body `<requires names="…"/>`
//!   off a [`ComponentDef`] / [`MixinDef`] and project to
//!   `Vec<CapabilityDef>`.
//! - [`resolve_capabilities`] — apply a component's required set
//!   against the active scope's registry and return a list of
//!   `(name, value)` bindings the caller threads into the
//!   instantiation scope (plus a list of missing required names
//!   for diagnostics).
//!
//! ## Capability values
//!
//! Phase 12 holds capability instances as [`serde_json::Value`]
//! — the same shape the binding map already uses. A host installs
//! a clipboard as e.g. `Value::Object({ kind: "system", id: "…" })`,
//! and the component's `clipboard.<method>` calls resolve through
//! the existing dotted-path expression lookup. Richer trait-shaped
//! capability dispatch (real Rust trait objects) is a follow-up;
//! the JSON model is enough to round-trip handles, identifiers,
//! and configuration through to handler code.

use std::collections::HashMap;
use std::sync::Arc;

use prism_core::language::prism_ui::{
    AttributeNamespace, AttributeValue, Element, Node as AstNode,
};

/// One declared `requires name: Type[?]` row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityDef {
    pub name: String,
    /// Declared type name as a raw string — `Clipboard`,
    /// `Network`, `FileSystem`, … Empty when the author wrote
    /// `requires foo` without a type (which the parser tolerates
    /// — the cap remains a pure name binding).
    pub ty: String,
    /// `?` suffix on the type — `network: Network?`. Optional
    /// capabilities don't trigger the missing-required diagnostic.
    pub optional: bool,
}

/// The active host-provided capability table. Cloning is cheap
/// (`Arc<HashMap>`); the inner map is built up via
/// [`LowerScope::with_capability`].
#[derive(Debug, Clone, Default)]
pub struct CapabilityRegistry {
    caps: Arc<HashMap<String, serde_json::Value>>,
}

impl CapabilityRegistry {
    pub fn empty() -> Self {
        Self::default()
    }

    /// Install `name → value`, returning a fresh registry. The
    /// existing entries are preserved; a colliding `name` is
    /// replaced (last-write-wins — tests routinely override a
    /// system capability with a mock).
    pub fn with_capability(mut self, name: impl Into<String>, value: serde_json::Value) -> Self {
        let mut map = (*self.caps).clone();
        map.insert(name.into(), value);
        self.caps = Arc::new(map);
        self
    }

    /// Look up a capability by name.
    pub fn get(&self, name: &str) -> Option<&serde_json::Value> {
        self.caps.get(name)
    }

    pub fn len(&self) -> usize {
        self.caps.len()
    }

    pub fn is_empty(&self) -> bool {
        self.caps.is_empty()
    }

    /// Phase 17 — every registered capability name. Used by the
    /// `.prui` linter to decide whether a `requires` declaration has
    /// a binding in the active scope.
    pub fn keys(&self) -> std::collections::HashSet<String> {
        self.caps.keys().cloned().collect()
    }
}

/// Parse a single `requires` body line into one or more
/// [`CapabilityDef`] rows. The canonical parser writes the full
/// line into the `names` attribute of a `<requires>` element, so
/// this function accepts the raw text and splits on top-level
/// commas (records / generics inside angle brackets are preserved
/// via depth tracking).
///
/// Shape:
///
/// ```text
/// requires clipboard: Clipboard
/// requires network:   Network?
/// requires a: A, b: B?
/// ```
pub fn parse_requires_line(line: &str) -> Vec<CapabilityDef> {
    let mut out = Vec::new();
    for raw in split_top_level_commas(line) {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        let (name, ty_raw) = match trimmed.split_once(':') {
            Some((n, t)) => (n.trim().to_string(), t.trim().to_string()),
            None => (trimmed.to_string(), String::new()),
        };
        if name.is_empty() {
            continue;
        }
        let optional = ty_raw.ends_with('?');
        let ty = if optional {
            ty_raw.trim_end_matches('?').trim().to_string()
        } else {
            ty_raw
        };
        out.push(CapabilityDef { name, ty, optional });
    }
    out
}

fn split_top_level_commas(line: &str) -> Vec<&str> {
    let mut depth: i32 = 0;
    let mut start = 0usize;
    let mut out = Vec::new();
    for (i, c) in line.char_indices() {
        match c {
            '<' | '(' | '[' | '{' => depth += 1,
            '>' | ')' | ']' | '}' => {
                if depth > 0 {
                    depth -= 1;
                }
            }
            ',' if depth == 0 => {
                out.push(&line[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    if start < line.len() {
        out.push(&line[start..]);
    } else if start == line.len() && !line.is_empty() && line.ends_with(',') {
        // trailing comma — skip
    }
    out
}

/// Harvest every body `<requires names="…"/>` element off a slice
/// of AST body nodes (a component / mixin's body). Returns the
/// flat list of declared capabilities, in declaration order.
pub fn harvest_requires(body: &[AstNode]) -> Vec<CapabilityDef> {
    let mut out = Vec::new();
    for node in body {
        let AstNode::Element(el) = node else {
            continue;
        };
        if el.tag != "requires" {
            continue;
        }
        let Some(line) = bare_string_attr(el, "names") else {
            continue;
        };
        out.extend(parse_requires_line(&line));
    }
    out
}

fn bare_string_attr(el: &Element, name: &str) -> Option<String> {
    el.attributes
        .iter()
        .find(|a| matches!(a.name.namespace, AttributeNamespace::Bare) && a.name.local == name)
        .and_then(|a| match &a.value {
            AttributeValue::String { value, .. } => Some(value.clone()),
            _ => None,
        })
}

/// Resolve every declared capability against the active registry.
/// Returns:
///
/// 1. The list of `(name, value)` pairs the caller should bind
///    into the instantiation scope (declared caps that were
///    found in the registry). Optional caps that weren't found
///    are bound as [`serde_json::Value::Null`] so a body
///    `{network?.post(…)}` reads a present-but-null binding.
/// 2. The list of *missing required* capability names — the
///    caller can surface these as a render-side diagnostic.
pub fn resolve_capabilities(
    declared: &[CapabilityDef],
    registry: &CapabilityRegistry,
) -> (Vec<(String, serde_json::Value)>, Vec<String>) {
    let mut bindings: Vec<(String, serde_json::Value)> = Vec::with_capacity(declared.len());
    let mut missing: Vec<String> = Vec::new();
    for cap in declared {
        match registry.get(&cap.name) {
            Some(value) => bindings.push((cap.name.clone(), value.clone())),
            None if cap.optional => {
                bindings.push((cap.name.clone(), serde_json::Value::Null));
            }
            None => missing.push(cap.name.clone()),
        }
    }
    (bindings, missing)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_single_required_line() {
        let caps = parse_requires_line("clipboard: Clipboard");
        assert_eq!(caps.len(), 1);
        assert_eq!(caps[0].name, "clipboard");
        assert_eq!(caps[0].ty, "Clipboard");
        assert!(!caps[0].optional);
    }

    #[test]
    fn parses_optional_marker() {
        let caps = parse_requires_line("network: Network?");
        assert_eq!(caps.len(), 1);
        assert_eq!(caps[0].name, "network");
        assert_eq!(caps[0].ty, "Network");
        assert!(caps[0].optional);
    }

    #[test]
    fn parses_multi_comma_line() {
        let caps = parse_requires_line("a: A, b: B?, c: C");
        assert_eq!(caps.len(), 3);
        assert_eq!(caps[0].name, "a");
        assert!(!caps[0].optional);
        assert_eq!(caps[1].name, "b");
        assert!(caps[1].optional);
        assert_eq!(caps[2].name, "c");
    }

    #[test]
    fn parses_bare_name_without_type() {
        let caps = parse_requires_line("foo");
        assert_eq!(caps.len(), 1);
        assert_eq!(caps[0].name, "foo");
        assert!(caps[0].ty.is_empty());
        assert!(!caps[0].optional);
    }

    #[test]
    fn split_top_level_commas_respects_brackets() {
        let parts = split_top_level_commas("a: A<X, Y>, b: B");
        // Commas inside `<…>` don't split; the outer `,` between
        // `>` and `b` does.
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0].trim(), "a: A<X, Y>");
        assert_eq!(parts[1].trim(), "b: B");
    }

    #[test]
    fn registry_install_and_lookup() {
        let r = CapabilityRegistry::empty().with_capability("clipboard", json!({ "kind": "test" }));
        assert_eq!(r.len(), 1);
        let v = r.get("clipboard").unwrap();
        assert_eq!(v["kind"], "test");
    }

    #[test]
    fn registry_shadows_last_write() {
        let r = CapabilityRegistry::empty()
            .with_capability("clipboard", json!("first"))
            .with_capability("clipboard", json!("second"));
        assert_eq!(r.get("clipboard").unwrap(), &json!("second"));
    }

    #[test]
    fn resolve_returns_missing_required() {
        let declared = vec![
            CapabilityDef {
                name: "clipboard".into(),
                ty: "Clipboard".into(),
                optional: false,
            },
            CapabilityDef {
                name: "network".into(),
                ty: "Network".into(),
                optional: true,
            },
        ];
        let registry = CapabilityRegistry::empty();
        let (bindings, missing) = resolve_capabilities(&declared, &registry);
        // Missing required → diagnostic; optional missing → bound
        // as Null.
        assert_eq!(missing, vec!["clipboard".to_string()]);
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].0, "network");
        assert_eq!(bindings[0].1, serde_json::Value::Null);
    }

    #[test]
    fn resolve_binds_present_capabilities() {
        let declared = vec![CapabilityDef {
            name: "clipboard".into(),
            ty: "Clipboard".into(),
            optional: false,
        }];
        let registry =
            CapabilityRegistry::empty().with_capability("clipboard", json!({ "kind": "system" }));
        let (bindings, missing) = resolve_capabilities(&declared, &registry);
        assert!(missing.is_empty());
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].0, "clipboard");
        assert_eq!(bindings[0].1, json!({ "kind": "system" }));
    }

    #[test]
    fn harvest_requires_walks_body_elements() {
        use prism_core::language::prism_ui::parse;
        let (doc, _) = parse(
            r#"component ShareButton(text: string) = {
  requires clipboard: Clipboard
  requires network: Network?
  <button/>
}"#,
        );
        // Drill into the component element's body.
        let AstNode::Element(comp) = &doc.nodes[0] else {
            panic!();
        };
        let caps = harvest_requires(&comp.children);
        assert_eq!(caps.len(), 2);
        assert_eq!(caps[0].name, "clipboard");
        assert!(!caps[0].optional);
        assert_eq!(caps[1].name, "network");
        assert!(caps[1].optional);
    }

    #[test]
    fn harvest_skips_empty_body() {
        let caps = harvest_requires(&[]);
        assert!(caps.is_empty());
    }
}
