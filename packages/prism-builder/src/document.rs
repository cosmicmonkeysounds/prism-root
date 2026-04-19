//! The builder document — a tree of typed nodes that round-trips
//! through serde. This is what gets written to disk and read back
//! by the next Studio boot.

use indexmap::IndexMap;
use prism_core::foundation::spatial::Transform2D;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::layout::{LayoutMode, PageLayout};
use crate::modifier::Modifier;
use crate::prefab::PrefabDef;
use crate::resource::{ResourceDef, ResourceId};
use crate::signal::Connection;

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
    #[serde(default)]
    pub modifiers: Vec<Modifier>,
}

impl Node {
    pub fn find(&self, target: &str) -> Option<&Node> {
        if self.id == target {
            return Some(self);
        }
        for child in &self.children {
            if let Some(hit) = child.find(target) {
                return Some(hit);
            }
        }
        None
    }
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
            modifiers: Vec::new(),
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
    #[serde(default)]
    pub resources: IndexMap<ResourceId, ResourceDef>,
    #[serde(default)]
    pub connections: Vec<Connection>,
    #[serde(default)]
    pub prefabs: IndexMap<String, PrefabDef>,
}
