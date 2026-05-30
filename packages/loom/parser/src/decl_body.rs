//! Lower a [`Declaration`]'s raw body into structured sub-ASTs for
//! the kinds that have grammar (CHARACTER / TRAIT → [`CharacterBody`],
//! STATS → [`StatsBody`], TREE → [`TreeBody`]). Spec §10 + §11.
//!
//! The raw `Vec<RawLine>` is kept verbatim on [`Declaration::body`];
//! this module never throws it away — the structured fields are
//! additive so downstream consumers that haven't migrated still see
//! what they saw before.

use crate::ast::{
    AttributeDecl, AxisDecl, CharacterBody, CohortBody, ConstructorCall, Declaration,
    DeclarationKind, DispositionAxis, FactionBody, GeneratorBody, GeneratorDecl, GoalDecl,
    HookDecl, InitDecl, InitParam, ItemBody, KnowledgeField, LocationBody, MethodDecl,
    PersonBody, PoolDecl, Property, PropertyValue, RawLine, ReactClause, RosterAssignment,
    RosterBody, RosterCastEntry, RosterCohortEntry, RosterLocationEntry, RosterSwingEntry,
    SceneBody, SceneState, SlotType, StatExprDecl, StatsBody, TreeBody, TreeNodeDecl,
};
use crate::diagnostics::{Code, Diagnostic};
use crate::source::Span;

/// Attach structured `character` / `stats` / `tree` bodies to `decl`
/// based on its [`DeclarationKind`]. Emits diagnostics for shape
/// violations but always returns; consumers can read `decl.body` as a
/// fallback.
pub fn lower(decl: &mut Declaration, diagnostics: &mut Vec<Diagnostic>) {
    match decl.kind {
        DeclarationKind::Character | DeclarationKind::Role | DeclarationKind::Trait => {
            decl.character = Some(lower_character(&decl.body, diagnostics));
        }
        DeclarationKind::Stats => {
            decl.stats = Some(lower_stats(&decl.body, diagnostics));
        }
        DeclarationKind::Tree => {
            decl.tree = Some(lower_tree(&decl.body, diagnostics));
        }
        DeclarationKind::Scene => {
            let (name, params) = split_name_and_params(&decl.name);
            decl.name = name;
            decl.scene = Some(lower_scene(params, &decl.body, diagnostics));
        }
        DeclarationKind::Generator => {
            decl.generator = Some(lower_generator(&decl.body, diagnostics, decl.span));
        }
        DeclarationKind::Cohort => {
            decl.cohort = Some(lower_cohort(&decl.name, &decl.body, decl.span, diagnostics));
        }
        DeclarationKind::Location => {
            decl.location = Some(lower_location(&decl.body));
        }
        DeclarationKind::Item => {
            decl.item = Some(ItemBody {
                inherits: decl.mixin.clone(),
                properties: lower_typed_properties(&decl.body),
            });
        }
        DeclarationKind::Faction => {
            decl.faction = Some(FactionBody {
                inherits: decl.mixin.clone(),
                properties: lower_typed_properties(&decl.body),
            });
        }
        DeclarationKind::Person => {
            decl.person = Some(lower_person(&decl.body));
        }
        DeclarationKind::Roster => {
            decl.roster = Some(lower_roster(&decl.body));
        }
    }
}

/// Lower a flat list of `name: <type> [= default]` raw lines into
/// structured [`Property`] entries (spec §8). Unknown shapes still
/// round-trip via [`Property::raw_type`] + [`Property::default`].
fn lower_typed_properties(body: &[RawLine]) -> Vec<Property> {
    let base_indent = body.first().map(|l| l.indent).unwrap_or(0);
    let mut out = Vec::new();
    for line in body {
        if line.indent != base_indent {
            continue;
        }
        let text = line.text.trim();
        if let Some(prop) = parse_typed_property(text, line.span) {
            out.push(prop);
        }
    }
    out
}

/// Parse one `name: <type-spec> [= default]` line (spec §8). The
/// type spelling is recognised structurally so the runtime can
/// check abstractness without re-lexing; the raw text is kept for
/// shapes the parser doesn't yet model.
pub(crate) fn parse_typed_property(text: &str, span: Span) -> Option<Property> {
    let colon = text.find(':')?;
    let name = text[..colon].trim();
    if name.is_empty()
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return None;
    }
    let rest = text[colon + 1..].trim();
    // `0 to 100 = 50` shape — split type / default on the *first*
    // `=` that isn't part of a `==`. Most slot types don't include
    // an `=` themselves, so a naive split is correct here.
    let (type_part, default_part) = match split_type_and_default(rest) {
        Some((t, d)) => (t.to_string(), Some(d.to_string())),
        None => (rest.to_string(), None),
    };
    let slot_type = parse_slot_type(&type_part, default_part.as_deref());
    Some(Property {
        name: name.to_string(),
        slot_type,
        default: default_part,
        raw_type: if type_part.is_empty() {
            None
        } else {
            Some(type_part)
        },
        span,
    })
}

fn split_type_and_default(rest: &str) -> Option<(&str, &str)> {
    // Honour `range 0 to 100 = 50` — the default is the part after
    // the *last* top-level `=`. Track bracket depth so default
    // expressions containing `[a = b]` survive.
    let bytes = rest.as_bytes();
    let mut depth = 0i32;
    let mut last_eq: Option<usize> = None;
    for (i, &b) in bytes.iter().enumerate() {
        match b as char {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            '=' if depth == 0 => {
                // Skip `==` operator.
                if bytes.get(i + 1) == Some(&b'=') || (i > 0 && bytes[i - 1] == b'=') {
                    continue;
                }
                last_eq = Some(i);
            }
            _ => {}
        }
    }
    let idx = last_eq?;
    Some((rest[..idx].trim(), rest[idx + 1..].trim()))
}

/// Recognise a slot type. Falls back to `Concrete(name)` for any
/// bare identifier, `None` for empty input.
pub(crate) fn parse_slot_type(raw: &str, default_text: Option<&str>) -> Option<SlotType> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    // `text?` — optional wrapper.
    if let Some(inner) = raw.strip_suffix('?') {
        let inner = parse_slot_type(inner.trim(), None)?;
        return Some(SlotType::Optional(Box::new(inner)));
    }
    // `list of X` / `map of K to V`.
    if let Some(rest) = raw.strip_prefix("list of ") {
        let inner = parse_slot_type(rest.trim(), None)
            .unwrap_or_else(|| SlotType::Concrete(rest.trim().to_string()));
        return Some(SlotType::ListOf(Box::new(inner)));
    }
    if let Some(rest) = raw.strip_prefix("map of ") {
        if let Some((k, v)) = rest.split_once(" to ") {
            let key = parse_slot_type(k.trim(), None)
                .unwrap_or_else(|| SlotType::Concrete(k.trim().to_string()));
            let value = parse_slot_type(v.trim(), None)
                .unwrap_or_else(|| SlotType::Concrete(v.trim().to_string()));
            return Some(SlotType::MapOf {
                key: Box::new(key),
                value: Box::new(value),
            });
        }
    }
    // `any` / `any of LOCATION`.
    if raw == "any" {
        return Some(SlotType::Any);
    }
    if let Some(rest) = raw.strip_prefix("any of ") {
        return Some(SlotType::AnyOf(rest.trim().to_string()));
    }
    // `range LO to HI` or bare `LO to HI` (numeric range).
    let range_body = raw.strip_prefix("range ").unwrap_or(raw);
    if let Some((lo_s, hi_s)) = range_body.split_once(" to ") {
        if let (Ok(lo), Ok(hi)) = (lo_s.trim().parse::<f64>(), hi_s.trim().parse::<f64>()) {
            let default = default_text.and_then(|d| d.trim().parse::<f64>().ok());
            return Some(SlotType::Range { lo, hi, default });
        }
    }
    // Sum: `a | b | c` (at least two pipes).
    if raw.contains('|') {
        let parts: Vec<String> = raw
            .split('|')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if parts.len() >= 2 {
            return Some(SlotType::Sum(parts));
        }
    }
    // Bare type name — must look like an identifier.
    if raw
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Some(SlotType::Concrete(raw.to_string()));
    }
    None
}

// ---------------------------------------------------------------------
// COHORT / LOCATION (spec §13.1)
// ---------------------------------------------------------------------

fn lower_cohort(
    name: &str,
    body: &[RawLine],
    decl_span: Span,
    diagnostics: &mut Vec<Diagnostic>,
) -> CohortBody {
    let mut out = CohortBody::default();
    for line in body {
        let text = line.text.trim();
        if let Some((key, value)) = split_property(text) {
            match key {
                "label" => out.label = Some(value.to_string()),
                "capacity" => out.capacity = value.parse::<u32>().ok(),
                _ => {}
            }
            out.properties.insert(
                key.to_string(),
                PropertyValue {
                    value: value.to_string(),
                    span: line.span,
                },
            );
        }
    }
    if out.capacity.is_none() {
        diagnostics.push(Diagnostic::error(
            Code::L1142CohortNoCapacity,
            decl_span,
            format!("`COHORT {name}` is missing a `capacity:` field"),
        ));
    }
    out
}

fn lower_location(body: &[RawLine]) -> LocationBody {
    let mut out = LocationBody::default();
    for line in body {
        let text = line.text.trim();
        if let Some((key, value)) = split_property(text) {
            match key {
                "label" => out.label = Some(value.to_string()),
                "ambient" => out.ambient = Some(value.to_string()),
                "capacity" => out.capacity = value.parse::<u32>().ok(),
                "contains" => {
                    out.contains = value
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                }
                _ => {}
            }
            out.properties.insert(
                key.to_string(),
                PropertyValue {
                    value: value.to_string(),
                    span: line.span,
                },
            );
        }
    }
    out
}

/// Split `patrol(character, route)` into (`patrol`, `["character",
/// "route"]`). Names without parens return an empty parameter list.
fn split_name_and_params(raw: &str) -> (String, Vec<String>) {
    let raw = raw.trim();
    if let Some(open) = raw.find('(') {
        let head = raw[..open].trim().to_string();
        let tail = &raw[open + 1..];
        let close = tail.rfind(')').unwrap_or(tail.len());
        let params: Vec<String> = tail[..close]
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        (head, params)
    } else {
        (raw.to_string(), Vec::new())
    }
}

// ---------------------------------------------------------------------
// SCENE
// ---------------------------------------------------------------------

fn lower_scene(
    params: Vec<String>,
    body: &[RawLine],
    diagnostics: &mut Vec<Diagnostic>,
) -> SceneBody {
    let mut out = SceneBody {
        params,
        ..SceneBody::default()
    };
    let base_indent = body.first().map(|l| l.indent).unwrap_or(0);

    let mut i = 0;
    let mut saw_state = false;
    while i < body.len() {
        let line = &body[i];
        if line.indent > base_indent {
            // Stray over-indent before first state opener — keep in
            // entry body verbatim.
            if !saw_state {
                out.entry.push(line.clone());
            }
            i += 1;
            continue;
        }
        let text = line.text.trim();

        if let Some(t) = text.strip_prefix("tier:") {
            out.tier = Some(t.trim().to_string());
            i += 1;
            continue;
        }
        if let Some(p) = text.strip_prefix("priority:") {
            out.priority = p.trim().parse::<f64>().ok();
            i += 1;
            continue;
        }

        // A bare-name line at the base indent that has at least one
        // indented body line after it is a labelled state opener.
        // Heuristic: identifier-shaped (alpha + _) and the next line
        // is more deeply indented.
        if is_state_label(text)
            && body
                .get(i + 1)
                .map(|n| n.indent > line.indent)
                .unwrap_or(false)
        {
            saw_state = true;
            let name = text.to_string();
            if name.is_empty() {
                diagnostics.push(Diagnostic::error(
                    Code::L1130SceneStateUnnamed,
                    line.span,
                    "scene state opener is missing a name",
                ));
            }
            let mut state = SceneState {
                name,
                body: Vec::new(),
                span: line.span,
            };
            i += 1;
            while i < body.len() && body[i].indent > line.indent {
                state.body.push(body[i].clone());
                state.span = Span::new(state.span.start, body[i].span.end);
                i += 1;
            }
            out.states.push(state);
            continue;
        }

        // Anything else at the base indent belongs to the entry body.
        out.entry.push(line.clone());
        i += 1;
    }
    out
}

fn is_state_label(text: &str) -> bool {
    if text.is_empty() {
        return false;
    }
    // Exclude things that obviously aren't bare-name labels: contain
    // colons, parens, brackets, dots, spaces, arrows, etc.
    if text
        .chars()
        .any(|c| !(c.is_ascii_alphanumeric() || c == '_'))
    {
        return false;
    }
    text.chars()
        .next()
        .map(|c| c.is_ascii_alphabetic() || c == '_')
        .unwrap_or(false)
}

// ---------------------------------------------------------------------
// GENERATOR (top-level)
// ---------------------------------------------------------------------

fn lower_generator(
    body: &[RawLine],
    diagnostics: &mut Vec<Diagnostic>,
    decl_span: Span,
) -> GeneratorBody {
    let mut out = GeneratorBody::default();
    let base_indent = body.first().map(|l| l.indent).unwrap_or(0);
    let mut i = 0;
    while i < body.len() {
        let line = &body[i];
        if line.indent > base_indent {
            out.body.push(line.clone());
            i += 1;
            continue;
        }
        let text = line.text.trim();
        if let Some(t) = text.strip_prefix("tier:") {
            out.tier = Some(t.trim().to_string());
            i += 1;
            continue;
        }
        if let Some(p) = text.strip_prefix("priority:") {
            out.priority = p.trim().parse::<f64>().ok();
            i += 1;
            continue;
        }
        if let Some(rest) = text.strip_prefix("on ") {
            out.start_when = Some(rest.trim().to_string());
            i += 1;
            // Body lines of `on …` follow indented.
            while i < body.len() && body[i].indent > base_indent {
                out.body.push(body[i].clone());
                i += 1;
            }
            continue;
        }
        out.body.push(line.clone());
        i += 1;
    }
    if out.body.is_empty() {
        diagnostics.push(Diagnostic::error(
            Code::L1131GeneratorMissingBody,
            decl_span,
            "generator declaration has no body",
        ));
    }
    out
}

// ---------------------------------------------------------------------
// CHARACTER / TRAIT
// ---------------------------------------------------------------------

fn lower_character(body: &[RawLine], diagnostics: &mut Vec<Diagnostic>) -> CharacterBody {
    let mut out = CharacterBody::default();
    let base_indent = body.first().map(|l| l.indent).unwrap_or(0);

    let mut i = 0;
    while i < body.len() {
        let line = &body[i];
        // Anything more indented than the base is part of a previous
        // block's body (handled by `consume_block`); skip if seen here.
        if line.indent > base_indent {
            i += 1;
            continue;
        }
        let text = line.text.trim();

        // `knows:` opens a typed-slot block.
        if text == "knows:" || text == "knows" {
            i += 1;
            while i < body.len() && body[i].indent > base_indent {
                if let Some(field) = parse_knowledge_field(&body[i], diagnostics) {
                    out.knowledge.push(field);
                }
                i += 1;
            }
            continue;
        }

        // `goal name`.
        if let Some(rest) = text.strip_prefix("goal ") {
            let name = rest.trim();
            if name.is_empty() {
                diagnostics.push(Diagnostic::error(
                    Code::L1104UnnamedGoal,
                    line.span,
                    "`goal` declaration is missing a name",
                ));
                i += 1;
                continue;
            }
            let mut goal = GoalDecl {
                name: name.to_string(),
                span: line.span,
                ..GoalDecl::default()
            };
            i += 1;
            while i < body.len() && body[i].indent > base_indent {
                fill_goal_field(&mut goal, &body[i].text);
                goal.span = Span::new(goal.span.start, body[i].span.end);
                i += 1;
            }
            out.goals.push(goal);
            continue;
        }

        // `on <event>` hook. The `: none` suffix (spec §9.5) marks
        // the hook as a child-side suppressor — at bundle-merge time
        // any inherited hook with the same event clause is dropped.
        if let Some(rest) = text.strip_prefix("on ") {
            let rest = rest.trim();
            let (event_text, suppressed) = match strip_suppression(rest) {
                Some(stripped) => (stripped.to_string(), true),
                None => (rest.to_string(), false),
            };
            let span_start = line.span.start;
            let mut hook = HookDecl {
                event: event_text,
                body: Vec::new(),
                suppressed,
                span: line.span,
            };
            i += 1;
            // A suppressor has no body of its own; still consume any
            // accidentally-indented continuation so we don't trip the
            // base-indent walker below.
            while i < body.len() && body[i].indent > base_indent {
                if !suppressed {
                    hook.body.push(body[i].clone());
                }
                hook.span = Span::new(span_start, body[i].span.end);
                i += 1;
            }
            out.hooks.push(hook);
            continue;
        }

        // `init(args)` constructor (spec v3 §9.6).
        if is_init_opener(text) {
            let (params, _inline_body) = parse_init_signature(text);
            let mut init = InitDecl {
                params,
                body: Vec::new(),
                span: line.span,
            };
            i += 1;
            while i < body.len() && body[i].indent > base_indent {
                init.body.push(body[i].clone());
                init.span = Span::new(init.span.start, body[i].span.end);
                i += 1;
            }
            out.init = Some(init);
            continue;
        }

        // `method name(args)` (spec v3 §9.6). The inline `method foo =
        // expr` form lifts the right-hand side onto `inline_expr` and
        // leaves the body empty.
        if let Some(rest) = text.strip_prefix("method ") {
            if let Some(mut method) = parse_method_opener(rest, line.span) {
                if method.inline_expr.is_none() {
                    i += 1;
                    while i < body.len() && body[i].indent > base_indent {
                        method.body.push(body[i].clone());
                        method.span = Span::new(method.span.start, body[i].span.end);
                        i += 1;
                    }
                } else {
                    i += 1;
                }
                out.methods.push(method);
                continue;
            }
            i += 1;
            continue;
        }

        // `generator name`.
        if let Some(rest) = text.strip_prefix("generator ") {
            let name = rest.trim().to_string();
            let mut gen = GeneratorDecl {
                name,
                span: line.span,
                ..GeneratorDecl::default()
            };
            i += 1;
            while i < body.len() && body[i].indent > base_indent {
                let inner = body[i].text.trim();
                if let Some(t) = inner.strip_prefix("tier:") {
                    gen.tier = Some(t.trim().to_string());
                } else if let Some(p) = inner.strip_prefix("priority:") {
                    gen.priority = p.trim().parse::<f64>().ok();
                } else {
                    gen.body.push(body[i].clone());
                }
                gen.span = Span::new(gen.span.start, body[i].span.end);
                i += 1;
            }
            out.generators.push(gen);
            continue;
        }

        // `reacts <cond> -> <tag>` (the arrow may be `→`).
        if let Some(rest) = text.strip_prefix("reacts ") {
            if let Some(react) = parse_react_clause(rest, line.span) {
                out.reacts.push(react);
            } else {
                diagnostics.push(Diagnostic::error(
                    Code::L1102MalformedReactClause,
                    line.span,
                    "`reacts` clause needs `<condition> → <tag>` (or `-> tag`)",
                ));
            }
            i += 1;
            continue;
        }

        // Disposition: `trusts X: N of M [mirror …]`.
        if let Some((verb, rest)) = strip_disposition_verb(text) {
            match parse_disposition(verb, rest, line.span) {
                Ok(axis) => out.disposition.push(axis),
                Err(code) => {
                    diagnostics.push(Diagnostic::error(code, line.span, code_message(code)))
                }
            }
            i += 1;
            continue;
        }

        // Plain `key: value` property. Special-cases:
        //   * `stats: Combat(strength: 12)` → structured ConstructorCall
        //     captured on `stats_ctor`, with `stats_profile` carrying
        //     the class name (spec v3 §9.6.3 canonical form).
        //   * `stats: Combat` followed by an indented `strength: 12`
        //     block → same lowering as the call form (spec v3 §9.6.3
        //     block sugar). The inner lines are consumed.
        //   * `stats.strength: 12` → dotted overrides folded into the
        //     stats_ctor args (spec v3 §9.6.3 dotted sugar).
        if let Some((key, value)) = split_property(text) {
            if key == "stats" {
                if let Some(call) = parse_constructor_call(value, line.span) {
                    out.stats_profile = Some(call.class.clone());
                    out.stats_ctor = Some(call);
                    i += 1;
                    continue;
                }
                // Bare `stats: Combat` — possibly followed by an
                // indented block sugar.
                out.stats_profile = Some(value.to_string());
                let mut ctor = ConstructorCall {
                    class: value.to_string(),
                    args: indexmap::IndexMap::new(),
                    span: line.span,
                };
                i += 1;
                while i < body.len() && body[i].indent > base_indent {
                    let inner = body[i].text.trim();
                    if let Some((k, v)) = split_property(inner) {
                        ctor.args.insert(k.to_string(), v.to_string());
                        ctor.span = Span::new(ctor.span.start, body[i].span.end);
                    }
                    i += 1;
                }
                if !ctor.args.is_empty() {
                    out.stats_ctor = Some(ctor);
                }
                continue;
            }
            // Dotted override `stats.<field>: <value>`.
            if let Some(field) = key.strip_prefix("stats.") {
                let entry = out.stats_ctor.get_or_insert_with(|| ConstructorCall {
                    class: out.stats_profile.clone().unwrap_or_default(),
                    args: indexmap::IndexMap::new(),
                    span: line.span,
                });
                entry.args.insert(field.to_string(), value.to_string());
                entry.span = Span::new(entry.span.start, line.span.end);
                out.properties.insert(
                    key.to_string(),
                    PropertyValue {
                        value: value.to_string(),
                        span: line.span,
                    },
                );
                i += 1;
                continue;
            }
            out.properties.insert(
                key.to_string(),
                PropertyValue {
                    value: value.to_string(),
                    span: line.span,
                },
            );
            // Mirror into the typed-slot view (spec §8). Spelled
            // out separately so the existing string-keyed
            // `properties` map keeps round-trip parity.
            if let Some(prop) = parse_typed_property(text, line.span) {
                out.typed_properties.push(prop);
            }
            i += 1;
            continue;
        }

        // Unknown line at the base indent: keep walking. The raw
        // body is still on `decl.body` for downstream tooling.
        i += 1;
    }
    out
}

// ---------------------------------------------------------------------
// Class layer — init / method / constructor calls (spec v3 §9.6)
// ---------------------------------------------------------------------

fn is_init_opener(text: &str) -> bool {
    text == "init" || text.starts_with("init(") || text.starts_with("init ")
}

/// Parse `init(name: type? = default, …)` into a parameter list.
/// Returns the params and any inline trailing expression (currently
/// unused — `init = expr` forms are not in the spec but we accept the
/// shape so we can extend later without breaking parses).
fn parse_init_signature(text: &str) -> (Vec<InitParam>, Option<String>) {
    let rest = text.trim_start_matches("init").trim();
    if rest.is_empty() {
        return (Vec::new(), None);
    }
    if let Some(stripped) = rest.strip_prefix('(') {
        if let Some(end) = stripped.rfind(')') {
            let inner = &stripped[..end];
            return (parse_param_list(inner), None);
        }
    }
    (Vec::new(), None)
}

fn parse_param_list(inner: &str) -> Vec<InitParam> {
    let mut params = Vec::new();
    for raw in split_top_level_commas(inner) {
        let raw = raw.trim();
        if raw.is_empty() {
            continue;
        }
        // `name: type? = default`
        let (head, default) = match raw.split_once('=') {
            Some((h, d)) => (h.trim(), Some(d.trim().to_string())),
            None => (raw, None),
        };
        let (name, raw_type) = match head.split_once(':') {
            Some((n, t)) => (n.trim().to_string(), Some(t.trim().to_string())),
            None => (head.to_string(), None),
        };
        if name.is_empty() {
            continue;
        }
        params.push(InitParam {
            name,
            raw_type,
            default,
        });
    }
    params
}

fn parse_method_opener(rest: &str, span: Span) -> Option<MethodDecl> {
    let rest = rest.trim();
    if rest.is_empty() {
        return None;
    }
    // `method name = expr` — inline form.
    if let Some((head, expr)) = rest.split_once('=') {
        let head = head.trim();
        // Watch for `method name() = expr` shape too.
        let (name, params) = parse_method_head(head);
        if name.is_empty() {
            return None;
        }
        return Some(MethodDecl {
            name,
            params,
            inline_expr: Some(expr.trim().to_string()),
            body: Vec::new(),
            span,
        });
    }
    let (name, params) = parse_method_head(rest);
    if name.is_empty() {
        return None;
    }
    Some(MethodDecl {
        name,
        params,
        inline_expr: None,
        body: Vec::new(),
        span,
    })
}

fn parse_method_head(text: &str) -> (String, Vec<InitParam>) {
    if let Some(open) = text.find('(') {
        let head = text[..open].trim().to_string();
        let tail = &text[open + 1..];
        let close = tail.rfind(')').unwrap_or(tail.len());
        let params = parse_param_list(&tail[..close]);
        (head, params)
    } else {
        (text.trim().to_string(), Vec::new())
    }
}

/// Recognise a constructor-call literal — `Combat(strength: 12)`. The
/// class name must look like an identifier and the open-paren must
/// immediately follow it (no space). Args are named (`name: value`).
pub(crate) fn parse_constructor_call(text: &str, span: Span) -> Option<ConstructorCall> {
    let text = text.trim();
    let open = text.find('(')?;
    let class = text[..open].trim();
    if class.is_empty() {
        return None;
    }
    if !class
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return None;
    }
    if !class
        .chars()
        .next()
        .map(|c| c.is_ascii_uppercase() || c == '_')
        .unwrap_or(false)
    {
        return None;
    }
    let tail = &text[open + 1..];
    let close = tail.rfind(')')?;
    let inner = &tail[..close];
    let mut args = indexmap::IndexMap::new();
    for raw in split_top_level_commas(inner) {
        let raw = raw.trim();
        if raw.is_empty() {
            continue;
        }
        if let Some((k, v)) = raw.split_once(':') {
            args.insert(k.trim().to_string(), v.trim().to_string());
        }
    }
    Some(ConstructorCall {
        class: class.to_string(),
        args,
        span,
    })
}

fn split_top_level_commas(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut start = 0usize;
    let bytes = text.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        match b as char {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            ',' if depth == 0 => {
                out.push(text[start..i].to_string());
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(text[start..].to_string());
    out
}

/// Detect the `: none` suppression suffix on a hook event clause
/// (spec §9.5). Returns the event text with the suffix stripped when
/// present, or `None` otherwise.
fn strip_suppression(text: &str) -> Option<&str> {
    let trimmed = text.trim_end();
    let stripped = trimmed.strip_suffix("none")?;
    let head = stripped.trim_end();
    let head = head.strip_suffix(':')?;
    Some(head.trim_end())
}

fn strip_disposition_verb(text: &str) -> Option<(&'static str, &str)> {
    for verb in ["trusts", "respects", "fears"] {
        if let Some(rest) = text.strip_prefix(verb) {
            if rest.starts_with(' ') {
                return Some((verb, rest.trim_start()));
            }
        }
    }
    None
}

fn parse_disposition(verb: &str, rest: &str, span: Span) -> Result<DispositionAxis, Code> {
    let colon = rest.find(':').ok_or(Code::L1100MissingDispositionTarget)?;
    let target = rest[..colon].trim();
    if target.is_empty() {
        return Err(Code::L1100MissingDispositionTarget);
    }
    let value_part = rest[colon + 1..].trim();
    let (amount_part, mirror) = match value_part.find(" mirror ") {
        Some(idx) => (
            value_part[..idx].trim(),
            Some(value_part[idx + 8..].trim().to_string()),
        ),
        None => (value_part, None),
    };
    let (current, max) = match amount_part.split_once(" of ") {
        Some((l, r)) => (
            l.trim()
                .parse::<f64>()
                .map_err(|_| Code::L1101MalformedDispositionAmount)?,
            r.trim()
                .parse::<f64>()
                .map_err(|_| Code::L1101MalformedDispositionAmount)?,
        ),
        None => return Err(Code::L1101MalformedDispositionAmount),
    };
    Ok(DispositionAxis {
        verb: verb.to_string(),
        target: target.to_string(),
        current,
        max,
        mirror,
        span,
    })
}

fn parse_react_clause(text: &str, span: Span) -> Option<ReactClause> {
    // Allow both `→` and `->`.
    let sep_index = text
        .find('→')
        .map(|i| (i, '→'.len_utf8()))
        .or_else(|| text.find("->").map(|i| (i, 2)))?;
    let (cond_part, tag_part) = (&text[..sep_index.0], &text[sep_index.0 + sep_index.1..]);
    let condition = cond_part.trim().to_string();
    let tag = tag_part.trim().to_string();
    if condition.is_empty() || tag.is_empty() {
        return None;
    }
    Some(ReactClause {
        condition,
        tag,
        span,
    })
}

fn parse_knowledge_field(
    line: &RawLine,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<KnowledgeField> {
    let text = line.text.trim();
    let colon = text.find(':');
    let colon = match colon {
        Some(c) => c,
        None => {
            diagnostics.push(Diagnostic::error(
                Code::L1103MalformedKnowledgeField,
                line.span,
                "knowledge field needs `name: type [= default]`",
            ));
            return None;
        }
    };
    let name = text[..colon].trim().to_string();
    let rest = text[colon + 1..].trim();
    let (type_spec, default) = match rest.find('=') {
        Some(idx) => (
            rest[..idx].trim().to_string(),
            Some(rest[idx + 1..].trim().to_string()),
        ),
        None => (rest.to_string(), None),
    };
    if name.is_empty() || type_spec.is_empty() {
        diagnostics.push(Diagnostic::error(
            Code::L1103MalformedKnowledgeField,
            line.span,
            "knowledge field needs `name: type [= default]`",
        ));
        return None;
    }
    Some(KnowledgeField {
        name,
        type_spec,
        default,
        span: line.span,
    })
}

fn fill_goal_field(goal: &mut GoalDecl, text: &str) {
    let text = text.trim();
    if let Some(v) = strip_keyed(text, "priority:") {
        goal.priority = v.parse::<f64>().ok();
    } else if let Some(v) = strip_keyed(text, "active when:") {
        goal.active_when = Some(v.to_string());
    } else if let Some(v) = strip_keyed(text, "completes when:") {
        goal.completes_when = Some(v.to_string());
    } else if let Some(v) = strip_keyed(text, "fails when:") {
        goal.fails_when = Some(v.to_string());
    } else if let Some(v) = strip_keyed(text, "drives:") {
        goal.drives = Some(v.to_string());
    } else if let Some(v) = strip_keyed(text, "on complete:") {
        goal.on_complete = Some(v.to_string());
    } else if let Some(v) = strip_keyed(text, "on fail:") {
        goal.on_fail = Some(v.to_string());
    }
}

fn strip_keyed<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    text.strip_prefix(key).map(str::trim)
}

fn split_property(text: &str) -> Option<(&str, &str)> {
    let colon = text.find(':')?;
    let key = text[..colon].trim();
    if key.is_empty()
        || !key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return None;
    }
    Some((key, text[colon + 1..].trim()))
}

fn code_message(code: Code) -> &'static str {
    match code {
        Code::L1100MissingDispositionTarget => "disposition line is missing a target name",
        Code::L1101MalformedDispositionAmount => "expected `<current> of <max>` numeric pair",
        _ => "malformed declaration",
    }
}

// ---------------------------------------------------------------------
// STATS
// ---------------------------------------------------------------------

fn lower_stats(body: &[RawLine], diagnostics: &mut Vec<Diagnostic>) -> StatsBody {
    let mut out = StatsBody::default();
    let base_indent = body.first().map(|l| l.indent).unwrap_or(0);

    let mut i = 0;
    while i < body.len() {
        let line = &body[i];
        if line.indent > base_indent {
            i += 1;
            continue;
        }
        let text = line.text.trim();

        if let Some(rest) = text.strip_prefix("attribute ") {
            match parse_attribute(rest, line.span) {
                Some(a) => out.attributes.push(a),
                None => diagnostics.push(Diagnostic::error(
                    Code::L1111MalformedAttribute,
                    line.span,
                    "expected `attribute name = N [, range LO to HI]`",
                )),
            }
            i += 1;
            continue;
        }

        if let Some(rest) = text.strip_prefix("axis ") {
            let name = rest.trim().to_string();
            let mut axis = AxisDecl {
                name,
                span: line.span,
                ..AxisDecl::default()
            };
            i += 1;
            while i < body.len() && body[i].indent > base_indent {
                let inner = body[i].text.trim();
                if let Some(v) = strip_keyed(inner, "mode:") {
                    axis.mode = Some(v.to_string());
                } else if let Some(v) = strip_keyed(inner, "curve:") {
                    axis.curve = Some(v.to_string());
                } else if let Some(v) = strip_keyed(inner, "on advance:") {
                    axis.on_advance = Some(v.to_string());
                } else if let Some(v) = strip_keyed(inner, "milestones:") {
                    // `milestones: tutorial, novice, adept, expert, master`
                    // (spec §11) — comma-split, trimmed, blanks dropped.
                    axis.milestones = v
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                }
                axis.span = Span::new(axis.span.start, body[i].span.end);
                i += 1;
            }
            if axis.mode.is_none() {
                diagnostics.push(Diagnostic::error(
                    Code::L1110AxisMissingMode,
                    axis.span,
                    format!("`axis {}` is missing a `mode:` field", axis.name),
                ));
            }
            out.axes.push(axis);
            continue;
        }

        if let Some(rest) = text.strip_prefix("pool ") {
            let name = rest.trim().to_string();
            let mut pool = PoolDecl {
                name,
                span: line.span,
                ..PoolDecl::default()
            };
            i += 1;
            while i < body.len() && body[i].indent > base_indent {
                let inner = body[i].text.trim();
                if let Some(v) = strip_keyed(inner, "max:") {
                    pool.max = Some(v.to_string());
                } else if let Some(v) = strip_keyed(inner, "regen:") {
                    pool.regen = Some(v.to_string());
                } else if let Some(v) = strip_keyed(inner, "cost:") {
                    pool.cost = Some(v.to_string());
                }
                pool.span = Span::new(pool.span.start, body[i].span.end);
                i += 1;
            }
            if pool.max.is_none() {
                diagnostics.push(Diagnostic::error(
                    Code::L1112PoolMissingMax,
                    pool.span,
                    format!("`pool {}` is missing a `max:` field", pool.name),
                ));
            }
            out.pools.push(pool);
            continue;
        }

        if let Some(rest) = text.strip_prefix("stat ") {
            if let Some(eq) = rest.find('=') {
                let name = rest[..eq].trim().to_string();
                let expression = rest[eq + 1..].trim().to_string();
                if !name.is_empty() && !expression.is_empty() {
                    out.stats.push(StatExprDecl {
                        name,
                        expression,
                        span: line.span,
                    });
                }
            }
            i += 1;
            continue;
        }

        // `init(args)` — STATS classes can have constructors too (spec v3 §9.6).
        if is_init_opener(text) {
            let (params, _) = parse_init_signature(text);
            let mut init = InitDecl {
                params,
                body: Vec::new(),
                span: line.span,
            };
            i += 1;
            while i < body.len() && body[i].indent > base_indent {
                init.body.push(body[i].clone());
                init.span = Span::new(init.span.start, body[i].span.end);
                i += 1;
            }
            out.init = Some(init);
            continue;
        }

        // `method name(args)` — STATS class methods (spec v3 §9.6).
        if let Some(rest) = text.strip_prefix("method ") {
            if let Some(mut method) = parse_method_opener(rest, line.span) {
                if method.inline_expr.is_none() {
                    i += 1;
                    while i < body.len() && body[i].indent > base_indent {
                        method.body.push(body[i].clone());
                        method.span = Span::new(method.span.start, body[i].span.end);
                        i += 1;
                    }
                } else {
                    i += 1;
                }
                out.methods.push(method);
                continue;
            }
            i += 1;
            continue;
        }

        i += 1;
    }
    out
}

fn parse_attribute(rest: &str, span: Span) -> Option<AttributeDecl> {
    // `name = N` or `name = N, range LO to HI`.
    let eq = rest.find('=')?;
    let name = rest[..eq].trim().to_string();
    let after = &rest[eq + 1..];
    let (default_part, range_part) = match after.find(',') {
        Some(c) => (after[..c].trim(), Some(after[c + 1..].trim())),
        None => (after.trim(), None),
    };
    let default: f64 = default_part.parse().ok()?;
    let (min, max) = match range_part.and_then(|s| s.strip_prefix("range ")) {
        Some(r) => match r.split_once(" to ") {
            Some((lo, hi)) => (lo.trim().parse().ok()?, hi.trim().parse().ok()?),
            None => (f64::NEG_INFINITY, f64::INFINITY),
        },
        None => (f64::NEG_INFINITY, f64::INFINITY),
    };
    if name.is_empty() {
        return None;
    }
    Some(AttributeDecl {
        name,
        default,
        min,
        max,
        span,
    })
}

// ---------------------------------------------------------------------
// TREE
// ---------------------------------------------------------------------

fn lower_tree(body: &[RawLine], diagnostics: &mut Vec<Diagnostic>) -> TreeBody {
    let mut out = TreeBody::default();
    let base_indent = body.first().map(|l| l.indent).unwrap_or(0);
    let mut i = 0;
    while i < body.len() {
        let line = &body[i];
        if line.indent > base_indent {
            i += 1;
            continue;
        }
        let text = line.text.trim();
        if let Some(rest) = text.strip_prefix("node ") {
            let name = rest.trim().to_string();
            if name.is_empty() {
                diagnostics.push(Diagnostic::error(
                    Code::L1113UnnamedTreeNode,
                    line.span,
                    "`node` declaration is missing a name",
                ));
                i += 1;
                continue;
            }
            let mut node = TreeNodeDecl {
                name,
                span: line.span,
                ..TreeNodeDecl::default()
            };
            i += 1;
            while i < body.len() && body[i].indent > base_indent {
                let inner = body[i].text.trim();
                if let Some(v) = strip_keyed(inner, "cost:") {
                    node.cost = Some(v.to_string());
                } else if let Some(v) = strip_keyed(inner, "requires:") {
                    node.requires = Some(v.to_string());
                } else if let Some(v) = strip_keyed(inner, "effect:") {
                    node.effects.push(v.to_string());
                }
                node.span = Span::new(node.span.start, body[i].span.end);
                i += 1;
            }
            out.nodes.push(node);
            continue;
        }
        i += 1;
    }
    out
}

// ---------------------------------------------------------------------
// PERSON (spec v3 §13.2)
// ---------------------------------------------------------------------

fn lower_person(body: &[RawLine]) -> PersonBody {
    let mut out = PersonBody::default();
    for line in body {
        let text = line.text.trim();
        let Some((key, value)) = split_property(text) else {
            continue;
        };
        match key {
            "display_name" => out.display_name = Some(value.to_string()),
            "pronouns" => out.pronouns = Some(value.to_string()),
            "email" => out.email = Some(value.to_string()),
            "device" => out.device = Some(value.to_string()),
            "notes" => out.notes = Some(value.to_string()),
            "content_tolerance" => out.content_tolerance = parse_bracketed_list(value),
            "accessibility" => out.accessibility = parse_bracketed_list(value),
            _ => {}
        }
        out.properties.insert(
            key.to_string(),
            PropertyValue {
                value: value.to_string(),
                span: line.span,
            },
        );
    }
    out
}

/// Split `[a, b, c]` (or bare `a, b, c`) into a vec of trimmed tokens.
fn parse_bracketed_list(raw: &str) -> Vec<String> {
    let trimmed = raw
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    trimmed
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

// ---------------------------------------------------------------------
// ROSTER (spec v3 §13.3)
// ---------------------------------------------------------------------

fn lower_roster(body: &[RawLine]) -> RosterBody {
    let mut out = RosterBody::default();
    let base_indent = body.first().map(|l| l.indent).unwrap_or(0);

    let mut i = 0;
    while i < body.len() {
        let line = &body[i];
        if line.indent > base_indent {
            i += 1;
            continue;
        }
        let text = line.text.trim();

        // Section openers.
        match text {
            "cast" | "cast:" => {
                i += 1;
                while i < body.len() && body[i].indent > base_indent {
                    let inner = body[i].text.trim();
                    if let Some(entry) = parse_cast_entry(inner, body[i].span) {
                        out.cast.push(entry);
                    }
                    i += 1;
                }
                continue;
            }
            "swings" | "swings:" => {
                i += 1;
                while i < body.len() && body[i].indent > base_indent {
                    let inner = body[i].text.trim();
                    if let Some(entry) = parse_swing_entry(inner, body[i].span) {
                        out.swings.push(entry);
                    }
                    i += 1;
                }
                continue;
            }
            "cohorts" | "cohorts:" => {
                i += 1;
                while i < body.len() && body[i].indent > base_indent {
                    let inner = body[i].text.trim();
                    if let Some(entry) = parse_cohort_entry(inner, body[i].span) {
                        out.cohorts.push(entry);
                    }
                    i += 1;
                }
                continue;
            }
            "locations" | "locations:" => {
                i += 1;
                while i < body.len() && body[i].indent > base_indent {
                    let inner = body[i].text.trim();
                    if let Some(entry) = parse_location_entry(inner, body[i].span) {
                        out.locations.push(entry);
                    }
                    i += 1;
                }
                continue;
            }
            "notes" | "notes:" => {
                i += 1;
                while i < body.len() && body[i].indent > base_indent {
                    let inner = body[i].text.trim();
                    if let Some((k, v)) = split_property(inner) {
                        out.notes.insert(
                            k.to_string(),
                            PropertyValue {
                                value: v.to_string(),
                                span: body[i].span,
                            },
                        );
                    }
                    i += 1;
                }
                continue;
            }
            _ => {}
        }

        if let Some((key, value)) = split_property(text) {
            match key {
                "date" => out.date = Some(value.to_string()),
                "capacity" => out.capacity = value.parse::<u32>().ok(),
                _ => {}
            }
            out.properties.insert(
                key.to_string(),
                PropertyValue {
                    value: value.to_string(),
                    span: line.span,
                },
            );
        }
        i += 1;
    }
    out
}

fn parse_cast_entry(text: &str, span: Span) -> Option<RosterCastEntry> {
    // `Wren        := jamie_lee`  or  `Initiate    := any of [audience]`
    let (role_part, rhs) = text.split_once(":=")?;
    let role = role_part.trim().to_string();
    let rhs = rhs.trim();
    if role.is_empty() {
        return None;
    }
    let assignment = if rhs.is_empty() || rhs == "none" {
        RosterAssignment::Ghost
    } else if let Some(rest) = rhs.strip_prefix("any of ") {
        RosterAssignment::AnyOf(parse_bracketed_list(rest))
    } else {
        RosterAssignment::Person(rhs.to_string())
    };
    Some(RosterCastEntry {
        role,
        assignment,
        span,
    })
}

fn parse_swing_entry(text: &str, span: Span) -> Option<RosterSwingEntry> {
    let (role_part, rhs) = text.split_once(":=")?;
    let role = role_part.trim().to_string();
    let fallbacks = parse_bracketed_list(rhs.trim());
    if role.is_empty() {
        return None;
    }
    Some(RosterSwingEntry {
        role,
        fallbacks,
        span,
    })
}

fn parse_cohort_entry(text: &str, span: Span) -> Option<RosterCohortEntry> {
    // `Initiates   start with: [audience]`
    let (name_part, rest) = text.split_once("start with:")?;
    let cohort = name_part.trim().to_string();
    if cohort.is_empty() {
        return None;
    }
    Some(RosterCohortEntry {
        cohort,
        start_with: parse_bracketed_list(rest.trim()),
        span,
    })
}

fn parse_location_entry(text: &str, span: Span) -> Option<RosterLocationEntry> {
    let (name_part, rest) = text.split_once("start with:")?;
    let location = name_part.trim().to_string();
    if location.is_empty() {
        return None;
    }
    Some(RosterLocationEntry {
        location,
        start_with: parse_bracketed_list(rest.trim()),
        span,
    })
}
