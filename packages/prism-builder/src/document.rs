//! The builder document — a tree of typed nodes that round-trips
//! through serde. This is what gets written to disk and read back
//! by the next Studio boot.

use indexmap::IndexMap;
use prism_core::foundation::spatial::Transform2D;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::layout::{LayoutMode, PageLayout};

pub type NodeId = String;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub id: NodeId,
    pub component: String,
    #[serde(default)]
    pub props: Value,
    #[serde(default)]
    pub children: Vec<Node>,
    #[serde(default)]
    pub layout_mode: LayoutMode,
    #[serde(default)]
    pub transform: Transform2D,
}

impl Default for Node {
    fn default() -> Self {
        Self {
            id: String::new(),
            component: String::new(),
            props: Value::Null,
            children: Vec::new(),
            layout_mode: LayoutMode::default(),
            transform: Transform2D::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BuilderDocument {
    pub root: Option<Node>,
    #[serde(default)]
    pub zones: IndexMap<String, Vec<Node>>,
    #[serde(default)]
    pub page_layout: PageLayout,
}
