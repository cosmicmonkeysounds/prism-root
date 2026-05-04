use prism_builder::layout::{
    AbsoluteProps, AlignOption, Dimension, FlexDirection, FlowDisplay, FlowProps, GridPlacement,
    JustifyOption, LayoutMode, PageSize,
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
    style.apply_field(key, value);
}

pub(super) fn apply_page_layout_edit(
    pl: &mut prism_builder::layout::PageLayout,
    key: &str,
    value: &str,
) {
    // `page_size` is a tagged enum with a `Custom { width, height }`
    // payload — outside the flat-struct derive's lane. Everything else
    // (margins via #[edit(nested, prefix = "margin_")] and the
    // column_gap/row_gap fan-out via #[edit(also)]) goes through the
    // derived dispatch table.
    if key == "page_size" {
        pl.size = match value {
            "Responsive" => PageSize::Responsive,
            "A4" => PageSize::A4,
            "A3" => PageSize::A3,
            "A5" => PageSize::A5,
            "Letter" => PageSize::Letter,
            "Legal" => PageSize::Legal,
            "Tabloid" => PageSize::Tabloid,
            "Custom" => PageSize::Custom {
                width: 1280.0,
                height: 800.0,
            },
            _ => pl.size,
        };
        return;
    }
    pl.apply_field(key, value);
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
    use prism_core::foundation::spatial::Anchor;
    let parse_f32 = |s: &str| s.parse::<f32>().unwrap_or(0.0);
    let t = &mut node.transform;
    match key {
        "transform.x" => t.position[0] = parse_f32(value),
        "transform.y" => t.position[1] = parse_f32(value),
        "transform.rotation" => t.rotation = parse_f32(value).to_radians(),
        "transform.scale_x" => t.scale[0] = parse_f32(value),
        "transform.scale_y" => t.scale[1] = parse_f32(value),
        "transform.anchor" => {
            t.anchor = match value {
                "top-left" => Anchor::TopLeft,
                "top-center" => Anchor::TopCenter,
                "top-right" => Anchor::TopRight,
                "center-left" => Anchor::CenterLeft,
                "center" => Anchor::Center,
                "center-right" => Anchor::CenterRight,
                "bottom-left" => Anchor::BottomLeft,
                "bottom-center" => Anchor::BottomCenter,
                "bottom-right" => Anchor::BottomRight,
                "stretch" => Anchor::Stretch,
                _ => t.anchor,
            };
        }
        _ => {}
    }
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

pub(super) fn apply_layout_to_node(node: &mut Node, key: &str, value: &str) {
    let parse_f32 = |s: &str| s.parse::<f32>().unwrap_or(0.0);

    // Handle Absolute mode width/height edits directly.
    if let LayoutMode::Absolute(abs) = &mut node.layout_mode {
        match key {
            "layout.display" => match value {
                "absolute" => return,
                "free" => {
                    node.layout_mode = LayoutMode::Free;
                    return;
                }
                "relative" => {
                    node.layout_mode = LayoutMode::Relative(FlowProps::default());
                    return;
                }
                _ => {
                    node.layout_mode = LayoutMode::Flow(FlowProps::default());
                }
            },
            "layout.width_unit" => {
                let cur = match abs.width {
                    Dimension::Px { value } | Dimension::Percent { value } => value,
                    Dimension::Auto => 0.0,
                };
                abs.width = match value {
                    "auto" => Dimension::Auto,
                    "px" => Dimension::Px { value: cur },
                    "%" => Dimension::Percent {
                        value: cur.min(100.0),
                    },
                    _ => abs.width,
                };
                return;
            }
            "layout.width_value" => {
                let v = value.parse::<f32>().unwrap_or(0.0);
                abs.width = match abs.width {
                    Dimension::Px { .. } => Dimension::Px { value: v },
                    Dimension::Percent { .. } => Dimension::Percent { value: v },
                    Dimension::Auto => Dimension::Px { value: v },
                };
                return;
            }
            "layout.height_unit" => {
                let cur = match abs.height {
                    Dimension::Px { value } | Dimension::Percent { value } => value,
                    Dimension::Auto => 0.0,
                };
                abs.height = match value {
                    "auto" => Dimension::Auto,
                    "px" => Dimension::Px { value: cur },
                    "%" => Dimension::Percent {
                        value: cur.min(100.0),
                    },
                    _ => abs.height,
                };
                return;
            }
            "layout.height_value" => {
                let v = value.parse::<f32>().unwrap_or(0.0);
                abs.height = match abs.height {
                    Dimension::Px { .. } => Dimension::Px { value: v },
                    Dimension::Percent { .. } => Dimension::Percent { value: v },
                    Dimension::Auto => Dimension::Px { value: v },
                };
                return;
            }
            _ => return,
        }
    }

    let flow = match &mut node.layout_mode {
        LayoutMode::Flow(f) | LayoutMode::Relative(f) => f,
        LayoutMode::Free => {
            if key == "layout.display" && value != "free" {
                match value {
                    "absolute" => {
                        node.layout_mode = LayoutMode::Absolute(AbsoluteProps::default());
                        return;
                    }
                    "relative" => {
                        node.layout_mode = LayoutMode::Relative(FlowProps::default());
                        return;
                    }
                    _ => {
                        node.layout_mode = LayoutMode::Flow(FlowProps::default());
                    }
                }
                match &mut node.layout_mode {
                    LayoutMode::Flow(f) => f,
                    _ => unreachable!(),
                }
            } else {
                return;
            }
        }
        LayoutMode::Absolute(_) => unreachable!(),
    };

    match key {
        "layout.display" => match value {
            "block" => flow.display = FlowDisplay::Block,
            "flex" => flow.display = FlowDisplay::Flex,
            "grid" => flow.display = FlowDisplay::Grid,
            "none" => flow.display = FlowDisplay::None,
            "free" => {
                node.layout_mode = LayoutMode::Free;
            }
            "absolute" => {
                node.layout_mode = LayoutMode::Absolute(AbsoluteProps::default());
            }
            "relative" => {
                node.layout_mode = LayoutMode::Relative(flow.clone());
            }
            _ => {}
        },
        "layout.width" => flow.width = parse_dimension(value),
        "layout.height" => flow.height = parse_dimension(value),
        "layout.gap" => flow.gap = parse_f32(value),
        "layout.flex_direction" => {
            flow.flex_direction = match value {
                "row" => FlexDirection::Row,
                "column" => FlexDirection::Column,
                "row-reverse" => FlexDirection::RowReverse,
                "column-reverse" => FlexDirection::ColumnReverse,
                _ => flow.flex_direction,
            };
        }
        "layout.flex_grow" => flow.flex_grow = parse_f32(value),
        "layout.flex_shrink" => flow.flex_shrink = parse_f32(value),
        "layout.align_items" => {
            flow.align_items = match value {
                "auto" => AlignOption::Auto,
                "start" => AlignOption::Start,
                "end" => AlignOption::End,
                "center" => AlignOption::Center,
                "stretch" => AlignOption::Stretch,
                "baseline" => AlignOption::Baseline,
                _ => flow.align_items,
            };
        }
        "layout.justify_content" => {
            flow.justify_content = match value {
                "start" => JustifyOption::Start,
                "end" => JustifyOption::End,
                "center" => JustifyOption::Center,
                "space-between" => JustifyOption::SpaceBetween,
                "space-around" => JustifyOption::SpaceAround,
                "space-evenly" => JustifyOption::SpaceEvenly,
                "stretch" => JustifyOption::Stretch,
                _ => flow.justify_content,
            };
        }
        "layout.grid_column" => flow.grid_column = parse_grid_placement(value),
        "layout.grid_row" => flow.grid_row = parse_grid_placement(value),
        "layout.padding" => {
            let vals = parse_edge_values(value);
            flow.padding = vals;
        }
        "layout.padding_top" => flow.padding.top = parse_f32(value),
        "layout.padding_right" => flow.padding.right = parse_f32(value),
        "layout.padding_bottom" => flow.padding.bottom = parse_f32(value),
        "layout.padding_left" => flow.padding.left = parse_f32(value),
        "layout.margin" => {
            let vals = parse_edge_values(value);
            flow.margin = vals;
        }
        "layout.margin_top" => flow.margin.top = parse_f32(value),
        "layout.margin_right" => flow.margin.right = parse_f32(value),
        "layout.margin_bottom" => flow.margin.bottom = parse_f32(value),
        "layout.margin_left" => flow.margin.left = parse_f32(value),
        "layout.width_unit" => {
            let current_value = match flow.width {
                Dimension::Px { value } => value,
                Dimension::Percent { value } => value,
                Dimension::Auto => 0.0,
            };
            flow.width = match value {
                "auto" => Dimension::Auto,
                "px" => Dimension::Px {
                    value: current_value,
                },
                "%" => Dimension::Percent {
                    value: current_value.min(100.0),
                },
                _ => flow.width,
            };
        }
        "layout.width_value" => {
            let v = parse_f32(value);
            flow.width = match flow.width {
                Dimension::Px { .. } => Dimension::Px { value: v },
                Dimension::Percent { .. } => Dimension::Percent { value: v },
                Dimension::Auto => Dimension::Px { value: v },
            };
        }
        "layout.height_unit" => {
            let current_value = match flow.height {
                Dimension::Px { value } => value,
                Dimension::Percent { value } => value,
                Dimension::Auto => 0.0,
            };
            flow.height = match value {
                "auto" => Dimension::Auto,
                "px" => Dimension::Px {
                    value: current_value,
                },
                "%" => Dimension::Percent {
                    value: current_value.min(100.0),
                },
                _ => flow.height,
            };
        }
        "layout.height_value" => {
            let v = parse_f32(value);
            flow.height = match flow.height {
                Dimension::Px { .. } => Dimension::Px { value: v },
                Dimension::Percent { .. } => Dimension::Percent { value: v },
                Dimension::Auto => Dimension::Px { value: v },
            };
        }
        "layout.grid_column_type" => {
            flow.grid_column = match value {
                "auto" => GridPlacement::Auto,
                "line" => GridPlacement::Line {
                    index: match flow.grid_column {
                        GridPlacement::Line { index } => index,
                        GridPlacement::Span { count } => count as i16,
                        GridPlacement::Auto => 1,
                    },
                },
                "span" => GridPlacement::Span {
                    count: match flow.grid_column {
                        GridPlacement::Span { count } => count,
                        GridPlacement::Line { index } => index.max(1) as u16,
                        GridPlacement::Auto => 1,
                    },
                },
                _ => flow.grid_column,
            };
        }
        "layout.grid_column_value" => {
            let v = parse_f32(value);
            flow.grid_column = match flow.grid_column {
                GridPlacement::Line { .. } => GridPlacement::Line { index: v as i16 },
                GridPlacement::Span { .. } => GridPlacement::Span {
                    count: (v as u16).max(1),
                },
                GridPlacement::Auto => GridPlacement::Line { index: v as i16 },
            };
        }
        "layout.grid_row_type" => {
            flow.grid_row = match value {
                "auto" => GridPlacement::Auto,
                "line" => GridPlacement::Line {
                    index: match flow.grid_row {
                        GridPlacement::Line { index } => index,
                        GridPlacement::Span { count } => count as i16,
                        GridPlacement::Auto => 1,
                    },
                },
                "span" => GridPlacement::Span {
                    count: match flow.grid_row {
                        GridPlacement::Span { count } => count,
                        GridPlacement::Line { index } => index.max(1) as u16,
                        GridPlacement::Auto => 1,
                    },
                },
                _ => flow.grid_row,
            };
        }
        "layout.grid_row_value" => {
            let v = parse_f32(value);
            flow.grid_row = match flow.grid_row {
                GridPlacement::Line { .. } => GridPlacement::Line { index: v as i16 },
                GridPlacement::Span { .. } => GridPlacement::Span {
                    count: (v as u16).max(1),
                },
                GridPlacement::Auto => GridPlacement::Line { index: v as i16 },
            };
        }
        _ => {}
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
