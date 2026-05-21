//! Expression evaluator lowered out of `interpret/mod.rs` during
//! Phase 0. See `docs/dev/prui-expressiveness-roadmap.md` §7.0.
//! Pure code move, no behaviour change. Backs every `{expr}` binding
//! in PRUI through the lookup, interpolation, and Pratt-bridge layer.
//! Public entry points are re-exported through `interpret/mod.rs` so
//! external callers keep their existing import paths.

use super::LowerScope;

/// Scan a literal text run for `{expr}` segments and resolve them
/// through the same `lookup_expression` → `evaluate_expression` cascade
/// the attribute-template path uses, so an authored `<text>Hello, {kind == 'error' ? 'oops' : name}</text>`
/// reads through the same vocabulary as `<container style:bg="{…}"/>`.
pub(super) fn interpolate(text: &str, scope: &LowerScope) -> String {
    if !text.contains('{') {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '{' {
            let mut body = String::new();
            for inner in chars.by_ref() {
                if inner == '}' {
                    break;
                }
                body.push(inner);
            }
            let body = body.trim();
            if let Some(v) = lookup_path_owned(body, scope) {
                out.push_str(&stringify_value(&v));
            } else if let Some(v) = evaluate_expression(body, scope) {
                out.push_str(&stringify_value(&v));
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Resolve a `{...}`-style expression body against the scope. Phase-2
/// minimum: bare identifiers + dotted paths into the bound JSON value
/// (object fields, array indices). Anything richer returns `None` and
/// the caller falls back to an empty string. The full expression
/// evaluator lives in `prism_core::language::expression` and lands here
/// behind the same seam when component instantiation grows past
/// identifiers.
///
/// Public-but-`#[doc(hidden)]` so the `prism-builder` resolver
/// (`RegistryTagResolver`) can pre-resolve attribute interpolations
/// before constructing the builder `Node`. Sole non-runtime caller.
#[doc(hidden)]
pub fn lookup_expression_in_scope<'a>(
    body: &str,
    scope: &'a LowerScope,
) -> Option<&'a serde_json::Value> {
    lookup_expression(body, scope)
}

/// Owned-value counterpart of [`lookup_expression_in_scope`] that
/// also supports virtual trailing segments (`.length`, `.size`,
/// `.count`, `.first`, `.last`) on arrays / objects / strings. Used
/// by `prism-builder`'s resolver so dispatched-tag attribute
/// resolution (`<shell.foo prop="{items.length}"/>`) reads through
/// the same vocabulary the runtime's own attribute paths use.
#[doc(hidden)]
pub fn lookup_path_owned_in_scope(body: &str, scope: &LowerScope) -> Option<serde_json::Value> {
    lookup_path_owned(body, scope)
}

/// Internal counterpart used by `lower_*` paths.
pub(super) fn lookup_expression<'a>(
    body: &str,
    scope: &'a LowerScope,
) -> Option<&'a serde_json::Value> {
    let body = body.trim();
    if body.is_empty() {
        return None;
    }
    let mut parts = body.split('.');
    let head = parts.next()?.trim();
    let mut cursor = scope.binding(head)?;
    for segment in parts {
        let key = segment.trim();
        if key.is_empty() {
            return None;
        }
        cursor = match cursor {
            serde_json::Value::Object(map) => map.get(key)?,
            serde_json::Value::Array(arr) => {
                let idx: usize = key.parse().ok()?;
                arr.get(idx)?
            }
            _ => return None,
        };
    }
    Some(cursor)
}

/// Owned-value path resolution that augments [`lookup_expression`]
/// with **virtual trailing segments** on arrays / objects:
///
/// | Segment | Array | Object | String |
/// |---|---|---|---|
/// | `.length`, `.size`, `.count` | number of items | number of keys | grapheme count |
/// | `.first` | first item | — | first char (as string) |
/// | `.last`  | last item  | — | last char (as string)  |
///
/// `arr.length` returns the integer length; `arr.first` / `.last`
/// return the JSON value at the leaf position (or `Null` for an empty
/// container). Strings inherit a parity surface so authors aren't
/// surprised by `name.length` on a string binding. Returns `None` when
/// the path doesn't terminate in a virtual segment AND
/// [`lookup_expression`] can't resolve it either.
///
/// Used by every "resolve a path body" seam — `resolved_attribute_string`,
/// `resolve_to_i64`, `eval_truthy`, the text-interpolation path —
/// so the same author-facing dotted-path vocabulary works everywhere.
pub(super) fn lookup_path_owned(body: &str, scope: &LowerScope) -> Option<serde_json::Value> {
    if let Some(v) = lookup_expression(body, scope) {
        return Some(v.clone());
    }
    // **Wave B (`prui-luau-fusion.md` §7.2)** — pipe rewrite. `a | f(x)`
    // and `a |> f(x)` desugar to `f(a, x)` (left-associative, F#/Elm
    // shape) *before* the call resolver runs, so a pipeline like
    // `tasks | filter('status','open') | take(5)` reads as nested
    // builtin calls. Pure string transform — no Lua needed for the
    // pipe itself (the closure args, if any, are resolved later by
    // the closure-aware builtin path).
    if let Some(rewritten) = rewrite_pipes(body) {
        if let Some(v) = lookup_path_owned(&rewritten, scope) {
            return Some(v);
        }
    }
    let body = body.trim();
    // **Functional helpers** — `map`, `reduce`, `filter`, `find`,
    // `slice`, `sort_by`, `unique`, `reverse`, `keys`, `values`,
    // `entries`, `includes`, `index_of`, `join`. These operate on
    // typed JSON values (arrays/objects) which the expression layer's
    // `ExprValue::{Number,String,Boolean}` can't carry, so the owned
    // lookup surface is the natural seam. Authors compose them inside
    // for-sources, attribute interpolations, and text bodies through
    // the same `{call(arr, …)}` shape.
    if let Some(v) = try_call_owned(body, scope) {
        return Some(v);
    }
    // **Wave A** — script-block scope. A `<script>`
    // block's top-level `local`s resolve here after the JSON binding
    // map and functional builtins miss (the §5.6 resolution stack:
    // script locals sit below `let`/`for` vars, above host bindings).
    #[cfg(feature = "luau")]
    if let Some(frame) = scope.luau_scope() {
        if let Some(v) = frame.lookup(body) {
            return Some(v);
        }
    }
    let (head, virtual_seg) = body.rsplit_once('.')?;
    let head = head.trim();
    let virtual_seg = virtual_seg.trim();
    if !matches!(virtual_seg, "length" | "size" | "count" | "first" | "last") {
        return None;
    }
    let parent = lookup_expression(head, scope)?;
    Some(match (parent, virtual_seg) {
        (serde_json::Value::Array(arr), "length" | "size" | "count") => {
            serde_json::Value::from(arr.len() as i64)
        }
        (serde_json::Value::Object(map), "length" | "size" | "count") => {
            serde_json::Value::from(map.len() as i64)
        }
        (serde_json::Value::String(s), "length" | "size" | "count") => {
            serde_json::Value::from(s.chars().count() as i64)
        }
        (serde_json::Value::Array(arr), "first") => {
            arr.first().cloned().unwrap_or(serde_json::Value::Null)
        }
        (serde_json::Value::Array(arr), "last") => {
            arr.last().cloned().unwrap_or(serde_json::Value::Null)
        }
        (serde_json::Value::String(s), "first") => s
            .chars()
            .next()
            .map(|c| serde_json::Value::from(c.to_string()))
            .unwrap_or(serde_json::Value::Null),
        (serde_json::Value::String(s), "last") => s
            .chars()
            .last()
            .map(|c| serde_json::Value::from(c.to_string()))
            .unwrap_or(serde_json::Value::Null),
        _ => return None,
    })
}

/// **Wave B (`prui-luau-fusion.md` §7.2)** — rewrite the pipe
/// operator into nested calls. `a | f(x)` and the `|>` alias both
/// desugar to `f(a, x)`; the operator is left-associative so
/// `t | filter(p) | take(5)` becomes `take(filter(t, p), 5)`.
///
/// Returns `Some(rewritten)` when at least one top-level pipe was
/// found (fully resolved — the result is pipe-free), `None` when the
/// body has no pipe so callers skip the extra resolution attempt.
///
/// Disambiguation rules (no Lua needed — this is a pure string
/// transform):
/// - `||` (logical or) is never a pipe.
/// - A closure literal's own bars (`|t| t.x`) are not pipes: the
///   pipe operator is either `|>` or a `|` with whitespace on *both*
///   sides, and closure bars never present that shape.
/// - Scanning is depth- and quote-aware, so a `|` inside
///   `filter(|t| …)` (depth ≥ 1) or inside a string is skipped.
pub(super) fn rewrite_pipes(body: &str) -> Option<String> {
    let split = find_last_top_level_pipe(body)?;
    let lhs = body[..split.0].trim();
    let rhs = body[split.1..].trim();
    if lhs.is_empty() || rhs.is_empty() {
        return None;
    }
    // Left-associative: recurse on the LHS first so `a | f | g`
    // resolves inner-out to `g(f(a))`.
    let lhs_rewritten = rewrite_pipes(lhs).unwrap_or_else(|| lhs.to_string());
    // RHS must be a call or a bare callable name. `f(x)` →
    // `f(lhs, x)`; `f()` / `f` → `f(lhs)`.
    let piped = if let Some(open) = rhs.find('(') {
        let close = matching_close_paren(rhs, open)?;
        // Anything after the call's `)` (a trailing `.field` or
        // another operator) isn't a valid pipe RHS — bail so the
        // caller treats the body as non-pipe.
        if rhs[close + 1..].trim() != "" {
            return None;
        }
        let name = rhs[..open].trim();
        let inner = rhs[open + 1..close].trim();
        if inner.is_empty() {
            format!("{name}({lhs_rewritten})")
        } else {
            format!("{name}({lhs_rewritten}, {inner})")
        }
    } else {
        // Bare name. Reject anything with operator/space chars so we
        // don't swallow a malformed RHS.
        if rhs
            .chars()
            .any(|c| !(c.is_alphanumeric() || c == '_' || c == '.'))
        {
            return None;
        }
        format!("{rhs}({lhs_rewritten})")
    };
    Some(piped)
}

/// Find the byte range `(start, end)` of the last top-level pipe
/// operator in `body`, where `start..end` is the operator span (so
/// `body[..start]` is the LHS and `body[end..]` the RHS). Skips
/// `||`, quoted regions, and any `|` nested in parens/brackets.
fn find_last_top_level_pipe(body: &str) -> Option<(usize, usize)> {
    let bytes = body.as_bytes();
    let mut depth = 0i32;
    let mut in_str: Option<u8> = None;
    let mut last: Option<(usize, usize)> = None;
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        match (in_str, c) {
            (Some(q), x) if x == q => in_str = None,
            (Some(_), _) => {}
            (None, b'\'' | b'"') => in_str = Some(c),
            (None, b'(' | b'[' | b'{') => depth += 1,
            (None, b')' | b']' | b'}') => depth -= 1,
            (None, b'|') if depth == 0 => {
                // `|>` operator.
                if bytes.get(i + 1) == Some(&b'>') {
                    last = Some((i, i + 2));
                    i += 2;
                    continue;
                }
                // `||` logical-or — skip both bars.
                if bytes.get(i + 1) == Some(&b'|') {
                    i += 2;
                    continue;
                }
                // `|` pipe only when whitespace-flanked (a closure's
                // own `|t|` bars never are).
                let prev_ws = i > 0 && bytes[i - 1].is_ascii_whitespace();
                let next_ws = bytes.get(i + 1).is_some_and(u8::is_ascii_whitespace);
                if prev_ws && next_ws {
                    last = Some((i, i + 1));
                }
            }
            _ => {}
        }
        i += 1;
    }
    last
}

/// Recognised functional builtin names that operate on typed JSON
/// values (arrays / objects / strings). Used by [`try_call_owned`]
/// to gate the cheap call-form parse — anything else falls through
/// to the full expression evaluator. Listed here as a single seam so
/// every consumer (call resolver, parse helper, future LSP
/// completion) reads from the same table.
const ARRAY_CALL_NAMES: &[&str] = &[
    "map",
    "reduce",
    "filter",
    "find",
    "slice",
    "sort_by",
    "unique",
    "reverse",
    "keys",
    "values",
    "entries",
    "includes",
    "index_of",
    "join",
    "concat_arr",
    "take",
    "drop",
    "pluck",
    "group_by",
    "count_by",
    "any",
    "all",
    "chunk",
    "zip",
    "range",
    // **Wave B** — closure-only builtin (no field-name form). Listed
    // so the call gate admits it; `eval_array_call` has no `reject`
    // arm, so a non-closure `reject(...)` resolves to `None`.
    "reject",
];

/// **Wave B (`prui-luau-fusion.md` §7.2 / B.4)** — builtins that
/// accept a closure literal as a second call form
/// (`filter(arr, |t| t.x)` alongside `filter(arr, "x", v)`). The
/// closure is evaluated per element through the per-document Luau
/// scope.
#[cfg(feature = "luau")]
const CLOSURE_BUILTINS: &[&str] = &[
    "filter", "reject", "map", "find", "any", "all", "sort_by", "group_by", "count_by",
];

/// **Wave B** — cheap shape check: does this raw arg look like a
/// closure literal? Full validation happens in `desugar_closure`;
/// this only decides whether to take the closure-aware builtin path
/// instead of the field-name path.
#[cfg(feature = "luau")]
pub(super) fn looks_like_closure(s: &str) -> bool {
    let s = s.trim();
    s.starts_with("\\fn") || (s.starts_with('|') && !s.starts_with("||"))
}

/// **Wave B** — split a call-arg list on top-level commas, returning
/// the raw (unevaluated) slices. Mirrors [`parse_call_args`]'s
/// depth/quote scanner but keeps the substrings verbatim so a
/// closure literal (`|t| t.x`) survives to `desugar_closure` instead
/// of being mangled by argument evaluation.
#[cfg(feature = "luau")]
fn split_top_level_args(inside: &str) -> Option<Vec<&str>> {
    let trimmed = inside.trim();
    if trimmed.is_empty() {
        return Some(Vec::new());
    }
    let bytes = inside.as_bytes();
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut in_str: Option<u8> = None;
    let mut start = 0usize;
    for (i, &c) in bytes.iter().enumerate() {
        match (in_str, c) {
            (Some(q), x) if x == q => in_str = None,
            (Some(_), _) => {}
            (None, b'\'' | b'"') => in_str = Some(c),
            (None, b'(' | b'[' | b'{') => depth += 1,
            (None, b')' | b']' | b'}') => {
                depth -= 1;
                if depth < 0 {
                    return None;
                }
            }
            (None, b',') if depth == 0 => {
                out.push(inside[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
    }
    if depth != 0 || in_str.is_some() {
        return None;
    }
    out.push(inside[start..].trim());
    Some(out)
}

/// **Wave B** — evaluate a closure-form functional builtin. `raw`
/// are the unevaluated arg slices: `raw[0]` is the collection
/// expression (resolved through the owned-value vocabulary so a
/// pipeline / nested call still works), `raw[1]` is the closure
/// literal, and (for `sort_by`) an optional `raw[2]` order token
/// (`'desc'`). Returns `None` on any shape mismatch so the caller
/// fails the call cleanly rather than emitting wrong data.
#[cfg(feature = "luau")]
fn eval_closure_builtin(
    name: &str,
    raw: &[&str],
    scope: &LowerScope,
    frame: &crate::luau_scope::LuauScopeFrame,
) -> Option<serde_json::Value> {
    use serde_json::Value;
    let arr = match lookup_path_owned(raw.first()?.trim(), scope)? {
        Value::Array(a) => a,
        _ => return None,
    };
    let clo = raw.get(1)?.trim();
    let truthy = |v: &Value| !matches!(v, Value::Null | Value::Bool(false));
    // Evaluate the closure for one element; a closure error aborts
    // the whole builtin (returns `None`) — no partial results.
    let eval = |item: &Value| -> Option<Value> {
        frame.call_closure(clo, std::slice::from_ref(item))?.ok()
    };
    match name {
        "filter" | "reject" => {
            let want = name == "filter";
            let mut out = Vec::new();
            for item in &arr {
                if truthy(&eval(item)?) == want {
                    out.push(item.clone());
                }
            }
            Some(Value::Array(out))
        }
        "map" => {
            let mut out = Vec::with_capacity(arr.len());
            for item in &arr {
                out.push(eval(item)?);
            }
            Some(Value::Array(out))
        }
        "find" => {
            for item in &arr {
                if truthy(&eval(item)?) {
                    return Some(item.clone());
                }
            }
            Some(Value::Null)
        }
        "any" => {
            for item in &arr {
                if truthy(&eval(item)?) {
                    return Some(Value::Bool(true));
                }
            }
            Some(Value::Bool(false))
        }
        "all" => {
            for item in &arr {
                if !truthy(&eval(item)?) {
                    return Some(Value::Bool(false));
                }
            }
            Some(Value::Bool(true))
        }
        "sort_by" => {
            // Decorate-sort-undecorate: the closure runs once per
            // element (not per comparison).
            let mut keyed: Vec<(Value, Value)> = Vec::with_capacity(arr.len());
            for item in &arr {
                keyed.push((eval(item)?, item.clone()));
            }
            keyed.sort_by(|a, b| compare_values(Some(&a.0), Some(&b.0)));
            let desc = raw
                .get(2)
                .map(|s| s.trim().trim_matches(['\'', '"']))
                .is_some_and(|s| s == "desc");
            if desc {
                keyed.reverse();
            }
            Some(Value::Array(keyed.into_iter().map(|(_, v)| v).collect()))
        }
        "group_by" | "count_by" => {
            let counting = name == "count_by";
            let mut map = serde_json::Map::new();
            for item in &arr {
                let key = stringify_value(&eval(item)?);
                if counting {
                    let slot = map.entry(key).or_insert(Value::from(0));
                    let n = slot.as_i64().unwrap_or(0) + 1;
                    *slot = Value::from(n);
                } else if let Some(a) = map
                    .entry(key)
                    .or_insert_with(|| Value::Array(Vec::new()))
                    .as_array_mut()
                {
                    a.push(item.clone());
                }
            }
            Some(Value::Object(map))
        }
        _ => None,
    }
}

/// Try to resolve `body` as a functional-builtin call — optionally
/// followed by a dotted access path: `map(arr, "field")`,
/// `slice(rows, 0, 5)`, `find(rows, 'id', 2).label`, etc. Returns
/// `Some(value)` on a successful evaluation; `None` when the body
/// isn't a recognised call shape, so the caller falls through to
/// virtual segments and then the expression evaluator. Args are
/// parsed as one of: number literal, single- or double-quoted
/// string, `true`/`false`/`null`, or a bare path resolved via
/// [`lookup_path_owned`] (so calls nest).
fn try_call_owned(body: &str, scope: &LowerScope) -> Option<serde_json::Value> {
    let body = body.trim();
    let open = body.find('(')?;
    let name = body[..open].trim();
    // **Wave F (`prui-luau-fusion.md` §7.9)** — colour helpers usable
    // from computed PRSS (`{ lua = "darken(tokens.colors.accent,
    // 0.1)" }`) and any expression slot. Native so the doc example
    // works without a `<script>`-defined helper; args resolve through
    // the same owned-value pipeline (so `tokens.colors.accent` is a
    // valid first arg).
    if matches!(
        name,
        "darken" | "lighten" | "alpha" | "mix" | "with" | "saturate" | "desaturate"
    ) {
        let close = matching_close_paren(body, open)?;
        let (positional, kwargs) = parse_call_args(&body[open + 1..close], scope)?;
        let v = eval_color_call(name, &positional, &kwargs)?;
        let tail = body[close + 1..].trim_start();
        return if tail.is_empty() {
            Some(v)
        } else {
            walk_dotted_path(&v, tail.strip_prefix('.')?)
        };
    }
    if !ARRAY_CALL_NAMES.contains(&name) {
        // **Wave A** — a `<script>` helper call
        // (`{priority_color(task.priority)}`). The call resolver
        // already parses args + the trailing dotted chain; we only
        // add a new dispatch arm for "the name is a harvested Luau
        // function". Args resolve through the same `parse_call_args`
        // path so nested builtin / binding args still work.
        #[cfg(feature = "luau")]
        if let Some(frame) = scope.luau_scope() {
            if frame.has_function(name) {
                let close = matching_close_paren(body, open)?;
                let (args, _kwargs) = parse_call_args(&body[open + 1..close], scope)?;
                let result = match frame.call(name, &args)? {
                    Ok(v) => v,
                    Err(_) => return None,
                };
                let tail = body[close + 1..].trim_start();
                if tail.is_empty() {
                    return Some(result);
                }
                let rest = tail.strip_prefix('.')?;
                return walk_dotted_path(&result, rest);
            }
        }
        return None;
    }
    // Match the closing paren that pairs with `open`, respecting
    // nested parens and quoted strings so `find(rows, 'id', 2)` and
    // `slice(filter(rows, 'k', 'v'), 0, 2)` both find their right
    // boundary cleanly.
    let close = matching_close_paren(body, open)?;
    let inside = &body[open + 1..close];
    // **Wave B** — closure-form builtin
    // (`filter(tasks, |t| t.priority == 'high')`). Checked before
    // `parse_call_args` so the closure literal isn't mangled by
    // argument evaluation. A closure-shaped arg with no Luau scope,
    // or a closure eval error, fails the call (returns `None`)
    // rather than silently falling into the field-name path.
    #[cfg(feature = "luau")]
    if CLOSURE_BUILTINS.contains(&name) {
        if let Some(raw) = split_top_level_args(inside) {
            if raw.iter().any(|a| looks_like_closure(a)) {
                let frame = scope.luau_scope()?;
                let v = eval_closure_builtin(name, &raw, scope, frame)?;
                let tail = body[close + 1..].trim_start();
                if tail.is_empty() {
                    return Some(v);
                }
                return walk_dotted_path(&v, tail.strip_prefix('.')?);
            }
        }
    }
    let (args, _kwargs) = parse_call_args(inside, scope)?;
    let call_value = eval_array_call(name, &args)?;
    let tail = body[close + 1..].trim_start();
    if tail.is_empty() {
        return Some(call_value);
    }
    // Trailing chain: must begin with `.` for the dotted-access
    // shape. Anything else (`(`, `[`, operator) is unsupported here —
    // the caller falls back to the full expression evaluator.
    let rest = tail.strip_prefix('.')?;
    walk_dotted_path(&call_value, rest)
}

/// Find the matching `)` for an opening paren at `open` in `body`.
/// Returns `None` when the body has unbalanced parens or unterminated
/// quoted strings — same defensive shape `parse_call_args` uses.
fn matching_close_paren(body: &str, open: usize) -> Option<usize> {
    let bytes = body.as_bytes();
    let mut depth = 0i32;
    let mut in_str: Option<u8> = None;
    let mut i = open;
    while i < bytes.len() {
        let ch = bytes[i];
        match (in_str, ch) {
            (Some(q), c) if c == q => in_str = None,
            (Some(_), _) => {}
            (None, b'\'' | b'"') => in_str = Some(ch),
            (None, b'(') => depth += 1,
            (None, b')') => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
                if depth < 0 {
                    return None;
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// Walk a dotted-path expression (`label`, `user.email`, `tabs.0`)
/// against an owned JSON value. Returns `Some(child)` on a successful
/// walk; `None` on a missing segment / type mismatch. Virtual
/// trailing segments (`.length`, `.first`, `.last`) are recognised
/// terminally so `slice(rows, 0, 5).length` reads as expected.
fn walk_dotted_path(root: &serde_json::Value, path: &str) -> Option<serde_json::Value> {
    let path = path.trim();
    if path.is_empty() {
        return Some(root.clone());
    }
    let mut cursor: serde_json::Value = root.clone();
    let segments: Vec<&str> = path.split('.').map(str::trim).collect();
    for (i, seg) in segments.iter().enumerate() {
        if seg.is_empty() {
            return None;
        }
        // Recognise a terminal virtual segment on the last position.
        if i == segments.len() - 1 {
            match (&cursor, *seg) {
                (serde_json::Value::Array(a), "length" | "size" | "count") => {
                    return Some(serde_json::Value::from(a.len() as i64));
                }
                (serde_json::Value::Object(m), "length" | "size" | "count") => {
                    return Some(serde_json::Value::from(m.len() as i64));
                }
                (serde_json::Value::String(s), "length" | "size" | "count") => {
                    return Some(serde_json::Value::from(s.chars().count() as i64));
                }
                (serde_json::Value::Array(a), "first") => {
                    return Some(a.first().cloned().unwrap_or(serde_json::Value::Null));
                }
                (serde_json::Value::Array(a), "last") => {
                    return Some(a.last().cloned().unwrap_or(serde_json::Value::Null));
                }
                _ => {}
            }
        }
        cursor = match cursor {
            serde_json::Value::Object(mut map) => map.remove(*seg)?,
            serde_json::Value::Array(arr) => {
                let idx: usize = seg.parse().ok()?;
                arr.into_iter().nth(idx)?
            }
            _ => return None,
        };
    }
    Some(cursor)
}

/// Split-call output: positional values in `.0`, raw kwarg pairs in
/// `.1`. Kwarg RHS stays as `String` so callers that care about
/// `+0.05` vs `0.05` (delta vs absolute) can inspect the leading
/// sign before number coercion.
type ParsedCallArgs = (Vec<serde_json::Value>, Vec<(String, String)>);

/// Split a call argument list on top-level commas (parens-depth
/// aware) and resolve each argument through the owned-value lookup
/// surface. Returns `None` when any unbalanced parens / quotes
/// surface, so a malformed call falls through to the next
/// resolution layer rather than silently producing wrong data.
///
/// Keyword arguments (`l=+0.05`, `c=-0.02`) are sliced off here —
/// they go into the second tuple element. Today only the colour
/// `with` call consumes kwargs; everything else passes positional
/// args and ignores the kwargs vec (which stays empty for pure
/// positional calls).
fn parse_call_args(s: &str, scope: &LowerScope) -> Option<ParsedCallArgs> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return Some((Vec::new(), Vec::new()));
    }
    let mut positional = Vec::new();
    let mut kwargs = Vec::new();
    let mut current = String::new();
    let mut depth = 0i32;
    let mut in_str: Option<char> = None;
    let flush = |slot: &mut String,
                 positional: &mut Vec<serde_json::Value>,
                 kwargs: &mut Vec<(String, String)>|
     -> Option<()> {
        let raw = slot.trim();
        if raw.is_empty() {
            slot.clear();
            return Some(());
        }
        if let Some((key, value)) = split_kwarg(raw) {
            kwargs.push((key.to_string(), value.to_string()));
        } else {
            positional.push(eval_call_arg(raw, scope)?);
        }
        slot.clear();
        Some(())
    };
    for ch in trimmed.chars() {
        match (in_str, ch) {
            (Some(q), c) if c == q => {
                in_str = None;
                current.push(c);
            }
            (Some(_), c) => current.push(c),
            (None, '\'') | (None, '"') => {
                in_str = Some(ch);
                current.push(ch);
            }
            (None, '(' | '[') => {
                depth += 1;
                current.push(ch);
            }
            (None, ')' | ']') => {
                depth -= 1;
                if depth < 0 {
                    return None;
                }
                current.push(ch);
            }
            (None, ',') if depth == 0 => {
                flush(&mut current, &mut positional, &mut kwargs)?;
            }
            (None, c) => current.push(c),
        }
    }
    if depth != 0 || in_str.is_some() {
        return None;
    }
    flush(&mut current, &mut positional, &mut kwargs)?;
    Some((positional, kwargs))
}

/// `key=value` slicer for [`parse_call_args`]. Returns `Some((key, value))`
/// when `raw` starts with a bare ident followed by `=` and a non-empty
/// RHS; otherwise `None` so the caller treats the slot as positional.
/// Quotes / nested parens inside `value` survive verbatim — kwarg
/// callers that need typed values run their own coercion on the RHS.
fn split_kwarg(raw: &str) -> Option<(&str, &str)> {
    let eq = raw.find('=')?;
    let key = raw[..eq].trim();
    let val = raw[eq + 1..].trim();
    if key.is_empty()
        || val.is_empty()
        || !key
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
        || !key.chars().next().unwrap_or(' ').is_alphabetic()
    {
        return None;
    }
    Some((key, val))
}

/// Resolve a single call argument to a typed JSON value. Tries, in
/// order: number literal, string literal (`'…'` / `"…"`),
/// `true`/`false`/`null`, then bare path / nested call via
/// [`lookup_path_owned`]. Returns `None` when nothing matches so
/// the caller fails the whole call cleanly.
fn eval_call_arg(s: &str, scope: &LowerScope) -> Option<serde_json::Value> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    if let Ok(n) = s.parse::<i64>() {
        return Some(serde_json::Value::from(n));
    }
    if let Ok(f) = s.parse::<f64>() {
        return Some(serde_json::Value::from(f));
    }
    if let (Some('\''), Some('\'')) = (s.chars().next(), s.chars().last()) {
        if s.len() >= 2 {
            return Some(serde_json::Value::from(&s[1..s.len() - 1]));
        }
    }
    if let (Some('"'), Some('"')) = (s.chars().next(), s.chars().last()) {
        if s.len() >= 2 {
            return Some(serde_json::Value::from(&s[1..s.len() - 1]));
        }
    }
    match s {
        "true" => return Some(serde_json::Value::Bool(true)),
        "false" => return Some(serde_json::Value::Bool(false)),
        "null" => return Some(serde_json::Value::Null),
        _ => {}
    }
    lookup_path_owned(s, scope)
}

/// Dispatch a parsed call to its implementation. Pure transformation
/// over `Vec<Value>` — no scope access here; everything resolves at
/// arg-parse time so the implementations stay test-friendly.
/// **Wave F** — parse `#rgb` / `#rrggbb` / `#rrggbbaa` into RGBA
/// (alpha defaults 255). Returns `None` for anything else (a token
/// reference that didn't resolve, a named colour) so the caller
/// fails the call cleanly.
fn parse_hex_rgba(s: &str) -> Option<(u8, u8, u8, u8)> {
    let h = s.trim().strip_prefix('#')?;
    let b = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).ok();
    match h.len() {
        3 => {
            let d = |i: usize| u8::from_str_radix(&h[i..i + 1], 16).ok().map(|v| v * 17);
            Some((d(0)?, d(1)?, d(2)?, 255))
        }
        6 => Some((b(0)?, b(2)?, b(4)?, 255)),
        8 => Some((b(0)?, b(2)?, b(4)?, b(6)?)),
        _ => None,
    }
}

fn rgba_hex(r: u8, g: u8, b: u8, a: u8) -> String {
    format!("#{r:02x}{g:02x}{b:02x}{a:02x}")
}

/// **§7.12 (Phase 1)** — colour math for computed PRSS / expression
/// slots, OKLCH-backed. `darken` / `lighten` / `mix` lerp in OKLab so
/// `darken('#3b82f6', 0.3)` produces a *vivid* darker blue, not a
/// desaturated grey-blue. `alpha` sets the alpha channel directly;
/// the chroma path is bypassed for it (alpha is RGB-orthogonal).
///
/// `with(c, l=, c=, h=, a=)` adjusts any OKLCh channel plus alpha;
/// leading `+`/`-` on a kwarg RHS marks a *delta* (added to the
/// current channel), bare numbers are *absolute* (override the
/// channel). `saturate(c, t)` / `desaturate(c, t)` are shorthands
/// for `with(c, c=±t * current-chroma)` — they scale chroma
/// multiplicatively, not additively, so a value-neutral grey stays
/// grey under `saturate`.
///
/// `amount`/`t` are clamped to `0.0..=1.0`; a non-colour first arg
/// (or a number that doesn't parse) returns `None` so the caller
/// fails the whole call cleanly.
fn eval_color_call(
    name: &str,
    args: &[serde_json::Value],
    kwargs: &[(String, String)],
) -> Option<serde_json::Value> {
    use super::color::{self, ChannelAdjust, ChannelAdjustments};
    let as_f = |v: &serde_json::Value| match v {
        serde_json::Value::Number(n) => n.as_f64(),
        serde_json::Value::String(s) => s.parse::<f64>().ok(),
        _ => None,
    };
    let as_hex = |v: &serde_json::Value| match v {
        serde_json::Value::String(s) => parse_hex_rgba(s),
        _ => None,
    };
    match name {
        "darken" => {
            let (r, g, b, a) = as_hex(args.first()?)?;
            let amt = as_f(args.get(1)?)?;
            let (rr, gg, bb) = color::darken(r, g, b, amt);
            Some(serde_json::Value::String(rgba_hex(rr, gg, bb, a)))
        }
        "lighten" => {
            let (r, g, b, a) = as_hex(args.first()?)?;
            let amt = as_f(args.get(1)?)?;
            let (rr, gg, bb) = color::lighten(r, g, b, amt);
            Some(serde_json::Value::String(rgba_hex(rr, gg, bb, a)))
        }
        "alpha" => {
            let (r, g, b, _) = as_hex(args.first()?)?;
            let a = (as_f(args.get(1)?)?.clamp(0.0, 1.0) * 255.0).round() as u8;
            Some(serde_json::Value::String(rgba_hex(r, g, b, a)))
        }
        "mix" => {
            let (r1, g1, b1, a1) = as_hex(args.first()?)?;
            let (r2, g2, b2, a2) = as_hex(args.get(1)?)?;
            let t = as_f(args.get(2)?).unwrap_or(0.5).clamp(0.0, 1.0);
            let (r, g, b) = color::mix(r1, g1, b1, r2, g2, b2, t);
            // Alpha lerps linearly — orthogonal to OKLCh adjustment.
            let a = (a1 as f64 + (a2 as f64 - a1 as f64) * t).round() as u8;
            Some(serde_json::Value::String(rgba_hex(r, g, b, a)))
        }
        "with" => {
            let (r, g, b, a) = as_hex(args.first()?)?;
            // Kwargs: l / c / h / a. Leading `+` or `-` on the RHS
            // marks a delta; bare number is absolute.
            let parse_adj = |raw: &str| -> Option<ChannelAdjust> {
                let r = raw.trim();
                if let Some(rest) = r.strip_prefix('+') {
                    Some(ChannelAdjust::Delta(rest.trim().parse::<f64>().ok()?))
                } else if r.starts_with('-') {
                    // Leading minus is part of the number; parse as
                    // delta but keep the sign.
                    Some(ChannelAdjust::Delta(r.parse::<f64>().ok()?))
                } else {
                    Some(ChannelAdjust::Set(r.parse::<f64>().ok()?))
                }
            };
            let mut adj = ChannelAdjustments::default();
            for (k, v) in kwargs {
                let parsed = parse_adj(v)?;
                match k.as_str() {
                    "l" => adj.l = Some(parsed),
                    "c" => adj.c = Some(parsed),
                    "h" => adj.h = Some(parsed),
                    "a" => adj.a = Some(parsed),
                    _ => return None,
                }
            }
            let (rr, gg, bb, aa) = color::with_channels(r, g, b, a, adj);
            Some(serde_json::Value::String(rgba_hex(rr, gg, bb, aa)))
        }
        "saturate" | "desaturate" => {
            // Scale chroma by `1 ± t` — multiplicative so a grey
            // input stays grey (current chroma is 0; scaling 0 by
            // anything is still 0). The `with` axis is absolute, so
            // we compute the target chroma here and feed it as
            // `c = Set(target)`.
            let (r, g, b, a) = as_hex(args.first()?)?;
            let amt = as_f(args.get(1)?)?.clamp(0.0, 1.0);
            let (cl, cc, ch) = color::srgb_to_oklch(r, g, b);
            let scale = if name == "saturate" {
                1.0 + amt
            } else {
                1.0 - amt
            };
            let (rr, gg, bb) = color::oklch_to_srgb(cl, (cc * scale).max(0.0), ch);
            Some(serde_json::Value::String(rgba_hex(rr, gg, bb, a)))
        }
        _ => None,
    }
}

fn eval_array_call(name: &str, args: &[serde_json::Value]) -> Option<serde_json::Value> {
    use serde_json::Value;
    let arr_arg = |i: usize| match args.get(i) {
        Some(Value::Array(a)) => Some(a),
        _ => None,
    };
    let str_arg = |i: usize| match args.get(i) {
        Some(Value::String(s)) => Some(s.as_str()),
        _ => None,
    };
    let i64_arg = |i: usize| match args.get(i) {
        Some(Value::Number(n)) => n.as_i64(),
        _ => None,
    };
    match name {
        "map" => {
            let arr = arr_arg(0)?;
            let field = str_arg(1)?;
            Some(Value::Array(
                arr.iter()
                    .map(|item| match item {
                        Value::Object(map) => map.get(field).cloned().unwrap_or(Value::Null),
                        _ => Value::Null,
                    })
                    .collect(),
            ))
        }
        "filter" => {
            let arr = arr_arg(0)?;
            let field = str_arg(1)?;
            let needle = args.get(2)?;
            let out: Vec<Value> = arr
                .iter()
                .filter(|item| match item {
                    Value::Object(map) => map
                        .get(field)
                        .map(|v| values_loose_eq(v, needle))
                        .unwrap_or(false),
                    _ => false,
                })
                .cloned()
                .collect();
            Some(Value::Array(out))
        }
        "find" => {
            let arr = arr_arg(0)?;
            let field = str_arg(1)?;
            let needle = args.get(2)?;
            for item in arr {
                if let Value::Object(map) = item {
                    if let Some(v) = map.get(field) {
                        if values_loose_eq(v, needle) {
                            return Some(item.clone());
                        }
                    }
                }
            }
            Some(Value::Null)
        }
        "reduce" => {
            // `reduce(arr, "op" [, "field"])` — op is one of:
            //   sum, product, min, max, count, avg.
            // Optional third arg names a field on object items; absent
            // means treat each item as a number directly.
            let arr = arr_arg(0)?;
            let op = str_arg(1)?;
            let field = str_arg(2);
            let extract = |item: &Value| -> Option<f64> {
                let target = match field {
                    Some(f) => match item {
                        Value::Object(map) => map.get(f)?,
                        _ => return None,
                    },
                    None => item,
                };
                match target {
                    Value::Number(n) => n.as_f64(),
                    Value::String(s) => s.parse::<f64>().ok(),
                    Value::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
                    _ => None,
                }
            };
            let nums: Vec<f64> = arr.iter().filter_map(extract).collect();
            let n = match op {
                "sum" => nums.iter().sum::<f64>(),
                "product" => nums.iter().product::<f64>(),
                "min" => nums.iter().copied().fold(f64::INFINITY, f64::min),
                "max" => nums.iter().copied().fold(f64::NEG_INFINITY, f64::max),
                "count" => nums.len() as f64,
                "avg" => {
                    if nums.is_empty() {
                        0.0
                    } else {
                        nums.iter().sum::<f64>() / nums.len() as f64
                    }
                }
                _ => return None,
            };
            if n.is_finite() && n.fract() == 0.0 && n.abs() <= i64::MAX as f64 {
                Some(Value::from(n as i64))
            } else {
                serde_json::Number::from_f64(n).map(Value::Number)
            }
        }
        "slice" => {
            let arr = arr_arg(0)?;
            let len = arr.len() as i64;
            let normalize = |n: i64| -> usize {
                if n < 0 {
                    ((len + n).max(0)) as usize
                } else {
                    (n.min(len)) as usize
                }
            };
            let start = normalize(i64_arg(1).unwrap_or(0));
            let end = normalize(i64_arg(2).unwrap_or(len));
            if start >= end {
                return Some(Value::Array(Vec::new()));
            }
            Some(Value::Array(arr[start..end].to_vec()))
        }
        "sort_by" => {
            let arr = arr_arg(0)?;
            let field = str_arg(1)?;
            let order = str_arg(2).unwrap_or("asc");
            let mut out: Vec<Value> = arr.clone();
            out.sort_by(|a, b| {
                let av = a.get(field);
                let bv = b.get(field);
                compare_values(av, bv)
            });
            if order == "desc" {
                out.reverse();
            }
            Some(Value::Array(out))
        }
        "unique" => {
            let arr = arr_arg(0)?;
            let mut seen: Vec<Value> = Vec::with_capacity(arr.len());
            for item in arr {
                if !seen.iter().any(|s| values_loose_eq(s, item)) {
                    seen.push(item.clone());
                }
            }
            Some(Value::Array(seen))
        }
        "reverse" => {
            // Single-arg array reverse — distinct from `reverse_arr`
            // and the `for=` `reverse` modifier; this returns a new
            // typed array suitable for downstream consumers (`{ first
            // = reverse(items).first }`).
            let arr = arr_arg(0)?;
            let mut out = arr.clone();
            out.reverse();
            Some(Value::Array(out))
        }
        "keys" => match args.first()? {
            Value::Object(map) => Some(Value::Array(
                map.keys().map(|k| Value::String(k.clone())).collect(),
            )),
            _ => None,
        },
        "values" => match args.first()? {
            Value::Object(map) => Some(Value::Array(map.values().cloned().collect())),
            _ => None,
        },
        "entries" => match args.first()? {
            Value::Object(map) => Some(Value::Array(
                map.iter()
                    .map(|(k, v)| {
                        let mut entry = serde_json::Map::new();
                        entry.insert("key".to_string(), Value::String(k.clone()));
                        entry.insert("value".to_string(), v.clone());
                        Value::Object(entry)
                    })
                    .collect(),
            )),
            _ => None,
        },
        "includes" => {
            let arr = arr_arg(0)?;
            let needle = args.get(1)?;
            Some(Value::Bool(arr.iter().any(|v| values_loose_eq(v, needle))))
        }
        "index_of" => {
            let arr = arr_arg(0)?;
            let needle = args.get(1)?;
            Some(Value::from(
                arr.iter()
                    .position(|v| values_loose_eq(v, needle))
                    .map(|i| i as i64)
                    .unwrap_or(-1),
            ))
        }
        "join" => {
            let arr = arr_arg(0)?;
            let sep = str_arg(1).unwrap_or(",");
            let s = arr
                .iter()
                .map(stringify_value)
                .collect::<Vec<_>>()
                .join(sep);
            Some(Value::String(s))
        }
        "concat_arr" => {
            let mut out: Vec<Value> = Vec::new();
            for a in args {
                if let Value::Array(items) = a {
                    out.extend(items.iter().cloned());
                }
            }
            Some(Value::Array(out))
        }
        "take" => {
            // `take(arr, n)` — first N items. `n <= 0` returns empty;
            // `n >= len` returns the whole array.
            let arr = arr_arg(0)?;
            let n = i64_arg(1).unwrap_or(0).max(0) as usize;
            Some(Value::Array(arr.iter().take(n).cloned().collect()))
        }
        "drop" => {
            // `drop(arr, n)` — every item AFTER the first N. Pair to
            // `take` for pagination patterns.
            let arr = arr_arg(0)?;
            let n = i64_arg(1).unwrap_or(0).max(0) as usize;
            Some(Value::Array(arr.iter().skip(n).cloned().collect()))
        }
        "pluck" => {
            // Alias for `map(arr, "field")` — same semantics, name
            // matches the lodash / underscore vocabulary so authors
            // coming from those libraries reach for the obvious word.
            let arr = arr_arg(0)?;
            let field = str_arg(1)?;
            Some(Value::Array(
                arr.iter()
                    .map(|item| match item {
                        Value::Object(map) => map.get(field).cloned().unwrap_or(Value::Null),
                        _ => Value::Null,
                    })
                    .collect(),
            ))
        }
        "group_by" => {
            // `group_by(arr, "field")` — IndexMap-shaped object whose
            // keys are the distinct field values (in first-seen order)
            // and values are arrays of the items that share that key.
            // Round-trips through `for="entry in entries(group_by(…))"`
            // so authors can render section-per-group views.
            let arr = arr_arg(0)?;
            let field = str_arg(1)?;
            let mut groups: serde_json::Map<String, Value> = serde_json::Map::new();
            for item in arr {
                let key = match item.get(field) {
                    Some(Value::String(s)) => s.clone(),
                    Some(other) => stringify_value(other),
                    None => String::new(),
                };
                groups
                    .entry(key)
                    .or_insert_with(|| Value::Array(Vec::new()))
                    .as_array_mut()
                    .unwrap()
                    .push(item.clone());
            }
            Some(Value::Object(groups))
        }
        "count_by" => {
            // `count_by(arr, "field")` — like `group_by` but values
            // are counts rather than item lists. Powers
            // `{"draft": 4, "active": 12}` summaries.
            let arr = arr_arg(0)?;
            let field = str_arg(1)?;
            let mut counts: serde_json::Map<String, Value> = serde_json::Map::new();
            for item in arr {
                let key = match item.get(field) {
                    Some(Value::String(s)) => s.clone(),
                    Some(other) => stringify_value(other),
                    None => String::new(),
                };
                let entry = counts.entry(key).or_insert_with(|| Value::from(0i64));
                let next = entry.as_i64().unwrap_or(0) + 1;
                *entry = Value::from(next);
            }
            Some(Value::Object(counts))
        }
        "any" => {
            // `any(arr, "field", value)` — true iff at least one item
            // matches the field test. Single-arg form `any(arr)`
            // returns "does the array contain a truthy value" — useful
            // when an upstream stage already filtered.
            let arr = arr_arg(0)?;
            let field = str_arg(1);
            let needle = args.get(2);
            Some(Value::Bool(arr.iter().any(|item| match (field, needle) {
                (Some(f), Some(n)) => item.get(f).map(|v| values_loose_eq(v, n)).unwrap_or(false),
                _ => is_truthy_value(item),
            })))
        }
        "all" => {
            // Sibling to `any` — every item must match. Empty array →
            // true (vacuous truth, matches Rust `all`'s shape).
            let arr = arr_arg(0)?;
            let field = str_arg(1);
            let needle = args.get(2);
            Some(Value::Bool(arr.iter().all(|item| match (field, needle) {
                (Some(f), Some(n)) => item.get(f).map(|v| values_loose_eq(v, n)).unwrap_or(false),
                _ => is_truthy_value(item),
            })))
        }
        "chunk" => {
            // `chunk(arr, n)` — split into fixed-size sub-arrays. The
            // final chunk is short when `arr.len() % n != 0`. `n <= 0`
            // returns the whole array as a single chunk to mimic the
            // lodash shape and keep callers from accidentally producing
            // infinite iteration on a misconfigured size.
            let arr = arr_arg(0)?;
            let n = i64_arg(1).unwrap_or(1).max(1) as usize;
            let out: Vec<Value> = arr
                .chunks(n)
                .map(|slice| Value::Array(slice.to_vec()))
                .collect();
            Some(Value::Array(out))
        }
        "zip" => {
            // `zip(a, b, …)` — produce an array of N-tuples (as
            // arrays), one per index, up to the shortest input. Used
            // for "render rows from two parallel lists" patterns
            // (e.g. headers + values).
            let arrays: Vec<&Vec<Value>> = args
                .iter()
                .filter_map(|v| {
                    if let Value::Array(a) = v {
                        Some(a)
                    } else {
                        None
                    }
                })
                .collect();
            if arrays.is_empty() {
                return Some(Value::Array(Vec::new()));
            }
            let len = arrays.iter().map(|a| a.len()).min().unwrap_or(0);
            let zipped: Vec<Value> = (0..len)
                .map(|i| Value::Array(arrays.iter().map(|a| a[i].clone()).collect()))
                .collect();
            Some(Value::Array(zipped))
        }
        "range" => {
            // `range(n)` / `range(start, end)` / `range(start, end,
            // step)` — produces an integer array. The same `0..n`
            // numbers a `for=` clause natively supports, but as a
            // standalone array value so the result can be passed
            // around as data (`pluck`, `concat_arr`, etc.).
            let (start, end, step) = match args.len() {
                1 => (0, i64_arg(0)?, 1),
                2 => (i64_arg(0)?, i64_arg(1)?, 1),
                3 => (i64_arg(0)?, i64_arg(1)?, i64_arg(2)?.max(1)),
                _ => return None,
            };
            if start >= end {
                return Some(Value::Array(Vec::new()));
            }
            let nums: Vec<Value> = (start..end)
                .step_by(step as usize)
                .map(Value::from)
                .collect();
            Some(Value::Array(nums))
        }
        _ => None,
    }
}

/// Truthiness predicate over a raw JSON value — matches the
/// `eval_truthy` rule (null / false / 0 / "" / [] / {} → false;
/// anything else → true). Shared by `any` / `all` so a single-arg
/// call (`any(rows)`) reads through the same vocabulary `if=`
/// uses.
fn is_truthy_value(v: &serde_json::Value) -> bool {
    use serde_json::Value;
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().map(|f| f != 0.0).unwrap_or(false),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}

/// Loose equality for typed JSON values — matches the expression
/// evaluator's `loose_eq` shape so `filter(rows, "id", "x")` reads
/// strings, `filter(rows, "count", 3)` reads numbers, and bool↔int
/// coercion stays consistent.
fn values_loose_eq(a: &serde_json::Value, b: &serde_json::Value) -> bool {
    use serde_json::Value;
    match (a, b) {
        (Value::Null, Value::Null) => true,
        (Value::Bool(x), Value::Bool(y)) => x == y,
        (Value::Number(x), Value::Number(y)) => x.as_f64() == y.as_f64(),
        (Value::String(x), Value::String(y)) => x == y,
        (Value::String(s), Value::Number(n)) | (Value::Number(n), Value::String(s)) => {
            s.parse::<f64>().ok() == n.as_f64()
        }
        (Value::Bool(b), Value::Number(n)) | (Value::Number(n), Value::Bool(b)) => {
            (if *b { 1.0 } else { 0.0 }) == n.as_f64().unwrap_or(0.0)
        }
        _ => stringify_value(a) == stringify_value(b),
    }
}

/// Order JSON values for `sort_by`. Numbers / strings sort naturally;
/// missing fields (`None`) sort last; mixed kinds fall back to
/// stringified comparison so iteration stays total.
fn compare_values(
    a: Option<&serde_json::Value>,
    b: Option<&serde_json::Value>,
) -> std::cmp::Ordering {
    use serde_json::Value;
    use std::cmp::Ordering;
    match (a, b) {
        (None, None) => Ordering::Equal,
        (None, _) => Ordering::Greater,
        (_, None) => Ordering::Less,
        (Some(Value::Number(x)), Some(Value::Number(y))) => x
            .as_f64()
            .unwrap_or(0.0)
            .partial_cmp(&y.as_f64().unwrap_or(0.0))
            .unwrap_or(Ordering::Equal),
        (Some(Value::String(x)), Some(Value::String(y))) => x.cmp(y),
        (Some(Value::Bool(x)), Some(Value::Bool(y))) => x.cmp(y),
        (Some(x), Some(y)) => stringify_value(x).cmp(&stringify_value(y)),
    }
}

/// Truthy evaluator for `if=` / `else-if=`. Routes through the full
/// expression evaluator in [`evaluate_expression`] so authors get
/// ternary, boolean `||`/`&&`/`!`, comparisons, arithmetic, and dotted
/// paths uniformly. The bare-path fast path (`{row.selected}`) still
/// resolves through [`lookup_expression`] so a directly-bound JSON
/// `Value` keeps its native truthy rule (empty arrays / empty objects
/// are falsy — the expression coercion in [`ExprValue::to_boolean`]
/// would otherwise stringify them).
pub(super) fn eval_truthy(body: &str, scope: &LowerScope) -> bool {
    let body = body
        .trim()
        .trim_start_matches('{')
        .trim_end_matches('}')
        .trim();
    if body.is_empty() {
        return false;
    }
    // Direct binding lookup — handles the `if="{row.selected}"` shape
    // where the bound value is a typed JSON value (array / object /
    // bool). Virtual `.length`/`.first`/`.last` segments resolve here
    // too, so `if="{items.length}"` is true iff non-empty without any
    // operator. Falls through for any operator-bearing expression.
    if let Some(v) = lookup_path_owned(body, scope) {
        return match v {
            serde_json::Value::Null => false,
            serde_json::Value::Bool(b) => b,
            serde_json::Value::Number(n) => n.as_f64().map(|f| f != 0.0).unwrap_or(false),
            serde_json::Value::String(s) => !s.is_empty(),
            serde_json::Value::Array(a) => !a.is_empty(),
            serde_json::Value::Object(o) => !o.is_empty(),
        };
    }
    // Operator-bearing expressions (`a == 'b'`, `enabled && !disabled`,
    // ternary heads, etc.) flow through the full Prism expression
    // evaluator. `None` means parse failure or unresolved operand —
    // treated as falsy, matching the JS rule.
    match evaluate_expression(body, scope) {
        Some(serde_json::Value::Null) | None => false,
        Some(serde_json::Value::Bool(b)) => b,
        Some(serde_json::Value::Number(n)) => n.as_f64().map(|f| f != 0.0).unwrap_or(false),
        Some(serde_json::Value::String(s)) => !s.is_empty(),
        Some(serde_json::Value::Array(a)) => !a.is_empty(),
        Some(serde_json::Value::Object(o)) => !o.is_empty(),
    }
}

/// Evaluate a `{...}` expression body against the active scope and
/// return its computed JSON value. Wave 11.2 substrate: gives every
/// authored attribute access to ternary (`a ? b : c`), boolean
/// (`&& || !`), comparison (`== != < <= > >=`), arithmetic (`+ - * / %`),
/// dotted paths (`item.label`, `tabs.0.name`), and the Prism expression
/// builtins (`upper`, `len`, `min`, `concat`, …) — all in one pass
/// through the existing `prism_core::language::expression` parser +
/// evaluator. No hand-rolled regex or string-indexed parsing.
///
/// Returns `None` when the body is empty, the parser surfaces errors,
/// or the result coerces to a JSON null. Callers fall back to their
/// type-specific default (empty string for templates, false for
/// `if=`, etc.).
#[doc(hidden)]
pub fn evaluate_expression(body: &str, scope: &LowerScope) -> Option<serde_json::Value> {
    use prism_core::language::expression::{evaluate, parse as parse_expr, ExprValue, ValueStore};

    let trimmed = body.trim();
    if trimmed.is_empty() {
        return None;
    }
    let parsed = parse_expr(trimmed);
    if !parsed.errors.is_empty() {
        return None;
    }
    let node = parsed.node?;

    struct ScopeStore<'a> {
        scope: &'a LowerScope,
    }
    impl<'a> ValueStore for ScopeStore<'a> {
        fn resolve(&self, operand_type: &str, id: &str, subfield: Option<&str>) -> ExprValue {
            if operand_type != "field" {
                return ExprValue::String(String::new());
            }
            // Compose the dotted path the way `lookup_path_owned` reads
            // it, so virtual `.length` / `.first` / `.last` segments
            // resolve the same way they do in bare-path lookups —
            // `items.length > 0` is the symmetric operator-bearing form
            // of `if="{items.length}"`.
            let full_path = match subfield {
                Some(rest) => format!("{}.{}", id, rest),
                None => id.to_string(),
            };
            match lookup_path_owned(&full_path, self.scope) {
                Some(v) => json_to_expr_value(&v),
                // Missing path → Null. Authors write `value == null`
                // to detect missing-key absence (Wave 11.3 of
                // composable-builder-plan.md). Falsy in boolean
                // ladders, empty in string ladders, zero in numeric
                // ladders — same coercion table the JS `null` has.
                None => ExprValue::Null,
            }
        }
    }
    let store = ScopeStore { scope };
    Some(expr_value_to_json(evaluate(&node, &store)))
}

fn json_to_expr_value(v: &serde_json::Value) -> prism_core::language::expression::ExprValue {
    use prism_core::language::expression::ExprValue;
    match v {
        serde_json::Value::Bool(b) => ExprValue::Boolean(*b),
        serde_json::Value::Number(n) => ExprValue::Number(n.as_f64().unwrap_or(0.0)),
        serde_json::Value::String(s) => ExprValue::String(s.clone()),
        serde_json::Value::Null => ExprValue::Null,
        // Arrays and objects don't participate in arithmetic / comparison
        // — fall through as their JSON-stringified form so authors who
        // accidentally compare an object stringify-compare instead of
        // crashing the render.
        serde_json::Value::Array(_) | serde_json::Value::Object(_) => {
            ExprValue::String(v.to_string())
        }
    }
}

fn expr_value_to_json(v: prism_core::language::expression::ExprValue) -> serde_json::Value {
    use prism_core::language::expression::ExprValue;
    match v {
        ExprValue::Boolean(b) => serde_json::Value::Bool(b),
        // Prefer the integer encoding when the value is exact — keeps
        // `Number(5)` stringifying as `"5"` rather than `"5.0"` so a
        // count derived through the evaluator surface (e.g.
        // `items.length + 1`) reads identically to one derived through
        // the direct `lookup_path_owned` surface.
        ExprValue::Number(n) => {
            if n.is_finite() && n.fract() == 0.0 && n >= i64::MIN as f64 && n <= i64::MAX as f64 {
                serde_json::Value::Number(serde_json::Number::from(n as i64))
            } else {
                serde_json::Number::from_f64(n)
                    .map(serde_json::Value::Number)
                    .unwrap_or(serde_json::Value::Null)
            }
        }
        ExprValue::String(s) => serde_json::Value::String(s),
        ExprValue::Null => serde_json::Value::Null,
    }
}

pub(super) fn stringify_value(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// Public-but-`#[doc(hidden)]` mirror of [`stringify_value`] for the
/// `prism-builder` resolver's templated-attribute path. Same shape:
/// strings pass through verbatim, nulls become empty, everything
/// else uses `Display`.
#[doc(hidden)]
pub fn stringify_value_for_template(value: &serde_json::Value) -> String {
    stringify_value(value)
}
