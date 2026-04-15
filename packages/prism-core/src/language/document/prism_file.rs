//! `PrismFile` — the single file/document abstraction that bridges
//! persistence, syntax, and rendering.
//!
//! Port of `language/document/prism-file.ts` from the pre-Rust
//! reference commit (ADR-002 §A1). Prior to `PrismFile` a "file" was
//! any of: raw string, `LoroText`, `GraphObject`, `BinaryRef`, or
//! `DocumentSchema`, each living in its own subsystem with no shared
//! contract. `PrismFile` wraps those bodies in a discriminated union
//! so that Surfaces, Syntax, Codegen, and Persistence can all agree
//! on "what a file is".
//!
//! Phase 1 of ADR-002 landed the TS version additively — nothing
//! consumed it yet. Phase 4 unifies the language + document
//! registries and wires the editor surface through `PrismFile`; the
//! [`LanguageRegistry::resolve_file`] helper in
//! `super::super::registry` is where that wiring happens.
//!
//! [`LanguageRegistry::resolve_file`]: crate::language::registry::LanguageRegistry::resolve_file

use std::collections::HashMap;

use serde_json::Value as JsonValue;

use crate::foundation::object_model::GraphObject;
use crate::foundation::vfs::BinaryRef;
use crate::language::forms::DocumentSchema;

// ── FileBody ───────────────────────────────────────────────────────

/// The payload of a [`PrismFile`]. Discriminated by variant so
/// surfaces and syntax can narrow on the body shape without reaching
/// into the CRDT / VFS / graph layers directly.
///
/// - `Text`   — plain string. Loro-backed text lives behind the
///   `crdt` feature in a future port; the current variant accepts
///   plain `String` so the document module stays framework-free.
/// - `Graph`  — a [`GraphObject`] rooted in the object model
///   (records, boards, spatial canvases). Editors project the graph
///   into whichever surface the user has open. Boxed so the enum
///   doesn't get dragged up to the largest variant.
/// - `Binary` — a [`BinaryRef`] pointing into the VFS (images,
///   audio, CAD).
#[derive(Debug, Clone, PartialEq)]
pub enum FileBody {
    Text(String),
    Graph(Box<GraphObject>),
    Binary(BinaryRef),
}

impl FileBody {
    /// `"text" | "graph" | "binary"` — the same kind strings the TS
    /// discriminator produced. Useful for logging and for matching
    /// the TS-era wire protocol until the postcard boundary ships.
    pub fn kind(&self) -> &'static str {
        match self {
            FileBody::Text(_) => "text",
            FileBody::Graph(_) => "graph",
            FileBody::Binary(_) => "binary",
        }
    }
}

// ── PrismFile ──────────────────────────────────────────────────────

/// A unified file/document record.
///
/// `language_id` resolves a
/// [`LanguageContribution`](crate::language::registry::LanguageContribution)
/// (parse / serialize / syntax provider / surface renderers).
/// `surface_id` is an explicit override for cases where a single
/// language supports multiple surfaces and the caller wants to pick
/// one up front; otherwise the surface is derived from `language_id`.
///
/// `schema` is carried alongside the body rather than stuffed inside
/// it so form-driven files (YAML/JSON with a known schema, Flux
/// records) can share the same file abstraction as free-form
/// documents. Now that [`language::forms`](crate::language::forms)
/// has landed the field is a strongly-typed [`DocumentSchema`] that
/// round-trips through serde.
#[derive(Debug, Clone, PartialEq)]
pub struct PrismFile {
    /// NSID or VFS path. The primary identity of the file.
    pub path: String,
    /// Language contribution id. Resolves parse/serialize/surface.
    pub language_id: Option<String>,
    /// Explicit surface override; defaults to the language's
    /// `default_mode`.
    pub surface_id: Option<String>,
    /// The actual content, discriminated by variant.
    pub body: FileBody,
    /// Optional form/field schema for structured editing.
    pub schema: Option<DocumentSchema>,
    /// Free-form metadata bag (owner, tags, custom keys).
    pub metadata: Option<HashMap<String, JsonValue>>,
}

// ── Narrowing helpers ──────────────────────────────────────────────

/// Returns `true` if the body is a [`FileBody::Text`].
pub fn is_text_body(body: &FileBody) -> bool {
    matches!(body, FileBody::Text(_))
}

/// Returns `true` if the body is a [`FileBody::Graph`].
pub fn is_graph_body(body: &FileBody) -> bool {
    matches!(body, FileBody::Graph(_))
}

/// Returns `true` if the body is a [`FileBody::Binary`].
pub fn is_binary_body(body: &FileBody) -> bool {
    matches!(body, FileBody::Binary(_))
}

// ── Constructors ───────────────────────────────────────────────────

/// Parameters for [`create_text_file`]. Mirrors the keyword-object
/// shape of the TS builder so porting call sites is mechanical.
#[derive(Debug, Clone, Default)]
pub struct TextFileParams {
    pub path: String,
    pub text: String,
    pub language_id: Option<String>,
    pub surface_id: Option<String>,
    pub schema: Option<DocumentSchema>,
    pub metadata: Option<HashMap<String, JsonValue>>,
}

/// Build a [`PrismFile`] with a text body.
pub fn create_text_file(params: TextFileParams) -> PrismFile {
    PrismFile {
        path: params.path,
        language_id: params.language_id,
        surface_id: params.surface_id,
        body: FileBody::Text(params.text),
        schema: params.schema,
        metadata: params.metadata,
    }
}

/// Parameters for [`create_graph_file`].
#[derive(Debug, Clone)]
pub struct GraphFileParams {
    pub path: String,
    pub object: GraphObject,
    pub language_id: Option<String>,
    pub surface_id: Option<String>,
    pub schema: Option<DocumentSchema>,
    pub metadata: Option<HashMap<String, JsonValue>>,
}

/// Build a [`PrismFile`] with a graph body.
pub fn create_graph_file(params: GraphFileParams) -> PrismFile {
    PrismFile {
        path: params.path,
        language_id: params.language_id,
        surface_id: params.surface_id,
        body: FileBody::Graph(Box::new(params.object)),
        schema: params.schema,
        metadata: params.metadata,
    }
}

/// Parameters for [`create_binary_file`].
#[derive(Debug, Clone)]
pub struct BinaryFileParams {
    pub path: String,
    pub binary: BinaryRef,
    pub language_id: Option<String>,
    pub surface_id: Option<String>,
    pub metadata: Option<HashMap<String, JsonValue>>,
}

/// Build a [`PrismFile`] with a binary body. Binary files never carry
/// a form schema — they're opaque blobs as far as the language layer
/// is concerned.
pub fn create_binary_file(params: BinaryFileParams) -> PrismFile {
    PrismFile {
        path: params.path,
        language_id: params.language_id,
        surface_id: params.surface_id,
        body: FileBody::Binary(params.binary),
        schema: None,
        metadata: params.metadata,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn sample_binary() -> BinaryRef {
        BinaryRef {
            hash: "deadbeef".into(),
            filename: "logo.png".into(),
            mime_type: "image/png".into(),
            size: 1024,
            imported_at: Utc::now(),
        }
    }

    #[test]
    fn text_file_carries_body_and_language_id() {
        let file = create_text_file(TextFileParams {
            path: "notes/today.luau".into(),
            text: "return 1".into(),
            language_id: Some("prism:luau".into()),
            ..Default::default()
        });
        assert_eq!(file.path, "notes/today.luau");
        assert_eq!(file.language_id.as_deref(), Some("prism:luau"));
        assert!(is_text_body(&file.body));
        assert_eq!(file.body.kind(), "text");
    }

    #[test]
    fn graph_file_narrows_via_helper() {
        let object = GraphObject::new("abc", "task", "Buy milk");
        let file = create_graph_file(GraphFileParams {
            path: "objects/task/abc".into(),
            object,
            language_id: None,
            surface_id: None,
            schema: None,
            metadata: None,
        });
        assert!(is_graph_body(&file.body));
        assert_eq!(file.body.kind(), "graph");
        if let FileBody::Graph(obj) = &file.body {
            assert_eq!(obj.id.as_str(), "abc");
        } else {
            panic!("expected graph body");
        }
    }

    #[test]
    fn binary_file_never_carries_schema() {
        let file = create_binary_file(BinaryFileParams {
            path: "assets/logo.png".into(),
            binary: sample_binary(),
            language_id: None,
            surface_id: None,
            metadata: None,
        });
        assert!(is_binary_body(&file.body));
        assert_eq!(file.body.kind(), "binary");
        assert!(file.schema.is_none());
    }

    #[test]
    fn narrowing_helpers_are_mutually_exclusive() {
        let text = create_text_file(TextFileParams {
            path: "a".into(),
            text: String::new(),
            ..Default::default()
        });
        assert!(is_text_body(&text.body));
        assert!(!is_graph_body(&text.body));
        assert!(!is_binary_body(&text.body));
    }

    #[test]
    fn surface_id_override_is_preserved() {
        let file = create_text_file(TextFileParams {
            path: "doc.md".into(),
            text: "# title".into(),
            language_id: Some("prism:markdown".into()),
            surface_id: Some("preview".into()),
            ..Default::default()
        });
        assert_eq!(file.surface_id.as_deref(), Some("preview"));
    }

    #[test]
    fn text_file_carries_typed_document_schema() {
        use crate::language::forms::{DocumentSchema, FieldSchema, FieldType};

        let schema = DocumentSchema {
            id: "contact".into(),
            name: "Contact".into(),
            fields: vec![FieldSchema::new("name", "Name", FieldType::Text)],
            sections: Vec::new(),
        };
        let file = create_text_file(TextFileParams {
            path: "forms/contact.json".into(),
            text: String::new(),
            schema: Some(schema.clone()),
            ..Default::default()
        });
        assert_eq!(file.schema.as_ref().map(|s| s.id.as_str()), Some("contact"));
        assert_eq!(file.schema, Some(schema));
    }
}
