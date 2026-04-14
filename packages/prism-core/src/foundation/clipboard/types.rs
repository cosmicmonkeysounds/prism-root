//! Clipboard data types — serializable subtrees + paste results.
//!
//! Port of `foundation/clipboard/clipboard-types.ts`.

use std::collections::HashMap;

use crate::foundation::object_model::types::{GraphObject, ObjectEdge};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipboardMode {
    Copy,
    Cut,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SerializedSubtree {
    pub root: GraphObject,
    pub descendants: Vec<GraphObject>,
    pub internal_edges: Vec<ObjectEdge>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClipboardEntry {
    pub mode: ClipboardMode,
    pub subtrees: Vec<SerializedSubtree>,
    pub source_ids: Vec<String>,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Default)]
pub struct PasteOptions {
    pub parent_id: Option<String>,
    pub position: Option<f64>,
}

#[derive(Debug, Clone, Default)]
pub struct PasteResult {
    pub created: Vec<GraphObject>,
    pub created_edges: Vec<ObjectEdge>,
    pub id_map: HashMap<String, String>,
}
