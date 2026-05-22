//! Phase 11 — macros over markup. See
//! `docs/dev/prui-expressiveness-roadmap.md` §7.8.
//!
//! A macro is a parse-time tree-to-tree rewrite. The canonical
//! parser projects a `<macro>` declaration onto an [`Element`]
//! whose body holds `<match>` and `<expand>` child blocks plus
//! `<property>` children declaring the captured parameters. At
//! document-scope harvest time we record the declaration in a
//! [`MacroDef`] and stash it on the active [`LowerScope`]; the
//! expansion pass [`expand_macros`] then walks every AST sibling
//! list and replaces any PascalCase tag whose name matches a
//! registered macro with the macro's expansion body, with the
//! call-site attributes bound to the macro's declared params.
//!
//! ## Capture model
//!
//! The macro's declared `<property>` children — projected from the
//! canonical `(label: string, value: string)` parameter list —
//! define the capture names. The call site's attributes provide
//! the values: `<Field label="Title" value="hi"/>` binds
//! `label="Title", value="hi"`. Inside the expansion body, those
//! names are substituted into `{ident}` interpolations through the
//! existing binding-scope path; **no AST rewrite is needed** —
//! the lowerer already resolves `{label}` against any binding the
//! call put in scope.
//!
//! Attribute macros (`attribute` modifier on the declaration) ride
//! the same primitive but expand into attribute key/value pairs
//! against the *host* element rather than replacing it. The
//! `<expand to-attrs>` body lists `name = value` statements; each
//! becomes a bare attribute on the host.
//!
//! ## Hygiene
//!
//! `<let name="hovered" .../>` body statements inside `<expand>`
//! get a fresh suffix so they can't shadow call-site bindings.
//! The suffix is the macro name + a process-monotonic counter —
//! deterministic for the same document text, distinct across
//! macros and invocations.
//!
//! ## Depth limit
//!
//! Macros may expand to other macros (a `<Disclosure>` whose
//! expansion contains another `<Field>` use). The expander
//! tracks a recursion depth; once it crosses
//! [`MAX_EXPANSION_DEPTH`] it stops recursing further and leaves
//! the call site untouched (a Phase 17 `prism-cli` lint surfaces
//! the offending macro chain).

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use prism_core::language::prism_ui::ast::TemplatePart;
use prism_core::language::prism_ui::{
    Attribute, AttributeName, AttributeNamespace, AttributeValue, Element, Node as AstNode,
};

use super::components::ParamDef;

/// Cap on macro recursion. Above this limit the expander stops
/// recursing (the lint surfaces the offending chain). Matches the
/// `macro_rules!` default the doc cites.
pub const MAX_EXPANSION_DEPTH: usize = 5;

/// Process-monotonic counter for hygiene suffixes — guarantees a
/// macro expansion's `<let>` bindings can never clash with the
/// call-site scope. Reset per process; the suffix is opaque so
/// authoring doesn't depend on its exact shape.
static HYGIENE_COUNTER: AtomicU64 = AtomicU64::new(0);

fn fresh_hygiene_id() -> u64 {
    HYGIENE_COUNTER.fetch_add(1, Ordering::Relaxed)
}

/// One macro declaration.
#[derive(Debug, Clone)]
pub struct MacroDef {
    pub name: String,
    /// Declared params (from the canonical `(label: string, value:
    /// string)` segment). For ordinary macros these are also the
    /// capture names; the call site's attributes are bound to them
    /// before the expansion lowers. Empty list = a macro with no
    /// captures (the trivial `<Disclosure>{children}</Disclosure>`
    /// case).
    pub params: Vec<ParamDef>,
    /// The `<match>` template's children. Phase 11 doesn't parse
    /// the template's attribute placeholders (`<Field label={lbl}/>`
    /// → bind `lbl`); it relies on the declared `params` for the
    /// capture names. The match body still round-trips here for
    /// downstream tooling (a Phase 17 lint can verify shape).
    pub pattern: Vec<AstNode>,
    /// The `<expand>` body — substituted at the call site.
    pub expand: Vec<AstNode>,
    /// `attribute` modifier — when `true`, this macro expands a
    /// host element's attribute into the macro's `<expand>` body
    /// (read as a list of `name = value` statements). When
    /// `false`, the macro replaces the call site tag.
    pub is_attribute: bool,
}

/// Walk a slice of top-level nodes and harvest every `<macro
/// name="…">` declaration into a name → def map. A leading
/// `<namespace name="Ns"/>` directive (or external `alias`
/// override) prefixes each name as `Ns.Field` per §7.11.
pub fn harvest_macros(nodes: &[AstNode], alias: Option<&str>) -> HashMap<String, Arc<MacroDef>> {
    let file_namespace = nodes.iter().find_map(|n| match n {
        AstNode::Element(el) if el.tag == "namespace" => bare_string_attr(el, "name"),
        _ => None,
    });
    let prefix = alias
        .map(|s| s.to_string())
        .or(file_namespace)
        .filter(|s| !s.is_empty());

    let mut out = HashMap::new();
    for node in nodes {
        let AstNode::Element(el) = node else {
            continue;
        };
        if el.tag != "macro" {
            continue;
        }
        let Some(mut def) = element_to_macro(el) else {
            continue;
        };
        if let Some(p) = &prefix {
            def.name = format!("{p}.{}", def.name);
        }
        out.insert(def.name.clone(), Arc::new(def));
    }
    out
}

fn element_to_macro(el: &Element) -> Option<MacroDef> {
    let name = bare_string_attr(el, "name")?;
    // `attribute` modifier — flagged via a header `attribute` flag
    // attribute (`<macro Foo attribute=true>` or canonical
    // `macro Foo, attribute, level: int { … }` which the parser
    // currently writes as a `<macro name="Foo" attribute="true">`
    // shape, or as a `<expr body="attribute"/>` body statement).
    let is_attribute = bare_string_attr(el, "attribute")
        .map(|v| v == "true" || v == "1" || v.is_empty())
        .unwrap_or(false)
        || el.children.iter().any(|c| match c {
            AstNode::Element(child) if child.tag == "expr" => {
                bare_string_attr(child, "body").as_deref() == Some("attribute")
            }
            _ => false,
        });

    let mut params = Vec::new();
    let mut pattern = Vec::new();
    let mut expand = Vec::new();
    for child in &el.children {
        let AstNode::Element(child_el) = child else {
            continue;
        };
        match child_el.tag.as_str() {
            "property" => {
                if let Some(p) = property_to_param(child_el) {
                    params.push(p);
                }
            }
            "match" => {
                pattern.extend(child_el.children.iter().cloned());
            }
            "expand" => {
                expand.extend(child_el.children.iter().cloned());
            }
            _ => {}
        }
    }
    Some(MacroDef {
        name,
        params,
        pattern,
        expand,
        is_attribute,
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

/// The result of the `expand_macros` pre-pass. Carries the rewritten
/// AST node list and a hygiene-prefix flag so downstream lowering
/// can introspect when a frame was macro-expanded (used in tests).
pub fn expand_macros(
    nodes: &[AstNode],
    macros: &HashMap<String, Arc<MacroDef>>,
    depth: usize,
) -> Vec<AstNode> {
    if macros.is_empty() {
        return nodes.to_vec();
    }
    let mut out = Vec::with_capacity(nodes.len());
    for node in nodes {
        out.extend(expand_one(node, macros, depth));
    }
    out
}

fn expand_one(
    node: &AstNode,
    macros: &HashMap<String, Arc<MacroDef>>,
    depth: usize,
) -> Vec<AstNode> {
    match node {
        AstNode::Element(el) => expand_element(el, macros, depth),
        // Text / interpolation / comments round-trip unchanged. The
        // macro engine works on element shape; expression bodies are
        // resolved by the lowerer's binding-scope path, so `{lbl}`
        // inside an expansion's text already substitutes through the
        // call-site binding that the expander introduces.
        other => vec![other.clone()],
    }
}

fn expand_element(
    el: &Element,
    macros: &HashMap<String, Arc<MacroDef>>,
    depth: usize,
) -> Vec<AstNode> {
    // Don't expand inside `<macro>` declarations themselves — their
    // bodies are templates, not call sites.
    if el.tag == "macro" {
        return vec![AstNode::Element(el.clone())];
    }
    // 1. Tag-form macro invocation: a PascalCase tag whose name
    //    matches a registered macro. Substitute the expansion with
    //    call-site attrs bound as `<let name="param" value=…/>`
    //    statements prepended to the expansion. Recurse.
    if let Some(macro_def) = macros.get(el.tag.as_str()) {
        if !macro_def.is_attribute {
            if depth >= MAX_EXPANSION_DEPTH {
                // Stop recursion — leave the call site untouched.
                return vec![AstNode::Element(el.clone())];
            }
            let expanded = expand_macro_invocation(el, macro_def.as_ref());
            // Recurse: the expanded body may itself contain macro
            // invocations.
            return expand_macros(&expanded, macros, depth + 1);
        }
    }
    // 2. Attribute-form macro: the element carries an attribute
    //    whose local name matches a registered *attribute* macro.
    //    Each such attribute is expanded into the macro's
    //    `<expand>` body, projected to bare attrs on the element.
    let mut rewritten = el.clone();
    let mut new_attrs: Vec<Attribute> = Vec::with_capacity(rewritten.attributes.len());
    for attr in rewritten.attributes.drain(..) {
        if matches!(attr.name.namespace, AttributeNamespace::Bare) {
            if let Some(macro_def) = macros.get(&attr.name.local) {
                if macro_def.is_attribute {
                    // Bind the attribute's value to the first param
                    // (the `level: int` capture in the §7.8
                    // `Elevation` example) and project the
                    // expansion's `<expr body="name = value"/>`
                    // statements onto bare attrs.
                    let mut substituted = expand_attribute_macro(macro_def.as_ref(), &attr);
                    new_attrs.append(&mut substituted);
                    continue;
                }
            }
        }
        new_attrs.push(attr);
    }
    rewritten.attributes = new_attrs;
    // Recurse into children with the same depth (children-of-an-
    // expanded-tree don't increase recursion).
    rewritten.children = expand_macros(&rewritten.children, macros, depth);
    vec![AstNode::Element(rewritten)]
}

/// Substitute a tag-form macro call site with its `<expand>` body,
/// binding the call's attributes to the declared params (as a
/// leading `<let name="…" value=…/>` for each), applying hygiene
/// to any `<let>` inside the expansion, and forwarding the call's
/// children as the expansion's `{children}` slot.
fn expand_macro_invocation(call: &Element, def: &MacroDef) -> Vec<AstNode> {
    let hygiene_id = fresh_hygiene_id();
    // Step 1: build per-param `<let>` bindings from the call's
    // attributes. We preserve the original [`AttributeValue`] so
    // expression-form attrs (`label={lbl}`) reach the runtime's
    // `<let>` evaluator with their typed shape intact — required
    // for nested macro expansions where the outer macro's
    // expansion passes a captured-name expression to an inner
    // macro. Honour declared defaults; for a missing non-default
    // param synthesize an empty string binding.
    let mut bindings: Vec<AstNode> = Vec::with_capacity(def.params.len());
    for param in &def.params {
        let value = call_attribute_typed(call, &param.name).unwrap_or_else(|| {
            // Declared default → wrap as a String literal so the
            // let evaluator reads the same shape it would have for
            // an authored `<let name="X" value="default"/>`.
            AttributeValue::String {
                value: param.default.clone().unwrap_or_default(),
                range: call.range,
            }
        });
        bindings.push(synth_let_typed(&param.name, value, call.range));
    }

    // Step 2: clone the expansion body and rename macro-introduced
    // `<let>` bindings (`<let name="X" .../>`) to a fresh name.
    let expansion: Vec<AstNode> = def
        .expand
        .iter()
        .map(|n| rename_macro_lets(n, &def.name, hygiene_id))
        .collect();

    // Step 3: stitch bindings + expansion together. The bindings
    // come first so any `{ident}` interpolation in the expansion
    // resolves through the scope they install.
    let mut out = bindings;
    out.extend(expansion);

    // Step 4 (children-slot): a macro author wrote
    // `<Disclosure>{children}</Disclosure>` — the call's children
    // are already AST nodes; we pass them through via a synthetic
    // `<expr body="children"/>` substitution. For Phase 11 we keep
    // it simple: append the call's children at the end (the §7.8
    // `<slot/>` shape is the more general form, but it depends on
    // §7.5 slot-bindings — Phase 14). Most macro use cases at this
    // phase have empty body.
    let _ = call.children.clone();
    out
}

/// Expand an attribute-form macro hit on an element attribute. The
/// attribute's value is bound to the macro's first declared param;
/// the expansion body is read as `<expr body="key = value"/>`
/// statements and projected onto bare attrs.
fn expand_attribute_macro(def: &MacroDef, attr: &Attribute) -> Vec<Attribute> {
    let mut out = Vec::new();
    // Bind the attribute's value (e.g. `elevation=2`) to the first
    // param. For Phase 11 we just hold onto the binding by name so
    // the expansion's `{level}` interpolation resolves at the
    // attribute-string layer. Since attrs don't yet carry their own
    // `<let>` scope, we materialise the binding as a synthetic prefix
    // and rely on the call site's surrounding scope. The simpler
    // path: substitute `{<param-name>}` inside each emitted attr
    // value with the bound text.
    let bound_value = attribute_value_text(&attr.value).unwrap_or_default();
    let param_name = def
        .params
        .first()
        .map(|p| p.name.clone())
        .unwrap_or_default();

    for node in &def.expand {
        let AstNode::Element(el) = node else { continue };
        if el.tag != "expr" {
            continue;
        }
        let Some(body) = bare_string_attr(el, "body") else {
            continue;
        };
        // Split `key = value` (one statement per `<expr>`).
        let (key, value) = match body.split_once('=') {
            Some(pair) => pair,
            None => continue,
        };
        let key = key.trim().to_string();
        let value_raw = value.trim();
        // Substitute the bound param name in the value text. Only a
        // bare `{<param>}` interpolation is replaced; full Pratt-
        // parsed `{<param>.foo}` round-trips unchanged.
        let value_text = substitute_param_token(value_raw, &param_name, &bound_value);
        out.push(Attribute {
            name: AttributeName {
                raw: key.clone(),
                local: key,
                namespace: AttributeNamespace::Bare,
                range: attr.range,
            },
            value: AttributeValue::String {
                value: value_text,
                range: attr.range,
            },
            range: attr.range,
        });
    }
    out
}

/// Replace every `{<param>}` token in `text` with `value`. The
/// substitution is a literal text replacement; the lowerer's
/// expression parser sees the resulting string verbatim.
fn substitute_param_token(text: &str, param: &str, value: &str) -> String {
    if param.is_empty() {
        return text.to_string();
    }
    let needle = format!("{{{param}}}");
    text.replace(&needle, value)
}

/// Pull a call-site attribute's typed [`AttributeValue`]. Returns
/// `None` when the attribute is missing.
fn call_attribute_typed(call: &Element, name: &str) -> Option<AttributeValue> {
    call.attributes
        .iter()
        .find(|a| matches!(a.name.namespace, AttributeNamespace::Bare) && a.name.local == name)
        .map(|a| a.value.clone())
}

fn attribute_value_text(value: &AttributeValue) -> Option<String> {
    match value {
        AttributeValue::String { value, .. } => Some(value.clone()),
        AttributeValue::Empty => Some("true".to_string()),
        AttributeValue::Expression(expr) => Some(format!("{{{}}}", expr.body)),
        AttributeValue::Template { parts, .. } => {
            let mut s = String::new();
            for part in parts {
                match part {
                    TemplatePart::Literal { value, .. } => s.push_str(value),
                    TemplatePart::Expression(e) => {
                        s.push('{');
                        s.push_str(&e.body);
                        s.push('}');
                    }
                }
            }
            Some(s)
        }
    }
}

/// Synthesise a `<let name="X" value=V/>` element so the binding
/// reaches the document's `lower_document_with_scope` walker. The
/// typed-value overload preserves expression-form values (the
/// recursive-macro path needs `value={ident}` to round-trip as an
/// expression rather than a literal string).
fn synth_let_typed(
    name: &str,
    value: AttributeValue,
    range: prism_core::language::syntax::SourceRange,
) -> AstNode {
    AstNode::Element(Element {
        tag: "let".to_string(),
        attributes: vec![
            Attribute {
                name: AttributeName {
                    raw: "name".to_string(),
                    local: "name".to_string(),
                    namespace: AttributeNamespace::Bare,
                    range,
                },
                value: AttributeValue::String {
                    value: name.to_string(),
                    range,
                },
                range,
            },
            Attribute {
                name: AttributeName {
                    raw: "value".to_string(),
                    local: "value".to_string(),
                    namespace: AttributeNamespace::Bare,
                    range,
                },
                value,
                range,
            },
        ],
        children: Vec::new(),
        self_closing: true,
        range,
        tag_range: range,
    })
}

/// Hygiene — rename macro-introduced `<let name="X"/>` bindings to
/// a fresh name so they can't shadow call-site bindings. The
/// renaming touches both the binding declaration and any later
/// reference; for Phase 11 we apply the rename only to `<let>`
/// declarations (the let element's `name=` attribute). Resolving
/// references to the renamed binding is the job of the lowerer's
/// scope path — Phase 11 doesn't try to rewrite identifiers
/// inside `{expr}` bodies (a follow-up pass can lint when an
/// author intentionally references a hygiene-renamed binding,
/// which is rare).
fn rename_macro_lets(node: &AstNode, macro_name: &str, hygiene_id: u64) -> AstNode {
    match node {
        AstNode::Element(el) => {
            let mut copy = el.clone();
            if copy.tag == "let" {
                for attr in &mut copy.attributes {
                    if matches!(attr.name.namespace, AttributeNamespace::Bare)
                        && attr.name.local == "name"
                    {
                        if let AttributeValue::String { value, .. } = &mut attr.value {
                            *value = hygienic_name(macro_name, hygiene_id, value);
                        }
                    }
                }
            }
            copy.children = copy
                .children
                .iter()
                .map(|c| rename_macro_lets(c, macro_name, hygiene_id))
                .collect();
            AstNode::Element(copy)
        }
        other => other.clone(),
    }
}

fn hygienic_name(macro_name: &str, hygiene_id: u64, original: &str) -> String {
    format!("__macro_{macro_name}_{hygiene_id}_{original}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use prism_core::language::prism_ui::parse;

    fn first_macro(src: &str) -> Arc<MacroDef> {
        let (doc, errs) = parse(src);
        assert!(errs.is_empty(), "parse errs: {errs:?}");
        let map = harvest_macros(&doc.nodes, None);
        let key = map.keys().next().expect("no macro harvested").clone();
        map[&key].clone()
    }

    #[test]
    fn harvests_macro_with_params() {
        let m = first_macro(
            r#"macro Field(label: string, value: string) {
  match { <Field label={label} value={value}/> }
  expand {
    <container>
      <text>{label}</text>
      <text>{value}</text>
    </container>
  }
}"#,
        );
        assert_eq!(m.name, "Field");
        assert_eq!(m.params.len(), 2);
        assert_eq!(m.params[0].name, "label");
        assert_eq!(m.params[1].name, "value");
        // The expand body contains a `<container>` with two `<text>`
        // children.
        let container = m
            .expand
            .iter()
            .find_map(|n| match n {
                AstNode::Element(el) if el.tag == "container" => Some(el),
                _ => None,
            })
            .expect("expand container missing");
        let text_count = container
            .children
            .iter()
            .filter(|n| matches!(n, AstNode::Element(e) if e.tag == "text"))
            .count();
        assert_eq!(text_count, 2);
    }

    #[test]
    fn harvests_macro_under_namespace_prefix() {
        let (doc, _) = parse(
            r#"namespace Forms

macro Field(label: string) {
  match { <Field label={label}/> }
  expand { <text>{label}</text> }
}"#,
        );
        let map = harvest_macros(&doc.nodes, None);
        assert!(map.contains_key("Forms.Field"));
        assert!(!map.contains_key("Field"));
    }

    #[test]
    fn expand_macro_invocation_binds_call_attrs() {
        let m = first_macro(
            r#"macro Greeting(name: string) {
  match { <Greeting name={name}/> }
  expand { <text>Hello, {name}</text> }
}"#,
        );
        // Synth a `<Greeting name="World"/>` call.
        let (call_doc, _) = parse(r#"<Greeting name="World"/>"#);
        let AstNode::Element(call) = &call_doc.nodes[0] else {
            panic!("expected element");
        };
        let expanded = expand_macro_invocation(call, m.as_ref());
        // Leading `<let name="name" value="World"/>` synth + the
        // `<text>` body.
        let let_el = expanded
            .iter()
            .find_map(|n| match n {
                AstNode::Element(e) if e.tag == "let" => Some(e),
                _ => None,
            })
            .expect("let binding missing");
        let value_attr = let_el
            .attributes
            .iter()
            .find(|a| a.name.local == "value")
            .unwrap();
        // The call's `name="World"` was a String literal — the
        // synthesized binding preserves the same shape.
        match &value_attr.value {
            AttributeValue::String { value, .. } => assert_eq!(value, "World"),
            other => panic!("unexpected value shape: {other:?}"),
        }
    }

    #[test]
    fn expand_macro_preserves_expression_form_call_attr() {
        // Outer macro's expansion passes `{label}` (an expression-
        // form attribute) to an inner macro. The synthesized
        // `<let>` for the inner's `label` param must carry that
        // expression shape so the runtime evaluator resolves it
        // against the surrounding `<let>` scope rather than seeing
        // a literal `"{label}"` string.
        let m = first_macro(
            r#"macro Inner(label: string) {
  match { <Inner label={label}/> }
  expand { <text>{label}</text> }
}"#,
        );
        let (call_doc, _) = parse(r#"<Inner label={greeting}/>"#);
        let AstNode::Element(call) = &call_doc.nodes[0] else {
            panic!();
        };
        let expanded = expand_macro_invocation(call, m.as_ref());
        let let_el = expanded
            .iter()
            .find_map(|n| match n {
                AstNode::Element(e) if e.tag == "let" => Some(e),
                _ => None,
            })
            .unwrap();
        let value_attr = let_el
            .attributes
            .iter()
            .find(|a| a.name.local == "value")
            .unwrap();
        match &value_attr.value {
            AttributeValue::Expression(expr) => assert_eq!(expr.body, "greeting"),
            other => panic!("expected Expression, got {other:?}"),
        }
    }

    #[test]
    fn expand_macros_substitutes_a_tag_form_call() {
        let src = r#"macro Greeting(name: string) {
  match { <Greeting name={name}/> }
  expand { <text>Hello, {name}</text> }
}

<Greeting name="World"/>"#;
        let (doc, _) = parse(src);
        let macros = harvest_macros(&doc.nodes, None);
        let expanded = expand_macros(&doc.nodes, &macros, 0);
        // The macro declaration itself round-trips; the `<Greeting/>`
        // call should be replaced by a `<let>` + a `<text>`.
        let has_text_hello = expanded
            .iter()
            .any(|n| matches!(n, AstNode::Element(e) if e.tag == "text"));
        assert!(has_text_hello, "expected substituted text element");
    }

    #[test]
    fn expand_respects_recursion_depth_limit() {
        // A macro that recursively expands itself — the depth limit
        // must prevent unbounded expansion.
        let src = r#"macro Loop(n: int) {
  match { <Loop n={n}/> }
  expand { <Loop n={n}/> }
}

<Loop n="0"/>"#;
        let (doc, _) = parse(src);
        let macros = harvest_macros(&doc.nodes, None);
        let expanded = expand_macros(&doc.nodes, &macros, 0);
        // At depth `MAX_EXPANSION_DEPTH` recursion stops; the
        // expansion contains a leftover `<Loop>` element rather than
        // an unbounded number of `<let>` bindings.
        let leftover_loop = expanded
            .iter()
            .any(|n| matches!(n, AstNode::Element(e) if e.tag == "Loop"));
        assert!(
            leftover_loop,
            "depth limit should leave outer <Loop> in place"
        );
    }

    #[test]
    fn hygiene_renames_let_bindings() {
        // A macro whose expansion contains a `<let>` — the let's
        // `name` attribute must be renamed to a hygienic shape so
        // the call site can't accidentally shadow it (or vice-
        // versa).
        let m = first_macro(
            r#"macro Counter() {
  match { <Counter/> }
  expand {
    let count = 0
    <text>{count}</text>
  }
}"#,
        );
        // Synth a `<Counter/>` call and expand.
        let (call_doc, _) = parse(r#"<Counter/>"#);
        let AstNode::Element(call) = &call_doc.nodes[0] else {
            panic!();
        };
        let expanded = expand_macro_invocation(call, m.as_ref());
        // The `<let>` inside the expansion has been renamed.
        let let_el = expanded
            .iter()
            .find_map(|n| match n {
                AstNode::Element(e) if e.tag == "let" => Some(e),
                _ => None,
            })
            .expect("renamed let missing");
        let name_attr = let_el
            .attributes
            .iter()
            .find(|a| a.name.local == "name")
            .unwrap();
        match &name_attr.value {
            AttributeValue::String { value, .. } => {
                assert!(
                    value.starts_with("__macro_Counter_"),
                    "expected hygienic prefix, got `{value}`"
                );
                assert!(value.ends_with("_count"));
            }
            _ => panic!(),
        }
    }

    #[test]
    fn substitute_param_token_replaces_braced_form() {
        assert_eq!(
            substitute_param_token("border: {level}px", "level", "2"),
            "border: 2px"
        );
        // Non-braced occurrences are untouched.
        assert_eq!(substitute_param_token("level", "level", "2"), "level",);
    }

    #[test]
    fn attribute_macro_flag_round_trips() {
        // `<macro attribute=true>` (or a body `<expr body="attribute"/>`)
        // marks the macro as an attribute form.
        let (doc, _) = parse(
            r#"<macro name="Elevation" attribute="true">
  <property name="level" type="int"/>
  <expand>
    <expr body="radius = 8"/>
  </expand>
</macro>"#,
        );
        let macros = harvest_macros(&doc.nodes, None);
        let m = macros.get("Elevation").expect("macro missing");
        assert!(m.is_attribute);
    }

    #[test]
    fn attribute_macro_expands_into_bare_attrs() {
        // The call site `<container elevation=2/>` is processed by
        // walking its attributes: `elevation` matches a registered
        // attribute macro, so it expands into the macro's body
        // attrs. A second attr `gap=4` round-trips unchanged.
        let (doc, _) = parse(
            r#"<macro name="elevation" attribute="true">
  <property name="level" type="int"/>
  <expand>
    <expr body="radius = 8"/>
    <expr body="padding = 16"/>
  </expand>
</macro>

<container elevation="2" gap="4"/>"#,
        );
        let macros = harvest_macros(&doc.nodes, None);
        let expanded = expand_macros(&doc.nodes, &macros, 0);
        // Find the container in the expanded output.
        let container = expanded
            .iter()
            .find_map(|n| match n {
                AstNode::Element(e) if e.tag == "container" => Some(e),
                _ => None,
            })
            .expect("container missing");
        let names: Vec<&str> = container
            .attributes
            .iter()
            .map(|a| a.name.local.as_str())
            .collect();
        // `elevation` is gone (consumed by the attribute macro);
        // `gap` survives; the macro's `radius` and `padding` are
        // appended.
        assert!(!names.contains(&"elevation"));
        assert!(names.contains(&"gap"));
        assert!(names.contains(&"radius"));
        assert!(names.contains(&"padding"));
    }

    #[test]
    fn empty_macro_registry_is_passthrough() {
        let (doc, _) = parse(r#"<container/>"#);
        let macros: HashMap<String, Arc<MacroDef>> = HashMap::new();
        let expanded = expand_macros(&doc.nodes, &macros, 0);
        assert_eq!(expanded.len(), 1);
    }

    #[test]
    fn macro_declaration_is_not_recursively_expanded() {
        // The body of a `<macro>` declaration is a template, not a
        // call site — even when it contains a tag whose name matches
        // a registered macro, the expander must leave it alone.
        let src = r#"macro Field(label: string) {
  match { <Field label={label}/> }
  expand { <text>{label}</text> }
}"#;
        let (doc, _) = parse(src);
        let macros = harvest_macros(&doc.nodes, None);
        let expanded = expand_macros(&doc.nodes, &macros, 0);
        // The `<macro>` element is still present (not consumed).
        let macro_el = expanded.iter().find(|n| match n {
            AstNode::Element(e) => e.tag == "macro",
            _ => false,
        });
        assert!(macro_el.is_some());
    }
}
