//! Internal helpers to keep bundle files declarative.
//!
//! All six built-in bundles follow the same mechanical shape: build a
//! list of `EntityDef` + `EdgeTypeDef` + `FluxAutomationPreset` +
//! `PrismPlugin` and fan them into the registries. These helpers cut
//! the line-noise by letting each bundle write field defs as one-liners
//! and edge defs as struct literals with sensible defaults.

use serde_json::Value as JsonValue;

use crate::foundation::object_model::types::DefaultChildView;
use crate::foundation::object_model::{
    EdgeBehavior, EdgeTypeDef, EntityDef, EntityFieldDef, EntityFieldType, EnumOption, UiHints,
};

pub(super) fn enum_options(pairs: &[(&str, &str)]) -> Vec<EnumOption> {
    pairs
        .iter()
        .map(|(v, l)| EnumOption {
            value: (*v).into(),
            label: (*l).into(),
        })
        .collect()
}

pub(super) struct Field {
    id: &'static str,
    field_type: EntityFieldType,
    label: Option<&'static str>,
    required: Option<bool>,
    default: Option<JsonValue>,
    expression: Option<&'static str>,
    enum_options: Option<Vec<EnumOption>>,
    ref_types: Option<Vec<String>>,
    ui: Option<UiHints>,
}

impl Field {
    pub(super) fn new(id: &'static str, field_type: EntityFieldType) -> Self {
        Self {
            id,
            field_type,
            label: None,
            required: None,
            default: None,
            expression: None,
            enum_options: None,
            ref_types: None,
            ui: None,
        }
    }

    pub(super) fn label(mut self, label: &'static str) -> Self {
        self.label = Some(label);
        self
    }

    pub(super) fn required(mut self) -> Self {
        self.required = Some(true);
        self
    }

    pub(super) fn default(mut self, value: JsonValue) -> Self {
        self.default = Some(value);
        self
    }

    pub(super) fn expression(mut self, expr: &'static str) -> Self {
        self.expression = Some(expr);
        self
    }

    pub(super) fn enum_values(mut self, values: Vec<EnumOption>) -> Self {
        self.enum_options = Some(values);
        self
    }

    pub(super) fn ref_types<I, S>(mut self, types: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.ref_types = Some(types.into_iter().map(Into::into).collect());
        self
    }

    pub(super) fn ui(mut self, ui: UiHints) -> Self {
        self.ui = Some(ui);
        self
    }

    pub(super) fn build(self) -> EntityFieldDef {
        EntityFieldDef {
            id: self.id.into(),
            field_type: self.field_type,
            label: self.label.map(Into::into),
            description: None,
            required: self.required,
            default: self.default,
            expression: self.expression.map(Into::into),
            enum_options: self.enum_options,
            ref_types: self.ref_types,
            lookup_relation: None,
            lookup_field: None,
            rollup_relation: None,
            rollup_field: None,
            rollup_function: None,
            ui: self.ui,
        }
    }
}

pub(super) fn ui_multiline() -> UiHints {
    UiHints {
        multiline: Some(true),
        ..UiHints::default()
    }
}

pub(super) fn ui_multiline_group(group: &str) -> UiHints {
    UiHints {
        multiline: Some(true),
        group: Some(group.into()),
        ..UiHints::default()
    }
}

pub(super) fn ui_readonly() -> UiHints {
    UiHints {
        readonly: Some(true),
        ..UiHints::default()
    }
}

pub(super) fn ui_readonly_multiline() -> UiHints {
    UiHints {
        multiline: Some(true),
        readonly: Some(true),
        ..UiHints::default()
    }
}

pub(super) fn ui_hidden() -> UiHints {
    UiHints {
        hidden: Some(true),
        ..UiHints::default()
    }
}

pub(super) fn ui_placeholder(text: &str) -> UiHints {
    UiHints {
        placeholder: Some(text.into()),
        ..UiHints::default()
    }
}

pub(super) struct EntitySpec {
    pub type_name: &'static str,
    pub nsid: &'static str,
    pub category: &'static str,
    pub label: &'static str,
    pub plural_label: &'static str,
    pub default_child_view: Option<DefaultChildView>,
    pub child_only: bool,
    pub extra_child_types: Option<Vec<String>>,
    pub fields: Vec<EntityFieldDef>,
}

pub(super) fn entity_def(spec: EntitySpec) -> EntityDef {
    EntityDef {
        type_name: spec.type_name.into(),
        nsid: Some(spec.nsid.into()),
        category: spec.category.into(),
        label: spec.label.into(),
        plural_label: Some(spec.plural_label.into()),
        description: None,
        color: None,
        default_child_view: spec.default_child_view,
        tabs: None,
        child_only: if spec.child_only { Some(true) } else { None },
        extra_child_types: spec.extra_child_types,
        extra_parent_types: None,
        fields: Some(spec.fields),
        api: None,
    }
}

pub(super) struct EdgeSpec {
    pub relation: &'static str,
    pub nsid: &'static str,
    pub label: &'static str,
    pub behavior: EdgeBehavior,
    pub source_types: Vec<String>,
    pub target_types: Option<Vec<String>>,
    pub description: Option<&'static str>,
    pub suggest_inline: bool,
    pub undirected: bool,
}

pub(super) fn edge_def(spec: EdgeSpec) -> EdgeTypeDef {
    EdgeTypeDef {
        relation: spec.relation.into(),
        nsid: Some(spec.nsid.into()),
        label: spec.label.into(),
        description: spec.description.map(Into::into),
        behavior: Some(spec.behavior),
        undirected: if spec.undirected { Some(true) } else { None },
        allow_multiple: None,
        cascade: None,
        suggest_inline: if spec.suggest_inline {
            Some(true)
        } else {
            None
        },
        color: None,
        source_types: Some(spec.source_types),
        source_categories: None,
        target_types: spec.target_types,
        target_categories: None,
        scope: None,
    }
}

pub(super) fn owned_strings<I, S>(iter: I) -> Vec<String>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    iter.into_iter().map(Into::into).collect()
}
