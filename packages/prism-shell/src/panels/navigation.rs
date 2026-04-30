//! Navigation panel — page management and link authoring for the active app.
//!
//! Shows the list of pages in the current `PrismApp`, lets the user
//! add, remove, reorder, and rename pages, and configure navigation
//! style. Also shows link summary: which components on the current page
//! have `href` props pointing to intra-app pages.

use prism_builder::app::{NavigationStyle, Page, PrismApp};
use prism_builder::document::BuilderDocument;
use prism_builder::signal::ActionKind;
use prism_builder::style::StyleProperties;
use prism_core::help::HelpEntry;

use super::Panel;

pub struct NavigationPanel;

impl Panel for NavigationPanel {
    fn id(&self) -> i32 {
        6
    }
    fn label(&self) -> &'static str {
        "navigation"
    }
    fn title(&self) -> &'static str {
        "Navigation"
    }
    fn hint(&self) -> &'static str {
        "Manage pages, routes, and intra-app links."
    }
    fn help_entry(&self) -> Option<HelpEntry> {
        Some(HelpEntry::new(
            "panel.navigation",
            "Navigation",
            "Manage the pages in your app. Add, remove, rename, and reorder pages. Configure navigation style (tabs, sidebar, bottom bar). View and create intra-app links.",
        ))
    }
}

/// One row in the page list shown by the navigation panel.
#[derive(Debug, Clone)]
pub struct PageRow {
    pub index: usize,
    pub id: String,
    pub title: String,
    pub route: String,
    pub is_active: bool,
    pub node_count: usize,
    pub link_count: usize,
}

impl NavigationPanel {
    pub fn page_rows(app: &PrismApp) -> Vec<PageRow> {
        app.pages
            .iter()
            .enumerate()
            .map(|(i, page)| {
                let node_count = count_nodes(page.document.root.as_ref());
                let link_count = count_links(page.document.root.as_ref());
                PageRow {
                    index: i,
                    id: page.id.clone(),
                    title: page.title.clone(),
                    route: page.route.clone(),
                    is_active: i == app.active_page,
                    node_count,
                    link_count,
                }
            })
            .collect()
    }

    pub fn navigation_style(app: &PrismApp) -> NavigationStyle {
        app.navigation.style
    }

    pub fn create_page(app: &mut PrismApp) -> usize {
        let num = app.pages.len() + 1;
        let page = Page {
            id: format!("page-{num}"),
            title: format!("Page {num}"),
            route: format!("/page-{num}"),
            source: String::new(),
            document: BuilderDocument::page_shell(),
            style: StyleProperties::default(),
        };
        app.add_page(page);
        app.pages.len() - 1
    }

    pub fn delete_page(app: &mut PrismApp, index: usize) -> bool {
        app.remove_page(index).is_some()
    }

    pub fn rename_page(app: &mut PrismApp, index: usize, title: &str) -> bool {
        if let Some(page) = app.pages.get_mut(index) {
            page.title = title.to_string();
            true
        } else {
            false
        }
    }

    pub fn set_page_route(app: &mut PrismApp, index: usize, route: &str) -> bool {
        if let Some(page) = app.pages.get_mut(index) {
            page.route = route.to_string();
            true
        } else {
            false
        }
    }

    pub fn move_page_up(app: &mut PrismApp, index: usize) -> bool {
        if index == 0 || index >= app.pages.len() {
            return false;
        }
        app.pages.swap(index, index - 1);
        if app.active_page == index {
            app.active_page = index - 1;
        } else if app.active_page == index - 1 {
            app.active_page = index;
        }
        true
    }

    pub fn move_page_down(app: &mut PrismApp, index: usize) -> bool {
        if index + 1 >= app.pages.len() {
            return false;
        }
        app.pages.swap(index, index + 1);
        if app.active_page == index {
            app.active_page = index + 1;
        } else if app.active_page == index + 1 {
            app.active_page = index;
        }
        true
    }

    pub fn set_navigation_style(app: &mut PrismApp, style: NavigationStyle) {
        app.navigation.style = style;
    }

    /// Collect all `href` values from the active page's document that
    /// point to intra-app routes.
    pub fn intra_app_links(app: &PrismApp) -> Vec<LinkEntry> {
        let routes: Vec<&str> = app.pages.iter().map(|p| p.route.as_str()).collect();
        let page_ids: Vec<&str> = app.pages.iter().map(|p| p.id.as_str()).collect();
        let doc = match app.active_document() {
            Some(d) => d,
            None => return Vec::new(),
        };
        let mut links = Vec::new();
        if let Some(root) = &doc.root {
            collect_links(root, &routes, &page_ids, &mut links);
        }
        links
    }
}

#[derive(Debug, Clone)]
pub struct LinkEntry {
    pub node_id: String,
    pub component: String,
    pub href: String,
    pub target_page_title: Option<String>,
}

// ── Visual graph data ────────────────────────────────────────────

/// A page node positioned in the visual navigation graph.
#[derive(Debug, Clone)]
pub struct GraphNode {
    pub page_index: usize,
    pub id: String,
    pub title: String,
    pub route: String,
    pub is_active: bool,
    pub node_count: usize,
    pub link_count: usize,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// A directed edge in the visual navigation graph.
#[derive(Debug, Clone)]
pub struct GraphEdge {
    pub id: String,
    pub source_page_index: usize,
    pub target_page_index: usize,
    pub label: String,
    /// "href" or "signal"
    pub kind: String,
}

const NODE_W: f32 = 140.0;
const NODE_H: f32 = 64.0;
const NODE_GAP_X: f32 = 40.0;
const NODE_GAP_Y: f32 = 32.0;
const COLS: usize = 3;
const GRAPH_PADDING: f32 = 24.0;

impl NavigationPanel {
    /// Compute positioned graph nodes for all pages.
    pub fn graph_nodes(app: &PrismApp) -> Vec<GraphNode> {
        app.pages
            .iter()
            .enumerate()
            .map(|(i, page)| {
                let col = i % COLS;
                let row = i / COLS;
                GraphNode {
                    page_index: i,
                    id: page.id.clone(),
                    title: page.title.clone(),
                    route: page.route.clone(),
                    is_active: i == app.active_page,
                    node_count: count_nodes(page.document.root.as_ref()),
                    link_count: count_links(page.document.root.as_ref()),
                    x: GRAPH_PADDING + col as f32 * (NODE_W + NODE_GAP_X),
                    y: GRAPH_PADDING + row as f32 * (NODE_H + NODE_GAP_Y),
                    width: NODE_W,
                    height: NODE_H,
                }
            })
            .collect()
    }

    /// Collect all cross-page edges across every page in the app.
    /// Finds both `href` prop links and `NavigateTo` signal connections.
    pub fn graph_edges(app: &PrismApp) -> Vec<GraphEdge> {
        let routes: Vec<&str> = app.pages.iter().map(|p| p.route.as_str()).collect();
        let page_ids: Vec<&str> = app.pages.iter().map(|p| p.id.as_str()).collect();
        let mut edges = Vec::new();

        for (src_idx, page) in app.pages.iter().enumerate() {
            // href-based links from this page's document tree
            if let Some(root) = &page.document.root {
                let mut href_targets = Vec::new();
                collect_href_targets(root, &mut href_targets);
                for (node_id, href) in href_targets {
                    let target_idx = routes
                        .iter()
                        .position(|r| *r == href)
                        .or_else(|| page_ids.iter().position(|id| *id == href));
                    if let Some(tgt_idx) = target_idx {
                        if tgt_idx != src_idx {
                            edges.push(GraphEdge {
                                id: format!("href:{src_idx}:{node_id}"),
                                source_page_index: src_idx,
                                target_page_index: tgt_idx,
                                label: format!("{node_id} → href"),
                                kind: "href".into(),
                            });
                        }
                    }
                }
            }

            // NavigateTo signal connections from this page's document
            for conn in &page.document.connections {
                if let ActionKind::NavigateTo { ref target } = conn.action {
                    let target_idx = routes
                        .iter()
                        .position(|r| *r == target.as_str())
                        .or_else(|| page_ids.iter().position(|id| *id == target.as_str()));
                    if let Some(tgt_idx) = target_idx {
                        if tgt_idx != src_idx {
                            edges.push(GraphEdge {
                                id: conn.id.clone(),
                                source_page_index: src_idx,
                                target_page_index: tgt_idx,
                                label: format!("{} → {}", conn.signal, target),
                                kind: "signal".into(),
                            });
                        }
                    }
                }
            }
        }

        edges
    }
}

fn collect_href_targets(node: &prism_builder::Node, out: &mut Vec<(String, String)>) {
    if let Some(href) = node.props.get("href").and_then(|v| v.as_str()) {
        if !href.is_empty() {
            out.push((node.id.clone(), href.to_string()));
        }
    }
    for child in &node.children {
        collect_href_targets(child, out);
    }
}

fn collect_links(
    node: &prism_builder::Node,
    routes: &[&str],
    page_ids: &[&str],
    out: &mut Vec<LinkEntry>,
) {
    if let Some(href) = node.props.get("href").and_then(|v| v.as_str()) {
        if !href.is_empty() {
            let target_title = routes
                .iter()
                .position(|r| *r == href)
                .or_else(|| page_ids.iter().position(|id| *id == href))
                .map(|idx| format!("Page {}", idx + 1));
            out.push(LinkEntry {
                node_id: node.id.clone(),
                component: node.component.clone(),
                href: href.to_string(),
                target_page_title: target_title,
            });
        }
    }
    for child in &node.children {
        collect_links(child, routes, page_ids, out);
    }
}

fn count_nodes(root: Option<&prism_builder::Node>) -> usize {
    match root {
        Some(node) => {
            1 + node
                .children
                .iter()
                .map(|c| count_nodes(Some(c)))
                .sum::<usize>()
        }
        None => 0,
    }
}

fn count_links(root: Option<&prism_builder::Node>) -> usize {
    match root {
        Some(node) => {
            let has_link = node
                .props
                .get("href")
                .and_then(|v| v.as_str())
                .map(|s| !s.is_empty())
                .unwrap_or(false);
            let self_count = if has_link { 1 } else { 0 };
            self_count
                + node
                    .children
                    .iter()
                    .map(|c| count_links(Some(c)))
                    .sum::<usize>()
        }
        None => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prism_builder::app::{AppIcon, NavigationConfig};
    use prism_builder::document::Node;
    use serde_json::json;

    fn test_app() -> PrismApp {
        PrismApp {
            id: "test".into(),
            name: "Test".into(),
            description: String::new(),
            icon: AppIcon::Cube,
            pages: vec![
                Page {
                    id: "p1".into(),
                    title: "Home".into(),
                    route: "/".into(),
                    source: String::new(),
                    document: BuilderDocument {
                        root: Some(Node {
                            id: "n1".into(),
                            component: "text".into(),
                            props: json!({ "body": "Hello", "href": "/about" }),
                            children: vec![],
                            ..Default::default()
                        }),
                        ..Default::default()
                    },
                    style: StyleProperties::default(),
                },
                Page {
                    id: "p2".into(),
                    title: "About".into(),
                    route: "/about".into(),
                    source: String::new(),
                    document: BuilderDocument::default(),
                    style: StyleProperties::default(),
                },
            ],
            active_page: 0,
            navigation: NavigationConfig::default(),
            style: StyleProperties::default(),
        }
    }

    #[test]
    fn page_rows_lists_all_pages() {
        let app = test_app();
        let rows = NavigationPanel::page_rows(&app);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].title, "Home");
        assert!(rows[0].is_active);
        assert_eq!(rows[0].link_count, 1);
        assert_eq!(rows[1].title, "About");
        assert!(!rows[1].is_active);
    }

    #[test]
    fn create_and_delete_page() {
        let mut app = test_app();
        let idx = NavigationPanel::create_page(&mut app);
        assert_eq!(idx, 2);
        assert_eq!(app.page_count(), 3);

        assert!(NavigationPanel::delete_page(&mut app, 2));
        assert_eq!(app.page_count(), 2);
    }

    #[test]
    fn rename_page() {
        let mut app = test_app();
        assert!(NavigationPanel::rename_page(&mut app, 0, "Landing"));
        assert_eq!(app.pages[0].title, "Landing");
    }

    #[test]
    fn set_page_route() {
        let mut app = test_app();
        assert!(NavigationPanel::set_page_route(&mut app, 1, "/info"));
        assert_eq!(app.pages[1].route, "/info");
    }

    #[test]
    fn move_page_up_and_down() {
        let mut app = test_app();
        app.active_page = 0;

        assert!(NavigationPanel::move_page_down(&mut app, 0));
        assert_eq!(app.pages[0].title, "About");
        assert_eq!(app.pages[1].title, "Home");
        assert_eq!(app.active_page, 1);

        assert!(NavigationPanel::move_page_up(&mut app, 1));
        assert_eq!(app.pages[0].title, "Home");
        assert_eq!(app.pages[1].title, "About");
        assert_eq!(app.active_page, 0);
    }

    #[test]
    fn move_page_bounds() {
        let mut app = test_app();
        assert!(!NavigationPanel::move_page_up(&mut app, 0));
        assert!(!NavigationPanel::move_page_down(&mut app, 1));
    }

    #[test]
    fn intra_app_links_finds_href() {
        let app = test_app();
        let links = NavigationPanel::intra_app_links(&app);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].href, "/about");
        assert!(links[0].target_page_title.is_some());
    }

    #[test]
    fn navigation_style_default_is_tabs() {
        let app = test_app();
        assert_eq!(
            NavigationPanel::navigation_style(&app),
            NavigationStyle::Tabs
        );
    }

    #[test]
    fn set_navigation_style() {
        let mut app = test_app();
        NavigationPanel::set_navigation_style(&mut app, NavigationStyle::Sidebar);
        assert_eq!(app.navigation.style, NavigationStyle::Sidebar);
    }

    #[test]
    fn graph_nodes_positions_pages() {
        let app = test_app();
        let nodes = NavigationPanel::graph_nodes(&app);
        assert_eq!(nodes.len(), 2);
        assert_eq!(nodes[0].title, "Home");
        assert!(nodes[0].is_active);
        assert!(nodes[0].x > 0.0);
        assert!(nodes[0].y > 0.0);
        assert!(nodes[0].width > 0.0);
        // Second node is in a different column
        assert!(nodes[1].x > nodes[0].x);
    }

    #[test]
    fn graph_edges_finds_href_links() {
        let app = test_app();
        let edges = NavigationPanel::graph_edges(&app);
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].source_page_index, 0);
        assert_eq!(edges[0].target_page_index, 1);
        assert_eq!(edges[0].kind, "href");
        assert_eq!(edges[0].id, "href:0:n1");
    }

    #[test]
    fn graph_edges_finds_signal_connections() {
        use prism_builder::signal::{ActionKind, Connection};
        let mut app = test_app();
        app.pages[1].document.connections.push(Connection {
            id: "c1".into(),
            source_node: "root".into(),
            signal: "clicked".into(),
            target_node: String::new(),
            action: ActionKind::NavigateTo { target: "/".into() },
            params: serde_json::Value::Null,
        });
        let edges = NavigationPanel::graph_edges(&app);
        assert_eq!(edges.len(), 2);
        let sig_edge = edges.iter().find(|e| e.kind == "signal").unwrap();
        assert_eq!(sig_edge.source_page_index, 1);
        assert_eq!(sig_edge.target_page_index, 0);
        assert_eq!(sig_edge.id, "c1");
    }

    #[test]
    fn graph_edges_skips_self_links() {
        let mut app = test_app();
        // Add href to self (page 0 linking to page 0's route "/")
        if let Some(root) = &mut app.pages[0].document.root {
            root.props = json!({ "body": "Self", "href": "/" });
        }
        let edges = NavigationPanel::graph_edges(&app);
        // Self-link should be excluded
        assert!(edges
            .iter()
            .all(|e| e.source_page_index != e.target_page_index));
    }
}
