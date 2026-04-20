use std::collections::HashSet;

use prism_builder::app::PrismApp;
use prism_builder::Node;
use serde::{Deserialize, Serialize};

use crate::app::ShellView;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExplorerNodeKind {
    App,
    Page,
    Node,
}

impl ExplorerNodeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::App => "app",
            Self::Page => "page",
            Self::Node => "node",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ExplorerNode {
    pub id: String,
    pub label: String,
    pub kind: ExplorerNodeKind,
    pub depth: i32,
    pub expanded: bool,
    pub is_active: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExplorerViewMode {
    #[default]
    List,
    Grid,
}

pub fn build_explorer_tree(
    apps: &[PrismApp],
    shell_view: &ShellView,
    expanded: &HashSet<String>,
) -> Vec<ExplorerNode> {
    let mut nodes = Vec::new();
    let active_app_id = shell_view.active_app_id();

    for app in apps {
        let app_key = format!("app:{}", app.id);
        let is_active_app = active_app_id == Some(app.id.as_str());
        let app_expanded = expanded.contains(&app_key) || is_active_app;

        nodes.push(ExplorerNode {
            id: app_key.clone(),
            label: app.name.clone(),
            kind: ExplorerNodeKind::App,
            depth: 0,
            expanded: app_expanded,
            is_active: is_active_app,
        });

        if app_expanded {
            for (i, page) in app.pages.iter().enumerate() {
                let page_key = format!("page:{}:{}", app.id, page.id);
                let is_active_page = is_active_app && i == app.active_page;
                let page_expanded = expanded.contains(&page_key) || is_active_page;

                nodes.push(ExplorerNode {
                    id: page_key.clone(),
                    label: page.title.clone(),
                    kind: ExplorerNodeKind::Page,
                    depth: 1,
                    expanded: page_expanded,
                    is_active: is_active_page,
                });

                if page_expanded {
                    if let Some(ref root) = page.document.root {
                        push_top_level_nodes(&mut nodes, root);
                    }
                }
            }
        }
    }

    nodes
}

fn push_top_level_nodes(out: &mut Vec<ExplorerNode>, root: &Node) {
    for child in &root.children {
        let label = node_label(child);
        out.push(ExplorerNode {
            id: format!("node:{}", child.id),
            label,
            kind: ExplorerNodeKind::Node,
            depth: 2,
            expanded: false,
            is_active: false,
        });
    }
}

fn node_label(node: &Node) -> String {
    let text = node
        .props
        .get("text")
        .or_else(|| node.props.get("body"))
        .or_else(|| node.props.get("title"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if text.is_empty() {
        format!("{} ({})", node.component, node.id)
    } else {
        let truncated: String = text.chars().take(30).collect();
        if truncated.len() < text.len() {
            format!("{truncated}…")
        } else {
            truncated
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prism_builder::app::{AppIcon, NavigationConfig, Page};
    use prism_builder::BuilderDocument;

    fn sample_apps() -> Vec<PrismApp> {
        vec![
            PrismApp {
                id: "a1".into(),
                name: "MyApp".into(),
                description: "test".into(),
                icon: AppIcon::Globe,
                pages: vec![
                    Page {
                        id: "p1".into(),
                        title: "Home".into(),
                        route: "/".into(),
                        document: BuilderDocument::default(),
                        style: Default::default(),
                    },
                    Page {
                        id: "p2".into(),
                        title: "About".into(),
                        route: "/about".into(),
                        document: BuilderDocument::default(),
                        style: Default::default(),
                    },
                ],
                active_page: 0,
                navigation: NavigationConfig::default(),
                style: Default::default(),
            },
            PrismApp {
                id: "a2".into(),
                name: "Other".into(),
                description: "other".into(),
                icon: AppIcon::Zap,
                pages: vec![Page {
                    id: "p1".into(),
                    title: "Main".into(),
                    route: "/".into(),
                    document: BuilderDocument::default(),
                    style: Default::default(),
                }],
                active_page: 0,
                navigation: NavigationConfig::default(),
                style: Default::default(),
            },
        ]
    }

    #[test]
    fn launchpad_shows_collapsed_apps() {
        let apps = sample_apps();
        let tree = build_explorer_tree(&apps, &ShellView::Launchpad, &HashSet::new());
        assert_eq!(tree.len(), 2);
        assert_eq!(tree[0].label, "MyApp");
        assert_eq!(tree[0].kind, ExplorerNodeKind::App);
        assert!(!tree[0].expanded);
        assert_eq!(tree[1].label, "Other");
    }

    #[test]
    fn active_app_auto_expands() {
        let apps = sample_apps();
        let view = ShellView::App {
            app_id: "a1".into(),
        };
        let tree = build_explorer_tree(&apps, &view, &HashSet::new());
        assert!(tree[0].expanded);
        assert!(tree[0].is_active);
        assert_eq!(tree[1].kind, ExplorerNodeKind::Page);
        assert_eq!(tree[1].label, "Home");
        assert!(tree[1].is_active);
        assert_eq!(tree[2].kind, ExplorerNodeKind::Page);
        assert_eq!(tree[2].label, "About");
        assert!(!tree[2].is_active);
    }

    #[test]
    fn manually_expanded_app() {
        let apps = sample_apps();
        let mut expanded = HashSet::new();
        expanded.insert("app:a2".into());
        let tree = build_explorer_tree(&apps, &ShellView::Launchpad, &expanded);
        let a2_idx = tree.iter().position(|n| n.label == "Other").unwrap();
        assert!(tree[a2_idx].expanded);
        assert_eq!(tree[a2_idx + 1].label, "Main");
    }

    #[test]
    fn node_kind_as_str() {
        assert_eq!(ExplorerNodeKind::App.as_str(), "app");
        assert_eq!(ExplorerNodeKind::Page.as_str(), "page");
        assert_eq!(ExplorerNodeKind::Node.as_str(), "node");
    }
}
