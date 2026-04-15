//! Codegen pipeline types — unified shapes for every emitter.
//!
//! Port of `language/codegen/codegen-types.ts`. Introduced by
//! ADR-002 §A3: the `input_kind` discriminator lets
//! [`CodegenPipeline`](super::pipeline::CodegenPipeline) accept
//! heterogeneous emitters (symbol tables, schema models, AST trees,
//! facet configs, raw data blobs, plugin-custom shapes) and route
//! each one by its declared input kind. Before the unification
//! there were two parallel emitter hierarchies —
//! `syntax/codegen/` (symbols) and `facet/emitters.ts` (schemas) —
//! that could not compose.
//!
//! Input kinds are open strings: the core ships with a handful of
//! well-known constants ([`EMITTER_KIND_SYMBOLS`] / [`EMITTER_KIND_SCHEMA`]
//! / …) but plugins may introduce their own without forking the
//! pipeline.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

/// Well-known `input_kind` for emitters that consume `Vec<SymbolDef>`.
pub const EMITTER_KIND_SYMBOLS: &str = "symbols";
/// Well-known `input_kind` for schema-model emitters (facet writers).
pub const EMITTER_KIND_SCHEMA: &str = "schema";
/// Well-known `input_kind` for arbitrary data-file emitters (json/yaml/toml).
pub const EMITTER_KIND_DATA: &str = "data";
/// Well-known `input_kind` for AST round-trip emitters.
pub const EMITTER_KIND_AST: &str = "ast";
/// Well-known `input_kind` for facet-builder configs.
pub const EMITTER_KIND_FACET: &str = "facet";

/// One file produced by an emitter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmittedFile {
    pub filename: String,
    pub content: String,
    /// Target language string — typically `"typescript"`, `"csharp"`,
    /// `"gdscript"`, `"luau"`, `"json"`, etc. Open set.
    pub language: String,
}

/// Per-run metadata carried alongside the input bundle.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodegenMeta {
    #[serde(rename = "projectName")]
    pub project_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Open extra slot for plugin-custom metadata. Mirrors the TS
    /// `[key: string]: unknown` index signature.
    #[serde(default, flatten)]
    pub extra: IndexMap<String, JsonValue>,
}

impl CodegenMeta {
    pub fn new(project_name: impl Into<String>) -> Self {
        Self {
            project_name: project_name.into(),
            version: None,
            extra: IndexMap::new(),
        }
    }

    pub fn with_version(mut self, version: impl Into<String>) -> Self {
        self.version = Some(version.into());
        self
    }
}

/// What an [`Emitter::emit`] returns: a bundle of generated files
/// plus any accumulated error strings.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodegenResult {
    #[serde(default)]
    pub files: Vec<EmittedFile>,
    #[serde(default)]
    pub errors: Vec<String>,
}

impl CodegenResult {
    pub fn new() -> Self {
        Self::default()
    }
}

/// A heterogeneous bundle of inputs keyed by the same discriminator
/// used on [`Emitter::input_kind`]. Well-known kinds share slots with
/// their constants above; plugins add their own keys directly.
#[derive(Debug, Default)]
pub struct CodegenInputs {
    slots: IndexMap<String, Box<dyn std::any::Any>>,
}

impl CodegenInputs {
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a value under `kind`. Replaces any previous value at
    /// that kind.
    pub fn insert<T: 'static>(&mut self, kind: impl Into<String>, value: T) -> &mut Self {
        self.slots.insert(kind.into(), Box::new(value));
        self
    }

    /// Chainable form of [`insert`](Self::insert).
    pub fn with<T: 'static>(mut self, kind: impl Into<String>, value: T) -> Self {
        self.insert(kind, value);
        self
    }

    /// Fetch a typed slot. Returns `None` if the slot is missing or
    /// the stored value's concrete type does not match `T`.
    pub fn get<T: 'static>(&self, kind: &str) -> Option<&T> {
        self.slots.get(kind).and_then(|b| b.downcast_ref::<T>())
    }

    /// Return the raw `&dyn Any` stored at `kind`, if any. Used by
    /// [`CodegenPipeline`](super::pipeline::CodegenPipeline) to
    /// dispatch emitters without knowing their concrete input type.
    pub fn any(&self, kind: &str) -> Option<&dyn std::any::Any> {
        self.slots
            .get(kind)
            .map(|b| b.as_ref() as &dyn std::any::Any)
    }

    /// Is there *any* value (typed or not) stored at `kind`?
    pub fn has(&self, kind: &str) -> bool {
        self.slots.contains_key(kind)
    }

    /// Iterator over every registered slot kind.
    pub fn kinds(&self) -> impl Iterator<Item = &str> {
        self.slots.keys().map(String::as_str)
    }
}

/// Trait every codegen emitter implements. Objects are held behind
/// `Box<dyn Emitter>` in [`CodegenPipeline`](super::pipeline::CodegenPipeline),
/// so the input is passed as `&dyn Any` and each emitter downcasts
/// to the concrete type it expects.
pub trait Emitter {
    /// Stable emitter id (used to prefix error messages).
    fn id(&self) -> &str;

    /// The `CodegenInputs` slot this emitter pulls from. One of the
    /// [`EMITTER_KIND_*`](EMITTER_KIND_SYMBOLS) constants for the
    /// well-known kinds, or a custom string for plugin emitters.
    fn input_kind(&self) -> &str;

    /// Run the emitter against the downcast input. Implementations
    /// should return a `CodegenResult` with at least one file on
    /// success or a populated `errors` vec on failure.
    fn emit(&self, input: &dyn std::any::Any, meta: &CodegenMeta) -> CodegenResult;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codegen_meta_round_trips_through_serde() {
        let meta = CodegenMeta::new("prism").with_version("0.1.0");
        let json = serde_json::to_string(&meta).unwrap();
        assert!(json.contains("\"projectName\":\"prism\""));
        assert!(json.contains("\"version\":\"0.1.0\""));
        let back: CodegenMeta = serde_json::from_str(&json).unwrap();
        assert_eq!(back, meta);
    }

    #[test]
    fn emitted_file_preserves_fields() {
        let f = EmittedFile {
            filename: "a.ts".into(),
            content: "export {}".into(),
            language: "typescript".into(),
        };
        let back: EmittedFile = serde_json::from_str(&serde_json::to_string(&f).unwrap()).unwrap();
        assert_eq!(back, f);
    }

    #[test]
    fn codegen_inputs_stores_and_retrieves_typed_slots() {
        let mut inputs = CodegenInputs::new();
        inputs.insert("symbols", vec![1u32, 2, 3]);
        inputs.insert("schema", String::from("hello"));
        assert!(inputs.has("symbols"));
        assert!(inputs.has("schema"));
        assert!(!inputs.has("data"));
        assert_eq!(inputs.get::<Vec<u32>>("symbols"), Some(&vec![1, 2, 3]));
        assert_eq!(inputs.get::<String>("schema"), Some(&String::from("hello")));
        // Wrong type returns None.
        assert_eq!(inputs.get::<String>("symbols"), None);
    }

    #[test]
    fn codegen_inputs_builder_chains() {
        let inputs = CodegenInputs::new()
            .with("a", 1u32)
            .with("b", 2u32)
            .with("c", 3u32);
        let kinds: Vec<&str> = inputs.kinds().collect();
        assert_eq!(kinds, vec!["a", "b", "c"]);
    }
}
