//! `template` — reusable `GraphObject` subtree blueprints.
//!
//! Port of `foundation/template/*`. Templates capture a shape
//! (types, names with `{{variables}}`, nested children, optional
//! edges) that can be stamped into a tree at will. The registry
//! borrows the target tree / edges / undo manager on each call
//! rather than storing references, mirroring `TreeClipboard`.

pub mod registry;
pub mod types;

pub use registry::{TemplateError, TemplateIdGen, TemplateRegistry};
pub use types::{
    CreateFromObjectMeta, InstantiateOptions, InstantiateResult, ObjectTemplate, TemplateEdge,
    TemplateFilter, TemplateNode, TemplateVariable,
};
