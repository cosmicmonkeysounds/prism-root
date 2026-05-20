//! Control-flow + match/suspense/language desugaring lowered out of
//! `interpret/mod.rs` during Phase 0 (see
//! `docs/dev/prui-expressiveness-roadmap.md` §7.0). The functions here
//! were originally defined inline; this module is a pure code move,
//! no behaviour change. The five entry points (`expand_match`,
//! `expand_suspense`, `expand_language`, `expand_control_flow`,
//! `resolve_for_iteration`) are `pub(super)` so the lower-element pass
//! in the parent module can call them by their unqualified name once
//! the surrounding `use control_flow::*` import is in place.

use prism_core::language::prism_ui::ast::{Attribute, AttributeName, Expression};
#[cfg(feature = "luau")]
use prism_core::language::prism_ui::parse;
use prism_core::language::prism_ui::{
    AttributeNamespace, AttributeValue, Element, Node as AstNode,
};
use prism_core::language::syntax::SourceRange;

#[cfg(feature = "luau")]
use super::lower_document_with_scope;
use super::{
    attribute_string, bare_attr_value, eval_truthy, evaluate_bare_attr_typed, evaluate_expression,
    lookup_path_owned, lower_children, LowerScope, Node,
};

/// Build a synthetic control-flow [`Attribute`] (`if` / `else-if` /
/// `else`) carrying `body` as a `{expr}` value. `range` is the
/// `<match>` element's span so diagnostics point at the sugar site.
fn cf_attr(local: &str, body: Option<&str>, range: SourceRange) -> Attribute {
    let value = match body {
        Some(b) => AttributeValue::Expression(Expression {
            body: b.to_string(),
            range,
        }),
        None => AttributeValue::Empty,
    };
    Attribute {
        name: AttributeName {
            raw: local.to_string(),
            local: local.to_string(),
            namespace: AttributeNamespace::ControlFlow,
            range,
        },
        value,
        range,
    }
}

/// Wrap `children` in a synthetic `<fragment>` element carrying one
/// control-flow attribute — the unit a `<case>` rewrites to.
fn cf_fragment(cf: Attribute, children: Vec<AstNode>, range: SourceRange) -> AstNode {
    AstNode::Element(Element {
        tag: "fragment".to_string(),
        attributes: vec![cf],
        children,
        self_closing: false,
        range,
        tag_range: range,
    })
}

/// **Wave D (§7.5)** — rewrite `<match on="{X}">` + `<case>` children
/// into a `<let>`-bound chained `if`/`else-if`/`else` and lower it.
///
/// - `<case is="lit">` → `__match == 'lit'` (string literal) or
///   `__match == (expr)` when `is="{expr}"`.
/// - An extra `if="{cond}"` on the case narrows: `(eq) and (cond)`.
/// - `<case default>` → the `else` arm (must be last; later cases
///   are unreachable and dropped, matching first-match-wins).
///
/// The matched expression is evaluated exactly once (the synthetic
/// `<let>`), so side-effect-free but non-trivial `on=` expressions
/// don't re-run per case.
pub(super) fn expand_match(el: &Element, scope: &LowerScope) -> Vec<Node> {
    let range = el.range;
    // The matched expression text (no surrounding braces).
    let on_body = el.attributes.iter().find_map(|a| {
        if a.name.local != "on" {
            return None;
        }
        match &a.value {
            AttributeValue::Expression(e) => Some(e.body.clone()),
            AttributeValue::String { value, .. } => Some(value.clone()),
            _ => None,
        }
    });
    let Some(on_body) = on_body else {
        return Vec::new();
    };
    // Unique binding name so nested `<match>` don't collide.
    let bind = format!("__match_{}", range.start.offset);

    let mut synthetic: Vec<AstNode> = Vec::new();
    // `<let name="bind" value="{on_body}"/>`
    synthetic.push(AstNode::Element(Element {
        tag: "let".to_string(),
        attributes: vec![
            Attribute {
                name: AttributeName {
                    raw: "name".into(),
                    local: "name".into(),
                    namespace: AttributeNamespace::Bare,
                    range,
                },
                value: AttributeValue::String {
                    value: bind.clone(),
                    range,
                },
                range,
            },
            Attribute {
                name: AttributeName {
                    raw: "value".into(),
                    local: "value".into(),
                    namespace: AttributeNamespace::Bare,
                    range,
                },
                value: AttributeValue::Expression(Expression {
                    body: on_body,
                    range,
                }),
                range,
            },
        ],
        children: Vec::new(),
        self_closing: true,
        range,
        tag_range: range,
    }));

    let mut first = true;
    let mut seen_default = false;
    for child in &el.children {
        let AstNode::Element(case) = child else {
            continue;
        };
        if case.tag != "case" || seen_default {
            // Non-`<case>` children and anything after `<case
            // default>` are unreachable — first-match-wins.
            continue;
        }
        let is_default = case
            .attributes
            .iter()
            .any(|a| a.name.local == "default" && matches!(a.value, AttributeValue::Empty));
        // Optional narrowing `if="{cond}"` on the case.
        let extra = case.attributes.iter().find_map(|a| {
            if matches!(a.name.namespace, AttributeNamespace::ControlFlow) && a.name.local == "if" {
                attribute_string(&a.value)
            } else {
                None
            }
        });
        let extra = extra.map(|s| {
            s.trim()
                .trim_start_matches('{')
                .trim_end_matches('}')
                .trim()
                .to_string()
        });

        let cf = if is_default {
            seen_default = true;
            cf_attr("else", None, range)
        } else {
            // `is=` literal vs expression.
            let rhs = case.attributes.iter().find_map(|a| {
                if a.name.local != "is" {
                    return None;
                }
                match &a.value {
                    AttributeValue::String { value, .. } => {
                        Some(format!("'{}'", value.replace('\'', "\\'")))
                    }
                    AttributeValue::Expression(e) => Some(format!("({})", e.body)),
                    _ => None,
                }
            });
            let Some(rhs) = rhs else { continue };
            let mut pred = format!("{bind} == {rhs}");
            if let Some(extra) = &extra {
                if !extra.is_empty() {
                    pred = format!("({pred}) and ({extra})");
                }
            }
            if first {
                cf_attr("if", Some(&pred), range)
            } else {
                cf_attr("else-if", Some(&pred), range)
            }
        };
        first = false;
        synthetic.push(cf_fragment(cf, case.children.clone(), range));
    }

    lower_children(&synthetic, scope)
}

/// **Wave D (§7.6)** — `<suspense>` lowering. The first
/// `<fallback>` child is the placeholder; the remaining children
/// are the primary subtree. If any `{expr}` slot the primary
/// subtree reads resolves to a *pending* marker
/// (`{ tag = "Pending" }`, the shape a `prism.objects:query_async`
/// coroutine binding carries before it resolves), the fallback is
/// rendered; otherwise the primary subtree renders (fallback
/// stripped). Full coroutine scheduling is open question 3 — this
/// is the lowering-time swap that makes the boundary observable.
pub(super) fn expand_suspense(el: &Element, scope: &LowerScope) -> Vec<Node> {
    let mut fallback: Vec<AstNode> = Vec::new();
    let mut primary: Vec<AstNode> = Vec::new();
    for child in &el.children {
        match child {
            AstNode::Element(c) if c.tag == "fallback" => {
                fallback.extend(c.children.iter().cloned());
            }
            other => primary.push(other.clone()),
        }
    }
    if subtree_has_pending(&primary, scope) {
        lower_children(&fallback, scope)
    } else {
        lower_children(&primary, scope)
    }
}

/// Does any expression referenced by `nodes` resolve to a pending
/// coroutine marker? Walks attribute values, interpolations, and
/// `for=` / `if=` predicates — a `None`-resolving binding is *not*
/// pending (that's just absent data); only an explicit
/// `{ tag = "Pending" }` object trips the fallback.
fn subtree_has_pending(nodes: &[AstNode], scope: &LowerScope) -> bool {
    fn is_pending(v: &serde_json::Value) -> bool {
        v.get("tag").and_then(|t| t.as_str()) == Some("Pending")
    }
    fn expr_pending(body: &str, scope: &LowerScope) -> bool {
        let b = body
            .trim()
            .trim_start_matches('{')
            .trim_end_matches('}')
            .trim();
        // Whole-expression value (covers `for="t in async_tasks"`
        // where the source *is* the pending object).
        if lookup_path_owned(b, scope)
            .or_else(|| evaluate_expression(b, scope))
            .as_ref()
            .map(is_pending)
            .unwrap_or(false)
        {
            return true;
        }
        // Root binding of a dotted path: `{async_tasks.tag}` is
        // pending if `async_tasks` itself is the pending marker —
        // reading *into* an unresolved coroutine still suspends.
        let head: String = b
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        if head.is_empty() || head.len() == b.len() {
            return false;
        }
        scope
            .binding(&head)
            .map(is_pending)
            .or_else(|| lookup_path_owned(&head, scope).as_ref().map(is_pending))
            .unwrap_or(false)
    }
    nodes.iter().any(|n| match n {
        AstNode::Interpolation(e) => expr_pending(&e.body, scope),
        AstNode::Element(el) => {
            el.attributes.iter().any(|a| match &a.value {
                AttributeValue::Expression(e) => expr_pending(&e.body, scope),
                AttributeValue::Template { parts, .. } => parts.iter().any(|p| match p {
                    prism_core::language::prism_ui::ast::TemplatePart::Expression(e) => {
                        expr_pending(&e.body, scope)
                    }
                    _ => false,
                }),
                _ => false,
            }) || subtree_has_pending(&el.children, scope)
        }
        _ => false,
    })
}

/// **Wave E (§7.8)** — expand a `<language name="x">body</language>`
/// block. The body (one raw [`AstNode::Text`] child, the grammar
/// parses `<language>` as raw-text) is handed to the dialect's
/// `parse(source)`; the returned `prui[[…]]` source is parsed and
/// lowered in the **current** scope (a dialect is authored inline
/// at the call site, so `{tokens.*}` / call-site bindings resolve —
/// unlike a macro, which is hygienic).
#[cfg(feature = "luau")]
pub(super) fn expand_language(el: &Element, scope: &LowerScope) -> Vec<Node> {
    let Some(frame) = scope.luau_scope() else {
        return Vec::new();
    };
    let name = el.attributes.iter().find_map(|a| {
        if a.name.local == "name" {
            match &a.value {
                AttributeValue::String { value, .. } => Some(value.clone()),
                AttributeValue::Expression(e) => Some(e.body.clone()),
                _ => None,
            }
        } else {
            None
        }
    });
    let Some(name) = name else {
        return Vec::new();
    };
    if !frame.has_dialect(&name) {
        return Vec::new();
    }
    let body: String = el
        .children
        .iter()
        .filter_map(|c| match c {
            AstNode::Text { value, .. } => Some(value.as_str()),
            _ => None,
        })
        .collect();
    let src = match frame.expand_dialect(&name, &body) {
        Some(Ok(s)) => s,
        _ => return Vec::new(),
    };
    let (doc, errors) = parse(&src);
    if !errors.is_empty() {
        return Vec::new();
    }
    lower_document_with_scope(&doc, scope)
}

// ---------------------------------------------------------------------------
// Control-flow expansion
// ---------------------------------------------------------------------------

/// Pre-pass that walks a sibling list and produces a flat
/// `Vec<(node, optional-child-scope)>` of nodes that survive the
/// control-flow gates. `else-if`/`else` chains are evaluated against
/// the most recent `if` predicate; `for` clones the element per item
/// with the iteration variable bound in a fork of the parent scope.
///
/// Returning a per-element optional scope (rather than rewriting the
/// AST) lets the iteration variable bind without cloning the entire
/// tree: the lowering pass picks up the `for`-bound scope only for
/// the element it belongs to, then descends back into the parent
/// scope for unrelated siblings.
pub(super) fn expand_control_flow(
    nodes: &[AstNode],
    scope: &LowerScope,
) -> Vec<(AstNode, Option<LowerScope>)> {
    let mut out: Vec<(AstNode, Option<LowerScope>)> = Vec::with_capacity(nodes.len());
    // Tracks whether the current `if`/`else-if`/`else` chain has
    // already taken a branch. A non-element sibling resets the chain
    // — same rule HTMX / Svelte use.
    let mut chain_taken: Option<bool> = None;
    // **Wave 14.2** — accumulating scope for sibling-level `<let
    // name="X" value="{…}"/>` bindings. Starts as `None` (subsequent
    // siblings see the parent scope unchanged); each `<let/>` clones
    // the running scope, evaluates its expression, and seeds the
    // binding so every subsequent sibling inherits it. Matches
    // Svelte's `{@const}` lexical scope: declaration onward, within
    // the same sibling list.
    let mut let_scope: Option<LowerScope> = None;

    for node in nodes {
        let AstNode::Element(el) = node else {
            // Whitespace-only text between tags is the parser's way of
            // round-tripping source layout — it must not break a
            // sibling `if`/`else-if`/`else` chain. `<!-- … -->`
            // comments are likewise never rendered output (they exist
            // only for formatter round-trip), so an HTML comment
            // between branches must not reset the chain either — this
            // matches the Svelte/HTMX rule that only an intervening
            // *render* node ends conditional grouping. Real text
            // content and `{…}` interpolations are render nodes and do
            // break the chain.
            let breaks_chain = match node {
                AstNode::Text { value, .. } => !value.trim().is_empty(),
                AstNode::Comment { .. } => false,
                _ => true,
            };
            if breaks_chain {
                chain_taken = None;
            }
            out.push((node.clone(), let_scope.clone()));
            continue;
        };
        // **Wave 14.2** — `<let name="X" value="{expr}"/>` evaluated
        // and bound into the running let_scope; doesn't render. Lives
        // before `control_flow_attr` so a `<let if="…"/>` doesn't
        // accidentally fire when the predicate is false (let-bindings
        // are unconditional by design — guard with a sibling `<if/>`
        // wrapper if conditional state is wanted). Typed resolution
        // through [`evaluate_bare_attr_typed`] preserves number /
        // bool / object shape, so a downstream `padding="{total}"`
        // reads through `parse_f32` cleanly.
        if el.tag == "let" {
            let active = let_scope.as_ref().unwrap_or(scope);
            let name = bare_attr_value(el, "name", active);
            let value = evaluate_bare_attr_typed(el, "value", active);
            if let (Some(name), Some(value)) = (name, value) {
                let next = let_scope
                    .clone()
                    .unwrap_or_else(|| scope.clone())
                    .with_binding(name, value);
                let_scope = Some(next);
            }
            continue;
        }
        // **Wave 14.2** — every code path that reads bindings now
        // consults the running `let_scope` first (sibling-level
        // shadowing of the parent scope). The original `scope`
        // parameter is the fallback when no `<let/>` preceded this
        // sibling.
        let active = let_scope.as_ref().unwrap_or(scope);
        let cf = control_flow_attr(el);
        match cf {
            None => {
                chain_taken = None;
                out.push((node.clone(), let_scope.clone()));
            }
            Some(ControlFlow::If(cond)) => {
                let take = eval_truthy(&cond, active);
                chain_taken = Some(take);
                if take {
                    out.push((node.clone(), let_scope.clone()));
                }
            }
            Some(ControlFlow::ElseIf(cond)) => {
                let take = matches!(chain_taken, Some(false)) && eval_truthy(&cond, active);
                if let Some(prev) = chain_taken.as_mut() {
                    *prev = *prev || take;
                }
                if take {
                    out.push((node.clone(), let_scope.clone()));
                }
            }
            Some(ControlFlow::Else) => {
                let take = matches!(chain_taken, Some(false));
                chain_taken = None;
                if take {
                    out.push((node.clone(), let_scope.clone()));
                }
            }
            Some(ControlFlow::For {
                var,
                index_var,
                source,
                step,
                reverse,
            }) => {
                // **Wave 15.2** — numeric range `0..n` / `0..=n` short-circuits the
                // binding-lookup path before falling back to **Wave 15.3** object
                // iteration on a JSON object (each LHS-pair element binds
                // `(value, key)`) and the original array iteration.
                // Post-Wave-15 modifiers: `step N` (range only) skips by `N`
                // between emitted endpoints; `reverse` flips the final order
                // of every iteration shape.
                let iter = resolve_for_iteration(&source, step, reverse, active);
                // **Wave 15.1** — empty iteration sets `chain_taken =
                // Some(false)` so a subsequent `else` (Svelte's
                // `{:each}{:else}` shape) runs as the empty-state fallback.
                // Non-empty iteration sets `Some(true)` to suppress the
                // else branch, matching the if-chain rule. The previous
                // unconditional reset-to-`None` becomes the "no for at
                // all" baseline up at `None` of `match cf`.
                chain_taken = Some(!iter.is_empty());
                for (key, item) in iter {
                    let mut child_scope = active.clone().with_binding(var.clone(), item);
                    // Optional iteration-index / object-key binding —
                    // `for="row, idx in rows"` exposes the index as a
                    // typed integer for arrays / ranges; `for="value, key
                    // in obj"` exposes the string key for objects. Same
                    // LHS slot, type depends on the source shape (Wave
                    // 15.3).
                    if let Some(idx_name) = index_var.as_ref() {
                        child_scope = child_scope.with_binding(idx_name.clone(), key);
                    }
                    out.push((node.clone(), Some(child_scope)));
                }
            }
        }
    }
    out
}

/// **Wave 15.2 / 15.3** — resolve the right-hand side of a `for=`
/// attribute into an ordered list of `(second-LHS, primary-LHS)`
/// pairs. The primary is the item bound to `var`; the second is the
/// value bound to `index_var` when present.
///
/// Three source shapes, in priority order:
/// 1. **Numeric range** `start..end` / `start..=end` (Wave 15.2).
///    Both endpoints resolve through the full expression evaluator
///    so `for="i in 0..items.length"` works as well as `for="i in
///    0..10"`. Reverse ranges (start > end) yield an empty
///    iteration — Rust `Range`'s shape.
/// 2. **JSON object** (Wave 15.3). When the source resolves to a
///    `Value::Object`, iterate `(key, value)` entries in insertion
///    order. The second-LHS binding receives the string key.
/// 3. **JSON array** (the pre-Wave-15 path). Iterate values; the
///    second-LHS binding receives the integer index.
pub(super) fn resolve_for_iteration(
    source: &str,
    step: Option<i64>,
    reverse: bool,
    scope: &LowerScope,
) -> Vec<(serde_json::Value, serde_json::Value)> {
    let trimmed = source.trim();
    // Wave 15.2 — range form. `..=` parsed before `..` so the
    // inclusive form isn't swallowed by the exclusive split.
    if let Some((start_raw, end_raw, inclusive)) = split_range(trimmed) {
        let start = resolve_to_i64(start_raw, scope);
        let end = resolve_to_i64(end_raw, scope);
        if let (Some(start), Some(end)) = (start, end) {
            let last = if inclusive { end } else { end - 1 };
            if start > last {
                return Vec::new();
            }
            let stride = step.unwrap_or(1).max(1) as usize;
            let mut values: Vec<i64> = (start..=last).step_by(stride).collect();
            if reverse {
                values.reverse();
            }
            return values
                .into_iter()
                .map(|n| (serde_json::Value::from(n), serde_json::Value::from(n)))
                .collect();
        }
        return Vec::new();
    }
    // Resolve the source as an arbitrary expression so a dotted-path
    // (`item.children`) and functional-helper calls (`map(rows,
    // 'label')`, `filter(rows, 'status', 'active')`, `slice(rows, 0,
    // 5)`) resolve through the same owned-value vocabulary as
    // attribute / text interpolations. Bare identifiers and virtual
    // segments still hit the cheap path inside `lookup_path_owned`.
    //
    // Note: `step` is range-only; arrays and objects always emit every
    // entry. `reverse` flips the final order for either shape.
    let resolved =
        lookup_path_owned(trimmed, scope).or_else(|| evaluate_expression(trimmed, scope));
    let mut entries: Vec<(serde_json::Value, serde_json::Value)> = match resolved {
        Some(serde_json::Value::Object(map)) => map
            .into_iter()
            .map(|(k, v)| (serde_json::Value::String(k), v))
            .collect(),
        Some(serde_json::Value::Array(arr)) => arr
            .into_iter()
            .enumerate()
            .map(|(i, v)| (serde_json::Value::from(i as i64), v))
            .collect(),
        _ => Vec::new(),
    };
    if reverse {
        entries.reverse();
    }
    entries
}

/// Split `start..end` / `start..=end` into `(start, end, inclusive)`.
/// Returns `None` when no `..` appears, so the caller falls through to
/// the value-binding lookup. Inclusive (`..=`) wins over exclusive
/// (`..`) so an author writing `0..=10` doesn't accidentally end up
/// with `0..` + `=10`.
fn split_range(body: &str) -> Option<(&str, &str, bool)> {
    if let Some((start, end)) = body.split_once("..=") {
        return Some((start.trim(), end.trim(), true));
    }
    if let Some((start, end)) = body.split_once("..") {
        return Some((start.trim(), end.trim(), false));
    }
    None
}

/// Resolve a range endpoint to an integer. Tries the cheap parse
/// first (so `0..10` doesn't pay the expression-parser cost), then
/// falls through to the full evaluator (so `0..items.length` works
/// when an array's length is exposed via a bound `length` field on
/// scope) and finally the bare-binding lookup.
fn resolve_to_i64(s: &str, scope: &LowerScope) -> Option<i64> {
    if let Ok(n) = s.parse::<i64>() {
        return Some(n);
    }
    let value = lookup_path_owned(s, scope).or_else(|| evaluate_expression(s, scope))?;
    match value {
        serde_json::Value::Number(n) => n.as_i64().or_else(|| n.as_f64().map(|f| f as i64)),
        serde_json::Value::String(s) => s.parse::<i64>().ok(),
        _ => None,
    }
}

#[derive(Debug, Clone)]
enum ControlFlow {
    If(String),
    ElseIf(String),
    Else,
    For {
        var: String,
        index_var: Option<String>,
        source: String,
        /// Iteration step. Only meaningful for numeric ranges
        /// (`for="i in 0..100 step 10"`). `None` defaults to step 1.
        /// Non-range sources ignore this — array / object iteration
        /// always emits every entry. Parser guarantees `>= 1` when
        /// `Some`.
        step: Option<i64>,
        /// Reverse iteration order. Applies after collecting all
        /// entries (range, array, or object). `for="item in items reverse"`,
        /// `for="i in 0..10 reverse"`, `for="i in 0..100 step 10 reverse"`
        /// all valid.
        reverse: bool,
    },
}

fn control_flow_attr(el: &Element) -> Option<ControlFlow> {
    for attr in &el.attributes {
        if !matches!(attr.name.namespace, AttributeNamespace::ControlFlow) {
            continue;
        }
        let body = attribute_string(&attr.value).unwrap_or_default();
        return Some(match attr.name.local.as_str() {
            "if" => ControlFlow::If(body),
            "else-if" => ControlFlow::ElseIf(body),
            "else" => ControlFlow::Else,
            "for" => parse_for_clause(&body)
                .map(|c| ControlFlow::For {
                    var: c.var,
                    index_var: c.index_var,
                    source: c.source,
                    step: c.step,
                    reverse: c.reverse,
                })
                .unwrap_or_else(|| ControlFlow::If("false".into())),
            _ => return None,
        });
    }
    None
}

/// `"post in posts"` → `("post", None, "posts")`.
/// `"post, idx in posts"` → `("post", Some("idx"), "posts")` — the
/// optional iteration-index variable lets authors build per-row stable
/// ids (`<x id="row-{idx}"/>`), critical for hit-testing dispatched
/// containers via `<dispatch for="row, idx in rows" id="props-{idx}"/>`.
/// Whitespace tolerant; any other shape returns `None` and the caller
/// treats the element as dropped (`if false`).
/// Parsed `for=` clause. Each field captures one part of the
/// `var [, index_var] in source [step N] [reverse]` shape.
struct ForClause {
    var: String,
    index_var: Option<String>,
    source: String,
    /// Iteration step. Range-only; `None` defaults to 1. Parser
    /// guarantees `>= 1` when `Some`.
    step: Option<i64>,
    /// Reverse iteration order after collection.
    reverse: bool,
}

/// `"post in posts"` → `ForClause { var: "post", index_var: None,
/// source: "posts", step: None, reverse: false }`.
/// `"post, idx in posts"` adds `index_var: Some("idx")`.
/// `"i in 0..100 step 10"` adds `step: Some(10)`.
/// `"item in items reverse"` adds `reverse: true`.
/// `"i in 0..100 step 10 reverse"` adds both.
///
/// The source identifier or range is the first whitespace-delimited
/// token after `in`; any trailing `step N` / `reverse` modifiers apply
/// in either order. Step `<= 0`, duplicate modifiers, and trailing
/// junk all return `None`, which the caller treats as `if false` (the
/// element is dropped).
fn parse_for_clause(body: &str) -> Option<ForClause> {
    let body = body
        .trim()
        .trim_start_matches('{')
        .trim_end_matches('}')
        .trim();
    // Split on `in` first so the iteration-variable side can carry an
    // optional comma-separated index name without ambiguity with the
    // `in` keyword.
    let mut halves = body.splitn(2, " in ");
    let lhs = halves.next()?.trim();
    let rhs = halves.next()?.trim();
    if rhs.is_empty() {
        return None;
    }
    // LHS shapes: `var` or `var, idx`. Reject anything else.
    let mut lhs_parts = lhs.split(',').map(|s| s.trim());
    let var = lhs_parts.next()?.to_string();
    if var.is_empty() {
        return None;
    }
    let index_var = lhs_parts
        .next()
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty());
    if lhs_parts.next().is_some() {
        return None;
    }
    // RHS shape: `<source> [step N] [reverse]` in either order. The
    // source can be a bare identifier, a dotted-path, a numeric range
    // (`0..n` / `0..=n`), OR a functional-helper call (`map(rows,
    // 'label')`, `slice(filter(rows, …), 0, 5)`) that may itself
    // contain whitespace inside its argument list. Modifiers are
    // recognised at the tail; the source extends from the start of
    // the RHS to the boundary before the first `step N` / `reverse`
    // token at paren-depth 0.
    let (source, after) = split_for_source(rhs)?;
    let source = source.trim().to_string();
    if source.is_empty() {
        return None;
    }
    let mut tokens = after.split_whitespace();
    let mut step: Option<i64> = None;
    let mut reverse = false;
    while let Some(tok) = tokens.next() {
        match tok {
            "reverse" => {
                if reverse {
                    return None;
                }
                reverse = true;
            }
            "step" => {
                if step.is_some() {
                    return None;
                }
                let v = tokens.next()?.parse::<i64>().ok()?;
                if v < 1 {
                    return None;
                }
                step = Some(v);
            }
            _ => return None,
        }
    }
    Some(ForClause {
        var,
        index_var,
        source,
        step,
        reverse,
    })
}

/// Split a `for=` right-hand-side into the source expression and the
/// trailing-modifier slice. Walks left-to-right tracking paren / quote
/// depth so a call-form source (`map(rows, 'label') reverse step 2`)
/// extends through the comma + nested parens cleanly. The boundary
/// fires when we see a whitespace-delimited `step` or `reverse` token
/// at paren-depth 0 outside any quoted string.
fn split_for_source(rhs: &str) -> Option<(String, String)> {
    let bytes = rhs.as_bytes();
    let mut depth = 0i32;
    let mut in_str: Option<u8> = None;
    let mut i = 0usize;
    let mut last_ws_end: Option<usize> = None;
    while i < bytes.len() {
        let ch = bytes[i];
        match (in_str, ch) {
            (Some(q), c) if c == q => in_str = None,
            (Some(_), _) => {}
            (None, b'\'' | b'"') => in_str = Some(ch),
            (None, b'(' | b'[') => depth += 1,
            (None, b')' | b']') => depth -= 1,
            (None, b' ' | b'\t') if depth == 0 => {
                // We're between tokens at depth 0. Peek the next
                // non-whitespace word and check whether it's a
                // modifier. If yes, this whitespace is the boundary.
                let mut j = i + 1;
                while j < bytes.len() && matches!(bytes[j], b' ' | b'\t') {
                    j += 1;
                }
                let mut k = j;
                while k < bytes.len() && !matches!(bytes[k], b' ' | b'\t') {
                    k += 1;
                }
                let tok = &rhs[j..k];
                if matches!(tok, "step" | "reverse") {
                    let source = rhs[..i].to_string();
                    let tail = rhs[j..].to_string();
                    return Some((source, tail));
                }
                last_ws_end = Some(k);
                i = k;
                continue;
            }
            _ => {}
        }
        i += 1;
    }
    let _ = last_ws_end;
    Some((rhs.to_string(), String::new()))
}
