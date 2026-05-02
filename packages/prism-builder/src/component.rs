//! The `Component` trait — every Slint-renderable block implements this.
//!
//! [`Component::render_slint`] — the interactive Studio path. Components
//! emit `.slint` DSL snippets into a shared [`crate::slint_source::SlintEmitter`];
//! the document walker in [`crate::render::render_document_slint_source`]
//! composes those snippets into a self-contained component source the
//! Studio shell hands to [`slint_interpreter::Compiler`].
//!
//! HTML SSR is handled by the separate [`crate::html_block::HtmlBlock`]
//! trait and [`crate::html_block::HtmlRegistry`], which `prism-relay`
//! uses for Sovereign Portal rendering.
//!
//! The trait is deliberately object-safe so `Arc<dyn Component>`s can
//! live in the [`crate::registry::ComponentRegistry`] and be dispatched
//! by `ComponentId` at walk time.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::PathBuf;

use prism_core::help::HelpEntry;
use serde_json::Value;
use thiserror::Error;

use crate::document::Node;
use crate::facet::FacetDef;
use crate::layout::{Dimension, FlexDirection, FlowDisplay, FlowProps, GridPlacement, LayoutMode};
use crate::prefab::PrefabDef;
use crate::registry::{ComponentRegistry, FieldSpec};
use crate::signal::SignalDef;
use crate::slint_source::SlintEmitter;
use crate::style::StyleProperties;
use crate::variant::VariantAxis;
use prism_core::foundation::spatial::Transform2D;
use prism_core::widget::ToolbarAction;

/// Stable identifier for a component *type* (e.g. `"card"`, `"button"`).
///
/// This is the key the [`crate::registry::ComponentRegistry`] uses when
/// looking up a renderer for a given node.
pub type ComponentId = String;

/// Errors a component can hit while rendering into a target backend.
/// Surfaces as 500s in `prism-relay` and as a developer-visible red
/// banner in Studio's builder pane.
#[derive(Debug, Error)]
pub enum RenderError {
    /// A child node references a `ComponentId` that isn't registered.
    #[error("unknown component: {0}")]
    UnknownComponent(ComponentId),

    /// Props failed validation or a component chose to bail.
    #[error("render failed: {0}")]
    Failed(String),
}

/// Slint-side render context — used by the Sovereign Portal's Phase-3
/// interactive surface and by tests. Carries a reference to the design
/// tokens so components can pull the shared palette without a
/// thread-local.
///
/// Kept as a separate context from [`RenderSlintContext`] because the
/// Studio builder (which writes through [`SlintEmitter`]) and the
/// ad-hoc host-side Slint callers (which want typed values without
/// DSL generation) have different needs.
pub struct RenderContext<'a> {
    pub tokens: &'a prism_core::design_tokens::DesignTokens,
}

/// Slint-side render context used by the DSL emitter. Carries the
/// registry (so parents can recurse into their children by
/// `ComponentId`) plus the design tokens (so components can reference
/// the shared palette without a thread-local) and a running counter
/// the walker uses to mint unique element ids.
///
/// Constructed fresh per document walk in
/// [`crate::render::render_document_slint_source`].
pub struct RenderSlintContext<'a> {
    pub tokens: &'a prism_core::design_tokens::DesignTokens,
    pub registry: &'a ComponentRegistry,
    pub resources:
        &'a indexmap::IndexMap<crate::resource::ResourceId, crate::resource::ResourceDef>,
    pub prefabs: &'a indexmap::IndexMap<String, PrefabDef>,
    pub facets: &'a indexmap::IndexMap<String, FacetDef>,
    pub facet_schemas: &'a indexmap::IndexMap<String, crate::facet::FacetSchema>,
    /// When true, emit `// @node-start` / `// @node-end` marker
    /// comments around each node for source map construction (ADR-006).
    pub emit_markers: bool,
    /// Style overrides for the node currently being rendered. Set by
    /// `render_child` before calling `Component::render_slint` so
    /// components can apply cascaded style properties (font-size,
    /// color, etc.) into their Slint output.
    current_style: RefCell<StyleProperties>,
    /// Pre-resolved asset paths: VFS hash → absolute filesystem path.
    /// Set by the host before rendering so VFS-backed images can emit
    /// `@image-url()` references the Slint interpreter can resolve.
    pub asset_paths: HashMap<String, PathBuf>,
    /// When true, `render_child` skips children that would emit x/y
    /// (Absolute, Free, Relative with offset). These are rendered in a
    /// second pass outside the parent's layout element so the Slint
    /// compiler doesn't reject the x/y properties.
    skip_positioned: Cell<bool>,
    /// Pre-resolved widget data keyed by node ID. Populated by the
    /// shell's `resolve_widget_data` for nodes backed by
    /// `CoreWidgetComponent`s that declare a `data_query`. Merged
    /// into props in `render_child` before template rendering.
    pub widget_data: HashMap<String, Value>,
}

impl<'a> RenderSlintContext<'a> {
    pub fn new(
        tokens: &'a prism_core::design_tokens::DesignTokens,
        registry: &'a ComponentRegistry,
        resources: &'a indexmap::IndexMap<
            crate::resource::ResourceId,
            crate::resource::ResourceDef,
        >,
        prefabs: &'a indexmap::IndexMap<String, PrefabDef>,
        facets: &'a indexmap::IndexMap<String, FacetDef>,
        facet_schemas: &'a indexmap::IndexMap<String, crate::facet::FacetSchema>,
        emit_markers: bool,
    ) -> Self {
        Self {
            tokens,
            registry,
            resources,
            prefabs,
            facets,
            facet_schemas,
            emit_markers,
            current_style: RefCell::new(StyleProperties::default()),
            asset_paths: HashMap::new(),
            skip_positioned: Cell::new(false),
            widget_data: HashMap::new(),
        }
    }

    /// Read the style overrides for the node currently being rendered.
    /// Components call this inside `render_slint` to apply cascaded
    /// style properties (font-size, color, etc.) instead of hardcoded
    /// defaults.
    pub fn style(&self) -> StyleProperties {
        self.current_style.borrow().clone()
    }

    pub fn child_emits_position(child: &Node) -> bool {
        match &child.layout_mode {
            LayoutMode::Absolute(_) | LayoutMode::Free => true,
            LayoutMode::Relative(_) => child.transform.position != [0.0, 0.0],
            LayoutMode::Flow(_) => false,
        }
    }

    pub fn render_child(&self, child: &Node, out: &mut SlintEmitter) -> Result<(), RenderError> {
        if child.props.get("visible").and_then(|v| v.as_bool()) == Some(false) {
            return Ok(());
        }
        if self.skip_positioned.get() && Self::child_emits_position(child) {
            return Ok(());
        }
        if self.emit_markers {
            out.line(format!("// @node-start:{}:{}", child.id, child.component));
            if let Some(f) = child.layout_mode.flow_props() {
                if let (GridPlacement::Line { index: col }, GridPlacement::Line { index: row }) =
                    (&f.grid_column, &f.grid_row)
                {
                    out.line(format!("// @grid:{},{}", col - 1, row - 1));
                }
            }
        }

        let component = self
            .registry
            .get(&child.component)
            .ok_or_else(|| RenderError::UnknownComponent(child.component.clone()))?;

        let mut props = crate::resource::resolve_resource_refs(&child.props, self.resources);
        if let Some(data) = self.widget_data.get(&child.id) {
            if props.is_null() {
                props = Value::Object(Default::default());
            }
            if let (Some(target), Some(source)) = (props.as_object_mut(), data.as_object()) {
                for (k, v) in source {
                    target.insert(k.clone(), v.clone());
                }
            }
        }
        let props = crate::variant::apply_variant_defaults(&props, &component.variants());

        *self.current_style.borrow_mut() = child.style.clone();

        let needs_layout_wrapper = match &child.layout_mode {
            LayoutMode::Flow(f) => !f.is_default(),
            LayoutMode::Free => true,
            LayoutMode::Absolute(_) => true,
            LayoutMode::Relative(f) => !f.is_default() || child.transform.position != [0.0, 0.0],
        };
        let needs_style_wrapper = child.style.has_background_or_border();
        let needs_scale_wrapper = Self::has_scale(&child.transform);

        let render_inner = |ctx: &Self, out: &mut SlintEmitter| -> Result<(), RenderError> {
            if needs_scale_wrapper {
                out.block("Rectangle", |out| {
                    Self::emit_scale(&child.transform, out);
                    out.line("horizontal-stretch: 1;");
                    out.line("vertical-stretch: 1;");
                    ctx.render_component_inner(
                        &*component,
                        &props,
                        &child.modifiers,
                        &child.children,
                        out,
                    )
                })
            } else {
                ctx.render_component_inner(
                    &*component,
                    &props,
                    &child.modifiers,
                    &child.children,
                    out,
                )
            }
        };

        if needs_layout_wrapper {
            let wrapper_element = self.layout_wrapper_element(&child.layout_mode);
            out.block(&wrapper_element, |out| {
                self.emit_layout_props(&child.layout_mode, &child.transform, out);
                if needs_style_wrapper {
                    out.block("Rectangle", |out| {
                        self.emit_style_wrapper_props(&child.style, out);
                        render_inner(self, out)
                    })
                } else {
                    render_inner(self, out)
                }
            })?;
        } else if needs_style_wrapper {
            out.block("Rectangle", |out| {
                self.emit_style_wrapper_props(&child.style, out);
                render_inner(self, out)
            })?;
        } else {
            render_inner(self, out)?;
        }

        if self.emit_markers {
            out.line(format!("// @node-end:{}", child.id));
        }

        Ok(())
    }

    fn render_component_inner(
        &self,
        component: &dyn Component,
        props: &Value,
        modifiers: &[crate::modifier::Modifier],
        children: &[Node],
        out: &mut SlintEmitter,
    ) -> Result<(), RenderError> {
        let has_positioned_children = children.iter().any(Self::child_emits_position);

        if !has_positioned_children {
            if modifiers.is_empty() {
                component.render_slint(self, props, children, out)
            } else {
                self.apply_slint_modifiers(modifiers, 0, props, children, component, out)
            }
        } else {
            // Wrap in Rectangle so flow children live inside the
            // component's layout, and positioned children live
            // outside it (as siblings). Slint layout elements reject
            // x/y on their children, so positioned nodes must not
            // be inside VerticalLayout / HorizontalLayout / GridLayout.
            out.block("Rectangle", |out| {
                out.line("horizontal-stretch: 1;");
                out.line("vertical-stretch: 1;");
                self.skip_positioned.set(true);
                let inner = if modifiers.is_empty() {
                    component.render_slint(self, props, children, out)
                } else {
                    self.apply_slint_modifiers(modifiers, 0, props, children, component, out)
                };
                self.skip_positioned.set(false);
                inner?;
                for child in children {
                    if Self::child_emits_position(child) {
                        self.render_child(child, out)?;
                    }
                }
                Ok(())
            })
        }
    }

    fn layout_wrapper_element(&self, layout_mode: &LayoutMode) -> String {
        match layout_mode {
            LayoutMode::Flow(f) | LayoutMode::Relative(f) => match f.display {
                FlowDisplay::Flex => match f.flex_direction {
                    FlexDirection::Row | FlexDirection::RowReverse => "HorizontalLayout".into(),
                    FlexDirection::Column | FlexDirection::ColumnReverse => "VerticalLayout".into(),
                },
                FlowDisplay::Grid => "GridLayout".into(),
                _ => "VerticalLayout".into(),
            },
            LayoutMode::Free | LayoutMode::Absolute(_) => "Rectangle".into(),
        }
    }

    fn emit_layout_props(
        &self,
        layout_mode: &LayoutMode,
        transform: &Transform2D,
        out: &mut SlintEmitter,
    ) {
        match layout_mode {
            LayoutMode::Flow(f) => {
                self.emit_flow_props(f, out);
            }
            LayoutMode::Relative(f) => {
                if transform.position != [0.0, 0.0] {
                    out.prop_px("x", transform.position[0] as f64);
                    out.prop_px("y", transform.position[1] as f64);
                }
                self.emit_flow_props(f, out);
            }
            LayoutMode::Absolute(abs) => {
                out.prop_px("x", transform.position[0] as f64);
                out.prop_px("y", transform.position[1] as f64);
                self.emit_dimension("width", abs.width, out);
                self.emit_dimension("height", abs.height, out);
            }
            LayoutMode::Free => {
                out.prop_px("x", transform.position[0] as f64);
                out.prop_px("y", transform.position[1] as f64);
            }
        }
        Self::emit_rotation(transform, out);
    }

    fn emit_rotation(transform: &Transform2D, out: &mut SlintEmitter) {
        if transform.rotation != 0.0 {
            out.property(
                "transform-rotation",
                format!("{}deg", transform.rotation.to_degrees()),
            );
        }
    }

    fn has_scale(transform: &Transform2D) -> bool {
        transform.scale[0] != 1.0 || transform.scale[1] != 1.0
    }

    fn emit_scale(transform: &Transform2D, out: &mut SlintEmitter) {
        if transform.scale[0] != 1.0 {
            out.prop_float("transform-scale-x", transform.scale[0] as f64);
        }
        if transform.scale[1] != 1.0 {
            out.prop_float("transform-scale-y", transform.scale[1] as f64);
        }
    }

    fn emit_flow_props(&self, f: &FlowProps, out: &mut SlintEmitter) {
        if f.gap != 0.0 && f.display == FlowDisplay::Flex {
            out.prop_px("spacing", f.gap as f64);
        }
        self.emit_dimension("preferred-width", f.width, out);
        self.emit_dimension("preferred-height", f.height, out);
        if f.padding.top != 0.0 {
            out.prop_px("padding-top", f.padding.top as f64);
        }
        if f.padding.right != 0.0 {
            out.prop_px("padding-right", f.padding.right as f64);
        }
        if f.padding.bottom != 0.0 {
            out.prop_px("padding-bottom", f.padding.bottom as f64);
        }
        if f.padding.left != 0.0 {
            out.prop_px("padding-left", f.padding.left as f64);
        }
        if f.display == FlowDisplay::Flex {
            out.line("alignment: start;");
        }
    }

    fn emit_dimension(&self, key: &str, dim: Dimension, out: &mut SlintEmitter) {
        match dim {
            Dimension::Auto => {}
            Dimension::Px { value } => {
                out.prop_px(key, value as f64);
            }
            Dimension::Percent { value } => {
                out.property(key, format!("{value}%"));
            }
        }
    }

    fn emit_style_wrapper_props(&self, style: &StyleProperties, out: &mut SlintEmitter) {
        if let Some(ref bg) = style.background {
            out.prop_color("background", bg);
        }
        if let Some(radius) = style.border_radius {
            out.prop_px("border-radius", radius as f64);
        }
        out.line("horizontal-stretch: 1;");
        out.line("vertical-stretch: 1;");
    }

    pub fn render_children(
        &self,
        children: &[Node],
        out: &mut SlintEmitter,
    ) -> Result<(), RenderError> {
        for child in children {
            self.render_child(child, out)?;
        }
        Ok(())
    }

    fn apply_slint_modifiers(
        &self,
        modifiers: &[crate::modifier::Modifier],
        index: usize,
        props: &serde_json::Value,
        children: &[Node],
        component: &dyn Component,
        out: &mut SlintEmitter,
    ) -> Result<(), RenderError> {
        if index >= modifiers.len() {
            return component.render_slint(self, props, children, out);
        }
        let modifier = &modifiers[index];
        match modifier.kind {
            crate::modifier::ModifierKind::ScrollOverflow => out.block("Flickable", |out| {
                self.apply_slint_modifiers(modifiers, index + 1, props, children, component, out)
            }),
            _ => self.apply_slint_modifiers(modifiers, index + 1, props, children, component, out),
        }
    }
}

/// The core Slint-side contract. Trait-objects of this type live in the
/// registry; each node in the builder document is dispatched through
/// whichever impl the registry hands back for its `ComponentId`.
pub trait Component: Send + Sync {
    fn id(&self) -> &ComponentId;

    /// Typed schema for the Studio property panel. Returns the ordered
    /// list of fields the component expects in a node's `props` map;
    /// the panel renders one editor per entry using the factories in
    /// [`crate::registry`].
    fn schema(&self) -> Vec<FieldSpec>;

    fn help_entry(&self) -> Option<HelpEntry> {
        None
    }

    fn signals(&self) -> Vec<SignalDef> {
        crate::signal::common_signals()
    }

    fn variants(&self) -> Vec<VariantAxis> {
        vec![]
    }

    fn toolbar_actions(&self) -> Vec<ToolbarAction> {
        vec![]
    }

    /// Paint the component as `.slint` DSL into a shared
    /// [`SlintEmitter`]. The default impl emits a semantically
    /// transparent `Rectangle { }` wrapper and recurses into children.
    /// Override to produce bespoke Slint markup.
    fn render_slint(
        &self,
        ctx: &RenderSlintContext<'_>,
        props: &Value,
        children: &[Node],
        out: &mut SlintEmitter,
    ) -> Result<(), RenderError> {
        let _ = props;
        let id = self.id().clone();
        out.block(format!("// component: {id}\nRectangle"), |out| {
            ctx.render_children(children, out)
        })?;
        Ok(())
    }
}
