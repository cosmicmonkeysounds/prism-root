//! `SymbolDef` — the "describe once, emit many targets" codegen DSL.
//!
//! Port of `language/codegen/symbol-def.ts`. A `SymbolDef` describes
//! one exported symbol (function, constant, class, enum, namespace,
//! field). The same definition is fed to multiple emitters to
//! produce TypeScript (`.ts`), C# (`.cs`), Luau type stubs
//! (`.d.luau`), and GDScript (`.gd`) output from one source of
//! truth. Emitter hierarchy lives in [`super::symbol_emitter`].

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

/// Which kind of declaration a [`SymbolDef`] describes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SymbolKind {
    #[default]
    Constant,
    Function,
    Class,
    Enum,
    Namespace,
    Field,
}

/// One positional parameter on a function symbol.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SymbolParam {
    pub name: String,
    pub r#type: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub optional: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Default value for TS optional params. Stored as a JSON value
    /// so `true` / `42` / `"foo"` round-trip without losing type.
    #[serde(
        default,
        rename = "defaultValue",
        skip_serializing_if = "Option::is_none"
    )]
    pub default_value: Option<JsonValue>,
}

impl SymbolParam {
    pub fn new(name: impl Into<String>, type_: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            r#type: type_.into(),
            ..Self::default()
        }
    }

    pub fn optional(mut self, optional: bool) -> Self {
        self.optional = optional;
        self
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }
}

/// One variant on an enum-kind [`SymbolDef`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnumValue {
    pub name: String,
    /// String-literal (`"active"`) or numeric (`1`) value. Matches
    /// the TS `string | number` union.
    pub value: JsonValue,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// A single exported symbol. All the optional slots mirror the TS
/// discriminated layout: each `SymbolKind` populates a specific
/// subset — `constant` uses `value` + `type`, `function` uses
/// `params` / `returns` / `async_`, `enum` uses `enum_values` +
/// `enum_is_string`, `namespace` / `class` use `children`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SymbolDef {
    pub name: String,
    pub kind: SymbolKind,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    // ── Constant / field ───────────────────────────────────────────
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<JsonValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r#type: Option<String>,

    // ── Function ───────────────────────────────────────────────────
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<SymbolParam>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub returns: Option<String>,
    /// `async` is a reserved word in Rust; the TS name comes back via
    /// `#[serde(rename)]` on serialize/deserialize.
    #[serde(default, rename = "async", skip_serializing_if = "std::ops::Not::not")]
    pub async_: bool,

    // ── Enum ───────────────────────────────────────────────────────
    #[serde(default, rename = "enumValues", skip_serializing_if = "Vec::is_empty")]
    pub enum_values: Vec<EnumValue>,
    #[serde(
        default,
        rename = "enumIsString",
        skip_serializing_if = "Option::is_none"
    )]
    pub enum_is_string: Option<bool>,

    // ── Namespace / class children ─────────────────────────────────
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<SymbolDef>,

    // ── Metadata ───────────────────────────────────────────────────
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deprecated: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
}

impl SymbolDef {
    pub fn constant(name: impl Into<String>, type_: impl Into<String>, value: JsonValue) -> Self {
        Self {
            name: name.into(),
            kind: SymbolKind::Constant,
            r#type: Some(type_.into()),
            value: Some(value),
            ..Self::default()
        }
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }
}

/// Build a namespace of string constants from an iterable of
/// `(name, value)` pairs. Mirrors the TS `constantNamespace` helper
/// used to declare "ID banks" (every plugin uses it for conversation
/// ids, scene ids, asset handles, etc).
pub fn constant_namespace<I, K, V>(
    name: impl Into<String>,
    values: I,
    description: Option<String>,
) -> SymbolDef
where
    I: IntoIterator<Item = (K, V)>,
    K: Into<String>,
    V: Into<String>,
{
    let children = values
        .into_iter()
        .map(|(k, v)| SymbolDef {
            name: k.into(),
            kind: SymbolKind::Constant,
            r#type: Some("string".into()),
            value: Some(JsonValue::String(v.into())),
            ..SymbolDef::default()
        })
        .collect();
    SymbolDef {
        name: name.into(),
        kind: SymbolKind::Namespace,
        description,
        children,
        ..SymbolDef::default()
    }
}

/// Build a function [`SymbolDef`] — mirrors the TS `fnSymbol` helper.
pub fn fn_symbol(
    name: impl Into<String>,
    params: Vec<SymbolParam>,
    returns: impl Into<String>,
    description: Option<String>,
) -> SymbolDef {
    SymbolDef {
        name: name.into(),
        kind: SymbolKind::Function,
        description,
        params,
        returns: Some(returns.into()),
        ..SymbolDef::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn constant_namespace_builds_children() {
        let ns = constant_namespace(
            "CONVERSATIONS",
            [
                ("TAVERN_INTRO", "tavern_intro"),
                ("GUARD_PATROL", "guard_patrol"),
            ],
            Some("Compiled conversation IDs".into()),
        );
        assert_eq!(ns.kind, SymbolKind::Namespace);
        assert_eq!(ns.children.len(), 2);
        assert_eq!(ns.children[0].value, Some(json!("tavern_intro")));
        assert_eq!(ns.children[0].r#type.as_deref(), Some("string"));
    }

    #[test]
    fn fn_symbol_sets_return_and_params() {
        let f = fn_symbol(
            "Start",
            vec![
                SymbolParam::new("id", "string"),
                SymbolParam::new("actorId", "string").optional(true),
            ],
            "void",
            None,
        );
        assert_eq!(f.kind, SymbolKind::Function);
        assert_eq!(f.returns.as_deref(), Some("void"));
        assert_eq!(f.params.len(), 2);
        assert!(f.params[1].optional);
    }

    #[test]
    fn symbol_def_round_trips_through_serde() {
        let sym = SymbolDef {
            name: "Count".into(),
            kind: SymbolKind::Enum,
            enum_values: vec![
                EnumValue {
                    name: "One".into(),
                    value: json!(1),
                    description: None,
                },
                EnumValue {
                    name: "Two".into(),
                    value: json!(2),
                    description: Some("second".into()),
                },
            ],
            enum_is_string: Some(false),
            ..SymbolDef::default()
        };
        let json = serde_json::to_string(&sym).unwrap();
        assert!(json.contains("\"kind\":\"enum\""));
        assert!(json.contains("\"enumValues\""));
        assert!(json.contains("\"enumIsString\":false"));
        let back: SymbolDef = serde_json::from_str(&json).unwrap();
        assert_eq!(back, sym);
    }

    #[test]
    fn symbol_param_default_skips_empty_fields() {
        let p = SymbolParam::new("id", "string");
        let json = serde_json::to_string(&p).unwrap();
        assert!(!json.contains("optional"));
        assert!(!json.contains("description"));
        assert!(!json.contains("defaultValue"));
    }
}
