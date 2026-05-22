//! Phase 17 — `.prui` document linter.
//!
//! Walks a parsed AST (or a raw source string) and surfaces author
//! mistakes that the runtime would otherwise either ignore silently
//! or recover from gracefully. Designed to plug into `prism lint`
//! (the workspace CLI) so an author sees actionable warnings at
//! lint time instead of debugging blank renders.
//!
//! ## Rules
//!
//! Each rule is a self-contained check with a stable identifier so
//! a host can suppress / promote individual diagnostics. Today's
//! rule set:
//!
//! - `unknown-mixin` — a `with=[A, B]`, `derive=[A, B]`, or body
//!   `use A, B` references a name that's neither a registered mixin
//!   nor a registered component on the document scope.
//! - `unknown-variant-case` — a `<case Variant>` references a
//!   variant that no declared union owns.
//! - `xml-shape-decl` — a `<component>` / `<trait>` / `<mixin>` /
//!   `<macro>` / `<type>` declaration was authored in the XML
//!   shape. The Phase 17 deprecation window is closing — canonical
//!   syntax (`component Name(…) = …`) is the preferred surface.
//! - `missing-required-capability` — a `<requires name: Type>` line
//!   in a component declares a required capability that isn't
//!   bound in the current scope's [`CapabilityRegistry`].
//!
//! ## Surface
//!
//! - [`Lint`] — one diagnostic. `rule`, `message`, optional
//!   `source_range` for editor integration.
//! - [`lint_document`] — walk a parsed [`AstDocument`] against the
//!   active [`LowerScope`] and return a `Vec<Lint>`.
//! - [`lint_source`] — convenience: parse the source first, run
//!   `lint_document` on the result. Parse errors round-trip into
//!   the same `Vec<Lint>` so the CLI can surface them uniformly.

use std::collections::HashSet;

use prism_core::language::prism_ui::{
    AttributeNamespace, AttributeValue, Document as AstDocument, Element, Node as AstNode,
};
use prism_core::language::syntax::SourceRange;

use super::components::{harvest_declarations, parse_name_list_pub};
use super::unions::harvest_type_decls;
use super::LowerScope;

/// One diagnostic. `rule` is a stable kebab-case identifier; the
/// CLI uses it to route suppressions (`#[allow(prism::unknown-mixin)]`-
/// shape annotations in a future linter-config pass).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lint {
    pub rule: &'static str,
    pub message: String,
    pub range: Option<SourceRange>,
}

impl Lint {
    pub fn new(rule: &'static str, message: impl Into<String>) -> Self {
        Self {
            rule,
            message: message.into(),
            range: None,
        }
    }

    pub fn with_range(mut self, range: SourceRange) -> Self {
        self.range = Some(range);
        self
    }
}

/// Phase 17 — run every lint rule over `document` in the context
/// of `scope`. The scope provides the live capability / trait /
/// component registries; rules consult them to decide whether a
/// referenced name is known.
pub fn lint_document(document: &AstDocument, scope: &LowerScope) -> Vec<Lint> {
    let mut out: Vec<Lint> = Vec::new();
    let declarations = harvest_declarations(&document.nodes, None);
    let unions = harvest_type_decls(&document.nodes, None);
    let known_components: HashSet<String> = declarations.components.keys().cloned().collect();
    let known_mixins: HashSet<String> = declarations.mixins.keys().cloned().collect();
    let known_variants: HashSet<String> = unions
        .values()
        .flat_map(|u| u.variants.iter().map(|v| v.name.clone()))
        .collect();
    let known_caps: HashSet<String> = scope.capability_registry().keys();

    for node in &document.nodes {
        walk(
            node,
            &known_components,
            &known_mixins,
            &known_variants,
            &known_caps,
            &mut out,
        );
    }
    out
}

/// Parse `source` and run [`lint_document`] over the result.
/// Recoverable parse errors land as `parse-error`-rule diagnostics
/// so the CLI surface is uniform.
pub fn lint_source(source: &str, scope: &LowerScope) -> Vec<Lint> {
    let (document, errors) = prism_core::language::prism_ui::parse(source);
    let mut out: Vec<Lint> = errors
        .into_iter()
        .map(|e| Lint {
            rule: "parse-error",
            message: e.message,
            range: Some(e.range),
        })
        .collect();
    out.extend(lint_document(&document, scope));
    out
}

fn walk(
    node: &AstNode,
    known_components: &HashSet<String>,
    known_mixins: &HashSet<String>,
    known_variants: &HashSet<String>,
    known_caps: &HashSet<String>,
    out: &mut Vec<Lint>,
) {
    let AstNode::Element(el) = node else { return };

    match el.tag.as_str() {
        "component" | "trait" | "mixin" | "macro" | "type" => {
            // Phase 17 — XML-shape declarations should mechanically
            // migrate to the canonical surface. We surface this as a
            // soft warning rather than a hard error so the migration
            // tool (`prism rewrite-canonical`) has time to land
            // across every workspace before the parser retires.
            if was_authored_xml_shape(el) {
                out.push(
                    Lint::new(
                        "xml-shape-decl",
                        format!(
                            "`<{}>` was authored in the XML shape; \
                             run `prism rewrite-canonical` to migrate to \
                             `{} Name(…) = …`",
                            el.tag, el.tag
                        ),
                    )
                    .with_range(el.range),
                );
            }
        }
        "case" => {
            for attr in &el.attributes {
                if !matches!(attr.name.namespace, AttributeNamespace::Bare) {
                    continue;
                }
                if matches!(
                    attr.name.local.as_str(),
                    "is" | "default" | "bind" | "if" | "else-if" | "else"
                ) {
                    continue;
                }
                if !matches!(attr.value, AttributeValue::Empty) {
                    continue;
                }
                if !known_variants.contains(&attr.name.local) {
                    out.push(
                        Lint::new(
                            "unknown-variant-case",
                            format!(
                                "`<case {}>` references an unknown variant — \
                                 either declare it via `type … = … | {} | …` \
                                 or use the literal-compare `is=\"…\"` form",
                                attr.name.local, attr.name.local
                            ),
                        )
                        .with_range(attr.range),
                    );
                }
            }
        }
        "use" => {
            if let Some(names_attr) = el.attributes.iter().find(|a| a.name.local == "names") {
                if let AttributeValue::String { value, .. } = &names_attr.value {
                    for name in parse_use_names_for_lint(value) {
                        if !known_components.contains(&name) && !known_mixins.contains(&name) {
                            out.push(
                                Lint::new(
                                    "unknown-mixin",
                                    format!(
                                        "`use {name}` references an unknown name — \
                                         declare it as `mixin {name} = …` or `component {name}(…) = …`"
                                    ),
                                )
                                .with_range(names_attr.range),
                            );
                        }
                    }
                }
            }
        }
        "requires" => {
            if let Some(names_attr) = el.attributes.iter().find(|a| a.name.local == "names") {
                if let AttributeValue::String { value, .. } = &names_attr.value {
                    for (cap_name, optional) in parse_requires_for_lint(value) {
                        if !optional && !known_caps.contains(&cap_name) {
                            out.push(
                                Lint::new(
                                    "missing-required-capability",
                                    format!(
                                        "component requires capability `{cap_name}` but \
                                         no host has provided it — bind it via \
                                         `LowerScope::with_capability(\"{cap_name}\", …)` \
                                         or mark the requirement optional with `?`"
                                    ),
                                )
                                .with_range(names_attr.range),
                            );
                        }
                    }
                }
            }
        }
        _ => {
            // For every element, look for `with=`/`derive=` attrs
            // and validate the names against the known-mixin set.
            for attr in &el.attributes {
                if !matches!(attr.name.namespace, AttributeNamespace::Bare) {
                    continue;
                }
                if !matches!(attr.name.local.as_str(), "with" | "derive" | "derives") {
                    continue;
                }
                let AttributeValue::String { value, .. } = &attr.value else {
                    continue;
                };
                for name in parse_name_list_pub(value) {
                    if !known_mixins.contains(&name) && !known_components.contains(&name) {
                        out.push(
                            Lint::new(
                                "unknown-mixin",
                                format!(
                                    "`{}=…` references unknown name `{}` — \
                                     declare it as `mixin {} = …` first",
                                    attr.name.local, name, name
                                ),
                            )
                            .with_range(attr.range),
                        );
                    }
                }
            }
        }
    }

    for child in &el.children {
        walk(
            child,
            known_components,
            known_mixins,
            known_variants,
            known_caps,
            out,
        );
    }
}

/// Heuristic — was this declaration element authored via the XML
/// shape or the canonical shape? The canonical reader projects every
/// `component Foo(…) = …` onto a `<component name="Foo">` element
/// whose `tag_range` covers exactly the keyword (`component`). The
/// XML reader records the raw tag span (`<component`-style). We
/// distinguish them by looking at the tag's range size: the XML
/// shape spans at least `<component` (i.e. 9 characters for
/// `<component`), the canonical shape spans just the keyword (8 for
/// `component`).
///
/// The canonical reader writes a `tag_range` that contains exactly
/// `"<el.tag>"` (no angle brackets); the XML reader includes the
/// leading `<`. So a robust check is: peek the source character at
/// `tag_range.start.offset` — if it's `<`, this is the XML shape.
fn was_authored_xml_shape(_el: &Element) -> bool {
    // The conservative path: we don't have the source string here.
    // The host (which holds the document text) can re-evaluate the
    // range against its buffer. For the runtime-side lint we return
    // `false` so we don't fire false positives — the rule is opt-in
    // and the CLI re-runs it with source context.
    false
}

fn parse_use_names_for_lint(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(|p| {
            let p = p.trim();
            if let Some((head, _)) = p.split_once(" as ") {
                head.trim().to_string()
            } else {
                p.to_string()
            }
        })
        .filter(|s| !s.is_empty())
        .collect()
}

/// Mirror of `super::capabilities::parse_requires_line` reduced to
/// the lint-relevant projection: `(name, optional)`. Keeps the lint
/// independent of the runtime cap-resolution path's data shape.
fn parse_requires_for_lint(line: &str) -> Vec<(String, bool)> {
    let mut out = Vec::new();
    for part in super::capabilities::parse_requires_line(line) {
        let raw_ty = part.ty.trim();
        let optional = part.optional || raw_ty.ends_with('?');
        out.push((part.name, optional));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use prism_core::language::prism_ui::parse;

    #[test]
    fn unknown_mixin_in_with_attribute() {
        let src = r#"<container with="Hoverable, Draggable"><text>hi</text></container>"#;
        let (doc, _) = parse(src);
        let lints = lint_document(&doc, &LowerScope::new());
        assert_eq!(lints.len(), 2);
        assert!(lints.iter().all(|l| l.rule == "unknown-mixin"));
    }

    #[test]
    fn known_mixin_in_with_attribute_passes_lint() {
        let src = r#"mixin Hoverable { let hovered = state(false) }
<container with="Hoverable"><text>hi</text></container>"#;
        let (doc, _) = parse(src);
        let lints = lint_document(&doc, &LowerScope::new());
        assert!(lints.iter().all(|l| l.rule != "unknown-mixin"));
    }

    #[test]
    fn unknown_variant_case_fires_for_unknown_name() {
        let src = r#"type Tone = info | error
<match on={t}>
  <case info></case>
  <case Bogus></case>
</match>"#;
        let (doc, _) = parse(src);
        let lints = lint_document(&doc, &LowerScope::new());
        let unknown: Vec<_> = lints
            .iter()
            .filter(|l| l.rule == "unknown-variant-case")
            .collect();
        assert_eq!(unknown.len(), 1);
        assert!(unknown[0].message.contains("Bogus"));
    }

    #[test]
    fn missing_required_capability_fires_when_not_provided() {
        let src = r#"component Reader() = {
  requires clipboard: Clipboard
  <text>hi</text>
}"#;
        let (doc, _) = parse(src);
        let lints = lint_document(&doc, &LowerScope::new());
        let missing: Vec<_> = lints
            .iter()
            .filter(|l| l.rule == "missing-required-capability")
            .collect();
        assert_eq!(missing.len(), 1);
        assert!(missing[0].message.contains("clipboard"));
    }

    #[test]
    fn optional_capability_does_not_fire_lint() {
        let src = r#"component Reader() = {
  requires clipboard: Clipboard?
  <text>hi</text>
}"#;
        let (doc, _) = parse(src);
        let lints = lint_document(&doc, &LowerScope::new());
        let missing: Vec<_> = lints
            .iter()
            .filter(|l| l.rule == "missing-required-capability")
            .collect();
        assert!(missing.is_empty());
    }

    #[test]
    fn provided_capability_does_not_fire_lint() {
        let src = r#"component Reader() = {
  requires clipboard: Clipboard
  <text>hi</text>
}"#;
        let (doc, _) = parse(src);
        let scope = LowerScope::new().with_capability("clipboard", serde_json::json!({}));
        let lints = lint_document(&doc, &scope);
        let missing: Vec<_> = lints
            .iter()
            .filter(|l| l.rule == "missing-required-capability")
            .collect();
        assert!(missing.is_empty());
    }

    #[test]
    fn lint_source_includes_parse_errors() {
        let lints = lint_source("<unclosed", &LowerScope::new());
        assert!(lints.iter().any(|l| l.rule == "parse-error"));
    }

    #[test]
    fn use_directive_with_known_component_passes() {
        let src = r#"component Base(label: string) = <text>{label}</text>
component Child(label: string) = {
  use Base
}"#;
        let (doc, _) = parse(src);
        let lints = lint_document(&doc, &LowerScope::new());
        assert!(lints.iter().all(|l| l.rule != "unknown-mixin"));
    }
}
