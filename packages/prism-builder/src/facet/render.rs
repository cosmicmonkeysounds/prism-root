//! `FacetComponent` — facet block Slint emitter.

use serde_json::Value;

use prism_core::help::HelpEntry;

use crate::component::{Component, ComponentId, RenderError, RenderSlintContext};
use crate::document::Node;
use crate::prefab::apply_prop_to_node;
use crate::registry::FieldSpec;
use crate::signal::{common_signals, SignalDef};
use crate::slint_source::SlintEmitter;

use super::*;

pub struct FacetComponent {
    pub id: ComponentId,
}

impl FacetComponent {
    pub fn new() -> Self {
        Self { id: "facet".into() }
    }
}

impl Default for FacetComponent {
    fn default() -> Self {
        Self::new()
    }
}

impl Component for FacetComponent {
    fn id(&self) -> &ComponentId {
        &self.id
    }

    fn schema(&self) -> Vec<FieldSpec> {
        crate::schemas::facet()
    }

    fn help_entry(&self) -> Option<HelpEntry> {
        Some(HelpEntry::new(
            "builder.components.facet",
            "Facet",
            "Programmatic list: expands a prefab template once per item in a data source.",
        ))
    }

    fn signals(&self) -> Vec<SignalDef> {
        common_signals()
    }

    fn render_slint(
        &self,
        ctx: &RenderSlintContext<'_>,
        props: &Value,
        _children: &[Node],
        out: &mut SlintEmitter,
    ) -> Result<(), RenderError> {
        let facet_id = props.get("facet_id").and_then(|v| v.as_str()).unwrap_or("");

        let facet = match ctx.facets.get(facet_id) {
            Some(f) => f,
            None => {
                let label = if facet_id.is_empty() {
                    "(no facet_id set)".to_string()
                } else {
                    format!("Facet: {facet_id} (not found)")
                };
                return out.block("Rectangle", |out| {
                    out.prop_px("preferred-height", 40.0);
                    out.line("background: #f0f0f0;");
                    out.block("Text", |out| {
                        out.prop_string("text", &label);
                        Ok(())
                    })
                });
            }
        };

        // ── Scalar output: no render, value binding handled at document level ──
        if let FacetOutput::Scalar { .. } = &facet.output {
            return Ok(());
        }

        let resolved = facet.resolve_items(ctx.resources, ctx.facet_schemas);

        match &facet.template {
            FacetTemplate::Inline { root: template } => {
                let items = match resolved {
                    ResolvedFacetData::Items(mut items) => {
                        if let Some(max) = props.get("max_items").and_then(|v| v.as_u64()) {
                            items.truncate(max as usize);
                        }
                        items
                    }
                    ResolvedFacetData::Single(val) => {
                        let mut root = template.clone();
                        resolve_template_expressions(&mut root, &val);
                        return ctx.render_child(&root, out);
                    }
                };
                let nodes: Vec<Node> = items
                    .iter()
                    .map(|item| {
                        let mut root = template.clone();
                        resolve_template_expressions(&mut root, item);
                        evaluate_variant_rules(&mut root, &facet.variant_rules, item);
                        *root
                    })
                    .collect();
                emit_facet_slint_layout(&facet.layout, &nodes, ctx, out)
            }
            FacetTemplate::ComponentRef { component_id } => {
                let prefab = ctx.prefabs.get(component_id.as_str()).ok_or_else(|| {
                    RenderError::Failed(format!("prefab '{component_id}' not found"))
                })?;
                let items = match resolved {
                    ResolvedFacetData::Items(mut items) => {
                        if let Some(max) = props.get("max_items").and_then(|v| v.as_u64()) {
                            items.truncate(max as usize);
                        }
                        items
                    }
                    ResolvedFacetData::Single(val) => {
                        let mut root = prefab.root.clone();
                        if let Some(first_binding) = facet.bindings.first() {
                            if let Some(slot) = prefab
                                .exposed
                                .iter()
                                .find(|s| s.key == first_binding.slot_key)
                            {
                                apply_prop_to_node(
                                    &mut root,
                                    &slot.target_node,
                                    &slot.target_prop,
                                    val,
                                );
                            }
                        }
                        return ctx.render_child(&root, out);
                    }
                };
                let nodes: Vec<Node> = items
                    .iter()
                    .map(|item| {
                        let mut root = prefab.root.clone();
                        apply_bindings(&mut root, prefab, &facet.bindings, item);
                        evaluate_variant_rules(&mut root, &facet.variant_rules, item);
                        root
                    })
                    .collect();
                emit_facet_slint_layout(&facet.layout, &nodes, ctx, out)
            }
        }
    }
}

/// Emit a list of prepared nodes into a Slint layout container.
///
/// When `columns` is set, items are chunked into rows of N and emitted
/// as a VerticalLayout of HorizontalLayouts (for row direction) or
/// HorizontalLayout of VerticalLayouts (for column direction), giving
/// a grid-like wrap effect.
fn emit_facet_slint_layout(
    layout: &FacetLayout,
    nodes: &[Node],
    ctx: &RenderSlintContext<'_>,
    out: &mut SlintEmitter,
) -> Result<(), RenderError> {
    let gap = layout.gap as f64;

    if let Some(cols) = layout.columns.filter(|&c| c > 1) {
        let cols = cols as usize;
        let (outer_tag, inner_tag) = match layout.direction {
            FacetDirection::Row => ("VerticalLayout", "HorizontalLayout"),
            FacetDirection::Column => ("HorizontalLayout", "VerticalLayout"),
        };
        out.block(outer_tag, |out| {
            if gap > 0.0 {
                out.prop_px("spacing", gap);
            }
            out.line("alignment: start;");
            for chunk in nodes.chunks(cols) {
                out.block(inner_tag, |out| {
                    if gap > 0.0 {
                        out.prop_px("spacing", gap);
                    }
                    out.line("alignment: start;");
                    for node in chunk {
                        ctx.render_child(node, out)?;
                    }
                    Ok(())
                })?;
            }
            Ok(())
        })
    } else {
        let layout_tag = match layout.direction {
            FacetDirection::Row => "HorizontalLayout",
            FacetDirection::Column => "VerticalLayout",
        };
        out.block(layout_tag, |out| {
            if gap > 0.0 {
                out.prop_px("spacing", gap);
            }
            out.line("alignment: start;");
            for node in nodes {
                ctx.render_child(node, out)?;
            }
            Ok(())
        })
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────
