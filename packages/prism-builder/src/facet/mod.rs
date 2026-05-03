//! Facets — data-driven content generators.
//!
//! A `FacetDef` pairs a [`FacetTemplate`] (inline node subtree or
//! component reference) with a [`FacetKind`] that determines how data
//! is produced. Each kind resolves to `Vec<Value>`, which feeds into
//! template expansion (clone + bind per item).
//!
//! See `docs/dev/facets.md` for the full design rationale.


mod schema;
mod kind;
mod data;
mod template;
mod resolve;
mod promote;
mod render;

pub use data::*;
pub use kind::*;
pub use promote::*;
pub use render::*;
pub use resolve::*;
pub use schema::*;
pub use template::*;

#[cfg(test)]
mod tests {
    use super::*;
    use indexmap::IndexMap;
    use serde_json::{json, Value};

    use prism_core::widget::{get_json_field, DataQuery, FilterOp, QueryFilter};

    use crate::document::Node;
    use crate::prefab::{ExposedSlot, PrefabDef};
    use crate::registry::{FieldKind, FieldSpec, NumericBounds, SelectOption};

    fn hero_prefab() -> PrefabDef {
        PrefabDef {
            id: "prefab:hero".into(),
            label: "Hero".into(),
            description: String::new(),
            root: Node {
                id: "hero-root".into(),
                component: "text".into(),
                props: json!({ "body": "default" }),
                children: vec![],
                ..Default::default()
            },
            exposed: vec![ExposedSlot {
                key: "title".into(),
                target_node: "hero-root".into(),
                target_prop: "body".into(),
                spec: FieldSpec::text("title", "Title"),
            }],
            variants: vec![],
            thumbnail: None,
        }
    }

    fn sample_facet() -> FacetDef {
        FacetDef {
            id: "facet:heroes".into(),
            label: "Heroes".into(),
            description: String::new(),
            kind: FacetKind::List,
            schema_id: None,
            template: FacetTemplate::default(),
            output: FacetOutput::default(),
            data: FacetDataSource::Static {
                items: vec![json!({ "name": "Alpha" }), json!({ "name": "Beta" })],
                records: vec![],
            },
            bindings: vec![FacetBinding {
                slot_key: "title".into(),
                item_field: "name".into(),
            }],
            variant_rules: vec![],
            layout: FacetLayout {
                direction: FacetDirection::Column,
                gap: 8.0,
                ..Default::default()
            },
            resolved_data: None,
        }
    }

    #[test]
    fn json_field_flat() {
        let item = json!({ "name": "Alpha" });
        assert_eq!(get_json_field(&item, "name"), Some(json!("Alpha")));
    }

    #[test]
    fn json_field_nested() {
        let item = json!({ "meta": { "title": "Deep" } });
        assert_eq!(get_json_field(&item, "meta.title"), Some(json!("Deep")));
    }

    #[test]
    fn json_field_missing_returns_none() {
        let item = json!({ "a": 1 });
        assert!(get_json_field(&item, "b").is_none());
        assert!(get_json_field(&item, "a.nested").is_none());
    }

    #[test]
    fn static_source_resolves_to_items() {
        let resources = IndexMap::new();
        let src = FacetDataSource::Static {
            items: vec![json!("a"), json!("b")],
            records: vec![],
        };
        let resolved = src.resolve(&resources);
        assert_eq!(resolved.len(), 2);
    }

    #[test]
    fn resource_source_resolves_from_registry() {
        use crate::resource::{ResourceDef, ResourceKind};
        let mut resources = IndexMap::new();
        resources.insert(
            "items".into(),
            ResourceDef {
                id: "items".into(),
                kind: ResourceKind::DataSource,
                label: "Items".into(),
                description: String::new(),
                data: json!([{ "name": "X" }, { "name": "Y" }]),
            },
        );
        let src = FacetDataSource::Resource { id: "items".into() };
        let resolved = src.resolve(&resources);
        assert_eq!(resolved.len(), 2);
        assert_eq!(resolved[0]["name"], "X");
    }

    #[test]
    fn resource_source_missing_returns_empty() {
        let resources = IndexMap::new();
        let src = FacetDataSource::Resource {
            id: "missing".into(),
        };
        assert!(src.resolve(&resources).is_empty());
    }

    #[test]
    fn apply_bindings_injects_values() {
        let prefab = hero_prefab();
        let item = json!({ "name": "TestTitle" });
        let bindings = vec![FacetBinding {
            slot_key: "title".into(),
            item_field: "name".into(),
        }];
        let mut root = prefab.root.clone();
        apply_bindings(&mut root, &prefab, &bindings, &item);
        assert_eq!(root.props["body"], "TestTitle");
    }

    #[test]
    fn apply_bindings_skips_missing_slot() {
        let prefab = hero_prefab();
        let item = json!({ "name": "TestTitle" });
        let bindings = vec![FacetBinding {
            slot_key: "nonexistent".into(),
            item_field: "name".into(),
        }];
        let mut root = prefab.root.clone();
        apply_bindings(&mut root, &prefab, &bindings, &item);
        // default value should be unchanged
        assert_eq!(root.props["body"], "default");
    }

    #[test]
    fn facet_def_round_trips_serde() {
        let def = sample_facet();
        let json = serde_json::to_string(&def).unwrap();
        let back: FacetDef = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, "facet:heroes");
        assert_eq!(back.bindings.len(), 1);
        assert_eq!(back.bindings[0].slot_key, "title");
    }

    #[test]
    fn facet_data_source_serde_static() {
        let src = FacetDataSource::Static {
            items: vec![json!({"a": 1})],
            records: vec![],
        };
        let json = serde_json::to_string(&src).unwrap();
        let back: FacetDataSource = serde_json::from_str(&json).unwrap();
        match back {
            FacetDataSource::Static { items, .. } => assert_eq!(items.len(), 1),
            _ => panic!("expected Static"),
        }
    }

    #[test]
    fn facet_data_source_serde_resource() {
        let src = FacetDataSource::Resource {
            id: "my-data".into(),
        };
        let json = serde_json::to_string(&src).unwrap();
        let back: FacetDataSource = serde_json::from_str(&json).unwrap();
        match back {
            FacetDataSource::Resource { id } => assert_eq!(id, "my-data"),
            _ => panic!("expected Resource"),
        }
    }

    #[test]
    fn facet_schema_has_required_facet_id() {
        let schema = crate::schemas::facet();
        let facet_id_spec = schema.iter().find(|s| s.key == "facet_id").unwrap();
        assert!(facet_id_spec.required);
    }

    fn sample_resources() -> IndexMap<String, crate::resource::ResourceDef> {
        use crate::resource::{ResourceDef, ResourceKind};
        let mut resources = IndexMap::new();
        resources.insert(
            "products".into(),
            ResourceDef {
                id: "products".into(),
                kind: ResourceKind::DataSource,
                label: "Products".into(),
                description: String::new(),
                data: json!([
                    { "name": "Apple", "status": "active", "price": 1.5 },
                    { "name": "Banana", "status": "inactive", "price": 0.5 },
                    { "name": "Cherry", "status": "active", "price": 3.0 },
                ]),
            },
        );
        resources
    }

    #[test]
    fn query_source_no_filter_no_sort_returns_all() {
        let resources = sample_resources();
        let src = FacetDataSource::Query {
            source: "products".into(),
            query: DataQuery::default(),
        };
        let items = src.resolve(&resources);
        assert_eq!(items.len(), 3);
    }

    #[test]
    fn query_source_equality_filter() {
        let resources = sample_resources();
        let src = FacetDataSource::Query {
            source: "products".into(),
            query: DataQuery {
                filters: vec![QueryFilter::new("status", FilterOp::Eq, json!("active"))],
                ..Default::default()
            },
        };
        let items = src.resolve(&resources);
        assert_eq!(items.len(), 2);
        assert!(items.iter().all(|i| i["status"] == "active"));
    }

    #[test]
    fn query_source_inequality_filter() {
        let resources = sample_resources();
        let src = FacetDataSource::Query {
            source: "products".into(),
            query: DataQuery {
                filters: vec![QueryFilter::new("status", FilterOp::Neq, json!("active"))],
                ..Default::default()
            },
        };
        let items = src.resolve(&resources);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0]["name"], "Banana");
    }

    #[test]
    fn query_source_sort_ascending() {
        use prism_core::widget::QuerySort;
        let resources = sample_resources();
        let src = FacetDataSource::Query {
            source: "products".into(),
            query: DataQuery {
                sort: vec![QuerySort {
                    field: "name".into(),
                    descending: false,
                }],
                ..Default::default()
            },
        };
        let items = src.resolve(&resources);
        assert_eq!(items[0]["name"], "Apple");
        assert_eq!(items[1]["name"], "Banana");
        assert_eq!(items[2]["name"], "Cherry");
    }

    #[test]
    fn query_source_sort_descending() {
        use prism_core::widget::QuerySort;
        let resources = sample_resources();
        let src = FacetDataSource::Query {
            source: "products".into(),
            query: DataQuery {
                sort: vec![QuerySort {
                    field: "name".into(),
                    descending: true,
                }],
                ..Default::default()
            },
        };
        let items = src.resolve(&resources);
        assert_eq!(items[0]["name"], "Cherry");
        assert_eq!(items[2]["name"], "Apple");
    }

    #[test]
    fn query_source_filter_and_sort() {
        use prism_core::widget::QuerySort;
        let resources = sample_resources();
        let src = FacetDataSource::Query {
            source: "products".into(),
            query: DataQuery {
                filters: vec![QueryFilter::new("status", FilterOp::Eq, json!("active"))],
                sort: vec![QuerySort {
                    field: "price".into(),
                    descending: true,
                }],
                ..Default::default()
            },
        };
        let items = src.resolve(&resources);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0]["name"], "Cherry");
        assert_eq!(items[1]["name"], "Apple");
    }

    #[test]
    fn query_source_with_limit() {
        let resources = sample_resources();
        let src = FacetDataSource::Query {
            source: "products".into(),
            query: DataQuery {
                limit: Some(2),
                ..Default::default()
            },
        };
        let items = src.resolve(&resources);
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn query_source_serde_round_trip() {
        use prism_core::widget::QuerySort;
        let src = FacetDataSource::Query {
            source: "data".into(),
            query: DataQuery {
                filters: vec![QueryFilter::new("active", FilterOp::Eq, json!(true))],
                sort: vec![QuerySort {
                    field: "name".into(),
                    descending: false,
                }],
                ..Default::default()
            },
        };
        let json_str = serde_json::to_string(&src).unwrap();
        let back: FacetDataSource = serde_json::from_str(&json_str).unwrap();
        match back {
            FacetDataSource::Query { source, query } => {
                assert_eq!(source, "data");
                assert_eq!(query.filters.len(), 1);
                assert_eq!(query.sort.len(), 1);
            }
            _ => panic!("expected Query"),
        }
    }

    // ── Schema tests ─────────────────────────────────────────────

    fn test_schema() -> FacetSchema {
        FacetSchema {
            id: "schema:test".into(),
            label: "Test Schema".into(),
            description: String::new(),
            fields: vec![
                FieldSpec::text("title", "Title").required(),
                FieldSpec::integer("count", "Count", NumericBounds::min_max(0.0, 100.0))
                    .with_default(json!(0)),
                FieldSpec::select(
                    "status",
                    "Status",
                    vec![
                        SelectOption::new("active", "Active"),
                        SelectOption::new("archived", "Archived"),
                    ],
                ),
            ],
        }
    }

    #[test]
    fn schema_serde_round_trip() {
        let schema = test_schema();
        let json = serde_json::to_string(&schema).unwrap();
        let back: FacetSchema = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, "schema:test");
        assert_eq!(back.fields.len(), 3);
    }

    #[test]
    fn schema_default_record_has_all_fields() {
        let schema = test_schema();
        let rec = schema.default_record("rec:1");
        assert_eq!(rec.id, "rec:1");
        assert_eq!(rec.fields.len(), 3);
        assert_eq!(rec.fields["title"], json!(""));
        assert_eq!(rec.fields["count"], json!(0));
        assert_eq!(rec.fields["status"], json!("active"));
    }

    #[test]
    fn schema_default_record_excludes_calculation_fields() {
        let schema = FacetSchema {
            id: "s:calc".into(),
            label: "With Calc".into(),
            description: String::new(),
            fields: vec![
                FieldSpec::number("price", "Price", NumericBounds::unbounded()),
                FieldSpec::calculation("total", "Total", "price * 2"),
            ],
        };
        let rec = schema.default_record("rec:1");
        assert_eq!(rec.fields.len(), 1);
        assert!(rec.fields.contains_key("price"));
        assert!(!rec.fields.contains_key("total"));
    }

    #[test]
    fn schema_validate_record_catches_missing_required() {
        let schema = test_schema();
        let rec = FacetRecord {
            id: "rec:1".into(),
            fields: IndexMap::new(),
        };
        let errors = schema.validate_record(&rec);
        assert!(errors.iter().any(|e| e.field == "title"));
    }

    #[test]
    fn schema_validate_record_catches_out_of_bounds() {
        let schema = test_schema();
        let mut rec = schema.default_record("rec:1");
        rec.fields.insert("count".into(), json!(200));
        let errors = schema.validate_record(&rec);
        assert!(errors.iter().any(|e| e.field == "count"));
    }

    #[test]
    fn schema_validate_record_catches_invalid_select() {
        let schema = test_schema();
        let mut rec = schema.default_record("rec:1");
        rec.fields.insert("title".into(), json!("ok"));
        rec.fields.insert("status".into(), json!("nonexistent"));
        let errors = schema.validate_record(&rec);
        assert!(errors.iter().any(|e| e.field == "status"));
    }

    #[test]
    fn schema_validate_record_passes_valid() {
        let schema = test_schema();
        let mut rec = schema.default_record("rec:1");
        rec.fields.insert("title".into(), json!("My Item"));
        rec.fields.insert("count".into(), json!(42));
        rec.fields.insert("status".into(), json!("active"));
        let errors = schema.validate_record(&rec);
        assert!(errors.is_empty());
    }

    #[test]
    fn facet_record_to_value() {
        let mut fields = IndexMap::new();
        fields.insert("name".into(), json!("Alpha"));
        fields.insert("age".into(), json!(25));
        let rec = FacetRecord {
            id: "rec:1".into(),
            fields,
        };
        let val = rec.to_value();
        assert_eq!(val["name"], "Alpha");
        assert_eq!(val["age"], 25);
    }

    #[test]
    fn static_source_prefers_records_over_items() {
        let resources = IndexMap::new();
        let mut fields = IndexMap::new();
        fields.insert("name".into(), json!("FromRecord"));
        let src = FacetDataSource::Static {
            items: vec![json!({"name": "FromItem"})],
            records: vec![FacetRecord {
                id: "r1".into(),
                fields,
            }],
        };
        let resolved = src.resolve(&resources);
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0]["name"], "FromRecord");
    }

    #[test]
    fn facet_def_schema_id_round_trips() {
        let def = FacetDef {
            id: "facet:test".into(),
            label: "Test".into(),
            description: String::new(),
            kind: FacetKind::List,
            schema_id: Some("schema:projects".into()),
            template: FacetTemplate::default(),
            output: FacetOutput::default(),
            data: FacetDataSource::default(),
            bindings: vec![],
            variant_rules: vec![],
            layout: FacetLayout::default(),
            resolved_data: None,
        };
        let json = serde_json::to_string(&def).unwrap();
        let back: FacetDef = serde_json::from_str(&json).unwrap();
        assert_eq!(back.schema_id, Some("schema:projects".into()));
    }

    #[test]
    fn facet_def_schema_id_none_omitted_in_json() {
        let def = FacetDef {
            id: "facet:test".into(),
            label: "Test".into(),
            description: String::new(),
            kind: FacetKind::List,
            schema_id: None,
            template: FacetTemplate::default(),
            output: FacetOutput::default(),
            data: FacetDataSource::default(),
            bindings: vec![],
            variant_rules: vec![],
            layout: FacetLayout::default(),
            resolved_data: None,
        };
        let json = serde_json::to_string(&def).unwrap();
        assert!(!json.contains("schema_id"));
    }

    #[test]
    fn facet_kind_default_is_list() {
        let kind = FacetKind::default();
        assert!(matches!(kind, FacetKind::List));
    }

    #[test]
    fn facet_kind_serde_list() {
        let def = sample_facet();
        let json = serde_json::to_string(&def).unwrap();
        let back: FacetDef = serde_json::from_str(&json).unwrap();
        assert!(matches!(back.kind, FacetKind::List));
    }

    #[test]
    fn facet_kind_serde_object_query() {
        use prism_core::widget::QuerySort;
        let mut def = sample_facet();
        def.kind = FacetKind::ObjectQuery {
            query: DataQuery {
                object_type: Some("BlogPost".into()),
                filters: vec![QueryFilter::new("status", FilterOp::Eq, json!("published"))],
                sort: vec![QuerySort {
                    field: "created_at".into(),
                    descending: true,
                }],
                limit: Some(10),
            },
        };
        let json = serde_json::to_string(&def).unwrap();
        let back: FacetDef = serde_json::from_str(&json).unwrap();
        match &back.kind {
            FacetKind::ObjectQuery { query } => {
                assert_eq!(query.object_type.as_deref(), Some("BlogPost"));
                assert_eq!(query.filters.len(), 1);
                assert!(query.sort[0].descending);
                assert_eq!(query.limit, Some(10));
            }
            _ => panic!("expected ObjectQuery"),
        }
    }

    #[test]
    fn facet_kind_serde_script() {
        let mut def = sample_facet();
        def.kind = FacetKind::Script {
            source: "return {}".into(),
            language: ScriptLanguage::Luau,
            graph: None,
        };
        let json = serde_json::to_string(&def).unwrap();
        let back: FacetDef = serde_json::from_str(&json).unwrap();
        match &back.kind {
            FacetKind::Script {
                source, language, ..
            } => {
                assert_eq!(source, "return {}");
                assert_eq!(*language, ScriptLanguage::Luau);
            }
            _ => panic!("expected Script"),
        }
    }

    #[test]
    fn facet_kind_serde_aggregate() {
        let mut def = sample_facet();
        def.kind = FacetKind::Aggregate {
            operation: AggregateOp::Sum,
            field: Some("price".into()),
        };
        let json = serde_json::to_string(&def).unwrap();
        let back: FacetDef = serde_json::from_str(&json).unwrap();
        match &back.kind {
            FacetKind::Aggregate { operation, field } => {
                assert!(matches!(operation, AggregateOp::Sum));
                assert_eq!(field.as_deref(), Some("price"));
            }
            _ => panic!("expected Aggregate"),
        }
    }

    #[test]
    fn facet_kind_serde_lookup() {
        let mut def = sample_facet();
        def.kind = FacetKind::Lookup {
            source_entity: "Project".into(),
            edge_type: "has_member".into(),
            target_entity: "User".into(),
        };
        let json = serde_json::to_string(&def).unwrap();
        let back: FacetDef = serde_json::from_str(&json).unwrap();
        match &back.kind {
            FacetKind::Lookup {
                source_entity,
                edge_type,
                target_entity,
            } => {
                assert_eq!(source_entity, "Project");
                assert_eq!(edge_type, "has_member");
                assert_eq!(target_entity, "User");
            }
            _ => panic!("expected Lookup"),
        }
    }

    #[test]
    fn facet_kind_backward_compat_missing_kind_defaults_to_list() {
        let json = r#"{
            "id": "facet:old",
            "label": "Old Facet",
            "data": { "kind": "static", "items": [] },
            "bindings": [],
            "layout": {}
        }"#;
        let def: FacetDef = serde_json::from_str(json).unwrap();
        assert!(matches!(def.kind, FacetKind::List));
        assert_eq!(def.effective_component_id(), Some("card"));
    }

    #[test]
    fn facet_kind_tag_round_trip() {
        for tag in FACET_KIND_TAGS {
            let kind = FacetKind::from_tag(tag);
            assert_eq!(kind.tag(), *tag);
        }
    }

    #[test]
    fn aggregate_count() {
        let items = vec![json!({"x": 1}), json!({"x": 2}), json!({"x": 3})];
        let result = apply_aggregate(&items, &AggregateOp::Count, None);
        assert_eq!(result, json!(3));
    }

    #[test]
    fn aggregate_sum() {
        let items = vec![json!({"x": 10}), json!({"x": 20}), json!({"x": 30})];
        let result = apply_aggregate(&items, &AggregateOp::Sum, Some("x"));
        assert_eq!(result, json!(60.0));
    }

    #[test]
    fn aggregate_min_max() {
        let items = vec![json!({"v": 5}), json!({"v": 2}), json!({"v": 8})];
        assert_eq!(
            apply_aggregate(&items, &AggregateOp::Min, Some("v")),
            json!(2.0)
        );
        assert_eq!(
            apply_aggregate(&items, &AggregateOp::Max, Some("v")),
            json!(8.0)
        );
    }

    #[test]
    fn aggregate_avg() {
        let items = vec![json!({"v": 10}), json!({"v": 20}), json!({"v": 30})];
        let result = apply_aggregate(&items, &AggregateOp::Avg, Some("v"));
        assert_eq!(result, json!(20.0));
    }

    #[test]
    fn aggregate_join() {
        let items = vec![
            json!({"name": "Alice"}),
            json!({"name": "Bob"}),
            json!({"name": "Carol"}),
        ];
        let result = apply_aggregate(
            &items,
            &AggregateOp::Join {
                separator: ", ".into(),
            },
            Some("name"),
        );
        assert_eq!(result, json!("Alice, Bob, Carol"));
    }

    #[test]
    fn aggregate_empty_items() {
        let items: Vec<Value> = vec![];
        assert_eq!(apply_aggregate(&items, &AggregateOp::Count, None), json!(0));
        assert_eq!(
            apply_aggregate(&items, &AggregateOp::Avg, Some("x")),
            Value::Null
        );
        assert_eq!(
            apply_aggregate(&items, &AggregateOp::Min, Some("x")),
            Value::Null
        );
    }

    #[test]
    fn aggregate_op_tag_round_trip() {
        for tag in AGGREGATE_OP_TAGS {
            let op = AggregateOp::from_tag(tag);
            assert_eq!(op.tag(), *tag);
        }
    }

    #[test]
    fn resolve_items_list_kind() {
        let def = sample_facet();
        let resources = IndexMap::new();
        let schemas = IndexMap::new();
        match def.resolve_items(&resources, &schemas) {
            ResolvedFacetData::Items(items) => assert_eq!(items.len(), 2),
            _ => panic!("expected Items"),
        }
    }

    #[test]
    fn resolve_items_aggregate_kind() {
        let mut def = sample_facet();
        def.kind = FacetKind::Aggregate {
            operation: AggregateOp::Count,
            field: None,
        };
        let resources = IndexMap::new();
        let schemas = IndexMap::new();
        match def.resolve_items(&resources, &schemas) {
            ResolvedFacetData::Single(val) => assert_eq!(val, json!(2)),
            _ => panic!("expected Single"),
        }
    }

    #[test]
    fn field_kind_text_is_default_builder() {
        let spec = FieldSpec::text("test", "Test");
        assert!(matches!(spec.kind, FieldKind::Text));
    }

    // ── Calculation field tests ──────────────────────────────────────

    #[test]
    fn evaluate_calculations_basic_arithmetic() {
        let schema = FacetSchema {
            id: "s1".into(),
            label: "Test".into(),
            description: String::new(),
            fields: vec![
                FieldSpec::number("price", "Price", NumericBounds::unbounded()),
                FieldSpec::integer("qty", "Quantity", NumericBounds::unbounded()),
                FieldSpec::calculation("total", "Total", "price * qty"),
            ],
        };

        let mut items = vec![
            json!({"price": 10.0, "qty": 3}),
            json!({"price": 5.5, "qty": 2}),
        ];

        evaluate_calculations(&mut items, &schema);

        assert_eq!(items[0]["total"], json!(30.0));
        assert_eq!(items[1]["total"], json!(11.0));
    }

    #[test]
    fn evaluate_calculations_string_concat() {
        let schema = FacetSchema {
            id: "s2".into(),
            label: "Test".into(),
            description: String::new(),
            fields: vec![
                FieldSpec::text("first", "First"),
                FieldSpec::text("last", "Last"),
                FieldSpec::calculation("full", "Full Name", "concat(first, \" \", last)"),
            ],
        };

        let mut items = vec![json!({"first": "Alice", "last": "Smith"})];
        evaluate_calculations(&mut items, &schema);
        assert_eq!(items[0]["full"], json!("Alice Smith"));
    }

    #[test]
    fn evaluate_calculations_empty_formula_skipped() {
        let schema = FacetSchema {
            id: "s3".into(),
            label: "Test".into(),
            description: String::new(),
            fields: vec![FieldSpec::calculation("calc", "Calc", String::new())],
        };

        let mut items = vec![json!({"x": 1})];
        evaluate_calculations(&mut items, &schema);
        assert!(items[0].get("calc").is_none());
    }

    #[test]
    fn evaluate_calculations_no_calc_fields_is_noop() {
        let schema = FacetSchema {
            id: "s4".into(),
            label: "Test".into(),
            description: String::new(),
            fields: vec![FieldSpec::text("name", "Name")],
        };

        let mut items = vec![json!({"name": "x"})];
        let original = items.clone();
        evaluate_calculations(&mut items, &schema);
        assert_eq!(items, original);
    }

    #[test]
    fn evaluate_calculations_non_object_items_skipped() {
        let schema = FacetSchema {
            id: "s5".into(),
            label: "Test".into(),
            description: String::new(),
            fields: vec![FieldSpec::calculation("calc", "Calc", "1 + 1")],
        };

        let mut items = vec![json!(42), json!("hello"), json!(null)];
        let original = items.clone();
        evaluate_calculations(&mut items, &schema);
        assert_eq!(items, original);
    }

    #[test]
    fn resolve_items_with_calculations() {
        let mut def = sample_facet();
        def.schema_id = Some("s1".into());
        def.data = FacetDataSource::Static {
            items: vec![
                json!({"name": "A", "price": 10.0, "qty": 2}),
                json!({"name": "B", "price": 5.0, "qty": 4}),
            ],
            records: vec![],
        };

        let resources = IndexMap::new();
        let mut schemas = IndexMap::new();
        schemas.insert(
            "s1".to_string(),
            FacetSchema {
                id: "s1".into(),
                label: "Products".into(),
                description: String::new(),
                fields: vec![FieldSpec::calculation("total", "Total", "price * qty")],
            },
        );

        match def.resolve_items(&resources, &schemas) {
            ResolvedFacetData::Items(items) => {
                assert_eq!(items[0]["total"], json!(20.0));
                assert_eq!(items[1]["total"], json!(20.0));
            }
            _ => panic!("expected Items"),
        }
    }

    #[test]
    fn resolve_items_aggregate_with_calculations() {
        let mut def = sample_facet();
        def.kind = FacetKind::Aggregate {
            operation: AggregateOp::Sum,
            field: Some("total".into()),
        };
        def.schema_id = Some("s1".into());
        def.data = FacetDataSource::Static {
            items: vec![
                json!({"price": 10.0, "qty": 2}),
                json!({"price": 5.0, "qty": 3}),
            ],
            records: vec![],
        };

        let resources = IndexMap::new();
        let mut schemas = IndexMap::new();
        schemas.insert(
            "s1".to_string(),
            FacetSchema {
                id: "s1".into(),
                label: "Products".into(),
                description: String::new(),
                fields: vec![FieldSpec::calculation("total", "Total", "price * qty")],
            },
        );

        match def.resolve_items(&resources, &schemas) {
            ResolvedFacetData::Single(val) => {
                assert_eq!(val, json!(35.0));
            }
            _ => panic!("expected Single"),
        }
    }

    // ── Visual graph / ScriptLanguage tests ──────────────────────

    #[test]
    fn script_language_visual_graph_serde() {
        let lang = ScriptLanguage::VisualGraph;
        let json = serde_json::to_string(&lang).unwrap();
        assert_eq!(json, "\"visual-graph\"");
        let back: ScriptLanguage = serde_json::from_str(&json).unwrap();
        assert_eq!(back, ScriptLanguage::VisualGraph);
    }

    #[test]
    fn script_kind_with_graph_serde() {
        use prism_core::language::visual::{ScriptGraph, ScriptNode, ScriptNodeKind};

        let mut graph = ScriptGraph::new("facet-graph", "Facet Script");
        graph.add_node(ScriptNode::new("entry", ScriptNodeKind::Entry, "Entry"));
        graph.add_node(ScriptNode::new("ret", ScriptNodeKind::Return, "return {}"));

        let mut def = sample_facet();
        def.kind = FacetKind::Script {
            source: String::new(),
            language: ScriptLanguage::VisualGraph,
            graph: Some(graph),
        };

        let json = serde_json::to_string(&def).unwrap();
        let back: FacetDef = serde_json::from_str(&json).unwrap();
        match &back.kind {
            FacetKind::Script {
                language, graph, ..
            } => {
                assert_eq!(*language, ScriptLanguage::VisualGraph);
                let g = graph.as_ref().unwrap();
                assert_eq!(g.nodes.len(), 2);
                assert_eq!(g.id, "facet-graph");
            }
            _ => panic!("expected Script"),
        }
    }

    #[test]
    fn script_kind_graph_none_omitted_in_json() {
        let mut def = sample_facet();
        def.kind = FacetKind::Script {
            source: "return {}".into(),
            language: ScriptLanguage::Luau,
            graph: None,
        };
        let json = serde_json::to_string(&def).unwrap();
        assert!(!json.contains("\"graph\""));
    }

    #[test]
    fn script_kind_backward_compat_missing_graph() {
        let json = r#"{
            "id": "facet:old-script",
            "label": "Old Script",
            "kind": { "type": "script", "source": "return {}", "language": "luau" },
            "data": { "kind": "static", "items": [] },
            "bindings": [],
            "layout": {}
        }"#;
        let def: FacetDef = serde_json::from_str(json).unwrap();
        match &def.kind {
            FacetKind::Script { graph, .. } => assert!(graph.is_none()),
            _ => panic!("expected Script"),
        }
    }

    // ── Variant rule tests ──────────────────────────────────────

    #[test]
    fn evaluate_variant_rules_sets_axis_prop() {
        let mut root = Node {
            id: "r".into(),
            component: "text".into(),
            props: json!({}),
            children: vec![],
            ..Default::default()
        };
        let rules = vec![FacetVariantRule {
            field: "featured".into(),
            value: "true".into(),
            axis_key: "variant".into(),
            axis_value: "highlight".into(),
        }];
        let item = json!({"featured": true});
        evaluate_variant_rules(&mut root, &rules, &item);
        assert_eq!(root.props["variant"], "highlight");
    }

    #[test]
    fn evaluate_variant_rules_no_match_no_change() {
        let mut root = Node {
            id: "r".into(),
            component: "text".into(),
            props: json!({"variant": "default"}),
            children: vec![],
            ..Default::default()
        };
        let rules = vec![FacetVariantRule {
            field: "featured".into(),
            value: "true".into(),
            axis_key: "variant".into(),
            axis_value: "highlight".into(),
        }];
        let item = json!({"featured": false});
        evaluate_variant_rules(&mut root, &rules, &item);
        assert_eq!(root.props["variant"], "default");
    }

    #[test]
    fn evaluate_variant_rules_multiple_rules() {
        let mut root = Node {
            id: "r".into(),
            component: "text".into(),
            props: json!({}),
            children: vec![],
            ..Default::default()
        };
        let rules = vec![
            FacetVariantRule {
                field: "status".into(),
                value: "active".into(),
                axis_key: "variant".into(),
                axis_value: "primary".into(),
            },
            FacetVariantRule {
                field: "size".into(),
                value: "large".into(),
                axis_key: "size".into(),
                axis_value: "lg".into(),
            },
        ];
        let item = json!({"status": "active", "size": "large"});
        evaluate_variant_rules(&mut root, &rules, &item);
        assert_eq!(root.props["variant"], "primary");
        assert_eq!(root.props["size"], "lg");
    }

    #[test]
    fn facet_def_variant_rules_serde() {
        let mut def = sample_facet();
        def.variant_rules = vec![FacetVariantRule {
            field: "featured".into(),
            value: "true".into(),
            axis_key: "variant".into(),
            axis_value: "highlight".into(),
        }];
        let json = serde_json::to_string(&def).unwrap();
        let back: FacetDef = serde_json::from_str(&json).unwrap();
        assert_eq!(back.variant_rules.len(), 1);
        assert_eq!(back.variant_rules[0].field, "featured");
        assert_eq!(back.variant_rules[0].axis_value, "highlight");
    }

    #[test]
    fn facet_def_variant_rules_omitted_when_empty() {
        let def = sample_facet();
        let json = serde_json::to_string(&def).unwrap();
        assert!(!json.contains("variant_rules"));
    }

    // ── FacetTemplate tests ─────────────────────────────────────

    #[test]
    fn facet_template_default_is_component_ref() {
        let t = FacetTemplate::default();
        match t {
            FacetTemplate::ComponentRef { component_id } => assert_eq!(component_id, "card"),
            _ => panic!("expected ComponentRef"),
        }
    }

    #[test]
    fn facet_template_inline_serde() {
        let t = FacetTemplate::Inline {
            root: Box::new(Node {
                id: "tpl".into(),
                component: "text".into(),
                props: json!({"body": "{{title}}"}),
                children: vec![],
                ..Default::default()
            }),
        };
        let json = serde_json::to_string(&t).unwrap();
        let back: FacetTemplate = serde_json::from_str(&json).unwrap();
        match back {
            FacetTemplate::Inline { root } => {
                assert_eq!(root.id, "tpl");
                assert_eq!(root.props["body"], "{{title}}");
            }
            _ => panic!("expected Inline"),
        }
    }

    #[test]
    fn facet_template_component_ref_serde() {
        let t = FacetTemplate::ComponentRef {
            component_id: "my-card".into(),
        };
        let json = serde_json::to_string(&t).unwrap();
        let back: FacetTemplate = serde_json::from_str(&json).unwrap();
        match back {
            FacetTemplate::ComponentRef { component_id } => {
                assert_eq!(component_id, "my-card");
            }
            _ => panic!("expected ComponentRef"),
        }
    }

    #[test]
    fn facet_output_default_is_repeated() {
        let o = FacetOutput::default();
        assert!(matches!(o, FacetOutput::Repeated));
    }

    #[test]
    fn facet_output_scalar_serde() {
        let o = FacetOutput::Scalar {
            target_node: "label-1".into(),
            target_prop: "body".into(),
        };
        let json = serde_json::to_string(&o).unwrap();
        let back: FacetOutput = serde_json::from_str(&json).unwrap();
        match back {
            FacetOutput::Scalar {
                target_node,
                target_prop,
            } => {
                assert_eq!(target_node, "label-1");
                assert_eq!(target_prop, "body");
            }
            _ => panic!("expected Scalar"),
        }
    }

    #[test]
    fn facet_def_backward_compat_no_template_or_output() {
        let json = r#"{
            "id": "facet:old",
            "label": "Old Facet",
            "data": { "kind": "static", "items": [] },
            "bindings": [],
            "layout": {}
        }"#;
        let def: FacetDef = serde_json::from_str(json).unwrap();
        assert!(matches!(def.template, FacetTemplate::ComponentRef { .. }));
        assert!(matches!(def.output, FacetOutput::Repeated));
    }

    // ── Expression resolution tests ─────────────────────────────

    #[test]
    fn resolve_expression_single_field() {
        let mut node = Node {
            id: "n1".into(),
            component: "text".into(),
            props: json!({"body": "{{title}}"}),
            children: vec![],
            ..Default::default()
        };
        let item = json!({"title": "Hello World"});
        resolve_template_expressions(&mut node, &item);
        assert_eq!(node.props["body"], "Hello World");
    }

    #[test]
    fn resolve_expression_preserves_number() {
        let mut node = Node {
            id: "n1".into(),
            component: "text".into(),
            props: json!({"count": "{{total}}"}),
            children: vec![],
            ..Default::default()
        };
        let item = json!({"total": 42});
        resolve_template_expressions(&mut node, &item);
        assert_eq!(node.props["count"], json!(42));
    }

    #[test]
    fn resolve_expression_record_prefix_stripped() {
        let mut node = Node {
            id: "n1".into(),
            component: "text".into(),
            props: json!({"body": "{{record.name}}"}),
            children: vec![],
            ..Default::default()
        };
        let item = json!({"name": "Test"});
        resolve_template_expressions(&mut node, &item);
        assert_eq!(node.props["body"], "Test");
    }

    #[test]
    fn resolve_expression_mixed_text() {
        let mut node = Node {
            id: "n1".into(),
            component: "text".into(),
            props: json!({"body": "Hello, {{name}}! You have {{count}} items."}),
            children: vec![],
            ..Default::default()
        };
        let item = json!({"name": "Alice", "count": 3});
        resolve_template_expressions(&mut node, &item);
        assert_eq!(node.props["body"], "Hello, Alice! You have 3 items.");
    }

    #[test]
    fn resolve_expression_nested_field() {
        let mut node = Node {
            id: "n1".into(),
            component: "text".into(),
            props: json!({"body": "{{meta.title}}"}),
            children: vec![],
            ..Default::default()
        };
        let item = json!({"meta": {"title": "Deep"}});
        resolve_template_expressions(&mut node, &item);
        assert_eq!(node.props["body"], "Deep");
    }

    #[test]
    fn resolve_expression_missing_field_is_null() {
        let mut node = Node {
            id: "n1".into(),
            component: "text".into(),
            props: json!({"body": "{{nonexistent}}"}),
            children: vec![],
            ..Default::default()
        };
        let item = json!({"name": "x"});
        resolve_template_expressions(&mut node, &item);
        assert_eq!(node.props["body"], Value::Null);
    }

    #[test]
    fn resolve_expression_no_expressions_is_noop() {
        let mut node = Node {
            id: "n1".into(),
            component: "text".into(),
            props: json!({"body": "plain text"}),
            children: vec![],
            ..Default::default()
        };
        let item = json!({"name": "x"});
        resolve_template_expressions(&mut node, &item);
        assert_eq!(node.props["body"], "plain text");
    }

    #[test]
    fn resolve_expression_children() {
        let mut node = Node {
            id: "root".into(),
            component: "container".into(),
            props: json!({}),
            children: vec![Node {
                id: "child".into(),
                component: "text".into(),
                props: json!({"body": "{{title}}"}),
                children: vec![],
                ..Default::default()
            }],
            ..Default::default()
        };
        let item = json!({"title": "Child Title"});
        resolve_template_expressions(&mut node, &item);
        assert_eq!(node.children[0].props["body"], "Child Title");
    }

    #[test]
    fn collect_expression_fields_finds_all() {
        let node = Node {
            id: "root".into(),
            component: "card".into(),
            props: json!({"title": "{{name}}", "body": "{{description}}"}),
            children: vec![Node {
                id: "img".into(),
                component: "image".into(),
                props: json!({"src": "{{thumbnail}}"}),
                children: vec![],
                ..Default::default()
            }],
            ..Default::default()
        };
        let fields = collect_expression_fields(&node);
        assert_eq!(fields.len(), 3);
        assert!(fields.iter().any(|(_, _, f)| f == "name"));
        assert!(fields.iter().any(|(_, _, f)| f == "description"));
        assert!(fields
            .iter()
            .any(|(id, _, f)| id == "img" && f == "thumbnail"));
    }

    // ── Component promotion tests ───────────────────────────────

    #[test]
    fn promote_inline_creates_prefab_and_bindings() {
        let root = Node {
            id: "tpl-root".into(),
            component: "card".into(),
            props: json!({"title": "{{name}}", "body": "{{desc}}"}),
            children: vec![],
            ..Default::default()
        };
        let (prefab, bindings) = promote_inline_to_component("facet:test", &root);
        assert_eq!(prefab.id, "user:facet:test");
        assert_eq!(prefab.exposed.len(), 2);
        assert_eq!(bindings.len(), 2);
        assert_eq!(prefab.root.props["title"], "");
        assert_eq!(prefab.root.props["body"], "");
    }

    // ── FacetDef helper tests ───────────────────────────────────

    #[test]
    fn effective_component_id_for_component_ref() {
        let def = sample_facet();
        assert_eq!(def.effective_component_id(), Some("card"));
    }

    #[test]
    fn effective_component_id_for_inline() {
        let mut def = sample_facet();
        def.template = FacetTemplate::Inline {
            root: Box::new(Node {
                id: "tpl".into(),
                component: "text".into(),
                props: json!({}),
                children: vec![],
                ..Default::default()
            }),
        };
        assert_eq!(def.effective_component_id(), None);
        assert!(def.is_inline());
    }

    #[test]
    fn is_scalar_output() {
        let mut def = sample_facet();
        assert!(!def.is_scalar());
        def.output = FacetOutput::Scalar {
            target_node: "n1".into(),
            target_prop: "body".into(),
        };
        assert!(def.is_scalar());
    }

    #[test]
    fn apply_scalar_bindings_injects_aggregate_value() {
        use crate::document::BuilderDocument;
        let mut doc = BuilderDocument {
            root: Some(Node {
                id: "root".into(),
                component: "container".into(),
                children: vec![Node {
                    id: "label".into(),
                    component: "text".into(),
                    props: json!({"body": "placeholder"}),
                    ..Default::default()
                }],
                ..Default::default()
            }),
            ..Default::default()
        };
        doc.facets.insert(
            "facet:sum".into(),
            FacetDef {
                id: "facet:sum".into(),
                label: "Sum".into(),
                kind: FacetKind::Aggregate {
                    operation: AggregateOp::Sum,
                    field: Some("amount".into()),
                },
                output: FacetOutput::Scalar {
                    target_node: "label".into(),
                    target_prop: "body".into(),
                },
                data: FacetDataSource::Static {
                    items: vec![json!({"amount": 10}), json!({"amount": 20})],
                    records: vec![],
                },
                ..Default::default()
            },
        );
        apply_scalar_bindings(&mut doc);
        let label = doc.root.as_ref().unwrap().find("label").unwrap();
        assert_eq!(label.props["body"], json!(30.0));
    }

    #[test]
    fn apply_scalar_bindings_skips_empty_target() {
        use crate::document::BuilderDocument;
        let mut doc = BuilderDocument {
            root: Some(Node {
                id: "root".into(),
                component: "text".into(),
                props: json!({"body": "original"}),
                ..Default::default()
            }),
            ..Default::default()
        };
        doc.facets.insert(
            "facet:x".into(),
            FacetDef {
                id: "facet:x".into(),
                kind: FacetKind::Aggregate {
                    operation: AggregateOp::Count,
                    field: None,
                },
                output: FacetOutput::Scalar {
                    target_node: String::new(),
                    target_prop: String::new(),
                },
                data: FacetDataSource::Static {
                    items: vec![json!({}), json!({})],
                    records: vec![],
                },
                ..Default::default()
            },
        );
        apply_scalar_bindings(&mut doc);
        assert_eq!(doc.root.as_ref().unwrap().props["body"], json!("original"));
    }

    #[test]
    fn set_component_id_updates_component_ref() {
        let mut def = sample_facet();
        def.set_component_id("hero");
        assert_eq!(def.effective_component_id(), Some("hero"));
    }

    #[test]
    fn set_component_id_noop_for_inline() {
        let mut def = sample_facet();
        def.template = FacetTemplate::Inline {
            root: Box::new(Node {
                id: "tpl".into(),
                component: "text".into(),
                ..Default::default()
            }),
        };
        def.set_component_id("hero");
        assert!(def.is_inline());
    }

    #[test]
    fn layout_serde_with_wrap_and_columns() {
        let layout = FacetLayout {
            direction: FacetDirection::Row,
            gap: 12.0,
            wrap: true,
            columns: Some(3),
        };
        let json = serde_json::to_string(&layout).unwrap();
        let back: FacetLayout = serde_json::from_str(&json).unwrap();
        assert!(back.wrap);
        assert_eq!(back.columns, Some(3));
        assert_eq!(back.gap, 12.0);
    }

    #[test]
    fn layout_defaults_no_wrap_no_columns() {
        let layout = FacetLayout::default();
        assert!(!layout.wrap);
        assert_eq!(layout.columns, None);
    }

    #[test]
    fn facet_def_serde_with_wrap_columns() {
        let mut def = sample_facet();
        def.layout.wrap = true;
        def.layout.columns = Some(4);
        let json = serde_json::to_string(&def).unwrap();
        let back: FacetDef = serde_json::from_str(&json).unwrap();
        assert!(back.layout.wrap);
        assert_eq!(back.layout.columns, Some(4));
    }
}
