//! `ScriptGraph` — language-agnostic dataflow graph model.
//!
//! Nodes map to AST constructs (statements, expressions, declarations).
//! Edges carry either execution flow (statement ordering) or data flow
//! (expression wiring). Visual position metadata enables the graph to
//! be rendered as a node editor.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

// ── Node Kinds ────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScriptNodeKind {
    // Control flow
    Entry,
    Return,
    Branch,
    Loop,
    Match,

    // Declarations
    FunctionDef,
    LocalAssignment,
    Assignment,

    // Expressions
    Literal,
    Variable,
    BinaryOp,
    UnaryOp,
    FunctionCall,
    MethodCall,
    Index,
    TableConstructor,

    // Prism integration
    DaemonCommand,
    CrdtRead,
    CrdtWrite,
    EventListener,

    // Extensible
    Comment,
    Block,
    Custom(String),
}

impl ScriptNodeKind {
    pub fn is_expression(&self) -> bool {
        matches!(
            self,
            Self::Literal
                | Self::Variable
                | Self::BinaryOp
                | Self::UnaryOp
                | Self::FunctionCall
                | Self::MethodCall
                | Self::Index
                | Self::TableConstructor
                | Self::DaemonCommand
                | Self::CrdtRead
        )
    }

    pub fn is_statement(&self) -> bool {
        !self.is_expression() || matches!(self, Self::FunctionCall | Self::MethodCall)
    }
}

// ── Ports ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PortKind {
    Execution,
    Data,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PortDirection {
    Input,
    Output,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DataType {
    Number,
    String,
    Boolean,
    Table,
    Function,
    Nil,
    Any,
    Custom(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PortDef {
    pub id: String,
    pub label: String,
    pub kind: PortKind,
    pub direction: PortDirection,
    pub data_type: DataType,
}

// ── Nodes ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScriptNode {
    pub id: String,
    pub kind: ScriptNodeKind,
    pub label: String,
    pub position: [f64; 2],
    pub ports: Vec<PortDef>,
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub properties: IndexMap<String, JsonValue>,
}

impl ScriptNode {
    pub fn new(id: impl Into<String>, kind: ScriptNodeKind, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            kind,
            label: label.into(),
            position: [0.0, 0.0],
            ports: Vec::new(),
            properties: IndexMap::new(),
        }
    }

    pub fn with_position(mut self, x: f64, y: f64) -> Self {
        self.position = [x, y];
        self
    }

    pub fn with_port(mut self, port: PortDef) -> Self {
        self.ports.push(port);
        self
    }

    pub fn with_property(mut self, key: impl Into<String>, value: JsonValue) -> Self {
        self.properties.insert(key.into(), value);
        self
    }

    pub fn input_ports(&self) -> impl Iterator<Item = &PortDef> {
        self.ports
            .iter()
            .filter(|p| p.direction == PortDirection::Input)
    }

    pub fn output_ports(&self) -> impl Iterator<Item = &PortDef> {
        self.ports
            .iter()
            .filter(|p| p.direction == PortDirection::Output)
    }

    pub fn exec_in(&self) -> Option<&PortDef> {
        self.ports
            .iter()
            .find(|p| p.direction == PortDirection::Input && p.kind == PortKind::Execution)
    }

    pub fn exec_out(&self) -> Option<&PortDef> {
        self.ports
            .iter()
            .find(|p| p.direction == PortDirection::Output && p.kind == PortKind::Execution)
    }
}

// ── Edges ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ScriptEdge {
    pub id: String,
    pub from_node: String,
    pub from_port: String,
    pub to_node: String,
    pub to_port: String,
}

// ── Graph ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ScriptGraph {
    pub id: String,
    pub name: String,
    pub nodes: Vec<ScriptNode>,
    pub edges: Vec<ScriptEdge>,
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub metadata: IndexMap<String, JsonValue>,
}

impl ScriptGraph {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            nodes: Vec::new(),
            edges: Vec::new(),
            metadata: IndexMap::new(),
        }
    }

    pub fn add_node(&mut self, node: ScriptNode) -> &mut Self {
        self.nodes.push(node);
        self
    }

    pub fn add_edge(&mut self, edge: ScriptEdge) -> &mut Self {
        self.edges.push(edge);
        self
    }

    pub fn get_node(&self, id: &str) -> Option<&ScriptNode> {
        self.nodes.iter().find(|n| n.id == id)
    }

    pub fn get_node_mut(&mut self, id: &str) -> Option<&mut ScriptNode> {
        self.nodes.iter_mut().find(|n| n.id == id)
    }

    pub fn remove_node(&mut self, id: &str) -> Option<ScriptNode> {
        let idx = self.nodes.iter().position(|n| n.id == id)?;
        let node = self.nodes.remove(idx);
        self.edges.retain(|e| e.from_node != id && e.to_node != id);
        Some(node)
    }

    pub fn edges_from(&self, node_id: &str) -> Vec<&ScriptEdge> {
        self.edges
            .iter()
            .filter(|e| e.from_node == node_id)
            .collect()
    }

    pub fn edges_to(&self, node_id: &str) -> Vec<&ScriptEdge> {
        self.edges.iter().filter(|e| e.to_node == node_id).collect()
    }

    pub fn topological_sort(&self) -> Vec<&str> {
        let mut in_degree: IndexMap<&str, usize> = IndexMap::new();
        for node in &self.nodes {
            in_degree.entry(node.id.as_str()).or_insert(0);
        }
        for edge in &self.edges {
            *in_degree.entry(edge.to_node.as_str()).or_insert(0) += 1;
        }

        let mut queue: Vec<&str> = in_degree
            .iter()
            .filter(|(_, &deg)| deg == 0)
            .map(|(&id, _)| id)
            .collect();
        let mut sorted = Vec::new();

        while let Some(id) = queue.pop() {
            sorted.push(id);
            for edge in self.edges_from(id) {
                if let Some(deg) = in_degree.get_mut(edge.to_node.as_str()) {
                    *deg = deg.saturating_sub(1);
                    if *deg == 0 {
                        queue.push(edge.to_node.as_str());
                    }
                }
            }
        }

        for node in &self.nodes {
            if !sorted.contains(&node.id.as_str()) {
                sorted.push(node.id.as_str());
            }
        }

        sorted
    }

    pub fn validate(&self) -> Vec<String> {
        let mut errors = Vec::new();

        let mut seen_ids = std::collections::HashSet::new();
        for node in &self.nodes {
            if !seen_ids.insert(&node.id) {
                errors.push(format!("duplicate node id: {}", node.id));
            }
        }

        for edge in &self.edges {
            if self.get_node(&edge.from_node).is_none() {
                errors.push(format!(
                    "edge {} references missing source node: {}",
                    edge.id, edge.from_node
                ));
            }
            if self.get_node(&edge.to_node).is_none() {
                errors.push(format!(
                    "edge {} references missing target node: {}",
                    edge.id, edge.to_node
                ));
            }
        }

        errors
    }
}

// ── Palette Definition ────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeKindDef {
    pub kind: ScriptNodeKind,
    pub label: String,
    pub description: String,
    pub category: String,
    pub default_ports: Vec<PortDef>,
}

// ── Helpers ───────────────────────────────────────────────────────────

pub fn exec_input(id: impl Into<String>) -> PortDef {
    PortDef {
        id: id.into(),
        label: "exec_in".into(),
        kind: PortKind::Execution,
        direction: PortDirection::Input,
        data_type: DataType::Any,
    }
}

pub fn exec_output(id: impl Into<String>) -> PortDef {
    PortDef {
        id: id.into(),
        label: "exec_out".into(),
        kind: PortKind::Execution,
        direction: PortDirection::Output,
        data_type: DataType::Any,
    }
}

pub fn data_input(id: impl Into<String>, label: impl Into<String>, dt: DataType) -> PortDef {
    PortDef {
        id: id.into(),
        label: label.into(),
        kind: PortKind::Data,
        direction: PortDirection::Input,
        data_type: dt,
    }
}

pub fn data_output(id: impl Into<String>, label: impl Into<String>, dt: DataType) -> PortDef {
    PortDef {
        id: id.into(),
        label: label.into(),
        kind: PortKind::Data,
        direction: PortDirection::Output,
        data_type: dt,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_graph() -> ScriptGraph {
        let mut g = ScriptGraph::new("test", "Test Graph");
        g.add_node(
            ScriptNode::new("entry", ScriptNodeKind::Entry, "Start")
                .with_port(exec_output("exec_out")),
        );
        g.add_node(
            ScriptNode::new("assign", ScriptNodeKind::LocalAssignment, "local x = 1")
                .with_port(exec_input("exec_in"))
                .with_port(exec_output("exec_out"))
                .with_port(data_output("value", "x", DataType::Number)),
        );
        g.add_node(
            ScriptNode::new("ret", ScriptNodeKind::Return, "return x")
                .with_port(exec_input("exec_in"))
                .with_port(data_input("value", "value", DataType::Any)),
        );
        g.add_edge(ScriptEdge {
            id: "e1".into(),
            from_node: "entry".into(),
            from_port: "exec_out".into(),
            to_node: "assign".into(),
            to_port: "exec_in".into(),
        });
        g.add_edge(ScriptEdge {
            id: "e2".into(),
            from_node: "assign".into(),
            from_port: "exec_out".into(),
            to_node: "ret".into(),
            to_port: "exec_in".into(),
        });
        g.add_edge(ScriptEdge {
            id: "e3".into(),
            from_node: "assign".into(),
            from_port: "value".into(),
            to_node: "ret".into(),
            to_port: "value".into(),
        });
        g
    }

    #[test]
    fn graph_topological_sort() {
        let g = sample_graph();
        let sorted = g.topological_sort();
        let entry_pos = sorted.iter().position(|&id| id == "entry").unwrap();
        let assign_pos = sorted.iter().position(|&id| id == "assign").unwrap();
        let ret_pos = sorted.iter().position(|&id| id == "ret").unwrap();
        assert!(entry_pos < assign_pos);
        assert!(assign_pos < ret_pos);
    }

    #[test]
    fn graph_validates_clean() {
        let g = sample_graph();
        assert!(g.validate().is_empty());
    }

    #[test]
    fn graph_validate_catches_missing_nodes() {
        let mut g = ScriptGraph::new("bad", "Bad");
        g.add_edge(ScriptEdge {
            id: "e1".into(),
            from_node: "ghost".into(),
            from_port: "out".into(),
            to_node: "phantom".into(),
            to_port: "in".into(),
        });
        let errors = g.validate();
        assert_eq!(errors.len(), 2);
    }

    #[test]
    fn remove_node_cleans_edges() {
        let mut g = sample_graph();
        g.remove_node("assign");
        assert!(g.get_node("assign").is_none());
        assert!(g
            .edges
            .iter()
            .all(|e| e.from_node != "assign" && e.to_node != "assign"));
    }

    #[test]
    fn node_port_queries() {
        let g = sample_graph();
        let assign = g.get_node("assign").unwrap();
        assert!(assign.exec_in().is_some());
        assert!(assign.exec_out().is_some());
        assert_eq!(assign.output_ports().count(), 2);
        assert_eq!(assign.input_ports().count(), 1);
    }

    #[test]
    fn graph_serializes_roundtrip() {
        let g = sample_graph();
        let json = serde_json::to_string(&g).unwrap();
        let g2: ScriptGraph = serde_json::from_str(&json).unwrap();
        assert_eq!(g, g2);
    }

    #[test]
    fn node_kind_classification() {
        assert!(ScriptNodeKind::Literal.is_expression());
        assert!(!ScriptNodeKind::Entry.is_expression());
        assert!(ScriptNodeKind::FunctionCall.is_expression());
        assert!(ScriptNodeKind::FunctionCall.is_statement());
    }
}
