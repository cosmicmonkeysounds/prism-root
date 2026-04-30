//! Prism App — a multi-page application built from [`BuilderDocument`] pages.
//!
//! An App groups pages with a navigation config (tabs, sidebar, etc.)
//! and metadata. The Builder creates and edits apps; the Studio shell
//! launches them from the Launchpad.

use serde::{Deserialize, Serialize};

use crate::document::BuilderDocument;
use crate::style::StyleProperties;

pub type AppId = String;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrismApp {
    pub id: AppId,
    pub name: String,
    pub description: String,
    pub icon: AppIcon,
    pub pages: Vec<Page>,
    pub active_page: usize,
    pub navigation: NavigationConfig,
    #[serde(default)]
    pub style: StyleProperties,
}

impl PrismApp {
    pub fn active_source(&self) -> Option<&str> {
        self.pages.get(self.active_page).map(|p| p.source.as_str())
    }

    pub fn active_source_mut(&mut self) -> Option<&mut String> {
        self.pages.get_mut(self.active_page).map(|p| &mut p.source)
    }

    pub fn active_document(&self) -> Option<&BuilderDocument> {
        self.pages.get(self.active_page).map(|p| &p.document)
    }

    pub fn active_document_mut(&mut self) -> Option<&mut BuilderDocument> {
        self.pages
            .get_mut(self.active_page)
            .map(|p| &mut p.document)
    }

    pub fn page_count(&self) -> usize {
        self.pages.len()
    }

    pub fn add_page(&mut self, page: Page) {
        self.pages.push(page);
    }

    /// Remove a page by index. Returns the removed page, or `None` if
    /// the index is out of bounds or removing would leave zero pages.
    pub fn remove_page(&mut self, index: usize) -> Option<Page> {
        if self.pages.len() <= 1 || index >= self.pages.len() {
            return None;
        }
        let removed = self.pages.remove(index);
        if self.active_page >= self.pages.len() {
            self.active_page = self.pages.len() - 1;
        } else if self.active_page > index {
            self.active_page -= 1;
        }
        Some(removed)
    }

    pub fn find_page_by_route(&self, route: &str) -> Option<usize> {
        self.pages.iter().position(|p| p.route == route)
    }

    pub fn find_page_by_id(&self, id: &str) -> Option<usize> {
        self.pages.iter().position(|p| p.id == id)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Page {
    pub id: String,
    pub title: String,
    pub route: String,
    #[serde(default)]
    pub source: String,
    #[serde(default, skip_serializing)]
    pub document: BuilderDocument,
    #[serde(default)]
    pub style: StyleProperties,
}

impl Page {
    pub fn ensure_source(
        &mut self,
        registry: &crate::registry::ComponentRegistry,
        tokens: &prism_core::design_tokens::DesignTokens,
    ) {
        if self.source.is_empty() && self.document.root.is_some() {
            if let Ok((src, _)) =
                crate::render::render_document_slint_source_mapped(&self.document, registry, tokens)
            {
                self.source = src;
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NavigationConfig {
    pub style: NavigationStyle,
}

impl Default for NavigationConfig {
    fn default() -> Self {
        Self {
            style: NavigationStyle::Tabs,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum NavigationStyle {
    Tabs,
    Sidebar,
    BottomBar,
    None,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum AppIcon {
    #[default]
    Cube,
    Globe,
    Music,
    Film,
    Zap,
    Star,
    Heart,
    Code,
}

impl AppIcon {
    pub fn label(&self) -> &'static str {
        match self {
            AppIcon::Cube => "cube",
            AppIcon::Globe => "globe",
            AppIcon::Music => "music",
            AppIcon::Film => "film",
            AppIcon::Zap => "zap",
            AppIcon::Star => "star",
            AppIcon::Heart => "heart",
            AppIcon::Code => "code",
        }
    }

    pub fn accent_color(&self) -> &'static str {
        match self {
            AppIcon::Cube => "#6366f1",
            AppIcon::Globe => "#06b6d4",
            AppIcon::Music => "#ec4899",
            AppIcon::Film => "#f59e0b",
            AppIcon::Zap => "#eab308",
            AppIcon::Star => "#a855f7",
            AppIcon::Heart => "#ef4444",
            AppIcon::Code => "#22c55e",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_active_document() {
        let app = PrismApp {
            id: "test".into(),
            name: "Test App".into(),
            description: "A test app".into(),
            icon: AppIcon::Cube,
            pages: vec![
                Page {
                    id: "p1".into(),
                    title: "Home".into(),
                    route: "/".into(),
                    source: "// page 1".into(),
                    document: BuilderDocument::default(),
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
        };
        assert!(app.active_document().is_some());
        assert_eq!(app.active_source(), Some("// page 1"));
        assert_eq!(app.page_count(), 2);
    }

    #[test]
    fn navigation_styles_roundtrip() {
        let config = NavigationConfig {
            style: NavigationStyle::Sidebar,
        };
        let json = serde_json::to_string(&config).unwrap();
        let parsed: NavigationConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.style, NavigationStyle::Sidebar);
    }

    #[test]
    fn app_icon_labels() {
        assert_eq!(AppIcon::Cube.label(), "cube");
        assert_eq!(AppIcon::Music.label(), "music");
        assert!(!AppIcon::Globe.accent_color().is_empty());
    }

    fn three_page_app() -> PrismApp {
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
                    document: BuilderDocument::default(),
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
                Page {
                    id: "p3".into(),
                    title: "Contact".into(),
                    route: "/contact".into(),
                    source: String::new(),
                    document: BuilderDocument::default(),
                    style: StyleProperties::default(),
                },
            ],
            active_page: 1,
            navigation: NavigationConfig::default(),
            style: StyleProperties::default(),
        }
    }

    #[test]
    fn add_page() {
        let mut app = three_page_app();
        assert_eq!(app.page_count(), 3);
        app.add_page(Page {
            id: "p4".into(),
            title: "New".into(),
            route: "/new".into(),
            source: String::new(),
            document: BuilderDocument::default(),
            style: StyleProperties::default(),
        });
        assert_eq!(app.page_count(), 4);
        assert_eq!(app.pages[3].title, "New");
    }

    #[test]
    fn remove_page_middle() {
        let mut app = three_page_app();
        app.active_page = 2;
        let removed = app.remove_page(1);
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().title, "About");
        assert_eq!(app.page_count(), 2);
        assert_eq!(app.active_page, 1);
    }

    #[test]
    fn remove_page_before_active_adjusts_index() {
        let mut app = three_page_app();
        app.active_page = 2;
        app.remove_page(0);
        assert_eq!(app.active_page, 1);
    }

    #[test]
    fn remove_active_page_clamps() {
        let mut app = three_page_app();
        app.active_page = 2;
        app.remove_page(2);
        assert_eq!(app.active_page, 1);
    }

    #[test]
    fn remove_last_remaining_page_fails() {
        let mut app = PrismApp {
            id: "t".into(),
            name: "T".into(),
            description: String::new(),
            icon: AppIcon::Cube,
            pages: vec![Page {
                id: "only".into(),
                title: "Only".into(),
                route: "/".into(),
                source: String::new(),
                document: BuilderDocument::default(),
                style: StyleProperties::default(),
            }],
            active_page: 0,
            navigation: NavigationConfig::default(),
            style: StyleProperties::default(),
        };
        assert!(app.remove_page(0).is_none());
        assert_eq!(app.page_count(), 1);
    }

    #[test]
    fn remove_page_out_of_bounds() {
        let mut app = three_page_app();
        assert!(app.remove_page(99).is_none());
    }

    #[test]
    fn find_page_by_route() {
        let app = three_page_app();
        assert_eq!(app.find_page_by_route("/about"), Some(1));
        assert_eq!(app.find_page_by_route("/missing"), None);
    }

    #[test]
    fn find_page_by_id() {
        let app = three_page_app();
        assert_eq!(app.find_page_by_id("p3"), Some(2));
        assert_eq!(app.find_page_by_id("missing"), None);
    }
}
