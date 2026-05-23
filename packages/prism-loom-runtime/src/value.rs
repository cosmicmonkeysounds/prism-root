//! Runtime [`Value`] type — the universe of things `$` lookups,
//! expression evaluation, and ledger queries can land on.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

/// Externally-tagged for serde compatibility with non-self-
/// describing formats (postcard, bincode). Internally-tagged
/// (`#[serde(tag = ...)]`) doesn't compose with enums that carry
/// primitive newtype variants, and postcard can't serialise them at
/// all.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Value {
    #[default]
    Nil,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    List(Vec<Value>),
    /// `IndexMap` preserves insertion order, which matters for
    /// participant rosters / faction membership / disposition axes
    /// that authors rely on iterating in declaration order.
    Map(IndexMap<String, Value>),
}

impl Value {
    /// Truthiness rule — matches the §10 expression grammar's
    /// implicit-bool semantics: `false`, `nil`, `0`, `0.0`, empty
    /// string / list / map are falsey; everything else is truthy.
    pub fn is_truthy(&self) -> bool {
        match self {
            Value::Nil => false,
            Value::Bool(b) => *b,
            Value::Int(n) => *n != 0,
            Value::Float(f) => *f != 0.0 && !f.is_nan(),
            Value::Str(s) => !s.is_empty(),
            Value::List(xs) => !xs.is_empty(),
            Value::Map(m) => !m.is_empty(),
        }
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Nil => "nil",
            Value::Bool(_) => "bool",
            Value::Int(_) => "int",
            Value::Float(_) => "float",
            Value::Str(_) => "string",
            Value::List(_) => "list",
            Value::Map(_) => "map",
        }
    }
}

impl From<bool> for Value {
    fn from(b: bool) -> Self {
        Value::Bool(b)
    }
}

impl From<i64> for Value {
    fn from(n: i64) -> Self {
        Value::Int(n)
    }
}

impl From<i32> for Value {
    fn from(n: i32) -> Self {
        Value::Int(n as i64)
    }
}

impl From<f64> for Value {
    fn from(f: f64) -> Self {
        Value::Float(f)
    }
}

impl From<&str> for Value {
    fn from(s: &str) -> Self {
        Value::Str(s.to_string())
    }
}

impl From<String> for Value {
    fn from(s: String) -> Self {
        Value::Str(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truthiness_matches_grammar() {
        assert!(!Value::Nil.is_truthy());
        assert!(!Value::Bool(false).is_truthy());
        assert!(Value::Bool(true).is_truthy());
        assert!(!Value::Int(0).is_truthy());
        assert!(Value::Int(7).is_truthy());
        assert!(!Value::Float(0.0).is_truthy());
        assert!(Value::Float(0.5).is_truthy());
        assert!(!Value::Str(String::new()).is_truthy());
        assert!(Value::Str("hi".into()).is_truthy());
        assert!(!Value::List(vec![]).is_truthy());
        assert!(Value::List(vec![Value::Int(1)]).is_truthy());
    }

    #[test]
    fn nan_is_falsey() {
        assert!(!Value::Float(f64::NAN).is_truthy());
    }

    #[test]
    fn serde_round_trip() {
        let v = Value::Map(
            [("name".to_string(), Value::Str("Wren".into()))]
                .into_iter()
                .collect(),
        );
        let s = serde_json::to_string(&v).unwrap();
        let back: Value = serde_json::from_str(&s).unwrap();
        assert_eq!(v, back);
    }
}
