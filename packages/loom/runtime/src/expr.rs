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
    List(Vec<Value>),
}

impl Value {
    pub fn truthy(&self) -> bool {
        match self {
            Value::Null => false,
            Value::Bool(b) => *b,
            Value::Number(n) => *n != 0.0 && !n.is_nan(),
            Value::String(s) => !s.is_empty(),
            Value::List(items) => !items.is_empty(),
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

    /// Borrow the list contents if `self` is a list. Used by
    /// `<for:>` and the `count(…)` helper.
    pub fn as_list(&self) -> Option<&[Value]> {
        match self {
            Value::List(items) => Some(items),
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
            Value::List(items) => {
                let inner: Vec<String> = items.iter().map(Value::display).collect();
                format!("[{}]", inner.join(", "))
            }
        }
    }
}

/// Read/write scope used by [`eval`] and the `set` builtin. Names are
/// flat dotted paths (`Wren.trust`) — Phase 4 doesn't need a nested
/// object model.
///
/// `collections` exposes virtual list values for project-wide groups
/// (`Characters`, `Participants`, `Items`) so list comprehensions and
/// `count(...)` see the live population without requiring authors to
/// hand-maintain a list (spec §12.1).
#[derive(Clone, Debug, Default)]
pub struct World {
    values: BTreeMap<String, Value>,
    collections: BTreeMap<String, Value>,
}

impl World {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn get(&self, key: &str) -> Value {
        if let Some(v) = self.values.get(key) {
            return v.clone();
        }
        if let Some(v) = self.collections.get(key) {
            return v.clone();
        }
        Value::Null
    }
    pub fn set(&mut self, key: impl Into<String>, value: Value) {
        self.values.insert(key.into(), value);
    }
    pub fn entries(&self) -> impl Iterator<Item = (&String, &Value)> {
        self.values.iter()
    }
    /// Publish a virtual collection name — `Characters`, `Participants`,
    /// `Items` — that list comprehensions can iterate over (spec §12.1).
    pub fn set_collection(&mut self, key: impl Into<String>, value: Value) {
        self.collections.insert(key.into(), value);
    }
    /// Read a virtual collection if one is registered. Returns `None`
    /// when the name is unknown (the regular `get` path then falls
    /// back to `Null`).
    pub fn collection(&self, key: &str) -> Option<&Value> {
        self.collections.get(key)
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
    /// `[a, b, c]` list literal.
    List(Vec<Expr>),
    Unary(UnOp, Box<Expr>),
    Binary(BinOp, Box<Expr>, Box<Expr>),
    Call(String, Vec<Expr>),
    /// `[ value for var in source ( where filter )? ]` — list
    /// comprehension over a list-valued `source` expression (spec
    /// §12.1).
    ListComp {
        value: Box<Expr>,
        var: String,
        source: Box<Expr>,
        filter: Option<Box<Expr>>,
    },
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
    eval_scoped(expr, world, &[], call_fn)
}

/// One local binding pair — used by list comprehensions to overlay
/// `var` over the world while evaluating the value / filter sub-expr.
type LocalScope<'a> = &'a [(&'a str, &'a Value)];

fn eval_scoped<F>(
    expr: &Expr,
    world: &World,
    locals: LocalScope<'_>,
    call_fn: &mut F,
) -> Result<Value, ExprError>
where
    F: FnMut(&str, Vec<CallArg<'_>>) -> Result<Value, ExprError>,
{
    Ok(match expr {
        Expr::Null => Value::Null,
        Expr::Bool(b) => Value::Bool(*b),
        Expr::Number(n) => Value::Number(*n),
        Expr::String(s) => Value::String(s.clone()),
        Expr::Path(segments) => resolve_path(segments, world, locals),
        Expr::List(items) => {
            let mut out = Vec::with_capacity(items.len());
            for it in items {
                out.push(eval_scoped(it, world, locals, call_fn)?);
            }
            Value::List(out)
        }
        Expr::Unary(op, inner) => {
            let v = eval_scoped(inner, world, locals, call_fn)?;
            match op {
                UnOp::Neg => Value::Number(-v.as_number().unwrap_or(0.0)),
                UnOp::Not => Value::Bool(!v.truthy()),
            }
        }
        Expr::Binary(op, l, r) => {
            // Short-circuit logical ops.
            if matches!(op, BinOp::And | BinOp::Or) {
                let lv = eval_scoped(l, world, locals, call_fn)?;
                let lt = lv.truthy();
                if matches!(op, BinOp::And) && !lt {
                    return Ok(Value::Bool(false));
                }
                if matches!(op, BinOp::Or) && lt {
                    return Ok(Value::Bool(true));
                }
                return Ok(Value::Bool(
                    eval_scoped(r, world, locals, call_fn)?.truthy(),
                ));
            }
            let lv = eval_scoped(l, world, locals, call_fn)?;
            let rv = eval_scoped(r, world, locals, call_fn)?;
            eval_binary(*op, lv, rv)
        }
        Expr::Call(name, args) => {
            let mut packed: Vec<CallArg<'_>> = Vec::with_capacity(args.len());
            for a in args {
                let value = eval_scoped(a, world, locals, call_fn)?;
                packed.push(CallArg { expr: a, value });
            }
            call_fn(name, packed)?
        }
        Expr::ListComp {
            value,
            var,
            source,
            filter,
        } => {
            // Evaluate the source — must be a list (collections like
            // `Characters` / `Participants` resolve through
            // `World::collection`, plain world entries fall through).
            let src_value = eval_scoped(source, world, locals, call_fn)?;
            let items: Vec<Value> = match src_value {
                Value::List(items) => items,
                _ => Vec::new(),
            };
            let mut out = Vec::with_capacity(items.len());
            for item in &items {
                // Build a new scope chain layered on top of the
                // caller's. The slice borrows live across the inner
                // eval_scoped call without aliasing the world.
                let extended: [(&str, &Value); 1] = [(var.as_str(), item)];
                // Concatenate by allocating a vec — comprehensions are
                // never on a hot loop. The slice is rebuilt every
                // iteration with this iteration's borrow.
                let mut combined: Vec<(&str, &Value)> = locals.to_vec();
                combined.extend_from_slice(&extended);
                if let Some(filter_expr) = filter {
                    let ok = eval_scoped(filter_expr, world, &combined, call_fn)?.truthy();
                    if !ok {
                        continue;
                    }
                }
                out.push(eval_scoped(value, world, &combined, call_fn)?);
            }
            Value::List(out)
        }
    })
}

/// Resolve a dotted path, checking local scope first (for list
/// comprehension iteration variables and beat-scope aliases) then the
/// world. The local can either name the full path head (`c` matching
/// `c.faction`) or the exact full dotted name.
fn resolve_path(segments: &[String], world: &World, locals: LocalScope<'_>) -> Value {
    let head = segments.first().map(String::as_str).unwrap_or("");
    for (name, value) in locals.iter().rev() {
        if *name == head {
            if segments.len() == 1 {
                return (*value).clone();
            }
            // Walk the rest of the path through the bound value's
            // nested form: today a comprehension binds a struct-ish
            // value as a flat string id (e.g. character name), so we
            // re-route to the world using `<id>.<rest>`.
            if let Value::String(id) = value {
                let key = std::iter::once(id.as_str())
                    .chain(segments.iter().skip(1).map(String::as_str))
                    .collect::<Vec<_>>()
                    .join(".");
                return world.get(&key);
            }
            // List/number/bool binds don't carry dotted children.
            return (*value).clone();
        }
    }
    world.get(&segments.join("."))
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
    LBracket,
    RBracket,
    Comma,
    Dot,
    For,
    In,
    Where,
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
            '[' => push_single(&mut out, Tok::LBracket, start, &mut i),
            ']' => push_single(&mut out, Tok::RBracket, start, &mut i),
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
                    "for" => Tok::For,
                    "in" => Tok::In,
                    "where" => Tok::Where,
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
            Tok::LBracket => {
                // Empty list — `[]`.
                if matches!(self.peek(), Some(Tok::RBracket)) {
                    self.bump();
                    return Ok(Expr::List(Vec::new()));
                }
                // Parse one expression: this is either the value of a
                // comprehension or the first element of a literal list.
                let first = self.parse_expr(0)?;
                // Comprehension: `value for var in source [where filter]`.
                if matches!(self.peek(), Some(Tok::For)) {
                    self.bump();
                    let var = match self.bump().map(|t| t.kind) {
                        Some(Tok::Ident(name)) => name,
                        _ => return Err(ExprError::Expected("identifier after `for`")),
                    };
                    if !matches!(self.peek(), Some(Tok::In)) {
                        return Err(ExprError::Expected("`in`"));
                    }
                    self.bump();
                    let source = self.parse_expr(0)?;
                    let filter = if matches!(self.peek(), Some(Tok::Where)) {
                        self.bump();
                        Some(Box::new(self.parse_expr(0)?))
                    } else {
                        None
                    };
                    let close = self.bump();
                    if !matches!(close.map(|t| t.kind), Some(Tok::RBracket)) {
                        return Err(ExprError::Expected("]"));
                    }
                    return Ok(Expr::ListComp {
                        value: Box::new(first),
                        var,
                        source: Box::new(source),
                        filter,
                    });
                }
                let mut items = vec![first];
                while matches!(self.peek(), Some(Tok::Comma)) {
                    self.bump();
                    if matches!(self.peek(), Some(Tok::RBracket)) {
                        break;
                    }
                    items.push(self.parse_expr(0)?);
                }
                let close = self.bump();
                if !matches!(close.map(|t| t.kind), Some(Tok::RBracket)) {
                    return Err(ExprError::Expected("]"));
                }
                Ok(Expr::List(items))
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
    fn list_comprehension_filters_and_maps() {
        // Publish a virtual `Characters` collection naming three
        // characters; for each, the world carries a `<name>.faction`.
        let mut w = World::new();
        w.set_collection(
            "Characters",
            Value::List(vec![
                Value::String("Wren".into()),
                Value::String("Fisher".into()),
                Value::String("Mara".into()),
            ]),
        );
        w.set("Player.faction", Value::String("dawn".into()));
        w.set("Wren.faction", Value::String("dawn".into()));
        w.set("Fisher.faction", Value::String("dusk".into()));
        w.set("Mara.faction", Value::String("dawn".into()));
        let parsed = parse("[c for c in Characters where c.faction == Player.faction]").unwrap();
        let v = eval(&parsed, &w, &mut no_calls()).unwrap();
        let names: Vec<String> = match v {
            Value::List(items) => items.into_iter().map(|v| v.display()).collect(),
            other => panic!("expected list, got {other:?}"),
        };
        assert_eq!(names, vec!["Wren".to_string(), "Mara".to_string()]);
    }

    #[test]
    fn list_comprehension_chains_through_let_results() {
        // A list literal can stand in for a previously-bound `let`.
        let mut w = World::new();
        w.set(
            "nearby",
            Value::List(vec![
                Value::String("Wren".into()),
                Value::String("Fisher".into()),
            ]),
        );
        w.set("Wren.disposition.Player", Value::String("hostile".into()));
        w.set("Fisher.disposition.Player", Value::String("warm".into()));
        let parsed = parse("[c for c in nearby where c.disposition.Player == 'hostile']").unwrap();
        let v = eval(&parsed, &w, &mut no_calls()).unwrap();
        let names: Vec<String> = match v {
            Value::List(items) => items.into_iter().map(|v| v.display()).collect(),
            other => panic!("expected list, got {other:?}"),
        };
        assert_eq!(names, vec!["Wren".to_string()]);
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
