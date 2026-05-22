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
//! Phase 10 lands mixin splicing — see [`MixinDef`] and the
//! parse-time `use Mixin` / `derive=` paths inside
//! [`instantiate_component`]. Capabilities (`<requires>`) are still
//! Phase 12+ and round-trip through the AST untouched today.

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
    /// Phase 10 — `derive=[A, B]` / `derives=[A, B]` header
    /// attribute. Names listed here are spliced into the body
    /// at parse time (equivalent to a body `use A, B` whose
    /// names all resolve to mixins). Empty when no derive header
    /// was present; mixin bodies are looked up against the active
    /// [`LowerScope::local_mixins`] table at instantiation time.
    pub derives: Vec<String>,
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

/// Phase 10 — a registered mixin declaration. Each top-level
/// `<mixin name="X">` (and each mixin re-imported via
/// `<import component="…"/>`) lands here. A mixin is a behaviour
/// bundle — `<let>` / `<on>` / `<style>` body statements spliced
/// into whatever component `use`s it (parse-time) or runtime-
/// composed onto an element via the `with=[…]` attribute.
///
/// Phase 10's runtime stores the splicable body as a flat list of
/// AST nodes; semantics for reactive `<let>` state, event handler
/// chaining (`super()`), and scoped `<style>` blocks are layered on
/// in later phases. The Phase 10 splice is purely structural:
/// mixin bodies are inserted into the host component's body where
/// the `<use>` lived (or appended via `derive=` / `with=`).
#[derive(Debug, Clone)]
pub struct MixinDef {
    pub name: String,
    /// The mixin's body statements (`<let>`, `<on>`, `<style>`,
    /// `<requires>`, `<expr>`). Spliced verbatim into the host at
    /// `use` / `derive=` / `with=` sites. The mixin's `<property>`
    /// elements are projected into `params` (mixins don't typically
    /// declare params but the canonical parser allows them).
    pub body: Vec<AstNode>,
    /// Declared params, projected from the header `( … )` list.
    /// Empty for the common case.
    pub params: Vec<ParamDef>,
}

/// Phase 8 unified harvester output — every component + trait
/// declaration in a single document or imported file, with the
/// file's leading `<namespace name="X"/>` directive already
/// applied to each name as a `Ns.Card`-shape prefix per §7.11.
#[derive(Debug, Clone, Default)]
pub struct HarvestedDeclarations {
    pub components: HashMap<String, Arc<ComponentDef>>,
    pub traits: HashMap<String, Arc<TraitDef>>,
    /// Phase 10 — mixin table. Distinct from `components` so the
    /// PascalCase tag-dispatch path (which lives in
    /// `interpret/elements.rs::lower_element_body`) doesn't
    /// accidentally invoke a mixin as a component.
    pub mixins: HashMap<String, Arc<MixinDef>>,
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
        for (k, v) in other.mixins {
            self.mixins.entry(k).or_insert(v);
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
            "mixin" => {
                let Some(mut def) = element_to_mixin(el) else {
                    continue;
                };
                if let Some(p) = &prefix {
                    def.name = format!("{p}.{}", def.name);
                }
                out.mixins.insert(def.name.clone(), Arc::new(def));
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
                    // Phase 7 / Phase 10 — body `use Name [, Name …]`.
                    // The first name still records as Phase 7 extends
                    // (a parent component); the `<use>` element itself
                    // round-trips through the body so the Phase 10
                    // splicer can re-walk it at instantiation time and
                    // resolve each name against the active mixin /
                    // component tables on the scope. A `use` whose only
                    // name is a *mixin* contributes no `extends`; the
                    // splicer treats it as a mixin reference.
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
                    body.push(child.clone());
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

    // Phase 10 — `derive=` / `derives=` header attribute. Both
    // shapes accepted: `derive="A, B"`, `derive="[A, B]"`. Square
    // brackets and surrounding whitespace are trimmed before split.
    let derives = bare_string_attr(el, "derive")
        .or_else(|| bare_string_attr(el, "derives"))
        .map(|raw| parse_name_list(&raw))
        .unwrap_or_default();

    Some(ComponentDef {
        name,
        params,
        extends: extends_header.or(extends_from_body),
        impls,
        derives,
        body,
    })
}

/// Public alias of [`parse_name_list`] for `interpret/elements.rs`
/// (the `with=` attribute reader). Kept under a distinct name so
/// the private surface inside this module stays untouched.
pub fn parse_name_list_pub(raw: &str) -> Vec<String> {
    parse_name_list(raw)
}

/// Phase 10 — parse a comma-separated name list as it appears in
/// `derive="A, B"` / `with="A, B"` / `with="[A, B]"`. Strips one
/// pair of surrounding square brackets and trims whitespace around
/// each name; empty entries are skipped.
pub(crate) fn parse_name_list(raw: &str) -> Vec<String> {
    let trimmed = raw.trim();
    let trimmed = trimmed
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .unwrap_or(trimmed);
    trimmed
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Phase 10 — project one `<mixin name="…">` AST element into a
/// [`MixinDef`]. The body is the mixin's spliceable statements
/// (`<let>`, `<on>`, `<style>`, `<requires>`, `<expr>`, render-tree
/// elements). Header `( … )` params become [`ParamDef`] rows. The
/// element is dropped when its `name=` attribute is missing.
fn element_to_mixin(el: &Element) -> Option<MixinDef> {
    let name = bare_string_attr(el, "name")?;
    let mut params = Vec::new();
    let mut body = Vec::new();
    for child in &el.children {
        match child {
            AstNode::Element(child_el) if child_el.tag == "property" => {
                if let Some(p) = property_to_param(child_el) {
                    params.push(p);
                }
            }
            _ => body.push(child.clone()),
        }
    }
    Some(MixinDef { name, body, params })
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
    let mut resolved = flatten_extends(def, scope, &mut HashSet::new());

    // Phase 10 — splice mixins. The `derive=[…]` header and every
    // body `<use names="A, B">` whose names resolve to a mixin in
    // the scope's local-mixin table contribute their body to the
    // host component. Names that resolve to a component instead are
    // skipped here (the first such name became the `extends` parent
    // during harvest, and any subsequent component name in the same
    // `use` list is ignored — multi-parent inheritance is rejected
    // per §7.3). The splice is positional: a body `<use>` element is
    // replaced in-place by the spliced mixin bodies, so authoring
    // intent (e.g. a `<use>` between two `<let>`s) is preserved.
    resolved.body = splice_mixins(&resolved, scope);

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
        // Phase 13 — preserve the typed JSON shape when an attribute
        // value is an `{expression}`. The expression evaluator
        // surfaces typed objects (variant constructors land here as
        // `{tag: …, …}`); a plain string-coercing path would
        // stringify the variant and lose its tag field.
        let value = match &attr.value {
            AttributeValue::Empty => serde_json::Value::Bool(true),
            prism_core::language::prism_ui::AttributeValue::Expression(e) => {
                super::expression::lookup_path_owned_in_scope(&e.body, scope)
                    .or_else(|| super::expression::evaluate_expression(&e.body, scope))
                    .unwrap_or_else(|| {
                        resolved_attribute_string(&attr.value, scope)
                            .map(serde_json::Value::String)
                            .unwrap_or(serde_json::Value::Null)
                    })
            }
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

    // Phase 12 — capability resolution. Harvest every body
    // `<requires names="…"/>` element off the resolved body,
    // resolve against the active scope's `CapabilityRegistry`,
    // bind every present (or optional-missing) cap into the child
    // scope. Missing required caps lower as a leading text node
    // diagnostic so the author sees an actionable message rather
    // than a silent omission (parse-time hard-failure is Phase 17
    // lint territory). Components without any `<requires>` body
    // statements skip this entirely.
    let declared_caps = super::capabilities::harvest_requires(&resolved.body);
    let mut diagnostics: Vec<Node> = Vec::new();
    if !declared_caps.is_empty() {
        let (bindings, missing) =
            super::capabilities::resolve_capabilities(&declared_caps, scope.capability_registry());
        for (name, value) in bindings {
            child_scope = child_scope.with_binding(name, value);
        }
        for name in &missing {
            diagnostics.push(Node::Text {
                id: String::new(),
                content: format!(
                    "[prism] component `{}` requires capability `{name}` (not provided by host)",
                    resolved.name
                ),
                props: crate::layout::TextProps::default(),
            });
        }
    }

    // Phase 14 — partition the call site's children into named slots
    // and the default-slot bucket. Any child carrying a literal
    // `slot="X"` attribute lands in the named bucket under `X`;
    // siblings without `slot=` flow into the default slot. The §7.5
    // canonical form authors slot props as attributes, but the body-
    // shape lets a call site pass tree literals without coining a
    // new grammar (`<List items={tasks}><heading slot="header">My
    // Tasks</heading></List>`). The default bucket still feeds the
    // unnamed `<slot/>` via `host_children_ui` so the Phase-5 default-
    // slot collapse stays intact.
    if !el.children.is_empty() {
        let (default_children, named_buckets) = partition_call_site_slots(&el.children);
        if !default_children.is_empty() {
            let lowered_default = lower_ast_children(&default_children, scope);
            if !lowered_default.is_empty() {
                child_scope = child_scope.with_host_children_ui(lowered_default);
            }
        }
        if !named_buckets.is_empty() {
            let mut named_lowered: HashMap<String, Vec<crate::layout::Node>> = HashMap::new();
            // Stash the raw AST under named-slot scope bindings so an
            // `<invoke slot="X">` reader can re-lower the slot's body
            // with per-call arg bindings (the §7.5 render-prop form).
            // The pre-lowered map mirrors the AST so the simple
            // `<slot name="X"/>` consumer still gets cached children.
            let mut named_ast_param: HashMap<String, Vec<AstNode>> = HashMap::new();
            for (name, nodes) in named_buckets {
                let lowered = lower_ast_children(&nodes, scope);
                named_lowered.insert(name.clone(), lowered);
                named_ast_param.insert(name.clone(), nodes);
            }
            child_scope = child_scope.with_host_children_by_slot(Arc::new(named_lowered));
            child_scope = child_scope.with_named_slot_ast(Arc::new(named_ast_param));
        }
    }

    // Phase 14 — slot prop defaults. For every declared param whose
    // type is `ui` or a `|…| → ui` callable, when the call site
    // didn't provide a same-named slot binding, project the
    // declared default as the AST for that slot. The default's AST
    // lowers at consumer time (`<slot name="X"/>` or `<invoke>`) so
    // the per-call argument scope still applies.
    for param in &resolved.params {
        if !is_slot_type(param.ty.as_deref()) {
            continue;
        }
        if child_scope.has_named_slot_ast(&param.name) {
            continue;
        }
        let Some(default) = &param.default else {
            continue;
        };
        if let Some(ast) = parse_slot_default(default) {
            child_scope = child_scope.with_named_slot_ast_one(param.name.clone(), ast);
        }
    }

    // Prepend any cap-missing diagnostics so they're visible above
    // the (possibly broken) component body.
    let body_nodes = lower_ast_children(&resolved.body, &child_scope);
    if diagnostics.is_empty() {
        body_nodes
    } else {
        diagnostics.extend(body_nodes);
        diagnostics
    }
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

    // Phase 10 — derive list flattens the same way. The child's
    // derives are appended to the parent's, deduped by name.
    let mut derives: Vec<String> = parent_resolved.derives.clone();
    for d in &def.derives {
        if !derives.iter().any(|existing| existing == d) {
            derives.push(d.clone());
        }
    }

    ComponentDef {
        name: def.name.clone(),
        params,
        extends: parent_resolved.extends.clone(),
        impls,
        derives,
        body,
    }
}

/// Phase 10 — splice mixin bodies into a resolved [`ComponentDef`]'s
/// body. Two splice sources contribute:
///
/// 1. **Body `<use names="A, B"/>` statements.** Each name resolves
///    against the scope's local-mixin table. A name that resolves
///    swaps its `<use>` element for the mixin's body (in declared
///    order); names that don't resolve are dropped from the body
///    along with the `<use>` element (they're either the
///    extends-parent already consumed at harvest, or unknown).
/// 2. **Header `derive=[…]`.** The list of mixin names is treated
///    as if a synthetic `<use names="A, B"/>` were appended after
///    every other body statement — derives apply last so a body
///    `<use>` listing the same mixin can shadow them.
///
/// Returns the rewritten body. Mixins that the scope doesn't know
/// are silently ignored (parse-time error surfacing is Phase 17).
fn splice_mixins(def: &ComponentDef, scope: &LowerScope) -> Vec<AstNode> {
    if scope.local_mixins.is_empty() && def.derives.is_empty() {
        // Cheap path — strip any leftover `<use>` elements so they
        // don't pollute the body (they only ever encoded extends /
        // mixin intent, neither of which lowers visually).
        return def
            .body
            .iter()
            .filter(|n| !is_use_element(n))
            .cloned()
            .collect();
    }
    let mut out: Vec<AstNode> = Vec::with_capacity(def.body.len());
    for node in &def.body {
        match node {
            AstNode::Element(el) if el.tag == "use" => {
                let names = bare_string_attr(el, "names").unwrap_or_default();
                for name in parse_use_names(&names) {
                    // A `use Parent` whose first name resolved to a
                    // component (the extends parent) is consumed
                    // during harvest. We still process every name in
                    // case authoring style mixed mixins after a
                    // parent name (`use Base, Hoverable, Draggable`).
                    if let Some(mixin) = scope.local_mixin(&name) {
                        out.extend(mixin.body.iter().cloned());
                    }
                }
            }
            _ => out.push(node.clone()),
        }
    }
    // Header `derive=[…]` — append last so body `<use>` order wins.
    for name in &def.derives {
        if let Some(mixin) = scope.local_mixin(name) {
            out.extend(mixin.body.iter().cloned());
        }
    }
    out
}

/// Names listed inside a body `<use names="A, B, C as Alias"/>`
/// element. Mirrors [`parse_name_list`] but additionally trims a
/// trailing `as <alias>` segment (the canonical parser captures
/// the whole `use` line verbatim). The alias bound is recorded as
/// the trailing segment for diagnostics only; Phase 10 doesn't yet
/// rename spliced mixin state through the alias.
fn parse_use_names(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(|part| {
            let part = part.trim();
            // Strip an optional `as <alias>` tail.
            if let Some((head, _)) = part.split_once(" as ") {
                head.trim().to_string()
            } else {
                part.to_string()
            }
        })
        .filter(|s| !s.is_empty())
        .collect()
}

fn is_use_element(node: &AstNode) -> bool {
    matches!(node, AstNode::Element(el) if el.tag == "use")
}

/// Phase 10 — splice the bodies of every mixin listed in `with` into
/// `children`. Used by the element lowerer when it sees a `with=…`
/// attribute on any element (`<container with=[Hoverable, Draggable]>
/// …</container>`). Each named mixin's body is appended *before* the
/// element's own children so the mixin's `<let>` bindings can be
/// referenced by interpolations in those children (the splice is
/// positionally equivalent to the §7.3 "Body Tag Primitives" — the
/// mixin contributes leading `let`/`on`/`style` siblings the host
/// children read through document scope).
///
/// Names that don't resolve to a mixin on `scope` are silently
/// skipped (consistent with the `use` body-statement splice).
pub fn splice_mixin_into_children(
    with_list: &[String],
    children: &[AstNode],
    scope: &LowerScope,
) -> Vec<AstNode> {
    if with_list.is_empty() || !scope.has_local_mixins() {
        return children.to_vec();
    }
    let mut out: Vec<AstNode> = Vec::with_capacity(children.len() + with_list.len() * 2);
    for name in with_list {
        if let Some(mixin) = scope.local_mixin(name) {
            out.extend(mixin.body.iter().cloned());
        }
    }
    out.extend(children.iter().cloned());
    out
}

/// A render-tree node is anything that lowers to a real UI node —
/// container / text / heading / a registered tag etc. Internal
/// declaration statements (`<requires>` / `<style>` / `<on>` /
/// `<use>` / `<let>`) don't count, so a child whose only body is
/// `<requires>` falls back to the parent's render tree.
fn is_render_tree_node(node: &AstNode) -> bool {
    match node {
        AstNode::Element(el) => !matches!(
            el.tag.as_str(),
            "requires" | "style" | "on" | "expr" | "use" | "let"
        ),
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

/// Phase 14 — bucket the children of a call site into the default-
/// slot pile and a `name → AST` named-slot map. A child carrying a
/// literal `slot="X"` attribute lands in `named[X]`; everything else
/// stays in the default bucket. The `slot=` attribute is stripped
/// before the child enters the named bucket so the rendered tree
/// doesn't carry the marker into runtime nodes.
fn partition_call_site_slots(
    children: &[AstNode],
) -> (Vec<AstNode>, HashMap<String, Vec<AstNode>>) {
    let mut default: Vec<AstNode> = Vec::new();
    let mut named: HashMap<String, Vec<AstNode>> = HashMap::new();
    for child in children {
        match child {
            AstNode::Element(el) => {
                if let Some((slot_name, stripped)) = take_slot_marker(el) {
                    named
                        .entry(slot_name)
                        .or_default()
                        .push(AstNode::Element(stripped));
                } else {
                    default.push(child.clone());
                }
            }
            other => default.push(other.clone()),
        }
    }
    (default, named)
}

/// If `el` carries a literal `slot="X"` attribute, return the slot
/// name plus a clone of `el` with that attribute removed. Returns
/// `None` when `slot=` is missing or non-string-shaped (an expression-
/// valued `slot={…}` falls through as a default-slot child — the
/// dynamic-slot-routing path is Phase-17+ polish).
fn take_slot_marker(el: &Element) -> Option<(String, Element)> {
    let idx = el.attributes.iter().position(|a| {
        matches!(a.name.namespace, AttributeNamespace::Bare) && a.name.local == "slot"
    })?;
    let name = match &el.attributes[idx].value {
        AttributeValue::String { value, .. } => value.clone(),
        _ => return None,
    };
    let mut clone = el.clone();
    clone.attributes.remove(idx);
    Some((name, clone))
}

/// Phase 14 — slot type detector. Any prop declared as `ui`,
/// `slot`, `slot<…>`, or a `|…| → ui` / `|…| -> ui` lambda type
/// triggers the default-resolution path. The matcher is intentionally
/// loose: types are stored as raw textual fragments by the canonical
/// reader, so we don't tokenise — a substring + prefix check is
/// enough for the surface declared in §7.5.
fn is_slot_type(ty: Option<&str>) -> bool {
    let Some(t) = ty else { return false };
    let t = t.trim();
    if t.is_empty() {
        return false;
    }
    if t == "ui" || t == "slot" {
        return true;
    }
    if t.starts_with("slot<") {
        return true;
    }
    // Lambda-return-ui: `|...| → ui` or `|...| -> ui`. The unicode
    // arrow is what the docs show; the ASCII form rounds out the
    // surface so editors that auto-convert don't break it.
    let normalised = t.replace('→', "->");
    if normalised.contains("->") && normalised.trim_end().ends_with("ui") {
        return true;
    }
    false
}

/// Parse a default expression for a slot-typed param into the AST
/// nodes the runtime should lower at consumer time. The default text
/// is what the canonical reader captured (e.g. `<heading>Tasks</heading>`,
/// `|t, i| <text>{t.title}</text>`); we hand it back to the parser
/// and return the resulting top-level nodes.
fn parse_slot_default(raw: &str) -> Option<Vec<AstNode>> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    // Strip a surrounding `{…}` brace pair — the canonical reader
    // wraps tree-literal defaults that way (`= <heading>…</heading>`
    // comes in unbraced; `= {<heading>…</heading>}` keeps the braces).
    let body = if raw.starts_with('{') && raw.ends_with('}') && raw.len() >= 2 {
        &raw[1..raw.len() - 1]
    } else {
        raw
    };
    let body = body.trim();
    // For lambda-shaped defaults (`|t, i| <text>…</text>`), peel the
    // parameter list off the front; the body after `|` is the AST.
    // The parameter names aren't stored on the slot binding here —
    // they're invoke-time arguments and the `<invoke>` shape names
    // them itself. (A future-phase capture-form (`<slot
    // name="row" captures="item, index">`) would surface them; that
    // remains roadmap polish.)
    let body = if let Some(after_open) = body.strip_prefix('|') {
        match after_open.find('|') {
            Some(end) => after_open[end + 1..].trim(),
            None => body,
        }
    } else {
        body
    };
    let (doc, errs) = prism_core::language::prism_ui::parse(body);
    if !errs.is_empty() {
        return None;
    }
    if doc.nodes.is_empty() {
        return None;
    }
    Some(doc.nodes)
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
                derives: Vec::new(),
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
                derives: Vec::new(),
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

    // ── Phase 10 — mixin harvest + splice ─────────────────────────

    #[test]
    fn harvests_mixin_declaration() {
        let (doc, errs) = parse(
            r#"mixin Hoverable {
  let hovered = state(false)
  on pointerenter { hovered <- true }
  on pointerleave { hovered <- false }
}"#,
        );
        assert!(errs.is_empty(), "parse errs: {errs:?}");
        let out = harvest_declarations(&doc.nodes, None);
        let m = out.mixins.get("Hoverable").expect("mixin missing");
        // One `<let>` + two `<on>` body statements.
        let let_count = m
            .body
            .iter()
            .filter(|n| matches!(n, AstNode::Element(e) if e.tag == "let"))
            .count();
        let on_count = m
            .body
            .iter()
            .filter(|n| matches!(n, AstNode::Element(e) if e.tag == "on"))
            .count();
        assert_eq!(let_count, 1);
        assert_eq!(on_count, 2);
    }

    #[test]
    fn harvests_mixin_under_namespace_prefix() {
        let (doc, _) = parse(
            r#"namespace Forms

mixin Highlightable { style { background = accent } }"#,
        );
        let out = harvest_declarations(&doc.nodes, None);
        assert!(out.mixins.contains_key("Forms.Highlightable"));
        assert!(!out.mixins.contains_key("Highlightable"));
    }

    #[test]
    fn parse_name_list_brackets_and_spaces() {
        assert_eq!(parse_name_list("A, B"), vec!["A", "B"]);
        assert_eq!(parse_name_list("[A, B]"), vec!["A", "B"]);
        assert_eq!(parse_name_list(" [ A , B , C ] "), vec!["A", "B", "C"]);
        let empty: Vec<String> = Vec::new();
        assert_eq!(parse_name_list(""), empty);
        assert_eq!(parse_name_list("[]"), empty);
    }

    #[test]
    fn parse_use_names_strips_alias_tail() {
        assert_eq!(parse_use_names("Hoverable as h"), vec!["Hoverable"]);
        assert_eq!(
            parse_use_names("Base, Hoverable as h, Draggable"),
            vec!["Base", "Hoverable", "Draggable"]
        );
    }

    #[test]
    fn body_use_resolving_to_mixin_splices_into_body() {
        // Build a scope carrying `Hoverable` as a mixin, then
        // instantiate a component whose body says `use Hoverable`.
        let (mixin_doc, _) = parse(
            r#"mixin Hoverable {
  let hovered = state(false)
}"#,
        );
        let mixins = harvest_declarations(&mixin_doc.nodes, None).mixins;
        let (comp_doc, _) = parse(
            r#"component Card(title: string) = {
  use Hoverable
  <container><text>{title}</text></container>
}"#,
        );
        let comps = harvest_components(&comp_doc.nodes);
        let card = comps.get("Card").expect("card missing");

        let scope = crate::interpret::LowerScope::new()
            .with_local_mixins(std::sync::Arc::new(mixins))
            .with_local_components(std::sync::Arc::new(comps.clone()));
        let resolved = flatten_extends(card.as_ref(), &scope, &mut HashSet::new());
        let body = splice_mixins(&resolved, &scope);
        // The mixin's `<let>` body statement landed before the
        // container in the spliced body.
        let let_idx = body
            .iter()
            .position(|n| matches!(n, AstNode::Element(e) if e.tag == "let"));
        let container_idx = body
            .iter()
            .position(|n| matches!(n, AstNode::Element(e) if e.tag == "container"));
        assert!(let_idx.is_some(), "mixin let missing from spliced body");
        assert!(container_idx.is_some(), "container missing from body");
        assert!(
            let_idx < container_idx,
            "mixin must splice before render tree"
        );
    }

    #[test]
    fn header_derive_attribute_splices_mixin_into_body() {
        let (mixin_doc, _) = parse(
            r#"mixin Hoverable {
  let hovered = state(false)
}"#,
        );
        let mixins = harvest_declarations(&mixin_doc.nodes, None).mixins;
        // Component with `derive=[Hoverable]` (XML-form attribute).
        let xml = r#"<component name="Card" derive="Hoverable"><container/></component>"#;
        let (comp_doc, _errs) = parse(xml);
        let comps = harvest_components(&comp_doc.nodes);
        let card = comps.get("Card").expect("card missing");
        assert_eq!(card.derives, vec!["Hoverable".to_string()]);

        let scope = crate::interpret::LowerScope::new()
            .with_local_mixins(std::sync::Arc::new(mixins))
            .with_local_components(std::sync::Arc::new(comps.clone()));
        let resolved = flatten_extends(card.as_ref(), &scope, &mut HashSet::new());
        let body = splice_mixins(&resolved, &scope);
        assert!(body
            .iter()
            .any(|n| matches!(n, AstNode::Element(e) if e.tag == "let")));
    }

    #[test]
    fn derive_list_brackets_accepted() {
        let xml = r#"<component name="Card" derive="[A, B]"><container/></component>"#;
        let (doc, _) = parse(xml);
        let comps = harvest_components(&doc.nodes);
        let card = comps.get("Card").unwrap();
        assert_eq!(card.derives, vec!["A".to_string(), "B".to_string()]);
    }

    #[test]
    fn derives_inherit_from_parent_then_dedupe() {
        // Parent derives `A`; child derives `B`. Resolved chain
        // should be `[A, B]`.
        let xml = r#"<component name="Parent" derive="A"><container/></component>
<component name="Child" extends="Parent" derive="B"><container/></component>"#;
        let (doc, _) = parse(xml);
        let comps = harvest_components(&doc.nodes);
        let child = comps.get("Child").expect("child missing");
        let scope = crate::interpret::LowerScope::new()
            .with_local_components(std::sync::Arc::new(comps.clone()));
        let resolved = flatten_extends(child.as_ref(), &scope, &mut HashSet::new());
        assert_eq!(resolved.derives, vec!["A".to_string(), "B".to_string()]);

        // Duplicate `B` in the child stays single after dedupe.
        let xml2 = r#"<component name="P" derive="B"><container/></component>
<component name="C" extends="P" derive="B"><container/></component>"#;
        let (doc, _) = parse(xml2);
        let comps = harvest_components(&doc.nodes);
        let child = comps.get("C").unwrap();
        let scope = crate::interpret::LowerScope::new()
            .with_local_components(std::sync::Arc::new(comps.clone()));
        let resolved = flatten_extends(child.as_ref(), &scope, &mut HashSet::new());
        assert_eq!(resolved.derives, vec!["B".to_string()]);
    }

    #[test]
    fn unknown_mixin_name_is_silently_dropped() {
        let (comp_doc, _) = parse(
            r#"component Card(title: string) = {
  use UnknownMixin
  <container><text>{title}</text></container>
}"#,
        );
        let comps = harvest_components(&comp_doc.nodes);
        let card = comps.get("Card").unwrap();
        let scope = crate::interpret::LowerScope::new()
            .with_local_components(std::sync::Arc::new(comps.clone()));
        let resolved = flatten_extends(card.as_ref(), &scope, &mut HashSet::new());
        let body = splice_mixins(&resolved, &scope);
        // No mixin → no `<let>` in body; the `<use>` element is
        // stripped (it carried only the mixin reference).
        assert!(body
            .iter()
            .all(|n| !matches!(n, AstNode::Element(e) if e.tag == "use" || e.tag == "let")));
    }

    #[test]
    fn splice_into_children_appends_mixin_body_first() {
        let (mixin_doc, _) = parse(
            r#"mixin Hoverable {
  let hovered = state(false)
}"#,
        );
        let mixins = harvest_declarations(&mixin_doc.nodes, None).mixins;
        let scope =
            crate::interpret::LowerScope::new().with_local_mixins(std::sync::Arc::new(mixins));
        let (frag_doc, _) = parse("<text>hello</text>");
        let result = splice_mixin_into_children(&["Hoverable".into()], &frag_doc.nodes, &scope);
        // First node is the mixin's `<let>`, second is `<text>`.
        let first_tag = match &result[0] {
            AstNode::Element(e) => e.tag.as_str(),
            _ => "",
        };
        let second_tag = match &result[1] {
            AstNode::Element(e) => e.tag.as_str(),
            _ => "",
        };
        assert_eq!(first_tag, "let");
        assert_eq!(second_tag, "text");
    }

    #[test]
    fn splice_into_children_skips_when_no_mixins() {
        let scope = crate::interpret::LowerScope::new();
        let (frag_doc, _) = parse("<text>hi</text>");
        let result = splice_mixin_into_children(&["AnyName".into()], &frag_doc.nodes, &scope);
        assert_eq!(result.len(), 1);
    }
}
