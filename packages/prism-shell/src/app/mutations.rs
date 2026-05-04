use prism_builder::layout::{
    AbsoluteProps, Dimension, FlowProps, GridPlacement, LayoutMode, PageSize,
};
use prism_builder::{
    AggregateOp, ExposedSlot, FacetBinding, FacetDataSource, FacetDef, FacetDirection, FacetKind,
    FacetOutput, FacetTemplate, FacetVariantRule, FieldSpec, Node, ScriptLanguage, StyleProperties,
};

pub(super) fn apply_facet_edit(def: &mut FacetDef, key: &str, value: &str) {
    match key {
        "kind" => {
            def.kind = FacetKind::from_tag(value);
        }
        "component_id" => def.set_component_id(value),
        "label" => def.label = value.to_string(),
        "direction" => {
            def.layout.direction = match value {
                "row" | "Row" => FacetDirection::Row,
                _ => FacetDirection::Column,
            };
        }
        "gap" => {
            if let Ok(v) = value.parse::<f32>() {
                def.layout.gap = v;
            }
        }
        "wrap" => {
            def.layout.wrap = value == "true";
        }
        "columns" => {
            def.layout.columns = value.parse::<u32>().ok().filter(|&n| n > 0);
        }
        "source_kind" => {
            def.data = match value {
                "resource" => {
                    let id = match &def.data {
                        FacetDataSource::Resource { id } => id.clone(),
                        FacetDataSource::Query { source, .. } => source.clone(),
                        _ => String::new(),
                    };
                    FacetDataSource::Resource { id }
                }
                "query" => {
                    let source = match &def.data {
                        FacetDataSource::Resource { id } => id.clone(),
                        FacetDataSource::Query { source, .. } => source.clone(),
                        _ => String::new(),
                    };
                    FacetDataSource::Query {
                        source,
                        query: prism_core::widget::DataQuery::default(),
                    }
                }
                _ => FacetDataSource::Static {
                    items: vec![],
                    records: vec![],
                },
            };
        }
        "source_id" => match &mut def.data {
            FacetDataSource::Resource { id } => *id = value.to_string(),
            FacetDataSource::Query { source, .. } => *source = value.to_string(),
            _ => {}
        },
        "filter" => {
            if let FacetDataSource::Query { query, .. } = &mut def.data {
                if value.is_empty() {
                    query.filters.clear();
                } else if let Some(qf) = prism_builder::parse_filter_expr(value) {
                    query.filters = vec![qf];
                }
            }
        }
        "sort_by" => {
            if let FacetDataSource::Query { query, .. } = &mut def.data {
                if value.is_empty() {
                    query.sort.clear();
                } else {
                    let (descending, path) = if let Some(stripped) = value.strip_prefix('-') {
                        (true, stripped)
                    } else {
                        (false, value)
                    };
                    query.sort = vec![prism_core::widget::QuerySort {
                        field: path.to_string(),
                        descending,
                    }];
                }
            }
        }
        "schema_id" => {
            def.schema_id = if value.is_empty() || value == "(none)" {
                None
            } else {
                Some(value.to_string())
            };
        }
        // ObjectQuery fields — all modify the embedded DataQuery
        "entity_type" => {
            if let FacetKind::ObjectQuery { ref mut query } = def.kind {
                query.object_type = if value.is_empty() {
                    None
                } else {
                    Some(value.to_string())
                };
            }
        }
        "oq_filter" => {
            if let FacetKind::ObjectQuery { ref mut query } = def.kind {
                if value.is_empty() {
                    query.filters.clear();
                } else if let Some(qf) = prism_builder::parse_filter_expr(value) {
                    query.filters = vec![qf];
                }
            }
        }
        "oq_sort_by" => {
            if let FacetKind::ObjectQuery { ref mut query } = def.kind {
                if value.is_empty() {
                    query.sort.clear();
                } else {
                    let (descending, path) = if let Some(stripped) = value.strip_prefix('-') {
                        (true, stripped)
                    } else {
                        (false, value)
                    };
                    query.sort = vec![prism_core::widget::QuerySort {
                        field: path.to_string(),
                        descending,
                    }];
                }
            }
        }
        "oq_limit" => {
            if let FacetKind::ObjectQuery { ref mut query } = def.kind {
                query.limit = value.parse::<usize>().ok().filter(|&n| n > 0);
            }
        }
        // Script fields
        "script_source" => {
            if let FacetKind::Script { ref mut source, .. } = def.kind {
                *source = value.to_string();
            }
        }
        "script_language" => {
            if let FacetKind::Script {
                ref mut language, ..
            } = def.kind
            {
                *language = match value {
                    "visual-graph" => ScriptLanguage::VisualGraph,
                    _ => ScriptLanguage::Luau,
                };
            }
            sync_script_language(def);
        }
        // Aggregate fields
        "agg_operation" => {
            if let FacetKind::Aggregate {
                ref mut operation, ..
            } = def.kind
            {
                *operation = AggregateOp::from_tag(value);
            }
        }
        "agg_field" => {
            if let FacetKind::Aggregate { ref mut field, .. } = def.kind {
                *field = if value.is_empty() {
                    None
                } else {
                    Some(value.to_string())
                };
            }
        }
        "agg_separator" => {
            if let FacetKind::Aggregate {
                operation: AggregateOp::Join { ref mut separator },
                ..
            } = def.kind
            {
                *separator = value.to_string();
            }
        }
        // Lookup fields
        "lookup_source" => {
            if let FacetKind::Lookup {
                ref mut source_entity,
                ..
            } = def.kind
            {
                *source_entity = value.to_string();
            }
        }
        "lookup_edge" => {
            if let FacetKind::Lookup {
                ref mut edge_type, ..
            } = def.kind
            {
                *edge_type = value.to_string();
            }
        }
        "lookup_target" => {
            if let FacetKind::Lookup {
                ref mut target_entity,
                ..
            } = def.kind
            {
                *target_entity = value.to_string();
            }
        }
        key if key.starts_with("binding.") => {
            let slot_key = &key["binding.".len()..];
            if !slot_key.is_empty() {
                if let Some(existing) = def.bindings.iter_mut().find(|b| b.slot_key == slot_key) {
                    existing.item_field = value.to_string();
                } else if !value.is_empty() {
                    def.bindings.push(FacetBinding {
                        slot_key: slot_key.to_string(),
                        item_field: value.to_string(),
                    });
                }
                def.bindings.retain(|b| !b.item_field.is_empty());
            }
        }
        key if key.starts_with("record.") => {
            let rest = &key["record.".len()..];
            if let Some((idx_str, field_key)) = rest.split_once('.') {
                if let Ok(idx) = idx_str.parse::<usize>() {
                    if let FacetDataSource::Static {
                        ref mut records, ..
                    } = def.data
                    {
                        if let Some(rec) = records.get_mut(idx) {
                            let parsed: serde_json::Value = serde_json::from_str(value)
                                .unwrap_or_else(|_| {
                                    if value.is_empty() {
                                        serde_json::Value::Null
                                    } else if value == "true" {
                                        serde_json::Value::Bool(true)
                                    } else if value == "false" {
                                        serde_json::Value::Bool(false)
                                    } else if let Ok(n) = value.parse::<f64>() {
                                        serde_json::json!(n)
                                    } else {
                                        serde_json::Value::String(value.to_string())
                                    }
                                });
                            rec.fields.insert(field_key.to_string(), parsed);
                        }
                    }
                }
            }
        }
        "add_variant_rule" => {
            def.variant_rules.push(FacetVariantRule {
                field: String::new(),
                value: String::new(),
                axis_key: String::new(),
                axis_value: String::new(),
            });
        }
        key if key.starts_with("remove_variant_rule.") => {
            if let Ok(idx) = key["remove_variant_rule.".len()..].parse::<usize>() {
                if idx < def.variant_rules.len() {
                    def.variant_rules.remove(idx);
                }
            }
        }
        key if key.starts_with("variant_rule.") => {
            let rest = &key["variant_rule.".len()..];
            if let Some((idx_str, field_name)) = rest.split_once('.') {
                if let Ok(idx) = idx_str.parse::<usize>() {
                    if let Some(rule) = def.variant_rules.get_mut(idx) {
                        match field_name {
                            "field" => rule.field = value.to_string(),
                            "value" => rule.value = value.to_string(),
                            "axis_key" => rule.axis_key = value.to_string(),
                            "axis_value" => rule.axis_value = value.to_string(),
                            _ => {}
                        }
                    }
                }
            }
        }
        "template_type" => match value {
            "inline" => {
                if !def.is_inline() {
                    def.template = FacetTemplate::Inline {
                        root: Box::new(Node {
                            id: "inline-root".into(),
                            component: "container".into(),
                            ..Default::default()
                        }),
                    };
                }
            }
            _ => {
                if def.is_inline() {
                    def.template = FacetTemplate::ComponentRef {
                        component_id: "card".into(),
                    };
                }
            }
        },
        "output_type" => match value {
            "scalar" => {
                if !def.is_scalar() {
                    def.output = FacetOutput::Scalar {
                        target_node: String::new(),
                        target_prop: String::new(),
                    };
                }
            }
            _ => {
                def.output = FacetOutput::Repeated;
            }
        },
        "scalar_target_node" => {
            if let FacetOutput::Scalar {
                ref mut target_node,
                ..
            } = def.output
            {
                *target_node = value.to_string();
            }
        }
        "scalar_target_prop" => {
            if let FacetOutput::Scalar {
                ref mut target_prop,
                ..
            } = def.output
            {
                *target_prop = value.to_string();
            }
        }
        _ => {}
    }
}

pub(super) fn sync_script_language(def: &mut FacetDef) {
    if let FacetKind::Script {
        ref mut source,
        ref language,
        ref mut graph,
    } = def.kind
    {
        match language {
            ScriptLanguage::VisualGraph => {
                if !source.is_empty() && graph.is_none() {
                    use prism_core::language::luau::LuauVisualLanguage;
                    use prism_core::language::visual::VisualLanguage;
                    if let Ok(g) = LuauVisualLanguage::new().decompile(source) {
                        *graph = Some(g);
                    }
                }
            }
            ScriptLanguage::Luau => {
                if let Some(g) = graph.take() {
                    use prism_core::language::luau::LuauVisualLanguage;
                    use prism_core::language::visual::VisualLanguage;
                    if let Ok(s) = LuauVisualLanguage::new().compile(&g) {
                        *source = s;
                    }
                }
            }
        }
    }
}

/// Build exposed slots from all string props on the root node.
/// Provides a starting-point binding surface when saving as a prefab.
pub(super) fn auto_expose_slots(node: &Node) -> Vec<ExposedSlot> {
    let mut slots = Vec::new();
    if let Some(obj) = node.props.as_object() {
        for (key, val) in obj {
            if val.is_string() && !key.starts_with('_') {
                let label = {
                    let mut chars = key.chars();
                    match chars.next() {
                        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                        None => key.clone(),
                    }
                };
                slots.push(ExposedSlot {
                    key: key.clone(),
                    target_node: node.id.clone(),
                    target_prop: key.clone(),
                    spec: FieldSpec::text(key.as_str(), label.as_str()),
                });
            }
        }
    }
    slots
}

pub(super) fn apply_style_edit(style: &mut StyleProperties, key: &str, value: &str) {
    style.apply_path(key, value);
}

pub(super) fn apply_page_layout_edit(
    pl: &mut prism_builder::layout::PageLayout,
    key: &str,
    value: &str,
) {
    // `PageLayout` derives `Editable`. The only legacy-key
    // translation left is for the `page_size` ComboBox: Slint emits
    // PascalCase variant names (`"A4"`, `"Custom"`, …) while the
    // derive's tag matcher expects the serde-rename'd form
    // (`"a4"`, `"custom"`). And we want `Custom` to seed reasonable
    // default dimensions instead of zeros.
    if key == "page_size" {
        pl.size = match value {
            "Custom" => PageSize::Custom {
                width: 1280.0,
                height: 800.0,
            },
            other => {
                let mut out = pl.size;
                out.apply_path("", &other.to_ascii_lowercase());
                out
            }
        };
        return;
    }
    pl.apply_path(key, value);
}

pub(super) fn apply_node_layout_edit(
    root: &mut Node,
    target: &str,
    key: &str,
    value: &str,
) -> bool {
    if root.id == target {
        apply_layout_to_node(root, key, value);
        return true;
    }
    for child in &mut root.children {
        if apply_node_layout_edit(child, target, key, value) {
            return true;
        }
    }
    false
}

pub(super) fn apply_node_transform_edit(
    root: &mut Node,
    target: &str,
    key: &str,
    value: &str,
) -> bool {
    if root.id == target {
        apply_transform_to_node(root, key, value);
        return true;
    }
    for child in &mut root.children {
        if apply_node_transform_edit(child, target, key, value) {
            return true;
        }
    }
    false
}

pub(super) fn apply_transform_to_node(node: &mut Node, key: &str, value: &str) {
    // Strip the `transform.` routing prefix that the dispatcher used
    // to find this node, then delegate to the derived path-walker.
    // `Transform2D` derives `Editable` (`prism-core::foundation::spatial`),
    // so:
    //   transform.position.0 / .1 → x / y
    //   transform.scale.0 / .1    → scale x / y
    //   transform.rotation        → degrees → radians (via #[edit(with = …)])
    //   transform.anchor          → kebab-case Anchor variant
    //   transform.pivot.0 / .1    → pivot x / y
    let path = key.strip_prefix("transform.").unwrap_or(key);
    node.transform.apply_path(path, value);
}

#[derive(Clone)]
pub(super) struct DragSnapshot {
    pub(super) node_id: String,
    pub(super) position: [f32; 2],
    pub(super) rotation: f32,
    pub(super) scale: [f32; 2],
    pub(super) pre_drag_source: String,
}

pub(super) fn find_node_transform(
    root: &Node,
    target: &str,
) -> Option<prism_core::foundation::spatial::Transform2D> {
    if root.id == target {
        return Some(root.transform.clone());
    }
    for child in &root.children {
        if let Some(t) = find_node_transform(child, target) {
            return Some(t);
        }
    }
    None
}

pub(super) fn apply_drag_to_node(
    root: &mut Node,
    tool: &str,
    dx: f32,
    dy: f32,
    shift: bool,
    snap: &DragSnapshot,
) {
    let node = if root.id == snap.node_id {
        root
    } else {
        fn find_mut<'a>(node: &'a mut Node, id: &str) -> Option<&'a mut Node> {
            for child in &mut node.children {
                if child.id == id {
                    return Some(child);
                }
                if let Some(n) = find_mut(child, id) {
                    return Some(n);
                }
            }
            None
        }
        match find_mut(root, &snap.node_id) {
            Some(n) => n,
            None => return,
        }
    };
    let t = &mut node.transform;
    match tool {
        "move" => {
            if shift {
                // Shift: constrain to major axis
                if dx.abs() > dy.abs() {
                    t.position[0] = snap.position[0] + dx;
                    t.position[1] = snap.position[1];
                } else {
                    t.position[0] = snap.position[0];
                    t.position[1] = snap.position[1] + dy;
                }
            } else {
                t.position[0] = snap.position[0] + dx;
                t.position[1] = snap.position[1] + dy;
            }
        }
        "rotate" => {
            let raw = snap.rotation + (dx * 0.5_f32).to_radians();
            if shift {
                // Shift: snap to 15-degree increments
                let deg = raw.to_degrees();
                let snapped = (deg / 15.0).round() * 15.0;
                t.rotation = snapped.to_radians();
            } else {
                t.rotation = raw;
            }
        }
        "scale" => {
            if shift {
                // Shift: uniform scale (use dx for both axes)
                let factor = (1.0 + dx / 100.0).max(0.01);
                t.scale[0] = snap.scale[0] * factor;
                t.scale[1] = snap.scale[1] * factor;
            } else {
                t.scale[0] = (snap.scale[0] * (1.0 + dx / 100.0)).max(0.01);
                t.scale[1] = (snap.scale[1] * (1.0 - dy / 100.0)).max(0.01);
            }
        }
        _ => {}
    }
}

#[derive(Clone)]
pub(super) struct ResizeSnapshot {
    pub(super) node_id: String,
    pub(super) position: [f32; 2],
    pub(super) width: f32,
    pub(super) height: f32,
}

#[derive(Clone)]
pub(super) struct GapResizeSnapshot {
    pub(super) parent_path: Vec<usize>,
    pub(super) gap_index: usize,
    pub(super) track_a: prism_builder::TrackSize,
    pub(super) track_b: prism_builder::TrackSize,
    pub(super) available: f32,
}

pub(super) fn find_node_layout_size(
    root: &Node,
    target: &str,
    layout: &prism_builder::ComputedLayout,
) -> Option<(f32, f32)> {
    if root.id == target {
        return layout
            .nodes
            .get(target)
            .map(|nl| (nl.rect.size.width, nl.rect.size.height));
    }
    for child in &root.children {
        if let Some(s) = find_node_layout_size(child, target, layout) {
            return Some(s);
        }
    }
    None
}

pub(super) fn apply_resize_to_node(
    root: &mut Node,
    handle: &str,
    dx: f32,
    dy: f32,
    shift: bool,
    snap: &ResizeSnapshot,
) {
    fn find_mut<'a>(node: &'a mut Node, id: &str) -> Option<&'a mut Node> {
        for child in &mut node.children {
            if child.id == id {
                return Some(child);
            }
            if let Some(n) = find_mut(child, id) {
                return Some(n);
            }
        }
        None
    }
    let node = if root.id == snap.node_id {
        root
    } else {
        match find_mut(root, &snap.node_id) {
            Some(n) => n,
            None => return,
        }
    };

    let (mut dx, mut dy) = (dx, dy);
    if shift {
        // Uniform: constrain aspect ratio
        let aspect = if snap.height > 0.0 {
            snap.width / snap.height
        } else {
            1.0
        };
        match handle {
            "tl" | "br" => {
                let d = if dx.abs() > dy.abs() { dx } else { dy * aspect };
                dx = d;
                dy = d / aspect;
            }
            "tr" | "bl" => {
                let d = if dx.abs() > dy.abs() {
                    dx
                } else {
                    -dy * aspect
                };
                dx = d;
                dy = -d / aspect;
            }
            _ => {}
        }
    }

    // Compute new position and size based on which handle is being dragged.
    // "tl" moves origin and shrinks; "br" only grows; edges move one axis.
    let (mut new_x, mut new_y, mut new_w, mut new_h) =
        (snap.position[0], snap.position[1], snap.width, snap.height);

    match handle {
        "tl" => {
            new_x += dx;
            new_y += dy;
            new_w -= dx;
            new_h -= dy;
        }
        "t" => {
            new_y += dy;
            new_h -= dy;
        }
        "tr" => {
            new_w += dx;
            new_y += dy;
            new_h -= dy;
        }
        "r" => {
            new_w += dx;
        }
        "br" => {
            new_w += dx;
            new_h += dy;
        }
        "b" => {
            new_h += dy;
        }
        "bl" => {
            new_x += dx;
            new_w -= dx;
            new_h += dy;
        }
        "l" => {
            new_x += dx;
            new_w -= dx;
        }
        _ => {}
    }

    let min_size = 4.0;
    new_w = new_w.max(min_size);
    new_h = new_h.max(min_size);

    node.transform.position = [new_x, new_y];
    match &mut node.layout_mode {
        LayoutMode::Absolute(abs) => {
            abs.width = Dimension::Px { value: new_w };
            abs.height = Dimension::Px { value: new_h };
        }
        LayoutMode::Free => {
            node.layout_mode = LayoutMode::Absolute(AbsoluteProps::fixed(new_w, new_h));
        }
        LayoutMode::Relative(f) | LayoutMode::Flow(f) => {
            f.width = Dimension::Px { value: new_w };
            f.height = Dimension::Px { value: new_h };
        }
    }
}

/// Reseat `node.layout_mode` to the named variant. Preserves
/// `FlowProps` across `Flow ↔ Relative` since those variants share
/// the same payload shape (the property panel UX expects the gap /
/// padding / direction the user already typed to survive a switch
/// to relative positioning).
fn reseat_layout_mode(node: &mut Node, variant: &str) {
    let cur = node.layout_mode.clone();
    node.layout_mode = match (variant, cur) {
        ("flow", LayoutMode::Relative(p)) => LayoutMode::Flow(p),
        ("relative", LayoutMode::Flow(p)) => LayoutMode::Relative(p),
        ("flow", _) => LayoutMode::Flow(FlowProps::default()),
        ("relative", _) => LayoutMode::Relative(FlowProps::default()),
        ("absolute", _) => LayoutMode::Absolute(AbsoluteProps::default()),
        ("free", _) => LayoutMode::Free,
        _ => node.layout_mode.clone(),
    };
}

/// Returns the canonical sub-path under `LayoutMode` that the active
/// variant currently exposes, or `None` if the variant has no payload.
/// Lets the legacy-key translator below dispatch through the derived
/// `LayoutMode::apply_path` without the caller having to know which
/// variant is active.
fn active_variant_prefix(mode: &LayoutMode) -> Option<&'static str> {
    match mode {
        LayoutMode::Flow(_) => Some("flow"),
        LayoutMode::Relative(_) => Some("relative"),
        LayoutMode::Absolute(_) => Some("absolute"),
        LayoutMode::Free => None,
    }
}

pub(super) fn apply_layout_to_node(node: &mut Node, key: &str, value: &str) {
    // Strip the dispatcher's `layout.` routing prefix.
    let sub = key.strip_prefix("layout.").unwrap_or(key);

    // `display` straddles two responsibilities in the legacy key
    // vocabulary: it both reseats the variant *and* sets a
    // `FlowDisplay` within `Flow`/`Relative`. Disambiguate by the
    // value.
    if sub == "display" {
        match value {
            "flow" | "relative" | "absolute" | "free" => reseat_layout_mode(node, value),
            "block" | "flex" | "grid" | "none" => {
                // FlowDisplay only exists inside Flow/Relative — if
                // we're currently Absolute or Free, the user picking
                // a display mode implies "switch to Flow with that
                // display." Mirrors the legacy hand-rolled behavior.
                if active_variant_prefix(&node.layout_mode).is_none()
                    || matches!(node.layout_mode, LayoutMode::Absolute(_))
                {
                    reseat_layout_mode(node, "flow");
                }
                if let Some(prefix) = active_variant_prefix(&node.layout_mode) {
                    node.layout_mode
                        .apply_path(&format!("{prefix}.display"), value);
                }
            }
            _ => {}
        }
        return;
    }

    // CSS-shorthand `padding` / `margin` (e.g. "8 16" → vertical 8,
    // horizontal 16) is parsed up front into an `Edges<f32>`, then
    // overwrites the active variant's edge struct directly. The
    // path-walker only knows how to set individual edges.
    if sub == "padding" || sub == "margin" {
        let edges = parse_edge_values(value);
        match &mut node.layout_mode {
            LayoutMode::Flow(f) | LayoutMode::Relative(f) => {
                if sub == "padding" {
                    f.padding = edges;
                } else {
                    f.margin = edges;
                }
            }
            _ => {}
        }
        return;
    }

    // Width/height live as `Dimension` (a tagged enum). Slint emits a
    // single string ("auto", "16px", "50%") via `parse_dimension`, so
    // we parse it up front and write the whole enum at once. Same
    // for `grid_column` / `grid_row` (`GridPlacement`).
    if sub == "width" || sub == "height" {
        let dim = parse_dimension(value);
        write_dimension(&mut node.layout_mode, sub, dim);
        return;
    }
    if sub == "grid_column" || sub == "grid_row" {
        let gp = parse_grid_placement(value);
        write_grid_placement(&mut node.layout_mode, sub, gp);
        return;
    }

    // Two-step UI: a `width_unit` ComboBox sets the `Dimension`
    // variant, then `width_value` LineEdit sets the active payload's
    // `value` field. Same for height + grid placements.
    if let Some(field) = sub.strip_suffix("_unit") {
        if matches!(field, "width" | "height") {
            apply_dimension_unit(&mut node.layout_mode, field, value);
            return;
        }
    }
    if let Some(field) = sub.strip_suffix("_value") {
        if matches!(field, "width" | "height") {
            apply_dimension_value(&mut node.layout_mode, field, value);
            return;
        }
    }
    if sub == "grid_column_type" || sub == "grid_row_type" {
        let field = sub.strip_suffix("_type").unwrap();
        apply_grid_placement_type(&mut node.layout_mode, field, value);
        return;
    }
    if sub == "grid_column_value" || sub == "grid_row_value" {
        let field = sub.strip_suffix("_value").unwrap();
        apply_grid_placement_value(&mut node.layout_mode, field, value);
        return;
    }

    // Per-edge writes: `padding_top` / `margin_left` / etc. translate
    // to `<variant>.<padding|margin>.<edge>` on the active variant.
    if let Some(prefix) = active_variant_prefix(&node.layout_mode) {
        if let Some(edge) = sub
            .strip_prefix("padding_")
            .or_else(|| sub.strip_prefix("margin_"))
        {
            let parent = if sub.starts_with("padding_") {
                "padding"
            } else {
                "margin"
            };
            node.layout_mode
                .apply_path(&format!("{prefix}.{parent}.{edge}"), value);
            return;
        }
        // Everything else — `gap`, `flex_direction`, `flex_grow`,
        // `flex_shrink`, `align_items`, `justify_content` — is a
        // direct field on the active variant payload. The derived
        // `apply_path` parses primitives and dispatches enum values
        // (kebab-case via serde rename_all).
        node.layout_mode
            .apply_path(&format!("{prefix}.{sub}"), value);
    }
}

fn write_dimension(mode: &mut LayoutMode, field: &str, dim: Dimension) {
    match (mode, field) {
        (LayoutMode::Flow(f) | LayoutMode::Relative(f), "width") => f.width = dim,
        (LayoutMode::Flow(f) | LayoutMode::Relative(f), "height") => f.height = dim,
        (LayoutMode::Absolute(a), "width") => a.width = dim,
        (LayoutMode::Absolute(a), "height") => a.height = dim,
        _ => {}
    }
}

fn write_grid_placement(mode: &mut LayoutMode, field: &str, gp: GridPlacement) {
    if let LayoutMode::Flow(f) | LayoutMode::Relative(f) = mode {
        match field {
            "grid_column" => f.grid_column = gp,
            "grid_row" => f.grid_row = gp,
            _ => {}
        }
    }
}

fn current_dimension(mode: &LayoutMode, field: &str) -> Dimension {
    match (mode, field) {
        (LayoutMode::Flow(f) | LayoutMode::Relative(f), "width") => f.width,
        (LayoutMode::Flow(f) | LayoutMode::Relative(f), "height") => f.height,
        (LayoutMode::Absolute(a), "width") => a.width,
        (LayoutMode::Absolute(a), "height") => a.height,
        _ => Dimension::Auto,
    }
}

fn dimension_scalar(d: Dimension) -> f32 {
    match d {
        Dimension::Px { value } | Dimension::Percent { value } => value,
        Dimension::Auto => 0.0,
    }
}

fn apply_dimension_unit(mode: &mut LayoutMode, field: &str, unit: &str) {
    let cur_value = dimension_scalar(current_dimension(mode, field));
    let dim = match unit {
        "auto" => Dimension::Auto,
        "px" => Dimension::Px { value: cur_value },
        "%" => Dimension::Percent {
            value: cur_value.min(100.0),
        },
        _ => return,
    };
    write_dimension(mode, field, dim);
}

fn apply_dimension_value(mode: &mut LayoutMode, field: &str, raw: &str) {
    let v = raw.parse::<f32>().unwrap_or(0.0);
    let dim = match current_dimension(mode, field) {
        Dimension::Px { .. } => Dimension::Px { value: v },
        Dimension::Percent { .. } => Dimension::Percent { value: v },
        Dimension::Auto => Dimension::Px { value: v },
    };
    write_dimension(mode, field, dim);
}

fn apply_grid_placement_type(mode: &mut LayoutMode, field: &str, kind: &str) {
    let (LayoutMode::Flow(f) | LayoutMode::Relative(f)) = mode else {
        return;
    };
    let cur = if field == "grid_column" {
        f.grid_column
    } else {
        f.grid_row
    };
    let new = match kind {
        "auto" => GridPlacement::Auto,
        "line" => GridPlacement::Line {
            index: match cur {
                GridPlacement::Line { index } => index,
                GridPlacement::Span { count } => count as i16,
                GridPlacement::Auto => 1,
            },
        },
        "span" => GridPlacement::Span {
            count: match cur {
                GridPlacement::Span { count } => count,
                GridPlacement::Line { index } => index.max(1) as u16,
                GridPlacement::Auto => 1,
            },
        },
        _ => cur,
    };
    if field == "grid_column" {
        f.grid_column = new;
    } else {
        f.grid_row = new;
    }
}

fn apply_grid_placement_value(mode: &mut LayoutMode, field: &str, raw: &str) {
    let (LayoutMode::Flow(f) | LayoutMode::Relative(f)) = mode else {
        return;
    };
    let v = raw.parse::<f32>().unwrap_or(0.0);
    let cur = if field == "grid_column" {
        f.grid_column
    } else {
        f.grid_row
    };
    let new = match cur {
        GridPlacement::Line { .. } => GridPlacement::Line { index: v as i16 },
        GridPlacement::Span { .. } => GridPlacement::Span {
            count: (v as u16).max(1),
        },
        GridPlacement::Auto => GridPlacement::Line { index: v as i16 },
    };
    if field == "grid_column" {
        f.grid_column = new;
    } else {
        f.grid_row = new;
    }
}

pub(super) fn parse_dimension(s: &str) -> Dimension {
    let s = s.trim();
    if s == "auto" {
        return Dimension::Auto;
    }
    if let Some(px) = s.strip_suffix("px") {
        if let Ok(v) = px.trim().parse::<f32>() {
            return Dimension::Px { value: v };
        }
    }
    if let Some(pct) = s.strip_suffix('%') {
        if let Ok(v) = pct.trim().parse::<f32>() {
            return Dimension::Percent { value: v };
        }
    }
    if let Ok(v) = s.parse::<f32>() {
        return Dimension::Px { value: v };
    }
    Dimension::Auto
}

pub(super) fn parse_grid_placement(s: &str) -> GridPlacement {
    let s = s.trim();
    if s == "auto" {
        return GridPlacement::Auto;
    }
    if let Some(rest) = s.strip_prefix("span ") {
        if let Ok(v) = rest.trim().parse::<u16>() {
            return GridPlacement::Span { count: v };
        }
    }
    if let Some(rest) = s.strip_prefix("line ") {
        if let Ok(v) = rest.trim().parse::<i16>() {
            return GridPlacement::Line { index: v };
        }
    }
    if let Ok(v) = s.parse::<i16>() {
        return GridPlacement::Line { index: v };
    }
    GridPlacement::Auto
}

pub(super) fn parse_edge_values(s: &str) -> prism_core::foundation::geometry::Edges<f32> {
    let parts: Vec<f32> = s
        .split_whitespace()
        .filter_map(|p| p.parse::<f32>().ok())
        .collect();
    match parts.len() {
        1 => prism_core::foundation::geometry::Edges::all(parts[0]),
        2 => prism_core::foundation::geometry::Edges::symmetric(parts[0], parts[1]),
        4 => prism_core::foundation::geometry::Edges::new(parts[0], parts[1], parts[2], parts[3]),
        _ => prism_core::foundation::geometry::Edges::ZERO,
    }
}
