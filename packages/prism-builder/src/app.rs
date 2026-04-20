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
}
