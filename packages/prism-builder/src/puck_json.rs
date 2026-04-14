//! One-way reader for legacy Puck documents.
//!
//! Puck writes JSON in the shape `{ root: { props }, content: [ { type,
//! props, ... } ], zones: { [zoneId]: [...] } }`. We keep reading that
//! format so existing user content boots; new content is written in the
//! [`crate::document::BuilderDocument`] schema.
//!
//! This module only *parses*. No rendering, no registry interaction —
//! a pure format adapter.

use serde::Deserialize;
use serde_json::Value;

use crate::document::{BuilderDocument, Node};

#[derive(Debug, Deserialize)]
struct PuckRoot {
    #[serde(default)]
    props: Value,
}

#[derive(Debug, Deserialize)]
struct PuckNode {
    #[serde(rename = "type")]
    ty: String,
    #[serde(default)]
    props: Value,
    #[serde(default)]
    content: Vec<PuckNode>,
}

#[derive(Debug, Deserialize)]
struct PuckDoc {
    #[serde(default)]
    root: Option<PuckRoot>,
    #[serde(default)]
    content: Vec<PuckNode>,
    #[serde(default)]
    zones: indexmap::IndexMap<String, Vec<PuckNode>>,
}

pub fn parse(raw: &str) -> Result<BuilderDocument, serde_json::Error> {
    let doc: PuckDoc = serde_json::from_str(raw)?;

    let root_node = doc.root.map(|root| Node {
        id: "root".to_string(),
        component: "puck.root".to_string(),
        props: root.props,
        children: doc.content.into_iter().map(to_node).collect(),
    });

    let zones = doc
        .zones
        .into_iter()
        .map(|(k, v)| (k, v.into_iter().map(to_node).collect()))
        .collect();

    Ok(BuilderDocument {
        root: root_node,
        zones,
    })
}

fn to_node(node: PuckNode) -> Node {
    let id = node
        .props
        .as_object()
        .and_then(|o| o.get("id"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("{}-{}", node.ty, fastrand_u32()));

    Node {
        id,
        component: node.ty,
        props: node.props,
        children: node.content.into_iter().map(to_node).collect(),
    }
}

fn fastrand_u32() -> u32 {
    use std::sync::atomic::{AtomicU32, Ordering};
    static COUNTER: AtomicU32 = AtomicU32::new(1);
    COUNTER.fetch_add(1, Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_empty_doc() {
        let doc = parse(r#"{"root":{"props":{}},"content":[],"zones":{}}"#).unwrap();
        assert!(doc.root.is_some());
    }

    #[test]
    fn parses_simple_tree() {
        let raw = r#"{
          "root": { "props": { "title": "Home" } },
          "content": [
            { "type": "Heading", "props": { "id": "h1", "text": "Hello" } },
            { "type": "Card", "props": { "id": "c1" }, "content": [
              { "type": "Text", "props": { "id": "t1", "body": "Body" } }
            ]}
          ],
          "zones": {}
        }"#;
        let doc = parse(raw).unwrap();
        let root = doc.root.unwrap();
        assert_eq!(root.children.len(), 2);
        assert_eq!(root.children[1].children[0].component, "Text");
    }
}
