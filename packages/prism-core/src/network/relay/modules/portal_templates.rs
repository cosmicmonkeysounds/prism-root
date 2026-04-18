//! Portal templates — reusable HTML template blueprints.

use std::collections::HashMap;
use std::sync::RwLock;

use serde::{Deserialize, Serialize};

use crate::network::relay::module_system::{capabilities, RelayContext, RelayModule};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PortalTemplate {
    pub template_id: String,
    pub name: String,
    pub description: String,
    pub css: String,
    pub header_html: String,
    pub footer_html: String,
    pub object_card_html: String,
    pub created_at: String,
}

pub struct PortalTemplateRegistry {
    templates: RwLock<HashMap<String, PortalTemplate>>,
    next_id: RwLock<u64>,
}

impl PortalTemplateRegistry {
    pub fn new() -> Self {
        Self {
            templates: RwLock::new(HashMap::new()),
            next_id: RwLock::new(1),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn register(
        &self,
        name: &str,
        description: &str,
        css: &str,
        header_html: &str,
        footer_html: &str,
        object_card_html: &str,
        now_iso: &str,
    ) -> PortalTemplate {
        let mut id_gen = self.next_id.write().unwrap();
        let template_id = format!("tpl-{}", *id_gen);
        *id_gen += 1;

        let template = PortalTemplate {
            template_id: template_id.clone(),
            name: name.to_string(),
            description: description.to_string(),
            css: css.to_string(),
            header_html: header_html.to_string(),
            footer_html: footer_html.to_string(),
            object_card_html: object_card_html.to_string(),
            created_at: now_iso.to_string(),
        };
        self.templates
            .write()
            .unwrap()
            .insert(template_id, template.clone());
        template
    }

    pub fn get(&self, template_id: &str) -> Option<PortalTemplate> {
        self.templates.read().unwrap().get(template_id).cloned()
    }

    pub fn list(&self) -> Vec<PortalTemplate> {
        self.templates.read().unwrap().values().cloned().collect()
    }

    pub fn remove(&self, template_id: &str) -> bool {
        self.templates
            .write()
            .unwrap()
            .remove(template_id)
            .is_some()
    }

    pub fn restore(&self, templates: Vec<PortalTemplate>) {
        let mut store = self.templates.write().unwrap();
        for t in templates {
            store.insert(t.template_id.clone(), t);
        }
    }
}

impl Default for PortalTemplateRegistry {
    fn default() -> Self {
        Self::new()
    }
}

pub struct PortalTemplateModule;

impl RelayModule for PortalTemplateModule {
    fn name(&self) -> &str {
        "portal-templates"
    }
    fn description(&self) -> &str {
        "Reusable portal HTML template blueprints"
    }
    fn install(&self, ctx: &RelayContext) {
        ctx.set_capability(capabilities::TEMPLATES, PortalTemplateRegistry::new());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_and_list() {
        let reg = PortalTemplateRegistry::new();
        reg.register(
            "Blog",
            "A blog template",
            "body{}",
            "<h1>{{portalName}}</h1>",
            "<footer/>",
            "<div>{{name}}</div>",
            "2026-04-18T00:00:00Z",
        );
        assert_eq!(reg.list().len(), 1);
    }

    #[test]
    fn remove() {
        let reg = PortalTemplateRegistry::new();
        let t = reg.register("Blog", "desc", "", "", "", "", "2026-04-18T00:00:00Z");
        assert!(reg.remove(&t.template_id));
        assert!(reg.get(&t.template_id).is_none());
    }
}
