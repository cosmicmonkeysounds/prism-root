//! Lower a [`Declaration`]'s raw body into structured sub-ASTs for
//! the kinds that have grammar (CHARACTER / TRAIT → [`CharacterBody`],
//! STATS → [`StatsBody`], TREE → [`TreeBody`]). Spec §10 + §11.
//!
//! The raw `Vec<RawLine>` is kept verbatim on [`Declaration::body`];
//! this module never throws it away — the structured fields are
//! additive so downstream consumers that haven't migrated still see
//! what they saw before.

use crate::ast::{
    AttributeDecl, AxisDecl, CharacterBody, CohortBody, Declaration, DeclarationKind,
    DispositionAxis, GeneratorBody, GeneratorDecl, GoalDecl, HookDecl, KnowledgeField,
    LocationBody, PoolDecl, PropertyValue, RawLine, ReactClause, SceneBody, SceneState,
    StatExprDecl, StatsBody, TreeBody, TreeNodeDecl,
};
use crate::diagnostics::{Code, Diagnostic};
use crate::source::Span;
use indexmap::IndexMap;

/// Attach structured `character` / `stats` / `tree` bodies to `decl`
/// based on its [`DeclarationKind`]. Emits diagnostics for shape
/// violations but always returns; consumers can read `decl.body` as a
/// fallback.
pub fn lower(decl: &mut Declaration, diagnostics: &mut Vec<Diagnostic>) {
    match decl.kind {
        DeclarationKind::Character | DeclarationKind::Trait => {
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
        _ => {}
    }
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
    text.chars().next().map(|c| c.is_ascii_alphabetic() || c == '_').unwrap_or(false)
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

        // `on <event>` hook.
        if let Some(rest) = text.strip_prefix("on ") {
            let event = rest.trim().to_string();
            let span_start = line.span.start;
            let mut hook = HookDecl {
                event,
                body: Vec::new(),
                span: line.span,
            };
            i += 1;
            while i < body.len() && body[i].indent > base_indent {
                hook.body.push(body[i].clone());
                hook.span = Span::new(span_start, body[i].span.end);
                i += 1;
            }
            out.hooks.push(hook);
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
                Err(code) => diagnostics.push(Diagnostic::error(code, line.span, code_message(code))),
            }
            i += 1;
            continue;
        }

        // Plain `key: value` property.
        if let Some((key, value)) = split_property(text) {
            if key == "stats" {
                out.stats_profile = Some(value.to_string());
            }
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

        // Unknown line at the base indent: keep walking. The raw
        // body is still on `decl.body` for downstream tooling.
        i += 1;
    }
    out
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
            l.trim().parse::<f64>().map_err(|_| Code::L1101MalformedDispositionAmount)?,
            r.trim().parse::<f64>().map_err(|_| Code::L1101MalformedDispositionAmount)?,
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
    let sep_index = text.find('→').map(|i| (i, '→'.len_utf8())).or_else(|| {
        text.find("->").map(|i| (i, 2))
    })?;
    let (cond_part, tag_part) = (&text[..sep_index.0], &text[sep_index.0 + sep_index.1..]);
    let condition = cond_part.trim().to_string();
    let tag = tag_part.trim().to_string();
    if condition.is_empty() || tag.is_empty() {
        return None;
    }
    Some(ReactClause { condition, tag, span })
}

fn parse_knowledge_field(line: &RawLine, diagnostics: &mut Vec<Diagnostic>) -> Option<KnowledgeField> {
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
    if key.is_empty() || !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
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

// `IndexMap` is referenced via the AST's `PropertyValue`; the
// import is kept for the `out.properties.insert` call above.
#[allow(dead_code)]
fn _indexmap_use(_: &IndexMap<String, PropertyValue>) {}
