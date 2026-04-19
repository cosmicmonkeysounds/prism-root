//! `kernel::builder::starter_app` — materialise a `StarterAppTemplate`
//! into a concrete `app → app-shell + routes + pages` tree.
//!
//! Port of `kernel/builder/starter-app.ts` at 8426588. Pure and
//! kernel-agnostic: callers pass in a `StarterCreateObjectFn`
//! closure mirroring `StudioKernel::create_object`, so tests can
//! supply a minimal fake while real consumers hand through a live
//! kernel. No Slint, no studio imports.

use serde_json::{Map as JsonMap, Value as JsonValue};
use std::collections::{BTreeMap, HashMap};

use super::types::{AppProfile, PageTemplateKind, StarterAppTemplate, StarterShellChild};

// ── Caller-facing types ─────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct StarterCreateObjectInput {
    pub type_name: String,
    pub name: String,
    pub parent_id: Option<String>,
    pub position: usize,
    pub data: JsonMap<String, JsonValue>,
}

#[derive(Debug, Clone)]
pub struct StarterCreatedObject {
    pub id: String,
}

pub type StarterCreateObjectFn<'a> =
    &'a mut dyn FnMut(StarterCreateObjectInput) -> StarterCreatedObject;

#[derive(Debug, Clone)]
pub struct MaterializedStarterApp {
    pub app_id: String,
    pub app_shell_id: String,
    pub route_to_page_id: HashMap<String, String>,
    pub route_ids: Vec<String>,
    pub home_route_id: String,
}

// ── Errors ──────────────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum StarterAppError {
    #[error("profile \"{0}\" has no starterApp template")]
    MissingTemplate(String),
    #[error("profile \"{0}\" starterApp has no routes")]
    NoRoutes(String),
    #[error("profile \"{0}\" starterApp must mark one route as home")]
    NoHome(String),
    #[error("profile \"{0}\" starterApp has {1} home routes, expected 1")]
    MultipleHomes(String, usize),
}

// ── Main entry ──────────────────────────────────────────────────────────────

pub fn materialize_starter_app(
    profile: &AppProfile,
    create_object: StarterCreateObjectFn<'_>,
) -> Result<MaterializedStarterApp, StarterAppError> {
    let template = profile
        .starter_app
        .as_ref()
        .ok_or_else(|| StarterAppError::MissingTemplate(profile.id.clone()))?;
    validate_template(&profile.id, template)?;

    // 1. Root `app` object.
    let app = create_object(StarterCreateObjectInput {
        type_name: "app".into(),
        name: template.label.clone(),
        parent_id: None,
        position: 0,
        data: {
            let mut m = JsonMap::new();
            m.insert("name".into(), JsonValue::String(template.label.clone()));
            m.insert("profileId".into(), JsonValue::String(profile.id.clone()));
            let theme_primary = profile
                .theme
                .as_ref()
                .and_then(|t| t.primary.clone())
                .unwrap_or_default();
            m.insert("themePrimary".into(), JsonValue::String(theme_primary));
            m.insert(
                "description".into(),
                JsonValue::String(template.description.clone().unwrap_or_default()),
            );
            m
        },
    });

    // 2. App Shell.
    let app_shell = create_object(StarterCreateObjectInput {
        type_name: "app-shell".into(),
        name: format!("{} App Shell", template.label),
        parent_id: Some(app.id.clone()),
        position: 0,
        data: btree_to_json_map(&template.app_shell.data),
    });
    if let Some(children) = template.app_shell.children.as_ref() {
        for (index, child) in children.iter().enumerate() {
            create_shell_child(&app_shell.id, child, index, create_object);
        }
    }

    // 3. Routes + pages.
    let mut route_ids: Vec<String> = Vec::new();
    let mut route_to_page_id: HashMap<String, String> = HashMap::new();
    let mut home_route_id: Option<String> = None;

    for (index, route_template) in template.routes.iter().enumerate() {
        // Page under the app.
        let page = create_object(StarterCreateObjectInput {
            type_name: "page".into(),
            name: route_template.label.clone(),
            parent_id: Some(app.id.clone()),
            position: index + 1,
            data: {
                let mut m = JsonMap::new();
                m.insert(
                    "title".into(),
                    JsonValue::String(route_template.label.clone()),
                );
                m.insert(
                    "slug".into(),
                    JsonValue::String(route_template.path.clone()),
                );
                m.insert("layout".into(), JsonValue::String("shell".into()));
                m.insert("published".into(), JsonValue::Bool(false));
                m
            },
        });

        // Page Shell under the page.
        let page_shell = create_object(StarterCreateObjectInput {
            type_name: "page-shell".into(),
            name: "Page Shell".into(),
            parent_id: Some(page.id.clone()),
            position: 0,
            data: btree_to_json_map(&template.default_page_shell.data),
        });
        if let Some(children) = template.default_page_shell.children.as_ref() {
            for (child_index, child) in children.iter().enumerate() {
                create_shell_child(&page_shell.id, child, child_index, create_object);
            }
        }
        seed_page_template_body(route_template.page_template, &page_shell.id, create_object);

        // Route under the app.
        let route = create_object(StarterCreateObjectInput {
            type_name: "route".into(),
            name: route_template.label.clone(),
            parent_id: Some(app.id.clone()),
            position: template.routes.len() + 1 + index,
            data: {
                let mut m = JsonMap::new();
                m.insert(
                    "path".into(),
                    JsonValue::String(route_template.path.clone()),
                );
                m.insert("pageId".into(), JsonValue::String(page.id.clone()));
                m.insert(
                    "label".into(),
                    JsonValue::String(route_template.label.clone()),
                );
                m.insert(
                    "showInNav".into(),
                    JsonValue::Bool(route_template.show_in_nav.unwrap_or(true)),
                );
                m.insert("parentRouteId".into(), JsonValue::String(String::new()));
                m
            },
        });

        route_ids.push(route.id.clone());
        route_to_page_id.insert(route.id.clone(), page.id.clone());
        if route_template.is_home == Some(true) {
            home_route_id = Some(route.id.clone());
        }
    }

    let home_route_id = home_route_id.ok_or_else(|| StarterAppError::NoHome(profile.id.clone()))?;

    Ok(MaterializedStarterApp {
        app_id: app.id,
        app_shell_id: app_shell.id,
        route_to_page_id,
        route_ids,
        home_route_id,
    })
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn create_shell_child(
    parent_id: &str,
    child: &StarterShellChild,
    position: usize,
    create_object: StarterCreateObjectFn<'_>,
) -> StarterCreatedObject {
    let mut data = JsonMap::new();
    data.insert("__slot".into(), JsonValue::String(child.slot.clone()));
    if let Some(child_data) = child.data.as_ref() {
        for (k, v) in child_data {
            data.insert(k.clone(), v.clone());
        }
    }
    create_object(StarterCreateObjectInput {
        type_name: child.type_name.clone(),
        name: child
            .name
            .clone()
            .unwrap_or_else(|| child.type_name.clone()),
        parent_id: Some(parent_id.into()),
        position,
        data,
    })
}

fn seed_page_template_body(
    template: PageTemplateKind,
    page_shell_id: &str,
    create_object: StarterCreateObjectFn<'_>,
) {
    match template {
        PageTemplateKind::Blank => {}
        PageTemplateKind::Landing => {
            create_object(StarterCreateObjectInput {
                type_name: "hero".into(),
                name: "Hero".into(),
                parent_id: Some(page_shell_id.into()),
                position: 0,
                data: {
                    let mut m = JsonMap::new();
                    m.insert("__slot".into(), JsonValue::String("main".into()));
                    m.insert("align".into(), JsonValue::String("center".into()));
                    m.insert("minHeight".into(), JsonValue::from(360));
                    m
                },
            });
            create_object(StarterCreateObjectInput {
                type_name: "text-block".into(),
                name: "Body".into(),
                parent_id: Some(page_shell_id.into()),
                position: 1,
                data: {
                    let mut m = JsonMap::new();
                    m.insert("__slot".into(), JsonValue::String("main".into()));
                    m.insert(
                        "content".into(),
                        JsonValue::String("Write a compelling intro here.".into()),
                    );
                    m
                },
            });
        }
        PageTemplateKind::Blog => {
            create_object(StarterCreateObjectInput {
                type_name: "heading".into(),
                name: "Title".into(),
                parent_id: Some(page_shell_id.into()),
                position: 0,
                data: {
                    let mut m = JsonMap::new();
                    m.insert("__slot".into(), JsonValue::String("main".into()));
                    m.insert("text".into(), JsonValue::String("Blog post title".into()));
                    m.insert("level".into(), JsonValue::String("h1".into()));
                    m.insert("align".into(), JsonValue::String("left".into()));
                    m
                },
            });
            create_object(StarterCreateObjectInput {
                type_name: "text-block".into(),
                name: "Body".into(),
                parent_id: Some(page_shell_id.into()),
                position: 1,
                data: {
                    let mut m = JsonMap::new();
                    m.insert("__slot".into(), JsonValue::String("main".into()));
                    m.insert(
                        "content".into(),
                        JsonValue::String("Start writing your post...".into()),
                    );
                    m
                },
            });
        }
    }
}

fn validate_template(
    profile_id: &str,
    template: &StarterAppTemplate,
) -> Result<(), StarterAppError> {
    if template.routes.is_empty() {
        return Err(StarterAppError::NoRoutes(profile_id.into()));
    }
    let home_count = template
        .routes
        .iter()
        .filter(|r| r.is_home == Some(true))
        .count();
    if home_count == 0 {
        return Err(StarterAppError::NoHome(profile_id.into()));
    }
    if home_count > 1 {
        return Err(StarterAppError::MultipleHomes(
            profile_id.into(),
            home_count,
        ));
    }
    Ok(())
}

fn btree_to_json_map(map: &BTreeMap<String, JsonValue>) -> JsonMap<String, JsonValue> {
    map.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
}
