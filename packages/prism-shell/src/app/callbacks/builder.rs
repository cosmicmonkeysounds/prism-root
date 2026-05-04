use std::cell::RefCell;
use std::rc::Rc;

use prism_builder::starter::{builtin_prefab, card_prefab_def, materialize_prefab};
use prism_builder::{
    compute_layout, path_from_string, CellEdge, FacetDataSource, FacetDef, FacetKind, FacetLayout,
    FacetOutput, FacetTemplate,
};
use serde_json::json;
use slint::ComponentHandle;

use super::super::{
    apply_drag_to_node, apply_resize_to_node, default_props_for_component, find_node_layout_size,
    find_node_transform, is_preview_mode, sync_ui_from_shared, DragSnapshot, GapResizeSnapshot,
    ResizeSnapshot, Shell, ShellInner, SourceSnapshot,
};
use super::with_shell;
use crate::AppWindow;

impl Shell {
    pub(super) fn wire_builder_callbacks(&self) {
        let weak = self.window.as_weak();
        let inner = Rc::clone(&self.inner);

        fn handle_node_move(
            inner: &Rc<RefCell<ShellInner>>,
            weak: &slint::Weak<AppWindow>,
            node_id: &slint::SharedString,
            direction: i32,
            label: &str,
        ) {
            if is_preview_mode(&inner.borrow().store.state().workspace) {
                return;
            }
            {
                let mut s = inner.borrow_mut();
                let nid = if node_id.is_empty() {
                    match s.store.state().selection.primary().cloned() {
                        Some(id) => id,
                        None => return,
                    }
                } else {
                    node_id.to_string()
                };
                s.push_undo(label);
                if let Some(ref mut live) = s.live {
                    let _ = live.move_node_in_source(&nid, direction);
                }
                s.sync_builder_document();
            }
            if let Some(w) = weak.upgrade() {
                sync_ui_from_shared(inner, &w);
            }
        }

        // Builder node clicked — select in edit mode, fire signal in preview mode
        self.window.on_builder_node_clicked({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |node_id| {
                eprintln!("[click] builder_node_clicked id={node_id}");
                with_shell(&inner, &weak, |s| {
                    let nid = node_id.to_string();
                    s.store.mutate(|state| {
                        state.selection.select(nid.clone());
                    });
                    if is_preview_mode(&s.store.state().workspace) {
                        s.fire_signal(&nid, "clicked", serde_json::Map::new());
                    } else if let Some(ref live) = s.live {
                        if let Some(sel) = live.select_node(&nid) {
                            s.store.mutate(|state| {
                                state
                                    .editor_state
                                    .set_cursor_position(sel.start_line, sel.start_col);
                                state
                                    .editor_state
                                    .extend_selection_to(sel.end_line, sel.end_col);
                            });
                        }
                    }
                });
            }
        });

        // Builder node double-clicked — fire double-clicked signal in preview mode
        self.window.on_builder_node_double_clicked({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |node_id| {
                {
                    let nid = node_id.to_string();
                    let mut s = inner.borrow_mut();
                    if is_preview_mode(&s.store.state().workspace) {
                        s.fire_signal(&nid, "double-clicked", serde_json::Map::new());
                    }
                }
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });

        // Builder node hovered — fire hovered signal (preview mode only)
        self.window.on_builder_node_hovered({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |node_id, x, y| {
                let dispatched = {
                    let nid = node_id.to_string();
                    let mut s = inner.borrow_mut();
                    s.fire_signal(&nid, "hovered", {
                        let mut p = serde_json::Map::new();
                        p.insert("x".into(), serde_json::Value::from(x as f64));
                        p.insert("y".into(), serde_json::Value::from(y as f64));
                        p
                    })
                };
                if dispatched {
                    if let Some(w) = weak.upgrade() {
                        sync_ui_from_shared(&inner, &w);
                    }
                }
            }
        });

        self.window.on_builder_node_hover_ended({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |node_id| {
                let dispatched = {
                    let nid = node_id.to_string();
                    let mut s = inner.borrow_mut();
                    s.fire_signal(&nid, "hover-ended", serde_json::Map::new())
                };
                if dispatched {
                    if let Some(w) = weak.upgrade() {
                        sync_ui_from_shared(&inner, &w);
                    }
                }
            }
        });

        // Inline text editing in builder
        self.window.on_builder_text_edited({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |node_id, value| {
                if is_preview_mode(&inner.borrow().store.state().workspace) {
                    return;
                }
                let node_id = node_id.to_string();
                let value = value.to_string();
                with_shell(&inner, &weak, |s| {
                    s.push_undo("Edit text");
                    let key = {
                        let component = s.live.as_mut().and_then(|l| {
                            let doc = l.document();
                            doc.root
                                .as_ref()
                                .and_then(|r| r.find(&node_id).map(|n| n.component.clone()))
                        });
                        match component.as_deref() {
                            Some("text") => "body",
                            Some("card") | Some("accordion") => "title",
                            Some("code") => "code",
                            _ => "text",
                        }
                    };
                    let formatted = format!(
                        "\"{}\"",
                        prism_builder::slint_source::escape_slint_string(&value)
                    );
                    if let Some(ref mut live) = s.live {
                        let _ = live.edit_prop_in_source(&node_id, key, &formatted);
                    }
                    s.sync_builder_document();
                });
            }
        });

        // Delete node from builder
        self.window.on_builder_delete_node({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |node_id| {
                if is_preview_mode(&inner.borrow().store.state().workspace) {
                    return;
                }
                with_shell(&inner, &weak, |s| {
                    let nid = if node_id.is_empty() {
                        match s.store.state().selection.primary().cloned() {
                            Some(id) => id,
                            None => return,
                        }
                    } else {
                        node_id.to_string()
                    };
                    s.fire_signal(&nid, "deleted", serde_json::Map::new());
                    s.push_undo("Delete node");
                    if let Some(ref mut live) = s.live {
                        let _ = live.remove_node_from_source(&nid);
                    }
                    s.store.mutate(|state| {
                        state.selection.clear();
                    });
                    s.sync_builder_document();
                });
            }
        });

        // Add component from palette
        self.window.on_add_component({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |component_type| {
                if is_preview_mode(&inner.borrow().store.state().workspace) {
                    return;
                }
                let ct = component_type.to_string();
                with_shell(&inner, &weak, |s| {
                    s.push_undo(&format!("Add {ct}"));
                    let parent_id = s.store.state().selection.primary().cloned();

                    // Look up user-defined prefabs from the live document.
                    let user_prefab = s.store.state().builder_document.prefabs.get(&ct).cloned();
                    let prefab_def = builtin_prefab(&ct).or(user_prefab);

                    let mounted_id;
                    if let Some(prefab_def) = prefab_def {
                        let mut counter = s.store.state().next_node_id;
                        let tree = materialize_prefab(&prefab_def, &mut counter);
                        let root_id = tree.id.clone();
                        if let Some(ref mut live) = s.live {
                            let _ = live.insert_tree_in_source(parent_id.as_deref(), &tree, None);
                        }
                        mounted_id = root_id.clone();
                        s.store.mutate(|state| {
                            state.next_node_id = counter;
                            state.selection.select(root_id);
                        });
                    } else if ct == "facet" {
                        let counter = s.store.state().next_node_id;
                        let facet_id = format!("facet:n{counter}");
                        let node_id = format!("n{counter}");
                        let props = json!({ "facet_id": facet_id });
                        if let Some(ref mut live) = s.live {
                            let _ = live.insert_node_in_source(
                                parent_id.as_deref(),
                                &ct,
                                &node_id,
                                &props,
                                None,
                            );
                        }
                        mounted_id = node_id.clone();
                        let nid = node_id.clone();
                        s.store.mutate(|state| {
                            state.next_node_id += 1;
                            state.selection.select(nid);
                            if let Some(doc) =
                                state.active_app_mut().and_then(|a| a.active_document_mut())
                            {
                                // Ensure "card" prefab is available for facet rendering.
                                doc.prefabs
                                    .entry("card".into())
                                    .or_insert_with(card_prefab_def);
                                doc.facets.insert(
                                    facet_id.clone(),
                                    FacetDef {
                                        id: facet_id.clone(),
                                        label: "New Facet".into(),
                                        description: String::new(),
                                        kind: FacetKind::List,
                                        schema_id: None,
                                        template: FacetTemplate::default(),
                                        output: FacetOutput::default(),
                                        data: FacetDataSource::Static {
                                            items: vec![],
                                            records: vec![],
                                        },
                                        bindings: vec![],
                                        variant_rules: vec![],
                                        layout: FacetLayout::default(),
                                        resolved_data: None,
                                    },
                                );
                            }
                        });
                    } else {
                        let node_id = format!("n{}", s.store.state().next_node_id);
                        let props = default_props_for_component(&ct);
                        if let Some(ref mut live) = s.live {
                            let _ = live.insert_node_in_source(
                                parent_id.as_deref(),
                                &ct,
                                &node_id,
                                &props,
                                None,
                            );
                        }
                        mounted_id = node_id.clone();
                        let nid = node_id.clone();
                        s.store.mutate(|state| {
                            state.next_node_id += 1;
                            state.selection.select(nid);
                        });
                    }
                    s.sync_builder_document();
                    s.fire_signal(&mounted_id, "mounted", serde_json::Map::new());
                });
            }
        });

        // Inspector node selection
        self.window.on_node_clicked({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |node_id| {
                inner.borrow_mut().store.mutate(|state| {
                    state.selection.select(node_id.to_string());
                });
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });
        self.window.on_node_move_up({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |node_id| handle_node_move(&inner, &weak, &node_id, -1, "Move node up")
        });
        self.window.on_node_move_down({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |node_id| handle_node_move(&inner, &weak, &node_id, 1, "Move node down")
        });

        // Node drag — snapshot initial transform on press
        self.window.on_node_drag_started({
            let inner = Rc::clone(&inner);
            move |node_id| {
                if is_preview_mode(&inner.borrow().store.state().workspace) {
                    return;
                }
                let nid = node_id.to_string();
                let mut s = inner.borrow_mut();
                if let Some(ref mut live) = s.live {
                    let pre_drag_source = live.source.clone();
                    let doc = live.document();
                    if let Some(ref root) = doc.root {
                        if let Some(t) = find_node_transform(root, &nid) {
                            s.drag_initial_transform = Some(DragSnapshot {
                                node_id: nid.clone(),
                                position: t.position,
                                rotation: t.rotation,
                                scale: t.scale,
                                pre_drag_source,
                            });
                        }
                    }
                }
                s.fire_signal(&nid, "drag-started", serde_json::Map::new());
            }
        });

        // Node drag — live update while dragging (delta from press point)
        self.window.on_node_drag_moved({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |node_id, tool, dx, dy, shift| {
                if is_preview_mode(&inner.borrow().store.state().workspace) {
                    return;
                }
                if inner.borrow().syncing.get() {
                    return;
                }
                let nid = node_id.to_string();
                let tool = tool.to_string();
                with_shell(&inner, &weak, |s| {
                    let snap = s.drag_initial_transform.clone();
                    if let (Some(ref snap), Some(ref mut live)) = (&snap, &mut s.live) {
                        if snap.node_id == nid {
                            let _ = live.mutate_document(|doc| {
                                if let Some(ref mut root) = doc.root {
                                    apply_drag_to_node(root, &tool, dx, dy, shift, snap);
                                }
                            });
                        }
                    }
                    s.sync_builder_document();
                });
            }
        });

        // Node drag — commit final transform with undo
        self.window.on_node_drag_finished({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |node_id, _tool, _dx, _dy, _shift| {
                if is_preview_mode(&inner.borrow().store.state().workspace) {
                    return;
                }
                if inner.borrow().syncing.get() {
                    return;
                }
                let nid = node_id.to_string();
                with_shell(&inner, &weak, |s| {
                    let snap = s.drag_initial_transform.take();
                    if let Some(snap) = snap {
                        if snap.node_id == nid {
                            let desc = match _tool.to_string().as_str() {
                                "rotate" => "Rotate node",
                                "scale" => "Scale node",
                                _ => "Move node",
                            };
                            let selection = s.store.state().selection.clone();
                            s.undo_past.push(SourceSnapshot {
                                description: desc.into(),
                                source: snap.pre_drag_source,
                                selection,
                            });
                            s.undo_future.clear();
                            if s.undo_past.len() > 100 {
                                s.undo_past.remove(0);
                            }
                        }
                    }
                    s.sync_builder_document();
                    s.fire_signal(&nid, "drag-ended", serde_json::Map::new());
                });
            }
        });

        // Node resize — snapshot initial position + dimensions on press
        self.window.on_node_resize_started({
            let inner = Rc::clone(&inner);
            move |node_id, _handle| {
                if is_preview_mode(&inner.borrow().store.state().workspace) {
                    return;
                }
                use prism_core::foundation::geometry::Size2;
                let nid = node_id.to_string();
                let mut s = inner.borrow_mut();
                let state = s.store.state();
                let doc = &state.builder_document;
                if let Some(ref root) = doc.root {
                    let vp = Size2::new(state.viewport_width, 800.0);
                    let layout = compute_layout(doc, vp);
                    if let Some(t) = find_node_transform(root, &nid) {
                        let (w, h) =
                            find_node_layout_size(root, &nid, &layout).unwrap_or((100.0, 100.0));
                        s.resize_initial = Some(ResizeSnapshot {
                            node_id: nid,
                            position: t.position,
                            width: w,
                            height: h,
                        });
                    }
                }
            }
        });

        // Node resize — live update while dragging
        self.window.on_node_resize_moved({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |node_id, handle, dx, dy, shift| {
                if is_preview_mode(&inner.borrow().store.state().workspace) {
                    return;
                }
                if inner.borrow().syncing.get() {
                    return;
                }
                let nid = node_id.to_string();
                let handle = handle.to_string();
                with_shell(&inner, &weak, |s| {
                    let snap = s.resize_initial.clone();
                    if let (Some(ref snap), Some(ref mut live)) = (&snap, &mut s.live) {
                        if snap.node_id == nid {
                            let _ = live.mutate_document(|doc| {
                                if let Some(ref mut root) = doc.root {
                                    apply_resize_to_node(root, &handle, dx, dy, shift, snap);
                                }
                            });
                        }
                    }
                    s.sync_builder_document();
                });
            }
        });

        // Node resize — commit final size with undo
        self.window.on_node_resize_finished({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |node_id, handle, dx, dy, shift| {
                if is_preview_mode(&inner.borrow().store.state().workspace) {
                    return;
                }
                if inner.borrow().syncing.get() {
                    return;
                }
                let nid = node_id.to_string();
                let handle = handle.to_string();
                with_shell(&inner, &weak, |s| {
                    let snap = s.resize_initial.take();
                    if let Some(snap) = snap {
                        if snap.node_id == nid {
                            s.push_undo("Resize node");
                            if let Some(ref mut live) = s.live {
                                let _ = live.mutate_document(|doc| {
                                    if let Some(ref mut root) = doc.root {
                                        apply_resize_to_node(root, &handle, dx, dy, shift, &snap);
                                    }
                                });
                            }
                        }
                    }
                    s.sync_builder_document();
                });
            }
        });

        // Grid cell clicked (select node or open palette for empty cell)
        self.window.on_grid_cell_clicked({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |path| {
                if is_preview_mode(&inner.borrow().store.state().workspace) {
                    return;
                }
                with_shell(&inner, &weak, |s| {
                    let occupant_id = {
                        let doc = &s.store.state().builder_document;
                        let cell_path = path_from_string(path.as_str());
                        doc.page_layout
                            .grid
                            .as_ref()
                            .and_then(|g| g.at(&cell_path))
                            .and_then(|c| c.node_id().map(String::from))
                    };
                    if let Some(id) = occupant_id {
                        s.store.mutate(|state| {
                            state.selection.select(id);
                        });
                    }
                });
            }
        });

        // Grid add at edge (recursive subdivision)
        self.window.on_grid_add_at_edge({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |cell_path, edge| {
                if is_preview_mode(&inner.borrow().store.state().workspace) {
                    return;
                }
                with_shell(&inner, &weak, |s| {
                    s.push_undo("Add cell");
                    let path = path_from_string(cell_path.as_str());
                    let cell_edge = match edge.as_str() {
                        "top" => CellEdge::Top,
                        "bottom" => CellEdge::Bottom,
                        "left" => CellEdge::Left,
                        "right" => CellEdge::Right,
                        _ => return,
                    };
                    s.store.mutate(|state| {
                        let _ = state
                            .builder_document
                            .page_layout
                            .insert_at_edge(&path, cell_edge);
                    });
                });
            }
        });

        // Gap resize — snapshot track sizes on press
        self.window.on_grid_gap_resize_started({
            let inner = Rc::clone(&inner);
            move |parent_path, gap_index| {
                if is_preview_mode(&inner.borrow().store.state().workspace) {
                    return;
                }
                let mut s = inner.borrow_mut();
                s.push_undo("Resize gap");
                let pp = path_from_string(parent_path.as_str());
                let gi = gap_index as usize;
                let doc = &s.store.state().builder_document;
                let resolved = doc.page_layout.resolved_size();
                let vw = s.store.state().viewport_width;
                let (pw, ph) = resolved
                    .map(|sz| (sz.width, sz.height))
                    .unwrap_or((vw, vw * 0.625));
                let content_w = pw - doc.page_layout.margins.left - doc.page_layout.margins.right;
                let content_h = ph - doc.page_layout.margins.top - doc.page_layout.margins.bottom;

                if let Some(grid) = &doc.page_layout.grid {
                    let parent = if pp.is_empty() {
                        Some(grid)
                    } else {
                        grid.at(&pp)
                    };
                    if let Some(prism_builder::GridCell::Split {
                        direction, tracks, ..
                    }) = parent
                    {
                        if gi + 1 < tracks.len() {
                            let available = match direction {
                                prism_builder::SplitDirection::Horizontal => content_w,
                                prism_builder::SplitDirection::Vertical => content_h,
                            };
                            s.gap_resize_snapshot = Some(GapResizeSnapshot {
                                parent_path: pp,
                                gap_index: gi,
                                track_a: tracks[gi],
                                track_b: tracks[gi + 1],
                                available,
                            });
                        }
                    }
                }
            }
        });

        // Gap resize — live update while dragging (cumulative delta from press)
        self.window.on_grid_gap_resize_moved({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |_parent_path, _gap_index, delta| {
                if is_preview_mode(&inner.borrow().store.state().workspace) {
                    return;
                }
                if inner.borrow().syncing.get() {
                    return;
                }
                with_shell(&inner, &weak, |s| {
                    let snap = s.gap_resize_snapshot.clone();
                    if let Some(ref snap) = snap {
                        s.store.mutate(|state| {
                            let layout = &mut state.builder_document.page_layout;
                            let grid = match layout.grid.as_mut() {
                                Some(g) => g,
                                None => return,
                            };
                            let parent = if snap.parent_path.is_empty() {
                                grid
                            } else {
                                match grid.at_mut(&snap.parent_path) {
                                    Some(p) => p,
                                    None => return,
                                }
                            };
                            if let prism_builder::GridCell::Split { tracks, .. } = parent {
                                if snap.gap_index + 1 < tracks.len() {
                                    tracks[snap.gap_index] = snap.track_a;
                                    tracks[snap.gap_index + 1] = snap.track_b;
                                }
                            }
                            let _ = layout.resize_gap(
                                &snap.parent_path,
                                snap.gap_index,
                                delta,
                                snap.available,
                            );
                        });
                    }
                });
            }
        });

        // Gap resize — finished
        self.window.on_grid_gap_resize_finished({
            let inner = Rc::clone(&inner);
            move || {
                inner.borrow_mut().gap_resize_snapshot = None;
            }
        });

        // Inspector delete-track (parses "cell:<path>" id format)
        self.window.on_inspector_delete_track({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |id| {
                if is_preview_mode(&inner.borrow().store.state().workspace) {
                    return;
                }
                let id_str = id.as_str();
                if let Some(path_str) = id_str.strip_prefix("cell:") {
                    let path = path_from_string(path_str);
                    with_shell(&inner, &weak, |s| {
                        s.push_undo("Remove cell");
                        s.store.mutate(|state| {
                            let _ = state.builder_document.page_layout.remove_cell(&path);
                        });
                    });
                }
            }
        });

        // Grid cell add component
        self.window.on_grid_cell_add_component({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |component_type, path| {
                if is_preview_mode(&inner.borrow().store.state().workspace) {
                    return;
                }
                let ct = component_type.to_string();
                let path_str = path.to_string();
                with_shell(&inner, &weak, |s| {
                    s.push_undo(&format!("Add {ct} at {path_str}"));

                    let user_prefab_grid =
                        s.store.state().builder_document.prefabs.get(&ct).cloned();
                    if let Some(prefab_def) = builtin_prefab(&ct).or(user_prefab_grid) {
                        let mut counter = s.store.state().next_node_id;
                        let tree = materialize_prefab(&prefab_def, &mut counter);
                        let root_id = tree.id.clone();
                        if let Some(ref mut live) = s.live {
                            let _ = live.insert_tree_in_source(Some("root"), &tree, None);
                        }
                        let cell_path = path_from_string(&path_str);
                        s.store.mutate(|state| {
                            state.next_node_id = counter;
                            state.selection.select(root_id.clone());
                            state
                                .builder_document
                                .page_layout
                                .place_node_at(&cell_path, root_id)
                                .ok();
                        });
                    } else {
                        let node_id = format!("n{}", s.store.state().next_node_id);
                        let props = default_props_for_component(&ct);
                        if let Some(ref mut live) = s.live {
                            let _ = live.insert_node_in_source(
                                Some("root"),
                                &ct,
                                &node_id,
                                &props,
                                None,
                            );
                        }
                        let nid = node_id.clone();
                        let cell_path = path_from_string(&path_str);
                        s.store.mutate(|state| {
                            state.next_node_id += 1;
                            state.selection.select(nid.clone());
                            state
                                .builder_document
                                .page_layout
                                .place_node_at(&cell_path, nid)
                                .ok();
                        });
                    }
                    s.sync_builder_document();
                    s.drag_component_type.clear();
                    s.pending_picker = None;
                });
            }
        });
    }
}
