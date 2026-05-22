//! Phase 15 — state-variant nested style records (Shape 1) +
//! pipeline form (Shape 2) per §7.6.
//!
//! Two new syntactic shapes ride on top of the existing
//! `style:<key>` / `style:<key>:<state>` vocabulary:
//!
//! 1. **Nested record** — `style={ background = accent, radius = 8,
//!    :hovered = { background = lighten(0.1) }, :pressed = { … } }`.
//!    The record collapses an N×M property × state grid into one
//!    declaration. We parse the record and emit per-property /
//!    per-state applies through the same [`super::style::apply_style_override`]
//!    seam the flat form uses.
//!
//! 2. **Pipeline** — `style.background={ accent | :hovered →
//!    lighten(0.1) | :pressed → darken(0.1) | :disabled → mute }`.
//!    The pipeline lists base-then-state deltas for one property.
//!    We split on top-level `|`, parse `state → value` per segment,
//!    and route each delta through the same flat-form seam.
//!
//! Both forms are purely a parse-time fan-out — no new runtime
//! data path. The existing per-key, per-state state-overrides
//! machinery handles the heavy lifting (hover swap, pressed bucket,
//! transition curves).
//!
//! Implicit `self` (the §7.6 footnote: `lighten(0.1)` inside
//! `:hovered.background` is sugar for `lighten(parent.background,
//! 0.1)`) lands as Phase 17 polish; today the explicit two-arg
//! form is the canonical authoring shape.

use crate::layout::ContainerProps;

use super::style::apply_style_override;
use super::LowerScope;

/// Expand a `style={…}` brace-record value into per-property /
/// per-state applies on `props`. The record body is the verbatim
/// expression text between the surrounding braces (no leading `{`,
/// no trailing `}` — the canonical reader strips them when projecting
/// onto `AttributeValue::Expression`).
///
/// Top-level entries are split on `,` (depth-aware over nested
/// braces). Each entry is `<key> = <value>`. Keys starting with `:`
/// are state buckets whose value must be a nested `{…}` record of
/// property assignments; bare keys are base-state properties applied
/// directly to `props`.
pub fn expand_style_record(body: &str, props: &mut ContainerProps, scope: &LowerScope) {
    let body = body.trim();
    if body.is_empty() {
        return;
    }
    let body = strip_one_brace_pair(body).unwrap_or(body);
    for entry in split_top_level_commas(body) {
        let entry = entry.trim();
        if entry.is_empty() {
            continue;
        }
        let Some((key, value)) = split_kv_top_level(entry) else {
            continue;
        };
        let key = key.trim();
        let value = value.trim();
        if let Some(state) = key.strip_prefix(':') {
            // Nested state record: `:hovered = { background = …, radius = … }`.
            let inner = strip_one_brace_pair(value).unwrap_or(value);
            for sub in split_top_level_commas(inner) {
                let Some((prop, val)) = split_kv_top_level(sub.trim()) else {
                    continue;
                };
                let local = format!("{}:{}", prop.trim(), state.trim());
                let resolved = resolve_value(val.trim(), scope);
                apply_style_override(props, &local, &resolved);
            }
        } else {
            // Base-state property: `background = accent`.
            let resolved = resolve_value(value, scope);
            apply_style_override(props, key, &resolved);
        }
    }
}

/// Expand a `style.<key>={ base | :state → delta | … }` pipeline
/// value onto `props`. The first segment is the base value (applied
/// to the key without a state suffix); each subsequent `:state →
/// value` segment applies as a state override on the same key.
///
/// The unicode arrow `→` and ASCII `->` are both accepted so editors
/// that auto-convert don't break the surface.
pub fn expand_style_pipeline(
    key: &str,
    body: &str,
    props: &mut ContainerProps,
    scope: &LowerScope,
) {
    let body = body.trim();
    if body.is_empty() {
        return;
    }
    let body = strip_one_brace_pair(body).unwrap_or(body);
    let normalised = body.replace('→', "->");
    let segments = split_top_level_pipes(&normalised);
    if segments.is_empty() {
        return;
    }
    // Base = first segment.
    let base = segments[0].trim();
    if !base.is_empty() {
        let resolved = resolve_value(base, scope);
        apply_style_override(props, key, &resolved);
    }
    for seg in &segments[1..] {
        let seg = seg.trim();
        let (state, value) = match seg.split_once("->") {
            Some((s, v)) => (s.trim(), v.trim()),
            None => continue,
        };
        let Some(state) = state.strip_prefix(':') else {
            continue;
        };
        let local = format!("{}:{}", key, state.trim());
        let resolved = resolve_value(value, scope);
        apply_style_override(props, &local, &resolved);
    }
}

/// Quick discriminator — does `body` carry a `|` separator at depth
/// zero (and an arrow somewhere)? The attribute walker uses this to
/// pick `expand_style_pipeline` over the plain single-value apply.
pub fn looks_like_pipeline(body: &str) -> bool {
    let normalised = body.replace('→', "->");
    let pipes = split_top_level_pipes(&normalised);
    pipes.len() > 1 && pipes[1..].iter().any(|s| s.contains("->"))
}

/// Quick discriminator — does `body` carry a `:` state key at depth
/// zero (the record shape `:hovered = {…}`)? Used by the attribute
/// walker to pick `expand_style_record` over generic key=value.
pub fn looks_like_record(body: &str) -> bool {
    let body = body.trim();
    let body = strip_one_brace_pair(body).unwrap_or(body);
    for entry in split_top_level_commas(body) {
        let entry = entry.trim();
        // A record entry is `<key> = <value>` — split on the first
        // top-level `=`.
        let Some((key, _)) = split_kv_top_level(entry) else {
            continue;
        };
        let key = key.trim();
        if key.starts_with(':')
            || matches!(
                key,
                "background"
                    | "radius"
                    | "padding"
                    | "gap"
                    | "width"
                    | "height"
                    | "opacity"
                    | "color"
            )
        {
            return true;
        }
    }
    false
}

/// Resolve a record's RHS into the string form `apply_style_override`
/// consumes. Two shapes need translation:
///
/// - A bare token (`accent`, `mute`) → look up
///   `tokens.colors.<name>` through the scope's binding map and
///   return the resolved hex string. Falls back to the raw token
///   for unrecognised names so a one-off literal still applies.
/// - An interpolation-shaped value (`lighten(accent, 0.1)`) → run
///   it through the expression evaluator; the resolved JSON value
///   stringifies into the flat form.
///
/// Numeric / quoted / hex values pass through verbatim.
fn resolve_value(raw: &str, scope: &LowerScope) -> String {
    let raw = raw.trim();
    if raw.is_empty() {
        return String::new();
    }
    // Quoted string — strip outer quotes for the existing
    // `apply_style_override` parsers.
    if let Some(stripped) = strip_quotes(raw) {
        return stripped.to_string();
    }
    // Hex / numeric literal — round-trip.
    if raw.starts_with('#') || raw.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        return raw.to_string();
    }
    // Call-shaped RHS or bare-identifier → expression evaluator.
    if let Some(value) = super::expression::lookup_path_owned_in_scope(raw, scope) {
        return super::expression::stringify_value_for_template(&value);
    }
    if let Some(value) = super::expression::evaluate_expression(raw, scope) {
        return super::expression::stringify_value_for_template(&value);
    }
    raw.to_string()
}

fn strip_quotes(s: &str) -> Option<&str> {
    let b = s.as_bytes();
    if b.len() < 2 {
        return None;
    }
    let first = b[0];
    let last = b[b.len() - 1];
    if (first == b'"' || first == b'\'') && first == last {
        return Some(&s[1..s.len() - 1]);
    }
    None
}

fn strip_one_brace_pair(s: &str) -> Option<&str> {
    let trimmed = s.trim();
    let inner = trimmed.strip_prefix('{')?.strip_suffix('}')?;
    Some(inner.trim())
}

/// Top-level `=` split that respects depth and quotes. Returns
/// `(key, value)` slices on success. The key may not be empty; the
/// value may be empty (a bare `<key> = ` is malformed — caller skips).
fn split_kv_top_level(entry: &str) -> Option<(&str, &str)> {
    let bytes = entry.as_bytes();
    let mut depth = 0i32;
    let mut quote: Option<u8> = None;
    for (i, &c) in bytes.iter().enumerate() {
        if let Some(q) = quote {
            if c == q {
                quote = None;
            }
            continue;
        }
        match c {
            b'"' | b'\'' => quote = Some(c),
            b'(' | b'[' | b'{' | b'<' => depth += 1,
            b')' | b']' | b'}' | b'>' => depth -= 1,
            b'=' if depth == 0 => {
                let key = entry[..i].trim_end();
                let value = entry[i + 1..].trim_start();
                if key.is_empty() {
                    return None;
                }
                return Some((key, value));
            }
            _ => {}
        }
    }
    None
}

fn split_top_level_commas(body: &str) -> Vec<&str> {
    let bytes = body.as_bytes();
    let mut depth = 0i32;
    let mut quote: Option<u8> = None;
    let mut start = 0usize;
    let mut out: Vec<&str> = Vec::new();
    let mut i = 0usize;
    while i < bytes.len() {
        let c = bytes[i];
        if let Some(q) = quote {
            if c == q {
                quote = None;
            } else if c == b'\\' && i + 1 < bytes.len() {
                i += 1;
            }
            i += 1;
            continue;
        }
        match c {
            b'"' | b'\'' => quote = Some(c),
            b'(' | b'[' | b'{' | b'<' => depth += 1,
            b')' | b']' | b'}' | b'>' => depth -= 1,
            b',' if depth == 0 => {
                let seg = body[start..i].trim();
                if !seg.is_empty() {
                    out.push(seg);
                }
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    if start < bytes.len() {
        let tail = body[start..].trim();
        if !tail.is_empty() {
            out.push(tail);
        }
    }
    out
}

fn split_top_level_pipes(body: &str) -> Vec<&str> {
    let bytes = body.as_bytes();
    let mut depth = 0i32;
    let mut quote: Option<u8> = None;
    let mut start = 0usize;
    let mut out: Vec<&str> = Vec::new();
    let mut i = 0usize;
    // Note: `<` / `>` aren't tracked here because the canonical
    // pipeline syntax (`base | :state -> value`) uses `->` as the
    // segment separator, where the `>` would otherwise decrement
    // depth and confuse the split. Generic type expressions don't
    // appear inside pipeline values.
    while i < bytes.len() {
        let c = bytes[i];
        if let Some(q) = quote {
            if c == q {
                quote = None;
            } else if c == b'\\' && i + 1 < bytes.len() {
                i += 1;
            }
            i += 1;
            continue;
        }
        match c {
            b'"' | b'\'' => quote = Some(c),
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            b'|' if depth == 0 => {
                // Don't split on `||` (logical OR) — Q8 keeps the
                // surrounding context handling that case.
                if i + 1 < bytes.len() && bytes[i + 1] == b'|' {
                    i += 2;
                    continue;
                }
                let seg = body[start..i].trim();
                if !seg.is_empty() {
                    out.push(seg);
                }
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    if start < bytes.len() {
        let tail = body[start..].trim();
        if !tail.is_empty() {
            out.push(tail);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::ContainerProps;

    #[test]
    fn record_applies_base_properties() {
        let mut props = ContainerProps::default();
        let body = "background = #ff0000ff, radius = 8";
        expand_style_record(body, &mut props, &LowerScope::new());
        let bg = props.background.expect("bg");
        assert_eq!((bg.r, bg.g, bg.b, bg.a), (255, 0, 0, 255));
        assert_eq!(props.radius.tl, 8.0);
    }

    #[test]
    fn record_applies_state_overrides() {
        let mut props = ContainerProps::default();
        let body = ":hovered = { background = #00ff00ff }";
        expand_style_record(body, &mut props, &LowerScope::new());
        let hover_bg = props
            .hover
            .as_ref()
            .and_then(|h| h.background)
            .expect("hover bg");
        assert_eq!(hover_bg.g, 255);
    }

    #[test]
    fn record_mixes_base_and_state() {
        let mut props = ContainerProps::default();
        let body = "background = #112233ff, :hovered = { background = #aabbccff }";
        expand_style_record(body, &mut props, &LowerScope::new());
        assert!(props.background.is_some());
        assert!(props.hover.as_ref().and_then(|h| h.background).is_some());
    }

    #[test]
    fn pipeline_applies_base_and_states() {
        let mut props = ContainerProps::default();
        let body = "#112233ff | :hovered -> #aabbccff | :pressed -> #001122ff";
        expand_style_pipeline("background", body, &mut props, &LowerScope::new());
        assert!(props.background.is_some());
        let hover_bg = props.hover.as_ref().and_then(|h| h.background);
        let press_bg = props.pressed.as_ref().and_then(|p| p.background);
        assert!(hover_bg.is_some(), "hover bg missing");
        assert!(press_bg.is_some(), "pressed bg missing");
    }

    #[test]
    fn pipeline_accepts_unicode_arrow() {
        let mut props = ContainerProps::default();
        let body = "#112233ff | :hovered → #aabbccff";
        expand_style_pipeline("background", body, &mut props, &LowerScope::new());
        let hover_bg = props.hover.as_ref().and_then(|h| h.background);
        assert!(hover_bg.is_some());
    }

    #[test]
    fn pipeline_skips_segments_with_no_arrow() {
        let mut props = ContainerProps::default();
        let body = "#112233ff | :hovered #aabbccff";
        expand_style_pipeline("background", body, &mut props, &LowerScope::new());
        assert!(props.background.is_some());
        let hover_bg = props.hover.as_ref().and_then(|h| h.background);
        assert!(
            hover_bg.is_none(),
            "missing arrow segment should be skipped"
        );
    }

    #[test]
    fn looks_like_pipeline_detects_multi_segment_with_arrow() {
        assert!(looks_like_pipeline("a | :hovered → b"));
        assert!(looks_like_pipeline("a | :hovered -> b"));
        assert!(!looks_like_pipeline("a | b"));
        assert!(!looks_like_pipeline("a"));
    }

    #[test]
    fn looks_like_record_detects_property_kv() {
        assert!(looks_like_record("background = accent"));
        assert!(looks_like_record(":hovered = { background = mute }"));
        assert!(!looks_like_record("just-a-string"));
    }

    #[test]
    fn split_top_level_commas_respects_nested_braces() {
        let parts = split_top_level_commas("a = 1, b = { c, d }, e = 2");
        assert_eq!(parts, vec!["a = 1", "b = { c, d }", "e = 2"]);
    }

    #[test]
    fn split_top_level_pipes_ignores_logical_or() {
        let parts = split_top_level_pipes("a || b | :hovered -> c");
        assert_eq!(parts, vec!["a || b", ":hovered -> c"]);
    }

    #[test]
    fn pipeline_with_no_states_applies_base_only() {
        let mut props = ContainerProps::default();
        expand_style_pipeline("background", "#abcdefff", &mut props, &LowerScope::new());
        assert!(props.background.is_some());
        assert!(props.hover.is_none());
    }
}
