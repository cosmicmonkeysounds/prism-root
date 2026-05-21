//! Style resolution helpers lowered out of `interpret/mod.rs` during
//! Phase 0 (see `docs/dev/prui-expressiveness-roadmap.md` §7.0). Pure
//! code move, no behaviour change. Functions called from the parent
//! lowering pass are `pub(super)`; everything else stays private to
//! this module.
//!
//! Owns: `REM_PX` base, `expand_length_units` / `parse_f32` (length
//! parsing), `apply_style_override` (the canonical `style:<key>` sink,
//! exported through `interpret/mod.rs`), `active_class_names` +
//! `apply_prss_class` + `apply_descendant_selectors` (PRSS class
//! application), short-token + descendant-selector helpers, and the
//! per-value `parse_direction` / `parse_sizing` / `parse_color` /
//! `parse_padding_shorthand` decoders.
//!
//! `STATE_SUFFIXES` itself lives in `prism_core::language::prism_ui`
//! (the parser-side suffix list); this module consumes it via
//! `split_state_suffix`.

use prism_core::language::prism_ui::{
    split_state_suffix, AttributeNamespace, AttributeValue, Element,
};

use crate::command::{Color, CornerRadius};
use crate::layout::{ContainerProps, Direction, Padding, Sizing, StateOverrides};

use super::elements::resolved_attribute_string;
use super::expression::{
    eval_truthy, evaluate_expression, interpolate, lookup_path_owned, stringify_value,
};
use super::LowerScope;

const REM_PX: f32 = 16.0;

/// Pre-expand any `Nrem` / `Nem` segments in `value` to their
/// pixel-resolved numeric form, using the scope's
/// [`LowerScope::rem_px`] base. Multi-segment strings (`padding="1rem 2rem"`)
/// expand each whitespace-separated token independently so the
/// downstream `parse_padding_shorthand` consumer reads pixel numbers
/// uniformly.
///
/// When the scope's rem base equals the canonical 16px default, the
/// helper short-circuits and returns the value unchanged so the
/// hot path (no token override) avoids the alloc.
pub(super) fn expand_length_units(value: &str, scope: &LowerScope) -> String {
    let rem_px = scope.rem_px();
    if (rem_px - REM_PX).abs() < f32::EPSILON {
        return value.to_string();
    }
    let mut out = String::with_capacity(value.len());
    let mut leading_ws_done = false;
    for token in value.split_whitespace() {
        if leading_ws_done {
            out.push(' ');
        } else {
            leading_ws_done = true;
        }
        // Order matters — `rem` ends in `em`. Try `rem` first.
        let expanded = if let Some(num) = token.strip_suffix("rem") {
            num.trim_end()
                .parse::<f32>()
                .ok()
                .map(|n| (n * rem_px).to_string())
        } else if let Some(num) = token.strip_suffix("em") {
            num.trim_end()
                .parse::<f32>()
                .ok()
                .map(|n| (n * rem_px).to_string())
        } else {
            None
        };
        match expanded {
            Some(s) => out.push_str(&s),
            None => out.push_str(token),
        }
    }
    out
}

/// Parse a length-valued string. Accepts:
///
/// | Form | Resolves to | Notes |
/// |---|---|---|
/// | `"14"` | `14.0` | Bare number = px (matches CSS) |
/// | `"14px"` | `14.0` | Explicit px |
/// | `"1rem"` | `16.0` | `n * REM_PX` |
/// | `"0.875rem"` | `14.0` | Decimal allowed |
/// | `"1em"` | `16.0` | Same as `rem` today — no parent-font-size scope threading yet |
///
/// Trailing whitespace tolerated. Anything else returns `None` —
/// caller drops the property silently (matching the existing
/// "unknown style value drops cleanly" discipline).
pub(super) fn parse_f32(s: &str) -> Option<f32> {
    let s = s.trim();
    // Order matters: `rem` ends in `em`, so check `rem` first.
    if let Some(num) = s.strip_suffix("rem") {
        return num.trim_end().parse::<f32>().ok().map(|n| n * REM_PX);
    }
    if let Some(num) = s.strip_suffix("em") {
        return num.trim_end().parse::<f32>().ok().map(|n| n * REM_PX);
    }
    if let Some(num) = s.strip_suffix("px") {
        return num.trim_end().parse::<f32>().ok();
    }
    s.parse::<f32>().ok()
}

/// Apply one `style:<key>[:<state>]="<value>"` override onto a
/// [`ContainerProps`]. Single source of truth for the style-attribute
/// vocabulary: every consumer (`apply_container_attributes`'s
/// `AttributeNamespace::Style` branch, the resolver-side
/// parent-passes-style-to-child seam in `ui_resolver.rs`, future Luau
/// style writers) calls this so the keys stay in sync.
///
/// The `local` argument is the attribute's local part (`background`,
/// `radius:hovered`, etc.). [`split_state_suffix`] is consulted
/// internally to peel any trailing `:state` so callers don't need
/// to.
///
/// Unknown keys land as `data-style-<key>` / `data-style-<key>-<state>`
/// semantic attrs so author intent survives even when the runtime
/// doesn't have first-class support for the override yet — same
/// "data round-trips, behaviour follows" pattern Waves 9.2/9.4 use.
/// **PRSS / class toggles** — collect the active class-name list for
/// `el` against `scope`, in document order:
///
/// 1. Every name from a static `class="..."` attribute (multiple
///    classes whitespace-separated).
/// 2. Every `class:<name>="{cond}"` toggle whose value is truthy.
///
/// Toggles layer onto the static list so a later
/// `class:foo="{true}"` wins on conflicts the same way a literal
/// `class="… foo"` would. Boolean `class:foo` (no `=value`) reads as
/// `true` so authors can write the bare attribute as a synonym for
/// `class:foo="true"`. Result preserves source-order duplicates so
/// `apply_prss_class`'s left-to-right specificity rule observes
/// the same shape it would have without toggles.
pub(super) fn active_class_names(el: &Element, scope: &LowerScope) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for attr in &el.attributes {
        match attr.name.namespace {
            AttributeNamespace::Identifier if attr.name.local == "class" => {
                if let Some(value) = resolved_attribute_string(&attr.value, scope) {
                    for name in value.split_whitespace() {
                        if !name.is_empty() {
                            out.push(name.to_string());
                        }
                    }
                }
            }
            AttributeNamespace::Class => {
                if attr.name.local.is_empty() {
                    continue;
                }
                if attr_value_is_truthy(&attr.value, scope) {
                    out.push(attr.name.local.clone());
                }
            }
            _ => {}
        }
    }
    out
}

/// Truthy evaluator for an `AttributeValue`. Mirrors the JS rule the
/// `if=` namespace already uses — bare `class:foo` (Empty) reads as
/// true, an Expression body flows through [`eval_truthy`], a
/// String/Template resolves and is parsed against the canonical
/// boolean spellings (`true` / `1` / non-empty arbitrary text → true;
/// `false` / `0` / empty → false). Lets authors write either
/// `class:active="{state.active}"` or `class:active` without
/// special-casing Empty downstream.
fn attr_value_is_truthy(value: &AttributeValue, scope: &LowerScope) -> bool {
    match value {
        // Boolean attribute (no `=` after the name) — the author's
        // intent is "always on", same shape as HTML's `disabled`.
        AttributeValue::Empty => true,
        // Expression bodies route through the full Prism truthy rule
        // so ternary / `&&` / dotted-path lookups read uniformly.
        AttributeValue::Expression(expr) => eval_truthy(&expr.body, scope),
        // String + Template values resolve to a string and then map
        // through the canonical boolean spellings. Authors writing
        // `class:foo="false"` get false; `class:foo="true"` true;
        // anything else is truthy if non-empty.
        AttributeValue::String { value, .. } => string_value_is_truthy(value),
        AttributeValue::Template { .. } => resolved_attribute_string(value, scope)
            .as_deref()
            .map(string_value_is_truthy)
            .unwrap_or(false),
    }
}

fn string_value_is_truthy(s: &str) -> bool {
    let trimmed = s.trim();
    !matches!(trimmed, "" | "false" | "0")
}

/// **PRSS** — resolve and apply one class name (with its `extends`
/// chain flattened parent-first) onto a [`ContainerProps`]. Property
/// values are interpolated against the active scope so a class can
/// reference `{tokens.colors.<name>}` and read through the same
/// expression evaluator inline `style:` uses.
///
/// Application order inside this function:
/// 1. Base properties from the flattened `extends` chain.
/// 2. State overrides — each `(state, key, value)` applied through
///    the existing `apply_style_override` state-suffix branch as
///    `<key>:<state>`.
///
/// Unknown class names are no-ops (a parent `extends` chain that
/// already surfaced a `missing-parent` diagnostic at parse time
/// drops cleanly at apply time too).
/// **Wave F (`prui-luau-fusion.md` §7.9)** — resolve a PRSS value
/// that may be a `{ lua = "…" }` computed expression. A
/// sentinel-prefixed value (encoded by the prism-core PRSS parser)
/// has its trailing expression evaluated through the same
/// owned-value pipeline class bindings use — so `tokens.*` lookups
/// and Luau-frame helpers (`darken(…)`) both resolve. A plain value
/// passes through untouched. Borrowed `Cow` on the common
/// (non-computed) path keeps the hot loop allocation-free.
fn prss_value_resolved<'a>(value: &'a str, scope: &LowerScope) -> std::borrow::Cow<'a, str> {
    match value.strip_prefix(prism_core::language::prss::LUA_VALUE_SENTINEL) {
        Some(expr) => {
            let resolved = lookup_path_owned(expr, scope)
                .or_else(|| evaluate_expression(expr, scope))
                .map(|v| stringify_value(&v))
                .unwrap_or_default();
            std::borrow::Cow::Owned(resolved)
        }
        None => std::borrow::Cow::Borrowed(value),
    }
}

pub(super) fn apply_prss_class(
    sheet: &prism_core::language::prss::StyleSheet,
    name: &str,
    props: &mut ContainerProps,
    scope: &LowerScope,
) {
    let Some(resolved) = sheet.resolve(name) else {
        return;
    };
    for (key, value) in &resolved.properties {
        let computed = prss_value_resolved(value, scope);
        let interpolated = interpolate(&computed, scope);
        let resolved_short = resolve_short_token(key, &interpolated, scope).unwrap_or(interpolated);
        let final_value = expand_length_units(&resolved_short, scope);
        apply_style_override(props, key, &final_value);
    }
    for (state, key, value) in &resolved.states {
        let computed = prss_value_resolved(value, scope);
        let interpolated = interpolate(&computed, scope);
        let resolved_short = resolve_short_token(key, &interpolated, scope).unwrap_or(interpolated);
        let final_value = expand_length_units(&resolved_short, scope);
        let suffixed = format!("{}:{}", key, state);
        apply_style_override(props, &suffixed, &final_value);
    }
}

/// **PRSS descendant selectors** — walk every multi-segment class
/// in the sheet and apply the ones whose segment chain matches the
/// element's active class set + the ancestor class chain on
/// `scope`. Ordering: each matching selector's properties layer on
/// top of the flat-class application, so a more specific selector
/// (`.btn .icon`) overrides the flat (`.icon`) for keys it sets.
/// Selectors are visited in declaration order; later wins on key
/// conflicts (matching the §4.6 application-order rule).
pub(super) fn apply_descendant_selectors(
    sheet: &prism_core::language::prss::StyleSheet,
    active: &[String],
    scope: &LowerScope,
    props: &mut ContainerProps,
) {
    let chain = scope.class_chain();
    for (_, segments, resolved) in sheet.descendant_selectors() {
        if !descendant_selector_matches(&segments, active, chain) {
            continue;
        }
        for (key, value) in &resolved.properties {
            let computed = prss_value_resolved(value, scope);
            let interpolated = interpolate(&computed, scope);
            let resolved_short =
                resolve_short_token(key, &interpolated, scope).unwrap_or(interpolated);
            let final_value = expand_length_units(&resolved_short, scope);
            apply_style_override(props, key, &final_value);
        }
        for (state, key, value) in &resolved.states {
            let computed = prss_value_resolved(value, scope);
            let interpolated = interpolate(&computed, scope);
            let resolved_short =
                resolve_short_token(key, &interpolated, scope).unwrap_or(interpolated);
            let final_value = expand_length_units(&resolved_short, scope);
            let suffixed = format!("{}:{}", key, state);
            apply_style_override(props, &suffixed, &final_value);
        }
    }
}

/// **Short-name token references** (`prss-reference.md` §6) — when a
/// PRSS class property or PRUI inline style value is a bare token
/// name (e.g. `radius = "md"` or `style:background="accent"`),
/// resolve it through the active `tokens.<bucket>.<name>` table on
/// `scope` to its underlying value (`8`, `"#7c3aed"`). Returns
/// `None` when the value isn't a bare token name (literal hex,
/// numeric with units, expression result, …) or when the looked-up
/// token isn't present — caller falls back to the raw value.
///
/// The bucket is derived from `key` (after a state-suffix split):
/// colors-typed keys (`background`, `color`, `border`) read from
/// `tokens.colors`; spacing-typed keys (`gap`, `padding`,
/// `padding-*`, `margin*`) read from `tokens.spacing`; `radius`
/// reads from `tokens.radius`; `font-size` / `line-height` read
/// from `tokens.typography` with the canonical `font-size-<short>`
/// / `line-height-<short>` key shape mirrored from
/// [`design_tokens_to_json`].
pub(super) fn resolve_short_token(key: &str, value: &str, scope: &LowerScope) -> Option<String> {
    let trimmed = value.trim();
    if !is_bare_token_name(trimmed) {
        return None;
    }
    let (bucket, token_key) = short_token_bucket_for_key(key, trimmed)?;
    let tokens = scope.binding("tokens")?.as_object()?;
    let bucket_obj = tokens.get(bucket)?.as_object()?;
    let val = bucket_obj.get(&token_key)?;
    Some(stringify_value(val))
}

/// True when `s` matches the shape PRSS short-name token references
/// recognise: lowercase ASCII identifier characters (a-z), digits,
/// `-`, or `_`, with a non-digit first character. Filters out hex
/// colors (`#…`), numeric values (`8`, `1.5`, `1rem`), expression
/// remnants (`{…}`), and capitalised words. The shape mirrors the
/// design-token key spelling — `accent`, `text-primary`, `surface-elevated`,
/// `font-size-md`, `md`.
fn is_bare_token_name(s: &str) -> bool {
    let mut chars = s.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_ascii_lowercase() || first == '_') {
        return false;
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
}

/// Map a PRSS / PRUI style key (after a state-suffix split) onto its
/// `(bucket, token_key)` lookup pair. `None` for keys that don't
/// participate in short-name resolution (`width`, `height`,
/// `direction`, `tag`, …).
fn short_token_bucket_for_key(key: &str, short: &str) -> Option<(&'static str, String)> {
    use prism_core::language::prism_ui::ast::split_state_suffix;
    let (bare_key, _) = split_state_suffix(key);
    match bare_key {
        "background" | "color" | "border" => Some(("colors", short.to_string())),
        "radius" => Some(("radius", short.to_string())),
        "gap" | "padding" | "padding-left" | "padding-right" | "padding-top" | "padding-bottom"
        | "margin" | "margin-left" | "margin-right" | "margin-top" | "margin-bottom" => {
            Some(("spacing", short.to_string()))
        }
        "font-size" => Some(("typography", format!("font-size-{}", short))),
        "line-height" => Some(("typography", format!("line-height-{}", short))),
        _ => None,
    }
}

/// CSS-style descendant matcher: the rightmost segment must match
/// a class in `current` (the element's active class set); each
/// preceding segment must match an ancestor's class set, in order
/// from innermost outward, with intermediate ancestors skipped if
/// they don't match.
///
/// `chain` is outermost-first as stored in [`LowerScope::class_chain`];
/// the match walks it from innermost (`chain.len()-1`) outward so the
/// nearest ancestor with the needed class is consumed first.
fn descendant_selector_matches(
    segments: &[&str],
    current: &[String],
    chain: &[Vec<String>],
) -> bool {
    if segments.is_empty() {
        return false;
    }
    let last = segments[segments.len() - 1];
    if !current.iter().any(|c| c == last) {
        return false;
    }
    let prefix = &segments[..segments.len() - 1];
    if prefix.is_empty() {
        // Single segment — caller handles flat application; we do
        // not apply here to avoid double-counting. Returning false
        // matches `descendant_selectors`'s `len() <= 1` filter; this
        // arm is defensive.
        return false;
    }
    let mut chain_idx = chain.len();
    // Walk prefix segments innermost-first. For each needle, scan
    // ancestors (innermost→outermost), consuming whichever one
    // contains it. Anything before that ancestor is still available
    // for outer prefix segments.
    for needle in prefix.iter().rev() {
        let mut found = false;
        while chain_idx > 0 {
            chain_idx -= 1;
            if chain[chain_idx].iter().any(|c| c == needle) {
                found = true;
                break;
            }
        }
        if !found {
            return false;
        }
    }
    true
}

pub fn apply_style_override(props: &mut ContainerProps, local: &str, value: &str) {
    let (key, state) = split_state_suffix(local);
    match (key, state) {
        ("background", None) => {
            if let Some(c) = parse_color(value) {
                props.background = Some(c);
            }
        }
        ("radius", None) => {
            if let Some(v) = parse_f32(value) {
                props.radius = CornerRadius {
                    tl: v,
                    tr: v,
                    br: v,
                    bl: v,
                };
            }
        }
        ("padding", None) => {
            // CSS-shorthand: `padding="8"` (uniform), `padding="8 16"`
            // (vertical, horizontal), `padding="8 16 24"` (top, H,
            // bottom), `padding="8 16 24 32"` (TRBL — CSS top/right/
            // bottom/left order). Single-value path stays through
            // `Padding::all` for back-compat.
            if let Some(p) = parse_padding_shorthand(value) {
                props.padding = p;
            }
        }
        ("padding-left", None) => set_padding_side(&mut props.padding, Some(value), Side::Left),
        ("padding-right", None) => set_padding_side(&mut props.padding, Some(value), Side::Right),
        ("padding-top", None) => set_padding_side(&mut props.padding, Some(value), Side::Top),
        ("padding-bottom", None) => set_padding_side(&mut props.padding, Some(value), Side::Bottom),
        ("gap", None) => {
            if let Some(v) = parse_f32(value) {
                props.gap = v;
            }
        }
        ("width", None) => {
            if let Some(s) = parse_sizing(value) {
                props.width = s;
            }
        }
        ("height", None) => {
            if let Some(s) = parse_sizing(value) {
                props.height = s;
            }
        }
        // Wave 14.6 — `style:opacity="0.5"` lowers to the
        // `ContainerProps::opacity` field. Clamped to `[0.0, 1.0]`;
        // malformed values silently drop (same pattern as the rest
        // of this vocabulary). `None` (the default) means "fully
        // opaque" — set to a float to fade the container's own
        // paint and cascade into children at command-emit time.
        ("opacity", None) => {
            if let Some(v) = parse_f32(value) {
                props.opacity = Some(v.clamp(0.0, 1.0));
            }
        }
        (key, Some(state)) => {
            // §7.7 Phase 1 — dispatch any key on a first-class state
            // (`hovered` / `pressed` / `focused` / `disabled`) into the
            // matching `StateOverrides` bucket.
            if let Some(slot) = state_override_slot(props, state) {
                if apply_to_state_overrides(slot, key, value) {
                    return;
                }
            }
            // **§7.15** — `:entry` / `:exit` are lifecycle markers
            // owned by the unified Animator trait, not state
            // overrides. Redirect them into the existing
            // `data-animate-in-<prop>` / `data-animate-out-<prop>`
            // semantic attrs the runtime animator reads at observe
            // time. The value-side spelling matches the Phase-15
            // canonical pipeline (`{ rest | :entry → from N over
            // MS }`) reduced to the attribute shorthand
            // `"<from> <duration>"` Wave 14.6 already shipped.
            match state {
                "entry" => {
                    props
                        .semantic
                        .attrs
                        .push((format!("data-animate-in-{}", key), value.to_string()));
                    return;
                }
                "exit" => {
                    props
                        .semantic
                        .attrs
                        .push((format!("data-animate-out-{}", key), value.to_string()));
                    return;
                }
                _ => {}
            }
            // Unrecognised key on a first-class state OR a state
            // without a first-class slot — round-trip as a semantic
            // attr (Wave 9.2 pattern). Covers `focus-within` /
            // `selected` / `empty` / `checked`.
            props
                .semantic
                .attrs
                .push((format!("data-style-{}-{}", key, state), value.to_string()));
        }
        (_, None) => {
            // Unknown bare key (no recognized state suffix). Drop
            // silently — same shape as the pre-extraction
            // `apply_container_attributes` branch. Authors who want
            // arbitrary `data-*` payloads have the `data:` namespace.
        }
    }
}

/// §7.7 Phase 1 — return the `StateOverrides` slot for the given
/// state, lazily allocating one on `props` if needed. Returns `None`
/// for states that don't have a first-class runtime bucket yet
/// (`focus-within` / `selected` / `empty` / `checked` / `entry` /
/// `exit`); callers fall back to the data-attr round-trip in that
/// case. The four first-class states are wired this way so adding a
/// fifth — when its runtime substrate lands — is one match arm here.
fn state_override_slot<'a>(
    props: &'a mut ContainerProps,
    state: &str,
) -> Option<&'a mut StateOverrides> {
    match state {
        "hovered" => Some(props.hover.get_or_insert_with(StateOverrides::default)),
        "pressed" => Some(props.pressed.get_or_insert_with(StateOverrides::default)),
        "focused" => Some(props.focused.get_or_insert_with(StateOverrides::default)),
        "disabled" => Some(props.disabled.get_or_insert_with(StateOverrides::default)),
        _ => None,
    }
}

/// §7.7 Phase 1 — apply a single `key=value` pair onto a state's
/// `StateOverrides` slot. Returns `true` when the key landed, `false`
/// when the key isn't understood at the override layer (the caller
/// falls back to the data-attr round-trip in that case so unknown
/// keys still survive through SSR / inspector / hot-reload).
///
/// The property whitelist matches the new `StateOverrides` fields —
/// `background`, `radius`, `color`, `padding`, `opacity`, `tint`. The
/// design table also lists `gap` / `width` / `height` / `transform`
/// as state-swappable; those force layout-topology rework and stay
/// data-attr round-trips for now (each will land as it grows its own
/// override field).
fn apply_to_state_overrides(slot: &mut StateOverrides, key: &str, value: &str) -> bool {
    match key {
        "background" => {
            if let Some(c) = parse_color(value) {
                slot.background = Some(c);
                return true;
            }
        }
        "radius" => {
            if let Some(v) = parse_f32(value) {
                slot.radius = Some(CornerRadius {
                    tl: v,
                    tr: v,
                    br: v,
                    bl: v,
                });
                return true;
            }
        }
        "color" => {
            if let Some(c) = parse_color(value) {
                slot.color = Some(c);
                return true;
            }
        }
        "padding" => {
            if let Some(p) = parse_padding_shorthand(value) {
                slot.padding = Some(p);
                return true;
            }
        }
        "opacity" => {
            if let Some(v) = parse_f32(value) {
                slot.opacity = Some(v.clamp(0.0, 1.0));
                return true;
            }
        }
        "tint" => {
            if let Some(c) = parse_color(value) {
                slot.tint = Some(c);
                return true;
            }
        }
        _ => {}
    }
    false
}

pub(super) fn parse_direction(s: &str) -> Direction {
    match s.trim() {
        "row" => Direction::Row,
        _ => Direction::Column,
    }
}

/// Parse a sizing value for `width` / `height`. In addition to
/// the length forms `parse_f32` accepts, this layer recognises:
///
/// | Form | Resolves to |
/// |---|---|
/// | `"grow"` | `Sizing::Grow` (`taffy::Dimension::Percent(1.0)`) |
/// | `"fit"` / `"auto"` | `Sizing::Fit` (`taffy::Dimension::Auto`) |
/// | `"50%"` | `Sizing::Percent(0.5)` (CSS-style; 0..1 clamped) |
/// | `"14"` / `"14px"` / `"1rem"` | `Sizing::Fixed(<px>)` via `parse_f32` |
pub(super) fn parse_sizing(s: &str) -> Option<Sizing> {
    let s = s.trim();
    match s {
        "grow" => Some(Sizing::Grow),
        "fit" | "auto" => Some(Sizing::Fit),
        _ => {
            if let Some(num) = s.strip_suffix('%') {
                return num
                    .trim_end()
                    .parse::<f32>()
                    .ok()
                    .map(|n| Sizing::Percent(n / 100.0));
            }
            parse_f32(s).map(Sizing::Fixed)
        }
    }
}

pub(super) fn parse_color(raw: &str) -> Option<Color> {
    let s = raw.trim();
    let hex = s.strip_prefix('#')?;
    let (r, g, b, a) = match hex.len() {
        6 => (
            u8::from_str_radix(&hex[0..2], 16).ok()?,
            u8::from_str_radix(&hex[2..4], 16).ok()?,
            u8::from_str_radix(&hex[4..6], 16).ok()?,
            255,
        ),
        8 => (
            u8::from_str_radix(&hex[0..2], 16).ok()?,
            u8::from_str_radix(&hex[2..4], 16).ok()?,
            u8::from_str_radix(&hex[4..6], 16).ok()?,
            u8::from_str_radix(&hex[6..8], 16).ok()?,
        ),
        3 => {
            let r = u8::from_str_radix(&hex[0..1], 16).ok()?;
            let g = u8::from_str_radix(&hex[1..2], 16).ok()?;
            let b = u8::from_str_radix(&hex[2..3], 16).ok()?;
            (r * 17, g * 17, b * 17, 255)
        }
        _ => return None,
    };
    Some(Color { r, g, b, a })
}

/// Parse a CSS-shorthand padding value into a [`Padding`]. Accepts
/// 1, 2, 3, or 4 whitespace-separated lengths matching the standard
/// CSS shorthand order. Returns `None` if any token fails to parse;
/// caller drops the property cleanly.
///
/// | Tokens | Shape |
/// |---|---|
/// | 1 | All sides uniform |
/// | 2 | Vertical, horizontal |
/// | 3 | Top, horizontal, bottom |
/// | 4 | Top, right, bottom, left (CSS TRBL) |
pub(super) fn parse_padding_shorthand(value: &str) -> Option<Padding> {
    let tokens: Vec<f32> = value
        .split_whitespace()
        .map(parse_f32)
        .collect::<Option<Vec<f32>>>()?;
    match tokens.len() {
        1 => Some(Padding::all(tokens[0])),
        2 => Some(Padding {
            top: tokens[0],
            right: tokens[1],
            bottom: tokens[0],
            left: tokens[1],
        }),
        3 => Some(Padding {
            top: tokens[0],
            right: tokens[1],
            bottom: tokens[2],
            left: tokens[1],
        }),
        4 => Some(Padding {
            top: tokens[0],
            right: tokens[1],
            bottom: tokens[2],
            left: tokens[3],
        }),
        _ => None,
    }
}

pub(super) enum Side {
    Left,
    Right,
    Top,
    Bottom,
}

pub(super) fn set_padding_side(padding: &mut Padding, raw: Option<&str>, side: Side) {
    let Some(v) = raw.and_then(parse_f32) else {
        return;
    };
    match side {
        Side::Left => padding.left = v,
        Side::Right => padding.right = v,
        Side::Top => padding.top = v,
        Side::Bottom => padding.bottom = v,
    }
}
