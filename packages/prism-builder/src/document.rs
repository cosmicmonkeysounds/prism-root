//! The builder document — a tree of typed nodes that round-trips
//! through serde. This is what gets written to disk and read back
//! by the next Studio boot.

use indexmap::IndexMap;
use prism_core::foundation::spatial::Transform2D;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::facet::{FacetDef, FacetSchema, FacetSchemaId};
use crate::layout::{LayoutMode, PageLayout};
use crate::modifier::Modifier;
use crate::prefab::PrefabDef;
use crate::resource::{ResourceDef, ResourceId};
use crate::signal::Connection;
use crate::style::StyleProperties;

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
    #[serde(default)]
    pub style: StyleProperties,
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

    pub fn find_mut(&mut self, target: &str) -> Option<&mut Node> {
        if self.id == target {
            return Some(self);
        }
        for child in &mut self.children {
            if let Some(hit) = child.find_mut(target) {
                return Some(hit);
            }
        }
        None
    }
}

impl BuilderDocument {
    /// Place a node at a specific grid cell by updating its FlowProps.
    pub fn place_in_grid(&mut self, node_id: &str, path: &[usize]) -> bool {
        self.page_layout
            .place_node_at(path, node_id.to_string())
            .is_ok()
    }

    /// Create a new page document with a basic shell layout.
    ///
    /// Returns a document with a single-column responsive grid (header +
    /// content rows), a root container, and sensible margins. This is the
    /// bare minimum starting point for every new page — users add
    /// components by dragging into the grid cells.
    pub fn page_shell() -> Self {
        use crate::layout::{FlowDisplay, FlowProps};
        use prism_core::foundation::geometry::Edges;

        Self {
            root: Some(Node {
                id: "root".into(),
                component: "container".into(),
                props: serde_json::json!({ "spacing": 16 }),
                layout_mode: LayoutMode::Flow(FlowProps {
                    display: FlowDisplay::Flex,
                    flex_direction: crate::layout::FlexDirection::Column,
                    gap: 16.0,
                    ..Default::default()
                }),
                children: vec![],
                ..Default::default()
            }),
            page_layout: PageLayout {
                size: crate::layout::PageSize::Responsive,
                margins: Edges::new(24.0, 32.0, 24.0, 32.0),
                grid: Some(crate::layout::GridCell::leaf()),
                column_gap: 16.0,
                row_gap: 16.0,
                ..Default::default()
            },
            ..Default::default()
        }
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
            style: StyleProperties::default(),
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
    #[serde(default)]
    pub facet_schemas: IndexMap<FacetSchemaId, FacetSchema>,
    #[serde(default)]
    pub facets: IndexMap<String, FacetDef>,
}
