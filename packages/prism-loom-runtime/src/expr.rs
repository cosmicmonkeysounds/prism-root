//! Lower parsed `SyntaxNode` expression trees (§10 of the grammar) into
//! the runtime's [`Expr`] form.
//!
//! The parser emits a Pratt-parsed tree of `binary_expr` / `unary_expr`
//! / `postfix_expr` / `grouped_expr` / `ledger_pred` / `resolve_ref`
//! nodes; the runtime needs a flat [`Expr`] the [`evaluate`] walker can
//! consume. This module is the bridge.
//!
//! Unsupported productions (list comprehensions, aggregate calls,
//! s-expressions, `let`-bindings inside expressions) lower to
//! [`Expr::Lit(Value::Nil)`] — they evaluate harmlessly, leaving room
//! for Phase 3+ to wire them in without breaking calling code.

use prism_core::language::loom::node_kinds as nk;
use prism_core::language::syntax::SyntaxNode;

use crate::ledger::LedgerField;
use crate::resolver::{Expr, LedgerPredKind};
use crate::value::Value;

/// Compile a parsed expression node tree into a runtime [`Expr`].
/// Returns `Expr::Lit(Value::Nil)` for any production that's outside
/// the Phase 2 evaluator's surface.
pub fn compile_expr(node: &SyntaxNode) -> Expr {
    match node.kind.as_str() {
        nk::BINARY_EXPR => compile_binary(node),
        nk::UNARY_EXPR => compile_unary(node),
        nk::GROUPED_EXPR => node
            .children
            .first()
            .map(compile_expr)
            .unwrap_or(Expr::Lit(Value::Nil)),
        nk::POSTFIX_EXPR => compile_postfix(node),
        nk::RESOLVE_REF => compile_resolve_ref(node),
        nk::STATIC_REF => compile_static_ref(node),
        nk::LEDGER_PRED => compile_ledger_pred(node),
        nk::AGGREGATE_CALL => compile_aggregate(node),
        nk::NUMBER => Expr::Lit(parse_number(node.value.as_deref().unwrap_or(""))),
        nk::STRING => Expr::Lit(Value::Str(unquote(node.value.as_deref().unwrap_or("")))),
        nk::BOOLEAN => Expr::Lit(match node.value.as_deref() {
            Some("true") => Value::Bool(true),
            _ => Value::Bool(false),
        }),
        nk::NIL => Expr::Lit(Value::Nil),
        nk::IDENT => Expr::Ident(node.value.clone().unwrap_or_default()),
        nk::INLINE_EVAL => node
            .children
            .first()
            .map(compile_expr)
            .unwrap_or(Expr::Lit(Value::Nil)),
        // Catch-all — any production we don't recognize lowers to Nil.
        // Tools downstream can spot it via the `Lit(Nil)` arm if they
        // want to flag unhandled expressions.
        _ => Expr::Lit(Value::Nil),
    }
}

fn compile_binary(node: &SyntaxNode) -> Expr {
    // Children: [lhs, op, rhs] (the parser shape).
    let lhs = node
        .children
        .first()
        .map(compile_expr)
        .unwrap_or(Expr::Lit(Value::Nil));
    let op = node
        .children
        .iter()
        .find(|c| c.kind == "op")
        .and_then(|c| c.value.clone())
        .unwrap_or_default();
    let rhs = node
        .children
        .get(2)
        .map(compile_expr)
        .unwrap_or(Expr::Lit(Value::Nil));
    let l = Box::new(lhs);
    let r = Box::new(rhs);
    match op.as_str() {
        "and" => Expr::And(l, r),
        "or" => Expr::Or(l, r),
        "==" | "is" => Expr::Eq(l, r),
        "!=" => Expr::Neq(l, r),
        "<" => Expr::Lt(l, r),
        "<=" => Expr::Lte(l, r),
        ">" => Expr::Gt(l, r),
        ">=" => Expr::Gte(l, r),
        "+" => Expr::Add(l, r),
        "-" => Expr::Sub(l, r),
        "*" => Expr::Mul(l, r),
        "/" => Expr::Div(l, r),
        // Unknown operator — short-circuit to Nil rather than crash.
        _ => Expr::Lit(Value::Nil),
    }
}

fn compile_unary(node: &SyntaxNode) -> Expr {
    let op = node
        .children
        .iter()
        .find(|c| c.kind == "op")
        .and_then(|c| c.value.clone())
        .unwrap_or_default();
    let operand = node
        .children
        .iter()
        .find(|c| c.kind != "op")
        .map(compile_expr)
        .unwrap_or(Expr::Lit(Value::Nil));
    match op.as_str() {
        "not" | "!" => Expr::Not(Box::new(operand)),
        "-" => Expr::Sub(Box::new(Expr::Lit(Value::Int(0))), Box::new(operand)),
        _ => operand,
    }
}

fn compile_postfix(node: &SyntaxNode) -> Expr {
    // The atom child should be a `resolve_ref` for the cases we care
    // about; field-access tails accumulate into the chain. The runtime
    // doesn't yet support call expressions on arbitrary atoms; those
    // collapse to Nil.
    let Some(atom) = node.children.first() else {
        return Expr::Lit(Value::Nil);
    };
    if atom.kind == nk::RESOLVE_REF {
        let mut expr = compile_resolve_ref(atom);
        for tail in &node.children[1..] {
            expr = extend_chain(expr, tail);
        }
        expr
    } else {
        compile_expr(atom)
    }
}

fn extend_chain(expr: Expr, tail: &SyntaxNode) -> Expr {
    if let Expr::Resolve {
        name,
        mut chain,
        presence_check,
    } = expr
    {
        match tail.kind.as_str() {
            nk::FIELD_ACCESS | nk::SAFE_NAV => {
                if let Some(seg) = tail
                    .children
                    .iter()
                    .find(|c| c.kind == nk::IDENT)
                    .and_then(|c| c.value.clone())
                {
                    chain.push(seg);
                }
            }
            _ => {}
        }
        Expr::Resolve {
            name,
            chain,
            presence_check,
        }
    } else {
        expr
    }
}

fn compile_resolve_ref(node: &SyntaxNode) -> Expr {
    let name = node
        .children
        .iter()
        .find(|c| c.kind == nk::IDENT)
        .and_then(|c| c.value.clone())
        .unwrap_or_default();
    let mut chain = Vec::new();
    if let Some(fc) = node.children.iter().find(|c| c.kind == nk::FIELD_CHAIN) {
        for seg in &fc.children {
            if matches!(seg.kind.as_str(), nk::FIELD_ACCESS | nk::SAFE_NAV) {
                if let Some(id) = seg
                    .children
                    .iter()
                    .find(|c| c.kind == nk::IDENT)
                    .and_then(|c| c.value.clone())
                {
                    chain.push(id);
                }
            }
        }
    }
    let presence_check = node.children.iter().any(|c| c.kind == nk::PRESENCE_CHECK);
    Expr::Resolve {
        name,
        chain,
        presence_check,
    }
}

fn compile_static_ref(node: &SyntaxNode) -> Expr {
    // Static refs evaluate to the qualified name as a string — useful
    // for `match $x.template` arms and equality checks against `@id`.
    let parts: Vec<String> = node
        .children
        .iter()
        .filter(|c| c.kind == nk::IDENT)
        .filter_map(|c| c.value.clone())
        .collect();
    Expr::Lit(Value::Str(format!("@{}", parts.join("."))))
}

fn compile_ledger_pred(node: &SyntaxNode) -> Expr {
    // The first IDENT child is the predicate name; the next IDENT is the arg.
    let idents: Vec<&str> = node
        .children
        .iter()
        .filter(|c| c.kind == nk::IDENT)
        .filter_map(|c| c.value.as_deref())
        .collect();
    let head = idents.first().copied().unwrap_or("");
    let arg = idents.get(1).copied().unwrap_or("").to_string();
    let kind = match head {
        "played" => LedgerPredKind::Played,
        "visits" => LedgerPredKind::Visits,
        "chose" => LedgerPredKind::Chose,
        "since" => LedgerPredKind::Since,
        _ => return Expr::Lit(Value::Nil),
    };
    Expr::LedgerPred { kind, arg }
}

fn compile_aggregate(node: &SyntaxNode) -> Expr {
    // Phase 2 only routes `count(field)` through to the ledger;
    // everything else stays as Nil until the list/aggregate pipeline
    // is wired in.
    let head = node
        .children
        .iter()
        .find(|c| c.kind == nk::IDENT)
        .and_then(|c| c.value.as_deref())
        .unwrap_or("");
    if head != "count" {
        return Expr::Lit(Value::Nil);
    }
    let field_ident = node
        .children
        .iter()
        .skip_while(|c| c.kind != nk::IDENT)
        .nth(1)
        .and_then(|c| c.value.as_deref())
        .unwrap_or("");
    let field = match field_ident {
        "speaker" => LedgerField::Speaker,
        "choice" => LedgerField::Choice,
        "event" => LedgerField::Event,
        _ => return Expr::Lit(Value::Nil),
    };
    Expr::LedgerCount(field)
}

fn parse_number(raw: &str) -> Value {
    // The lexer hands us `10`, `10.5`, `2.5s`, etc. Strip a trailing
    // duration unit before parsing — duration coercion is a Phase 3+
    // concern.
    let cleaned: String = raw
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '-')
        .collect();
    if cleaned.contains('.') {
        cleaned
            .parse::<f64>()
            .map(Value::Float)
            .unwrap_or(Value::Nil)
    } else {
        cleaned.parse::<i64>().map(Value::Int).unwrap_or(Value::Nil)
    }
}

fn unquote(raw: &str) -> String {
    if raw.len() >= 2 && raw.starts_with('"') && raw.ends_with('"') {
        raw[1..raw.len() - 1].to_string()
    } else {
        raw.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prism_core::language::loom::parser::parse;

    fn first_expression(src: &str) -> Expr {
        // Wrap the expression in a `let` binding so the parser routes
        // through `parse_expression`. The resulting tree is:
        //   root → let_binding(IDENT("x"), <expr>)
        let wrapped = format!("# d\nlet __x = {src}\n");
        let parsed = parse(&wrapped);
        let doc = parsed
            .root
            .children
            .iter()
            .find(|c| c.kind == nk::DOCUMENT)
            .expect("document");
        let lb = doc
            .children
            .iter()
            .find(|c| c.kind == nk::LET_BINDING)
            .expect("let-binding");
        let expr_node = lb
            .children
            .iter()
            .find(|c| c.kind != nk::IDENT)
            .expect("rhs");
        compile_expr(expr_node)
    }

    #[test]
    fn lowers_literal_int() {
        assert!(matches!(first_expression("42"), Expr::Lit(Value::Int(42))));
    }

    #[test]
    fn lowers_literal_float() {
        match first_expression("0.5") {
            Expr::Lit(Value::Float(f)) => assert!((f - 0.5).abs() < f64::EPSILON),
            other => panic!("expected float lit, got {other:?}"),
        }
    }

    #[test]
    fn lowers_string_literal() {
        match first_expression("\"hello\"") {
            Expr::Lit(Value::Str(s)) => assert_eq!(s, "hello"),
            other => panic!("expected string lit, got {other:?}"),
        }
    }

    #[test]
    fn lowers_boolean_and_nil() {
        assert!(matches!(
            first_expression("true"),
            Expr::Lit(Value::Bool(true))
        ));
        assert!(matches!(first_expression("nil"), Expr::Lit(Value::Nil)));
    }

    #[test]
    fn lowers_resolve_ref_with_chain() {
        match first_expression("$elena.disposition.trust") {
            Expr::Resolve {
                name,
                chain,
                presence_check,
            } => {
                assert_eq!(name, "elena");
                assert_eq!(chain, vec!["disposition".to_string(), "trust".to_string()]);
                assert!(!presence_check);
            }
            other => panic!("expected resolve, got {other:?}"),
        }
    }

    #[test]
    fn lowers_presence_check() {
        match first_expression("$keeper_name?") {
            Expr::Resolve { presence_check, .. } => assert!(presence_check),
            other => panic!("expected resolve, got {other:?}"),
        }
    }

    #[test]
    fn lowers_binary_comparison() {
        match first_expression("$trust > 50") {
            Expr::Gt(l, r) => {
                assert!(matches!(*l, Expr::Resolve { .. }));
                assert!(matches!(*r, Expr::Lit(Value::Int(50))));
            }
            other => panic!("expected gt, got {other:?}"),
        }
    }

    #[test]
    fn lowers_and_or_chains() {
        match first_expression("$a and $b or $c") {
            Expr::Or(_, _) => {}
            other => panic!("expected or at root, got {other:?}"),
        }
    }

    #[test]
    fn lowers_unary_not() {
        match first_expression("not $betrayed") {
            Expr::Not(inner) => {
                assert!(matches!(*inner, Expr::Resolve { .. }));
            }
            other => panic!("expected not, got {other:?}"),
        }
    }

    #[test]
    fn lowers_ledger_pred() {
        match first_expression("played(start)") {
            Expr::LedgerPred { kind, arg } => {
                assert_eq!(kind, LedgerPredKind::Played);
                assert_eq!(arg, "start");
            }
            other => panic!("expected ledger pred, got {other:?}"),
        }
    }

    #[test]
    fn lowers_grouped_expression() {
        match first_expression("(1 + 2)") {
            Expr::Add(l, r) => {
                assert!(matches!(*l, Expr::Lit(Value::Int(1))));
                assert!(matches!(*r, Expr::Lit(Value::Int(2))));
            }
            other => panic!("expected add, got {other:?}"),
        }
    }

    #[test]
    fn lowers_postfix_field_access_extends_chain() {
        match first_expression("$elena.knows") {
            Expr::Resolve { name, chain, .. } => {
                assert_eq!(name, "elena");
                assert_eq!(chain, vec!["knows".to_string()]);
            }
            other => panic!("expected resolve, got {other:?}"),
        }
    }

    #[test]
    fn static_ref_lowers_to_string() {
        match first_expression("@harbor") {
            Expr::Lit(Value::Str(s)) => assert_eq!(s, "@harbor"),
            other => panic!("expected static ref string, got {other:?}"),
        }
    }
}
