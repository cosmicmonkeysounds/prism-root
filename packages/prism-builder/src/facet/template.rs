//! Templates, outputs, and the `FacetDef` glue.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::component::ComponentId;
use crate::document::{Node, NodeId};
use crate::resource::ResourceId;

use super::*;

/// How a facet renders each data item (or a single computed value).
///
/// `Inline` owns a `Node` subtree directly — `{{field}}` expressions in
/// its props are resolved against each data item at render time.
/// `ComponentRef` points to a registered component (backward-compatible
/// with the existing prefab pipeline).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum FacetTemplate {
    /// Reference a registered component or prefab by ID.
    ComponentRef { component_id: ComponentId },
    /// Inline node subtree owned by this facet. Editable in context on
    /// the builder canvas. Bindings are `{{record.field}}` expressions
    /// in node props — no separate binding list needed.
    Inline { root: Box<Node> },
}

impl Default for FacetTemplate {
    fn default() -> Self {
        Self::ComponentRef {
            component_id: "card".into(),
        }
    }
}

/// Whether a facet repeats a template per data item or binds a single
/// scalar value directly to a target widget's prop.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum FacetOutput {
    #[default]
    Repeated,
    Scalar {
        target_node: NodeId,
        target_prop: String,
    },
}

// ── FacetDef ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FacetDef {
    pub id: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(default)]
    pub kind: FacetKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema_id: Option<FacetSchemaId>,
    #[serde(default)]
    pub template: FacetTemplate,
    #[serde(default)]
    pub output: FacetOutput,
    #[serde(default)]
    pub data: FacetDataSource,
    #[serde(default)]
    pub bindings: Vec<FacetBinding>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variant_rules: Vec<FacetVariantRule>,
    #[serde(default)]
    pub layout: FacetLayout,
    #[serde(skip)]
    pub resolved_data: Option<Vec<Value>>,
}

/// Result of resolving a facet's data. `Items` produces N prefab instances;
/// `Single` produces one instance with the aggregate value bound.
pub enum ResolvedFacetData {
    Items(Vec<Value>),
    Single(Value),
}

impl FacetDef {
    /// Return the component ID for `ComponentRef` templates, or `None` for inline.
    pub fn effective_component_id(&self) -> Option<&str> {
        match &self.template {
            FacetTemplate::ComponentRef { component_id } => Some(component_id),
            FacetTemplate::Inline { .. } => None,
        }
    }

    /// Mutably access the component ID inside a `ComponentRef` template.
    pub fn set_component_id(&mut self, id: &str) {
        if let FacetTemplate::ComponentRef { component_id } = &mut self.template {
            *component_id = id.to_string();
        }
    }

    /// Returns true if this facet uses an inline template.
    pub fn is_inline(&self) -> bool {
        matches!(self.template, FacetTemplate::Inline { .. })
    }

    /// Returns true if this facet produces scalar output.
    pub fn is_scalar(&self) -> bool {
        matches!(self.output, FacetOutput::Scalar { .. })
    }

    /// Resolve this facet's data items based on its kind.
    /// ObjectQuery, Script, and Lookup use `resolved_data` if the shell
    /// pre-populated it; otherwise they return empty.
    /// When `facet_schemas` is provided and this facet has a `schema_id`,
    /// `Calculation` fields in the schema are evaluated against each item.
    pub fn resolve_items(
        &self,
        resources: &IndexMap<ResourceId, crate::resource::ResourceDef>,
        facet_schemas: &IndexMap<FacetSchemaId, FacetSchema>,
    ) -> ResolvedFacetData {
        let schema = self.schema_id.as_ref().and_then(|id| facet_schemas.get(id));

        match &self.kind {
            FacetKind::List => {
                let mut items = self.data.resolve(resources);
                if let Some(s) = schema {
                    evaluate_calculations(&mut items, s);
                }
                ResolvedFacetData::Items(items)
            }
            FacetKind::ObjectQuery { .. } | FacetKind::Script { .. } | FacetKind::Lookup { .. } => {
                let mut items = self.resolved_data.clone().unwrap_or_default();
                if let Some(s) = schema {
                    evaluate_calculations(&mut items, s);
                }
                ResolvedFacetData::Items(items)
            }
            FacetKind::Aggregate { operation, field } => {
                let mut items = self.data.resolve(resources);
                if let Some(s) = schema {
                    evaluate_calculations(&mut items, s);
                }
                let result = apply_aggregate(&items, operation, field.as_deref());
                ResolvedFacetData::Single(result)
            }
        }
    }
}
