//! HTML rendering trait — the SSR path for Sovereign Portals.
//!
//! Separated from [`crate::component::Component`] so the Slint-side
//! builder and the relay-side HTML renderer are independent concerns.
//! The relay depends on `HtmlBlock` + `HtmlRegistry`; the shell
//! depends on `Component` + `ComponentRegistry`. Both registries
//! share the same `ComponentId` key space and `FieldSpec` schema.

use serde_json::Value;

use crate::component::{ComponentId, RenderError};
use crate::document::Node;
use crate::html::Html;
use crate::registry::FieldSpec;
use crate::signal::SignalDef;
use crate::variant::VariantAxis;

/// HTML render context. Carries the registry so parents can recurse
/// into children by `ComponentId`, plus the design tokens for
/// consistent theming.
pub struct HtmlRenderContext<'a> {
    pub tokens: &'a prism_core::design_tokens::DesignTokens,
    pub registry: &'a HtmlRegistry,
    pub resources:
        &'a indexmap::IndexMap<crate::resource::ResourceId, crate::resource::ResourceDef>,
}

impl<'a> HtmlRenderContext<'a> {
    pub fn render_child(&self, child: &Node, out: &mut Html) -> Result<(), RenderError> {
        if child.props.get("visible").and_then(|v| v.as_bool()) == Some(false) {
            return Ok(());
        }
        let block = self
            .registry
            .get(&child.component)
            .ok_or_else(|| RenderError::UnknownComponent(child.component.clone()))?;

        let props = crate::resource::resolve_resource_refs(&child.props, self.resources);
        let props = crate::variant::apply_variant_defaults(&props, &block.variants());

        if child.modifiers.is_empty() {
            block.render_html(self, &props, &child.children, out)
        } else {
            self.apply_html_modifiers(&child.modifiers, 0, &props, &child.children, block, out)
        }
    }

    pub fn render_children(&self, children: &[Node], out: &mut Html) -> Result<(), RenderError> {
        for child in children {
            self.render_child(child, out)?;
        }
        Ok(())
    }

    fn apply_html_modifiers(
        &self,
        modifiers: &[crate::modifier::Modifier],
        index: usize,
        props: &serde_json::Value,
        children: &[Node],
        block: &dyn HtmlBlock,
        out: &mut Html,
    ) -> Result<(), RenderError> {
        if index >= modifiers.len() {
            return block.render_html(self, props, children, out);
        }
        let modifier = &modifiers[index];
        match modifier.kind {
            crate::modifier::ModifierKind::ScrollOverflow => {
                out.open_attrs("div", &[("style", "overflow:auto")]);
                self.apply_html_modifiers(modifiers, index + 1, props, children, block, out)?;
                out.close("div");
                Ok(())
            }
            _ => self.apply_html_modifiers(modifiers, index + 1, props, children, block, out),
        }
    }
}

/// The HTML rendering contract. Every block that can appear in a
/// Sovereign Portal SSR response implements this.
pub trait HtmlBlock: Send + Sync {
    fn id(&self) -> &ComponentId;
    fn schema(&self) -> Vec<FieldSpec>;

    fn signals(&self) -> Vec<SignalDef> {
        crate::signal::common_signals()
    }

    fn variants(&self) -> Vec<VariantAxis> {
        vec![]
    }

    fn render_html(
        &self,
        ctx: &HtmlRenderContext<'_>,
        props: &Value,
        children: &[Node],
        out: &mut Html,
    ) -> Result<(), RenderError> {
        let _ = props;
        out.open_attrs("div", &[("data-component", self.id())]);
        ctx.render_children(children, out)?;
        out.close("div");
        Ok(())
    }
}

/// Registry of HTML renderers keyed by `ComponentId`.
pub struct HtmlRegistry {
    blocks: indexmap::IndexMap<ComponentId, std::sync::Arc<dyn HtmlBlock>>,
}

impl HtmlRegistry {
    pub fn new() -> Self {
        Self {
            blocks: indexmap::IndexMap::new(),
        }
    }

    pub fn register(
        &mut self,
        block: std::sync::Arc<dyn HtmlBlock>,
    ) -> Result<(), crate::registry::RegistryError> {
        let id = block.id().clone();
        if self.blocks.contains_key(&id) {
            return Err(crate::registry::RegistryError::AlreadyRegistered(id));
        }
        self.blocks.insert(id, block);
        Ok(())
    }

    pub fn len(&self) -> usize {
        self.blocks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }

    pub fn get(&self, id: &str) -> Option<&dyn HtmlBlock> {
        self.blocks.get(id).map(|arc| arc.as_ref())
    }

    pub fn iter(&self) -> impl Iterator<Item = &dyn HtmlBlock> {
        self.blocks.values().map(|arc| arc.as_ref())
    }
}

impl Default for HtmlRegistry {
    fn default() -> Self {
        Self::new()
    }
}
