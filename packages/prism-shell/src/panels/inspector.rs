//! Inspector panel — flat indented dump of the active document's
//! node tree, one node per line. The Slint side paints this into a
//! monospaced `Text` block. Non-interactive today; selection wiring
//! lands when the Properties panel needs to drive it.

use prism_builder::{BuilderDocument, Node};
use prism_core::help::HelpEntry;

use super::Panel;

pub struct InspectorPanel;

impl InspectorPanel {
    pub const ID: i32 = 2;
    pub fn new() -> Self {
        Self
    }

    /// Render `doc.root` as an indented textual tree. Each line is
    /// `<indent><node-id> · <component-id>` so the viewer can spot
    /// both the slotmap id and the component type at a glance.
    pub fn tree(doc: &BuilderDocument) -> String {
        let mut out = String::new();
        if let Some(root) = &doc.root {
            walk(root, 0, &mut out);
        } else {
            out.push_str("(empty document)");
        }
        out
    }
}

fn walk(node: &Node, depth: usize, out: &mut String) {
    for _ in 0..depth {
        out.push_str("  ");
    }
    out.push_str(&node.id);
    out.push_str(" · ");
    out.push_str(&node.component);
    out.push('\n');
    for child in &node.children {
        walk(child, depth + 1, out);
    }
}

impl Default for InspectorPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl Panel for InspectorPanel {
    fn id(&self) -> i32 {
        Self::ID
    }
    fn label(&self) -> &'static str {
        "Inspector"
    }
    fn title(&self) -> &'static str {
        "Inspector"
    }
    fn hint(&self) -> &'static str {
        "Flat indented dump of the active document's node tree."
    }
    fn help_entry(&self) -> Option<HelpEntry> {
        Some(HelpEntry::new(
            "shell.panels.inspector",
            "Inspector",
            "Document tree inspector. Shows the component hierarchy with drag-to-reorder and selection.",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prism_builder::Node;
    use serde_json::json;

    #[test]
    fn empty_document_renders_placeholder() {
        assert_eq!(
            InspectorPanel::tree(&BuilderDocument::default()),
            "(empty document)"
        );
    }

    #[test]
    fn tree_walks_children_with_indent() {
        let doc = BuilderDocument {
            root: Some(Node {
                id: "root".into(),
                component: "container".into(),
                props: json!({}),
                children: vec![
                    Node {
                        id: "a".into(),
                        component: "text".into(),
                        props: json!({}),
                        children: vec![],
                        ..Default::default()
                    },
                    Node {
                        id: "b".into(),
                        component: "text".into(),
                        props: json!({}),
                        children: vec![],
                        ..Default::default()
                    },
                ],
                ..Default::default()
            }),
            ..Default::default()
        };
        let tree = InspectorPanel::tree(&doc);
        assert!(tree.contains("root · container"));
        assert!(tree.contains("  a · text"));
        assert!(tree.contains("  b · text"));
    }
}
