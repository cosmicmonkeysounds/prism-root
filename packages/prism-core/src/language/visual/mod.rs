//! `language/visual` — generalized bidirectional visual↔textual
//! script bridge.
//!
//! Provides a language-agnostic dataflow graph model (`ScriptGraph`)
//! and a `VisualLanguage` trait that concrete languages implement to
//! enable bidirectional editing: users can work in a text editor OR
//! a visual node-graph editor and the two stay in sync.
//!
//! The graph is the canonical intermediate representation. Each node
//! maps to an AST construct; edges carry either execution flow
//! (statement ordering) or data flow (expression wiring). Concrete
//! languages provide `compile` (graph → source) and `decompile`
//! (source → graph) implementations.
//!
//! Designed for reuse across Luau, Loom Lang, and any future Prism
//! scripting language.

pub mod bridge;
pub mod graph;

pub use bridge::{VisualLanguage, VisualLanguageError};
pub use graph::{
    DataType, NodeKindDef, PortDef, PortDirection, PortKind, ScriptEdge, ScriptGraph, ScriptNode,
    ScriptNodeKind,
};
