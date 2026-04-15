//! `language/document` — the unified file/document abstraction.
//!
//! Port of `packages/prism-core/src/language/document/*.ts` from the
//! pre-Rust reference commit (8426588, ADR-002 §A1). A [`PrismFile`]
//! wraps whichever body kind a file happens to have — raw text, a
//! graph object, or a VFS binary reference — behind one discriminated
//! union so Surfaces, Syntax, Codegen, and Persistence can all agree
//! on "what a file is".
//!
//! The `schema` field is typed as
//! [`crate::language::forms::DocumentSchema`] now that `language::forms`
//! has landed.

pub mod prism_file;

pub use prism_file::{
    create_binary_file, create_graph_file, create_text_file, is_binary_body, is_graph_body,
    is_text_body, BinaryFileParams, FileBody, GraphFileParams, PrismFile, TextFileParams,
};
