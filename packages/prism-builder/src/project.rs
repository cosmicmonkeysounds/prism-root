use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::app::{AppIcon, NavigationConfig, Page, PrismApp};
use crate::document::BuilderDocument;
use crate::facet::{FacetDef, FacetSchema, FacetSchemaId};
use crate::layout::PageLayout;
use crate::prefab::PrefabDef;
use crate::resource::{ResourceDef, ResourceId};
use crate::signal::Connection;
use crate::style::StyleProperties;

pub const FILE_EXTENSION: &str = "prism";
pub const FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectFile {
    pub version: u32,
    pub apps: Vec<SavedApp>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedApp {
    pub id: String,
    pub name: String,
    pub description: String,
    pub icon: AppIcon,
    pub pages: Vec<SavedPage>,
    pub active_page: usize,
    pub navigation: NavigationConfig,
    #[serde(default)]
    pub style: StyleProperties,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedPage {
    pub id: String,
    pub title: String,
    pub route: String,
    #[serde(default)]
    pub source: String,
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
    #[serde(default)]
    pub style: StyleProperties,
}

impl ProjectFile {
    pub fn from_apps(
        apps: &[PrismApp],
        registry: &crate::registry::ComponentRegistry,
        tokens: &prism_core::design_tokens::DesignTokens,
    ) -> Self {
        let saved = apps
            .iter()
            .map(|app| SavedApp::from_app(app, registry, tokens))
            .collect();
        Self {
            version: FORMAT_VERSION,
            apps: saved,
        }
    }

    pub fn into_apps(self) -> Vec<PrismApp> {
        self.apps.into_iter().map(SavedApp::into_app).collect()
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}

impl SavedApp {
    fn from_app(
        app: &PrismApp,
        registry: &crate::registry::ComponentRegistry,
        tokens: &prism_core::design_tokens::DesignTokens,
    ) -> Self {
        let pages = app
            .pages
            .iter()
            .map(|p| SavedPage::from_page(p, registry, tokens))
            .collect();
        Self {
            id: app.id.clone(),
            name: app.name.clone(),
            description: app.description.clone(),
            icon: app.icon.clone(),
            pages,
            active_page: app.active_page,
            navigation: app.navigation.clone(),
            style: app.style.clone(),
        }
    }

    fn into_app(self) -> PrismApp {
        let pages = self.pages.into_iter().map(SavedPage::into_page).collect();
        PrismApp {
            id: self.id,
            name: self.name,
            description: self.description,
            icon: self.icon,
            pages,
            active_page: self.active_page,
            navigation: self.navigation,
            style: self.style,
        }
    }
}

impl SavedPage {
    fn from_page(
        page: &Page,
        registry: &crate::registry::ComponentRegistry,
        tokens: &prism_core::design_tokens::DesignTokens,
    ) -> Self {
        let mut source = page.source.clone();
        if source.is_empty() && page.document.root.is_some() {
            if let Ok((src, _)) =
                crate::render::render_document_slint_source_mapped(&page.document, registry, tokens)
            {
                source = src;
            }
        }
        Self {
            id: page.id.clone(),
            title: page.title.clone(),
            route: page.route.clone(),
            source,
            page_layout: page.document.page_layout.clone(),
            resources: page.document.resources.clone(),
            connections: page.document.connections.clone(),
            prefabs: page.document.prefabs.clone(),
            facet_schemas: page.document.facet_schemas.clone(),
            facets: page.document.facets.clone(),
            style: page.style.clone(),
        }
    }

    fn into_page(self) -> Page {
        let document = BuilderDocument {
            root: None,
            zones: IndexMap::new(),
            page_layout: self.page_layout,
            resources: self.resources,
            connections: self.connections,
            prefabs: self.prefabs,
            facet_schemas: self.facet_schemas,
            facets: self.facets,
        };
        Page {
            id: self.id,
            title: self.title,
            route: self.route,
            source: self.source,
            document,
            style: self.style,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::AppIcon;
    use crate::document::Node;
    use crate::starter::register_builtins;
    use prism_core::design_tokens::DEFAULT_TOKENS;
    use serde_json::json;

    fn test_app() -> PrismApp {
        PrismApp {
            id: "test-app".into(),
            name: "Test".into(),
            description: "A test app".into(),
            icon: AppIcon::Cube,
            pages: vec![Page {
                id: "p1".into(),
                title: "Home".into(),
                route: "/".into(),
                source: String::new(),
                document: BuilderDocument {
                    root: Some(Node {
                        id: "root".into(),
                        component: "text".into(),
                        props: json!({ "body": "Hello" }),
                        children: vec![],
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                style: StyleProperties::default(),
            }],
            active_page: 0,
            navigation: NavigationConfig::default(),
            style: StyleProperties::default(),
        }
    }

    #[test]
    fn round_trip_preserves_app_metadata() {
        let mut reg = crate::registry::ComponentRegistry::new();
        register_builtins(&mut reg).unwrap();
        let apps = vec![test_app()];
        let file = ProjectFile::from_apps(&apps, &reg, &DEFAULT_TOKENS);
        let json = file.to_json().unwrap();
        let restored = ProjectFile::from_json(&json).unwrap();
        assert_eq!(restored.version, FORMAT_VERSION);
        assert_eq!(restored.apps.len(), 1);
        assert_eq!(restored.apps[0].name, "Test");
        assert_eq!(restored.apps[0].pages.len(), 1);
        assert_eq!(restored.apps[0].pages[0].title, "Home");
    }

    #[test]
    fn source_is_generated_when_empty() {
        let mut reg = crate::registry::ComponentRegistry::new();
        register_builtins(&mut reg).unwrap();
        let apps = vec![test_app()];
        let file = ProjectFile::from_apps(&apps, &reg, &DEFAULT_TOKENS);
        assert!(!file.apps[0].pages[0].source.is_empty());
    }

    #[test]
    fn into_apps_reconstructs_sidecar_data() {
        let mut reg = crate::registry::ComponentRegistry::new();
        register_builtins(&mut reg).unwrap();

        let mut app = test_app();
        app.pages[0]
            .document
            .connections
            .push(crate::signal::Connection {
                id: "c1".into(),
                source_node: "root".into(),
                signal: "clicked".into(),
                target_node: "root".into(),
                action: crate::signal::ActionKind::ToggleVisibility,
                params: serde_json::Value::Null,
            });
        let file = ProjectFile::from_apps(&[app], &reg, &DEFAULT_TOKENS);
        let restored = file.into_apps();
        assert_eq!(restored[0].pages[0].document.connections.len(), 1);
        assert_eq!(restored[0].pages[0].document.connections[0].id, "c1");
    }

    #[test]
    fn version_field_is_set() {
        let mut reg = crate::registry::ComponentRegistry::new();
        register_builtins(&mut reg).unwrap();
        let file = ProjectFile::from_apps(&[], &reg, &DEFAULT_TOKENS);
        assert_eq!(file.version, FORMAT_VERSION);
    }

    #[test]
    fn empty_project_round_trips() {
        let mut reg = crate::registry::ComponentRegistry::new();
        register_builtins(&mut reg).unwrap();
        let file = ProjectFile::from_apps(&[], &reg, &DEFAULT_TOKENS);
        let json = file.to_json().unwrap();
        let restored = ProjectFile::from_json(&json).unwrap();
        assert!(restored.apps.is_empty());
    }
}
