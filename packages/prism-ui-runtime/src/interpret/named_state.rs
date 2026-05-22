//! Phase 16 — named state-responsive values (§7.6 Shape 3).
//!
//! A workspace-shared `@color`, `@spacing`, or `@radius` value
//! carries one base and a set of per-state deltas:
//!
//! ```prss
//! @color responsive-accent = accent
//!   | :hovered  → lighten(0.1)
//!   | :pressed  → darken(0.1)
//!   | :disabled → mute
//!
//! [class.btn]     background = responsive-accent
//! [class.alt-btn] background = responsive-accent
//! ```
//!
//! Both buttons inherit the entire base + hover/pressed/disabled
//! curve from one source-of-truth declaration. The N-button × M-state
//! repetition collapses into one declaration plus N references.
//!
//! ## Runtime shape
//!
//! [`StateResponsiveValue`] holds the parsed declaration: a `base`
//! string plus a state-name → delta-string map. A document scope
//! carries an `Arc<HashMap<String, StateResponsiveValue>>` of these
//! (installed by the host's PRSS pre-pass or programmatically via
//! [`super::LowerScope::with_state_responsive_value`]).
//!
//! When the per-attribute walker resolves a value like
//! `style.background={responsive-accent}` or `style:background="responsive-accent"`,
//! the lookup checks the named-value table *first*. A hit fans out
//! through the same `apply_style_override` seam as the Phase 15
//! pipeline form: base value applies to the unsuffixed key, each
//! state delta applies to `<key>:<state>`. A miss falls through to
//! the existing token / hex / literal vocabulary.

use std::collections::HashMap;

/// A single state-responsive value declaration. The `base` is the
/// default value (no state). Each entry in `states` keys a
/// pseudo-state (`hovered`, `pressed`, `disabled`, `focused`, etc.)
/// to its delta value. Values are stored as untyped strings so a
/// single declaration can carry colour / length / number — the same
/// type the underlying property reader expects.
#[derive(Debug, Clone)]
pub struct StateResponsiveValue {
    pub base: String,
    pub states: Vec<(String, String)>,
}

impl StateResponsiveValue {
    pub fn new(base: impl Into<String>) -> Self {
        Self {
            base: base.into(),
            states: Vec::new(),
        }
    }

    /// Add a state delta. Chained builder so a host can write
    /// `StateResponsiveValue::new("accent").with_state("hovered",
    /// "lighten(accent, 0.1)").with_state("pressed", "darken(accent, 0.1)")`.
    pub fn with_state(mut self, state: impl Into<String>, value: impl Into<String>) -> Self {
        self.states.push((state.into(), value.into()));
        self
    }
}

/// Phase 16 — parse a state-responsive pipeline (`base | :hovered →
/// delta | :pressed → delta`) into a [`StateResponsiveValue`]. The
/// unicode arrow `→` and ASCII `->` are both accepted. Returns
/// `None` when the body has no `|` separators (a plain literal
/// doesn't need the state-responsive wrapper).
pub fn parse_state_responsive_pipeline(body: &str) -> Option<StateResponsiveValue> {
    let body = body.trim();
    if body.is_empty() {
        return None;
    }
    let normalised = body.replace('→', "->");
    let segments = split_top_level_pipes(&normalised);
    if segments.len() < 2 {
        return None;
    }
    let base = segments[0].trim().to_string();
    if base.is_empty() {
        return None;
    }
    let mut value = StateResponsiveValue::new(base);
    for seg in &segments[1..] {
        let seg = seg.trim();
        let Some((state, delta)) = seg.split_once("->") else {
            continue;
        };
        let state = state.trim().trim_start_matches(':').trim().to_string();
        if state.is_empty() {
            continue;
        }
        let delta = delta.trim().to_string();
        if delta.is_empty() {
            continue;
        }
        value.states.push((state, delta));
    }
    Some(value)
}

/// Phase 16 — parse a top-level `@color name = pipeline` PRUI
/// declaration into a `(name, value)` pair. The directive shape:
///
/// - `@color name = base | :state → delta | …`
/// - `@spacing name = base | :state → delta | …`
/// - `@radius name = base | :state → delta | …`
///
/// All three live in the same table; the prefix is documentation
/// (each kind targets a different property domain but the runtime
/// data shape is identical — a base plus state deltas). Returns
/// `None` on malformed input.
pub fn parse_at_directive(line: &str) -> Option<(String, StateResponsiveValue)> {
    let line = line.trim();
    let body = line
        .strip_prefix("@color")
        .or_else(|| line.strip_prefix("@spacing"))
        .or_else(|| line.strip_prefix("@radius"))?;
    let body = body.trim_start();
    let (name, rest) = body.split_once('=')?;
    let name = name.trim().to_string();
    if name.is_empty() {
        return None;
    }
    let rest = rest.trim();
    if rest.is_empty() {
        return None;
    }
    // The RHS may carry `|` segments or a bare literal. A bare
    // literal is *still* a valid state-responsive value — it just
    // has no state deltas.
    if rest.contains('|') {
        let value = parse_state_responsive_pipeline(rest)?;
        return Some((name, value));
    }
    Some((name, StateResponsiveValue::new(rest.to_string())))
}

fn split_top_level_pipes(body: &str) -> Vec<&str> {
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
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            b'|' if depth == 0 => {
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

/// Phase 16 — harvest every `@color` / `@spacing` / `@radius`
/// directive from a PRUI document's leading top-level text content
/// or from any inline `<style>` body. Returns a name → value map
/// the runtime installs on `LowerScope::state_responsive_values`.
///
/// The walk is intentionally line-oriented: each `@` directive lives
/// on its own line (the canonical reader doesn't dedicate a parse
/// arm to them, so we sniff them out of raw source-level text).
pub fn harvest_at_directives_from_source(source: &str) -> HashMap<String, StateResponsiveValue> {
    let mut out = HashMap::new();
    for line in source.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with('@') {
            continue;
        }
        if let Some((name, value)) = parse_at_directive(trimmed) {
            out.entry(name).or_insert(value);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_full_pipeline() {
        let v = parse_state_responsive_pipeline(
            "accent | :hovered -> lighten(accent, 0.1) | :pressed -> darken(accent, 0.1) | :disabled -> mute",
        )
        .expect("pipeline should parse");
        assert_eq!(v.base, "accent");
        assert_eq!(v.states.len(), 3);
        assert_eq!(v.states[0].0, "hovered");
        assert_eq!(v.states[1].0, "pressed");
        assert_eq!(v.states[2].0, "disabled");
    }

    #[test]
    fn pipeline_returns_none_for_bare_literal() {
        assert!(parse_state_responsive_pipeline("accent").is_none());
    }

    #[test]
    fn pipeline_accepts_unicode_arrow() {
        let v = parse_state_responsive_pipeline("accent | :hovered → lighten(0.1)").unwrap();
        assert_eq!(v.states[0].0, "hovered");
        assert_eq!(v.states[0].1, "lighten(0.1)");
    }

    #[test]
    fn at_directive_parses_color() {
        let (name, value) =
            parse_at_directive("@color responsive-accent = accent | :hovered -> lighten(0.1)")
                .unwrap();
        assert_eq!(name, "responsive-accent");
        assert_eq!(value.base, "accent");
    }

    #[test]
    fn at_directive_parses_spacing_and_radius() {
        assert!(parse_at_directive("@spacing tight = 4").is_some());
        assert!(parse_at_directive("@radius soft = 8 | :hovered -> 12").is_some());
    }

    #[test]
    fn at_directive_returns_none_without_equals() {
        assert!(parse_at_directive("@color responsive-accent accent").is_none());
        assert!(parse_at_directive("@unknown name = 1").is_none());
    }

    #[test]
    fn at_directive_with_bare_literal_value() {
        let (name, value) = parse_at_directive("@color brand = #112233ff").unwrap();
        assert_eq!(name, "brand");
        assert_eq!(value.base, "#112233ff");
        assert!(value.states.is_empty());
    }

    #[test]
    fn harvest_picks_multiple_directives() {
        let src = r#"@color brand = #ff0000ff | :hovered -> #aa0000ff
@spacing pad = 8 | :hovered -> 12
<container/>"#;
        let map = harvest_at_directives_from_source(src);
        assert_eq!(map.len(), 2);
        assert!(map.contains_key("brand"));
        assert!(map.contains_key("pad"));
    }

    #[test]
    fn harvest_keeps_first_on_collision() {
        let src = "@color a = #ff0000ff\n@color a = #00ff00ff";
        let map = harvest_at_directives_from_source(src);
        assert_eq!(map["a"].base, "#ff0000ff");
    }

    #[test]
    fn state_responsive_value_builder() {
        let v = StateResponsiveValue::new("accent")
            .with_state("hovered", "lighten(0.1)")
            .with_state("pressed", "darken(0.1)");
        assert_eq!(v.base, "accent");
        assert_eq!(v.states.len(), 2);
    }
}
