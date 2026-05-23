//! Resolve-reference (`$name(...).field`) lookup engine + a small
//! expression evaluator over the resolved values.
//!
//! Grammar §11.1 defines the lookup chain:
//!
//!   1. Participant scope (when in `as participant`)
//!   2. Conversation roles (`$SPEAKER`, `$LISTENER`, `$PLAYER`, …)
//!   3. Local `let` bindings
//!   4. `var`s in the operand registry
//!   5. Entities
//!   6. Cohorts
//!
//! Phase 1 implements (3) and (4) — `let` bindings and host-supplied
//! vars — plus a `roles` table for (2) the host can populate. Entity
//! / cohort lookup hooks into the bundle's registries as a final
//! fallback. Field chains walk into `Value::Map` recursively.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::ledger::{Ledger, LedgerField};
use super::value::Value;

/// Read-only-ish snapshot the resolver consults. The host owns the
/// stores and mutates them between frames; the resolver only reads.
#[derive(Debug)]
pub struct ResolverContext<'a> {
    pub vars: &'a HashMap<String, Value>,
    pub lets: &'a HashMap<String, Value>,
    pub roles: &'a HashMap<String, Value>,
    pub ledger: &'a Ledger,
}

/// One resolved value plus the slot it came from — useful for
/// debugging and diagnostics. The runtime only consumes the `value`;
/// diagnostics consumers can render the path.
#[derive(Debug, Clone, PartialEq)]
pub struct Resolved {
    pub value: Value,
    pub source: ResolveSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolveSource {
    Let,
    Var,
    Role,
    Missing,
}

/// Resolve `$<name>` against the standard chain. Falls through to
/// `Value::Nil` from a `Missing` source when nothing matches —
/// diagnostics consumers can distinguish "explicitly nil" from
/// "couldn't find" by inspecting `source`.
pub fn resolve_name(ctx: &ResolverContext<'_>, name: &str) -> Resolved {
    if let Some(v) = ctx.lets.get(name).cloned() {
        return Resolved {
            value: v,
            source: ResolveSource::Let,
        };
    }
    if let Some(v) = ctx.vars.get(name).cloned() {
        return Resolved {
            value: v,
            source: ResolveSource::Var,
        };
    }
    if let Some(v) = ctx.roles.get(name).cloned() {
        return Resolved {
            value: v,
            source: ResolveSource::Role,
        };
    }
    Resolved {
        value: Value::Nil,
        source: ResolveSource::Missing,
    }
}

/// Walk into a `$ref.field.subfield...` chain. Returns Nil when any
/// step hits a non-map / missing key — the §10.1 safe-nav behaviour
/// for `?.` is a separate path (call [`field_chain_safe`]).
pub fn field_chain(root: Value, chain: &[String]) -> Value {
    let mut current = root;
    for step in chain {
        current = match current {
            Value::Map(mut m) => m.shift_remove(step).unwrap_or(Value::Nil),
            _ => return Value::Nil,
        };
    }
    current
}

/// Safe-nav variant — returns Nil on the first miss instead of
/// propagating through a non-map.
pub fn field_chain_safe(root: Value, chain: &[String]) -> Value {
    let mut current = root;
    for step in chain {
        current = match current {
            Value::Map(m) => m.get(step).cloned().unwrap_or(Value::Nil),
            Value::Nil => Value::Nil,
            _ => return Value::Nil,
        };
    }
    current
}

/// Tiny expression evaluator covering the operators the runtime's
/// guards need: literals, identifiers (resolved against the chain),
/// `and` / `or` / `not`, equality and comparison, arithmetic, and
/// the ledger predicates. The full Pratt-parsed [`Expr`] tree comes
/// later; today the runtime evaluates a small set of pre-canned
/// guards that the bundle compiler will emit when Phase 2 wires the
/// expression compiler in.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Expr {
    Lit(Value),
    Ident(String),
    /// `$name(.field)*` with optional presence check at tail (`$x?`).
    Resolve {
        name: String,
        chain: Vec<String>,
        presence_check: bool,
    },
    Not(Box<Expr>),
    And(Box<Expr>, Box<Expr>),
    Or(Box<Expr>, Box<Expr>),
    Eq(Box<Expr>, Box<Expr>),
    Neq(Box<Expr>, Box<Expr>),
    Lt(Box<Expr>, Box<Expr>),
    Lte(Box<Expr>, Box<Expr>),
    Gt(Box<Expr>, Box<Expr>),
    Gte(Box<Expr>, Box<Expr>),
    Add(Box<Expr>, Box<Expr>),
    Sub(Box<Expr>, Box<Expr>),
    Mul(Box<Expr>, Box<Expr>),
    Div(Box<Expr>, Box<Expr>),
    /// `played(section)` / `visits(section)` / `chose(key)` /
    /// `since(event)` — argument is the bare identifier token.
    LedgerPred {
        kind: LedgerPredKind,
        arg: String,
    },
    /// `count(field)` / `last(field).<accessor>` — field is the
    /// ledger field name. Currently exposes only `count`.
    LedgerCount(LedgerField),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LedgerPredKind {
    Played,
    Visits,
    Chose,
    Since,
}

pub fn evaluate(expr: &Expr, ctx: &ResolverContext<'_>, now_ms: u64) -> Value {
    match expr {
        Expr::Lit(v) => v.clone(),
        Expr::Ident(name) => resolve_name(ctx, name).value,
        Expr::Resolve {
            name,
            chain,
            presence_check,
        } => {
            let root = resolve_name(ctx, name);
            let v = field_chain_safe(root.value, chain);
            if *presence_check {
                Value::Bool(!matches!(v, Value::Nil))
            } else {
                v
            }
        }
        Expr::Not(inner) => Value::Bool(!evaluate(inner, ctx, now_ms).is_truthy()),
        Expr::And(l, r) => {
            let lv = evaluate(l, ctx, now_ms);
            if lv.is_truthy() {
                evaluate(r, ctx, now_ms)
            } else {
                lv
            }
        }
        Expr::Or(l, r) => {
            let lv = evaluate(l, ctx, now_ms);
            if lv.is_truthy() {
                lv
            } else {
                evaluate(r, ctx, now_ms)
            }
        }
        Expr::Eq(l, r) => Value::Bool(value_eq(
            &evaluate(l, ctx, now_ms),
            &evaluate(r, ctx, now_ms),
        )),
        Expr::Neq(l, r) => Value::Bool(!value_eq(
            &evaluate(l, ctx, now_ms),
            &evaluate(r, ctx, now_ms),
        )),
        Expr::Lt(l, r) => Value::Bool(
            value_cmp(&evaluate(l, ctx, now_ms), &evaluate(r, ctx, now_ms))
                .map(|o| o == std::cmp::Ordering::Less)
                .unwrap_or(false),
        ),
        Expr::Lte(l, r) => Value::Bool(
            value_cmp(&evaluate(l, ctx, now_ms), &evaluate(r, ctx, now_ms))
                .map(|o| o != std::cmp::Ordering::Greater)
                .unwrap_or(false),
        ),
        Expr::Gt(l, r) => Value::Bool(
            value_cmp(&evaluate(l, ctx, now_ms), &evaluate(r, ctx, now_ms))
                .map(|o| o == std::cmp::Ordering::Greater)
                .unwrap_or(false),
        ),
        Expr::Gte(l, r) => Value::Bool(
            value_cmp(&evaluate(l, ctx, now_ms), &evaluate(r, ctx, now_ms))
                .map(|o| o != std::cmp::Ordering::Less)
                .unwrap_or(false),
        ),
        Expr::Add(l, r) => arith(
            &evaluate(l, ctx, now_ms),
            &evaluate(r, ctx, now_ms),
            Op::Add,
        ),
        Expr::Sub(l, r) => arith(
            &evaluate(l, ctx, now_ms),
            &evaluate(r, ctx, now_ms),
            Op::Sub,
        ),
        Expr::Mul(l, r) => arith(
            &evaluate(l, ctx, now_ms),
            &evaluate(r, ctx, now_ms),
            Op::Mul,
        ),
        Expr::Div(l, r) => arith(
            &evaluate(l, ctx, now_ms),
            &evaluate(r, ctx, now_ms),
            Op::Div,
        ),
        Expr::LedgerPred { kind, arg } => match kind {
            LedgerPredKind::Played => Value::Bool(ctx.ledger.played(arg)),
            LedgerPredKind::Visits => Value::Int(ctx.ledger.visits(arg) as i64),
            LedgerPredKind::Chose => Value::Bool(ctx.ledger.chose(arg)),
            LedgerPredKind::Since => ctx
                .ledger
                .since(arg, now_ms)
                .map(|ms| Value::Int(ms as i64))
                .unwrap_or(Value::Nil),
        },
        Expr::LedgerCount(field) => Value::Int(ctx.ledger.count(*field) as i64),
    }
}

#[derive(Debug, Clone, Copy)]
enum Op {
    Add,
    Sub,
    Mul,
    Div,
}

fn value_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Nil, Value::Nil) => true,
        (Value::Int(x), Value::Int(y)) => x == y,
        (Value::Float(x), Value::Float(y)) => x == y,
        (Value::Int(x), Value::Float(y)) | (Value::Float(y), Value::Int(x)) => (*x as f64) == *y,
        (Value::Bool(x), Value::Bool(y)) => x == y,
        (Value::Str(x), Value::Str(y)) => x == y,
        _ => false,
    }
}

fn value_cmp(a: &Value, b: &Value) -> Option<std::cmp::Ordering> {
    use std::cmp::Ordering;
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => x.partial_cmp(y),
        (Value::Float(x), Value::Float(y)) => x.partial_cmp(y),
        (Value::Int(x), Value::Float(y)) => (*x as f64).partial_cmp(y),
        (Value::Float(x), Value::Int(y)) => x.partial_cmp(&(*y as f64)),
        (Value::Str(x), Value::Str(y)) => Some(x.cmp(y)),
        (Value::Bool(x), Value::Bool(y)) => Some(x.cmp(y)),
        (Value::Nil, Value::Nil) => Some(Ordering::Equal),
        _ => None,
    }
}

fn arith(a: &Value, b: &Value, op: Op) -> Value {
    match (a, b, op) {
        (Value::Int(x), Value::Int(y), Op::Add) => Value::Int(x.saturating_add(*y)),
        (Value::Int(x), Value::Int(y), Op::Sub) => Value::Int(x.saturating_sub(*y)),
        (Value::Int(x), Value::Int(y), Op::Mul) => Value::Int(x.saturating_mul(*y)),
        (Value::Int(x), Value::Int(y), Op::Div) if *y != 0 => Value::Int(x / y),
        (Value::Float(x), Value::Float(y), op) => Value::Float(match op {
            Op::Add => x + y,
            Op::Sub => x - y,
            Op::Mul => x * y,
            Op::Div => x / y,
        }),
        (Value::Int(x), Value::Float(y), op) | (Value::Float(y), Value::Int(x), op) => {
            let x = *x as f64;
            let y = *y;
            Value::Float(match op {
                Op::Add => x + y,
                Op::Sub => x - y,
                Op::Mul => x * y,
                Op::Div => x / y,
            })
        }
        // String concat under `+` mirrors the §10 grammar's implicit
        // promotion when one side is a string.
        (Value::Str(x), other, Op::Add) | (other, Value::Str(x), Op::Add) => {
            let other_str = match other {
                Value::Str(s) => s.clone(),
                Value::Int(n) => n.to_string(),
                Value::Float(f) => f.to_string(),
                Value::Bool(b) => b.to_string(),
                _ => String::new(),
            };
            Value::Str(format!("{x}{other_str}"))
        }
        _ => Value::Nil,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use indexmap::IndexMap;

    fn ctx<'a>(
        lets: &'a HashMap<String, Value>,
        vars: &'a HashMap<String, Value>,
        roles: &'a HashMap<String, Value>,
        ledger: &'a Ledger,
    ) -> ResolverContext<'a> {
        ResolverContext {
            lets,
            vars,
            roles,
            ledger,
        }
    }

    #[test]
    fn resolve_name_walks_let_var_role_in_order() {
        let mut lets = HashMap::new();
        let mut vars = HashMap::new();
        let mut roles = HashMap::new();
        let ledger = Ledger::new();
        lets.insert("x".into(), Value::Int(1));
        vars.insert("x".into(), Value::Int(2));
        roles.insert("x".into(), Value::Int(3));
        let c = ctx(&lets, &vars, &roles, &ledger);
        let r = resolve_name(&c, "x");
        assert_eq!(r.value, Value::Int(1));
        assert_eq!(r.source, ResolveSource::Let);
    }

    #[test]
    fn missing_name_returns_nil_with_missing_source() {
        let lets = HashMap::new();
        let vars = HashMap::new();
        let roles = HashMap::new();
        let ledger = Ledger::new();
        let c = ctx(&lets, &vars, &roles, &ledger);
        let r = resolve_name(&c, "ghost");
        assert_eq!(r.value, Value::Nil);
        assert_eq!(r.source, ResolveSource::Missing);
    }

    #[test]
    fn field_chain_walks_into_map() {
        let mut inner = IndexMap::new();
        inner.insert("name".to_string(), Value::Str("Wren".into()));
        let root = Value::Map(inner);
        assert_eq!(
            field_chain(root.clone(), &["name".into()]),
            Value::Str("Wren".into())
        );
        assert_eq!(field_chain(root, &["missing".into()]), Value::Nil);
    }

    #[test]
    fn field_chain_through_non_map_returns_nil() {
        let r = field_chain(Value::Int(7), &["nope".into()]);
        assert_eq!(r, Value::Nil);
    }

    #[test]
    fn arithmetic_promotes_int_to_float() {
        let lets = HashMap::new();
        let vars = HashMap::new();
        let roles = HashMap::new();
        let ledger = Ledger::new();
        let c = ctx(&lets, &vars, &roles, &ledger);
        let e = Expr::Add(
            Box::new(Expr::Lit(Value::Int(2))),
            Box::new(Expr::Lit(Value::Float(0.5))),
        );
        assert_eq!(evaluate(&e, &c, 0), Value::Float(2.5));
    }

    #[test]
    fn comparison_returns_bool() {
        let lets = HashMap::new();
        let vars = HashMap::new();
        let roles = HashMap::new();
        let ledger = Ledger::new();
        let c = ctx(&lets, &vars, &roles, &ledger);
        let e = Expr::Gt(
            Box::new(Expr::Lit(Value::Int(5))),
            Box::new(Expr::Lit(Value::Int(3))),
        );
        assert_eq!(evaluate(&e, &c, 0), Value::Bool(true));
    }

    #[test]
    fn and_short_circuits() {
        let mut vars = HashMap::new();
        vars.insert("met_wren".to_string(), Value::Bool(false));
        let lets = HashMap::new();
        let roles = HashMap::new();
        let ledger = Ledger::new();
        let c = ctx(&lets, &vars, &roles, &ledger);
        let e = Expr::And(
            Box::new(Expr::Ident("met_wren".to_string())),
            Box::new(Expr::Lit(Value::Bool(true))),
        );
        // false && anything → false
        assert_eq!(evaluate(&e, &c, 0), Value::Bool(false));
    }

    #[test]
    fn presence_check_returns_bool() {
        let mut vars = HashMap::new();
        vars.insert("trust".to_string(), Value::Int(50));
        let lets = HashMap::new();
        let roles = HashMap::new();
        let ledger = Ledger::new();
        let c = ctx(&lets, &vars, &roles, &ledger);

        let present = Expr::Resolve {
            name: "trust".to_string(),
            chain: Vec::new(),
            presence_check: true,
        };
        assert_eq!(evaluate(&present, &c, 0), Value::Bool(true));

        let absent = Expr::Resolve {
            name: "ghost".to_string(),
            chain: Vec::new(),
            presence_check: true,
        };
        assert_eq!(evaluate(&absent, &c, 0), Value::Bool(false));
    }

    #[test]
    fn ledger_predicates_route_through_evaluator() {
        let lets = HashMap::new();
        let vars = HashMap::new();
        let roles = HashMap::new();
        let mut ledger = Ledger::new();
        ledger.push(super::super::ledger::LedgerEntry::Visited {
            section: "start".into(),
            at_ms: 0,
        });
        let c = ctx(&lets, &vars, &roles, &ledger);
        let e = Expr::LedgerPred {
            kind: LedgerPredKind::Played,
            arg: "start".into(),
        };
        assert_eq!(evaluate(&e, &c, 0), Value::Bool(true));
        let e2 = Expr::LedgerPred {
            kind: LedgerPredKind::Visits,
            arg: "start".into(),
        };
        assert_eq!(evaluate(&e2, &c, 0), Value::Int(1));
    }
}
