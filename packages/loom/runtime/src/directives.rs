//! Directives — `<kind: args>` runtime calls (spec §14).
//!
//! At source level a directive looks like a function call wrapped in
//! angle brackets. The parser captures the raw inner text; this
//! module turns that raw text into a structured [`DirectiveCall`] and
//! resolves it against a [`Registry`] of handlers.
//!
//! Phase-4 scope: a native Rust registry with a small set of builtin
//! handlers (`sfx`, `cue`, `set`, `fire`, `pause`, `anchor`). The
//! eventual Luau bridge (spec §14) plugs in by implementing
//! [`Handler`] over a Luau function reference.

use std::collections::HashMap;

use indexmap::IndexMap;

use crate::expr::{self, Expr, ExprError, Value, World};
use crate::ledger::Ledger;

/// One parsed directive call.
#[derive(Clone, Debug, PartialEq)]
pub struct DirectiveCall {
    pub kind: String,
    pub positional: Vec<Expr>,
    pub named: IndexMap<String, Expr>,
    /// `set`'s special-form payload: `<set: lhs op rhs>` is captured
    /// here when present. Other kinds leave this `None`.
    pub assign: Option<Assign>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Assign {
    pub path: Vec<String>,
    pub op: AssignOp,
    pub rhs: Expr,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AssignOp {
    Set,
    AddAssign,
    SubAssign,
    MulAssign,
    DivAssign,
}

#[derive(Clone, Debug, thiserror::Error, PartialEq)]
pub enum DirectiveError {
    #[error("empty directive `<>`")]
    Empty,
    #[error("directive `{0}` is not registered")]
    UnknownKind(String),
    #[error("malformed directive: {0}")]
    Parse(String),
    #[error(transparent)]
    Expr(#[from] ExprError),
    #[error("directive `{kind}` rejected its arguments: {message}")]
    BadArgs { kind: String, message: String },
    #[error("assignment expects `<set: path OP expr>` where OP is `=`/`+=`/`-=`/`*=`/`/=`")]
    BadAssignment,
}

/// Parse `raw` (the inside of `<…>`) into a structured call.
pub fn parse(raw: &str) -> Result<DirectiveCall, DirectiveError> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err(DirectiveError::Empty);
    }
    let (kind, rest) = match raw.find(':') {
        Some(idx) => (raw[..idx].trim().to_string(), raw[idx + 1..].trim()),
        None => (raw.to_string(), ""),
    };
    if kind.is_empty() {
        return Err(DirectiveError::Parse("missing directive name".into()));
    }
    if kind == "set" {
        let assign = parse_assignment(rest)?;
        return Ok(DirectiveCall {
            kind,
            positional: Vec::new(),
            named: IndexMap::new(),
            assign: Some(assign),
        });
    }
    let (positional, named) = parse_args(rest)?;
    Ok(DirectiveCall {
        kind,
        positional,
        named,
        assign: None,
    })
}

fn parse_args(text: &str) -> Result<(Vec<Expr>, IndexMap<String, Expr>), DirectiveError> {
    let mut positional = Vec::new();
    let mut named: IndexMap<String, Expr> = IndexMap::new();
    if text.is_empty() {
        return Ok((positional, named));
    }
    for chunk in split_top_level_commas(text) {
        let chunk = chunk.trim();
        if chunk.is_empty() {
            continue;
        }
        if let Some((name, value)) = top_level_name_value(chunk) {
            named.insert(name, expr::parse(value)?);
        } else {
            positional.push(expr::parse(chunk)?);
        }
    }
    Ok((positional, named))
}

fn parse_assignment(text: &str) -> Result<Assign, DirectiveError> {
    let text = text.trim();
    for (token, op) in [
        ("+=", AssignOp::AddAssign),
        ("-=", AssignOp::SubAssign),
        ("*=", AssignOp::MulAssign),
        ("/=", AssignOp::DivAssign),
    ] {
        if let Some(idx) = find_top_level(text, token) {
            let lhs = text[..idx].trim();
            let rhs = text[idx + token.len()..].trim();
            return build_assign(lhs, rhs, op);
        }
    }
    if let Some(idx) = find_top_level_eq(text) {
        let lhs = text[..idx].trim();
        let rhs = text[idx + 1..].trim();
        return build_assign(lhs, rhs, AssignOp::Set);
    }
    Err(DirectiveError::BadAssignment)
}

fn build_assign(lhs: &str, rhs: &str, op: AssignOp) -> Result<Assign, DirectiveError> {
    let path: Vec<String> = lhs
        .split('.')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if path.is_empty() || !path.iter().all(|s| is_ident(s)) {
        return Err(DirectiveError::BadAssignment);
    }
    Ok(Assign {
        path,
        op,
        rhs: expr::parse(rhs)?,
    })
}

fn is_ident(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn split_top_level_commas(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut quote: Option<char> = None;
    let bytes = text.as_bytes();
    let mut last = 0;
    for (i, &b) in bytes.iter().enumerate() {
        let c = b as char;
        if let Some(q) = quote {
            if c == q {
                quote = None;
            }
            continue;
        }
        match c {
            '"' | '\'' => quote = Some(c),
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            ',' if depth == 0 => {
                out.push(&text[last..i]);
                last = i + 1;
            }
            _ => {}
        }
    }
    out.push(&text[last..]);
    out
}

fn top_level_name_value(chunk: &str) -> Option<(String, &str)> {
    let mut depth = 0i32;
    let mut quote: Option<char> = None;
    for (i, b) in chunk.bytes().enumerate() {
        let c = b as char;
        if let Some(q) = quote {
            if c == q {
                quote = None;
            }
            continue;
        }
        match c {
            '"' | '\'' => quote = Some(c),
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            ':' if depth == 0 => {
                let name = chunk[..i].trim();
                if is_ident(name) {
                    return Some((name.to_string(), &chunk[i + 1..]));
                }
                return None;
            }
            _ => {}
        }
    }
    None
}

fn find_top_level(text: &str, needle: &str) -> Option<usize> {
    let nb = needle.as_bytes();
    let bytes = text.as_bytes();
    let mut depth = 0i32;
    let mut quote: Option<char> = None;
    let mut i = 0;
    while i + nb.len() <= bytes.len() {
        let c = bytes[i] as char;
        if let Some(q) = quote {
            if c == q {
                quote = None;
            }
            i += 1;
            continue;
        }
        match c {
            '"' | '\'' => {
                quote = Some(c);
                i += 1;
                continue;
            }
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            _ => {}
        }
        if depth == 0 && &bytes[i..i + nb.len()] == nb {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// Find a top-level `=` that is **not** part of `==`, `!=`, `<=`, `>=`,
/// `+=`, `-=`, `*=`, `/=`.
fn find_top_level_eq(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut depth = 0i32;
    let mut quote: Option<char> = None;
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i] as char;
        if let Some(q) = quote {
            if c == q {
                quote = None;
            }
            i += 1;
            continue;
        }
        match c {
            '"' | '\'' => {
                quote = Some(c);
            }
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            '=' if depth == 0 => {
                let prev = if i > 0 { bytes[i - 1] as char } else { ' ' };
                let next = if i + 1 < bytes.len() {
                    bytes[i + 1] as char
                } else {
                    ' '
                };
                if next != '=' && !matches!(prev, '=' | '!' | '<' | '>' | '+' | '-' | '*' | '/') {
                    return Some(i);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

// ---------------------------------------------------------------------
// Registry
// ---------------------------------------------------------------------

pub struct CallContext<'a> {
    pub kind: &'a str,
    pub positional: &'a [Value],
    pub named: &'a IndexMap<String, Value>,
    pub assign: Option<&'a Assign>,
    pub world: &'a mut World,
    pub ledger: &'a mut Ledger,
}

pub enum HandlerOutcome {
    /// The playhead will emit `Event::Directive { kind, … }`.
    Handled,
    /// The handler already pushed whatever events it wanted; the
    /// generic envelope is skipped.
    Suppressed,
}

pub trait Handler: Send + Sync {
    fn call(&self, ctx: &mut CallContext<'_>) -> Result<HandlerOutcome, DirectiveError>;
}

#[derive(Default)]
pub struct Registry {
    handlers: HashMap<String, Box<dyn Handler>>,
}

impl Registry {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn register(&mut self, name: impl Into<String>, handler: impl Handler + 'static) {
        self.handlers.insert(name.into(), Box::new(handler));
    }
    pub fn contains(&self, name: &str) -> bool {
        self.handlers.contains_key(name)
    }
    pub fn get(&self, name: &str) -> Option<&dyn Handler> {
        self.handlers.get(name).map(|h| h.as_ref())
    }
    pub fn with_builtins() -> Self {
        let mut r = Self::new();
        crate::builtins::register(&mut r);
        r
    }
}

/// Dispatch one [`DirectiveCall`] through `registry`, evaluating its
/// expression args against `world` first.
pub fn dispatch(
    call: &DirectiveCall,
    registry: &Registry,
    world: &mut World,
    ledger: &mut Ledger,
) -> Result<(HandlerOutcome, Vec<Value>, IndexMap<String, Value>), DirectiveError> {
    let handler = registry
        .get(&call.kind)
        .ok_or_else(|| DirectiveError::UnknownKind(call.kind.clone()))?;

    let positional = eval_args(&call.positional, world)?;
    let named = eval_named(&call.named, world)?;
    let outcome = {
        let mut ctx = CallContext {
            kind: &call.kind,
            positional: &positional,
            named: &named,
            assign: call.assign.as_ref(),
            world,
            ledger,
        };
        handler.call(&mut ctx)?
    };
    Ok((outcome, positional, named))
}

fn eval_args(args: &[Expr], world: &World) -> Result<Vec<Value>, DirectiveError> {
    let mut out = Vec::with_capacity(args.len());
    for a in args {
        out.push(expr::eval(a, world, &mut |name, _args| {
            Err(ExprError::UnknownFunction(name.into()))
        })?);
    }
    Ok(out)
}

fn eval_named(
    args: &IndexMap<String, Expr>,
    world: &World,
) -> Result<IndexMap<String, Value>, DirectiveError> {
    let mut out = IndexMap::new();
    for (k, v) in args {
        out.insert(
            k.clone(),
            expr::eval(v, world, &mut |name, _args| {
                Err(ExprError::UnknownFunction(name.into()))
            })?,
        );
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bare_call() {
        let c = parse("sfx").unwrap();
        assert_eq!(c.kind, "sfx");
        assert!(c.positional.is_empty());
        assert!(c.named.is_empty());
    }

    #[test]
    fn parses_positional_and_named() {
        let c = parse("sfx: bell, fade: 200").unwrap();
        assert_eq!(c.kind, "sfx");
        assert_eq!(c.positional.len(), 1);
        assert_eq!(c.named.len(), 1);
        assert!(c.named.contains_key("fade"));
    }

    #[test]
    fn parses_string_with_comma_inside() {
        let c = parse("cue: 'one, two', main").unwrap();
        assert_eq!(c.positional.len(), 2);
    }

    #[test]
    fn parses_assignment_set() {
        let c = parse("set: Wren.trust = 50").unwrap();
        let a = c.assign.unwrap();
        assert_eq!(a.path, vec!["Wren", "trust"]);
        assert_eq!(a.op, AssignOp::Set);
    }

    #[test]
    fn parses_compound_assignment() {
        let c = parse("set: stats.health += 10").unwrap();
        let a = c.assign.unwrap();
        assert_eq!(a.op, AssignOp::AddAssign);
    }

    #[test]
    fn rejects_set_without_assignment() {
        assert_eq!(parse("set: 42"), Err(DirectiveError::BadAssignment));
    }
}
