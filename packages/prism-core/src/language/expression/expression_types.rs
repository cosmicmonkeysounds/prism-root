//! Expression AST + value types.
//!
//! Port of `language/expression/expression-types.ts`. Kept
//! serde-compatible with the legacy JSON shape so formula strings
//! stored in Loro CRDT payloads parse back into the same tree.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExprType {
    Number,
    Boolean,
    String,
    Unknown,
}

/// Runtime value produced by an expression.
///
/// `Null` was added 2026-05-13 to close the field-editor DSL gap
/// (Wave 11.3 of `docs/dev/composable-builder-plan.md`): the
/// JSON-prop reader returns `Null` for missing keys, and DSL authors
/// write `value == null` / `value != null` against it. Coerces to
/// `0` / `false` / `""` for the other type ladders so arithmetic and
/// truthiness round-trip safely against missing data.
#[derive(Debug, Clone, PartialEq)]
pub enum ExprValue {
    Number(f64),
    Boolean(bool),
    String(String),
    Null,
}

impl ExprValue {
    pub fn to_number(&self) -> f64 {
        match self {
            Self::Number(n) => *n,
            Self::Boolean(b) => {
                if *b {
                    1.0
                } else {
                    0.0
                }
            }
            Self::String(s) => s.parse::<f64>().unwrap_or(0.0),
            Self::Null => 0.0,
        }
    }

    pub fn to_boolean(&self) -> bool {
        match self {
            Self::Boolean(b) => *b,
            Self::Number(n) => *n != 0.0,
            Self::String(s) => !s.is_empty(),
            // Null is falsy — matches JS / Lua's `if value then` shape
            // so DSL `value ? "x" : "y"` without an explicit null
            // check picks the "y" branch on missing data.
            Self::Null => false,
        }
    }

    pub fn to_string_value(&self) -> String {
        match self {
            Self::String(s) => s.clone(),
            Self::Number(n) => format_number(*n),
            Self::Boolean(b) => b.to_string(),
            // Empty string keeps interpolation-into-attributes
            // (`<input value="{value}"/>`) clean when the source is
            // missing — no literal "null" leaks into rendered HTML.
            Self::Null => String::new(),
        }
    }

    /// JS-style loose equality used by `==` / `!=`: `true == 1`,
    /// `"42" == 42`, etc. Matches legacy `lv == rv` semantics, with
    /// the addition that `null == null` (only) — `null != 0`,
    /// `null != ""`, `null != false`. The asymmetry is deliberate:
    /// authors writing `value == null` want to distinguish "key
    /// missing" from "key present but falsy."
    pub fn loose_eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Null, Self::Null) => true,
            (Self::Null, _) | (_, Self::Null) => false,
            (Self::Number(a), Self::Number(b)) => a == b,
            (Self::Boolean(a), Self::Boolean(b)) => a == b,
            (Self::String(a), Self::String(b)) => a == b,
            (Self::Number(n), Self::Boolean(b)) | (Self::Boolean(b), Self::Number(n)) => {
                *n == if *b { 1.0 } else { 0.0 }
            }
            (Self::Number(n), Self::String(s)) | (Self::String(s), Self::Number(n)) => {
                s.parse::<f64>().map(|v| v == *n).unwrap_or(false)
            }
            (Self::Boolean(b), Self::String(s)) | (Self::String(s), Self::Boolean(b)) => {
                let target = if *b { 1.0 } else { 0.0 };
                s.parse::<f64>().map(|v| v == target).unwrap_or(false)
            }
        }
    }
}

/// JS's `String(n)` produces "42" for 42 and "3.14" for 3.14 — no
/// trailing ".0". Match that so formula outputs stay stable.
pub fn format_number(n: f64) -> String {
    if n.is_nan() {
        "NaN".to_string()
    } else if n.is_infinite() {
        if n.is_sign_positive() {
            "Infinity".to_string()
        } else {
            "-Infinity".to_string()
        }
    } else if n == n.trunc() && n.is_finite() && n.abs() < 1e21 {
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExprError {
    pub message: String,
    pub offset: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Pow,
    Mod,
    Eq,
    Ne,
    Lt,
    Lte,
    Gt,
    Gte,
    And,
    Or,
}

impl BinaryOp {
    pub fn symbol(self) -> &'static str {
        match self {
            Self::Add => "+",
            Self::Sub => "-",
            Self::Mul => "*",
            Self::Div => "/",
            Self::Pow => "^",
            Self::Mod => "%",
            Self::Eq => "==",
            Self::Ne => "!=",
            Self::Lt => "<",
            Self::Lte => "<=",
            Self::Gt => ">",
            Self::Gte => ">=",
            Self::And => "and",
            Self::Or => "or",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UnaryOp {
    Neg,
    Not,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AnyExprNode {
    Literal {
        value: ExprValue,
        expr_type: ExprType,
    },
    Operand {
        operand_type: String,
        id: String,
        subfield: Option<String>,
    },
    Unary {
        op: UnaryOp,
        operand: Box<AnyExprNode>,
    },
    Binary {
        op: BinaryOp,
        left: Box<AnyExprNode>,
        right: Box<AnyExprNode>,
    },
    Call {
        name: String,
        args: Vec<AnyExprNode>,
    },
    /// `cond ? then : else_branch` — right-associative ternary.
    /// JS-truthy on `cond`. Added Wave 11.2 substrate to unblock the
    /// remaining shell migrations whose row variants needed
    /// `selected ? SELECTED_BG : kind == 'error' ? ERROR_BG : DEFAULT_BG`.
    Conditional {
        cond: Box<AnyExprNode>,
        then_branch: Box<AnyExprNode>,
        else_branch: Box<AnyExprNode>,
    },
}

impl AnyExprNode {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Literal { .. } => "literal",
            Self::Operand { .. } => "operand",
            Self::Unary { .. } => "unary",
            Self::Binary { .. } => "binary",
            Self::Call { .. } => "call",
            Self::Conditional { .. } => "conditional",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParseResult {
    pub node: Option<AnyExprNode>,
    pub errors: Vec<ExprError>,
}

/// Duck-typed variable store. Formula, lookup, and rollup fields
/// all read operand values through this trait.
pub trait ValueStore {
    fn resolve(&self, operand_type: &str, id: &str, subfield: Option<&str>) -> ExprValue;
}
