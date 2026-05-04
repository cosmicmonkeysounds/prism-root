#![allow(unused_imports)]

use prism_builder::card_prefab_def;
use prism_builder::layout::{
    AlignOption, Dimension, FlexDirection, FlowDisplay, GridPlacement, JustifyOption, LayoutMode,
};
use prism_builder::style::StyleProperties;
use prism_builder::{
    AggregateOp, FacetDataSource, FacetDirection, FacetKind, FacetOutput, FacetTemplate,
    AGGREGATE_OP_TAGS, FACET_KIND_TAGS,
};
use prism_builder::{
    BuilderDocument, ComponentRegistry, FieldKind, FieldSpec, FieldValue, Node, NodeId, PrismApp,
};
use prism_core::foundation::spatial::Transform2D;
use prism_core::help::HelpEntry;
use serde_json::Value;

use super::{
    dimension_slider, dimension_unit, format_align, format_anchor, format_display, format_f32,
    format_flex_direction, format_justify, format_number, layout_number, layout_select,
    placement_slider, placement_type, row_from_spec, style_rows_from, FieldRowData,
    PropertiesPanel, PropertySection, StyleOrigin,
};

impl PropertiesPanel {
    pub(super) fn facet_rows(doc: &BuilderDocument, node: &Node) -> Vec<FieldRowData> {
        let facet_id = node
            .props
            .get("facet_id")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let facet_ids: Vec<String> = doc.facets.keys().cloned().collect();

        let Some(def) = doc.facets.get(facet_id) else {
            return vec![FieldRowData {
                key: "facet.facet_id".into(),
                label: "Facet ID".into(),
                kind: "select".into(),
                value: facet_id.to_string(),
                required: true,
                min: 0.0,
                max: 0.0,
                has_bounds: false,
                options: facet_ids,
            }];
        };

        let schema = def
            .schema_id
            .as_ref()
            .and_then(|sid| doc.facet_schemas.get(sid));

        // Schema selection dropdown
        let mut schema_options: Vec<String> = vec!["(none)".into()];
        for sid in doc.facet_schemas.keys() {
            schema_options.push(sid.clone());
        }
        let schema_value = def.schema_id.as_deref().unwrap_or("(none)").to_string();

        // Prefab dropdown (for ComponentRef template)
        let mut prefab_options: Vec<String> = vec!["card".into()];
        for id in doc.prefabs.keys() {
            if id != "card" {
                prefab_options.push(id.clone());
            }
        }

        // Template type
        let template_value = match &def.template {
            FacetTemplate::ComponentRef { .. } => "component-ref",
            FacetTemplate::Inline { .. } => "inline",
        };

        // Output type
        let output_value = match &def.output {
            FacetOutput::Repeated => "repeated",
            FacetOutput::Scalar { .. } => "scalar",
        };

        // ── Common header (all kinds) ─────────────────────────
        let mut rows = vec![
            FieldRowData {
                key: "facet.kind".into(),
                label: "Kind".into(),
                kind: "select".into(),
                value: def.kind.tag().to_string(),
                required: true,
                min: 0.0,
                max: 0.0,
                has_bounds: false,
                options: FACET_KIND_TAGS.iter().map(|s| (*s).into()).collect(),
            },
            FieldRowData {
                key: "facet.schema_id".into(),
                label: "Schema".into(),
                kind: "select".into(),
                value: schema_value,
                required: false,
                min: 0.0,
                max: 0.0,
                has_bounds: false,
                options: schema_options,
            },
            FieldRowData {
                key: "facet.template_type".into(),
                label: "Template".into(),
                kind: "select".into(),
                value: template_value.to_string(),
                required: true,
                min: 0.0,
                max: 0.0,
                has_bounds: false,
                options: vec!["component-ref".into(), "inline".into()],
            },
        ];

        // Show prefab selector only for ComponentRef templates
        if matches!(def.template, FacetTemplate::ComponentRef { .. }) {
            rows.push(FieldRowData {
                key: "facet.component_id".into(),
                label: "Component template".into(),
                kind: "select".into(),
                value: def.effective_component_id().unwrap_or("card").to_string(),
                required: true,
                min: 0.0,
                max: 0.0,
                has_bounds: false,
                options: prefab_options,
            });
        }

        // Output type selector
        rows.push(FieldRowData {
            key: "facet.output_type".into(),
            label: "Output".into(),
            kind: "select".into(),
            value: output_value.to_string(),
            required: true,
            min: 0.0,
            max: 0.0,
            has_bounds: false,
            options: vec!["repeated".into(), "scalar".into()],
        });

        // Scalar target fields
        if let FacetOutput::Scalar {
            target_node,
            target_prop,
        } = &def.output
        {
            rows.push(FieldRowData {
                key: "facet.scalar_target_node".into(),
                label: "Target node".into(),
                kind: "text".into(),
                value: target_node.clone(),
                required: true,
                min: 0.0,
                max: 0.0,
                has_bounds: false,
                options: vec![],
            });
            rows.push(FieldRowData {
                key: "facet.scalar_target_prop".into(),
                label: "Target prop".into(),
                kind: "text".into(),
                value: target_prop.clone(),
                required: true,
                min: 0.0,
                max: 0.0,
                has_bounds: false,
                options: vec![],
            });
        }

        // ── Kind-specific sections ────────────────────────────
        match &def.kind {
            FacetKind::List => {
                Self::push_data_source_rows(&mut rows, def);
                Self::push_layout_rows(&mut rows, def);
            }
            FacetKind::ObjectQuery { query } => {
                let entity_type = query.object_type.as_deref().unwrap_or("");
                let filter_str = query
                    .filters
                    .first()
                    .map(|f| {
                        let op_str = match f.op {
                            prism_core::widget::FilterOp::Eq => "==",
                            prism_core::widget::FilterOp::Neq => "!=",
                            _ => "==",
                        };
                        let val = match &f.value {
                            serde_json::Value::String(s) => s.clone(),
                            other => other.to_string(),
                        };
                        format!("{} {} {}", f.field, op_str, val)
                    })
                    .unwrap_or_default();
                let sort_str = query
                    .sort
                    .first()
                    .map(|s| {
                        if s.descending {
                            format!("-{}", s.field)
                        } else {
                            s.field.clone()
                        }
                    })
                    .unwrap_or_default();
                rows.push(FieldRowData {
                    key: "facet.entity_type".into(),
                    label: "Entity type".into(),
                    kind: "text".into(),
                    value: entity_type.to_string(),
                    required: true,
                    min: 0.0,
                    max: 0.0,
                    has_bounds: false,
                    options: vec![],
                });
                rows.push(FieldRowData {
                    key: "facet.oq_filter".into(),
                    label: "Filter".into(),
                    kind: "text".into(),
                    value: filter_str,
                    required: false,
                    min: 0.0,
                    max: 0.0,
                    has_bounds: false,
                    options: vec![],
                });
                rows.push(FieldRowData {
                    key: "facet.oq_sort_by".into(),
                    label: "Sort by".into(),
                    kind: "text".into(),
                    value: sort_str,
                    required: false,
                    min: 0.0,
                    max: 0.0,
                    has_bounds: false,
                    options: vec![],
                });
                rows.push(FieldRowData {
                    key: "facet.oq_limit".into(),
                    label: "Limit".into(),
                    kind: "integer".into(),
                    value: query.limit.map(|l| l.to_string()).unwrap_or_default(),
                    required: false,
                    min: 0.0,
                    max: 10000.0,
                    has_bounds: true,
                    options: vec![],
                });
                Self::push_layout_rows(&mut rows, def);
            }
            FacetKind::Script {
                source,
                language,
                graph,
            } => {
                rows.push(FieldRowData {
                    key: "facet.script_language".into(),
                    label: "Language".into(),
                    kind: "select".into(),
                    value: match language {
                        prism_builder::ScriptLanguage::Luau => "luau".to_string(),
                        prism_builder::ScriptLanguage::VisualGraph => "visual-graph".to_string(),
                    },
                    required: false,
                    min: 0.0,
                    max: 0.0,
                    has_bounds: false,
                    options: vec!["luau".into(), "visual-graph".into()],
                });
                match language {
                    prism_builder::ScriptLanguage::Luau => {
                        rows.push(FieldRowData {
                            key: "facet.script_source".into(),
                            label: "Luau script".into(),
                            kind: "textarea".into(),
                            value: source.clone(),
                            required: false,
                            min: 0.0,
                            max: 0.0,
                            has_bounds: false,
                            options: vec![],
                        });
                    }
                    prism_builder::ScriptLanguage::VisualGraph => {
                        let node_count = graph.as_ref().map(|g| g.nodes.len()).unwrap_or(0);
                        let edge_count = graph.as_ref().map(|g| g.edges.len()).unwrap_or(0);
                        rows.push(FieldRowData {
                            key: "facet.graph_info".into(),
                            label: "Graph".into(),
                            kind: "text".into(),
                            value: format!("{node_count} nodes, {edge_count} edges"),
                            required: false,
                            min: 0.0,
                            max: 0.0,
                            has_bounds: false,
                            options: vec![],
                        });
                        if !source.is_empty() {
                            rows.push(FieldRowData {
                                key: "facet.graph_source_preview".into(),
                                label: "Compiled source".into(),
                                kind: "textarea".into(),
                                value: source.clone(),
                                required: false,
                                min: 0.0,
                                max: 0.0,
                                has_bounds: false,
                                options: vec![],
                            });
                        }
                    }
                }
                Self::push_layout_rows(&mut rows, def);
            }
            FacetKind::Aggregate { operation, field } => {
                Self::push_data_source_rows(&mut rows, def);
                rows.push(FieldRowData {
                    key: "facet.agg_operation".into(),
                    label: "Operation".into(),
                    kind: "select".into(),
                    value: operation.tag().to_string(),
                    required: true,
                    min: 0.0,
                    max: 0.0,
                    has_bounds: false,
                    options: AGGREGATE_OP_TAGS.iter().map(|s| (*s).into()).collect(),
                });
                if !matches!(operation, AggregateOp::Count) {
                    rows.push(FieldRowData {
                        key: "facet.agg_field".into(),
                        label: "Field".into(),
                        kind: "text".into(),
                        value: field.as_deref().unwrap_or("").into(),
                        required: true,
                        min: 0.0,
                        max: 0.0,
                        has_bounds: false,
                        options: vec![],
                    });
                }
                if let AggregateOp::Join { separator } = operation {
                    rows.push(FieldRowData {
                        key: "facet.agg_separator".into(),
                        label: "Separator".into(),
                        kind: "text".into(),
                        value: separator.clone(),
                        required: false,
                        min: 0.0,
                        max: 0.0,
                        has_bounds: false,
                        options: vec![],
                    });
                }
            }
            FacetKind::Lookup {
                source_entity,
                edge_type,
                target_entity,
            } => {
                rows.push(FieldRowData {
                    key: "facet.lookup_source".into(),
                    label: "Source entity".into(),
                    kind: "text".into(),
                    value: source_entity.clone(),
                    required: true,
                    min: 0.0,
                    max: 0.0,
                    has_bounds: false,
                    options: vec![],
                });
                rows.push(FieldRowData {
                    key: "facet.lookup_edge".into(),
                    label: "Edge type".into(),
                    kind: "text".into(),
                    value: edge_type.clone(),
                    required: true,
                    min: 0.0,
                    max: 0.0,
                    has_bounds: false,
                    options: vec![],
                });
                rows.push(FieldRowData {
                    key: "facet.lookup_target".into(),
                    label: "Target entity".into(),
                    kind: "text".into(),
                    value: target_entity.clone(),
                    required: true,
                    min: 0.0,
                    max: 0.0,
                    has_bounds: false,
                    options: vec![],
                });
                Self::push_layout_rows(&mut rows, def);
            }
        }

        // ── Bindings (all kinds) ──────────────────────────────
        Self::push_binding_rows(&mut rows, doc, def, schema);

        // ── Variant Rules ────────────────────────────────────
        Self::push_variant_rule_rows(&mut rows, def);

        // ── Records (List + static source + schema) ───────────
        if matches!(def.kind, FacetKind::List) {
            Self::push_record_rows(&mut rows, def, schema);
        }

        rows
    }

    fn push_data_source_rows(rows: &mut Vec<FieldRowData>, def: &prism_builder::FacetDef) {
        let (source_kind, item_count_label, source_id, filter_val, sort_val) = match &def.data {
            FacetDataSource::Static { items, records } => {
                let count = if records.is_empty() {
                    items.len()
                } else {
                    records.len()
                };
                (
                    "static",
                    format!("{count} items"),
                    String::new(),
                    String::new(),
                    String::new(),
                )
            }
            FacetDataSource::Resource { id } => (
                "resource",
                "resource".into(),
                id.clone(),
                String::new(),
                String::new(),
            ),
            FacetDataSource::Query { source, query } => {
                let f = query
                    .filters
                    .first()
                    .map(|qf| {
                        let op_str = match qf.op {
                            prism_core::widget::FilterOp::Eq => "==",
                            prism_core::widget::FilterOp::Neq => "!=",
                            _ => "==",
                        };
                        let val = match &qf.value {
                            serde_json::Value::String(s) => s.clone(),
                            other => other.to_string(),
                        };
                        format!("{} {} {}", qf.field, op_str, val)
                    })
                    .unwrap_or_default();
                let s = query
                    .sort
                    .first()
                    .map(|qs| {
                        if qs.descending {
                            format!("-{}", qs.field)
                        } else {
                            qs.field.clone()
                        }
                    })
                    .unwrap_or_default();
                ("query", "query".into(), source.clone(), f, s)
            }
        };

        rows.push(FieldRowData {
            key: "facet.source_kind".into(),
            label: "Data source".into(),
            kind: "select".into(),
            value: source_kind.to_string(),
            required: false,
            min: 0.0,
            max: 0.0,
            has_bounds: false,
            options: vec!["static".into(), "resource".into(), "query".into()],
        });

        if source_kind != "static" {
            rows.push(FieldRowData {
                key: "facet.source_id".into(),
                label: "Source resource ID".into(),
                kind: "text".into(),
                value: source_id,
                required: false,
                min: 0.0,
                max: 0.0,
                has_bounds: false,
                options: vec![],
            });
        }

        if source_kind == "query" {
            rows.push(FieldRowData {
                key: "facet.filter".into(),
                label: "Filter expression".into(),
                kind: "text".into(),
                value: filter_val,
                required: false,
                min: 0.0,
                max: 0.0,
                has_bounds: false,
                options: vec![],
            });
            rows.push(FieldRowData {
                key: "facet.sort_by".into(),
                label: "Sort by field".into(),
                kind: "text".into(),
                value: sort_val,
                required: false,
                min: 0.0,
                max: 0.0,
                has_bounds: false,
                options: vec![],
            });
        }

        rows.push(FieldRowData {
            key: "facet.item_count".into(),
            label: "Items".into(),
            kind: "text".into(),
            value: item_count_label,
            required: false,
            min: 0.0,
            max: 0.0,
            has_bounds: false,
            options: vec![],
        });
    }

    fn push_layout_rows(rows: &mut Vec<FieldRowData>, def: &prism_builder::FacetDef) {
        let direction_label = match def.layout.direction {
            FacetDirection::Row => "row",
            FacetDirection::Column => "column",
        };
        rows.extend([
            FieldRowData {
                key: "facet.direction".into(),
                label: "Direction".into(),
                kind: "select".into(),
                value: direction_label.to_string(),
                required: false,
                min: 0.0,
                max: 0.0,
                has_bounds: false,
                options: vec!["column".into(), "row".into()],
            },
            FieldRowData {
                key: "facet.gap".into(),
                label: "Gap (px)".into(),
                kind: "integer".into(),
                value: format!("{}", def.layout.gap as u32),
                required: false,
                min: 0.0,
                max: 128.0,
                has_bounds: true,
                options: vec![],
            },
            FieldRowData {
                key: "facet.wrap".into(),
                label: "Wrap".into(),
                kind: "boolean".into(),
                value: def.layout.wrap.to_string(),
                required: false,
                min: 0.0,
                max: 0.0,
                has_bounds: false,
                options: vec![],
            },
            FieldRowData {
                key: "facet.columns".into(),
                label: "Columns".into(),
                kind: "integer".into(),
                value: def
                    .layout
                    .columns
                    .map(|c| c.to_string())
                    .unwrap_or_default(),
                required: false,
                min: 0.0,
                max: 12.0,
                has_bounds: true,
                options: vec![],
            },
        ]);
    }

    fn push_binding_rows(
        rows: &mut Vec<FieldRowData>,
        doc: &BuilderDocument,
        def: &prism_builder::FacetDef,
        schema: Option<&prism_builder::FacetSchema>,
    ) {
        let cid = def.effective_component_id().unwrap_or("card");
        let prefab_exposed = if cid == "card" {
            doc.prefabs
                .get("card")
                .map(|p| p.exposed.clone())
                .unwrap_or_else(|| card_prefab_def().exposed)
        } else {
            doc.prefabs
                .get(cid)
                .map(|p| p.exposed.clone())
                .unwrap_or_default()
        };
        if prefab_exposed.is_empty() {
            return;
        }

        rows.push(FieldRowData {
            key: "facet.bindings_header".into(),
            label: "── Bindings ──".into(),
            kind: "text".into(),
            value: String::new(),
            required: false,
            min: 0.0,
            max: 0.0,
            has_bounds: false,
            options: vec![],
        });

        let schema_field_options: Vec<String> = if let Some(s) = schema {
            let mut opts = vec!["".into()];
            opts.extend(s.fields.iter().map(|f| f.key.clone()));
            opts
        } else {
            vec![]
        };

        for slot in &prefab_exposed {
            let bound_field = def
                .bindings
                .iter()
                .find(|b| b.slot_key == slot.key)
                .map(|b| b.item_field.clone())
                .unwrap_or_default();

            if schema_field_options.is_empty() {
                rows.push(FieldRowData {
                    key: format!("facet.binding.{}", slot.key),
                    label: format!("Bind: {}", slot.key),
                    kind: "text".into(),
                    value: bound_field,
                    required: false,
                    min: 0.0,
                    max: 0.0,
                    has_bounds: false,
                    options: vec![],
                });
            } else {
                rows.push(FieldRowData {
                    key: format!("facet.binding.{}", slot.key),
                    label: format!("Bind: {}", slot.key),
                    kind: "select".into(),
                    value: bound_field,
                    required: false,
                    min: 0.0,
                    max: 0.0,
                    has_bounds: false,
                    options: schema_field_options.clone(),
                });
            }
        }
    }

    fn push_variant_rule_rows(rows: &mut Vec<FieldRowData>, def: &prism_builder::FacetDef) {
        if def.variant_rules.is_empty() {
            rows.push(FieldRowData {
                key: "facet.add_variant_rule".into(),
                label: "+ Add variant rule".into(),
                kind: "text".into(),
                value: String::new(),
                required: false,
                min: 0.0,
                max: 0.0,
                has_bounds: false,
                options: vec![],
            });
            return;
        }

        rows.push(FieldRowData {
            key: "facet.variant_rules_header".into(),
            label: "── Variant Rules ──".into(),
            kind: "text".into(),
            value: String::new(),
            required: false,
            min: 0.0,
            max: 0.0,
            has_bounds: false,
            options: vec![],
        });

        for (i, rule) in def.variant_rules.iter().enumerate() {
            rows.push(FieldRowData {
                key: format!("facet.variant_rule.{i}.field"),
                label: "When field".into(),
                kind: "text".into(),
                value: rule.field.clone(),
                required: true,
                min: 0.0,
                max: 0.0,
                has_bounds: false,
                options: vec![],
            });
            rows.push(FieldRowData {
                key: format!("facet.variant_rule.{i}.value"),
                label: "equals".into(),
                kind: "text".into(),
                value: rule.value.clone(),
                required: true,
                min: 0.0,
                max: 0.0,
                has_bounds: false,
                options: vec![],
            });
            rows.push(FieldRowData {
                key: format!("facet.variant_rule.{i}.axis_key"),
                label: "set axis".into(),
                kind: "text".into(),
                value: rule.axis_key.clone(),
                required: true,
                min: 0.0,
                max: 0.0,
                has_bounds: false,
                options: vec![],
            });
            rows.push(FieldRowData {
                key: format!("facet.variant_rule.{i}.axis_value"),
                label: "to".into(),
                kind: "text".into(),
                value: rule.axis_value.clone(),
                required: true,
                min: 0.0,
                max: 0.0,
                has_bounds: false,
                options: vec![],
            });
            rows.push(FieldRowData {
                key: format!("facet.remove_variant_rule.{i}"),
                label: "Remove rule".into(),
                kind: "text".into(),
                value: String::new(),
                required: false,
                min: 0.0,
                max: 0.0,
                has_bounds: false,
                options: vec![],
            });
        }

        rows.push(FieldRowData {
            key: "facet.add_variant_rule".into(),
            label: "+ Add variant rule".into(),
            kind: "text".into(),
            value: String::new(),
            required: false,
            min: 0.0,
            max: 0.0,
            has_bounds: false,
            options: vec![],
        });
    }

    fn push_record_rows(
        rows: &mut Vec<FieldRowData>,
        def: &prism_builder::FacetDef,
        schema: Option<&prism_builder::FacetSchema>,
    ) {
        let FacetDataSource::Static { records, .. } = &def.data else {
            return;
        };
        let Some(s) = schema else {
            return;
        };
        if records.is_empty() {
            return;
        }

        rows.push(FieldRowData {
            key: "facet.records_header".into(),
            label: "── Records ──".into(),
            kind: "text".into(),
            value: String::new(),
            required: false,
            min: 0.0,
            max: 0.0,
            has_bounds: false,
            options: vec![],
        });
        for (ri, rec) in records.iter().enumerate() {
            rows.push(FieldRowData {
                key: format!("facet.record_header.{ri}"),
                label: format!("Record {} ({})", ri + 1, rec.id),
                kind: "text".into(),
                value: String::new(),
                required: false,
                min: 0.0,
                max: 0.0,
                has_bounds: false,
                options: vec![],
            });
            for field in &s.fields {
                if matches!(field.kind, FieldKind::Calculation { .. }) {
                    continue;
                }
                let val = rec
                    .fields
                    .get(&field.key)
                    .map(|v| match v {
                        serde_json::Value::String(s) => s.clone(),
                        serde_json::Value::Null => String::new(),
                        other => other.to_string(),
                    })
                    .unwrap_or_default();

                let (kind, opts, min, max, has_bounds) = match &field.kind {
                    FieldKind::Text
                    | FieldKind::TextArea
                    | FieldKind::Date
                    | FieldKind::DateTime
                    | FieldKind::File(_) => ("text", vec![], 0.0, 0.0, false),
                    FieldKind::Number(b) => (
                        "number",
                        vec![],
                        b.min.unwrap_or(0.0) as f32,
                        b.max.unwrap_or(0.0) as f32,
                        b.min.is_some() || b.max.is_some(),
                    ),
                    FieldKind::Currency { .. } => ("number", vec![], 0.0, 0.0, false),
                    FieldKind::Integer(b) => (
                        "integer",
                        vec![],
                        b.min.unwrap_or(0.0) as f32,
                        b.max.unwrap_or(0.0) as f32,
                        b.min.is_some() || b.max.is_some(),
                    ),
                    FieldKind::Duration => ("integer", vec![], 0.0, 0.0, false),
                    FieldKind::Boolean => ("boolean", vec![], 0.0, 0.0, false),
                    FieldKind::Color => ("color", vec![], 0.0, 0.0, false),
                    FieldKind::Select(options) => (
                        "select",
                        options.iter().map(|o| o.value.clone()).collect(),
                        0.0,
                        0.0,
                        false,
                    ),
                    FieldKind::Calculation { .. } => unreachable!(),
                    FieldKind::Custom { .. } => ("text", vec![], 0.0, 0.0, false),
                };

                rows.push(FieldRowData {
                    key: format!("facet.record.{}.{}", ri, field.key),
                    label: field.label.clone(),
                    kind: kind.into(),
                    value: val,
                    required: field.required,
                    min,
                    max,
                    has_bounds,
                    options: opts,
                });
            }
        }
    }
}
