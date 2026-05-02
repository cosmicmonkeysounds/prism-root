//! Bridge from `prism_core::widget::WidgetContribution` to the builder's
//! `Component` trait.
//!
//! Core engines declare droppable widgets via pure-data
//! [`WidgetContribution`]s with no builder dependency. This module wraps
//! each contribution in a [`CoreWidgetComponent`] that implements
//! [`Component`] and renders through the existing Slint pipeline.
//!
//! [`register_core_widgets`] collects all engine contributions and feeds
//! them into the [`ComponentRegistry`].

use std::sync::Arc;

use serde_json::Value;

use prism_core::widget::{
    LayoutDirection, SignalSpec, TemplateNode, ToolbarAction, VariantSpec, WidgetContribution,
};

use crate::component::{Component, ComponentId, RenderError, RenderSlintContext};
use crate::document::Node;
use crate::registry::{ComponentRegistry, FieldSpec, RegistryError};
use crate::signal::{with_common_signals, SignalDef};
use crate::slint_source::SlintEmitter;
use crate::variant::{VariantAxis, VariantOption};

// ── CoreWidgetComponent ─────────────────────────────────────────

/// Wraps a [`WidgetContribution`] from a core engine and implements
/// [`Component`] so the builder can render it through the Slint pipeline.
pub struct CoreWidgetComponent {
    contribution: WidgetContribution,
}

impl CoreWidgetComponent {
    pub fn new(contribution: WidgetContribution) -> Self {
        Self { contribution }
    }
}

impl Component for CoreWidgetComponent {
    fn id(&self) -> &ComponentId {
        &self.contribution.id
    }

    fn schema(&self) -> Vec<FieldSpec> {
        self.contribution.config_fields.clone()
    }

    fn signals(&self) -> Vec<SignalDef> {
        let mapped: Vec<SignalDef> = self
            .contribution
            .signals
            .iter()
            .map(map_signal_spec)
            .collect();
        with_common_signals(mapped)
    }

    fn variants(&self) -> Vec<VariantAxis> {
        self.contribution
            .variants
            .iter()
            .map(map_variant_spec)
            .collect()
    }

    fn toolbar_actions(&self) -> Vec<ToolbarAction> {
        self.contribution.toolbar_actions.clone()
    }

    fn render_slint(
        &self,
        ctx: &RenderSlintContext<'_>,
        props: &Value,
        _children: &[Node],
        out: &mut SlintEmitter,
    ) -> Result<(), RenderError> {
        render_template_node(ctx, &self.contribution.template.root, props, out)
    }
}

// ── Mapping helpers ─────────────────────────────────────────────

fn map_signal_spec(spec: &SignalSpec) -> SignalDef {
    SignalDef {
        name: spec.name.clone(),
        description: spec.description.clone(),
        payload: spec.payload_fields.clone(),
    }
}

fn map_variant_spec(spec: &VariantSpec) -> VariantAxis {
    VariantAxis {
        key: spec.key.clone(),
        label: spec.label.clone(),
        options: spec
            .options
            .iter()
            .map(|o| VariantOption {
                value: o.value.clone(),
                label: o.label.clone(),
                overrides: o.overrides.clone(),
            })
            .collect(),
    }
}

// ── Template rendering ──────────────────────────────────────────

/// Recursively walk a [`TemplateNode`] tree and emit Slint DSL.
fn render_template_node(
    ctx: &RenderSlintContext<'_>,
    node: &TemplateNode,
    props: &Value,
    out: &mut SlintEmitter,
) -> Result<(), RenderError> {
    match node {
        TemplateNode::Container {
            direction,
            gap,
            padding,
            children,
        } => {
            let element = match direction {
                LayoutDirection::Horizontal => "HorizontalLayout",
                LayoutDirection::Vertical => "VerticalLayout",
            };
            out.block(element, |out| {
                if let Some(g) = gap {
                    out.prop_px("spacing", *g as f64);
                }
                if let Some(p) = padding {
                    let px = *p as f64;
                    out.prop_px("padding-top", px);
                    out.prop_px("padding-right", px);
                    out.prop_px("padding-bottom", px);
                    out.prop_px("padding-left", px);
                }
                for child in children {
                    render_template_node(ctx, child, props, out)?;
                }
                Ok(())
            })
        }

        TemplateNode::Component {
            component_id,
            props: template_props,
        } => {
            let merged = merge_props(props, template_props);
            let component = ctx
                .registry
                .get(component_id)
                .ok_or_else(|| RenderError::UnknownComponent(component_id.clone()))?;
            component.render_slint(ctx, &merged, &[], out)
        }

        TemplateNode::DataBinding {
            field,
            component_id,
            prop_key,
        } => {
            let value = props.get(field).cloned().unwrap_or(Value::Null);
            let binding_props = serde_json::json!({ prop_key: value });
            let component = ctx
                .registry
                .get(component_id)
                .ok_or_else(|| RenderError::UnknownComponent(component_id.clone()))?;
            component.render_slint(ctx, &binding_props, &[], out)
        }

        TemplateNode::Repeater {
            source,
            item_template,
            empty_label,
        } => {
            let items = props
                .get(source.as_str())
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            if items.is_empty() {
                let label = empty_label.as_deref().unwrap_or("No items");
                out.block("Text", |out| {
                    out.prop_string("text", label);
                    out.prop_px("font-size", 12.0);
                    out.prop_color("color", "#888888");
                    Ok(())
                })
            } else {
                for item in &items {
                    render_template_node(ctx, item_template, item, out)?;
                }
                Ok(())
            }
        }

        TemplateNode::Conditional {
            field,
            child,
            fallback,
        } => {
            let is_truthy = props
                .get(field)
                .map(|v| match v {
                    Value::Bool(b) => *b,
                    Value::Null => false,
                    Value::String(s) => !s.is_empty(),
                    Value::Number(n) => n.as_f64().unwrap_or(0.0) != 0.0,
                    _ => true,
                })
                .unwrap_or(false);

            if is_truthy {
                render_template_node(ctx, child, props, out)
            } else if let Some(fb) = fallback {
                render_template_node(ctx, fb, props, out)
            } else {
                Ok(())
            }
        }
    }
}

/// Merge instance `props` with template-level `template_props`.
/// Instance props take precedence.
fn merge_props(instance: &Value, template: &Value) -> Value {
    match (instance, template) {
        (Value::Object(inst), Value::Object(tmpl)) => {
            let mut merged = tmpl.clone();
            for (k, v) in inst {
                merged.insert(k.clone(), v.clone());
            }
            Value::Object(merged)
        }
        _ => instance.clone(),
    }
}

// ── Registration ────────────────────────────────────────────────

/// Collect all widget contributions from the six Tier 1 core engines.
pub fn collect_all_contributions() -> Vec<WidgetContribution> {
    let mut all = Vec::new();
    all.extend(prism_core::domain::calendar::widget_contributions());
    all.extend(prism_core::domain::timekeeping::widget_contributions());
    all.extend(prism_core::domain::ledger::widget_contributions());
    all.extend(prism_core::domain::spreadsheet::widget_contributions());
    all.extend(prism_core::interaction::comments::widget_contributions());
    all.extend(prism_core::interaction::dashboard::widget_contributions());
    all
}

/// Wrap each core-engine [`WidgetContribution`] in a
/// [`CoreWidgetComponent`] and register it into the given
/// [`ComponentRegistry`].
pub fn register_core_widgets(registry: &mut ComponentRegistry) -> Result<(), RegistryError> {
    for contribution in collect_all_contributions() {
        registry.register(Arc::new(CoreWidgetComponent::new(contribution)))?;
    }
    Ok(())
}

// ── Tests ───────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use prism_core::widget::{VariantOptionSpec, WidgetCategory, WidgetSize, WidgetTemplate};
    use serde_json::json;

    fn test_contribution() -> WidgetContribution {
        WidgetContribution {
            id: "test-widget".into(),
            label: "Test Widget".into(),
            description: "A test widget".into(),
            category: WidgetCategory::Display,
            config_fields: vec![
                FieldSpec::text("title", "Title"),
                FieldSpec::boolean("show_icon", "Show Icon"),
            ],
            signals: vec![SignalSpec::new("item-selected", "An item was selected")
                .with_payload(vec![FieldSpec::text("item_id", "Item ID")])],
            variants: vec![VariantSpec {
                key: "size".into(),
                label: "Size".into(),
                options: vec![
                    VariantOptionSpec {
                        value: "sm".into(),
                        label: "Small".into(),
                        overrides: json!({"height": 24}),
                    },
                    VariantOptionSpec {
                        value: "lg".into(),
                        label: "Large".into(),
                        overrides: json!({"height": 48}),
                    },
                ],
            }],
            toolbar_actions: vec![ToolbarAction::signal("refresh", "Refresh", "refresh-icon")],
            default_size: WidgetSize::new(2, 1),
            template: WidgetTemplate {
                root: TemplateNode::Container {
                    direction: LayoutDirection::Vertical,
                    gap: Some(8),
                    padding: Some(12),
                    children: vec![TemplateNode::Component {
                        component_id: "text".into(),
                        props: json!({"body": "Hello"}),
                    }],
                },
            },
            ..Default::default()
        }
    }

    #[test]
    fn id_returns_contribution_id() {
        let comp = CoreWidgetComponent::new(test_contribution());
        assert_eq!(comp.id(), "test-widget");
    }

    #[test]
    fn schema_returns_config_fields() {
        let comp = CoreWidgetComponent::new(test_contribution());
        let schema = comp.schema();
        assert_eq!(schema.len(), 2);
        assert_eq!(schema[0].key, "title");
        assert_eq!(schema[1].key, "show_icon");
    }

    #[test]
    fn signals_maps_and_includes_common() {
        let comp = CoreWidgetComponent::new(test_contribution());
        let signals = comp.signals();
        // Common signals (12) + 1 component-specific = 13
        assert_eq!(signals.len(), 13);
        let custom = signals.iter().find(|s| s.name == "item-selected").unwrap();
        assert_eq!(custom.description, "An item was selected");
        assert_eq!(custom.payload.len(), 1);
        assert_eq!(custom.payload[0].key, "item_id");
        // Common signals present
        assert!(signals.iter().any(|s| s.name == "clicked"));
        assert!(signals.iter().any(|s| s.name == "hovered"));
    }

    #[test]
    fn variants_maps_correctly() {
        let comp = CoreWidgetComponent::new(test_contribution());
        let variants = comp.variants();
        assert_eq!(variants.len(), 1);
        assert_eq!(variants[0].key, "size");
        assert_eq!(variants[0].label, "Size");
        assert_eq!(variants[0].options.len(), 2);
        assert_eq!(variants[0].options[0].value, "sm");
        assert_eq!(variants[0].options[0].label, "Small");
        assert_eq!(variants[0].options[0].overrides, json!({"height": 24}));
        assert_eq!(variants[0].options[1].value, "lg");
    }

    #[test]
    fn toolbar_actions_are_accessible() {
        let comp = CoreWidgetComponent::new(test_contribution());
        let actions = Component::toolbar_actions(&comp);
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].id, "refresh");
        assert_eq!(actions[0].label, "Refresh");
    }

    #[test]
    fn render_slint_container_with_component() {
        // Build a registry with the text component so the template
        // can resolve `component_id: "text"`.
        let mut registry = ComponentRegistry::new();
        crate::starter::register_builtins(&mut registry).unwrap();

        let tokens = prism_core::design_tokens::DesignTokens::default();
        let resources = indexmap::IndexMap::new();
        let prefabs = indexmap::IndexMap::new();
        let facets = indexmap::IndexMap::new();
        let facet_schemas = indexmap::IndexMap::new();
        let ctx = RenderSlintContext::new(
            &tokens,
            &registry,
            &resources,
            &prefabs,
            &facets,
            &facet_schemas,
            false,
        );

        let comp = CoreWidgetComponent::new(test_contribution());
        let mut out = SlintEmitter::new();
        comp.render_slint(&ctx, &json!({}), &[], &mut out).unwrap();

        let source = out.build();
        assert!(source.contains("VerticalLayout"));
        assert!(source.contains("spacing: 8px"));
        assert!(source.contains("padding-top: 12px"));
        assert!(source.contains("Text"));
    }

    #[test]
    fn render_slint_data_binding() {
        let mut registry = ComponentRegistry::new();
        crate::starter::register_builtins(&mut registry).unwrap();

        let tokens = prism_core::design_tokens::DesignTokens::default();
        let resources = indexmap::IndexMap::new();
        let prefabs = indexmap::IndexMap::new();
        let facets = indexmap::IndexMap::new();
        let facet_schemas = indexmap::IndexMap::new();
        let ctx = RenderSlintContext::new(
            &tokens,
            &registry,
            &resources,
            &prefabs,
            &facets,
            &facet_schemas,
            false,
        );

        let contribution = WidgetContribution {
            id: "binding-test".into(),
            label: "Binding".into(),
            template: WidgetTemplate {
                root: TemplateNode::DataBinding {
                    field: "label".into(),
                    component_id: "text".into(),
                    prop_key: "body".into(),
                },
            },
            ..Default::default()
        };

        let comp = CoreWidgetComponent::new(contribution);
        let mut out = SlintEmitter::new();
        comp.render_slint(&ctx, &json!({"label": "Hello World"}), &[], &mut out)
            .unwrap();

        let source = out.build();
        assert!(source.contains("Text"));
        assert!(source.contains("Hello World"));
    }

    #[test]
    fn render_slint_repeater_empty() {
        let mut registry = ComponentRegistry::new();
        crate::starter::register_builtins(&mut registry).unwrap();

        let tokens = prism_core::design_tokens::DesignTokens::default();
        let resources = indexmap::IndexMap::new();
        let prefabs = indexmap::IndexMap::new();
        let facets = indexmap::IndexMap::new();
        let facet_schemas = indexmap::IndexMap::new();
        let ctx = RenderSlintContext::new(
            &tokens,
            &registry,
            &resources,
            &prefabs,
            &facets,
            &facet_schemas,
            false,
        );

        let contribution = WidgetContribution {
            id: "repeater-test".into(),
            label: "Repeater".into(),
            template: WidgetTemplate {
                root: TemplateNode::Repeater {
                    source: "items".into(),
                    item_template: Box::new(TemplateNode::Component {
                        component_id: "text".into(),
                        props: json!({"body": "item"}),
                    }),
                    empty_label: Some("No items yet".into()),
                },
            },
            ..Default::default()
        };

        let comp = CoreWidgetComponent::new(contribution);
        let mut out = SlintEmitter::new();
        comp.render_slint(&ctx, &json!({}), &[], &mut out).unwrap();

        let source = out.build();
        assert!(source.contains("Text"));
        assert!(source.contains("No items yet"));
    }

    #[test]
    fn render_slint_repeater_with_data() {
        let mut registry = ComponentRegistry::new();
        crate::starter::register_builtins(&mut registry).unwrap();

        let tokens = prism_core::design_tokens::DesignTokens::default();
        let resources = indexmap::IndexMap::new();
        let prefabs = indexmap::IndexMap::new();
        let facets = indexmap::IndexMap::new();
        let facet_schemas = indexmap::IndexMap::new();
        let ctx = RenderSlintContext::new(
            &tokens,
            &registry,
            &resources,
            &prefabs,
            &facets,
            &facet_schemas,
            false,
        );

        let contribution = WidgetContribution {
            id: "repeater-data".into(),
            label: "Repeater".into(),
            template: WidgetTemplate {
                root: TemplateNode::Repeater {
                    source: "items".into(),
                    item_template: Box::new(TemplateNode::DataBinding {
                        field: "title".into(),
                        component_id: "text".into(),
                        prop_key: "body".into(),
                    }),
                    empty_label: Some("No items".into()),
                },
            },
            ..Default::default()
        };

        let comp = CoreWidgetComponent::new(contribution);
        let mut out = SlintEmitter::new();
        let props = json!({
            "items": [
                {"title": "First"},
                {"title": "Second"},
                {"title": "Third"}
            ]
        });
        comp.render_slint(&ctx, &props, &[], &mut out).unwrap();

        let source = out.build();
        assert!(source.contains("First"));
        assert!(source.contains("Second"));
        assert!(source.contains("Third"));
        assert!(!source.contains("No items"));
    }

    #[test]
    fn render_slint_conditional_truthy() {
        let mut registry = ComponentRegistry::new();
        crate::starter::register_builtins(&mut registry).unwrap();

        let tokens = prism_core::design_tokens::DesignTokens::default();
        let resources = indexmap::IndexMap::new();
        let prefabs = indexmap::IndexMap::new();
        let facets = indexmap::IndexMap::new();
        let facet_schemas = indexmap::IndexMap::new();
        let ctx = RenderSlintContext::new(
            &tokens,
            &registry,
            &resources,
            &prefabs,
            &facets,
            &facet_schemas,
            false,
        );

        let contribution = WidgetContribution {
            id: "cond-test".into(),
            label: "Cond".into(),
            template: WidgetTemplate {
                root: TemplateNode::Conditional {
                    field: "show_icon".into(),
                    child: Box::new(TemplateNode::Component {
                        component_id: "text".into(),
                        props: json!({"body": "Visible"}),
                    }),
                    fallback: Some(Box::new(TemplateNode::Component {
                        component_id: "text".into(),
                        props: json!({"body": "Hidden"}),
                    })),
                },
            },
            ..Default::default()
        };

        let comp = CoreWidgetComponent::new(contribution);

        // Truthy case
        let mut out = SlintEmitter::new();
        comp.render_slint(&ctx, &json!({"show_icon": true}), &[], &mut out)
            .unwrap();
        let source = out.build();
        assert!(source.contains("Visible"));
        assert!(!source.contains("Hidden"));

        // Falsy case
        let mut out = SlintEmitter::new();
        comp.render_slint(&ctx, &json!({"show_icon": false}), &[], &mut out)
            .unwrap();
        let source = out.build();
        assert!(source.contains("Hidden"));
        assert!(!source.contains("Visible"));
    }

    #[test]
    fn render_slint_conditional_no_fallback() {
        let mut registry = ComponentRegistry::new();
        crate::starter::register_builtins(&mut registry).unwrap();

        let tokens = prism_core::design_tokens::DesignTokens::default();
        let resources = indexmap::IndexMap::new();
        let prefabs = indexmap::IndexMap::new();
        let facets = indexmap::IndexMap::new();
        let facet_schemas = indexmap::IndexMap::new();
        let ctx = RenderSlintContext::new(
            &tokens,
            &registry,
            &resources,
            &prefabs,
            &facets,
            &facet_schemas,
            false,
        );

        let contribution = WidgetContribution {
            id: "cond-no-fb".into(),
            label: "Cond".into(),
            template: WidgetTemplate {
                root: TemplateNode::Conditional {
                    field: "active".into(),
                    child: Box::new(TemplateNode::Component {
                        component_id: "text".into(),
                        props: json!({"body": "Active"}),
                    }),
                    fallback: None,
                },
            },
            ..Default::default()
        };

        let comp = CoreWidgetComponent::new(contribution);
        let mut out = SlintEmitter::new();
        comp.render_slint(&ctx, &json!({"active": false}), &[], &mut out)
            .unwrap();
        let source = out.build();
        assert!(source.is_empty());
    }

    #[test]
    fn default_contribution_renders_empty_layout() {
        let mut registry = ComponentRegistry::new();
        crate::starter::register_builtins(&mut registry).unwrap();

        let tokens = prism_core::design_tokens::DesignTokens::default();
        let resources = indexmap::IndexMap::new();
        let prefabs = indexmap::IndexMap::new();
        let facets = indexmap::IndexMap::new();
        let facet_schemas = indexmap::IndexMap::new();
        let ctx = RenderSlintContext::new(
            &tokens,
            &registry,
            &resources,
            &prefabs,
            &facets,
            &facet_schemas,
            false,
        );

        let c = WidgetContribution {
            id: "empty-widget".into(),
            ..Default::default()
        };
        let comp = CoreWidgetComponent::new(c);
        let mut out = SlintEmitter::new();
        comp.render_slint(&ctx, &json!({}), &[], &mut out).unwrap();
        let source = out.build();
        assert!(source.contains("VerticalLayout"));
    }

    #[test]
    fn collect_all_contributions_returns_all_engines() {
        let contributions = collect_all_contributions();
        assert_eq!(contributions.len(), 20);
        assert!(contributions.iter().any(|c| c.id == "calendar-month-view"));
        assert!(contributions.iter().any(|c| c.id == "stopwatch"));
        assert!(contributions
            .iter()
            .any(|c| c.id == "ledger-account-summary"));
        assert!(contributions
            .iter()
            .any(|c| c.id == "spreadsheet-data-table"));
        assert!(contributions.iter().any(|c| c.id == "comment-thread"));
        assert!(contributions.iter().any(|c| c.id == "dashboard-stats"));
    }

    #[test]
    fn register_core_widgets_populates_registry() {
        let mut registry = ComponentRegistry::new();
        register_core_widgets(&mut registry).unwrap();
        let count = collect_all_contributions().len();
        assert_eq!(registry.len(), count);
    }

    #[test]
    fn merge_props_template_plus_instance() {
        let instance = json!({"title": "Custom", "extra": 42});
        let template = json!({"title": "Default", "color": "blue"});
        let merged = merge_props(&instance, &template);
        // Instance wins on collision
        assert_eq!(merged["title"], "Custom");
        // Template key preserved
        assert_eq!(merged["color"], "blue");
        // Instance extra key preserved
        assert_eq!(merged["extra"], 42);
    }

    #[test]
    fn horizontal_container_emits_horizontal_layout() {
        let mut registry = ComponentRegistry::new();
        crate::starter::register_builtins(&mut registry).unwrap();

        let tokens = prism_core::design_tokens::DesignTokens::default();
        let resources = indexmap::IndexMap::new();
        let prefabs = indexmap::IndexMap::new();
        let facets = indexmap::IndexMap::new();
        let facet_schemas = indexmap::IndexMap::new();
        let ctx = RenderSlintContext::new(
            &tokens,
            &registry,
            &resources,
            &prefabs,
            &facets,
            &facet_schemas,
            false,
        );

        let contribution = WidgetContribution {
            id: "horiz-test".into(),
            label: "Horiz".into(),
            template: WidgetTemplate {
                root: TemplateNode::Container {
                    direction: LayoutDirection::Horizontal,
                    gap: Some(4),
                    padding: None,
                    children: vec![],
                },
            },
            ..Default::default()
        };

        let comp = CoreWidgetComponent::new(contribution);
        let mut out = SlintEmitter::new();
        comp.render_slint(&ctx, &json!({}), &[], &mut out).unwrap();
        let source = out.build();
        assert!(source.contains("HorizontalLayout"));
        assert!(source.contains("spacing: 4px"));
    }
}
