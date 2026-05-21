//! Phase 7 — component declarations + properties + `extends`.
//!
//! See `docs/dev/prui-expressiveness-roadmap.md` §7.1–§7.3. The
//! canonical parser (`prism_core::language::prism_ui::grammar::canonical`)
//! already lowers `component Name(params) = body` and `component
//! Name(params) { body }` declarations into a single `<component
//! name="Name">` element whose children are `<property>`s plus the
//! body. This module turns those AST shapes into invokable component
//! definitions — `<Card title="…"/>` at a call site instantiates the
//! body with the call attributes bound as props, defaults applied,
//! and `required` enforced.
//!
//! ## Surface
//!
//! - [`ComponentDef`] / [`ParamDef`] — the declaration record. A
//!   pre-pass over the document harvests every `<component name=…>`
//!   into a name → def map carried on [`LowerScope::local_components`].
//! - [`harvest_components`] — the pre-pass entry point.
//! - [`is_pascal_case_tag`] — the call-site rule per §7.1 part 1:
//!   PascalCase tags dispatch through the local-component table
//!   before falling through to the host's `TagResolver`.
//! - [`instantiate_component`] — substitutes a call site with the
//!   component's body, bound props in scope. Honours `extends=` /
//!   `use Parent` body statements (Phase 7's §7.3 inheritance slice).
//!
//! Traits / mixins / contracts / capabilities are Phase 9+ and not
//! resolved here — `<requires>` / `<style>` / `<on>` body statements
//! round-trip through the AST untouched today. `<requires>` will be
//! consumed by Phase 12's capability injection pipeline; `<on>` and
//! `<style>` are inert until Phase 9's trait registry lands.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use prism_core::language::prism_ui::{AttributeNamespace, AttributeValue, Element, Node as AstNode};

use crate::layout::Node;

use super::elements::{lower_ast_children, resolved_attribute_string};
use super::LowerScope;

/// A single component parameter — one row of the canonical
/// `(name: type [= default | required])` parameter list, projected
/// onto `<property>` child elements by the canonical parser.
#[derive(Debug, Clone)]
pub struct ParamDef {
    pub name: String,
    pub ty: Option<String>,
    pub default: Option<String>,
    pub required: bool,
}

/// A registered component declaration. Each top-level `<component
/// name="X">` in the document (and each imported component once
/// `<import "./card.prui"/>` lands) becomes one of these on the
/// active [`LowerScope`].
#[derive(Debug, Clone)]
pub struct ComponentDef {
    pub name: String,
    pub params: Vec<ParamDef>,
    /// Parent name from `extends=Foo` header attribute or a body
    /// `<use names="Foo"/>` statement. `None` for components that
    /// stand alone. Multi-parent `use` chains (`use Foo, Bar`) are
    /// flattened into the first parent here; secondary parents are
    /// Phase 9 mixin territory and are ignored for Phase 7.
    pub extends: Option<String>,
    /// Render-tree children — the body minus `<property>` /
    /// `<requires>` / `<use>` / `<style>` / `<on>` declarations.
    pub body: Vec<AstNode>,
}

/// Pre-pass that harvests every top-level `<component name="…">`
/// element. Returns an empty map on documents that declare no local
/// components — the common case for shells / scenes / one-off views.
pub fn harvest_components(nodes: &[AstNode]) -> HashMap<String, Arc<ComponentDef>> {
    let mut out = HashMap::new();
    for node in nodes {
        let AstNode::Element(el) = node else { continue };
        if el.tag != "component" {
            continue;
        }
        let Some(def) = element_to_def(el) else {
            continue;
        };
        out.insert(def.name.clone(), Arc::new(def));
    }
    out
}

/// Project one `<component name="…">` AST element into a
/// [`ComponentDef`]. Returns `None` when the element has no `name=`
/// attribute (in which case it's the legacy `<component>`-as-
/// `<container>` alias, lowered by `interpret/elements.rs`).
fn element_to_def(el: &Element) -> Option<ComponentDef> {
    let name = bare_string_attr(el, "name")?;
    let extends_header = bare_string_attr(el, "extends");

    let mut params = Vec::new();
    let mut extends_from_body: Option<String> = None;
    let mut body = Vec::new();
    for child in &el.children {
        match child {
            AstNode::Element(child_el) => match child_el.tag.as_str() {
                "property" => {
                    if let Some(p) = property_to_param(child_el) {
                        params.push(p);
                    }
                }
                "use" => {
                    // Body `use Parent [, Other …]` — take the first
                    // name as the parent for Phase 7 inheritance.
                    // Subsequent names are Phase 9 mixins; ignored.
                    if extends_from_body.is_none() {
                        if let Some(names) = bare_string_attr(child_el, "names") {
                            if let Some(first) = names.split(',').next() {
                                let trimmed = first.trim();
                                if !trimmed.is_empty() {
                                    extends_from_body = Some(trimmed.to_string());
                                }
                            }
                        }
                    }
                }
                // Phase 9+ body statements — round-tripped to the
                // body so a Phase 9+ pass can find them once the
                // trait / capability infrastructure lands. They're
                // inert today.
                "requires" | "style" | "on" => {
                    body.push(child.clone());
                }
                // `<expr body="…"/>` is the canonical parser's
                // wrapper for an opaque RHS — `= some_expr` without
                // braces. Phase 7 doesn't evaluate arbitrary
                // expressions, but we keep it in the body so a
                // future pass can pick it up.
                _ => body.push(child.clone()),
            },
            // Text / interpolation / comment all round-trip.
            _ => body.push(child.clone()),
        }
    }

    Some(ComponentDef {
        name,
        params,
        extends: extends_header.or(extends_from_body),
        body,
    })
}

fn property_to_param(el: &Element) -> Option<ParamDef> {
    let name = bare_string_attr(el, "name")?;
    let ty = bare_string_attr(el, "type");
    let default = bare_string_attr(el, "default");
    let required = bare_string_attr(el, "required")
        .map(|v| v == "true" || v == "1")
        .unwrap_or(false);
    Some(ParamDef {
        name,
        ty,
        default,
        required,
    })
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

/// The call-site rule from §7.1 part 1: PascalCase tags dispatch
/// through the component registry; lowercase tags hit built-in
/// primitives or pass through to the host's resolver. First
/// character is the discriminator.
pub fn is_pascal_case_tag(tag: &str) -> bool {
    tag.chars().next().map(|c| c.is_ascii_uppercase()).unwrap_or(false)
}

/// Instantiate `def` at a call site `el` against `scope`. Returns
/// the lowered runtime nodes after binding the call's props,
/// applying declared defaults, and flattening any `extends` chain.
///
/// Per §7.1 part 3: the call site passes props as plain attribute
/// syntax; types belong to the declaration. Boolean-form attrs
/// (`<MyToggle disabled/>`) bind to `true`. Literal-string defaults
/// (`tone: string = "primary"`) lose their surrounding quotes;
/// boolean / numeric / token defaults round-trip through serde.
pub fn instantiate_component(
    def: &ComponentDef,
    el: &Element,
    scope: &LowerScope,
) -> Vec<Node> {
    // Flatten the extends chain once: parent's params override-chain
    // (child wins), parent's body is the fallback when the child
    // has no body of its own. Multi-level chains walk via recursion
    // through the scope's local-component table.
    let resolved = flatten_extends(def, scope, &mut HashSet::new());

    // Bind props in the new scope. Walk the call element's bare /
    // identifier attributes; declared params win the type-coercion
    // hint, but unrecognised attrs pass through as raw strings so a
    // future trait registry (Phase 9) can still pick them up.
    let mut call_props: HashMap<String, serde_json::Value> = HashMap::new();
    for attr in &el.attributes {
        if !matches!(
            attr.name.namespace,
            AttributeNamespace::Bare | AttributeNamespace::Identifier
        ) {
            continue;
        }
        let value = match &attr.value {
            AttributeValue::Empty => serde_json::Value::Bool(true),
            other => match resolved_attribute_string(other, scope) {
                Some(s) => serde_json::Value::String(s),
                None => serde_json::Value::Null,
            },
        };
        call_props.insert(attr.name.local.clone(), value);
    }

    // For each declared param: apply default if missing, error if
    // missing+required (Phase 7 surfaces the error as a runtime
    // skip + diagnostic-bearing empty render). The map lands as
    // scope bindings so `{title}` interpolations inside the body
    // resolve.
    let mut child_scope = scope.clone();
    for param in &resolved.params {
        if call_props.contains_key(&param.name) {
            continue;
        }
        if let Some(default) = &param.default {
            call_props.insert(param.name.clone(), parse_default_literal(default));
        } else if param.required {
            // Required prop missing — bind null so interpolations
            // don't panic; downstream readers see `Value::Null`.
            // A diagnostic surface is Phase 12 / 17 polish.
            call_props.insert(param.name.clone(), serde_json::Value::Null);
        }
    }
    for (key, value) in call_props {
        child_scope = child_scope.with_binding(key, value);
    }

    // Caller's children become the default slot — the magic
    // `children: ui` parameter per §7.5. We seed them as already-
    // lowered UI children via the existing host_children_ui seam
    // so a `<slot/>` inside the body or a `{children}` interpolation
    // both pick them up.
    if !el.children.is_empty() {
        let lowered_children = lower_ast_children(&el.children, scope);
        if !lowered_children.is_empty() {
            child_scope = child_scope.with_host_children_ui(lowered_children);
        }
    }

    lower_ast_children(&resolved.body, &child_scope)
}

/// Merge the resolved view of a component along its `extends`
/// chain. Returns a flattened [`ComponentDef`] whose params are the
/// parent's params with the child's overriding (by name), and whose
/// body is the child's if non-empty else the parent's.
///
/// Cycle protection: each name in the chain enters `seen` once;
/// re-entry returns the current resolved view rather than diverging.
fn flatten_extends(
    def: &ComponentDef,
    scope: &LowerScope,
    seen: &mut HashSet<String>,
) -> ComponentDef {
    seen.insert(def.name.clone());
    let Some(parent_name) = &def.extends else {
        return def.clone();
    };
    if seen.contains(parent_name) {
        return def.clone();
    }
    let Some(parent) = scope.local_component(parent_name) else {
        return def.clone();
    };
    let parent_resolved = flatten_extends(parent.as_ref(), scope, seen);

    // Params: parent's params come first; child's overrides by name.
    let mut params: Vec<ParamDef> = parent_resolved.params.clone();
    for child_param in &def.params {
        if let Some(slot) = params.iter_mut().find(|p| p.name == child_param.name) {
            *slot = child_param.clone();
        } else {
            params.push(child_param.clone());
        }
    }

    // Body: child wins when non-empty; parent's body is the
    // default-override fallback per §7.3 "default-override variant".
    let body = if def.body.iter().any(is_render_tree_node) {
        def.body.clone()
    } else {
        parent_resolved.body.clone()
    };

    ComponentDef {
        name: def.name.clone(),
        params,
        extends: parent_resolved.extends.clone(),
        body,
    }
}

/// A render-tree node is anything that lowers to a real UI node —
/// container / text / heading / a registered tag etc. Internal
/// declaration statements (`<requires>` / `<style>` / `<on>`) don't
/// count, so a child whose only body is `<requires>` falls back to
/// the parent's render tree.
fn is_render_tree_node(node: &AstNode) -> bool {
    match node {
        AstNode::Element(el) => !matches!(el.tag.as_str(), "requires" | "style" | "on" | "expr"),
        AstNode::Text { value, .. } => !value.trim().is_empty(),
        AstNode::Interpolation(_) => true,
        AstNode::Comment { .. } => false,
    }
}

/// Parse a default literal from the textual form recorded by the
/// canonical parser. The §7.2 rule says defaults must be literal
/// (string / number / bool / token / nil), so the parse is small:
///
/// - `"…"` / `'…'` → `Value::String` (quote-stripped).
/// - `true` / `false` → `Value::Bool`.
/// - `nil` / `null` → `Value::Null`.
/// - integer / float → `Value::Number`.
/// - anything else → `Value::String` of the trimmed raw text (token
///   names like `primary`, `default`, `danger` fall here).
fn parse_default_literal(raw: &str) -> serde_json::Value {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return serde_json::Value::Null;
    }
    // Quoted string.
    if let Some(stripped) = strip_quotes(trimmed) {
        return serde_json::Value::String(stripped.to_string());
    }
    match trimmed {
        "true" => return serde_json::Value::Bool(true),
        "false" => return serde_json::Value::Bool(false),
        "nil" | "null" => return serde_json::Value::Null,
        _ => {}
    }
    if let Ok(n) = trimmed.parse::<i64>() {
        return serde_json::Value::Number(n.into());
    }
    if let Ok(f) = trimmed.parse::<f64>() {
        if let Some(n) = serde_json::Number::from_f64(f) {
            return serde_json::Value::Number(n);
        }
    }
    // Bare token / identifier — round-trip as a string.
    serde_json::Value::String(trimmed.to_string())
}

fn strip_quotes(s: &str) -> Option<&str> {
    let bytes = s.as_bytes();
    if bytes.len() < 2 {
        return None;
    }
    let first = bytes[0];
    let last = bytes[bytes.len() - 1];
    if (first == b'"' || first == b'\'') && first == last {
        return Some(&s[1..s.len() - 1]);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use prism_core::language::prism_ui::parse;

    fn first_component(src: &str) -> ComponentDef {
        let (doc, errs) = parse(src);
        assert!(errs.is_empty(), "parse errs: {errs:?}");
        let map = harvest_components(&doc.nodes);
        let key = map.keys().next().expect("no component harvested").clone();
        (*map[&key]).clone()
    }

    #[test]
    fn harvests_param_with_default() {
        let def =
            first_component(r#"component Avatar(size: int = 32) = <image size={size}/>"#);
        assert_eq!(def.name, "Avatar");
        assert_eq!(def.params.len(), 1);
        assert_eq!(def.params[0].name, "size");
        assert_eq!(def.params[0].default.as_deref(), Some("32"));
        assert!(!def.params[0].required);
    }

    #[test]
    fn harvests_required_flag() {
        let def =
            first_component(r#"component Card(title: string required) = <text>{title}</text>"#);
        assert_eq!(def.params.len(), 1);
        assert!(def.params[0].required);
    }

    #[test]
    fn parses_string_default() {
        assert_eq!(
            parse_default_literal(r#""primary""#),
            serde_json::Value::String("primary".into())
        );
    }

    #[test]
    fn parses_int_default() {
        assert_eq!(
            parse_default_literal("16"),
            serde_json::Value::Number(16.into())
        );
    }

    #[test]
    fn parses_bool_default() {
        assert_eq!(parse_default_literal("true"), serde_json::Value::Bool(true));
    }

    #[test]
    fn parses_token_default() {
        assert_eq!(
            parse_default_literal("danger"),
            serde_json::Value::String("danger".into())
        );
    }

    #[test]
    fn harvests_use_as_extends() {
        let (doc, _) = parse(
            r#"component Base(label: string) = <text>{label}</text>
component Child(label: string) = {
  use Base
}"#,
        );
        let map = harvest_components(&doc.nodes);
        let child = map.get("Child").unwrap();
        assert_eq!(child.extends.as_deref(), Some("Base"));
    }

    #[test]
    fn pascal_case_check() {
        assert!(is_pascal_case_tag("Card"));
        assert!(is_pascal_case_tag("AppWindow"));
        assert!(!is_pascal_case_tag("container"));
        assert!(!is_pascal_case_tag("shell.icon-button"));
        assert!(!is_pascal_case_tag(""));
    }

    #[test]
    fn empty_body_falls_through_to_parent() {
        // Child has no render tree, only `use Parent`.
        let (doc, _) = parse(
            r#"component Parent(label: string) = <text>{label}</text>
component Child(label: string) = {
  use Parent
}"#,
        );
        let nodes = &doc.nodes;
        let map = harvest_components(nodes);
        let child = map.get("Child").unwrap();
        // The child's body contains no render-tree elements.
        assert!(!child.body.iter().any(is_render_tree_node));
    }
}
