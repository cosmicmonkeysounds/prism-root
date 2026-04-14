//! Expression evaluator + context-store helpers.
//!
//! Port of `language/expression/evaluator.ts`. Built-ins are
//! implemented twice: a numeric-only `BUILTINS` table for the
//! "abs / ceil / floor / …" family, and an extended variadic
//! table for string / date / aggregate helpers.

use std::collections::HashMap;

use chrono::{Datelike, NaiveDate, NaiveDateTime, Utc};

use super::expression_types::{AnyExprNode, BinaryOp, ExprValue, UnaryOp, ValueStore};
use super::parser::parse;

pub fn evaluate(node: &AnyExprNode, store: &dyn ValueStore) -> ExprValue {
    eval_node(node, store)
}

fn eval_node(node: &AnyExprNode, store: &dyn ValueStore) -> ExprValue {
    match node {
        AnyExprNode::Literal { value, .. } => value.clone(),
        AnyExprNode::Operand {
            operand_type,
            id,
            subfield,
        } => store.resolve(operand_type, id, subfield.as_deref()),
        AnyExprNode::Unary { op, operand } => {
            let v = eval_node(operand, store);
            match op {
                UnaryOp::Neg => ExprValue::Number(-v.to_number()),
                UnaryOp::Not => ExprValue::Boolean(!v.to_boolean()),
            }
        }
        AnyExprNode::Binary { op, left, right } => eval_binary(*op, left, right, store),
        AnyExprNode::Call { name, args } => eval_call(name, args, store),
    }
}

fn eval_binary(
    op: BinaryOp,
    left: &AnyExprNode,
    right: &AnyExprNode,
    store: &dyn ValueStore,
) -> ExprValue {
    if op == BinaryOp::And {
        let lv = eval_node(left, store);
        return if lv.to_boolean() {
            eval_node(right, store)
        } else {
            lv
        };
    }
    if op == BinaryOp::Or {
        let lv = eval_node(left, store);
        return if lv.to_boolean() {
            lv
        } else {
            eval_node(right, store)
        };
    }

    let lv = eval_node(left, store);
    let rv = eval_node(right, store);

    match op {
        BinaryOp::Add => {
            if matches!(lv, ExprValue::String(_)) || matches!(rv, ExprValue::String(_)) {
                ExprValue::String(lv.to_string_value() + &rv.to_string_value())
            } else {
                ExprValue::Number(lv.to_number() + rv.to_number())
            }
        }
        BinaryOp::Sub => ExprValue::Number(lv.to_number() - rv.to_number()),
        BinaryOp::Mul => ExprValue::Number(lv.to_number() * rv.to_number()),
        BinaryOp::Div => {
            let d = rv.to_number();
            if d == 0.0 {
                ExprValue::Number(0.0)
            } else {
                ExprValue::Number(lv.to_number() / d)
            }
        }
        BinaryOp::Pow => ExprValue::Number(lv.to_number().powf(rv.to_number())),
        BinaryOp::Mod => {
            // JS `%` preserves the sign of the left operand; Rust
            // `%` does the same for floats so this is 1-for-1.
            ExprValue::Number(lv.to_number() % rv.to_number())
        }
        BinaryOp::Eq => ExprValue::Boolean(lv.loose_eq(&rv)),
        BinaryOp::Ne => ExprValue::Boolean(!lv.loose_eq(&rv)),
        BinaryOp::Lt => ExprValue::Boolean(lv.to_number() < rv.to_number()),
        BinaryOp::Lte => ExprValue::Boolean(lv.to_number() <= rv.to_number()),
        BinaryOp::Gt => ExprValue::Boolean(lv.to_number() > rv.to_number()),
        BinaryOp::Gte => ExprValue::Boolean(lv.to_number() >= rv.to_number()),
        BinaryOp::And | BinaryOp::Or => unreachable!("handled above"),
    }
}

fn eval_call(name: &str, args: &[AnyExprNode], store: &dyn ValueStore) -> ExprValue {
    let lower = name.to_ascii_lowercase();
    let evaluated: Vec<ExprValue> = args.iter().map(|a| eval_node(a, store)).collect();

    // Extended (variadic / non-numeric) builtins first.
    if let Some(v) = eval_extended_builtin(&lower, &evaluated) {
        return v;
    }

    // Numeric builtins: coerce args to numbers.
    let nums: Vec<f64> = evaluated.iter().map(|v| v.to_number()).collect();
    if let Some(v) = eval_numeric_builtin(&lower, &nums) {
        return ExprValue::Number(v);
    }
    ExprValue::Number(0.0)
}

fn eval_numeric_builtin(name: &str, args: &[f64]) -> Option<f64> {
    let a = |i: usize| args.get(i).copied().unwrap_or(0.0);
    Some(match name {
        "abs" => a(0).abs(),
        "ceil" => a(0).ceil(),
        "floor" => a(0).floor(),
        "round" => {
            // JS `Math.round` rounds half to +∞; Rust `f64::round`
            // rounds half away from zero. Reimplement half-to-+∞.
            (a(0) + 0.5).floor()
        }
        "sqrt" => a(0).sqrt(),
        "pow" => a(0).powf(args.get(1).copied().unwrap_or(1.0)),
        "min" => a(0).min(a(1)),
        "max" => a(0).max(a(1)),
        "clamp" => a(2).min(a(0).max(a(1))),
        _ => return None,
    })
}

fn eval_extended_builtin(name: &str, args: &[ExprValue]) -> Option<ExprValue> {
    let get_str = |i: usize| args.get(i).map(|v| v.to_string_value()).unwrap_or_default();
    let get_num = |i: usize| args.get(i).map(|v| v.to_number()).unwrap_or(0.0);

    match name {
        "len" => Some(ExprValue::Number(get_str(0).chars().count() as f64)),
        "lower" => Some(ExprValue::String(get_str(0).to_lowercase())),
        "upper" => Some(ExprValue::String(get_str(0).to_uppercase())),
        "trim" => Some(ExprValue::String(get_str(0).trim().to_string())),
        "concat" => Some(ExprValue::String(
            args.iter().map(|v| v.to_string_value()).collect::<String>(),
        )),
        "left" => {
            let s = get_str(0);
            let n = get_num(1).max(0.0).floor() as usize;
            Some(ExprValue::String(s.chars().take(n).collect()))
        }
        "right" => {
            let s = get_str(0);
            let n = get_num(1).max(0.0).floor() as usize;
            if n == 0 {
                Some(ExprValue::String(String::new()))
            } else {
                let total = s.chars().count();
                let skip = total.saturating_sub(n);
                Some(ExprValue::String(s.chars().skip(skip).collect()))
            }
        }
        "mid" => {
            let s = get_str(0);
            let start = get_num(1).max(0.0).floor() as usize;
            let len = get_num(2).max(0.0).floor() as usize;
            Some(ExprValue::String(s.chars().skip(start).take(len).collect()))
        }
        "substitute" => {
            let s = get_str(0);
            let find = get_str(1);
            let replace = get_str(2);
            if find.is_empty() {
                Some(ExprValue::String(s))
            } else {
                Some(ExprValue::String(s.replace(&find, &replace)))
            }
        }
        "today" => Some(ExprValue::String(Utc::now().date_naive().to_string())),
        "now" => Some(ExprValue::String(
            Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string(),
        )),
        "year" => Some(ExprValue::Number(
            parse_iso_date(&get_str(0))
                .map(|d| d.year() as f64)
                .unwrap_or(0.0),
        )),
        "month" => Some(ExprValue::Number(
            parse_iso_date(&get_str(0))
                .map(|d| d.month() as f64)
                .unwrap_or(0.0),
        )),
        "day" => Some(ExprValue::Number(
            parse_iso_date(&get_str(0))
                .map(|d| d.day() as f64)
                .unwrap_or(0.0),
        )),
        "datediff" => {
            let da = parse_iso_date(&get_str(0));
            let db = parse_iso_date(&get_str(1));
            if let (Some(da), Some(db)) = (da, db) {
                let unit = get_str(2).to_lowercase();
                let unit = if unit.is_empty() {
                    "days".to_string()
                } else {
                    unit
                };
                let n = match unit.as_str() {
                    "months" => {
                        ((db.year() - da.year()) * 12 + (db.month() as i32 - da.month() as i32))
                            as f64
                    }
                    "years" => (db.year() - da.year()) as f64,
                    _ => {
                        let ms = db.signed_duration_since(da).num_milliseconds();
                        (ms as f64 / (1000.0 * 60.0 * 60.0 * 24.0)).floor()
                    }
                };
                Some(ExprValue::Number(n))
            } else {
                Some(ExprValue::Number(0.0))
            }
        }
        "sum" => Some(ExprValue::Number(args.iter().map(|v| v.to_number()).sum())),
        "avg" => {
            if args.is_empty() {
                Some(ExprValue::Number(0.0))
            } else {
                let total: f64 = args.iter().map(|v| v.to_number()).sum();
                Some(ExprValue::Number(total / args.len() as f64))
            }
        }
        "count" => Some(ExprValue::Number(args.len() as f64)),
        _ => None,
    }
}

fn parse_iso_date(s: &str) -> Option<NaiveDateTime> {
    if s.is_empty() {
        return None;
    }
    if let Ok(d) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        return Some(d.and_hms_opt(0, 0, 0).unwrap());
    }
    if let Ok(dt) = NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S") {
        return Some(dt);
    }
    if let Ok(dt) = NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.3f") {
        return Some(dt);
    }
    if let Ok(dt) = NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%SZ") {
        return Some(dt);
    }
    if let Ok(dt) = NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.3fZ") {
        return Some(dt);
    }
    None
}

// ── evaluateExpression wrapper ────────────────────────────────────

fn builtin_names() -> &'static [&'static str] {
    &[
        "abs",
        "ceil",
        "floor",
        "round",
        "sqrt",
        "pow",
        "min",
        "max",
        "clamp",
        "len",
        "lower",
        "upper",
        "trim",
        "concat",
        "left",
        "right",
        "mid",
        "substitute",
        "today",
        "now",
        "year",
        "month",
        "day",
        "datediff",
        "sum",
        "avg",
        "count",
    ]
}

fn is_keyword(lower: &str) -> bool {
    matches!(lower, "true" | "false" | "and" | "or" | "not")
}

fn is_builtin(lower: &str) -> bool {
    builtin_names().contains(&lower)
}

/// Wrap bare identifiers that aren't builtins or keywords with
/// `[field:<name>]` so the expression parser treats them as
/// operands. Matches the TS regex walker 1:1 (strings, operands,
/// and keywords are passed through; only bare idents get wrapped).
pub fn wrap_bare_identifiers(formula: &str) -> String {
    let bytes = formula.as_bytes();
    let mut out = String::with_capacity(formula.len());
    let mut i = 0usize;
    while i < bytes.len() {
        let ch = bytes[i] as char;

        // Bracketed operand: consume until matching ']'.
        if ch == '[' {
            if let Some(rel) = formula[i + 1..].find(']') {
                let end = i + 1 + rel + 1;
                out.push_str(&formula[i..end]);
                i = end;
                continue;
            }
        }

        // Quoted string: consume through closing quote.
        if ch == '\'' || ch == '"' {
            let quote = ch;
            out.push(ch);
            i += 1;
            while i < bytes.len() && bytes[i] as char != quote {
                if bytes[i] as char == '\\' && i + 1 < bytes.len() {
                    out.push(bytes[i] as char);
                    out.push(bytes[i + 1] as char);
                    i += 2;
                    continue;
                }
                out.push(bytes[i] as char);
                i += 1;
            }
            if i < bytes.len() {
                out.push(bytes[i] as char);
                i += 1;
            }
            continue;
        }

        // Identifier: collect [a-zA-Z_][a-zA-Z0-9_]*
        if ch.is_ascii_alphabetic() || ch == '_' {
            let start = i;
            while i < bytes.len() {
                let c = bytes[i] as char;
                if c.is_ascii_alphanumeric() || c == '_' {
                    i += 1;
                } else {
                    break;
                }
            }
            let name = &formula[start..i];
            let lower = name.to_ascii_lowercase();
            if is_builtin(&lower) || is_keyword(&lower) {
                out.push_str(name);
            } else {
                out.push_str("[field:");
                out.push_str(name);
                out.push(']');
            }
            continue;
        }

        out.push(ch);
        i += 1;
    }
    out
}

/// Simple `HashMap<String, ExprValue>`-backed store.
pub struct ContextStore<'a> {
    ctx: &'a HashMap<String, ExprValue>,
}

impl<'a> ContextStore<'a> {
    pub fn new(ctx: &'a HashMap<String, ExprValue>) -> Self {
        Self { ctx }
    }
}

impl<'a> ValueStore for ContextStore<'a> {
    fn resolve(&self, _operand_type: &str, id: &str, _subfield: Option<&str>) -> ExprValue {
        self.ctx.get(id).cloned().unwrap_or(ExprValue::Number(0.0))
    }
}

pub struct EvaluateResult {
    pub result: ExprValue,
    pub errors: Vec<String>,
}

pub fn evaluate_expression(formula: &str, ctx: &HashMap<String, ExprValue>) -> EvaluateResult {
    let wrapped = wrap_bare_identifiers(formula);
    let parse_result = parse(&wrapped);
    if parse_result.node.is_none() || !parse_result.errors.is_empty() {
        return EvaluateResult {
            result: ExprValue::Boolean(false),
            errors: parse_result.errors.into_iter().map(|e| e.message).collect(),
        };
    }
    let store = ContextStore::new(ctx);
    let node = parse_result.node.unwrap();
    let result = evaluate(&node, &store);
    EvaluateResult {
        result,
        errors: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn num(n: f64) -> ExprValue {
        ExprValue::Number(n)
    }

    fn s(v: &str) -> ExprValue {
        ExprValue::String(v.to_string())
    }

    fn ctx(pairs: &[(&str, ExprValue)]) -> HashMap<String, ExprValue> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), v.clone()))
            .collect()
    }

    #[test]
    fn arithmetic() {
        let r = evaluate_expression("1 + 2 * 3", &ctx(&[]));
        assert_eq!(r.result, num(7.0));
        assert!(r.errors.is_empty());
    }

    #[test]
    fn string_concat_via_plus() {
        let r = evaluate_expression("'hello ' + 'world'", &ctx(&[]));
        assert_eq!(r.result, s("hello world"));
    }

    #[test]
    fn field_lookup_via_wrapping() {
        let r = evaluate_expression("amount * 2", &ctx(&[("amount", num(5.0))]));
        assert_eq!(r.result, num(10.0));
    }

    #[test]
    fn abs_and_pow_builtins() {
        assert_eq!(evaluate_expression("abs(-5)", &ctx(&[])).result, num(5.0));
        assert_eq!(
            evaluate_expression("pow(2, 10)", &ctx(&[])).result,
            num(1024.0)
        );
    }

    #[test]
    fn short_circuit_and_or() {
        let r = evaluate_expression("false and 1/0", &ctx(&[]));
        assert_eq!(r.result, ExprValue::Boolean(false));
        let r = evaluate_expression("true or 1/0", &ctx(&[]));
        assert_eq!(r.result, ExprValue::Boolean(true));
    }

    #[test]
    fn comparison_returns_boolean() {
        assert_eq!(
            evaluate_expression("3 < 5", &ctx(&[])).result,
            ExprValue::Boolean(true)
        );
    }

    #[test]
    fn string_builtins() {
        assert_eq!(
            evaluate_expression("upper('abc')", &ctx(&[])).result,
            s("ABC")
        );
        assert_eq!(
            evaluate_expression("len('abc')", &ctx(&[])).result,
            num(3.0)
        );
        assert_eq!(
            evaluate_expression("left('hello', 3)", &ctx(&[])).result,
            s("hel")
        );
        assert_eq!(
            evaluate_expression("right('hello', 3)", &ctx(&[])).result,
            s("llo")
        );
    }

    #[test]
    fn datediff_days() {
        let r = evaluate_expression("datediff('2026-01-01', '2026-01-11', 'days')", &ctx(&[]));
        assert_eq!(r.result, num(10.0));
    }

    #[test]
    fn wrap_bare_identifiers_leaves_keywords() {
        let wrapped = wrap_bare_identifiers("amount > 0 and not done");
        assert_eq!(wrapped, "[field:amount] > 0 and not [field:done]");
    }

    #[test]
    fn wrap_bare_identifiers_leaves_brackets_and_strings() {
        let wrapped = wrap_bare_identifiers("[field:x] + 'hello'");
        assert_eq!(wrapped, "[field:x] + 'hello'");
    }

    #[test]
    fn sum_and_avg_builtins() {
        assert_eq!(
            evaluate_expression("sum(1, 2, 3, 4)", &ctx(&[])).result,
            num(10.0)
        );
        assert_eq!(
            evaluate_expression("avg(2, 4, 6)", &ctx(&[])).result,
            num(4.0)
        );
    }
}
