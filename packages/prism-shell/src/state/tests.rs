use super::*;

fn doc_with_three_nodes() -> prism_builder::BuilderDocument {
    use prism_builder::{BuilderDocument, Node};
    BuilderDocument {
        root: Some(Node {
            id: "root".into(),
            component: "container".into(),
            children: vec![
                Node {
                    id: "heading".into(),
                    component: "text".into(),
                    props: json!({ "body": "Hello", "level": "h1" }),
                    ..Default::default()
                },
                Node {
                    id: "btn".into(),
                    component: "button".into(),
                    props: json!({ "label": "Go" }),
                    ..Default::default()
                },
            ],
            ..Default::default()
        }),
        ..Default::default()
    }
}

#[test]
fn resync_builds_inspector_tree_depth_first_with_selection_flag() {
    // §43 C1: every node in the document shows up as an inspector
    // row, depth-first, with `selected` set on the matching id.
    let mut state = AppState::default();
    state.canvas.document = doc_with_three_nodes();
    state.canvas.selection = Some("btn".into());
    state.resync_builder_for_selection(None);
    let ids: Vec<&str> = state
        .builder
        .inspector
        .iter()
        .map(|n| n.id.as_str())
        .collect();
    assert_eq!(ids, vec!["root", "heading", "btn"]);
    let selected_ids: Vec<&str> = state
        .builder
        .inspector
        .iter()
        .filter(|n| n.selected)
        .map(|n| n.id.as_str())
        .collect();
    assert_eq!(selected_ids, vec!["btn"]);
}

#[test]
fn resync_builds_property_rows_from_selected_node_schema() {
    // §43 C1: with a registry, the selected node's schema lowers
    // to property rows. Without a registry, the rows stay empty.
    let mut reg = prism_builder::ComponentRegistry::new();
    prism_builder::starter::register_builtins(&mut reg).expect("builtins");
    let mut state = AppState::default();
    state.canvas.document = doc_with_three_nodes();
    state.canvas.selection = Some("heading".into());

    state.resync_builder_for_selection(Some(&reg));
    let rows = &state.builder.property_rows;
    assert!(
        !rows.is_empty(),
        "expected property rows for `text` component"
    );
    assert_eq!(rows[0].component, "shell.section-header");
    let editors: Vec<&PropertyRow> = rows
        .iter()
        .filter(|r| r.component == "shell.field-editor")
        .collect();
    assert!(
        !editors.is_empty(),
        "expected at least one field-editor row"
    );

    // Without a registry, no rows are derived — keeps headless
    // and partially-loaded paths working.
    state.resync_builder_for_selection(None);
    assert!(state.builder.property_rows.is_empty());
}

#[test]
fn selection_change_repopulates_property_rows() {
    // §43 E2: the named verification test for Phase C. Moving the
    // selection from one node to another re-derives the
    // properties form from the *new* node's schema — old rows are
    // not carried over.
    let mut reg = prism_builder::ComponentRegistry::new();
    prism_builder::starter::register_builtins(&mut reg).expect("builtins");
    let mut state = AppState::default();
    state.canvas.document = doc_with_three_nodes();

    // Helper: collect `key` strings from every `shell.field-editor`
    // row's `props` payload.
    fn keys_of(rows: &[PropertyRow]) -> Vec<String> {
        rows.iter()
            .filter(|r| r.component == "shell.field-editor")
            .filter_map(|r| {
                r.props
                    .get("key")
                    .and_then(|v| v.as_str())
                    .map(String::from)
            })
            .collect()
    }

    // Select the `text` heading first → property rows derived from
    // the `text` schema (must include the `body` key).
    state.canvas.selection = Some("heading".into());
    state.resync_builder_for_selection(Some(&reg));
    let heading_keys = keys_of(&state.builder.property_rows);
    assert!(
        heading_keys.iter().any(|k| k == "body"),
        "text schema must include `body`, got {heading_keys:?}"
    );

    // Switch the selection to the `button` node — the property
    // rows must repopulate from the *new* schema. The `text`
    // and `disabled` fields are on `button` but not on `text`,
    // pinning the swap.
    assert!(state.select_node("btn", Some(&reg)));
    let button_keys = keys_of(&state.builder.property_rows);
    assert!(
        button_keys.iter().any(|k| k == "text"),
        "button schema must include `text`, got {button_keys:?}"
    );
    assert!(
        button_keys.iter().any(|k| k == "disabled"),
        "button schema must include `disabled`, got {button_keys:?}"
    );
    assert!(
        !button_keys.iter().any(|k| k == "body"),
        "stale `body` field from previous selection must clear"
    );

    // Section header repopulates with the new component label.
    let header = state
        .builder
        .property_rows
        .iter()
        .find(|r| r.component == "shell.section-header")
        .expect("section header row");
    let header_label = header
        .props
        .get("label")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert_eq!(
        header_label, "button",
        "section header must reflect new selection"
    );
}

/// Wave 1.3 of `docs/dev/composable-builder-plan.md` — pin the
/// modifier-sections derivation. Each attached `node.modifiers`
/// entry emits one `shell.modifier-header` row plus its schema
/// rows; an `shell.add-modifier-button` row trails the stack as
/// the footer.
#[test]
fn derive_property_rows_emits_section_per_attached_modifier() {
    use prism_builder::{Modifier, ModifierKind};
    let mut reg = prism_builder::ComponentRegistry::new();
    prism_builder::starter::register_builtins(&mut reg).expect("builtins");
    let mod_reg = std::sync::Arc::new(prism_builder::ModifierRegistry::with_builtins());

    let mut doc = doc_with_three_nodes();
    // Attach two modifiers to the button.
    let btn = doc.root.as_mut().unwrap().find_mut("btn").unwrap();
    btn.modifiers.push(
        Modifier::from_kind(ModifierKind::Tooltip).with_props(json!({
            "text": "Click me",
            "placement": "top",
        })),
    );
    btn.modifiers
        .push(Modifier::from_kind(ModifierKind::ResponsiveVisibility));

    let mut state = AppState::default();
    state.canvas.document = doc;
    state.canvas.selection = Some("btn".into());
    state.modifier_registry = Some(std::sync::Arc::clone(&mod_reg));
    state.resync_builder_for_selection(Some(&reg));

    // Row structure: section-header(button) + button schema rows
    // + modifier-header(Tooltip) + tooltip schema rows
    // + modifier-header(Responsive Visibility) + responsive rows
    // + add-modifier-button footer.
    let kinds: Vec<&str> = state
        .builder
        .property_rows
        .iter()
        .map(|r| r.component.as_str())
        .collect();

    let mod_header_count = kinds
        .iter()
        .filter(|c| **c == "shell.modifier-header")
        .count();
    assert_eq!(mod_header_count, 2, "one header per attached modifier");

    let footer_count = kinds
        .iter()
        .filter(|c| **c == "shell.add-modifier-button")
        .count();
    assert_eq!(footer_count, 1, "exactly one add-modifier footer");

    // Footer comes last.
    assert_eq!(
        kinds.last().copied(),
        Some("shell.add-modifier-button"),
        "footer must be the final row"
    );

    // First modifier header carries Tooltip's label.
    let first_mod_header = state
        .builder
        .property_rows
        .iter()
        .find(|r| r.component == "shell.modifier-header")
        .expect("at least one modifier header");
    assert_eq!(
        first_mod_header.props.get("label").and_then(|v| v.as_str()),
        Some("Tooltip"),
        "first modifier header label",
    );
    assert_eq!(
        first_mod_header
            .props
            .get("modifier-id")
            .and_then(|v| v.as_str()),
        Some("tooltip"),
    );
    assert_eq!(
        first_mod_header
            .props
            .get("modifier-idx")
            .and_then(|v| v.as_u64()),
        Some(0),
    );
}

/// Wave 1.3 — disabled modifiers emit the header but suppress
/// their schema rows (so the panel stays compact for off
/// behaviours).
#[test]
fn disabled_modifier_emits_header_but_no_schema_rows() {
    use prism_builder::{Modifier, ModifierKind};
    let mut reg = prism_builder::ComponentRegistry::new();
    prism_builder::starter::register_builtins(&mut reg).expect("builtins");
    let mod_reg = std::sync::Arc::new(prism_builder::ModifierRegistry::with_builtins());

    let mut doc = doc_with_three_nodes();
    let btn = doc.root.as_mut().unwrap().find_mut("btn").unwrap();
    btn.modifiers
        .push(Modifier::from_kind(ModifierKind::Tooltip).disabled());

    let mut state = AppState::default();
    state.canvas.document = doc;
    state.canvas.selection = Some("btn".into());
    state.modifier_registry = Some(std::sync::Arc::clone(&mod_reg));
    state.resync_builder_for_selection(Some(&reg));

    let kinds: Vec<&str> = state
        .builder
        .property_rows
        .iter()
        .map(|r| r.component.as_str())
        .collect();
    let header_count = kinds
        .iter()
        .filter(|c| **c == "shell.modifier-header")
        .count();
    assert_eq!(header_count, 1, "one header for the disabled modifier");

    // Two text rows (Tooltip schema is `text` + `placement`)
    // must NOT appear since the modifier is disabled.
    let modifier_field_rows: Vec<_> = state
        .builder
        .property_rows
        .iter()
        .filter(|r| r.component == "shell.field-editor")
        .filter(|r| r.props.get("modifier-idx").is_some())
        .collect();
    assert!(
        modifier_field_rows.is_empty(),
        "disabled modifier must not emit schema rows; got {modifier_field_rows:?}",
    );
}

/// Wave 1.5 — attach + detach + toggle mutators round-trip
/// through the doc + property rows.
#[test]
fn attach_modifier_appends_section_and_resyncs() {
    use prism_builder::ModifierKind;
    let mut reg = prism_builder::ComponentRegistry::new();
    prism_builder::starter::register_builtins(&mut reg).expect("builtins");
    let mod_reg = std::sync::Arc::new(prism_builder::ModifierRegistry::with_builtins());

    let mut state = AppState::default();
    state.canvas.document = doc_with_three_nodes();
    state.canvas.selection = Some("btn".into());
    state.modifier_registry = Some(std::sync::Arc::clone(&mod_reg));

    assert!(state.attach_modifier("btn", ModifierKind::Tooltip.id(), Some(&reg)));
    // Modifier landed on the doc node.
    let btn = state
        .canvas
        .document
        .root
        .as_ref()
        .unwrap()
        .find("btn")
        .unwrap();
    assert_eq!(btn.modifiers.len(), 1);
    assert_eq!(btn.modifiers[0].kind, "tooltip");
    // Property rows resync'd — there's now a modifier-header.
    let headers: Vec<_> = state
        .builder
        .property_rows
        .iter()
        .filter(|r| r.component == "shell.modifier-header")
        .collect();
    assert_eq!(headers.len(), 1);

    // Second attempt to attach the same id is a no-op (one
    // modifier of each id per node).
    assert!(!state.attach_modifier("btn", ModifierKind::Tooltip.id(), Some(&reg)));
}

#[test]
fn toggle_modifier_flips_enabled_and_resyncs() {
    use prism_builder::{Modifier, ModifierKind};
    let mod_reg = std::sync::Arc::new(prism_builder::ModifierRegistry::with_builtins());
    let mut state = AppState::default();
    state.canvas.document = doc_with_three_nodes();
    state.canvas.selection = Some("btn".into());
    state.modifier_registry = Some(std::sync::Arc::clone(&mod_reg));
    let btn = state
        .canvas
        .document
        .root
        .as_mut()
        .unwrap()
        .find_mut("btn")
        .unwrap();
    btn.modifiers
        .push(Modifier::from_kind(ModifierKind::Tooltip));

    assert!(state.toggle_modifier("btn", 0, None));
    let btn = state
        .canvas
        .document
        .root
        .as_ref()
        .unwrap()
        .find("btn")
        .unwrap();
    assert!(!btn.modifiers[0].enabled);
    assert!(state.toggle_modifier("btn", 0, None));
    let btn = state
        .canvas
        .document
        .root
        .as_ref()
        .unwrap()
        .find("btn")
        .unwrap();
    assert!(btn.modifiers[0].enabled);

    // Out-of-range idx is a no-op.
    assert!(!state.toggle_modifier("btn", 99, None));
}

#[test]
fn detach_modifier_drops_section_and_resyncs() {
    use prism_builder::{Modifier, ModifierKind};
    let mut reg = prism_builder::ComponentRegistry::new();
    prism_builder::starter::register_builtins(&mut reg).expect("builtins");
    let mod_reg = std::sync::Arc::new(prism_builder::ModifierRegistry::with_builtins());

    let mut state = AppState::default();
    state.canvas.document = doc_with_three_nodes();
    state.canvas.selection = Some("btn".into());
    state.modifier_registry = Some(std::sync::Arc::clone(&mod_reg));
    let btn = state
        .canvas
        .document
        .root
        .as_mut()
        .unwrap()
        .find_mut("btn")
        .unwrap();
    btn.modifiers
        .push(Modifier::from_kind(ModifierKind::Tooltip));
    btn.modifiers
        .push(Modifier::from_kind(ModifierKind::HoverEffect));

    // Detach idx 0 → Tooltip removed; HoverEffect now at idx 0.
    assert!(state.detach_modifier("btn", 0, Some(&reg)));
    let btn = state
        .canvas
        .document
        .root
        .as_ref()
        .unwrap()
        .find("btn")
        .unwrap();
    assert_eq!(btn.modifiers.len(), 1);
    assert_eq!(btn.modifiers[0].kind, "hover-effect");

    // Out-of-range detach is a no-op.
    assert!(!state.detach_modifier("btn", 99, Some(&reg)));
}

#[test]
fn reorder_modifier_swaps_indices() {
    use prism_builder::{Modifier, ModifierKind};
    let mut state = AppState::default();
    state.canvas.document = doc_with_three_nodes();
    let btn = state
        .canvas
        .document
        .root
        .as_mut()
        .unwrap()
        .find_mut("btn")
        .unwrap();
    btn.modifiers
        .push(Modifier::from_kind(ModifierKind::Tooltip));
    btn.modifiers
        .push(Modifier::from_kind(ModifierKind::HoverEffect));
    btn.modifiers
        .push(Modifier::from_kind(ModifierKind::EnterAnimation));

    assert!(state.reorder_modifier("btn", 0, 2, None));
    let btn = state
        .canvas
        .document
        .root
        .as_ref()
        .unwrap()
        .find("btn")
        .unwrap();
    // After reorder: [hover-effect, enter-animation, tooltip]
    let ids: Vec<&str> = btn.modifiers.iter().map(|m| m.kind.as_str()).collect();
    assert_eq!(ids, vec!["hover-effect", "enter-animation", "tooltip"]);

    // No-op identical indices.
    assert!(!state.reorder_modifier("btn", 1, 1, None));
    // Out-of-range fails cleanly.
    assert!(!state.reorder_modifier("btn", 0, 99, None));
}

#[test]
fn set_modifier_prop_writes_to_modifier_props_not_node_props() {
    use prism_builder::{Modifier, ModifierKind};
    let mut state = AppState::default();
    state.canvas.document = doc_with_three_nodes();
    let btn = state
        .canvas
        .document
        .root
        .as_mut()
        .unwrap()
        .find_mut("btn")
        .unwrap();
    btn.modifiers
        .push(Modifier::from_kind(ModifierKind::Tooltip));

    assert!(state.set_modifier_prop("btn", 0, "text", json!("Save changes"), None,));
    let btn = state
        .canvas
        .document
        .root
        .as_ref()
        .unwrap()
        .find("btn")
        .unwrap();
    assert_eq!(
        btn.modifiers[0].props.get("text").and_then(|v| v.as_str()),
        Some("Save changes")
    );
    // Owning node's `props` is untouched.
    assert!(btn.props.get("text").is_none());
}

/// Wave 1.3 — without a modifier registry installed (tests /
/// headless), the property-row shape collapses to the pre-Wave-1
/// flat-list form (no modifier headers, no footer).
#[test]
fn no_modifier_registry_means_no_modifier_rows() {
    use prism_builder::{Modifier, ModifierKind};
    let mut reg = prism_builder::ComponentRegistry::new();
    prism_builder::starter::register_builtins(&mut reg).expect("builtins");

    let mut doc = doc_with_three_nodes();
    let btn = doc.root.as_mut().unwrap().find_mut("btn").unwrap();
    btn.modifiers
        .push(Modifier::from_kind(ModifierKind::Tooltip));

    let mut state = AppState::default();
    state.canvas.document = doc;
    state.canvas.selection = Some("btn".into());
    // Note: modifier_registry left as None.
    state.resync_builder_for_selection(Some(&reg));

    let kinds: Vec<&str> = state
        .builder
        .property_rows
        .iter()
        .map(|r| r.component.as_str())
        .collect();
    assert!(
        !kinds.contains(&"shell.modifier-header"),
        "no modifier headers without registry; got {kinds:?}",
    );
    assert!(
        !kinds.contains(&"shell.add-modifier-button"),
        "no add-modifier footer without registry; got {kinds:?}",
    );
}

#[test]
fn select_node_moves_selection_and_resyncs_inspector() {
    // §43 C3: programmatic `select_node` mutates `canvas.selection`
    // and re-derives the inspector tree so the new row carries
    // the `selected` flag.
    let mut state = AppState::default();
    state.canvas.document = doc_with_three_nodes();
    state.canvas.selection = Some("heading".into());
    state.resync_builder_for_selection(None);
    assert!(state.select_node("btn", None));
    assert_eq!(state.canvas.selection.as_deref(), Some("btn"));
    let row = state
        .builder
        .inspector
        .iter()
        .find(|n| n.id == "btn")
        .expect("btn row");
    assert!(row.selected);
}

#[test]
fn select_node_rejects_unknown_ids() {
    // §43 C3: stale ids surfaced by the hit-test surface (e.g. a
    // doc edit between layout and click) don't move selection.
    let mut state = AppState::default();
    state.canvas.document = doc_with_three_nodes();
    state.canvas.selection = Some("heading".into());
    assert!(!state.select_node("missing", None));
    assert_eq!(state.canvas.selection.as_deref(), Some("heading"));
}

#[test]
fn select_node_returns_false_when_selection_already_matches() {
    let mut state = AppState::default();
    state.canvas.document = doc_with_three_nodes();
    state.canvas.selection = Some("heading".into());
    assert!(!state.select_node("heading", None));
}

#[test]
fn set_node_prop_mutates_props_and_resyncs() {
    // §43 C2: `set_node_prop` writes one key on the target doc
    // node and re-derives the property rows so the form reflects
    // the new value on the next frame.
    let mut reg = prism_builder::ComponentRegistry::new();
    prism_builder::starter::register_builtins(&mut reg).expect("builtins");
    let mut state = AppState::default();
    state.canvas.document = doc_with_three_nodes();
    state.canvas.selection = Some("heading".into());
    state.resync_builder_for_selection(Some(&reg));

    assert!(state.set_node_prop("heading", "body", json!("Updated body"), Some(&reg)));
    let body = state
        .canvas
        .document
        .root
        .as_ref()
        .unwrap()
        .find("heading")
        .unwrap()
        .props
        .get("body")
        .cloned()
        .unwrap();
    assert_eq!(body, json!("Updated body"));
    // Idempotent edits return false — the derivation pass doesn't
    // need to rerun when the value didn't change.
    assert!(!state.set_node_prop("heading", "body", json!("Updated body"), Some(&reg)));
}

// ── Wave 3.2 palette drag → drop unit tests ─────────────────────

#[test]
fn begin_palette_drag_requires_armed_palette_pill() {
    // Wave 3.2: without `palette_selected` no drag opens. The
    // route in events.rs only calls `begin_palette_drag` after
    // confirming the hit is canvas-resident, but the mutator
    // still guards against a stale call.
    let mut state = AppState::default();
    assert!(!state.begin_palette_drag(0.0, 0.0, None));
    assert!(state.catalog.palette_drag.is_none());
}

#[test]
fn begin_palette_drag_records_kind_pointer_and_target() {
    let mut state = AppState::default();
    state.catalog.palette_selected = Some("button".into());
    assert!(state.begin_palette_drag(12.0, 34.0, Some("root")));
    let drag = state.catalog.palette_drag.as_ref().unwrap();
    assert_eq!(drag.kind, "button");
    assert_eq!(drag.pointer, (12.0, 34.0));
    assert_eq!(drag.drop_target.as_deref(), Some("root"));
}

#[test]
fn update_palette_drag_advances_pointer_and_target() {
    let mut state = AppState::default();
    state.catalog.palette_selected = Some("text".into());
    state.begin_palette_drag(0.0, 0.0, None);
    assert!(state.update_palette_drag(40.0, 60.0, Some("root")));
    let drag = state.catalog.palette_drag.as_ref().unwrap();
    assert_eq!(drag.pointer, (40.0, 60.0));
    assert_eq!(drag.drop_target.as_deref(), Some("root"));
}

#[test]
fn end_palette_drag_inserts_under_target_and_clears_palette_state() {
    let mut reg = prism_builder::ComponentRegistry::new();
    prism_builder::starter::register_builtins(&mut reg).expect("builtins");
    let mut state = AppState::default();
    state.canvas.document = doc_with_three_nodes();
    state.catalog.palette_selected = Some("text".into());
    state.begin_palette_drag(0.0, 0.0, Some("root"));
    let new_id = state
        .end_palette_drag(Some(&reg))
        .expect("drop inserts a node");
    // The new node lands as a child of `root` (its declared target).
    let root = state.canvas.document.root.as_ref().unwrap();
    assert!(
        root.children.iter().any(|c| c.id == new_id),
        "new node under root",
    );
    assert!(state.catalog.palette_drag.is_none(), "drag consumed");
    assert!(state.catalog.palette_selected.is_none(), "palette cleared");
    assert_eq!(state.canvas.selection.as_deref(), Some(new_id.as_str()));
}

#[test]
fn end_palette_drag_with_no_target_falls_back_to_root() {
    let mut state = AppState::default();
    state.canvas.document = doc_with_three_nodes();
    state.catalog.palette_selected = Some("text".into());
    state.begin_palette_drag(0.0, 0.0, None);
    let new_id = state.end_palette_drag(None).expect("drop succeeds");
    let root = state.canvas.document.root.as_ref().unwrap();
    assert!(root.children.iter().any(|c| c.id == new_id));
}

#[test]
fn cancel_palette_drag_drops_session_without_inserting() {
    let mut state = AppState::default();
    state.canvas.document = doc_with_three_nodes();
    let before = state.canvas.node_count();
    state.catalog.palette_selected = Some("text".into());
    state.begin_palette_drag(0.0, 0.0, Some("root"));
    assert!(state.cancel_palette_drag());
    assert!(state.catalog.palette_drag.is_none());
    // Cancel does NOT clear `palette_selected` — re-arming the
    // same pill across Esc is the desired UX (user picked the
    // tool intentionally; Esc only cancels the current gesture).
    assert_eq!(
        state.catalog.palette_selected.as_deref(),
        Some("text"),
        "cancel preserves the armed pill"
    );
    assert_eq!(
        state.canvas.node_count(),
        before,
        "cancelling never mutates the doc"
    );
}

// ── Wave 3.4 context menu unit tests ────────────────────────────

#[test]
fn open_context_menu_on_selected_canvas_node_carries_node_actions() {
    let mut reg = prism_builder::ComponentRegistry::new();
    prism_builder::starter::register_builtins(&mut reg).expect("builtins");
    let mut state = AppState::default();
    state.canvas.document = doc_with_three_nodes();
    // Right-clicking moves selection to the target id before the
    // items are derived; mirror that here.
    assert!(state.open_context_menu(10.0, 20.0, Some("btn"), Some(&reg)));
    assert_eq!(state.canvas.selection.as_deref(), Some("btn"));
    let labels: Vec<&str> = state
        .menus
        .context
        .iter()
        .filter(|m| !m.separator)
        .map(|m| m.label.as_str())
        .collect();
    assert!(labels.contains(&"Move Up"));
    assert!(labels.contains(&"Move Down"));
    assert!(labels.contains(&"Delete"));
    assert!(labels.contains(&"Copy"));
    assert!(labels.contains(&"Duplicate"));
}

#[test]
fn open_context_menu_on_empty_canvas_falls_back_to_paste() {
    let mut state = AppState::default();
    // No selection → paste-only menu.
    assert!(state.canvas.selection.is_none());
    assert!(state.open_context_menu(0.0, 0.0, None, None));
    let labels: Vec<&str> = state
        .menus
        .context
        .iter()
        .filter(|m| !m.separator)
        .map(|m| m.label.as_str())
        .collect();
    assert_eq!(labels, vec!["Paste"]);
}

#[test]
fn close_context_menu_clears_open_menu() {
    let mut state = AppState::default();
    state.menus.context.push(MenuItem {
        label: "x".into(),
        shortcut: None,
        command: None,
        separator: false,
        enabled: true,
    });
    assert!(state.close_context_menu());
    assert!(state.menus.context.is_empty());
}

#[test]
fn close_context_menu_is_idempotent_against_empty_menu() {
    let mut state = AppState::default();
    // Closing an already-closed menu is a clean no-op so an
    // every-frame dismiss route stays quiet.
    assert!(!state.close_context_menu());
}

// ── Wave 4 connection picker + mutator tests ────────────────────

#[test]
fn open_color_picker_seeds_target_key_value() {
    // Wave 2.4 — opening seeds the (target_id, key, value) triple
    // so subsequent preset clicks can route through
    // `set_color_picker_value` without re-reading the swatch's
    // routing attrs.
    let mut state = AppState::default();
    assert!(state.open_color_picker("demo-heading", "color", "#ff0000"));
    assert!(state.overlay.color_picker.open);
    assert_eq!(state.overlay.color_picker.target_id, "demo-heading");
    assert_eq!(state.overlay.color_picker.key, "color");
    assert_eq!(state.overlay.color_picker.value, "#ff0000");
}

#[test]
fn open_color_picker_against_same_target_is_a_noop() {
    let mut state = AppState::default();
    state.open_color_picker("demo-heading", "color", "#ff0000");
    // Re-opening with the same target leaves the picker as-is;
    // the user's in-progress preview survives a spurious second
    // click on the swatch.
    state.overlay.color_picker.value = "#00ff00".into();
    assert!(!state.open_color_picker("demo-heading", "color", "#ff0000"));
    assert_eq!(state.overlay.color_picker.value, "#00ff00");
}

#[test]
fn open_color_picker_against_different_target_reseeds() {
    let mut state = AppState::default();
    state.open_color_picker("a", "fg", "#fff");
    // A *different* swatch reseeds — the picker tracks one
    // anchor at a time.
    assert!(state.open_color_picker("b", "bg", "#000"));
    assert_eq!(state.overlay.color_picker.target_id, "b");
    assert_eq!(state.overlay.color_picker.key, "bg");
}

#[test]
fn close_color_picker_returns_true_only_when_open() {
    let mut state = AppState::default();
    assert!(!state.close_color_picker());
    state.open_color_picker("x", "k", "#fff");
    assert!(state.close_color_picker());
    assert!(!state.overlay.color_picker.open);
    assert!(state.overlay.color_picker.target_id.is_empty());
}

#[test]
fn color_picker_props_emit_eight_presets_open_or_closed() {
    // Wave 2.4 — `color_picker_props` always emits the full preset
    // list (so the closed overlay's tree shape stays stable
    // through layout); the `open` field drives the hidden / shown
    // branch on the DSL side.
    let mut state = AppState::default();
    let closed = state.overlay.color_picker_props();
    assert_eq!(closed["open"], Value::Bool(false));
    let presets = closed["presets"].as_array().expect("presets array");
    assert_eq!(presets.len(), ColorPicker::PRESETS.len());
    state.open_color_picker("x", "k", "#0060c0");
    let open = state.overlay.color_picker_props();
    assert_eq!(open["open"], Value::Bool(true));
    // One preset matches the current value → `selected=true`.
    let presets = open["presets"].as_array().expect("presets array");
    let selected_count = presets
        .iter()
        .filter(|p| p["selected"].as_bool().unwrap_or(false))
        .count();
    assert_eq!(selected_count, 1);
}

#[test]
fn open_connection_picker_seeds_form_defaults() {
    let mut state = AppState::default();
    assert!(state.open_connection_picker());
    assert!(state.overlay.connection_picker.open);
    // Defaults populate the source + kind so the user starts on
    // a usable form; target is empty until the user picks a
    // node.
    assert_eq!(state.overlay.connection_picker.source_signal, "clicked");
    assert_eq!(state.overlay.connection_picker.action_kind, "SetProperty");
    assert!(state.overlay.connection_picker.target_label.is_empty());
}

#[test]
fn open_connection_picker_is_idempotent_against_open_state() {
    let mut state = AppState::default();
    state.open_connection_picker();
    state.overlay.connection_picker.target_label = "x".into();
    // Re-opening keeps the user's in-progress entry intact.
    assert!(!state.open_connection_picker());
    assert_eq!(state.overlay.connection_picker.target_label, "x");
}

#[test]
fn close_connection_picker_clears_form_and_returns_true_when_open() {
    let mut state = AppState::default();
    state.open_connection_picker();
    state.overlay.connection_picker.target_label = "x".into();
    assert!(state.close_connection_picker());
    assert!(!state.overlay.connection_picker.open);
    assert!(state.overlay.connection_picker.target_label.is_empty());
    // Idempotent against already-closed state.
    assert!(!state.close_connection_picker());
}

#[test]
fn cycle_connection_picker_action_kind_wraps_at_end_of_variant_list() {
    let mut state = AppState::default();
    state.open_connection_picker();
    // Walk the entire variant list once + one wrap, asserting
    // every step yields the next declared variant.
    let kinds: Vec<&str> = ConnectionPicker::ACTION_KINDS.to_vec();
    let mut seen: Vec<String> = vec![state.overlay.connection_picker.action_kind.clone()];
    for _ in 0..kinds.len() {
        state.cycle_connection_picker_action_kind();
        seen.push(state.overlay.connection_picker.action_kind.clone());
    }
    // After N+1 cycles the value returned to the start.
    assert_eq!(seen.first(), seen.last(), "wraps to start: {seen:?}");
    // Every variant appeared at least once across the walk.
    for k in kinds {
        assert!(seen.iter().any(|s| s == k), "{k} appeared; got {seen:?}");
    }
}

#[test]
fn confirm_connection_picker_inserts_unique_id_and_moves_cursor() {
    let mut reg = prism_builder::ComponentRegistry::new();
    prism_builder::starter::register_builtins(&mut reg).expect("builtins");
    let mut state = AppState::default();
    state.canvas.document = doc_with_three_nodes();
    state.open_connection_picker();
    state.overlay.connection_picker.source_signal = "clicked".into();
    state.overlay.connection_picker.target_label = "demo-button".into();
    let id = state.confirm_connection_picker(Some(&reg));
    assert!(id.is_some(), "confirm returns the new id");
    assert!(!state.overlay.connection_picker.open, "picker closes");
    // Cursor lands on the new row.
    assert_eq!(state.builder.selected_connection, id);
    // A second confirm with the same fields generates a unique
    // id by appending a numeric suffix.
    state.open_connection_picker();
    state.overlay.connection_picker.source_signal = "clicked".into();
    state.overlay.connection_picker.target_label = "demo-button".into();
    let id2 = state
        .confirm_connection_picker(Some(&reg))
        .expect("second confirm");
    assert_ne!(id, Some(id2.clone()), "ids do not collide");
}

#[test]
fn confirm_connection_picker_with_empty_source_signal_is_a_noop() {
    let mut state = AppState::default();
    let before = state.builder.signal_connections.len();
    state.open_connection_picker();
    state.overlay.connection_picker.source_signal = String::new();
    assert!(state.confirm_connection_picker(None).is_none());
    assert_eq!(state.builder.signal_connections.len(), before);
    // Confirm still closes the picker (the user's intent was
    // clearly "I'm done"; clearing the form lets Esc behave
    // the same as Cancel).
    assert!(!state.overlay.connection_picker.open);
}

#[test]
fn update_signal_connection_field_writes_one_field_and_resyncs() {
    let mut state = AppState::default();
    state.builder.signal_connections.push(SignalConnection {
        id: "c1".into(),
        source_signal: "clicked".into(),
        action_kind: "SetProperty".into(),
        target_label: "x".into(),
    });
    assert!(state.update_signal_connection_field("c1", "target-label", "demo-button", None));
    assert_eq!(
        state.builder.signal_connections[0].target_label,
        "demo-button"
    );
    // Idempotent edits return false — re-writing the same value
    // skips the resync path.
    assert!(!state.update_signal_connection_field("c1", "target-label", "demo-button", None));
    // Unknown ids fall through cleanly.
    assert!(!state.update_signal_connection_field("missing", "target-label", "z", None));
    // Unknown field keys fall through cleanly.
    assert!(!state.update_signal_connection_field("c1", "made-up-key", "z", None));
}

#[test]
fn add_signal_connection_returns_id_and_lands_on_cursor() {
    let mut state = AppState::default();
    let id = state.add_signal_connection(
        SignalConnection {
            id: "c-foo".into(),
            source_signal: "clicked".into(),
            action_kind: "EmitSignal".into(),
            target_label: "y".into(),
        },
        None,
    );
    assert_eq!(id, "c-foo");
    assert_eq!(state.builder.selected_connection.as_deref(), Some("c-foo"));
    assert_eq!(state.builder.signal_connections.len(), 1);
}

// ── Wave 3.3 selection bbox + resize tests ──────────────────────

#[test]
fn builder_canvas_props_emit_selection_rect_when_bbox_set() {
    let mut state = AppState::default();
    state.canvas.selection_bbox = Some(SelectionBbox {
        x: 10.0,
        y: 20.0,
        width: 100.0,
        height: 50.0,
    });
    let props = state.canvas.builder_canvas_props();
    let rect = props.get("selection-rect").expect("emitted");
    assert_eq!(rect["x"], 10.0);
    assert_eq!(rect["y"], 20.0);
    assert_eq!(rect["width"], 100.0);
    assert_eq!(rect["height"], 50.0);
}

#[test]
fn builder_canvas_props_omit_selection_rect_when_no_bbox() {
    let state = AppState::default();
    let props = state.canvas.builder_canvas_props();
    assert!(props.get("selection-rect").is_none());
}

#[test]
fn palette_drag_overlay_active_shape() {
    // Wave 3.2 polish: `palette_drag_overlay` projects a
    // `PaletteDrag` into the JSON the canvas overlay's ghost
    // paint reads. The four keys (active, kind, x, y,
    // drop-target) are all required for the renderer-side
    // anchor + label to populate.
    let drag = PaletteDrag {
        kind: "button".into(),
        pointer: (40.0, 80.0),
        drop_target: Some("demo-heading".into()),
    };
    let v = CanvasSlot::palette_drag_overlay(Some(&drag));
    assert_eq!(v["active"], true);
    assert_eq!(v["kind"], "button");
    assert_eq!(v["x"], 40.0);
    assert_eq!(v["y"], 80.0);
    assert_eq!(v["drop-target"], "demo-heading");
}

#[test]
fn palette_drag_overlay_inactive_when_no_drag() {
    let v = CanvasSlot::palette_drag_overlay(None);
    assert_eq!(v["active"], false);
    assert!(v.get("kind").is_none());
}

#[test]
fn begin_resize_drag_requires_selection() {
    let mut state = AppState::default();
    assert!(!state.begin_resize_drag("br", 0.0, 0.0));
}

#[test]
fn begin_resize_drag_rejects_unknown_direction() {
    let mut state = AppState::default();
    state.canvas.document = doc_with_three_nodes();
    state.canvas.selection = Some("heading".into());
    assert!(!state.begin_resize_drag("???", 0.0, 0.0));
}

#[test]
fn resize_drag_round_trip_translates_position_by_handle_signs() {
    use prism_core::foundation::spatial::Transform2D;
    let mut state = AppState::default();
    state.canvas.document = doc_with_three_nodes();
    // Seed the heading at (100, 100) so the deltas are visible.
    let root = state.canvas.document.root.as_mut().unwrap();
    let heading = root.find_mut("heading").unwrap();
    heading.transform = Transform2D {
        position: [100.0, 100.0],
        ..Default::default()
    };
    state.canvas.selection = Some("heading".into());
    // Bottom-right handle: positive on both axes.
    assert!(state.begin_resize_drag("br", 0.0, 0.0));
    assert!(state.update_resize_drag(20.0, 30.0));
    let pos = state
        .canvas
        .document
        .root
        .as_ref()
        .unwrap()
        .find("heading")
        .unwrap()
        .transform
        .position;
    assert_eq!(pos, [120.0, 130.0]);
    assert!(state.end_resize_drag());
    assert!(state.canvas.resize_drag.is_none());
    // After commit, the mutation persists.
    let final_pos = state
        .canvas
        .document
        .root
        .as_ref()
        .unwrap()
        .find("heading")
        .unwrap()
        .transform
        .position;
    assert_eq!(final_pos, [120.0, 130.0]);
}

#[test]
fn resize_drag_top_left_handle_translates_negative_on_both_axes() {
    use prism_core::foundation::spatial::Transform2D;
    let mut state = AppState::default();
    state.canvas.document = doc_with_three_nodes();
    let root = state.canvas.document.root.as_mut().unwrap();
    let heading = root.find_mut("heading").unwrap();
    heading.transform = Transform2D {
        position: [200.0, 200.0],
        ..Default::default()
    };
    state.canvas.selection = Some("heading".into());
    assert!(state.begin_resize_drag("tl", 50.0, 50.0));
    // Drag towards (40, 40) — both deltas negative under tl
    // handle signs, so the node moves "up-left" by the deltas.
    assert!(state.update_resize_drag(40.0, 40.0));
    let pos = state
        .canvas
        .document
        .root
        .as_ref()
        .unwrap()
        .find("heading")
        .unwrap()
        .transform
        .position;
    // Snapshot (200, 200) + (-1 * (40-50), -1 * (40-50)) = (210, 210).
    assert_eq!(pos, [210.0, 210.0]);
}

#[test]
fn clear_selection_drops_property_rows_keeps_inspector_with_no_selected() {
    // §43 C1: Esc → no selection → properties empty, inspector
    // intact with all selected flags cleared.
    let mut reg = prism_builder::ComponentRegistry::new();
    prism_builder::starter::register_builtins(&mut reg).expect("builtins");
    let mut state = AppState::default();
    state.canvas.document = doc_with_three_nodes();
    state.canvas.selection = Some("heading".into());
    state.resync_builder_for_selection(Some(&reg));
    assert!(!state.builder.property_rows.is_empty());

    state.clear_selection();
    assert!(state.canvas.selection.is_none());
    assert!(state.builder.property_rows.is_empty());
    assert_eq!(state.builder.inspector.len(), 3, "inspector still there");
    assert!(state.builder.inspector.iter().all(|n| !n.selected));
}

#[test]
fn default_chrome_emits_app_name_and_status() {
    let state = AppState::default();
    let props = state.chrome.app_window_props(&state.workspace);
    assert_eq!(props["app-name"], "Prism");
    assert_eq!(props["status"], "Ready");
    assert!(props["menus"].is_array());
    assert!(props["nav-buttons"].is_array());
}

#[test]
fn status_bar_props_carries_status_and_segments() {
    let mut state = AppState::default();
    state.chrome.status = "Saving…".into();
    let props = state
        .chrome
        .status_bar_props(&state.workspace, &state.canvas);
    // Back-compat: the original `status` key is still emitted so
    // headless / legacy consumers keep working.
    assert_eq!(props["status"], "Saving…");
    // §43 D5: the new `segments` array is the multi-segment payload
    // — `status / active-page / selection / node-count / app-name`.
    let segments = props["segments"].as_array().expect("segments array");
    assert_eq!(segments.len(), 5);
    assert_eq!(segments[0], "Saving…");
    assert_eq!(
        segments[1],
        state.workspace.workspace.active_page().label.as_str()
    );
    assert_eq!(segments[2], "No selection");
    assert_eq!(segments[3], "0 nodes");
    assert_eq!(segments[4], state.chrome.app_name.as_str());
}

#[test]
fn status_bar_segments_track_selection_and_node_count() {
    use prism_builder::{BuilderDocument, Node};
    let mut state = AppState::default();
    state.canvas.document = BuilderDocument {
        root: Some(Node {
            id: "root".into(),
            component: "container".into(),
            children: vec![Node {
                id: "child".into(),
                component: "text".into(),
                props: json!({ "body": "Hello world" }),
                ..Default::default()
            }],
            ..Default::default()
        }),
        ..Default::default()
    };
    state.canvas.selection = Some("child".into());
    let props = state
        .chrome
        .status_bar_props(&state.workspace, &state.canvas);
    let segments = props["segments"].as_array().unwrap();
    // Selection label uses the inspector heuristic — prefers the
    // `body`/`label`/`title` prop when present.
    assert_eq!(segments[2], "Hello world");
    // 2 nodes — root + child.
    assert_eq!(segments[3], "2 nodes");
}

#[test]
fn workflow_page_bar_marks_exactly_one_active() {
    let state = AppState::default();
    let props = state.workspace.workflow_page_bar_props();
    let pages = props["pages"].as_array().expect("pages array");
    assert_eq!(pages.len(), state.workspace.workspace.pages().len());
    let active_count = pages.iter().filter(|p| p["active"] == true).count();
    assert_eq!(active_count, 1);
    assert_eq!(pages[0]["active"], true);
}

#[test]
fn switching_page_moves_active_flag() {
    let mut state = AppState::default();
    let target = state.workspace.workspace.pages()[2].id.clone();
    state.workspace.workspace.switch_page_by_id(&target);
    let props = state.workspace.workflow_page_bar_props();
    let pages = props["pages"].as_array().unwrap();
    assert_eq!(pages[2]["active"], true);
    assert_eq!(pages[0]["active"], false);
}

#[test]
fn menu_bar_row_pulls_tabs_from_workspace() {
    let state = AppState::default();
    let props = state.chrome.menu_bar_row_props(&state.workspace);
    assert_eq!(props["app-name"], "Prism");
    let tabs = props["tabs"].as_array().expect("tabs array");
    assert_eq!(tabs.len(), state.workspace.workspace.pages().len());
    // First page is active in the default workspace.
    assert_eq!(tabs[0]["active"], true);
}

#[test]
fn app_window_composes_chrome_with_workspace_tabs() {
    let mut state = AppState::default();
    let target = state.workspace.workspace.pages()[1].id.clone();
    state.workspace.workspace.switch_page_by_id(&target);
    let props = state.chrome.app_window_props(&state.workspace);
    let tabs = props["tabs"].as_array().unwrap();
    assert_eq!(tabs[1]["active"], true, "tabs reflect active page");
    assert_eq!(props["app-name"], "Prism", "chrome data still flows");
}

// ── overlay ───────────────────────────────────────────────────

#[test]
fn toast_stack_props_serialise_kind_as_string() {
    let mut overlay = OverlaySlot::default();
    overlay.toasts.push(Toast {
        title: "Saved".into(),
        body: "Project flushed".into(),
        kind: ToastKind::Success,
    });
    let props = overlay.toast_stack_props();
    let toasts = props["toasts"].as_array().unwrap();
    assert_eq!(toasts.len(), 1);
    assert_eq!(toasts[0]["kind"], "success");
    assert_eq!(toasts[0]["title"], "Saved");
}

#[test]
fn command_palette_props_default_is_closed_with_empty_query() {
    let props = OverlaySlot::default().command_palette_props();
    assert_eq!(props["open"], false);
    assert_eq!(props["query"], "");
    assert_eq!(props["results"].as_array().unwrap().len(), 0);
}

#[test]
fn help_tooltip_props_collapses_to_invisible_when_none() {
    let props = OverlaySlot::default().help_tooltip_props();
    assert_eq!(props["visible"], false);
    assert_eq!(props["title"], "");
}

#[test]
fn help_tooltip_props_emits_visible_when_present() {
    let overlay = OverlaySlot {
        help_tooltip: Some(HelpTooltip {
            title: "Save".into(),
            summary: "Persist project".into(),
        }),
        ..Default::default()
    };
    let props = overlay.help_tooltip_props();
    assert_eq!(props["visible"], true);
    assert_eq!(props["title"], "Save");
}

// ── cursor helpers ───────────────────────────────────────────

#[derive(Debug, PartialEq, Eq)]
struct CursorRow {
    id: &'static str,
}

impl CursorKey for CursorRow {
    fn cursor_key(&self) -> &str {
        self.id
    }
}

#[test]
fn select_cursor_row_moves_cursor_idempotent_against_self_and_unknown() {
    let items = vec![CursorRow { id: "a" }, CursorRow { id: "b" }];
    let mut cursor = None;
    assert!(select_cursor_row(&items, &mut cursor, "b"));
    assert_eq!(cursor.as_deref(), Some("b"));
    // Idempotent.
    assert!(!select_cursor_row(&items, &mut cursor, "b"));
    // Unknown ids leave the cursor alone.
    assert!(!select_cursor_row(&items, &mut cursor, "ghost"));
    assert_eq!(cursor.as_deref(), Some("b"));
}

#[test]
fn delete_cursor_row_drops_cursored_row_and_clears_cursor() {
    let mut items = vec![CursorRow { id: "a" }, CursorRow { id: "b" }];
    let mut cursor = Some("a".to_string());
    assert!(delete_cursor_row(&mut items, &mut cursor));
    assert_eq!(items, vec![CursorRow { id: "b" }]);
    assert!(cursor.is_none());
    // Empty cursor → no-op.
    assert!(!delete_cursor_row(&mut items, &mut cursor));
    // Stale cursor → no-op + cleared.
    cursor = Some("ghost".into());
    assert!(!delete_cursor_row(&mut items, &mut cursor));
    assert!(cursor.is_none());
}

#[test]
fn pop_cursor_row_returns_original_index_for_post_remove_bookkeeping() {
    let mut items = vec![
        CursorRow { id: "a" },
        CursorRow { id: "b" },
        CursorRow { id: "c" },
    ];
    let mut cursor = Some("b".to_string());
    assert_eq!(pop_cursor_row(&mut items, &mut cursor), Some(1));
    assert_eq!(items, vec![CursorRow { id: "a" }, CursorRow { id: "c" }]);
}

#[test]
fn iter_with_cursor_pairs_each_row_with_is_selected_flag() {
    let items = vec![
        CursorRow { id: "a" },
        CursorRow { id: "b" },
        CursorRow { id: "c" },
    ];
    let flags: Vec<bool> = iter_with_cursor(&items, Some("b"))
        .map(|(_, sel)| sel)
        .collect();
    assert_eq!(flags, vec![false, true, false]);
}

// ── builder ───────────────────────────────────────────────────

#[test]
fn properties_panel_props_round_trips_typed_rows() {
    let mut builder = BuilderSlot::default();
    builder.property_rows.push(PropertyRow {
        component: "shell.section-header".into(),
        props: json!({ "label": "Layout" }),
    });
    builder.property_rows.push(PropertyRow {
        component: "shell.field-editor".into(),
        props: json!({ "key": "x", "kind": "number", "value": 10 }),
    });
    let props = builder.properties_panel_props();
    let rows = props["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["component"], "shell.section-header");
    assert_eq!(rows[1]["props"]["value"], 10);
}

#[test]
fn properties_panel_props_with_focus_stamps_focused_on_matching_row() {
    let mut builder = BuilderSlot::default();
    builder.property_rows.push(PropertyRow {
        component: "shell.field-editor".into(),
        props: json!({
            "key": "body",
            "kind": "text",
            "value": "hi",
            "target-id": "demo-heading",
        }),
    });
    builder.property_rows.push(PropertyRow {
        component: "shell.field-editor".into(),
        props: json!({
            "key": "level",
            "kind": "select",
            "value": "h1",
            "target-id": "demo-heading",
        }),
    });
    let focus = FieldFocus {
        target_id: "demo-heading".into(),
        key: "body".into(),
        kind: "text".into(),
        original: "hi".into(),
        editor: prism_ui_runtime::editor::TextEditor::with_text("hi"),
    };
    let props = builder.properties_panel_props_with(Some(&focus));
    let rows = props["rows"].as_array().unwrap();
    assert_eq!(rows[0]["props"]["focused"], true);
    // Sibling rows stay unfocused — no stray flag.
    assert_eq!(rows[1]["props"].get("focused"), None);
}

#[test]
fn signals_panel_props_emits_connection_array() {
    let mut builder = BuilderSlot::default();
    builder.signal_connections.push(SignalConnection {
        id: "c1".into(),
        source_signal: "clicked".into(),
        action_kind: "SetProperty".into(),
        target_label: "x".into(),
    });
    let props = builder.signals_panel_props();
    assert_eq!(props["title"], "Signals");
    let conns = props["connections"].as_array().unwrap();
    assert_eq!(conns[0]["connection-id"], "c1");
    assert_eq!(conns[0]["source-signal"], "clicked");
    assert_eq!(conns[0]["action-kind"], "SetProperty");
    assert_eq!(conns[0]["selected"], false);
    assert_eq!(conns[0]["show-delete"], false);
}

#[test]
fn select_signal_connection_moves_cursor() {
    let mut builder = BuilderSlot::default();
    builder.signal_connections.push(SignalConnection {
        id: "c1".into(),
        source_signal: "clicked".into(),
        action_kind: "EmitSignal".into(),
        target_label: "y".into(),
    });
    builder.signal_connections.push(SignalConnection {
        id: "c2".into(),
        source_signal: "hovered".into(),
        action_kind: "SetProperty".into(),
        target_label: "x".into(),
    });
    assert!(builder.select_signal_connection("c2"));
    assert_eq!(builder.selected_connection.as_deref(), Some("c2"));
    // Idempotent: re-selecting the same row returns false.
    assert!(!builder.select_signal_connection("c2"));
    // Unknown ids leave the cursor alone.
    assert!(!builder.select_signal_connection("ghost"));
}

#[test]
fn signals_panel_props_marks_cursor_row_selected_and_show_delete() {
    let mut builder = BuilderSlot::default();
    for id in ["c1", "c2"] {
        builder.signal_connections.push(SignalConnection {
            id: id.into(),
            source_signal: "clicked".into(),
            action_kind: "EmitSignal".into(),
            target_label: "x".into(),
        });
    }
    builder.select_signal_connection("c2");
    let props = builder.signals_panel_props();
    let conns = props["connections"].as_array().unwrap();
    assert_eq!(conns[0]["selected"], false);
    assert_eq!(conns[1]["selected"], true);
    assert_eq!(conns[1]["show-delete"], true);
}

#[test]
fn delete_selected_signal_connection_drops_cursor_row() {
    let mut builder = BuilderSlot::default();
    for id in ["c1", "c2"] {
        builder.signal_connections.push(SignalConnection {
            id: id.into(),
            source_signal: "clicked".into(),
            action_kind: "EmitSignal".into(),
            target_label: "x".into(),
        });
    }
    builder.select_signal_connection("c2");
    assert!(builder.delete_selected_signal_connection());
    let remaining: Vec<&str> = builder
        .signal_connections
        .iter()
        .map(|c| c.id.as_str())
        .collect();
    assert_eq!(remaining, vec!["c1"]);
    assert!(builder.selected_connection.is_none());
    // Idempotent: no cursor, nothing to delete.
    assert!(!builder.delete_selected_signal_connection());
}

#[test]
fn schema_designer_props_carries_fields() {
    let builder = BuilderSlot {
        schema: SchemaDoc {
            title: "Posts".into(),
            schema_name: "post".into(),
            fields: vec![SchemaField {
                name: "title".into(),
                kind: "text".into(),
                required: true,
            }],
            ..Default::default()
        },
        ..Default::default()
    };
    let props = builder.schema_designer_props();
    assert_eq!(props["title"], "Posts");
    assert_eq!(props["schema-name"], "post");
    let fields = props["fields"].as_array().unwrap();
    assert_eq!(fields[0]["field-name"], "title");
    assert_eq!(fields[0]["required"], true);
    assert_eq!(fields[0]["selected"], false);
    assert_eq!(fields[0]["show-delete"], false);
}

#[test]
fn select_schema_field_moves_cursor() {
    let mut builder = BuilderSlot {
        schema: SchemaDoc {
            fields: vec![
                SchemaField {
                    name: "title".into(),
                    kind: "text".into(),
                    required: true,
                },
                SchemaField {
                    name: "body".into(),
                    kind: "rich-text".into(),
                    required: false,
                },
            ],
            ..Default::default()
        },
        ..Default::default()
    };
    assert!(builder.select_schema_field("body"));
    assert_eq!(builder.schema.selected_field.as_deref(), Some("body"));
    assert!(!builder.select_schema_field("body"));
    assert!(!builder.select_schema_field("ghost"));
}

#[test]
fn schema_designer_props_marks_cursor_row_selected_and_show_delete() {
    let mut builder = BuilderSlot {
        schema: SchemaDoc {
            fields: vec![
                SchemaField {
                    name: "title".into(),
                    kind: "text".into(),
                    required: false,
                },
                SchemaField {
                    name: "body".into(),
                    kind: "rich-text".into(),
                    required: false,
                },
            ],
            ..Default::default()
        },
        ..Default::default()
    };
    builder.select_schema_field("body");
    let props = builder.schema_designer_props();
    let fields = props["fields"].as_array().unwrap();
    assert_eq!(fields[0]["selected"], false);
    assert_eq!(fields[1]["selected"], true);
    assert_eq!(fields[1]["show-delete"], true);
}

#[test]
fn delete_selected_schema_field_drops_cursor_row() {
    let mut builder = BuilderSlot {
        schema: SchemaDoc {
            fields: vec![
                SchemaField {
                    name: "title".into(),
                    kind: "text".into(),
                    required: false,
                },
                SchemaField {
                    name: "body".into(),
                    kind: "rich-text".into(),
                    required: false,
                },
            ],
            ..Default::default()
        },
        ..Default::default()
    };
    builder.select_schema_field("body");
    assert!(builder.delete_selected_schema_field());
    let remaining: Vec<&str> = builder
        .schema
        .fields
        .iter()
        .map(|f| f.name.as_str())
        .collect();
    assert_eq!(remaining, vec!["title"]);
    assert!(builder.schema.selected_field.is_none());
    assert!(!builder.delete_selected_schema_field());
}

#[test]
fn inspector_tree_props_carries_typed_nodes() {
    let mut builder = BuilderSlot::default();
    builder.inspector.push(InspectorNode {
        id: "n1".into(),
        label: "Root".into(),
        depth: 0,
        selected: true,
    });
    let props = builder.inspector_tree_props();
    let nodes = props["nodes"].as_array().unwrap();
    assert_eq!(nodes[0]["selected"], true);
    assert_eq!(nodes[0]["depth"], 0);
}

// ── navigation ────────────────────────────────────────────────

fn sample_nav() -> NavigationSlot {
    NavigationSlot {
        pages: vec![
            NavPage {
                id: "home".into(),
                title: "Home".into(),
                route: "/".into(),
                x: 0.0,
                y: 0.0,
                node_count: 4,
                link_count: 1,
                is_active: true,
            },
            NavPage {
                id: "about".into(),
                title: "About".into(),
                route: "/about".into(),
                x: 200.0,
                y: 0.0,
                node_count: 1,
                link_count: 0,
                is_active: false,
            },
        ],
        edges: vec![NavEdge {
            from: 0,
            to: 1,
            kind: NavEdgeKind::Href,
        }],
        ..Default::default()
    }
}

#[test]
fn nav_page_list_props_emits_list_only() {
    let nav = sample_nav();
    let props = nav.nav_page_list_props();
    let pages = props["pages"].as_array().unwrap();
    assert_eq!(pages.len(), 2);
    assert_eq!(pages[0]["page-title"], "Home");
    assert_eq!(pages[0]["node-count"], 4);
    assert!(props.get("edges").is_none(), "list does not leak edges");
}

#[test]
fn select_page_by_id_moves_active_flag_and_returns_true() {
    let mut nav = sample_nav();
    assert!(nav.pages[0].is_active);
    assert!(!nav.pages[1].is_active);
    assert!(nav.select_page_by_id("about"));
    assert!(!nav.pages[0].is_active);
    assert!(nav.pages[1].is_active);
}

#[test]
fn select_page_by_id_returns_false_when_already_active() {
    let mut nav = sample_nav();
    assert!(!nav.select_page_by_id("home"));
}

#[test]
fn select_page_by_id_returns_false_for_unknown_id() {
    let mut nav = sample_nav();
    assert!(!nav.select_page_by_id("nonexistent"));
    // Active flag stays put.
    assert!(nav.pages[0].is_active);
}

#[test]
fn select_row_moves_the_chevron_cursor_without_touching_active_flag() {
    let mut nav = sample_nav();
    assert!(nav.select_row("about"));
    assert_eq!(nav.selected_page.as_deref(), Some("about"));
    // Active page didn't move — cursor is disjoint from is_active.
    assert!(nav.pages[0].is_active);
    assert!(!nav.pages[1].is_active);
}

#[test]
fn pages_list_props_emits_selected_and_show_delete_for_cursor_row() {
    let mut nav = sample_nav();
    nav.select_row("about");
    let props = nav.nav_page_list_props();
    let pages = props["pages"].as_array().unwrap();
    assert_eq!(pages[0]["selected"], false);
    assert_eq!(pages[1]["selected"], true);
    assert_eq!(pages[1]["show-delete"], true);
}

#[test]
fn reorder_selected_swaps_cursor_page_with_neighbour() {
    let mut nav = sample_nav();
    nav.select_row("about");
    assert!(nav.reorder_selected(-1));
    assert_eq!(nav.pages[0].id, "about");
    assert_eq!(nav.pages[1].id, "home");
    // Cursor still points at "about" — it moved with the page.
    assert_eq!(nav.selected_page.as_deref(), Some("about"));
}

#[test]
fn reorder_selected_at_boundary_returns_false() {
    let mut nav = sample_nav();
    nav.select_row("home");
    assert!(!nav.reorder_selected(-1));
}

#[test]
fn delete_selected_removes_page_and_clears_cursor() {
    let mut nav = sample_nav();
    nav.select_row("about");
    assert!(nav.delete_selected());
    assert_eq!(nav.pages.len(), 1);
    assert!(nav.selected_page.is_none());
}

#[test]
fn delete_selected_active_page_promotes_a_survivor() {
    let mut nav = sample_nav();
    nav.select_row("home"); // Home is active.
    assert!(nav.delete_selected());
    // "about" inherits active status so the workspace stays
    // pointed at something.
    assert!(nav.pages[0].is_active);
    assert_eq!(nav.pages[0].id, "about");
}

#[test]
fn nav_graph_props_emits_pages_with_positions_and_edges() {
    let nav = sample_nav();
    let props = nav.nav_graph_props();
    assert_eq!(props["title"], "Pages");
    let pages = props["pages"].as_array().unwrap();
    assert_eq!(pages[0]["label"], "Home");
    assert_eq!(pages[1]["x"], 200.0);
    let edges = props["edges"].as_array().unwrap();
    assert_eq!(edges[0]["kind"], "href");
    assert_eq!(edges[0]["from"], 0);
}

// ── catalog ───────────────────────────────────────────────────

fn sample_catalog() -> CatalogSlot {
    CatalogSlot {
        launchpad_title: "Welcome".into(),
        apps: vec![AppCard {
            id: "lattice".into(),
            label: "Lattice".into(),
            icon: "icons/lattice.svg".into(),
            summary: "Visual web builder".into(),
        }],
        files: vec![
            FileNode {
                id: "src".into(),
                label: "src".into(),
                depth: 0,
                kind: FileKind::Directory,
                path: std::path::PathBuf::new(),
            },
            FileNode {
                id: "src/lib.rs".into(),
                label: "lib.rs".into(),
                depth: 1,
                kind: FileKind::File,
                path: std::path::PathBuf::new(),
            },
        ],
        palette: vec![PaletteItem {
            id: "heading".into(),
            label: "Heading".into(),
            icon: "icons/heading.svg".into(),
            category: "Text".into(),
        }],
        palette_selected: Some("heading".into()),
        palette_drag: None,
    }
}

#[test]
fn launchpad_props_carry_title_and_apps() {
    let cat = sample_catalog();
    let props = cat.launchpad_props();
    assert_eq!(props["title"], "Welcome");
    let apps = props["apps"].as_array().unwrap();
    assert_eq!(apps[0]["app-id"], "lattice");
    assert_eq!(apps[0]["summary"], "Visual web builder");
}

#[test]
fn explorer_props_emit_depth_and_kind() {
    let cat = sample_catalog();
    let props = cat.explorer_props();
    let nodes = props["nodes"].as_array().unwrap();
    assert_eq!(nodes[0]["kind"], "directory");
    assert_eq!(nodes[1]["kind"], "file");
    assert_eq!(nodes[1]["depth"], 1);
}

#[test]
fn component_palette_props_omit_selected_when_none() {
    let mut cat = sample_catalog();
    cat.palette_selected = None;
    let props = cat.component_palette_props();
    assert!(props.get("selected-id").is_none());
}

#[test]
fn component_palette_props_include_selected_when_some() {
    let cat = sample_catalog();
    let props = cat.component_palette_props();
    assert_eq!(props["selected-id"], "heading");
    assert_eq!(props["items"][0]["item-id"], "heading");
}

// ── docs ──────────────────────────────────────────────────────

fn sample_docs() -> DocsSlot {
    DocsSlot {
        topic: DocsTopic {
            title: "Builder".into(),
            summary: "Edit visually.".into(),
            body: "Long form…".into(),
        },
        sidebar_mode: String::new(),
    }
}

#[test]
fn docs_view_pins_mode_full() {
    let props = sample_docs().docs_view_props();
    assert_eq!(props["mode"], "full");
    assert_eq!(props["title"], "Builder");
}

#[test]
fn docs_sidebar_defaults_to_sidebar_mode() {
    let props = sample_docs().docs_sidebar_props();
    assert_eq!(props["mode"], "sidebar");
}

#[test]
fn docs_sidebar_respects_explicit_mode() {
    let mut docs = sample_docs();
    docs.sidebar_mode = "outline".into();
    let props = docs.docs_sidebar_props();
    assert_eq!(props["mode"], "outline");
}

#[test]
fn docs_view_and_sidebar_share_topic_shape() {
    // Rule-of-three confirmation: both bindings emit byte-identical
    // title/summary/body keys, so the private helper is the sole
    // source of the shared shape. Drift would show up here first.
    let docs = sample_docs();
    let view = docs.docs_view_props();
    let sidebar = docs.docs_sidebar_props();
    assert_eq!(view["title"], sidebar["title"]);
    assert_eq!(view["summary"], sidebar["summary"]);
    assert_eq!(view["body"], sidebar["body"]);
}

// ── menus ─────────────────────────────────────────────────────

fn sample_menus() -> MenuSlot {
    MenuSlot {
        dropdown: vec![
            MenuItem {
                label: "Save".into(),
                shortcut: Some("Ctrl+S".into()),
                command: Some("file.save".into()),
                separator: false,
                enabled: true,
            },
            MenuItem::separator(),
            MenuItem {
                label: "Quit".into(),
                shortcut: None,
                command: Some("app.quit".into()),
                separator: false,
                enabled: true,
            },
        ],
        context: vec![MenuItem {
            label: "Delete".into(),
            shortcut: Some("Del".into()),
            command: Some("edit.delete".into()),
            separator: false,
            enabled: true,
        }],
    }
}

#[test]
fn menu_dropdown_emits_items_with_shortcut_and_command() {
    let props = sample_menus().menu_dropdown_props();
    let items = props["items"].as_array().unwrap();
    assert_eq!(items.len(), 3);
    assert_eq!(items[0]["label"], "Save");
    assert_eq!(items[0]["shortcut"], "Ctrl+S");
    assert_eq!(items[0]["command"], "file.save");
    assert_eq!(items[1]["separator"], true);
    // shortcut/command are omitted when None
    assert!(items[2].get("shortcut").is_none());
}

#[test]
fn context_menu_uses_same_item_shape_as_dropdown() {
    // Rule-of-three confirmation: identical key set across both
    // emitters via the shared `items_json` helper. A drift would
    // require editing one site for both to keep parity, which is
    // exactly the duplication the helper prevents.
    let menus = sample_menus();
    let drop = menus.menu_dropdown_props();
    let ctx = menus.context_menu_props();
    let drop_keys: Vec<_> = drop["items"][0]
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect();
    let ctx_keys: Vec<_> = ctx["items"][0]
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect();
    assert_eq!(drop_keys, ctx_keys);
}

#[test]
fn menu_separator_helper_marks_separator_true_and_disabled() {
    let sep = MenuItem::separator();
    assert!(sep.separator);
    assert!(!sep.enabled);
    assert!(sep.command.is_none());
}

// ── canvas ────────────────────────────────────────────────────

fn sample_canvas() -> CanvasSlot {
    use prism_builder::Node;
    let root = Node {
        id: "root".into(),
        component: "container".into(),
        transform: Transform2D {
            position: [100.0, 80.0],
            ..Default::default()
        },
        ..Default::default()
    };
    CanvasSlot {
        document: BuilderDocument {
            root: Some(root),
            ..Default::default()
        },
        selection: Some("root".into()),
        tool: ToolMode::Move,
        viewport: CanvasViewport::default(),
        picker: PickerState {
            open: true,
            anchor_x: 50.0,
            anchor_y: 60.0,
            candidates: vec![PickerCandidate {
                id: "heading".into(),
                label: "Heading".into(),
                icon: "icons/heading.svg".into(),
            }],
        },
        code_buffer: {
            let mut editor = prism_ui_runtime::editor::TextEditor::new_multi_line();
            editor.set_text("<container/>");
            editor.place_caret_at(7, false);
            CodeBuffer {
                editor,
                language: "prui".into(),
                scroll_x: 0.0,
                scroll_y: 0.0,
                cached_spans: std::cell::RefCell::new(SpansCache::default()),
            }
        },
        code_buffer_meta: EditorTabMeta::untitled(),
        code_tabs: Vec::new(),
        code_active_tab: 0,
        device: Device::Desktop,
        drag: None,
        bindings: prism_builder::DocumentBindings::new(),
        selection_bbox: None,
        resize_drag: None,
    }
}

#[test]
fn code_editor_props_carry_source_caret_and_language() {
    let props = sample_canvas().code_editor_props();
    assert_eq!(props["source"], "<container/>");
    assert_eq!(props["caret"], 7);
    assert_eq!(props["language"], "prui");
}

#[test]
fn device_from_id_parses_known_ids_and_rejects_others() {
    assert_eq!(Device::from_id("desktop"), Some(Device::Desktop));
    assert_eq!(Device::from_id("tablet"), Some(Device::Tablet));
    assert_eq!(Device::from_id("mobile"), Some(Device::Mobile));
    assert_eq!(Device::from_id("phablet"), None);
    assert_eq!(Device::from_id(""), None);
}

#[test]
fn builder_canvas_props_emit_selection_and_viewport() {
    let props = sample_canvas().builder_canvas_props();
    assert_eq!(props["selection-id"], "root");
    assert_eq!(props["tool"], "move");
    assert_eq!(props["zoom"], 1.0);
    assert_eq!(props["place-mode"], true);
}

#[test]
fn gizmo_props_share_shape_across_three_modes() {
    // Rule-of-three parity: same key set on all three gizmo
    // emissions, only `tool` differs. Drift in any field would
    // break this assertion in one place, not three.
    let canvas = sample_canvas();
    let m = canvas.gizmo_move_props();
    let r = canvas.gizmo_rotate_props();
    let s = canvas.gizmo_scale_props();
    let keys = |v: &Value| -> Vec<String> { v.as_object().unwrap().keys().cloned().collect() };
    assert_eq!(keys(&m), keys(&r));
    assert_eq!(keys(&r), keys(&s));
    assert_eq!(m["tool"], "move");
    assert_eq!(r["tool"], "rotate");
    assert_eq!(s["tool"], "scale");
    // Visibility flows through the active tool — only the matching
    // gizmo paints on a given frame.
    assert_eq!(m["visible"], true);
    assert_eq!(r["visible"], false);
    assert_eq!(s["visible"], false);
}

#[test]
fn gizmo_and_resize_handle_share_selection_center() {
    // §22 cross-binding parity: flipping the selection's transform
    // shows up in *both* gizmo and resize-handle emissions through
    // the same `selection_center()` helper. The load-bearing
    // duplication check for the canvas slot.
    let mut canvas = sample_canvas();
    canvas.tool = ToolMode::Move;
    let g0 = canvas.gizmo_move_props();
    let h0 = canvas.resize_handle_props();
    let g0_x = g0["center-x"].as_f64().unwrap();
    // Top handle's `x` is the bbox mid-x, which is the center-x.
    let top = h0["handles"]
        .as_array()
        .unwrap()
        .iter()
        .find(|h| h["id"] == "t")
        .unwrap();
    let h0_x = top["x"].as_f64().unwrap();
    assert!((g0_x - h0_x).abs() < 0.01, "shared center on first frame");

    canvas
        .document
        .root
        .as_mut()
        .unwrap()
        .find_mut("root")
        .unwrap()
        .transform
        .position[0] = 250.0;
    let g1 = canvas.gizmo_move_props();
    let h1 = canvas.resize_handle_props();
    let top1 = h1["handles"]
        .as_array()
        .unwrap()
        .iter()
        .find(|h| h["id"] == "t")
        .unwrap();
    assert!(
        (g1["center-x"].as_f64().unwrap() - top1["x"].as_f64().unwrap()).abs() < 0.01,
        "shared center on second frame"
    );
    assert_eq!(g1["center-x"], 250.0);
}

#[test]
fn resize_handle_props_collapse_to_invisible_when_no_selection() {
    let mut canvas = sample_canvas();
    canvas.selection = None;
    let props = canvas.resize_handle_props();
    assert_eq!(props["visible"], false);
    assert_eq!(props["handles"].as_array().unwrap().len(), 0);
}

#[test]
fn component_picker_props_omit_candidates_when_closed_but_keep_shape() {
    let mut canvas = sample_canvas();
    canvas.picker.open = false;
    let props = canvas.component_picker_props();
    assert_eq!(props["open"], false);
    // Shape stays — `open: false` is the visibility signal, not key
    // absence (same data-shape rule as gizmos).
    assert!(props.get("candidates").is_some());
}

#[test]
fn pointer_drag_round_trip_under_move_tool() {
    let mut canvas = sample_canvas();
    canvas.tool = ToolMode::Move;
    // Hit somewhere inside the selection bbox.
    assert!(!canvas.pointer_down(100.0, 80.0));
    assert!(canvas.drag_active(), "down captured the drag");
    let dirty = canvas.pointer_move(150.0, 110.0);
    assert!(dirty);
    let pos = canvas.document.root.as_ref().unwrap().transform.position;
    assert_eq!(pos, [150.0, 110.0], "delta applied to position");
    assert!(canvas.pointer_up(0.0, 0.0));
    assert!(!canvas.drag_active(), "up released the drag");
}

#[test]
fn pointer_drag_round_trip_under_rotate_tool() {
    let mut canvas = sample_canvas();
    canvas.tool = ToolMode::Rotate;
    canvas.pointer_down(100.0, 80.0);
    canvas.pointer_move(180.0, 80.0); // dx=80, 0.5°/px = 40°
    let rot = canvas.document.root.as_ref().unwrap().transform.rotation;
    let expected = 40_f32.to_radians();
    assert!((rot - expected).abs() < 1e-4, "got {rot}, want {expected}");
    canvas.pointer_up(0.0, 0.0);
}

#[test]
fn pointer_drag_round_trip_under_scale_tool() {
    let mut canvas = sample_canvas();
    canvas.tool = ToolMode::Scale;
    canvas.pointer_down(100.0, 80.0);
    canvas.pointer_move(200.0, 130.0); // dx=100 → +1.0, dy=50 → +0.5
    let scale = canvas.document.root.as_ref().unwrap().transform.scale;
    assert!((scale[0] - 2.0).abs() < 1e-4);
    assert!((scale[1] - 1.5).abs() < 1e-4);
    canvas.pointer_up(0.0, 0.0);
}

#[test]
fn pointer_down_outside_selection_does_not_capture() {
    let mut canvas = sample_canvas();
    canvas.pointer_down(1000.0, 1000.0);
    assert!(!canvas.drag_active(), "miss must not capture a drag");
    let dirty = canvas.pointer_move(1100.0, 1100.0);
    assert!(!dirty, "no drag, no redraw");
}

#[test]
fn pointer_drag_with_no_selection_is_noop() {
    let mut canvas = sample_canvas();
    canvas.selection = None;
    canvas.pointer_down(100.0, 80.0);
    assert!(!canvas.drag_active());
}

#[test]
fn nav_active_flag_propagates_through_both_emitters() {
    // §19 cross-binding parity: bumping `is_active` shows up in
    // both shapes — list and graph — without duplicate emitter
    // logic. The shared subset is the load-bearing check.
    let mut nav = sample_nav();
    nav.pages[0].is_active = false;
    nav.pages[1].is_active = true;
    let list = nav.nav_page_list_props();
    let graph = nav.nav_graph_props();
    assert_eq!(list["pages"][1]["is-active"], true);
    assert_eq!(graph["pages"][1]["is-active"], true);
}
