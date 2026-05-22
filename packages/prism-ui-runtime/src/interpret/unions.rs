//! Phase 13 — algebraic property types (discriminated unions) and
//! `<case Variant(fields)>` destructure.
//!
//! See `docs/dev/prui-expressiveness-roadmap.md` §7.2 (Discriminated
//! unions — props that carry variant fields) and §7.10 (Algebraic
//! property types and pattern destructure).
//!
//! Two declaration shapes ship:
//!
//! 1. A top-level `type Tone = info | success(duration: int = 2000) |
//!    error(dismissable: bool = true, retry: action?)` declaration.
//!    The canonical reader emits `<type name="Tone" body="…"/>`; this
//!    module parses the body into a [`UnionDef`].
//! 2. An inline `union<info, success { duration: int = 2000 }, error
//!    { … }>` in a property's type slot. The canonical reader stores
//!    the type as a raw string on the `<property>` element; we sniff
//!    a `union<…>` prefix and parse the body the same way (the inner
//!    `,` separator becomes our outer `|`).
//!
//! Variants are JSON-shaped at runtime: a variant value is
//! `{"tag": "<variant-name>", "<field1>": …, "<field2>": …}`. The
//! match expansion in [`super::control_flow::expand_match`] reads
//! the `tag` field to discriminate; the destructure form pulls each
//! declared field by name into a synthetic `<let>` binding before
//! the case body lowers.
//!
//! Call-site construction (`<Toast tone={error(dismissable=true,
//! retry=$retry)}/>`) is wired through the expression evaluator
//! (`expression::try_call_owned` checks the variant registry before
//! returning `None`).

use std::collections::HashMap;
use std::sync::Arc;

use prism_core::language::prism_ui::{
    AttributeNamespace, AttributeValue, Element, Node as AstNode,
};

use super::components::ParamDef;

/// A single variant — a name plus an ordered field list. Fields are
/// reused [`ParamDef`] rows so defaults and `required` round-trip.
#[derive(Debug, Clone)]
pub struct VariantDef {
    pub name: String,
    pub fields: Vec<ParamDef>,
}

/// One union — a name plus its variants. The variant list preserves
/// declaration order so positional destructure (`<case error(_, r)>`)
/// indexes against it.
#[derive(Debug, Clone)]
pub struct UnionDef {
    pub name: String,
    pub variants: Vec<VariantDef>,
}

impl UnionDef {
    pub fn variant(&self, name: &str) -> Option<&VariantDef> {
        self.variants.iter().find(|v| v.name == name)
    }
}

/// Phase 13 — walk a document and harvest every top-level `<type
/// name="X" body="…"/>` declaration whose body parses as a union.
/// Non-union `type` aliases (`type Id = string`) are skipped — they
/// land in a future-phase scalar-alias registry.
///
/// The `alias` parameter mirrors [`super::harvest_declarations`]'s
/// namespace-prefix handling — a file-level `<namespace name="Forms"/>`
/// directive or an `as=` override on `<import>` prefixes every
/// declared union name as `Forms.Tone` per §7.11.
pub fn harvest_type_decls(
    nodes: &[AstNode],
    alias: Option<&str>,
) -> HashMap<String, Arc<UnionDef>> {
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
        let AstNode::Element(el) = node else { continue };
        if el.tag != "type" {
            continue;
        }
        let Some(name) = bare_string_attr(el, "name") else {
            continue;
        };
        let Some(body) = bare_string_attr(el, "body") else {
            continue;
        };
        let variants = parse_union_body(&body);
        if variants.is_empty() {
            // Scalar / non-variant alias — Phase 13 ignores it.
            continue;
        }
        let qualified = match &prefix {
            Some(p) => format!("{p}.{name}"),
            None => name.clone(),
        };
        out.insert(
            qualified.clone(),
            Arc::new(UnionDef {
                name: qualified,
                variants,
            }),
        );
    }
    out
}

/// Phase 13 — parse the body of a `union<…>` or `type` declaration
/// into an ordered list of variants. Returns an empty list when the
/// body has no top-level `|` separator AND the single token doesn't
/// look like a bare variant name (so a scalar alias `type Id =
/// string` is rejected — its body has neither shape).
///
/// Accepted shapes per variant:
///
/// - `info` — bare name, no fields.
/// - `success { duration: int = 2000 }` — record-form fields.
/// - `success(duration: int = 2000)` — paren-form fields (the §7.10
///   `<case success(d)>` destructure pairs with this).
///
/// The `?` optional suffix on a field type (`retry: action?`) is
/// stripped — the field is parsed as if it were declared `action`.
/// (Phase 13's runtime treats every variant field as optional at the
/// construction site; the `?` is purely for the documented type
/// signature.)
pub fn parse_union_body(body: &str) -> Vec<VariantDef> {
    let raw = body.trim();
    if raw.is_empty() {
        return Vec::new();
    }
    let parts = split_top_level_pipes(raw);
    let mut variants = Vec::new();
    let mut likely_union = parts.len() > 1;
    for part in &parts {
        let Some(v) = parse_variant(part) else {
            continue;
        };
        if !v.fields.is_empty() {
            likely_union = true;
        }
        variants.push(v);
    }
    if !likely_union {
        // Single segment with no fields — e.g. `type Id = string`.
        // Don't claim this as a union.
        return Vec::new();
    }
    variants
}

/// Parse one variant segment (`info`, `success { … }`, `error(…)`).
/// Returns `None` when the segment is empty or starts with a non-
/// identifier character.
fn parse_variant(seg: &str) -> Option<VariantDef> {
    let seg = seg.trim();
    if seg.is_empty() {
        return None;
    }
    // Variant name: identifier prefix.
    let mut end = 0usize;
    for (i, c) in seg.char_indices() {
        if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
            end = i + c.len_utf8();
        } else {
            break;
        }
    }
    if end == 0 {
        return None;
    }
    let name = seg[..end].to_string();
    let rest = seg[end..].trim();
    if rest.is_empty() {
        return Some(VariantDef {
            name,
            fields: Vec::new(),
        });
    }
    let (open, close) = if rest.starts_with('{') && rest.ends_with('}') {
        ('{', '}')
    } else if rest.starts_with('(') && rest.ends_with(')') {
        ('(', ')')
    } else {
        // Unknown trailing form; treat as a field-less variant.
        return Some(VariantDef {
            name,
            fields: Vec::new(),
        });
    };
    let inner = &rest[open.len_utf8()..rest.len() - close.len_utf8()];
    let fields = parse_field_list(inner);
    Some(VariantDef { name, fields })
}

fn parse_field_list(raw: &str) -> Vec<ParamDef> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Vec::new();
    }
    split_top_level_commas(raw)
        .into_iter()
        .filter_map(|seg| parse_field_line(seg.trim()))
        .collect()
}

fn parse_field_line(line: &str) -> Option<ParamDef> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    // `name: type [= default] [required]`
    let (name, rest) = line.split_once(':')?;
    let name = name.trim().to_string();
    if name.is_empty() {
        return None;
    }
    let rest = rest.trim();
    let (ty, default, required) = parse_field_type_default(rest);
    Some(ParamDef {
        name,
        ty: if ty.is_empty() { None } else { Some(ty) },
        default,
        computed_default: None,
        required,
    })
}

fn parse_field_type_default(rest: &str) -> (String, Option<String>, bool) {
    let mut required = false;
    let mut working = rest.to_string();
    // Trailing `required` (or after a `= default`).
    if let Some(stripped) = working.strip_suffix("required") {
        working = stripped.trim_end().to_string();
        required = true;
    }
    // `= default` boundary at top level.
    let (ty, default) = match find_top_level_equals(&working) {
        Some(idx) => {
            let ty = working[..idx].trim().to_string();
            let default = working[idx + 1..].trim().to_string();
            (ty, Some(default))
        }
        None => (working.trim().to_string(), None),
    };
    let mut ty = ty;
    if ty.ends_with('?') {
        // Optional suffix is documentation; the runtime tolerates a
        // missing field regardless.
        ty.pop();
        ty = ty.trim_end().to_string();
    }
    (ty, default, required)
}

/// `union<…>` inline type — sniff the prefix, split the inside on
/// top-level commas (not pipes), and treat each comma-separated
/// segment as one variant (`info`, `success { … }`, `error { … }`).
///
/// Returns `None` when the input doesn't start with `union<` so the
/// caller can fall through to other type-set kinds (`array<T>`,
/// `enum<a|b>`, `object<{…}>`, scalar names).
pub fn parse_inline_union(type_text: &str) -> Option<Vec<VariantDef>> {
    let t = type_text.trim();
    let rest = t.strip_prefix("union<")?;
    let rest = rest.strip_suffix('>')?;
    let parts = split_top_level_commas(rest);
    if parts.is_empty() {
        return None;
    }
    let mut variants = Vec::new();
    for part in parts {
        if let Some(v) = parse_variant(part.trim()) {
            variants.push(v);
        }
    }
    if variants.is_empty() {
        None
    } else {
        Some(variants)
    }
}

/// Build a flat `variant_name → (union_name, ordered field list)`
/// lookup for the expression evaluator. Two unions with the same
/// variant name will pick the first walked (insertion-order); a
/// `prism-cli` lint surfaces the ambiguity later.
pub fn variant_lookup(unions: &HashMap<String, Arc<UnionDef>>) -> HashMap<String, Arc<VariantDef>> {
    let mut out: HashMap<String, Arc<VariantDef>> = HashMap::new();
    for def in unions.values() {
        for variant in &def.variants {
            out.entry(variant.name.clone())
                .or_insert_with(|| Arc::new(variant.clone()));
        }
    }
    out
}

/// Phase 13 — variant constructor call evaluation. Given a variant
/// definition + the call site's positional values + named kwargs,
/// build the `{"tag": name, ...fields}` JSON shape that the runtime
/// match / destructure reads. Missing fields fall through to the
/// variant's declared default; required fields with no provided
/// value still bind (as `null`) — the runtime is graceful, and a
/// Phase 17 lint surfaces the mismatch at parse time.
pub fn build_variant_value(
    variant: &VariantDef,
    positional: &[serde_json::Value],
    kwargs: &[(String, serde_json::Value)],
) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    map.insert(
        "tag".to_string(),
        serde_json::Value::String(variant.name.clone()),
    );
    let kwargs_by_name: HashMap<&str, &serde_json::Value> =
        kwargs.iter().map(|(k, v)| (k.as_str(), v)).collect();
    for (idx, field) in variant.fields.iter().enumerate() {
        if let Some(v) = kwargs_by_name.get(field.name.as_str()) {
            map.insert(field.name.clone(), (*v).clone());
            continue;
        }
        if let Some(v) = positional.get(idx) {
            map.insert(field.name.clone(), v.clone());
            continue;
        }
        if let Some(default) = &field.default {
            map.insert(field.name.clone(), parse_default_literal(default));
            continue;
        }
        // Missing — bind null so destructure reads it as nil-shaped.
        map.insert(field.name.clone(), serde_json::Value::Null);
    }
    serde_json::Value::Object(map)
}

/// Re-implementation of the components.rs default literal parser,
/// kept private so we can land variant defaults without making the
/// other module re-export its helper.
fn parse_default_literal(raw: &str) -> serde_json::Value {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return serde_json::Value::Null;
    }
    if let Some(stripped) = strip_quotes(trimmed) {
        return serde_json::Value::String(stripped.to_string());
    }
    match trimmed {
        "true" => return serde_json::Value::Bool(true),
        "false" => return serde_json::Value::Bool(false),
        "nil" | "null" => return serde_json::Value::Null,
        _ => {}
    }
    if let Ok(n) = trimmed.parse::<i64>() {
        return serde_json::Value::Number(n.into());
    }
    if let Ok(f) = trimmed.parse::<f64>() {
        if let Some(n) = serde_json::Number::from_f64(f) {
            return serde_json::Value::Number(n);
        }
    }
    serde_json::Value::String(trimmed.to_string())
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

fn bare_string_attr(el: &Element, name: &str) -> Option<String> {
    el.attributes
        .iter()
        .find(|a| matches!(a.name.namespace, AttributeNamespace::Bare) && a.name.local == name)
        .and_then(|a| match &a.value {
            AttributeValue::String { value, .. } => Some(value.clone()),
            _ => None,
        })
}

/// Split `body` on top-level `|` (respecting `<…>` / `(…)` / `{…}`
/// / `[…]` / quoted strings). Empty segments are skipped.
fn split_top_level_pipes(body: &str) -> Vec<&str> {
    let bytes = body.as_bytes();
    let mut depth = 0i32;
    let mut quote: Option<u8> = None;
    let mut parts: Vec<&str> = Vec::new();
    let mut start = 0usize;
    let mut i = 0;
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
            b'<' | b'(' | b'[' | b'{' => depth += 1,
            b'>' | b')' | b']' | b'}' => depth -= 1,
            b'|' if depth == 0 => {
                if start < i {
                    parts.push(body[start..i].trim());
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
            parts.push(tail);
        }
    }
    parts.into_iter().filter(|s| !s.is_empty()).collect()
}

/// Split on top-level commas, depth-aware over `<>`, `()`, `[]`, `{}`,
/// `'`, `"`.
pub(crate) fn split_top_level_commas(body: &str) -> Vec<&str> {
    let bytes = body.as_bytes();
    let mut depth = 0i32;
    let mut quote: Option<u8> = None;
    let mut parts: Vec<&str> = Vec::new();
    let mut start = 0usize;
    let mut i = 0;
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
            b'<' | b'(' | b'[' | b'{' => depth += 1,
            b'>' | b')' | b']' | b'}' => depth -= 1,
            b',' if depth == 0 => {
                if start < i {
                    parts.push(body[start..i].trim());
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
            parts.push(tail);
        }
    }
    parts.into_iter().filter(|s| !s.is_empty()).collect()
}

/// Top-level `=` position (depth-aware). Returns `None` when the
/// rest contains no equals sign at depth zero.
fn find_top_level_equals(body: &str) -> Option<usize> {
    let bytes = body.as_bytes();
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
            b'<' | b'(' | b'[' | b'{' => depth += 1,
            b'>' | b')' | b']' | b'}' => depth -= 1,
            b'=' if depth == 0 => return Some(i),
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use prism_core::language::prism_ui::parse;

    #[test]
    fn parses_simple_union_body() {
        let v = parse_union_body("info | success | error");
        assert_eq!(v.len(), 3);
        assert_eq!(v[0].name, "info");
        assert_eq!(v[1].name, "success");
        assert_eq!(v[2].name, "error");
        assert!(v.iter().all(|x| x.fields.is_empty()));
    }

    #[test]
    fn parses_record_form_fields() {
        let v = parse_union_body("info | success { duration: int = 2000 }");
        assert_eq!(v.len(), 2);
        assert!(v[0].fields.is_empty());
        assert_eq!(v[1].name, "success");
        assert_eq!(v[1].fields.len(), 1);
        let f = &v[1].fields[0];
        assert_eq!(f.name, "duration");
        assert_eq!(f.ty.as_deref(), Some("int"));
        assert_eq!(f.default.as_deref(), Some("2000"));
    }

    #[test]
    fn parses_paren_form_fields() {
        let v = parse_union_body("success(duration: int = 2000)");
        // Single variant without `|` separators isn't a union unless
        // it has fields — make sure the paren-form trips the flag.
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].name, "success");
        assert_eq!(v[0].fields[0].name, "duration");
    }

    #[test]
    fn rejects_scalar_alias() {
        let v = parse_union_body("string");
        assert!(v.is_empty());
        let v = parse_union_body("array<int>");
        assert!(v.is_empty());
    }

    #[test]
    fn strips_optional_suffix() {
        let v = parse_union_body("error(retry: action?)");
        assert_eq!(v[0].fields[0].ty.as_deref(), Some("action"));
    }

    #[test]
    fn full_three_variant_union() {
        let v = parse_union_body(
            "info | success { duration: int = 2000 } | error { dismissable: bool = true, retry: action? }",
        );
        assert_eq!(v.len(), 3);
        assert_eq!(v[2].name, "error");
        assert_eq!(v[2].fields.len(), 2);
        assert_eq!(v[2].fields[0].name, "dismissable");
        assert_eq!(v[2].fields[0].default.as_deref(), Some("true"));
        assert_eq!(v[2].fields[1].name, "retry");
        assert_eq!(v[2].fields[1].ty.as_deref(), Some("action"));
    }

    #[test]
    fn parses_inline_union_type() {
        let v = parse_inline_union("union<info, success { duration: int = 2000 }>")
            .expect("union<…> should parse");
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].name, "info");
        assert_eq!(v[1].fields[0].name, "duration");
    }

    #[test]
    fn parses_inline_union_with_required_field() {
        let v = parse_inline_union("union<error { retry: action required }>").unwrap();
        assert!(v[0].fields[0].required);
    }

    #[test]
    fn inline_union_returns_none_for_other_prefixes() {
        assert!(parse_inline_union("array<int>").is_none());
        assert!(parse_inline_union("enum<a|b|c>").is_none());
        assert!(parse_inline_union("string").is_none());
    }

    #[test]
    fn harvests_type_decl_from_document() {
        let (doc, errs) = parse(
            r#"type Tone = info | success(duration: int = 2000) | error(dismissable: bool = true, retry: action?)"#,
        );
        assert!(errs.is_empty(), "parse errors: {errs:?}");
        let unions = harvest_type_decls(&doc.nodes, None);
        let tone = unions.get("Tone").expect("Tone missing");
        assert_eq!(tone.variants.len(), 3);
        assert_eq!(tone.variants[0].name, "info");
        assert_eq!(tone.variants[1].name, "success");
        assert_eq!(tone.variants[2].name, "error");
    }

    #[test]
    fn harvested_type_under_file_namespace() {
        let (doc, _) = parse(
            r#"namespace Notif
type Tone = info | error"#,
        );
        let unions = harvest_type_decls(&doc.nodes, None);
        assert!(unions.contains_key("Notif.Tone"));
        assert!(!unions.contains_key("Tone"));
    }

    #[test]
    fn build_variant_value_positional() {
        let v = VariantDef {
            name: "success".into(),
            fields: vec![ParamDef {
                name: "duration".into(),
                ty: Some("int".into()),
                default: None,
                computed_default: None,
                required: false,
            }],
        };
        let val = build_variant_value(&v, &[serde_json::json!(3000)], &[]);
        assert_eq!(val["tag"], "success");
        assert_eq!(val["duration"], 3000);
    }

    #[test]
    fn build_variant_value_kwargs_win_over_positional() {
        let v = VariantDef {
            name: "error".into(),
            fields: vec![
                ParamDef {
                    name: "dismissable".into(),
                    ty: Some("bool".into()),
                    default: Some("true".into()),
                    computed_default: None,
                    required: false,
                },
                ParamDef {
                    name: "retry".into(),
                    ty: Some("action".into()),
                    default: None,
                    computed_default: None,
                    required: false,
                },
            ],
        };
        let val = build_variant_value(
            &v,
            &[],
            &[
                ("dismissable".into(), serde_json::json!(false)),
                ("retry".into(), serde_json::json!("$cb")),
            ],
        );
        assert_eq!(val["dismissable"], false);
        assert_eq!(val["retry"], "$cb");
    }

    #[test]
    fn build_variant_value_uses_field_defaults() {
        let v = VariantDef {
            name: "error".into(),
            fields: vec![ParamDef {
                name: "dismissable".into(),
                ty: Some("bool".into()),
                default: Some("true".into()),
                computed_default: None,
                required: false,
            }],
        };
        let val = build_variant_value(&v, &[], &[]);
        assert_eq!(val["dismissable"], true);
        assert_eq!(val["tag"], "error");
    }

    #[test]
    fn variant_lookup_flattens_across_unions() {
        let mut unions: HashMap<String, Arc<UnionDef>> = HashMap::new();
        unions.insert(
            "Tone".into(),
            Arc::new(UnionDef {
                name: "Tone".into(),
                variants: vec![
                    VariantDef {
                        name: "info".into(),
                        fields: Vec::new(),
                    },
                    VariantDef {
                        name: "success".into(),
                        fields: Vec::new(),
                    },
                ],
            }),
        );
        let table = variant_lookup(&unions);
        assert!(table.contains_key("info"));
        assert!(table.contains_key("success"));
    }
}
