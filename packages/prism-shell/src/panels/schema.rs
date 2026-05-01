use prism_builder::BuilderDocument;
use prism_builder::{FacetSchema, SchemaFieldKind};

use super::properties::FieldRowData;

pub struct SchemaDesignerPanel;

impl SchemaDesignerPanel {
    pub fn schema_list_rows(doc: &BuilderDocument) -> Vec<SchemaListRow> {
        doc.facet_schemas
            .values()
            .map(|s| SchemaListRow {
                id: s.id.clone(),
                label: s.label.clone(),
                field_count: s.fields.len(),
            })
            .collect()
    }

    pub fn field_rows(schema: &FacetSchema) -> Vec<FieldRowData> {
        let mut rows = vec![
            FieldRowData {
                key: "schema.label".into(),
                label: "Schema Name".into(),
                kind: "text".into(),
                value: schema.label.clone(),
                required: true,
                min: 0.0,
                max: 0.0,
                has_bounds: false,
                options: vec![],
            },
            FieldRowData {
                key: "schema.description".into(),
                label: "Description".into(),
                kind: "text".into(),
                value: schema.description.clone(),
                required: false,
                min: 0.0,
                max: 0.0,
                has_bounds: false,
                options: vec![],
            },
        ];

        for (i, field) in schema.fields.iter().enumerate() {
            rows.push(FieldRowData {
                key: format!("schema.field.{i}.header"),
                label: format!("── Field {} ──", i + 1),
                kind: "text".into(),
                value: String::new(),
                required: false,
                min: 0.0,
                max: 0.0,
                has_bounds: false,
                options: vec![],
            });
            rows.push(FieldRowData {
                key: format!("schema.field.{i}.key"),
                label: "Key".into(),
                kind: "text".into(),
                value: field.key.clone(),
                required: true,
                min: 0.0,
                max: 0.0,
                has_bounds: false,
                options: vec![],
            });
            rows.push(FieldRowData {
                key: format!("schema.field.{i}.label"),
                label: "Label".into(),
                kind: "text".into(),
                value: field.label.clone(),
                required: true,
                min: 0.0,
                max: 0.0,
                has_bounds: false,
                options: vec![],
            });
            rows.push(FieldRowData {
                key: format!("schema.field.{i}.kind"),
                label: "Type".into(),
                kind: "select".into(),
                value: kind_to_string(&field.kind),
                required: true,
                min: 0.0,
                max: 0.0,
                has_bounds: false,
                options: FIELD_KIND_OPTIONS.iter().map(|s| (*s).into()).collect(),
            });
            rows.push(FieldRowData {
                key: format!("schema.field.{i}.required"),
                label: "Required".into(),
                kind: "boolean".into(),
                value: field.required.to_string(),
                required: false,
                min: 0.0,
                max: 0.0,
                has_bounds: false,
                options: vec![],
            });
            if let SchemaFieldKind::Calculation { formula } = &field.kind {
                rows.push(FieldRowData {
                    key: format!("schema.field.{i}.formula"),
                    label: "Formula".into(),
                    kind: "text".into(),
                    value: formula.clone(),
                    required: true,
                    min: 0.0,
                    max: 0.0,
                    has_bounds: false,
                    options: vec![],
                });
            }
        }

        rows
    }
}

pub struct SchemaListRow {
    pub id: String,
    pub label: String,
    pub field_count: usize,
}

const FIELD_KIND_OPTIONS: &[&str] = &[
    "text",
    "number",
    "integer",
    "boolean",
    "date",
    "color",
    "image",
    "url",
    "select",
    "calculation",
];

fn kind_to_string(kind: &SchemaFieldKind) -> String {
    match kind {
        SchemaFieldKind::Text => "text",
        SchemaFieldKind::Number { .. } => "number",
        SchemaFieldKind::Integer { .. } => "integer",
        SchemaFieldKind::Boolean => "boolean",
        SchemaFieldKind::Date => "date",
        SchemaFieldKind::Color => "color",
        SchemaFieldKind::Image => "image",
        SchemaFieldKind::Url => "url",
        SchemaFieldKind::Select { .. } => "select",
        SchemaFieldKind::Calculation { .. } => "calculation",
    }
    .into()
}

pub fn kind_from_string(s: &str) -> SchemaFieldKind {
    match s {
        "number" => SchemaFieldKind::Number {
            min: None,
            max: None,
        },
        "integer" => SchemaFieldKind::Integer {
            min: None,
            max: None,
        },
        "boolean" => SchemaFieldKind::Boolean,
        "date" => SchemaFieldKind::Date,
        "color" => SchemaFieldKind::Color,
        "image" => SchemaFieldKind::Image,
        "url" => SchemaFieldKind::Url,
        "select" => SchemaFieldKind::Select { options: vec![] },
        "calculation" => SchemaFieldKind::Calculation {
            formula: String::new(),
        },
        _ => SchemaFieldKind::Text,
    }
}

pub fn apply_schema_edit(schema: &mut FacetSchema, key: &str, value: &str) {
    match key {
        "label" => schema.label = value.to_string(),
        "description" => schema.description = value.to_string(),
        key if key.starts_with("field.") => {
            let rest = &key["field.".len()..];
            if let Some((idx_str, prop)) = rest.split_once('.') {
                if let Ok(idx) = idx_str.parse::<usize>() {
                    if let Some(field) = schema.fields.get_mut(idx) {
                        match prop {
                            "key" => field.key = value.to_string(),
                            "label" => field.label = value.to_string(),
                            "kind" => field.kind = kind_from_string(value),
                            "required" => field.required = value == "true",
                            "formula" => {
                                if let SchemaFieldKind::Calculation { ref mut formula } = field.kind
                                {
                                    *formula = value.to_string();
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prism_builder::SchemaField;

    fn test_schema() -> FacetSchema {
        FacetSchema {
            id: "schema:test".into(),
            label: "Test".into(),
            description: String::new(),
            fields: vec![
                SchemaField {
                    key: "title".into(),
                    label: "Title".into(),
                    kind: SchemaFieldKind::Text,
                    required: true,
                    default_value: None,
                },
                SchemaField {
                    key: "count".into(),
                    label: "Count".into(),
                    kind: SchemaFieldKind::Integer {
                        min: Some(0),
                        max: Some(100),
                    },
                    required: false,
                    default_value: None,
                },
            ],
        }
    }

    #[test]
    fn field_rows_contains_schema_label() {
        let schema = test_schema();
        let rows = SchemaDesignerPanel::field_rows(&schema);
        assert!(rows.iter().any(|r| r.key == "schema.label"));
        let label_row = rows.iter().find(|r| r.key == "schema.label").unwrap();
        assert_eq!(label_row.value, "Test");
    }

    #[test]
    fn field_rows_contains_all_fields() {
        let schema = test_schema();
        let rows = SchemaDesignerPanel::field_rows(&schema);
        assert!(rows.iter().any(|r| r.key == "schema.field.0.key"));
        assert!(rows.iter().any(|r| r.key == "schema.field.1.key"));
    }

    #[test]
    fn apply_schema_edit_updates_label() {
        let mut schema = test_schema();
        apply_schema_edit(&mut schema, "label", "Updated");
        assert_eq!(schema.label, "Updated");
    }

    #[test]
    fn apply_schema_edit_updates_field_key() {
        let mut schema = test_schema();
        apply_schema_edit(&mut schema, "field.0.key", "name");
        assert_eq!(schema.fields[0].key, "name");
    }

    #[test]
    fn apply_schema_edit_changes_field_kind() {
        let mut schema = test_schema();
        apply_schema_edit(&mut schema, "field.0.kind", "number");
        assert!(matches!(
            schema.fields[0].kind,
            SchemaFieldKind::Number { .. }
        ));
    }

    #[test]
    fn apply_schema_edit_toggles_required() {
        let mut schema = test_schema();
        assert!(schema.fields[0].required);
        apply_schema_edit(&mut schema, "field.0.required", "false");
        assert!(!schema.fields[0].required);
    }

    #[test]
    fn kind_round_trips_through_string() {
        for opt in FIELD_KIND_OPTIONS {
            let kind = kind_from_string(opt);
            let s = kind_to_string(&kind);
            assert_eq!(&s, opt);
        }
    }

    #[test]
    fn schema_list_rows_returns_all_schemas() {
        let mut doc = BuilderDocument::default();
        doc.facet_schemas.insert("s1".into(), test_schema());
        let rows = SchemaDesignerPanel::schema_list_rows(&doc);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "schema:test");
    }

    #[test]
    fn field_rows_shows_formula_for_calculation_fields() {
        let schema = FacetSchema {
            id: "s:calc".into(),
            label: "Calc".into(),
            description: String::new(),
            fields: vec![SchemaField {
                key: "total".into(),
                label: "Total".into(),
                kind: SchemaFieldKind::Calculation {
                    formula: "price * qty".into(),
                },
                required: false,
                default_value: None,
            }],
        };
        let rows = SchemaDesignerPanel::field_rows(&schema);
        let formula_row = rows.iter().find(|r| r.key == "schema.field.0.formula");
        assert!(formula_row.is_some());
        assert_eq!(formula_row.unwrap().value, "price * qty");
    }

    #[test]
    fn apply_schema_edit_updates_formula() {
        let mut schema = FacetSchema {
            id: "s:calc".into(),
            label: "Calc".into(),
            description: String::new(),
            fields: vec![SchemaField {
                key: "total".into(),
                label: "Total".into(),
                kind: SchemaFieldKind::Calculation {
                    formula: "a + b".into(),
                },
                required: false,
                default_value: None,
            }],
        };
        apply_schema_edit(&mut schema, "field.0.formula", "price * qty");
        match &schema.fields[0].kind {
            SchemaFieldKind::Calculation { formula } => {
                assert_eq!(formula, "price * qty");
            }
            _ => panic!("expected Calculation kind"),
        }
    }
}
