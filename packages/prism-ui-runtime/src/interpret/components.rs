//! Phase 7–9 — component / trait declarations, file namespaces,
//! and the `<import component="…"/>` projection.
//!
//! See `docs/dev/prui-expressiveness-roadmap.md` §7.1–§7.4 and §7.11.
//! The canonical parser already projects every declaration onto a
//! single AST element whose `tag` is the keyword and whose header
//! data lives on attributes:
//!
//! - `<component name="Card">` with `<property>` children + body.
//! - `<trait name="Focusable">` with body children (empty body = a
//!   pure contract; `<trait Marker {}/>` is the trivial case).
//! - `<namespace name="Forms"/>` — a leading file-scope directive
//!   that prefixes every component / trait declared in this file
//!   (§7.11 C# rule).
//!
//! ## Surface
//!
//! - [`ComponentDef`] / [`ParamDef`] / [`TraitDef`] — the declaration
//!   records. A pre-pass over the document harvests every
//!   `<component name=…>` / `<trait name=…>` element into name →
//!   def maps carried on [`LowerScope::local_components`] /
//!   [`LowerScope::local_traits`].
//! - [`harvest_declarations`] — the unified pre-pass; replaces the
//!   Phase 7 `harvest_components`-only entry point. Returns components
//!   *and* traits in one walk so both share the namespace prefix logic.
//!   The legacy [`harvest_components`] alias is retained for call
//!   sites that only need the component half.
//! - [`is_pascal_case_tag`] — the call-site rule per §7.1 part 1:
//!   PascalCase (and PascalCase-dotted, like `Forms.TextField`) tags
//!   dispatch through the local-component table before falling
//!   through to the host's `TagResolver`.
//! - [`instantiate_component`] — substitutes a call site with the
//!   component's body, bound props in scope. Honours `extends=` /
//!   `use Parent` body statements (Phase 7's §7.3 inheritance slice).
//!
//! Mixins / capabilities are Phase 10+ and not resolved here —
//! `<requires>` / `<style>` / `<on>` body statements round-trip
//! through the AST untouched today. `<requires>` will be consumed by
//! Phase 12's capability injection pipeline; `<on>` and `<style>` are
//! inert until Phase 10's mixin pipeline lands.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use prism_core::language::prism_ui::{
    AttributeNamespace, AttributeValue, Element, Node as AstNode,
};

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
    /// Phase 8 — `: Trait, Trait` conformance list. Parsed by the
    /// canonical reader into the `impls="Focusable, Pointable"`
    /// header attribute; we project it onto a typed `Vec` here so
    /// downstream passes (the trait registry resolution and the
    /// LSP) don't have to re-split. Empty when the component
    /// conforms to no traits (the common case).
    pub impls: Vec<String>,
    /// Render-tree children — the body minus `<property>` /
    /// `<requires>` / `<use>` / `<style>` / `<on>` declarations.
    pub body: Vec<AstNode>,
}

/// Phase 8 — a registered trait declaration. Each top-level
/// `<trait name="X">` (and each trait registered through the
/// `<import component="./*.prui"/>` projection) lives in the
/// scope's `local_traits` table. A trait with an empty member list
/// *is* the §7.3 "contract" — Marker-shape, used only for slot-type
/// bounds. Non-empty `members` describe the typed shape conformers
/// must provide.
#[derive(Debug, Clone)]
pub struct TraitDef {
    pub name: String,
    /// The typed members the trait declares — projected from each
    /// `<property name="…" type="…">` child the canonical parser
    /// emitted for `name: type` body lines. Empty list = contract.
    pub members: Vec<ParamDef>,
    /// `recursive` flag (Q5) — set when the canonical header
    /// carried the `recursive` keyword. Trait self-reference is
    /// only legal when this is true; Phase 8 records it so a Phase 9
    /// type-checker can enforce it. No runtime effect today.
    pub recursive: bool,
}

/// Phase 8 unified harvester output — every component + trait
/// declaration in a single document or imported file, with the
/// file's leading `<namespace name="X"/>` directive already
/// applied to each name as a `Ns.Card`-shape prefix per §7.11.
#[derive(Debug, Clone, Default)]
pub struct HarvestedDeclarations {
    pub components: HashMap<String, Arc<ComponentDef>>,
    pub traits: HashMap<String, Arc<TraitDef>>,
}

impl HarvestedDeclarations {
    /// Merge `other` into `self` with the same C# / collision rule as
    /// §7.11 / Q1: duplicate bare names are skipped (last-wins is
    /// rejected — but at the runtime layer we can't usefully surface
    /// the parse error from a downstream pass, so we degrade to a
    /// silent first-wins). The merge runs after both files have had
    /// their own `<namespace>` prefix applied, so a collision here
    /// means two files declared the same bare component.
    pub fn merge(&mut self, other: HarvestedDeclarations) {
        for (k, v) in other.components {
            self.components.entry(k).or_insert(v);
        }
        for (k, v) in other.traits {
            self.traits.entry(k).or_insert(v);
        }
    }
}

/// Phase 8 — unified pre-pass that harvests every top-level
/// `<component name="…">` and `<trait name="…">` element in one
/// walk, applying the file's leading `<namespace name="Ns"/>`
/// directive (or an external `alias` override) as a `Ns.<Name>`
/// prefix per §7.11. The dotted form is what `<Ns.Card/>` resolves
/// against at the call site.
pub fn harvest_declarations(nodes: &[AstNode], alias: Option<&str>) -> HarvestedDeclarations {
    // §7.11 — `<namespace name="X"/>` at file scope wins unless the
    // importer overrides with `as=`. `alias` carries the override.
    let file_namespace = nodes.iter().find_map(|n| match n {
        AstNode::Element(el) if el.tag == "namespace" => bare_string_attr(el, "name"),
        _ => None,
    });
    let prefix = alias
        .map(|s| s.to_string())
        .or(file_namespace)
        .filter(|s| !s.is_empty());

    let mut out = HarvestedDeclarations::default();
    for node in nodes {
        let AstNode::Element(el) = node else { continue };
        match el.tag.as_str() {
            "component" => {
                let Some(mut def) = element_to_def(el) else {
                    continue;
                };
                if let Some(p) = &prefix {
                    def.name = format!("{p}.{}", def.name);
                }
                out.components.insert(def.name.clone(), Arc::new(def));
            }
            "trait" => {
                let Some(mut def) = element_to_trait(el) else {
                    continue;
                };
                if let Some(p) = &prefix {
                    def.name = format!("{p}.{}", def.name);
                }
                out.traits.insert(def.name.clone(), Arc::new(def));
            }
            _ => {}
        }
    }
    out
}

/// Phase 7 back-compat wrapper — the unified [`harvest_declarations`]
/// is the new entry point, but call sites that only care about the
/// component half still get the lean Map<name, def> shape they had.
/// No `<namespace>` prefix is applied here (the unified entry runs
/// for documents that need it).
pub fn harvest_components(nodes: &[AstNode]) -> HashMap<String, Arc<ComponentDef>> {
    harvest_declarations(nodes, None).components
}

/// Project a `<trait name="X">` AST element into a [`TraitDef`].
/// Returns `None` when the element has no `name=` attribute.
///
/// Trait body lines (`focus: action`, `is-hovered: bool = false`)
/// arrive in two shapes from the canonical reader:
///
/// 1. `<property name="focus" type="action"/>` — when the keyword
///    `property` was explicitly written (a forward-compat shape).
/// 2. `<expr body="focus: action"/>` — the common case, since
///    trait member lines don't lead with a body-keyword and the
///    block-body parser falls back to the opaque-expression slot.
///
/// We parse both into [`ParamDef`] rows so the trait's typed
/// members surface uniformly downstream regardless of the parser's
/// internal projection.
fn element_to_trait(el: &Element) -> Option<TraitDef> {
    let name = bare_string_attr(el, "name")?;
    let recursive = bare_string_attr(el, "recursive")
        .map(|v| v == "true" || v == "1" || v.is_empty())
        .unwrap_or(false);
    let mut members = Vec::new();
    for child in &el.children {
        let AstNode::Element(child_el) = child else {
            continue;
        };
        match child_el.tag.as_str() {
            "property" => {
                if let Some(p) = property_to_param(child_el) {
                    members.push(p);
                }
            }
            "expr" => {
                if let Some(body) = bare_string_attr(child_el, "body") {
                    if let Some(p) = parse_trait_member_line(&body) {
                        members.push(p);
                    }
                }
            }
            _ => {}
        }
    }
    Some(TraitDef {
        name,
        members,
        recursive,
    })
}

/// Parse a single trait body line into a [`ParamDef`]. Accepts the
/// canonical `name: type` and `name: type = default` shapes (with an
/// optional trailing `required`). Lines that don't match `name:`
/// fall through as `None` — the caller skips them (parse-time
/// diagnostics belong to the canonical reader).
fn parse_trait_member_line(line: &str) -> Option<ParamDef> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    let (name, rest) = line.split_once(':')?;
    let name = name.trim().to_string();
    if name.is_empty() {
        return None;
    }
    let rest = rest.trim();
    // Pull off an optional `required` token (may sit before or
    // after `= default`).
    let (rest, required_a) = strip_required(rest);
    // Pull off an optional `= default`.
    let (ty, default) = if let Some(eq_idx) = rest.find('=') {
        let (ty, default) = rest.split_at(eq_idx);
        (ty.trim().to_string(), Some(default[1..].trim().to_string()))
    } else {
        (rest.to_string(), None)
    };
    let (ty, required_b) = strip_required(&ty);
    Some(ParamDef {
        name,
        ty: if ty.is_empty() {
            None
        } else {
            Some(ty.to_string())
        },
        default,
        required: required_a || required_b,
    })
}

fn strip_required(s: &str) -> (&str, bool) {
    let t = s.trim();
    if let Some(rest) = t.strip_suffix("required") {
        // Ensure `required` is a whole-word suffix, not e.g. the tail
        // of an identifier like `is-required`.
        let prefix = rest.trim_end();
        if prefix.len() < rest.len() {
            return (prefix, true);
        }
    }
    if let Some(rest) = t.strip_prefix("required") {
        let after = rest.trim_start();
        if after.len() < rest.len() {
            return (after, true);
        }
    }
    (t, false)
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

    // Phase 8 — `: Trait, Trait` lands on `impls="Focusable, Pointable"`
    // from the canonical parser (see `grammar/canonical.rs::parse_impl_list`).
    // Split on `,` and trim; empty stays an empty Vec.
    let impls = bare_string_attr(el, "impls")
        .map(|s| {
            s.split(',')
                .map(|t| t.trim().to_string())
                .filter(|t| !t.is_empty())
                .collect()
        })
        .unwrap_or_default();

    Some(ComponentDef {
        name,
        params,
        extends: extends_header.or(extends_from_body),
        impls,
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
    tag.chars()
        .next()
        .map(|c| c.is_ascii_uppercase())
        .unwrap_or(false)
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
pub fn instantiate_component(def: &ComponentDef, el: &Element, scope: &LowerScope) -> Vec<Node> {
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

    // Phase 8 — trait conformance flattens like params: parent first,
    // child's additions appended (deduped). The combined list is what
    // a future trait-registry pass walks to install the conformance.
    let mut impls: Vec<String> = parent_resolved.impls.clone();
    for t in &def.impls {
        if !impls.iter().any(|existing| existing == t) {
            impls.push(t.clone());
        }
    }

    ComponentDef {
        name: def.name.clone(),
        params,
        extends: parent_resolved.extends.clone(),
        impls,
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
        let def = first_component(r#"component Avatar(size: int = 32) = <image size={size}/>"#);
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
    fn harvests_trait_with_members() {
        let (doc, _) = parse(
            r#"trait Focusable {
  focus: action
  blur: action
}"#,
        );
        let out = harvest_declarations(&doc.nodes, None);
        let t = out.traits.get("Focusable").expect("trait missing");
        assert_eq!(t.members.len(), 2);
        assert_eq!(t.members[0].name, "focus");
        assert_eq!(t.members[1].name, "blur");
    }

    #[test]
    fn harvests_marker_contract_trait() {
        // §7.3 — a body-less trait *is* the contract form.
        let (doc, _) = parse("trait Marker");
        let out = harvest_declarations(&doc.nodes, None);
        let t = out.traits.get("Marker").expect("marker trait missing");
        assert!(t.members.is_empty(), "expected contract (no members)");
    }

    #[test]
    fn harvests_component_impls() {
        let (doc, _) =
            parse(r#"component TaskRow(task: Task) : Focusable, Pointable = <container/>"#);
        let out = harvest_declarations(&doc.nodes, None);
        let c = out.components.get("TaskRow").expect("component missing");
        assert_eq!(
            c.impls,
            vec!["Focusable".to_string(), "Pointable".to_string()]
        );
    }

    #[test]
    fn file_namespace_prefixes_declarations() {
        // §7.11 — `<namespace name="Forms"/>` at the head of a file
        // prefixes every component / trait in the file. The dotted
        // form is what a call site like `<Forms.TextField/>` resolves
        // against.
        let (doc, _) = parse(
            r#"namespace Forms

component TextField(value: string) = <input value={value}/>
component DropdownField(value: string) = <input value={value}/>
trait FieldLike { value: any }"#,
        );
        let out = harvest_declarations(&doc.nodes, None);
        assert!(out.components.contains_key("Forms.TextField"));
        assert!(out.components.contains_key("Forms.DropdownField"));
        assert!(out.traits.contains_key("Forms.FieldLike"));
        // The bare names should NOT be present — the prefix is the
        // only entry point.
        assert!(!out.components.contains_key("TextField"));
    }

    #[test]
    fn import_alias_overrides_file_namespace() {
        // §7.11 — `as=fields` on the import overrides the file's own
        // `<namespace=Forms/>` directive.
        let (doc, _) = parse(
            r#"namespace Forms
component TextField(value: string) = <input value={value}/>"#,
        );
        let out = harvest_declarations(&doc.nodes, Some("fields"));
        assert!(out.components.contains_key("fields.TextField"));
        assert!(!out.components.contains_key("Forms.TextField"));
    }

    #[test]
    fn merge_keeps_first_on_collision() {
        let mut a = HarvestedDeclarations::default();
        a.components.insert(
            "Card".into(),
            Arc::new(ComponentDef {
                name: "Card".into(),
                params: Vec::new(),
                extends: None,
                impls: Vec::new(),
                body: Vec::new(),
            }),
        );
        let mut b = HarvestedDeclarations::default();
        b.components.insert(
            "Card".into(),
            Arc::new(ComponentDef {
                name: "Card".into(),
                params: vec![ParamDef {
                    name: "shouldnt-show".into(),
                    ty: None,
                    default: None,
                    required: false,
                }],
                extends: None,
                impls: Vec::new(),
                body: Vec::new(),
            }),
        );
        a.merge(b);
        let merged = a.components.get("Card").unwrap();
        assert!(merged.params.is_empty(), "first wins on collision");
    }

    #[test]
    fn dotted_pascal_case_is_recognised() {
        // Phase 8 call-site dispatch: `<Forms.TextField/>` still
        // passes the PascalCase check because the first character
        // is uppercase. The dispatch path keys off the full dotted
        // tag string.
        assert!(is_pascal_case_tag("Forms.TextField"));
        assert!(is_pascal_case_tag("MyNs.SubNs.Field"));
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
