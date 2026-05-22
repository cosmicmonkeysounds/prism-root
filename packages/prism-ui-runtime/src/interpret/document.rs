//! Document-walking helpers lowered out of `interpret/mod.rs` during
//! Phase 0. See `docs/dev/prui-expressiveness-roadmap.md` §7.0. Pure
//! code move, no behaviour change. Holds the static pre-scan that
//! `lower_document_with_scope` runs over an `AstDocument` before the
//! per-element walk: collects inline scripts/stylesheets, parses
//! `<import>` rows, resolves the `require()` graph for Luau, and
//! gathers `<teleport>` retargets keyed by destination tag.

use std::collections::HashMap;

use prism_core::language::prism_ui::{
    AttributeNamespace, AttributeValue, Element, Node as AstNode,
};

#[cfg(feature = "luau")]
use super::ImportResolver;

/// **Wave A / §5.10** — collect the raw body of every top-level
/// `<script>` element, in source order. `<script>` *is* Luau —
/// there is no `lang=` selector (the grammar flags one as a
/// recoverable diagnostic), so every block feeds the Lua state. The
/// grammar parses a `<script>` block as a raw-text element (one
/// [`AstNode::Text`] child); multiple inline blocks concatenate at
/// the [`crate::luau_scope::LuauScopeFrame`] seam.
#[cfg(feature = "luau")]
pub(super) fn collect_script_bodies(nodes: &[AstNode]) -> Vec<String> {
    let mut out = Vec::new();
    for node in nodes {
        let AstNode::Element(el) = node else { continue };
        if el.tag != "script" {
            continue;
        }
        // **§5.10** — `<script>` *is* Luau; there is no `lang=`
        // selector any more (the grammar flags it as a recoverable
        // diagnostic). Every `<script>` body feeds the Lua state.
        for child in &el.children {
            if let AstNode::Text { value, .. } = child {
                out.push(value.clone());
            }
        }
    }
    out
}

/// **Wave H / §5.10 (`prui-luau-fusion.md`)** — collect the body of
/// every top-level inline `<style>` block, in source order.
/// `<style>` *is* PRSS — no `lang=` selector. Grammar parses
/// `<style>` as raw-text (one [`AstNode::Text`] child).
pub(super) fn collect_inline_stylesheets(nodes: &[AstNode]) -> Vec<String> {
    let mut out = Vec::new();
    for node in nodes {
        let AstNode::Element(el) = node else { continue };
        if el.tag != "style" {
            continue;
        }
        // **§5.10** — `<style>` *is* PRSS; no `lang=` selector.
        for child in &el.children {
            if let AstNode::Text { value, .. } = child {
                out.push(value.clone());
            }
        }
    }
    out
}

/// **Wave H (§5.4 / §5.9 / §5.10)** — a parsed `<import
/// KIND="path"/> [as <alias>]` row. `kind` is one of the three live
/// projections; `alias` carries the postfix `as` namespace (applied
/// for `script` imports — §5.9 tier 2 — parsed-only for the others).
pub(super) struct ImportSpec {
    pub(super) kind: String,
    pub(super) path: String,
    /// Only consumed by the `#[cfg(feature = "luau")]` script-import
    /// path (§5.9 tier 2); the HTML/SSR build never namespaces Luau.
    #[cfg_attr(not(feature = "luau"), allow(dead_code))]
    pub(super) alias: Option<String>,
}

/// **Wave H (§5.4) + Phase 8 (§7.11)** — collect every `<import>`
/// row. The `kind`/path is the first attribute whose local name is
/// one of the four live projections (`stylesheet` / `script` /
/// `dialect` / `component`); an `as=` attribute on the same element
/// supplies the alias. The earlier `widget=` projection was renamed
/// to `component=` in Phase 8 — the parser emits `component=` for
/// every `.prui` import and `widget=` is no longer recognised at the
/// projection level (the `<import widget=…/>` collector path is
/// gone with this rename).
pub(super) fn collect_imports(nodes: &[AstNode]) -> Vec<ImportSpec> {
    let mut out = Vec::new();
    for node in nodes {
        let AstNode::Element(el) = node else { continue };
        if el.tag != "import" {
            continue;
        }
        let alias = el.attributes.iter().find_map(|a| {
            (a.name.local == "as").then(|| match &a.value {
                AttributeValue::String { value, .. } => Some(value.clone()),
                _ => None,
            })?
        });
        for a in &el.attributes {
            if matches!(
                a.name.local.as_str(),
                "stylesheet" | "script" | "dialect" | "component"
            ) {
                if let AttributeValue::String { value, .. } = &a.value {
                    out.push(ImportSpec {
                        kind: a.name.local.clone(),
                        path: value.clone(),
                        alias: alias.clone(),
                    });
                }
            }
        }
    }
    out
}

/// **Wave H.7 (`prui-luau-fusion.md` §5.9 tier 3)** — extract every
/// literal `require("…")` / `require('…')` path from a Luau source,
/// skipping comments and string/long-string bodies so a `require`
/// token inside prose or a quote is never mistaken for a call. The
/// Lua expression form `require(expr)` (non-literal) is intentionally
/// not followed — only static, resolver-checkable paths participate
/// (design principle 1: the graph is sized up front).
#[cfg(feature = "luau")]
fn scan_require_literals(src: &str) -> Vec<String> {
    use prism_core::language::syntax::Scanner;
    // Consume through a `]]` long-bracket close (block comment / long
    // string terminator), or EOF.
    fn skip_to_long_close(sc: &mut Scanner) {
        loop {
            if sc.is_at_end() {
                break;
            }
            if sc.peek() == Some(']') && sc.peek_ahead(1) == Some(']') {
                sc.advance();
                sc.advance();
                break;
            }
            sc.advance();
        }
    }
    let mut sc = Scanner::new(src);
    let mut out = Vec::new();
    while let Some(c) = sc.peek() {
        // `--` line / `--[[ ]]` block comment.
        if c == '-' && sc.peek_ahead(1) == Some('-') {
            sc.advance();
            sc.advance();
            if sc.peek() == Some('[') && sc.peek_ahead(1) == Some('[') {
                sc.advance();
                sc.advance();
                skip_to_long_close(&mut sc);
            } else {
                while let Some(ch) = sc.peek() {
                    if ch == '\n' {
                        break;
                    }
                    sc.advance();
                }
            }
            continue;
        }
        // `[[ … ]]` long string.
        if c == '[' && sc.peek_ahead(1) == Some('[') {
            sc.advance();
            sc.advance();
            skip_to_long_close(&mut sc);
            continue;
        }
        // `"…"` / `'…'` short string.
        if c == '"' || c == '\'' {
            sc.advance();
            while let Some(ch) = sc.advance() {
                if ch == '\\' {
                    sc.advance();
                    continue;
                }
                if ch == c {
                    break;
                }
            }
            continue;
        }
        if c.is_ascii_alphabetic() || c == '_' {
            let word = sc
                .scan_while(|x| x.is_ascii_alphanumeric() || x == '_')
                .to_string();
            if word == "require" {
                while matches!(sc.peek(), Some(' ' | '\t' | '\n' | '\r')) {
                    sc.advance();
                }
                if sc.peek() == Some('(') {
                    sc.advance();
                    while matches!(sc.peek(), Some(' ' | '\t' | '\n' | '\r')) {
                        sc.advance();
                    }
                    if let Some(q @ ('"' | '\'')) = sc.peek() {
                        sc.advance();
                        let mut p = String::new();
                        while let Some(ch) = sc.advance() {
                            if ch == '\\' {
                                if let Some(n) = sc.advance() {
                                    p.push(n);
                                }
                                continue;
                            }
                            if ch == q {
                                break;
                            }
                            p.push(ch);
                        }
                        if !p.is_empty() {
                            out.push(p);
                        }
                    }
                }
            }
            continue;
        }
        sc.advance();
    }
    out
}

/// **Wave H.7** — depth-first, deps-first transitive resolution of
/// the `require` graph through the host [`ImportResolver`] (kind
/// `script`, the same seam `<import script>` uses). Each module is
/// emitted *after* its dependencies and *exactly once* (keyed by the
/// literal path); a re-entered path (cycle) is skipped — its
/// `require` then hits the not-yet-cached branch and errors, so a
/// structural cycle is a bounded load error, never a hang.
#[cfg(feature = "luau")]
pub(super) fn resolve_require_graph(
    roots: &[&str],
    res: &dyn ImportResolver,
) -> Vec<(String, String)> {
    use std::collections::HashSet;
    fn visit(
        path: &str,
        res: &dyn ImportResolver,
        out: &mut Vec<(String, String)>,
        done: &mut HashSet<String>,
        stack: &mut HashSet<String>,
    ) {
        if done.contains(path) || stack.contains(path) {
            return;
        }
        let Some(src) = res.resolve_import("script", path) else {
            return;
        };
        stack.insert(path.to_string());
        for dep in scan_require_literals(&src) {
            visit(&dep, res, out, done, stack);
        }
        stack.remove(path);
        out.push((path.to_string(), src));
        done.insert(path.to_string());
    }
    let mut out = Vec::new();
    let mut done = HashSet::new();
    let mut stack = HashSet::new();
    for root in roots {
        for dep in scan_require_literals(root) {
            visit(&dep, res, &mut out, &mut done, &mut stack);
        }
    }
    out
}

/// **Wave B** — does any expression body in the document use the
/// closure (`|x| …`, `\fn(x) …`) or pipe (`|>`, ` | `) sigils?
/// Drives lazy provisioning of an empty Luau frame for script-less
/// documents. Scans only parsed expression text (attribute values,
/// interpolations, control-flow predicates) — never literal text
/// runs — and strips `||` first so a plain logical-or never
/// triggers a Lua state.
/// **Wave E.3 (§7.8)** — does the document contain a `<language>`
/// element (the desugar target of a `~name{…}` sigil too)? Gates
/// whether host builtin dialect scripts are worth a Lua state.
#[cfg(feature = "luau")]
pub(super) fn document_uses_dialect(nodes: &[AstNode]) -> bool {
    nodes.iter().any(|n| match n {
        AstNode::Element(el) => el.tag == "language" || document_uses_dialect(&el.children),
        _ => false,
    })
}

#[cfg(feature = "luau")]
pub(super) fn document_uses_luau_expr(nodes: &[AstNode]) -> bool {
    fn expr_has_sigil(body: &str) -> bool {
        let stripped = body.replace("||", "");
        stripped.contains("\\fn") || stripped.contains('|')
    }
    fn attr_has(value: &AttributeValue) -> bool {
        match value {
            AttributeValue::Expression(e) => expr_has_sigil(&e.body),
            AttributeValue::Template { parts, .. } => parts.iter().any(|p| match p {
                prism_core::language::prism_ui::ast::TemplatePart::Expression(e) => {
                    expr_has_sigil(&e.body)
                }
                prism_core::language::prism_ui::ast::TemplatePart::Literal { .. } => false,
            }),
            _ => false,
        }
    }
    nodes.iter().any(|node| match node {
        AstNode::Element(el) => {
            el.attributes.iter().any(|a| attr_has(&a.value))
                || document_uses_luau_expr(&el.children)
        }
        AstNode::Interpolation(e) => expr_has_sigil(&e.body),
        _ => false,
    })
}

/// **Wave 14.3** — recursive AST scan that collects every
/// `<teleport to="X">` element's children into a `target → AST
/// payload` map. Walks normal element children but stops at
/// `<teleport>` so a payload that itself contains a `<teleport>` is
/// kept verbatim (the inner teleport is re-discovered when the
/// payload is appended at the outer target and lowered through the
/// same `lower_document_with_scope` recursion — see the teleport
/// handler in `lower_element`).
/// Build a synthetic `Element` whose tag is `new_tag` and whose
/// attributes are `el`'s minus the `tag=` slot the runtime consumed
/// for dispatch routing. Used by `<dispatch tag="{expr}"/>` so the
/// rewritten element flows through the same lowering path an authored
/// element would have. Range data is copied verbatim — diagnostics
/// retain the original source span.
pub(super) fn element_with_retagged(el: &Element, new_tag: &str) -> Element {
    let attributes = el
        .attributes
        .iter()
        .filter(|attr| {
            !(matches!(attr.name.namespace, AttributeNamespace::Bare) && attr.name.local == "tag")
        })
        .cloned()
        .collect();
    Element {
        tag: new_tag.to_string(),
        attributes,
        children: el.children.clone(),
        self_closing: el.self_closing,
        range: el.range,
        tag_range: el.tag_range,
    }
}

pub(super) fn collect_teleports(nodes: &[AstNode], out: &mut HashMap<String, Vec<AstNode>>) {
    for node in nodes {
        let AstNode::Element(el) = node else { continue };
        if el.tag == "teleport" {
            if let Some(target) = teleport_target(el) {
                out.entry(target)
                    .or_default()
                    .extend(el.children.iter().cloned());
            }
            // Don't recurse into a teleport's children — they're the
            // payload, not search targets for nested teleports.
            continue;
        }
        collect_teleports(&el.children, out);
    }
}

/// **Wave 14.3** — extract the literal `to="X"` attribute from a
/// `<teleport>` element. Only literal-string targets are recognised
/// today; expression-valued `to="{…}"` rounds-trip without routing
/// (the pre-scan has no scope to evaluate against). Authors who need
/// a dynamic destination wrap multiple `<teleport>` elements in an
/// `if=` chain instead.
fn teleport_target(el: &Element) -> Option<String> {
    for attr in &el.attributes {
        if matches!(attr.name.namespace, AttributeNamespace::Bare) && attr.name.local == "to" {
            if let AttributeValue::String { value, .. } = &attr.value {
                let trimmed = value.trim();
                if !trimmed.is_empty() {
                    return Some(trimmed.to_string());
                }
            }
        }
    }
    None
}
