//! Bridge from `prism_core::widget::WidgetContribution` to the builder's
//! unified [`Block`] trait.
//!
//! Core engines declare droppable widgets via pure-data
//! [`WidgetContribution`]s with no builder dependency. This module wraps
//! each contribution in a [`CoreWidgetBlock`] that implements [`Block`];
//! the blanket impl in `crate::block` derives the matching `Component`
//! impl so a single instance feeds the [`ComponentRegistry`].

use std::sync::Arc;

use prism_core::widget::{SignalSpec, ToolbarAction, VariantSpec, WidgetContribution};

use crate::block::Block;
use crate::component::ComponentId;
use crate::registry::{ComponentRegistry, FieldSpec, RegistryError};
use crate::signal::{with_common_signals, SignalDef};
use crate::variant::{VariantAxis, VariantOption};

// ── CoreWidgetBlock ─────────────────────────────────────────────

/// Wraps a [`WidgetContribution`] from a core engine and implements
/// the unified [`Block`] trait. The blanket impl in `crate::block`
/// derives a matching `Component` impl so the `Arc<CoreWidgetBlock>`
/// registers into [`ComponentRegistry`] directly.
pub struct CoreWidgetBlock {
    contribution: WidgetContribution,
}

impl CoreWidgetBlock {
    pub fn new(contribution: WidgetContribution) -> Self {
        Self { contribution }
    }

    pub fn contribution(&self) -> &WidgetContribution {
        &self.contribution
    }
}

impl Block for CoreWidgetBlock {
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

    /// Lower the contribution's `template.root` through the unified
    /// `TemplateNode` walker. The host node's props feed
    /// `DataBinding` / `Repeater` / `Conditional` / `Image` / `Link`
    /// field lookups, and its authored children surface through the
    /// `Children` template variant. Single seam — every core-engine
    /// widget shares the same lowering path as derive-emitted blocks.
    fn lower_ui(
        &self,
        ctx: &crate::ui_lower::LowerCtx<'_>,
        node: &crate::document::Node,
        style: &crate::style::StyleProperties,
    ) -> prism_ui_runtime::layout::Node {
        crate::template_lower::lower_template(
            ctx,
            &self.contribution.template.root,
            &node.props,
            &node.children,
            style,
            &node.id,
        )
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

// ── Registration ────────────────────────────────────────────────

/// Concatenates `widget_contributions()` from every listed provider
/// module. Explicit list (rather than an attribute scrape) keeps the
/// dependency graph auditable — adding a new engine is a one-line edit.
macro_rules! widget_providers {
    ($($call:expr),* $(,)?) => {{
        let mut __all: ::std::vec::Vec<::prism_core::widget::WidgetContribution> = ::std::vec::Vec::new();
        $( __all.extend($call); )*
        __all
    }};
}

/// Collect all widget contributions from core engines.
pub fn collect_all_contributions() -> Vec<WidgetContribution> {
    widget_providers![
        // Tier 1
        prism_core::domain::calendar::widget_contributions(),
        prism_core::domain::timekeeping::widget_contributions(),
        prism_core::domain::ledger::widget_contributions(),
        prism_core::domain::spreadsheet::widget_contributions(),
        prism_core::interaction::comments::widget_contributions(),
        prism_core::interaction::dashboard::widget_contributions(),
        // Tier 2
        prism_core::domain::habits::widget_contributions(),
        prism_core::domain::goals::widget_contributions(),
        prism_core::domain::fitness::widget_contributions(),
        prism_core::domain::reminders::widget_contributions(),
        prism_core::domain::crm::widget_contributions(),
        prism_core::domain::projects::widget_contributions(),
        prism_core::domain::focus_planner::widget_contributions(),
        // Views
        prism_core::widget::view_contributions(),
    ]
}

/// Wrap each core-engine [`WidgetContribution`] in a [`CoreWidgetBlock`]
/// and register it into the given [`ComponentRegistry`].
pub fn register_core_widgets(registry: &mut ComponentRegistry) -> Result<(), RegistryError> {
    for contribution in collect_all_contributions() {
        registry.register(Arc::new(CoreWidgetBlock::new(contribution)))?;
    }
    Ok(())
}

// ── Tests ───────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use prism_core::widget::{
        LayoutDirection, TemplateNode, VariantOptionSpec, WidgetCategory, WidgetSize,
        WidgetTemplate,
    };
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
        let comp = CoreWidgetBlock::new(test_contribution());
        assert_eq!(comp.id(), "test-widget");
    }

    #[test]
    fn schema_returns_config_fields() {
        let comp = CoreWidgetBlock::new(test_contribution());
        let schema = comp.schema();
        assert_eq!(schema.len(), 2);
        assert_eq!(schema[0].key, "title");
        assert_eq!(schema[1].key, "show_icon");
    }

    #[test]
    fn signals_maps_and_includes_common() {
        let comp = CoreWidgetBlock::new(test_contribution());
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
        let comp = CoreWidgetBlock::new(test_contribution());
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
        let comp = CoreWidgetBlock::new(test_contribution());
        let actions = Block::toolbar_actions(&comp);
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].id, "refresh");
        assert_eq!(actions[0].label, "Refresh");
    }

    #[test]
    fn collect_all_contributions_returns_all_engines() {
        let contributions = collect_all_contributions();
        assert_eq!(contributions.len(), 45);
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
    fn lower_ui_walks_template_through_template_lower() {
        use crate::registry::ComponentRegistry;
        use crate::style::StyleProperties;
        use crate::ui_lower::LowerCtx;
        use prism_ui_runtime::layout::Node as UiNode;

        let block = CoreWidgetBlock::new(test_contribution());
        let registry = ComponentRegistry::new();
        let style = StyleProperties::default();
        let ctx = LowerCtx::new(Some(&registry), &style);
        let host = crate::document::Node {
            id: "host".into(),
            component: "test-widget".into(),
            ..Default::default()
        };
        // Contribution template is a Vertical { gap=8, padding=12 }
        // container — `lower_ui` should walk that, not fall through
        // to `default_container`.
        let lowered = Block::lower_ui(&block, &ctx, &host, &style);
        match lowered {
            UiNode::Container { props, .. } => {
                assert_eq!(props.gap, 8.0);
                assert_eq!(props.padding.left, 12.0);
            }
            _ => panic!("expected container"),
        }
    }
}
