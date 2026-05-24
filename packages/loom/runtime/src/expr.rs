//! Tiny expression language used by directive arguments and reactive
//! `let` bindings (spec §12.1, §14).
//!
//! Scope (Phase 4 — directives + expressions): literals (numbers,
//! strings, booleans, null), identifiers / dotted paths, parenthesised
//! sub-expressions, unary `-` / `!`, binary arithmetic, comparison,
//! logical operators, and call form `name(arg, arg, …)`. The Luau
//! bridge in §14 takes over from here once it lands; for now the
//! runtime evaluates everything natively against a [`World`] scope so
//! the playhead can execute `<set:>`, `<if:>`-style guards, and
//! directive arg lists end-to-end.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// One evaluated value. Deliberately small — the language grows as
/// later phases add typed handles (Character, Location, …).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Value {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
}

impl Value {
    pub fn truthy(&self) -> bool {
        match self {
            Value::Null => false,
            Value::Bool(b) => *b,
            Value::Number(n) => *n != 0.0 && !n.is_nan(),
            Value::String(s) => !s.is_empty(),
        }
    }

    pub fn as_number(&self) -> Option<f64> {
        match self {
            Value::Number(n) => Some(*n),
            Value::Bool(true) => Some(1.0),
            Value::Bool(false) => Some(0.0),
            _ => None,
        }
    }

    pub fn display(&self) -> String {
        match self {
            Value::Null => "null".into(),
            Value::Bool(b) => b.to_string(),
            Value::Number(n) => {
                if n.fract() == 0.0 && n.is_finite() && n.abs() < 1e16 {
                    format!("{}", *n as i64)
                } else {
                    format!("{n}")
                }
            }
            Value::String(s) => s.clone(),
        }
    }
}

/// Read/write scope used by [`eval`] and the `set` builtin. Names are
/// flat dotted paths (`Wren.trust`) — Phase 4 doesn't need a nested
/// object model.
#[derive(Clone, Debug, Default)]
pub struct World {
    values: BTreeMap<String, Value>,
}

impl World {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn get(&self, key: &str) -> Value {
        self.values.get(key).cloned().unwrap_or(Value::Null)
    }
    pub fn set(&mut self, key: impl Into<String>, value: Value) {
        self.values.insert(key.into(), value);
    }
    pub fn entries(&self) -> impl Iterator<Item = (&String, &Value)> {
        self.values.iter()
    }
}

/// Expression AST.
#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    /// Dotted path — `Wren.trust.Player`.
    Path(Vec<String>),
    Unary(UnOp, Box<Expr>),
    Binary(BinOp, Box<Expr>, Box<Expr>),
    Call(String, Vec<Expr>),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UnOp {
    Neg,
    Not,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}

#[derive(Clone, Debug, thiserror::Error, PartialEq)]
pub enum ExprError {
    #[error("expected expression at byte {0}")]
    ExpectedExpression(usize),
    #[error("unexpected token `{0}` at byte {1}")]
    Unexpected(String, usize),
    #[error("unterminated string literal")]
    UnterminatedString,
    #[error("expected `{0}`")]
    Expected(&'static str),
    #[error("unknown function `{0}`")]
    UnknownFunction(String),
}

/// Parse `source` into one [`Expr`]. Any trailing garbage is an error.
pub fn parse(source: &str) -> Result<Expr, ExprError> {
    let tokens = tokenize(source)?;
    let mut p = Parser { tokens, pos: 0 };
    let expr = p.parse_expr(0)?;
    if p.pos != p.tokens.len() {
        let tok = &p.tokens[p.pos];
        return Err(ExprError::Unexpected(format!("{:?}", tok.kind), tok.start));
    }
    Ok(expr)
}

/// Evaluate `expr` against `world`. Unknown identifiers resolve to
/// [`Value::Null`]; unknown call targets are an error. Calls are
/// delegated to `call_fn`, which receives the function name plus a
/// [`CallArg`] per argument carrying both the parsed AST (so a query
/// like `played(intro)` can read the symbolic argument) and the
/// evaluated value.
pub fn eval<F>(expr: &Expr, world: &World, call_fn: &mut F) -> Result<Value, ExprError>
where
    F: FnMut(&str, Vec<CallArg<'_>>) -> Result<Value, ExprError>,
{
    Ok(match expr {
        Expr::Null => Value::Null,
        Expr::Bool(b) => Value::Bool(*b),
        Expr::Number(n) => Value::Number(*n),
        Expr::String(s) => Value::String(s.clone()),
        Expr::Path(segments) => world.get(&segments.join(".")),
        Expr::Unary(op, inner) => {
            let v = eval(inner, world, call_fn)?;
            match op {
                UnOp::Neg => Value::Number(-v.as_number().unwrap_or(0.0)),
                UnOp::Not => Value::Bool(!v.truthy()),
            }
        }
        Expr::Binary(op, l, r) => {
            // Short-circuit logical ops.
            if matches!(op, BinOp::And | BinOp::Or) {
                let lv = eval(l, world, call_fn)?;
                let lt = lv.truthy();
                if matches!(op, BinOp::And) && !lt {
                    return Ok(Value::Bool(false));
                }
                if matches!(op, BinOp::Or) && lt {
                    return Ok(Value::Bool(true));
                }
                return Ok(Value::Bool(eval(r, world, call_fn)?.truthy()));
            }
            let lv = eval(l, world, call_fn)?;
            let rv = eval(r, world, call_fn)?;
            eval_binary(*op, lv, rv)
        }
        Expr::Call(name, args) => {
            let mut packed: Vec<CallArg<'_>> = Vec::with_capacity(args.len());
            for a in args {
                let value = eval(a, world, call_fn)?;
                packed.push(CallArg { expr: a, value });
            }
            call_fn(name, packed)?
        }
    })
}

/// One argument passed to a `call_fn`. Carries both the parsed AST
/// (so ledger queries can read symbolic names like `played(intro)`)
/// and the evaluated [`Value`].
#[derive(Clone, Debug)]
pub struct CallArg<'a> {
    pub expr: &'a Expr,
    pub value: Value,
}

impl CallArg<'_> {
    /// Best-effort symbolic name — the dotted path joined by `.`
    /// when the arg is a [`Expr::Path`], else `None`. Lets queries
    /// like `played(bell_seen)` recover the literal identifier when
    /// the world lookup would resolve to [`Value::Null`].
    pub fn symbol(&self) -> Option<String> {
        match self.expr {
            Expr::Path(segs) => Some(segs.join(".")),
            _ => None,
        }
    }

    /// Coerce the argument to a string — prefer a literal string
    /// value, fall back to the symbolic name for path args, then to
    /// the display form of the evaluated value.
    pub fn as_name(&self) -> String {
        match &self.value {
            Value::String(s) => s.clone(),
            Value::Null => self.symbol().unwrap_or_default(),
            other => self.symbol().unwrap_or_else(|| other.display()),
        }
    }
}

fn eval_binary(op: BinOp, l: Value, r: Value) -> Value {
    let num = |v: &Value| v.as_number().unwrap_or(0.0);
    match op {
        BinOp::Add => match (&l, &r) {
            (Value::String(a), b) => Value::String(format!("{}{}", a, b.display())),
            (a, Value::String(b)) => Value::String(format!("{}{}", a.display(), b)),
            _ => Value::Number(num(&l) + num(&r)),
        },
        BinOp::Sub => Value::Number(num(&l) - num(&r)),
        BinOp::Mul => Value::Number(num(&l) * num(&r)),
        BinOp::Div => Value::Number(num(&l) / num(&r)),
        BinOp::Mod => Value::Number(num(&l) % num(&r)),
        BinOp::Eq => Value::Bool(values_equal(&l, &r)),
        BinOp::Ne => Value::Bool(!values_equal(&l, &r)),
        BinOp::Lt => Value::Bool(num(&l) < num(&r)),
        BinOp::Le => Value::Bool(num(&l) <= num(&r)),
        BinOp::Gt => Value::Bool(num(&l) > num(&r)),
        BinOp::Ge => Value::Bool(num(&l) >= num(&r)),
        BinOp::And | BinOp::Or => unreachable!("short-circuited above"),
    }
}

fn values_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Null, Value::Null) => true,
        (Value::Bool(x), Value::Bool(y)) => x == y,
        (Value::Number(x), Value::Number(y)) => x == y,
        (Value::String(x), Value::String(y)) => x == y,
        _ => {
            // Cross-type numeric coercion so `Wren.trust == 50` works
            // when one side came from an integer literal and the other
            // from a stored f64.
            match (a.as_number(), b.as_number()) {
                (Some(x), Some(y)) => x == y,
                _ => false,
            }
        }
    }
}

// ---------------------------------------------------------------------
// Tokenizer
// ---------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Number(f64),
    String(String),
    Ident(String),
    True,
    False,
    Null,
    And,
    Or,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Bang,
    EqEq,
    NotEq,
    Lt,
    Le,
    Gt,
    Ge,
    LParen,
    RParen,
    Comma,
    Dot,
}

#[derive(Clone, Debug, PartialEq)]
struct Token {
    kind: Tok,
    start: usize,
}

fn tokenize(source: &str) -> Result<Vec<Token>, ExprError> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i] as char;
        if c.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        let start = i;
        // Two-char operators first.
        if i + 1 < bytes.len() {
            let pair = &source[i..i + 2];
            let two = match pair {
                "==" => Some(Tok::EqEq),
                "!=" => Some(Tok::NotEq),
                "<=" => Some(Tok::Le),
                ">=" => Some(Tok::Ge),
                "&&" => Some(Tok::And),
                "||" => Some(Tok::Or),
                _ => None,
            };
            if let Some(kind) = two {
                out.push(Token { kind, start });
                i += 2;
                continue;
            }
        }
        match c {
            '+' => push_single(&mut out, Tok::Plus, start, &mut i),
            '-' => push_single(&mut out, Tok::Minus, start, &mut i),
            '*' => push_single(&mut out, Tok::Star, start, &mut i),
            '/' => push_single(&mut out, Tok::Slash, start, &mut i),
            '%' => push_single(&mut out, Tok::Percent, start, &mut i),
            '!' => push_single(&mut out, Tok::Bang, start, &mut i),
            '<' => push_single(&mut out, Tok::Lt, start, &mut i),
            '>' => push_single(&mut out, Tok::Gt, start, &mut i),
            '(' => push_single(&mut out, Tok::LParen, start, &mut i),
            ')' => push_single(&mut out, Tok::RParen, start, &mut i),
            ',' => push_single(&mut out, Tok::Comma, start, &mut i),
            '.' => push_single(&mut out, Tok::Dot, start, &mut i),
            '"' | '\'' => {
                let quote = c;
                i += 1;
                let str_start = i;
                while i < bytes.len() && bytes[i] as char != quote {
                    i += 1;
                }
                if i >= bytes.len() {
                    return Err(ExprError::UnterminatedString);
                }
                let s = source[str_start..i].to_string();
                i += 1; // skip closing quote
                out.push(Token {
                    kind: Tok::String(s),
                    start,
                });
            }
            ch if ch.is_ascii_digit() => {
                let mut j = i;
                while j < bytes.len() && (bytes[j].is_ascii_digit() || bytes[j] == b'.') {
                    j += 1;
                }
                let n: f64 = source[i..j]
                    .parse()
                    .map_err(|_| ExprError::Unexpected(source[i..j].to_string(), i))?;
                out.push(Token {
                    kind: Tok::Number(n),
                    start,
                });
                i = j;
            }
            ch if ch.is_ascii_alphabetic() || ch == '_' => {
                let mut j = i;
                while j < bytes.len() {
                    let b = bytes[j] as char;
                    if b.is_ascii_alphanumeric() || b == '_' {
                        j += 1;
                    } else {
                        break;
                    }
                }
                let word = &source[i..j];
                let kind = match word {
                    "true" => Tok::True,
                    "false" => Tok::False,
                    "null" => Tok::Null,
                    "and" => Tok::And,
                    "or" => Tok::Or,
                    "not" => Tok::Bang,
                    other => Tok::Ident(other.to_string()),
                };
                out.push(Token { kind, start });
                i = j;
            }
            _ => return Err(ExprError::Unexpected(c.to_string(), i)),
        }
    }
    Ok(out)
}

fn push_single(out: &mut Vec<Token>, kind: Tok, start: usize, i: &mut usize) {
    out.push(Token { kind, start });
    *i += 1;
}

// ---------------------------------------------------------------------
// Parser (precedence climbing)
// ---------------------------------------------------------------------

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.tokens.get(self.pos).map(|t| &t.kind)
    }

    fn bump(&mut self) -> Option<Token> {
        let t = self.tokens.get(self.pos).cloned();
        if t.is_some() {
            self.pos += 1;
        }
        t
    }

    fn parse_expr(&mut self, min_prec: u8) -> Result<Expr, ExprError> {
        let mut lhs = self.parse_prefix()?;
        while let Some(t) = self.peek() {
            let (op, prec) = match binop_prec(t) {
                Some(p) => p,
                None => break,
            };
            if prec < min_prec {
                break;
            }
            self.bump();
            let rhs = self.parse_expr(prec + 1)?;
            lhs = Expr::Binary(op, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn parse_prefix(&mut self) -> Result<Expr, ExprError> {
        let tok = self
            .bump()
            .ok_or(ExprError::ExpectedExpression(usize::MAX))?;
        match tok.kind {
            Tok::Number(n) => Ok(Expr::Number(n)),
            Tok::String(s) => Ok(Expr::String(s)),
            Tok::True => Ok(Expr::Bool(true)),
            Tok::False => Ok(Expr::Bool(false)),
            Tok::Null => Ok(Expr::Null),
            Tok::Minus => Ok(Expr::Unary(
                UnOp::Neg,
                Box::new(self.parse_expr(precedence_unary())?),
            )),
            Tok::Bang => Ok(Expr::Unary(
                UnOp::Not,
                Box::new(self.parse_expr(precedence_unary())?),
            )),
            Tok::LParen => {
                let inner = self.parse_expr(0)?;
                let close = self.bump();
                if !matches!(close.map(|t| t.kind), Some(Tok::RParen)) {
                    return Err(ExprError::Expected(")"));
                }
                Ok(inner)
            }
            Tok::Ident(name) => {
                // Call?
                if matches!(self.peek(), Some(Tok::LParen)) {
                    self.bump();
                    let mut args = Vec::new();
                    if !matches!(self.peek(), Some(Tok::RParen)) {
                        loop {
                            args.push(self.parse_expr(0)?);
                            if matches!(self.peek(), Some(Tok::Comma)) {
                                self.bump();
                                continue;
                            }
                            break;
                        }
                    }
                    let close = self.bump();
                    if !matches!(close.map(|t| t.kind), Some(Tok::RParen)) {
                        return Err(ExprError::Expected(")"));
                    }
                    Ok(Expr::Call(name, args))
                } else {
                    // Dotted path.
                    let mut path = vec![name];
                    while matches!(self.peek(), Some(Tok::Dot)) {
                        self.bump();
                        match self.bump().map(|t| t.kind) {
                            Some(Tok::Ident(seg)) => path.push(seg),
                            _ => return Err(ExprError::Expected("identifier after `.`")),
                        }
                    }
                    Ok(Expr::Path(path))
                }
            }
            other => Err(ExprError::Unexpected(format!("{other:?}"), tok.start)),
        }
    }
}

fn binop_prec(t: &Tok) -> Option<(BinOp, u8)> {
    Some(match t {
        Tok::Or => (BinOp::Or, 1),
        Tok::And => (BinOp::And, 2),
        Tok::EqEq => (BinOp::Eq, 3),
        Tok::NotEq => (BinOp::Ne, 3),
        Tok::Lt => (BinOp::Lt, 4),
        Tok::Le => (BinOp::Le, 4),
        Tok::Gt => (BinOp::Gt, 4),
        Tok::Ge => (BinOp::Ge, 4),
        Tok::Plus => (BinOp::Add, 5),
        Tok::Minus => (BinOp::Sub, 5),
        Tok::Star => (BinOp::Mul, 6),
        Tok::Slash => (BinOp::Div, 6),
        Tok::Percent => (BinOp::Mod, 6),
        _ => return None,
    })
}

fn precedence_unary() -> u8 {
    7
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_calls() -> impl FnMut(&str, Vec<CallArg<'_>>) -> Result<Value, ExprError> {
        |name, _args| Err(ExprError::UnknownFunction(name.into()))
    }

    fn ev(src: &str, world: &World) -> Value {
        let expr = parse(src).expect(src);
        eval(&expr, world, &mut no_calls()).expect(src)
    }

    #[test]
    fn literals_and_arithmetic() {
        let w = World::new();
        assert_eq!(ev("1 + 2 * 3", &w), Value::Number(7.0));
        assert_eq!(ev("(1 + 2) * 3", &w), Value::Number(9.0));
        assert_eq!(ev("-5 + 10", &w), Value::Number(5.0));
    }

    #[test]
    fn comparisons_and_logic() {
        let w = World::new();
        assert_eq!(ev("3 > 2 and 1 == 1", &w), Value::Bool(true));
        assert_eq!(ev("3 < 2 or 1 == 1", &w), Value::Bool(true));
        assert_eq!(ev("not false", &w), Value::Bool(true));
    }

    #[test]
    fn path_lookup_defaults_null() {
        let mut w = World::new();
        w.set("Wren.trust", Value::Number(72.0));
        assert_eq!(ev("Wren.trust", &w), Value::Number(72.0));
        assert_eq!(ev("Wren.unknown", &w), Value::Null);
        assert_eq!(ev("Wren.trust > 50", &w), Value::Bool(true));
    }

    #[test]
    fn string_concat() {
        let w = World::new();
        assert_eq!(ev("'hi ' + 'there'", &w), Value::String("hi there".into()));
        assert_eq!(ev("'n=' + 3", &w), Value::String("n=3".into()));
    }

    #[test]
    fn call_invokes_user_fn() {
        let w = World::new();
        let expr = parse("count(7) + 1").unwrap();
        let mut calls = 0;
        let mut f = |name: &str, args: Vec<CallArg<'_>>| -> Result<Value, ExprError> {
            assert_eq!(name, "count");
            assert_eq!(args.len(), 1);
            assert_eq!(args[0].value, Value::Number(7.0));
            calls += 1;
            Ok(Value::Number(10.0))
        };
        assert_eq!(eval(&expr, &w, &mut f).unwrap(), Value::Number(11.0));
        assert_eq!(calls, 1);
    }
}
