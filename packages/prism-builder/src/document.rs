//! The builder document — a tree of typed nodes that round-trips
//! through serde. This is what gets written to disk and read back
//! by the next Studio boot.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub type NodeId = String;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub id: NodeId,
    pub component: String,
    #[serde(default)]
    pub props: Value,
    #[serde(default)]
    pub children: Vec<Node>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BuilderDocument {
    pub root: Option<Node>,
    #[serde(default)]
    pub zones: IndexMap<String, Vec<Node>>,
}
