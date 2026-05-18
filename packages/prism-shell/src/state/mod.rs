//! `AppState` — slot-typed root of every datum a binding can read.
//!
//! Each top-level field is a *slot*: a typed sub-state owned by one
//! domain (chrome, workspace, selection, overlay, builder, project,
//! …). Slots own their own typed accessors and JSON emitters, so:
//!
//! * The bindings table in [`crate::props`] never reaches across
//!   slot boundaries — every closure asks the slot it cares about
//!   for one fully-shaped JSON value.
//! * `panel_props.rs`-style helpers move *onto* the slot they
//!   describe; there is no separate "shape this state into a row"
//!   layer that the closures have to plumb through.
//! * Adding a new datum is one struct field on the right slot and
//!   one method that returns the JSON shape its block consumes.
//!   Existing bindings keep compiling.
//!
//! Cross-slot composition (e.g. `shell.app-window` mixes chrome data
//! with workspace tabs) takes the *secondary slot as a `&` argument*
//! to the *primary slot's* method — `ChromeSlot::app_window_props(&self,
//! ws: &WorkspaceSlot)`. The bindings table still reads as one row,
//! the JSON shape still lives on exactly one method, and the
//! data-dependency graph stays visible at the call site.
//!
//! See `docs/dev/clay-migration-plan.md` §19.

use prism_builder::{BuilderDocument, NodeId};
use prism_core::foundation::spatial::Transform2D;
use prism_dock::{DockCatalog, DockNode, DockWorkspace};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// Row types that participate in the "selected row + trash button"
/// pattern (B6 fifth wave). Three call sites compose against this
/// trait — nav pages, schema fields, signal connections — and share
/// the [`select_cursor_row`], [`delete_cursor_row`], and
/// [`iter_with_cursor`] helpers below. Adding a fourth cursor-driven
/// row is one impl + three short delegators on the owning slot.
pub trait CursorKey {
    /// String key used to identify this row when the chevron cursor
    /// lands on it. Stable for the row's lifetime in the slot; uniqueness
    /// is the caller's responsibility (mirrors the
    /// `prism_builder::Connection::id` contract).
    fn cursor_key(&self) -> &str;
}

/// Move the chevron `cursor` onto the row whose [`CursorKey::cursor_key`]
/// matches `id`. Returns `true` when the cursor actually moved (no-op
/// for unknown ids and for idempotent re-selects). The three slot-side
/// `select_*` methods all delegate here so the click-route, the
/// `cmd <id>` dispatch, and any future programmatic select converge
/// on one implementation.
pub fn select_cursor_row<T: CursorKey>(items: &[T], cursor: &mut Option<String>, id: &str) -> bool {
    if !items.iter().any(|x| x.cursor_key() == id) {
        return false;
    }
    if cursor.as_deref() == Some(id) {
        return false;
    }
    *cursor = Some(id.to_string());
    true
}

/// Remove the cursored row from `items`, clearing the cursor. Returns
/// `true` when a row was actually dropped (the cursor was set *and*
/// pointed at an extant row). The returned `usize` on the [`pop_cursor_row`]
/// variant is for callers that need to react to the drop site (e.g.
/// `NavigationSlot::delete_selected` promotes the neighbouring page to
/// active when the deleted one held the active flag).
pub fn delete_cursor_row<T: CursorKey>(items: &mut Vec<T>, cursor: &mut Option<String>) -> bool {
    pop_cursor_row(items, cursor).is_some()
}

/// Variant of [`delete_cursor_row`] that returns the original index of
/// the dropped row, so callers can run "after-removal" bookkeeping
/// (e.g. active-flag promotion) without re-scanning. `None` means the
/// cursor was empty or pointed at a stale id.
pub fn pop_cursor_row<T: CursorKey>(
    items: &mut Vec<T>,
    cursor: &mut Option<String>,
) -> Option<usize> {
    let id = cursor.take()?;
    let idx = items.iter().position(|x| x.cursor_key() == id)?;
    items.remove(idx);
    Some(idx)
}

/// Pair each row with its `is_selected` boolean (derived from `cursor`).
/// JSON emitters fold over the returned iterator to write the per-row
/// `selected` / `show-delete` flags without re-implementing the cursor
/// comparison at every call site.
pub fn iter_with_cursor<'a, T: CursorKey>(
    items: &'a [T],
    cursor: Option<&'a str>,
) -> impl Iterator<Item = (&'a T, bool)> {
    items
        .iter()
        .map(move |item| (item, cursor == Some(item.cursor_key())))
}

/// Reloadable root state. `Default` returns the zero-data shell that
/// the §17 contract boots into; ports re-introduce real data slot by
/// slot.
#[derive(Default, Clone)]
pub struct AppState {
    pub chrome: ChromeSlot,
    pub workspace: WorkspaceSlot,
    pub overlay: OverlaySlot,
    pub builder: BuilderSlot,
    pub navigation: NavigationSlot,
    pub catalog: CatalogSlot,
    pub docs: DocsSlot,
    pub menus: MenuSlot,
    pub canvas: CanvasSlot,
    pub project: ProjectSlot,
    pub search: SearchSlot,
    /// **IDE-mode Phase 4 / cross-cutting §4.3** — DevTools / Inspector
    /// panel state. Four lenses (Document / Presence / Probes / Bindings)
    /// rendered through `shell.devtools`. The probe stream and presence
    /// list are append-only buffers the host populates; the bindings
    /// snapshot is captured on demand by the binding closure.
    pub devtools: DevToolsSlot,
    /// Active text-input focus for the property-row field-edit path
    /// (`text` / `color` / `file` kinds). `None` means no field is
    /// editing — the property-row click sets it, `Enter` commits and
    /// clears it, `Esc` abandons (restoring `original`) and clears it.
    /// Every keystroke updates `draft` *and* the bound prop so the
    /// rendered value stays in sync without a separate "commit on
    /// blur" path.
    pub field_focus: Option<FieldFocus>,
    /// `true` while the in-shell code editor (`shell.code-editor`) has
    /// keyboard focus. Clicks on the editor body flip this on; clicks
    /// elsewhere (or Esc) flip it off. Keystrokes route through the
    /// `code_buffer.editor` instead of the per-row field-focus path
    /// — the code editor's larger buffer + multi-line key set (Ctrl+K,
    /// Ctrl+D, etc.) needs the engine's full surface, not the inline-
    /// field subset.
    pub code_editor_focused: bool,
    /// Active pointer-drag against the code editor body. Set on
    /// pointer-down (anchored at the click's byte), updated on
    /// pointer-move (extends the selection through the editor's
    /// `place_caret_at(.., extend=true)` seam), cleared on
    /// pointer-up. Carries the hit's bounds + scroll snapshot so
    /// every move tick can resolve the new byte without re-walking
    /// the surface.
    pub editor_drag: Option<EditorDrag>,
    /// Active pointer-drag for the number-scrubber (`number` /
    /// `integer` field-edits). `None` means no scrub in progress — the
    /// router's pointer-down on a number field-edit initialises this
    /// with the start position + start value, pointer-move updates the
    /// bound prop, pointer-up clears it. Clicks shorter than the drag
    /// threshold fall through to the existing `+1 step` path.
    pub number_drag: Option<NumberDrag>,
    /// **Wave 1** of `docs/dev/composable-builder-plan.md`: optional
    /// shared modifier registry. `Shell::new` populates this once at
    /// boot with the six-baseline `ModifierRegistry::with_builtins()`;
    /// `resync_builder_for_selection` pulls from here to derive
    /// modifier sections + the add-modifier footer. Tests that
    /// pre-date Wave 1 leave it `None` to preserve the flat-rows
    /// shape. The `Arc` keeps `AppState: Clone` cheap and shares the
    /// registry across reloads.
    pub modifier_registry: Option<std::sync::Arc<prism_builder::ModifierRegistry>>,
    /// **Wave 14.3** — set of `(container_id, data-on-<event>-once attr key)`
    /// pairs that have already fired. The router gates `.once` dispatch
    /// through this set so a subsequent click on the same handler
    /// no-ops. Lives on the shell rather than the runtime because
    /// "did this fire already" is per-shell-session state, not
    /// per-frame layout state.
    pub once_fired: std::collections::HashSet<(String, String)>,
}

/// Text-input focus state. Lives on `AppState` rather than a service so
/// every consumer (renderer, click router, key handler) reads it from
/// one place. `original` lets `Esc` restore the prop the user was
/// editing; without it abandoning a half-typed change would still
/// leave the document dirty.
///
/// The `editor` field carries the live edit session: buffer + caret +
/// selection + undo history. The same engine powers single-line
/// string fields (kind `text` / `color` / `file`) and multi-line code
/// editors (kind `textarea` / `code`) — the only difference is
/// `editor.is_multiline()`, set at session start by [`AppState::begin_field_focus`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldFocus {
    pub target_id: String,
    pub key: String,
    pub kind: String,
    pub original: String,
    pub editor: prism_ui_runtime::editor::TextEditor,
}

impl FieldFocus {
    /// Current draft text — i.e. what the field would commit if the
    /// user pressed Enter right now.
    pub fn draft(&self) -> &str {
        self.editor.text()
    }

    pub fn caret_byte(&self) -> usize {
        self.editor.caret_byte()
    }

    pub fn selection(&self) -> Option<(usize, usize)> {
        self.editor.selection()
    }
}

/// Active editor-body drag-select. Set on pointer-down inside the
/// code-editor body; pointer-move resolves a new byte and extends
/// the editor's selection from the anchor; pointer-up clears it.
/// The hit's bounds + scroll snapshot are captured at the anchor
/// so per-move resolution doesn't have to re-look-up the surface.
#[derive(Clone, Debug, PartialEq)]
pub struct EditorDrag {
    pub anchor_byte: usize,
    pub bounds_x: f32,
    pub bounds_y: f32,
    pub bounds_w: f32,
    pub scroll_x: f32,
    pub scroll_y: f32,
    pub font_size: f32,
}

/// Number-scrubber state. Stored on `AppState` (not the canvas slot)
/// because the canvas's existing `pointer_down/move/up` triple is
/// for the document-canvas gizmos; a property-row scrubber lives in a
/// disjoint surface (the right rail) and needs its own capture.
#[derive(Clone, Debug, PartialEq)]
pub struct NumberDrag {
    pub target_id: String,
    pub key: String,
    pub kind: String,
    pub start_x: f32,
    pub start_value: f64,
    pub min: Option<f64>,
    pub max: Option<f64>,
    /// `true` once the pointer moved more than the drag threshold —
    /// distinguishes "click for +1 step" from "drag to scrub" so the
    /// pointer-up handler can dispatch the click variant.
    pub moved: bool,
}

/// Bag of arguments to [`AppState::begin_number_drag`]. Folded into a
/// struct so the call site reads with named fields and clippy's
/// `too_many_arguments` lint stays happy.
pub struct NumberDragInit<'a> {
    pub target_id: &'a str,
    pub key: &'a str,
    pub kind: &'a str,
    pub start_x: f32,
    pub start_value: f64,
    pub min: Option<f64>,
    pub max: Option<f64>,
}

impl AppState {
    /// Cross-slot invariant: clearing the selection clears it on
    /// every slot that owns one. The §25 doctrine — multi-slot
    /// invariants live on the multi-slot type, not on each service
    /// command body. Adding a new selection-bearing slot extends
    /// this method, not every Esc handler.
    pub fn clear_selection(&mut self) {
        self.canvas.selection = None;
        // Wave 3.3: the bbox is keyed to the prior selection; drop
        // it so the next frame doesn't paint a stale gizmo around
        // a node that's no longer selected.
        self.canvas.selection_bbox = None;
        for node in &mut self.builder.inspector {
            node.selected = false;
        }
        // §43 C1: no selection → no property rows. The inspector
        // tree stays (a fresh selection re-flips the flag on the
        // matching row); the form must reset because it's keyed
        // entirely to the selected node's schema.
        self.builder.property_rows.clear();
    }

    /// §43 C1: re-derive `builder.inspector` and `builder.property_rows`
    /// from the current canvas document and selection. Called whenever
    /// a mutation could have changed either side — document edit,
    /// selection change, registry swap.
    ///
    /// Splitting `set_selection` into a derivation pass means the
    /// router can mutate `canvas.selection` directly through whatever
    /// path makes sense (pointer arms, palette commands, keyboard
    /// nav) and then call this once to keep the builder panels in
    /// sync — no per-callsite duplication.
    pub fn resync_builder_for_selection(
        &mut self,
        registry: Option<&prism_builder::ComponentRegistry>,
    ) {
        // Wave 1: pull the modifier registry off `AppState` (set once
        // at boot by `Shell::new`). Tests that leave it `None`
        // preserve the pre-Wave-1 flat-rows shape; the live shell
        // gets one section per attached `node.modifiers` entry plus
        // an add-modifier footer.
        let mod_registry = self.modifier_registry.clone();
        self.builder.inspector =
            derive_inspector_tree(&self.canvas.document, &self.canvas.selection);
        self.builder.property_rows = derive_property_rows(
            registry,
            mod_registry.as_deref(),
            &self.canvas.document,
            self.canvas.selection.as_deref(),
        );
    }

    /// §43 C3: set the canvas selection to a specific doc-node-id and
    /// resync the builder slot. The single mutator the hit-test
    /// router (inspector-row click), the keyboard arrow nudges
    /// (which already mutate `canvas.selection` directly via
    /// `SelectionService`), and any future programmatic select path
    /// all converge on. Returns `true` when the selection moved —
    /// the caller can use this to gate redraw requests.
    pub fn select_node(
        &mut self,
        node_id: &str,
        registry: Option<&prism_builder::ComponentRegistry>,
    ) -> bool {
        // Reject ids that don't exist in the active document — the
        // hit-test surface can produce stale ids when the document
        // changes between layout and click. Headless render paths
        // (no document) also drop here.
        let exists = self
            .canvas
            .document
            .root
            .as_ref()
            .and_then(|r| r.find(node_id))
            .is_some();
        if !exists {
            return false;
        }
        let already = self.canvas.selection.as_deref() == Some(node_id);
        if already {
            return false;
        }
        self.canvas.selection = Some(node_id.into());
        self.resync_builder_for_selection(registry);
        true
    }

    /// Wave 3.2 of `docs/dev/composable-builder-plan.md` — capture a
    /// palette-driven drag at the given pointer position. Returns
    /// `true` when a palette item is armed (`palette_selected.is_some()`)
    /// and a drag session opened; `false` otherwise (the click falls
    /// through to canvas-node-select or the gizmo capture).
    pub fn begin_palette_drag(&mut self, x: f32, y: f32, drop_target: Option<&str>) -> bool {
        let Some(kind) = self.catalog.palette_selected.clone() else {
            return false;
        };
        self.catalog.palette_drag = Some(PaletteDrag {
            kind,
            pointer: (x, y),
            drop_target: drop_target.map(str::to_string),
        });
        true
    }

    /// Wave 3.2 — update the in-flight palette drag's pointer and
    /// current drop target. Returns `true` when the drag is active so
    /// the next frame redraws the ghost; `false` when no drag is in
    /// flight (the pointer-move falls through to the canvas).
    pub fn update_palette_drag(&mut self, x: f32, y: f32, drop_target: Option<&str>) -> bool {
        let Some(drag) = self.catalog.palette_drag.as_mut() else {
            return false;
        };
        drag.pointer = (x, y);
        drag.drop_target = drop_target.map(str::to_string);
        true
    }

    /// Wave 3.2 — commit the in-flight palette drag. Materialises a
    /// fresh node of the dragged kind, inserts it under the resolved
    /// drop target (or under the document root when none was hit),
    /// clears the palette-selected pill, and moves the canvas
    /// selection onto the new node so the inspector / properties
    /// refresh against it. Returns the new node id on success.
    pub fn end_palette_drag(
        &mut self,
        registry: Option<&prism_builder::ComponentRegistry>,
    ) -> Option<NodeId> {
        let drag = self.catalog.palette_drag.take()?;
        // Clear the armed pill so subsequent clicks don't re-trigger
        // a drop; the user re-picks an item to drop again. Matches
        // the discoverable Figma/Sketch model — palette pick =
        // one-shot intent.
        self.catalog.palette_selected = None;
        let value = palette_node_template(&drag.kind)?;
        let new_id = self
            .canvas
            .insert_under_node(value, drag.drop_target.as_deref())?;
        self.canvas.selection = Some(new_id.clone());
        self.resync_builder_for_selection(registry);
        Some(new_id)
    }

    /// Wave 3.2 — discard the in-flight palette drag without
    /// inserting anything. Used by Esc-cancel and by routes that
    /// pre-empt the drag (e.g. the user opens a context menu mid-drag).
    /// Returns `true` when a drag was actually cancelled.
    pub fn cancel_palette_drag(&mut self) -> bool {
        self.catalog.palette_drag.take().is_some()
    }

    /// Wave 3.4 — open the canvas context menu at `(x, y)`. When
    /// `target_id` resolves to a canvas-document node the menu is
    /// populated with that node's actions (move up / move down /
    /// delete / duplicate); on the empty canvas it falls back to
    /// document-level actions (paste from clipboard when non-empty).
    /// Returns `true` when the menu actually opened — `false` when
    /// no actions would be available so the existing menu state
    /// stays untouched.
    pub fn open_context_menu(
        &mut self,
        _x: f32,
        _y: f32,
        target_id: Option<&str>,
        registry: Option<&prism_builder::ComponentRegistry>,
    ) -> bool {
        // Target id moves the canvas selection so subsequent
        // command activation (`builder.delete-selected`,
        // `builder.move-selected-up`, …) operates on the right
        // node. Empty-canvas right-clicks clear the selection so
        // paste-from-clipboard lands at root.
        if let Some(id) = target_id {
            self.select_node(id, registry);
        }
        let items = canvas_context_menu_items(self);
        if items.is_empty() {
            return false;
        }
        self.menus.context = items;
        true
    }

    /// Wave 3.4 — close the canvas context menu. Returns `true`
    /// when the menu was actually open (so the next frame
    /// re-renders without it).
    pub fn close_context_menu(&mut self) -> bool {
        if self.menus.context.is_empty() {
            return false;
        }
        self.menus.context.clear();
        true
    }

    /// Wave 3.3 — set the canvas's selection bbox from a hit-test
    /// result. Called by `route_canvas_node_select` after the
    /// selection cursor moves so the next frame paints the gizmo +
    /// 8-handle ring around the actual click rect. `None` clears the
    /// bbox so a stale outline doesn't survive a deselect.
    pub fn set_selection_bbox(&mut self, bbox: Option<SelectionBbox>) {
        self.canvas.selection_bbox = bbox;
    }

    // ── Wave 4 signal-connection mutators ───────────────────────────
    //
    // The signals panel reads `state.builder.signal_connections` as a
    // flat view-model list. Wave 4 adds the four edit paths the panel
    // can drive:
    //
    // * `add_signal_connection`     — push a fresh row
    // * `update_signal_connection_field` — edit one cell on a row
    // * `delete_selected_signal_connection` — already on `BuilderSlot`
    //
    // Each mutator ends with `resync_builder_for_selection` so the
    // panel rows + inspector tree stay coherent. The connection
    // picker overlay (Wave 4.3) drives `add_signal_connection`
    // through `confirm_connection_picker`.

    /// Wave 4.4 — append a fresh `SignalConnection` and refresh the
    /// builder panels. Returns the id of the row (used by tests +
    /// the picker confirm to move the cursor onto the newly-created
    /// row so the user can immediately delete or re-edit it).
    pub fn add_signal_connection(
        &mut self,
        connection: SignalConnection,
        registry: Option<&prism_builder::ComponentRegistry>,
    ) -> String {
        let id = connection.id.clone();
        self.builder.signal_connections.push(connection);
        // Move the chevron cursor onto the new row so the trash
        // affordance is immediately reachable; mirrors
        // `attach_modifier`'s "land on the freshly-created row" UX.
        self.builder.selected_connection = Some(id.clone());
        self.resync_builder_for_selection(registry);
        id
    }

    /// Wave 4.4 — write one field (`source-signal` / `action-kind` /
    /// `target-label`) on the named connection. Returns `true` when
    /// the document actually changed. Unknown ids and unknown field
    /// keys are clean no-ops so a stale picker dispatch doesn't blow
    /// up; the caller can gate redraws on the return value.
    pub fn update_signal_connection_field(
        &mut self,
        connection_id: &str,
        field: &str,
        value: &str,
        registry: Option<&prism_builder::ComponentRegistry>,
    ) -> bool {
        let Some(conn) = self
            .builder
            .signal_connections
            .iter_mut()
            .find(|c| c.id == connection_id)
        else {
            return false;
        };
        let slot: &mut String = match field {
            "source-signal" => &mut conn.source_signal,
            "action-kind" => &mut conn.action_kind,
            "target-label" => &mut conn.target_label,
            _ => return false,
        };
        if slot == value {
            return false;
        }
        *slot = value.to_string();
        self.resync_builder_for_selection(registry);
        true
    }

    // ── Wave 4.3 connection-picker mutators ─────────────────────────

    /// Wave 4.3 — flip the picker open with sensible defaults so the
    /// user starts on a usable form shape. Re-opening an already-
    /// open picker is a clean no-op; the existing fields persist so
    /// an accidental click on the "+" button doesn't reset the
    /// user's in-progress entry.
    /// Wave 2.4 — open the color-picker overlay against a
    /// `(target_id, key, value)` triple. Idempotent against an
    /// already-open picker pinned to the same target. Returns
    /// `true` when the picker state actually changed.
    pub fn open_color_picker(&mut self, target_id: &str, key: &str, value: &str) -> bool {
        let p = &self.overlay.color_picker;
        if p.open && p.target_id == target_id && p.key == key {
            return false;
        }
        self.overlay.color_picker = ColorPicker {
            open: true,
            target_id: target_id.to_string(),
            key: key.to_string(),
            value: value.to_string(),
            slider_drag: None,
        };
        true
    }

    /// Wave 2.4 — close the color picker. Used by Esc, the close
    /// button, clicking outside, and post-commit. Returns `true`
    /// when it was actually open.
    pub fn close_color_picker(&mut self) -> bool {
        if !self.overlay.color_picker.open {
            return false;
        }
        self.overlay.color_picker = ColorPicker::default();
        true
    }

    /// Wave 2.4 — commit a color value (from a preset click, the
    /// hex echo input, or a future HSL slider) through
    /// `set_node_prop`. The picker stays open so the user can
    /// preview successive presets without re-clicking the swatch;
    /// the close path is the dedicated `close_color_picker` /
    /// `cancel-color-picker` route. Returns `true` when the bound
    /// prop actually moved.
    pub fn set_color_picker_value(
        &mut self,
        value: &str,
        registry: Option<&prism_builder::ComponentRegistry>,
    ) -> bool {
        let picker = self.overlay.color_picker.clone();
        if !picker.open || picker.target_id.is_empty() || picker.key.is_empty() {
            return false;
        }
        self.overlay.color_picker.value = value.to_string();
        self.set_node_prop(
            &picker.target_id,
            &picker.key,
            Value::String(value.to_string()),
            registry,
        )
    }

    /// Wave 2.3 — open the select-dropdown overlay against a
    /// `(target_id, key)` pair. Idempotent against an already-open
    /// dropdown pinned to the same target. Returns `true` when the
    /// state actually changed. Options are passed verbatim from the
    /// field-editor row's `data-options-json` attr so the dropdown
    /// renders the same labelled rows the schema declared.
    pub fn open_select_dropdown(
        &mut self,
        target_id: &str,
        key: &str,
        value: &str,
        options: Vec<Value>,
    ) -> bool {
        let p = &self.overlay.select_dropdown;
        if p.open && p.target_id == target_id && p.key == key {
            return false;
        }
        self.overlay.select_dropdown = SelectDropdown {
            open: true,
            target_id: target_id.to_string(),
            key: key.to_string(),
            value: value.to_string(),
            options,
        };
        true
    }

    /// Wave 2.3 — close the dropdown without committing. Used by
    /// Esc, the close button, clicking outside, and post-commit.
    /// Returns `true` when it was actually open.
    pub fn close_select_dropdown(&mut self) -> bool {
        if !self.overlay.select_dropdown.open {
            return false;
        }
        self.overlay.select_dropdown = SelectDropdown::default();
        true
    }

    /// Wave 2.3 — commit a select option value through `set_node_prop`
    /// and close the dropdown. Returns `true` when the bound prop
    /// actually moved (closing happens regardless).
    pub fn commit_select_dropdown_value(
        &mut self,
        value: &str,
        registry: Option<&prism_builder::ComponentRegistry>,
    ) -> bool {
        let picker = self.overlay.select_dropdown.clone();
        self.overlay.select_dropdown = SelectDropdown::default();
        if picker.target_id.is_empty() || picker.key.is_empty() {
            return false;
        }
        self.set_node_prop(
            &picker.target_id,
            &picker.key,
            Value::String(value.to_string()),
            registry,
        )
    }

    pub fn open_connection_picker(&mut self) -> bool {
        if self.overlay.connection_picker.open {
            return false;
        }
        self.overlay.connection_picker = ConnectionPicker {
            open: true,
            source_signal: "clicked".into(),
            action_kind: "SetProperty".into(),
            target_label: String::new(),
        };
        true
    }

    /// Wave 4.3 — close the picker without inserting. Used by Esc,
    /// Cancel, and the post-confirm clear inside
    /// `confirm_connection_picker`.
    pub fn close_connection_picker(&mut self) -> bool {
        if !self.overlay.connection_picker.open {
            return false;
        }
        self.overlay.connection_picker = ConnectionPicker::default();
        true
    }

    /// Wave 4.3 — write one of the picker's three form fields. Used
    /// by the picker's click-to-cycle action-kind row and any future
    /// text-input integration on the source / target fields.
    /// Returns `true` when the field actually changed.
    pub fn set_connection_picker_field(&mut self, field: &str, value: &str) -> bool {
        let picker = &mut self.overlay.connection_picker;
        let slot: &mut String = match field {
            "source-signal" => &mut picker.source_signal,
            "action-kind" => &mut picker.action_kind,
            "target-label" => &mut picker.target_label,
            _ => return false,
        };
        if slot == value {
            return false;
        }
        *slot = value.to_string();
        true
    }

    /// Wave 4.3 — advance the picker's action-kind through the
    /// declared variant list, wrapping at the end. Mirrors the
    /// click-to-cycle semantics on `field-edit` rows for `select`
    /// kinds. Returns `true` when the value moved.
    pub fn cycle_connection_picker_action_kind(&mut self) -> bool {
        let next = ConnectionPicker::cycle_action_kind(&self.overlay.connection_picker.action_kind);
        if next == self.overlay.connection_picker.action_kind {
            return false;
        }
        self.overlay.connection_picker.action_kind = next.to_string();
        true
    }

    /// Wave 4.3 — commit the picker's form into a fresh
    /// `SignalConnection`. The id is generated from the source +
    /// target so re-confirming a duplicate yields a stable
    /// collision name; `rename_for_uniqueness` is the deduper.
    /// Returns the freshly-created id; the picker closes regardless.
    pub fn confirm_connection_picker(
        &mut self,
        registry: Option<&prism_builder::ComponentRegistry>,
    ) -> Option<String> {
        let picker = self.overlay.connection_picker.clone();
        if !picker.open || picker.source_signal.is_empty() {
            self.close_connection_picker();
            return None;
        }
        let raw_id = format!(
            "c-{}-{}",
            sanitise_id(&picker.source_signal),
            sanitise_id(&picker.target_label),
        );
        let id = uniquify_connection_id(&self.builder.signal_connections, &raw_id);
        let connection = SignalConnection {
            id: id.clone(),
            source_signal: picker.source_signal,
            action_kind: picker.action_kind,
            target_label: picker.target_label,
        };
        self.add_signal_connection(connection, registry);
        self.close_connection_picker();
        Some(id)
    }

    /// Wave 3.3 — start a resize drag against the currently-selected
    /// canvas node. `direction` is the handle's `data-direction`
    /// emission (`tl|t|tr|r|br|b|bl|l`); `(x, y)` is the
    /// pointer-down position. Returns `true` when a session opens
    /// (selection exists + handle direction recognised); `false`
    /// otherwise so the canvas's `pointer_down` falls through to
    /// the existing gizmo capture.
    pub fn begin_resize_drag(&mut self, direction: &str, x: f32, y: f32) -> bool {
        let Some(node_id) = self.canvas.selection.clone() else {
            return false;
        };
        let Some(snapshot) = self
            .canvas
            .document
            .root
            .as_ref()
            .and_then(|r| r.find(&node_id))
            .map(|n| n.transform.clone())
        else {
            return false;
        };
        if !matches!(direction, "tl" | "t" | "tr" | "r" | "br" | "b" | "bl" | "l") {
            return false;
        }
        self.canvas.resize_drag = Some(ResizeDragState {
            direction: direction.to_string(),
            node_id,
            origin: (x, y),
            snapshot,
        });
        true
    }

    /// Wave 3.3 — apply a resize delta to the selection's transform.
    /// The drag mutates `transform.position` along the handle's
    /// outward axes (matching `apply_handle_delta`'s discipline —
    /// real width/height mutators land alongside the per-node
    /// `width` / `height` layout API). Returns `true` when an
    /// active session translated the node; `false` when no drag is
    /// in flight.
    pub fn update_resize_drag(&mut self, x: f32, y: f32) -> bool {
        let Some(drag) = self.canvas.resize_drag.as_ref().cloned() else {
            return false;
        };
        let zoom = self.canvas.viewport.zoom.max(f32::EPSILON);
        let dx = (x - drag.origin.0) / zoom;
        let dy = (y - drag.origin.1) / zoom;
        let (px, py) = resize_delta_signs(&drag.direction);
        let Some(node) = self
            .canvas
            .document
            .root
            .as_mut()
            .and_then(|r| r.find_mut(&drag.node_id))
        else {
            return false;
        };
        node.transform.position[0] = drag.snapshot.position[0] + px * dx;
        node.transform.position[1] = drag.snapshot.position[1] + py * dy;
        true
    }

    /// Wave 3.3 — commit the in-flight resize drag. Returns `true`
    /// when a session was active so the next frame redraws against
    /// the mutated transform.
    pub fn end_resize_drag(&mut self) -> bool {
        self.canvas.resize_drag.take().is_some()
    }

    /// §43 C2: set one property on a doc node and resync the builder
    /// slot. Acts as the "BuilderService::set_node_prop" mutator the
    /// plan calls for — every property-edit code path (hit-test
    /// field-editor click, future Luau action, future programmatic
    /// edit) routes through this method so the derivation pass runs
    /// exactly once per edit. Returns `true` when the document
    /// actually changed.
    pub fn set_node_prop(
        &mut self,
        node_id: &str,
        key: &str,
        value: Value,
        registry: Option<&prism_builder::ComponentRegistry>,
    ) -> bool {
        // PartialEq gate: skip the derivation pass on idempotent
        // edits. The reactive seam (`NodeMutator::write`) is
        // unconditional otherwise — repeated equal writes still wake
        // subscribers (matches `Signal::set` semantics).
        let target = self
            .canvas
            .document
            .root
            .as_mut()
            .and_then(|r| r.find_mut(node_id));
        let Some(target) = target else {
            return false;
        };
        if target.props.get(key) == Some(&value) {
            return false;
        }
        prism_builder::NodeMutator::with_bindings(&self.canvas.bindings).write(target, key, value);
        self.resync_builder_for_selection(registry);
        true
    }

    // ── Wave 1.5 modifier mutators ───────────────────────────────────
    //
    // Each mutator validates against the doc, short-circuits on
    // no-op edits, and ends with `resync_builder_for_selection` so
    // the inspector + property rows stay coherent. Mirrors §43 C
    // discipline: one re-derivation seam, never per-callsite.

    /// Wave 1.5 — attach a behaviour to a doc node. Returns `true` when
    /// the document actually changed. Rejects unknown node ids and
    /// duplicate attachments (one modifier of each id per node).
    pub fn attach_modifier(
        &mut self,
        node_id: &str,
        modifier_id: &str,
        registry: Option<&prism_builder::ComponentRegistry>,
    ) -> bool {
        let target = self
            .canvas
            .document
            .root
            .as_mut()
            .and_then(|r| r.find_mut(node_id));
        let Some(target) = target else {
            return false;
        };
        if target.modifiers.iter().any(|m| m.kind == modifier_id) {
            return false;
        }
        target
            .modifiers
            .push(prism_builder::Modifier::new(modifier_id));
        self.resync_builder_for_selection(registry);
        true
    }

    /// Wave 1.5 — remove an attached behaviour at `idx`. No-op on
    /// out-of-range or unknown node id.
    pub fn detach_modifier(
        &mut self,
        node_id: &str,
        idx: usize,
        registry: Option<&prism_builder::ComponentRegistry>,
    ) -> bool {
        let target = self
            .canvas
            .document
            .root
            .as_mut()
            .and_then(|r| r.find_mut(node_id));
        let Some(target) = target else {
            return false;
        };
        if idx >= target.modifiers.len() {
            return false;
        }
        target.modifiers.remove(idx);
        self.resync_builder_for_selection(registry);
        true
    }

    /// Wave 1.5 — flip the `enabled` flag on a modifier. The render
    /// fold skips disabled entries; the inspector emits the header but
    /// suppresses the schema rows (so the panel collapses for off
    /// behaviours).
    pub fn toggle_modifier(
        &mut self,
        node_id: &str,
        idx: usize,
        registry: Option<&prism_builder::ComponentRegistry>,
    ) -> bool {
        let target = self
            .canvas
            .document
            .root
            .as_mut()
            .and_then(|r| r.find_mut(node_id));
        let Some(target) = target else {
            return false;
        };
        let Some(modifier) = target.modifiers.get_mut(idx) else {
            return false;
        };
        modifier.enabled = !modifier.enabled;
        self.resync_builder_for_selection(registry);
        true
    }

    /// Wave 1.5 — reorder modifiers in the stack (innermost-first
    /// `wrap` order is read off the `Vec` in reverse, so swapping
    /// indices visibly changes the on-canvas composition). No-op on
    /// out-of-range indices or `from == to`.
    pub fn reorder_modifier(
        &mut self,
        node_id: &str,
        from: usize,
        to: usize,
        registry: Option<&prism_builder::ComponentRegistry>,
    ) -> bool {
        if from == to {
            return false;
        }
        let target = self
            .canvas
            .document
            .root
            .as_mut()
            .and_then(|r| r.find_mut(node_id));
        let Some(target) = target else {
            return false;
        };
        if from >= target.modifiers.len() || to >= target.modifiers.len() {
            return false;
        }
        let m = target.modifiers.remove(from);
        target.modifiers.insert(to, m);
        self.resync_builder_for_selection(registry);
        true
    }

    /// Wave 1.5 — write one prop on an attached modifier. Mirror of
    /// `set_node_prop` for the modifier sections of the inspector;
    /// the field-edit router dispatches here when a row carries
    /// `data-edit-target="modifier"` + `data-modifier-idx`.
    pub fn set_modifier_prop(
        &mut self,
        node_id: &str,
        modifier_idx: usize,
        key: &str,
        value: serde_json::Value,
        registry: Option<&prism_builder::ComponentRegistry>,
    ) -> bool {
        let target = self
            .canvas
            .document
            .root
            .as_mut()
            .and_then(|r| r.find_mut(node_id));
        let Some(target) = target else {
            return false;
        };
        let Some(modifier) = target.modifiers.get_mut(modifier_idx) else {
            return false;
        };
        // Coerce existing `props` to an object if needed (a brand-new
        // modifier has `props: Null`).
        if !modifier.props.is_object() {
            modifier.props = serde_json::Value::Object(serde_json::Map::new());
        }
        let map = modifier.props.as_object_mut().expect("just coerced");
        if map.get(key) == Some(&value) {
            return false;
        }
        map.insert(key.into(), value);
        self.resync_builder_for_selection(registry);
        true
    }

    /// Begin (or move) a text-input focus session onto a property-row
    /// field. Pulls the current value from the doc node so `Esc` can
    /// restore it; mirrors the "click sets focus" model. Returns
    /// `true` when the focus actually moved (no-op when the same
    /// target/key already holds focus).
    pub fn begin_field_focus(&mut self, target_id: &str, key: &str, kind: &str) -> bool {
        if let Some(focus) = &self.field_focus {
            if focus.target_id == target_id && focus.key == key {
                return false;
            }
        }
        let original = self
            .canvas
            .document
            .root
            .as_ref()
            .and_then(|r| r.find(target_id))
            .and_then(|n| n.props.get(key))
            .map(value_as_string)
            .unwrap_or_default();
        let multiline = matches!(kind, "textarea" | "code");
        let editor =
            prism_ui_runtime::editor::TextEditor::with_text(original.clone()).multiline(multiline);
        self.field_focus = Some(FieldFocus {
            target_id: target_id.into(),
            key: key.into(),
            kind: kind.into(),
            original,
            editor,
        });
        true
    }

    /// Append one chunk of typed text to the focused field's draft and
    /// flush the new value into the underlying doc-node prop. Returns
    /// `true` when a focus session was active.
    pub fn type_field_text(
        &mut self,
        text: &str,
        registry: Option<&prism_builder::ComponentRegistry>,
    ) -> bool {
        let Some(focus) = self.field_focus.as_mut() else {
            return false;
        };
        focus.editor.apply_text(text);
        self.flush_focus_to_prop(registry);
        true
    }

    /// Flush the focused field's editor text into the bound doc prop.
    /// Called after every mutation that the shared text-input dispatch
    /// performs directly on `field_focus.editor`. Public because
    /// `FieldFocusService` drives the editor through the shared
    /// helper and reaches back here for the prop side of the seam.
    pub fn flush_focus_to_prop(&mut self, registry: Option<&prism_builder::ComponentRegistry>) {
        let Some(focus) = self.field_focus.as_ref() else {
            return;
        };
        let target = focus.target_id.clone();
        let key = focus.key.clone();
        let draft = focus.editor.text().to_string();
        self.set_node_prop(&target, &key, Value::String(draft), registry);
    }

    /// Commit the focused field. The draft is already flushed into the
    /// prop by every keystroke, so this just clears the focus. Returns
    /// `true` when a focus session ended.
    pub fn commit_field_focus(&mut self) -> bool {
        self.field_focus.take().is_some()
    }

    /// Abandon the focused field. Restores the original value (the one
    /// the prop carried when focus began) and clears the focus session.
    pub fn cancel_field_focus(
        &mut self,
        registry: Option<&prism_builder::ComponentRegistry>,
    ) -> bool {
        let Some(focus) = self.field_focus.take() else {
            return false;
        };
        // Restore — but only if the draft actually diverged. Skipping
        // the write on no-op edits keeps the resync pass off the
        // happy path.
        if focus.editor.text() != focus.original {
            self.set_node_prop(
                &focus.target_id,
                &focus.key,
                Value::String(focus.original),
                registry,
            );
        }
        true
    }

    /// Begin a pointer-scrub session on a number / integer field-edit.
    /// `min` / `max` come from the schema bounds; clamping happens in
    /// [`Self::update_number_drag`]. The session records the *start*
    /// pointer x and start value so move deltas can be applied
    /// without re-reading the attr on every move tick.
    pub fn begin_number_drag(&mut self, drag: NumberDragInit<'_>) {
        self.number_drag = Some(NumberDrag {
            target_id: drag.target_id.into(),
            key: drag.key.into(),
            kind: drag.kind.into(),
            start_x: drag.start_x,
            start_value: drag.start_value,
            min: drag.min,
            max: drag.max,
            moved: false,
        });
    }

    /// Apply a pointer-move tick to the active scrub session. Returns
    /// the new value and `true` when the underlying prop actually
    /// moved (so the router can request a redraw). The first move
    /// past the drag threshold flips `moved`, which suppresses the
    /// fallback "click for +1" path on pointer-up.
    pub fn update_number_drag(
        &mut self,
        x: f32,
        registry: Option<&prism_builder::ComponentRegistry>,
    ) -> bool {
        const DRAG_THRESHOLD_PX: f32 = 3.0;
        const PX_PER_UNIT: f32 = 4.0;
        let Some(drag) = self.number_drag.as_ref() else {
            return false;
        };
        let dx = x - drag.start_x;
        if !drag.moved && dx.abs() < DRAG_THRESHOLD_PX {
            return false;
        }
        let raw = drag.start_value + (dx / PX_PER_UNIT) as f64;
        let clamped = match (drag.min, drag.max) {
            (Some(mn), Some(mx)) => raw.clamp(mn, mx),
            (Some(mn), None) => raw.max(mn),
            (None, Some(mx)) => raw.min(mx),
            (None, None) => raw,
        };
        let value = if drag.kind == "integer" {
            Value::from(clamped.round() as i64)
        } else {
            json!(clamped)
        };
        let (target, key) = (drag.target_id.clone(), drag.key.clone());
        let drag_mut = self.number_drag.as_mut().expect("checked above");
        drag_mut.moved = true;
        self.set_node_prop(&target, &key, value, registry)
    }

    /// End the active scrub session. Returns whether the user dragged
    /// far enough to count as a scrub — callers use the inverted
    /// answer to dispatch the click variant (the existing `+1` step
    /// in the field-edit router).
    pub fn end_number_drag(&mut self) -> Option<bool> {
        self.number_drag.take().map(|d| d.moved)
    }
}

mod inspector;
pub(crate) use inspector::*;

mod slots_core;
pub use slots_core::*;

mod overlay;
pub use overlay::*;

mod slots_doc;
pub use slots_doc::*;

mod canvas;
pub use canvas::*;

#[cfg(test)]
mod tests;
