//! Inspector-tree + property-row derivation helpers.
//! Split out of `state/mod.rs` (Phase B.4). `use super::*`
//! inherits intra-`state` types + crate imports; cross-module
//! callers reach widened `pub(crate)` items.

use super::*;

/// Project an arbitrary serde value to its textual form so the
/// field-focus path can populate `original` regardless of the prop's
/// json kind. Strings come through as-is; numbers / booleans go
/// through `to_string`; null and arrays / objects fall through as the
/// empty string (the field-edit kinds that focus today —
/// `text` / `color` / `file` — are always stringly typed).
pub(crate) fn value_as_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        _ => String::new(),
    }
}

/// Walk `doc.root` depth-first and project each node onto an
/// `InspectorNode` row. Single source of truth for the inspector's
/// shape; called from `resync_builder_for_selection`.
pub(crate) fn derive_inspector_tree(
    doc: &prism_builder::BuilderDocument,
    selection: &Option<NodeId>,
) -> Vec<InspectorNode> {
    let mut out: Vec<InspectorNode> = Vec::new();
    if let Some(root) = doc.root.as_ref() {
        walk_inspector(root, 0, selection.as_deref(), &mut out);
    }
    out
}

pub(crate) fn walk_inspector(
    node: &prism_builder::Node,
    depth: u32,
    selection: Option<&str>,
    out: &mut Vec<InspectorNode>,
) {
    let label = inspector_label_for(node);
    out.push(InspectorNode {
        id: node.id.clone(),
        label,
        depth,
        selected: selection == Some(node.id.as_str()),
    });
    for child in &node.children {
        walk_inspector(child, depth + 1, selection, out);
    }
}

/// Friendly label for an inspector row. Prefers a string prop the user
/// likely recognises (`label` / `body` / `title`) over the raw id,
/// falling back to `"<component> · <id>"` when no human-readable text
/// is set. Mirrors the Slint era's row-label heuristic.
pub(crate) fn inspector_label_for(node: &prism_builder::Node) -> String {
    for key in ["label", "title", "body", "name"] {
        if let Some(s) = node.props.get(key).and_then(|v| v.as_str()) {
            let trimmed = s.trim();
            if !trimmed.is_empty() {
                let snippet: String = trimmed.chars().take(40).collect();
                return snippet;
            }
        }
    }
    if node.id.is_empty() {
        node.component.clone()
    } else {
        format!("{} · {}", node.component, node.id)
    }
}

/// Project the selected node's schema (`Vec<FieldSpec>` from the
/// registry) onto a flat list of `PropertyRow`s the
/// `shell.properties-panel` block consumes. Each row carries the
/// `component` id (`shell.field-editor`) and the `props` shape the
/// editor block reads (key / label / kind / value).
///
/// Returns an empty vector when there's no selection, no registry,
/// or the selected node's component isn't registered — every case
/// the live shell can hit during boot, headless tests, or partially
/// loaded plugins. Headless render paths keep working.
pub(crate) fn derive_property_rows(
    registry: Option<&prism_builder::ComponentRegistry>,
    modifier_registry: Option<&prism_builder::ModifierRegistry>,
    doc: &prism_builder::BuilderDocument,
    selection: Option<&str>,
) -> Vec<PropertyRow> {
    let Some(id) = selection else {
        return Vec::new();
    };
    let Some(root) = doc.root.as_ref() else {
        return Vec::new();
    };
    let Some(node) = root.find(id) else {
        return Vec::new();
    };
    let Some(reg) = registry else {
        return Vec::new();
    };
    let Some(component) = reg.get(&node.component) else {
        return Vec::new();
    };
    let schema = component.schema();
    let mut rows: Vec<PropertyRow> = Vec::with_capacity(schema.len() + 1);
    // ── Section 1: the node's typed component identity ───────────
    rows.push(PropertyRow {
        component: "shell.section-header".into(),
        props: json!({
            "label": node.component,
            "data-target-id": node.id,
        }),
    });
    for spec in schema {
        rows.push(property_row_from_spec(&spec, &node.props, &node.id));
    }
    // ── Section 2..N: one per attached modifier ──────────────────
    //
    // Wave 1: each attached `node.modifiers` entry produces a
    // `shell.modifier-header` row (with toggle + remove affordances)
    // followed by its schema rows projected through
    // `property_row_from_spec`. Modifier props live in a flat
    // namespace (`modifier.<idx>.<key>`) so the existing field-edit
    // routing reaches them through one path; the `target-id` carries
    // the **owning node's** id with the modifier index in
    // `data-modifier-idx` so the click router can disambiguate.
    if let Some(mod_reg) = modifier_registry {
        for (idx, modifier) in node.modifiers.iter().enumerate() {
            let descriptor = mod_reg.descriptor(&modifier.kind);
            let label = descriptor
                .as_ref()
                .map(|d| d.label.as_str())
                .unwrap_or(modifier.kind.as_str());
            let description = descriptor
                .as_ref()
                .map(|d| d.description.as_str())
                .unwrap_or("");
            rows.push(PropertyRow {
                component: "shell.modifier-header".into(),
                props: json!({
                    "label": label,
                    "description": description,
                    "modifier-id": modifier.kind,
                    "modifier-idx": idx,
                    "enabled": modifier.enabled,
                    "target-id": node.id,
                }),
            });
            if modifier.enabled {
                let m_schema = mod_reg.schema_for(&modifier.kind);
                for spec in m_schema {
                    rows.push(property_row_from_modifier_spec(
                        &spec,
                        &modifier.props,
                        &node.id,
                        idx,
                    ));
                }
            }
        }
        // ── Footer: + Add Behaviour ──────────────────────────────
        rows.push(PropertyRow {
            component: "shell.add-modifier-button".into(),
            props: json!({
                "target-id": node.id,
                // Names of behaviours already attached (the picker
                // filters these out so users can't double-attach).
                "attached": node.modifiers.iter().map(|m| m.kind.clone()).collect::<Vec<_>>(),
            }),
        });
    }
    rows
}

/// Project a modifier's `FieldSpec` onto a property row keyed to the
/// owning node + modifier index. Mirror of `property_row_from_spec`
/// but adds `data-modifier-idx` so the §43 C2 hit-test router
/// dispatches to `AppState::set_modifier_prop` (Wave 1.5) instead of
/// `set_node_prop`.
pub(crate) fn property_row_from_modifier_spec(
    spec: &prism_core::widget::field::FieldSpec,
    props: &Value,
    target_id: &str,
    modifier_idx: usize,
) -> PropertyRow {
    let base = property_row_from_spec(spec, props, target_id);
    let mut props_obj = base.props;
    if let Some(obj) = props_obj.as_object_mut() {
        obj.insert("modifier-idx".into(), json!(modifier_idx));
        // Override the kind-edit route so the router routes to a
        // modifier prop write, not a node prop write.
        obj.insert("edit-target".into(), json!("modifier"));
    }
    PropertyRow {
        component: base.component,
        props: props_obj,
    }
}

/// Project one `FieldSpec` onto a `PropertyRow` consumed by
/// `shell.field-editor`. The editor block reads `key / label / kind /
/// value / required / target-id` — extracting them here keeps the
/// panel binding a one-line forwarder and pins the shape in tests.
///
/// `target_id` flows in from the selected doc node so the lowered
/// row's `data-target-id` attr carries it. The §43 C2 hit-test
/// router consults that attr to dispatch the edit back to
/// [`AppState::set_node_prop`].
pub(crate) fn property_row_from_spec(
    spec: &prism_core::widget::field::FieldSpec,
    props: &Value,
    target_id: &str,
) -> PropertyRow {
    use prism_core::widget::field::FieldKind;

    let kind: &str = match &spec.kind {
        FieldKind::Text => "text",
        FieldKind::TextArea => "textarea",
        FieldKind::Number(_) => "number",
        FieldKind::Integer(_) => "integer",
        FieldKind::Boolean => "boolean",
        FieldKind::Select(_) => "select",
        FieldKind::Color => "color",
        FieldKind::File(_) => "file",
        FieldKind::Date => "date",
        FieldKind::DateTime => "datetime",
        FieldKind::Duration => "duration",
        FieldKind::Currency { .. } => "currency",
        FieldKind::Calculation { .. } => "calculation",
        FieldKind::Custom { tag, .. } => tag.as_str(),
    };
    let value = props
        .get(&spec.key)
        .cloned()
        .unwrap_or_else(|| spec.default.clone());
    // Wave 11.3 — pre-compute `data-value` (the string projection of
    // `value` the hit-test router reads as `data-value`) so the DSL
    // field-editor block doesn't need a runtime variant-match. The
    // old `prop_str` in the Rust block only handled `Value::String`;
    // surfacing the projection here means a `Number(8)` prop shows
    // up as `"8"` for `data-value` consistently across every kind.
    let data_value = match &value {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        _ => String::new(),
    };
    let mut row_props = json!({
        "key": spec.key,
        "label": spec.label,
        "kind": kind,
        "value": value,
        "required": spec.required,
        "target-id": target_id,
        "data-value": data_value,
    });
    // Kind-specific extensions: select carries its options so the
    // click-to-cycle path (`handle_field_edit_click`) can step through
    // them without re-resolving the spec; number / integer carry their
    // bounds so the +/- step clamps at the schema-declared range.
    //
    // Wave 11.3 — also pre-compute the DSL-side substrate the
    // `shell.field-editor` block reads at lower time:
    // - `options-joined`: comma-joined option values (the
    //   `data-options` attr the select-cycle router reads).
    // - `min-set` / `max-set`: booleans the DSL's `if=` predicate
    //   tests without null-comparison expressions.
    // - `slider-fill-pct`: clamped 0..100 fraction; the slider
    //   track's filled rect reads it as a `width="{slider-fill-pct}%"`.
    // - `drag-display-value`: the same `format_drag_value` shape
    //   the legacy `chrome::drag_number_field_node` emitted, so
    //   the DSL surface stays byte-identical to the migrated Rust
    //   block.
    // - `accept-joined`: comma-joined accept list for file-kind
    //   browse dialogs (matches `<input accept="…">` shape).
    match &spec.kind {
        FieldKind::Select(options) => {
            row_props["options"] = Value::Array(
                options
                    .iter()
                    .map(|o| json!({ "value": o.value, "label": o.label }))
                    .collect(),
            );
            // Wave 2.3 — pack as `value:label,value:label,…` so the
            // dropdown's `open_select_dropdown` handler can recover
            // both halves from the `data-options` attr without a
            // second binding pass. Bare values (no `:label`) round
            // through unchanged, with the value doubling as label —
            // matches the legacy click-to-cycle behaviour.
            let joined = options
                .iter()
                .map(|o| {
                    if o.label == o.value {
                        o.value.clone()
                    } else {
                        format!("{}:{}", o.value, o.label)
                    }
                })
                .collect::<Vec<_>>()
                .join(",");
            row_props["options-joined"] = json!(joined);
        }
        FieldKind::Number(bounds) | FieldKind::Integer(bounds) => {
            if let Some(min) = bounds.min {
                row_props["min"] = json!(min);
            }
            if let Some(max) = bounds.max {
                row_props["max"] = json!(max);
            }
            let min_set = bounds.min.is_some();
            let max_set = bounds.max.is_some();
            row_props["min-set"] = json!(min_set);
            row_props["max-set"] = json!(max_set);
            let raw = value
                .as_f64()
                .or_else(|| value.as_str().and_then(|s| s.parse().ok()))
                .unwrap_or(0.0);
            row_props["drag-display-value"] = json!(format_drag_value(raw));
            if let (Some(mn), Some(mx)) = (bounds.min, bounds.max) {
                if mx > mn {
                    let pct = ((raw - mn) / (mx - mn)).clamp(0.0, 1.0) * 100.0;
                    row_props["slider-fill-pct"] = json!(pct);
                }
            }
        }
        FieldKind::File(cfg) => {
            if !cfg.accept.is_empty() {
                row_props["accept"] = json!(cfg.accept.join(","));
            }
        }
        _ => {}
    }
    PropertyRow {
        component: "shell.field-editor".into(),
        props: row_props,
    }
}

/// Wave 11.3 — `chrome::format_drag_value` lifted out of the
/// `chrome` module so `property_row_from_spec` can pre-compute
/// `drag-display-value` without the field-editor migration
/// leaving a Rust-only consumer behind. Rounds to two decimal
/// places, trims trailing zeros and a trailing decimal point so
/// integers render as `"3"` rather than `"3.00"`.
pub(crate) fn format_drag_value(v: f64) -> String {
    let rounded = (v * 100.0).round() / 100.0;
    let raw = format!("{rounded:.2}");
    let trimmed = raw.trim_end_matches('0').trim_end_matches('.');
    if trimmed.is_empty() {
        "0".into()
    } else {
        trimmed.to_string()
    }
}
