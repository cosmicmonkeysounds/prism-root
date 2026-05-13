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
use prism_dock::DockWorkspace;
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
    /// Active text-input focus for the property-row field-edit path
    /// (`text` / `color` / `file` kinds). `None` means no field is
    /// editing — the property-row click sets it, `Enter` commits and
    /// clears it, `Esc` abandons (restoring `original`) and clears it.
    /// Every keystroke updates `draft` *and* the bound prop so the
    /// rendered value stays in sync without a separate "commit on
    /// blur" path.
    pub field_focus: Option<FieldFocus>,
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
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldFocus {
    pub target_id: String,
    pub key: String,
    pub kind: String,
    pub draft: String,
    pub original: String,
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
        self.field_focus = Some(FieldFocus {
            target_id: target_id.into(),
            key: key.into(),
            kind: kind.into(),
            draft: original.clone(),
            original,
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
        focus.draft.push_str(text);
        let (target, key, draft) = (
            focus.target_id.clone(),
            focus.key.clone(),
            focus.draft.clone(),
        );
        self.set_node_prop(&target, &key, Value::String(draft), registry);
        true
    }

    /// Drop the last character from the focused field's draft (UTF-8
    /// safe) and flush. Returns `true` when a focus session was active
    /// *and* something was actually deleted (an empty draft is a
    /// no-op so repeated backspaces don't keep firing resyncs).
    pub fn backspace_field(&mut self, registry: Option<&prism_builder::ComponentRegistry>) -> bool {
        let Some(focus) = self.field_focus.as_mut() else {
            return false;
        };
        if focus.draft.pop().is_none() {
            return false;
        }
        let (target, key, draft) = (
            focus.target_id.clone(),
            focus.key.clone(),
            focus.draft.clone(),
        );
        self.set_node_prop(&target, &key, Value::String(draft), registry);
        true
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
        if focus.draft != focus.original {
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

/// Project an arbitrary serde value to its textual form so the
/// field-focus path can populate `original` regardless of the prop's
/// json kind. Strings come through as-is; numbers / booleans go
/// through `to_string`; null and arrays / objects fall through as the
/// empty string (the field-edit kinds that focus today —
/// `text` / `color` / `file` — are always stringly typed).
fn value_as_string(v: &Value) -> String {
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
fn derive_inspector_tree(
    doc: &prism_builder::BuilderDocument,
    selection: &Option<NodeId>,
) -> Vec<InspectorNode> {
    let mut out: Vec<InspectorNode> = Vec::new();
    if let Some(root) = doc.root.as_ref() {
        walk_inspector(root, 0, selection.as_deref(), &mut out);
    }
    out
}

fn walk_inspector(
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
fn inspector_label_for(node: &prism_builder::Node) -> String {
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
fn derive_property_rows(
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
fn property_row_from_modifier_spec(
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
fn property_row_from_spec(
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
    let mut row_props = json!({
        "key": spec.key,
        "label": spec.label,
        "kind": kind,
        "value": value,
        "required": spec.required,
        "target-id": target_id,
    });
    // Kind-specific extensions: select carries its options so the
    // click-to-cycle path (`handle_field_edit_click`) can step through
    // them without re-resolving the spec; number / integer carry their
    // bounds so the +/- step clamps at the schema-declared range.
    match &spec.kind {
        FieldKind::Select(options) => {
            row_props["options"] = Value::Array(
                options
                    .iter()
                    .map(|o| json!({ "value": o.value, "label": o.label }))
                    .collect(),
            );
        }
        FieldKind::Number(bounds) | FieldKind::Integer(bounds) => {
            if let Some(min) = bounds.min {
                row_props["min"] = json!(min);
            }
            if let Some(max) = bounds.max {
                row_props["max"] = json!(max);
            }
        }
        _ => {}
    }
    PropertyRow {
        component: "shell.field-editor".into(),
        props: row_props,
    }
}

// ── project ───────────────────────────────────────────────────────

/// IO-side state for `PersistenceService` + `ProjectService`. Holds
/// the active document file (Save / Save As target) and the active
/// project folder (Open Folder / Close Folder). `dirty` is the one
/// shared flag both services flip — the title bar reads it through
/// chrome via [`Self::title_suffix`], the only cross-slot reader.
///
/// See `docs/dev/clay-migration-plan.md` §26.
#[derive(Clone, Debug, Default)]
pub struct ProjectSlot {
    pub current_file: Option<std::path::PathBuf>,
    pub root: Option<std::path::PathBuf>,
    pub dirty: bool,
    /// Recently opened files — populated by `PersistenceService` on
    /// every successful open/save, capped at 8 entries.
    pub recent: Vec<std::path::PathBuf>,
}

impl ProjectSlot {
    pub const RECENT_LIMIT: usize = 8;

    pub fn touch(&mut self, path: std::path::PathBuf) {
        self.recent.retain(|p| p != &path);
        self.recent.insert(0, path);
        self.recent.truncate(Self::RECENT_LIMIT);
    }

    /// `" — Untitled"` / `" — foo.prism *"` etc. The title-bar shape
    /// lives here, not in chrome, so chrome stays a static slot.
    pub fn title_suffix(&self) -> String {
        let label = match (&self.current_file, &self.root) {
            (Some(p), _) => p
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("Untitled")
                .to_string(),
            (None, Some(r)) => r
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("Project")
                .to_string(),
            _ => "Untitled".to_string(),
        };
        let mark = if self.dirty { " *" } else { "" };
        format!(" — {label}{mark}")
    }
}

// ── search ────────────────────────────────────────────────────────

/// Search overlay state — query buffer, ranked hits, modal-open flag.
/// `SearchService` owns every mutator; bindings only read.
///
/// See `docs/dev/clay-migration-plan.md` §26.
#[derive(Clone, Debug, Default)]
pub struct SearchSlot {
    pub open: bool,
    pub query: String,
    pub results: Vec<SearchHit>,
    pub selected_index: usize,
}

#[derive(Clone, Debug)]
pub struct SearchHit {
    pub node_id: String,
    pub label: String,
    pub snippet: String,
    pub score: f32,
}

impl SearchSlot {
    pub fn search_overlay_props(&self) -> Value {
        json!({
            "open": self.open,
            "query": self.query,
            "selected-index": self.selected_index,
            "results": Value::Array(
                self.results.iter().map(|h| json!({
                    "node-id": h.node_id,
                    "label": h.label,
                    "snippet": h.snippet,
                    "score": h.score,
                })).collect()
            ),
        })
    }
}

// ── chrome ────────────────────────────────────────────────────────

/// Static chrome strings — app name in the menu row, status string
/// in the bottom bar. The four nav-buttons (Home/etc.) live here too;
/// they're pure chrome ornament with no data dependencies.
///
/// Three bindings read from this slot:
/// `shell.status-bar` (status only), `shell.menu-bar-row` (chrome +
/// workspace tabs), `shell.app-window` (chrome + workspace tabs +
/// nav-buttons).
#[derive(Clone, Debug)]
pub struct ChromeSlot {
    pub app_name: String,
    pub status: String,
    pub nav_buttons: Vec<NavButton>,
    pub menus: Vec<MenuLabel>,
    /// Currently expanded menu pill (`Some(id)` when a menu dropdown
    /// should render). The block reads this through `active-menu`
    /// in `menu_bar_row_props`. `None` means no menu is open.
    pub active_menu: Option<String>,
}

/// Top-bar menu pill. Rendered in `shell.menu-bar-row` and reused by
/// `shell.app-window` for embedded chrome. The `id` is the stable
/// routing key — surfaced as `data-target-id` on the rendered pill so
/// `handle_menu_pill_click` can move the active-menu cursor without
/// a parallel array lookup.
#[derive(Clone, Debug)]
pub struct MenuLabel {
    pub id: String,
    pub label: String,
}

/// Activity-bar button. The runtime block reads `icon`/`selected`/`nav-id`;
/// the `id` is the stable routing key (`home` / `folder` / `search` / …)
/// so `handle_nav_button_click` can flip the radio-style selection
/// state on the matching row.
#[derive(Clone, Debug)]
pub struct NavButton {
    pub id: String,
    pub icon: String,
    pub selected: bool,
}

impl Default for ChromeSlot {
    fn default() -> Self {
        Self {
            app_name: "Prism".into(),
            status: "Ready".into(),
            nav_buttons: vec![NavButton {
                id: "home".into(),
                icon: "icons/home.svg".into(),
                selected: true,
            }],
            menus: ["File", "Edit", "View", "Help"]
                .into_iter()
                .map(|l| MenuLabel {
                    id: l.to_lowercase(),
                    label: l.into(),
                })
                .collect(),
            active_menu: None,
        }
    }
}

impl ChromeSlot {
    /// Toggle which activity-bar button is the radio-selected one. The
    /// `nav-id` flows from the click handler; clicking the already-
    /// selected button is a no-op (`returns false`) so the frame can
    /// stay clean. Clicking a different button flips both the old and
    /// new rows' `selected` flags in one pass.
    pub fn select_nav_button(&mut self, id: &str) -> bool {
        if self.nav_buttons.iter().any(|b| b.id == id && b.selected) {
            return false;
        }
        let mut moved = false;
        for b in self.nav_buttons.iter_mut() {
            let want = b.id == id;
            if b.selected != want {
                b.selected = want;
                moved = true;
            }
        }
        moved
    }

    /// Toggle the active menu cursor. Clicking the already-open menu
    /// closes it; clicking a different pill moves the cursor; clicking
    /// an unknown id is a no-op. The `shell.menu-bar-row` block reads
    /// the cursor through the `active-menu` index emission so the
    /// pill paints its `aria-expanded` + tinted background on the
    /// next frame.
    pub fn select_menu(&mut self, id: &str) -> bool {
        let next = if self.active_menu.as_deref() == Some(id) {
            None
        } else if self.menus.iter().any(|m| m.id == id) {
            Some(id.to_string())
        } else {
            return false;
        };
        if next == self.active_menu {
            return false;
        }
        self.active_menu = next;
        true
    }
}

impl ChromeSlot {
    /// JSON for `shell.app-window`. Composes chrome data with the
    /// workspace's tab list — the secondary-arg pattern from §19.
    /// The skeleton's structural attrs (`id`, `panel-id`) win over
    /// emissions, so this method emits only data attrs.
    pub fn app_window_props(&self, workspace: &WorkspaceSlot) -> Value {
        json!({
            "app-name": self.app_name,
            "status": self.status,
            "menus": self.menus_json(),
            "active-menu": self.active_menu_index(),
            "tabs": workspace.tabs_json(),
            "nav-buttons": self.nav_buttons_json(),
        })
    }

    /// JSON for `shell.menu-bar-row` — top chrome with menu pills,
    /// app name, and the workflow page tabs. Same secondary-arg
    /// composition as `app_window_props` (chrome owns the row, tabs
    /// flow in by reference).
    pub fn menu_bar_row_props(&self, workspace: &WorkspaceSlot) -> Value {
        json!({
            "app-name": self.app_name,
            "menus": self.menus_json(),
            "active-menu": self.active_menu_index(),
            "tabs": workspace.tabs_json(),
        })
    }

    /// JSON for `shell.status-bar`. §43 D5: emits a multi-segment
    /// strip — `status / active-page / selection / node-count /
    /// app-name` — so the renderer can paint the DaVinci-style pipe-
    /// separated footer rather than a single label. Same secondary-
    /// arg pattern as [`Self::app_window_props`]: the chrome slot
    /// owns the row, the workspace + canvas slots flow in by
    /// reference (the cross-slot composition discipline from §19).
    ///
    /// `status` is the only string this slot itself owns; every
    /// other segment is derived from another slot at call time so a
    /// page switch / selection change / document edit shows up on
    /// the next frame without any per-segment plumbing.
    pub fn status_bar_props(&self, workspace: &WorkspaceSlot, canvas: &CanvasSlot) -> Value {
        let active_page_label = workspace.workspace.active_page().label.clone();
        let selection_label = canvas.selection_label();
        let node_count = canvas.node_count();
        let count_word = if node_count == 1 { "node" } else { "nodes" };
        let node_count_segment = format!("{node_count} {count_word}");
        json!({
            "status": self.status,
            "segments": Value::Array(vec![
                json!(self.status),
                json!(active_page_label),
                json!(selection_label),
                json!(node_count_segment),
                json!(self.app_name),
            ]),
        })
    }

    fn menus_json(&self) -> Value {
        Value::Array(
            self.menus
                .iter()
                .map(|m| json!({ "id": m.id, "label": m.label }))
                .collect(),
        )
    }

    fn nav_buttons_json(&self) -> Value {
        Value::Array(
            self.nav_buttons
                .iter()
                .map(|b| {
                    json!({
                        "nav-id": b.id,
                        "icon": b.icon,
                        "selected": b.selected,
                    })
                })
                .collect(),
        )
    }

    /// `active-menu` resolved to the matching index for the
    /// `shell.menu-bar-row` lowering, which currently reads index
    /// rather than id. Returns -1 when no menu is open (the same
    /// sentinel the block already understands).
    fn active_menu_index(&self) -> i64 {
        match self.active_menu.as_deref() {
            None => -1,
            Some(active) => self
                .menus
                .iter()
                .position(|m| m.id == active)
                .map(|i| i as i64)
                .unwrap_or(-1),
        }
    }
}

// ── workspace ─────────────────────────────────────────────────────

/// Workflow-page state — the seven DaVinci-style pages and the
/// active index. Wraps `prism_dock::DockWorkspace` directly so dock
/// ports (panels, dividers, tab bars) all read from the same source.
///
/// Two bindings consume this slot today:
/// `shell.workflow-page-bar` (the bottom mode bar) and
/// `shell.menu-bar-row` (top tabs). Both go through `tabs_json` /
/// `workflow_page_bar_props` — never inline JSON construction.
#[derive(Clone, Debug)]
pub struct WorkspaceSlot {
    pub workspace: DockWorkspace,
    /// Currently-loaded app id. Drives `shell.app-card`'s
    /// "active" tint on the Launchpad and is the seam through which
    /// per-app skeleton swaps will eventually plug in (D4 follow-up
    /// in `ui-migration-followups.md`).
    pub active_app: Option<String>,
}

impl Default for WorkspaceSlot {
    fn default() -> Self {
        Self {
            workspace: DockWorkspace::with_builtins(),
            active_app: None,
        }
    }
}

impl WorkspaceSlot {
    /// Switch the active app to `id`. Returns `true` when the cursor
    /// actually moved (so the click handler can flag the frame
    /// dirty). Passing the already-active id is a no-op.
    pub fn set_active_app(&mut self, id: &str) -> bool {
        if self.active_app.as_deref() == Some(id) {
            return false;
        }
        self.active_app = Some(id.to_string());
        true
    }

    /// JSON for `shell.workflow-page-bar`: one row per page with the
    /// active flag pre-resolved.
    pub fn workflow_page_bar_props(&self) -> Value {
        json!({ "pages": self.pages_json() })
    }

    /// JSON for `shell.dock-workspace`: the active page's `DockNode`
    /// tree, serialised through serde so the block can recurse over
    /// it without depending on `prism-dock` types in its props bag.
    /// One emission, one source of truth — switching the active
    /// workflow page (or customising the dock layout) automatically
    /// flows through this method on the next frame.
    pub fn dock_workspace_props(&self) -> Value {
        json!({ "dock": self.workspace.active_dock().root })
    }

    /// Tab list shape consumed by both `shell.menu-bar-row` and
    /// `shell.app-window` (top-bar tabs). Crate-public so the chrome
    /// slot's composition methods can borrow it without duplicating
    /// the JSON shape.
    pub(crate) fn tabs_json(&self) -> Value {
        let active = self.workspace.active_index();
        Value::Array(
            self.workspace
                .pages()
                .iter()
                .enumerate()
                .map(|(i, p)| {
                    json!({
                        "tab-id": p.id,
                        "label": p.label,
                        "active": i == active,
                    })
                })
                .collect(),
        )
    }

    fn pages_json(&self) -> Value {
        let active = self.workspace.active_index();
        Value::Array(
            self.workspace
                .pages()
                .iter()
                .enumerate()
                .map(|(i, p)| {
                    json!({
                        "page-id": p.id,
                        "label": p.label,
                        "icon-hint": p.icon_hint,
                        "active": i == active,
                    })
                })
                .collect(),
        )
    }
}

// ── overlay ───────────────────────────────────────────────────────

/// Floating chrome — toasts, the command palette, and the help
/// tooltip. None of these own a dock panel; they paint on top of
/// the app-window via the parsed skeleton's overlay siblings.
///
/// Three bindings consume this slot:
/// `shell.toast-stack`, `shell.command-palette`, `shell.help-tooltip`.
/// Each method emits the JSON shape its block already speaks (see
/// `components/{toast,command_palette,help_tooltip}.rs`).
#[derive(Clone, Debug, Default)]
pub struct OverlaySlot {
    pub toasts: Vec<Toast>,
    pub command_palette: CommandPalette,
    pub help_tooltip: Option<HelpTooltip>,
    /// Wave 1.6 of `docs/dev/composable-builder-plan.md` — modifier
    /// picker overlay. `open = true` after the inspector's "+ Add
    /// Behaviour" footer is clicked; selecting a behaviour or pressing
    /// Esc closes it.
    pub modifier_picker: ModifierPicker,
    /// Wave 4.3 of `docs/dev/composable-builder-plan.md` — connection
    /// picker overlay. `open = true` after the signals panel's "+
    /// Add Connection" footer is clicked; the three form fields
    /// drive a new `SignalConnection` on confirm.
    pub connection_picker: ConnectionPicker,
}

/// Wave 1.6 — open/closed state of the `shell.modifier-picker`
/// overlay. The owning-node id seeds the attach mutator on selection;
/// `attached` filters the registry list so users can't double-attach.
#[derive(Clone, Debug, Default)]
pub struct ModifierPicker {
    pub open: bool,
    pub target_id: String,
    pub attached: Vec<String>,
}

/// Wave 4.3 — open/closed state of the `shell.connection-picker`
/// overlay plus the three form fields the user fills in to build
/// a new `SignalConnection`. Default state is "closed with empty
/// fields"; `open_connection_picker` flips `open` true with sensible
/// defaults so the user starts on a usable shape.
#[derive(Clone, Debug, Default)]
pub struct ConnectionPicker {
    pub open: bool,
    pub source_signal: String,
    pub action_kind: String,
    pub target_label: String,
}

impl ConnectionPicker {
    /// Wave 4.3 — ordered list of `ActionKind` variant labels the
    /// picker cycles through when the user clicks the action-kind
    /// row. The order mirrors `prism_builder::signal::ActionKind`'s
    /// declaration so adding a variant there is the only edit a
    /// future grammar change needs.
    pub const ACTION_KINDS: &'static [&'static str] = &[
        "SetProperty",
        "ToggleVisibility",
        "NavigateTo",
        "PlayAnimation",
        "EmitSignal",
        "Custom",
        "Bind",
    ];

    /// Next variant after `current` in `ACTION_KINDS`, wrapping at
    /// the end. Used by the picker's click-to-cycle row.
    pub fn cycle_action_kind(current: &str) -> &'static str {
        let len = Self::ACTION_KINDS.len();
        let idx = Self::ACTION_KINDS
            .iter()
            .position(|k| *k == current)
            .map(|i| (i + 1) % len)
            .unwrap_or(0);
        Self::ACTION_KINDS[idx]
    }
}

#[derive(Clone, Debug)]
pub struct Toast {
    pub title: String,
    pub body: String,
    pub kind: ToastKind,
}

#[derive(Clone, Copy, Debug, Default)]
pub enum ToastKind {
    #[default]
    Info,
    Success,
    Warning,
    Error,
}

impl ToastKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Success => "success",
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct CommandPalette {
    pub open: bool,
    pub query: String,
    pub results: Vec<CommandResult>,
    pub selected_index: usize,
}

#[derive(Clone, Debug)]
pub struct CommandResult {
    pub id: String,
    pub label: String,
    pub shortcut: Option<String>,
}

#[derive(Clone, Debug)]
pub struct HelpTooltip {
    pub title: String,
    pub summary: String,
}

impl OverlaySlot {
    /// JSON for `shell.toast-stack`. Empty list is valid — the block
    /// renders the empty container.
    pub fn toast_stack_props(&self) -> Value {
        json!({ "toasts": self.toasts_json() })
    }

    /// JSON for `shell.command-palette`. The block reads `query`,
    /// `results`, and `selected-index`; visibility is gated by `open`
    /// (skeleton-side `visible="…"` author attr binds against it).
    pub fn command_palette_props(&self) -> Value {
        json!({
            "open": self.command_palette.open,
            "query": self.command_palette.query,
            "results": self.results_json(),
            "selected-index": self.command_palette.selected_index,
        })
    }

    /// JSON for `shell.help-tooltip`. When no tooltip is showing, all
    /// fields are empty strings — the block paints nothing.
    pub fn help_tooltip_props(&self) -> Value {
        match &self.help_tooltip {
            Some(t) => json!({ "title": t.title, "summary": t.summary, "visible": true }),
            None => json!({ "title": "", "summary": "", "visible": false }),
        }
    }

    /// JSON for `shell.modifier-picker`. Wave 1.6 of
    /// `docs/dev/composable-builder-plan.md`. Pulls the registered
    /// behaviours from the shared `ModifierRegistry`, filters out the
    /// already-attached ids (the `add-modifier-open` route stashed
    /// them on `self.modifier_picker.attached`), and emits the
    /// picker's open/target-id state.
    pub fn modifier_picker_props(
        &self,
        registry: Option<&prism_builder::ModifierRegistry>,
    ) -> Value {
        let options = match (self.modifier_picker.open, registry) {
            (true, Some(reg)) => {
                let attached: std::collections::HashSet<&str> = self
                    .modifier_picker
                    .attached
                    .iter()
                    .map(String::as_str)
                    .collect();
                let entries: Vec<Value> = reg
                    .list()
                    .into_iter()
                    .filter(|d| !attached.contains(d.id.as_str()))
                    .map(|d| {
                        json!({
                            "id": d.id,
                            "label": d.label,
                            "description": d.description,
                        })
                    })
                    .collect();
                Value::Array(entries)
            }
            _ => Value::Array(Vec::new()),
        };
        json!({
            "open": self.modifier_picker.open,
            "target-id": self.modifier_picker.target_id,
            "options": options,
        })
    }

    /// Wave 4.3 — JSON for `shell.connection-picker`. The block
    /// reads `open` to gate visibility and the three form fields
    /// for the row labels; the routing attrs the user clicks come
    /// from the block itself, not the prop bag.
    pub fn connection_picker_props(&self) -> Value {
        json!({
            "open": self.connection_picker.open,
            "source-signal": self.connection_picker.source_signal,
            "action-kind": self.connection_picker.action_kind,
            "target-label": self.connection_picker.target_label,
        })
    }

    fn toasts_json(&self) -> Value {
        Value::Array(
            self.toasts
                .iter()
                .map(|t| {
                    json!({
                        "title": t.title,
                        "body": t.body,
                        "kind": t.kind.as_str(),
                    })
                })
                .collect(),
        )
    }

    fn results_json(&self) -> Value {
        Value::Array(
            self.command_palette
                .results
                .iter()
                .map(|r| {
                    let mut o = json!({ "id": r.id, "label": r.label });
                    if let Some(sc) = &r.shortcut {
                        o["shortcut"] = json!(sc);
                    }
                    o
                })
                .collect(),
        )
    }

    /// Fuzzy-filter a list of `(id, label)` command rows against the
    /// palette's current query. Returns the indices of `rows` that
    /// match, ordered best-score first. (§25 — single matcher, single
    /// caller. No service rebuilds the command list; no service
    /// reimplements the matcher.)
    pub fn filter_commands(&self, rows: &[(&str, &str)]) -> Vec<usize> {
        let q = self.command_palette.query.to_lowercase();
        if q.is_empty() {
            return (0..rows.len()).collect();
        }
        let mut scored: Vec<(usize, i32)> = rows
            .iter()
            .enumerate()
            .filter_map(|(i, (id, label))| {
                let id_lc = id.to_lowercase();
                let lab_lc = label.to_lowercase();
                let score = if lab_lc.contains(&q) {
                    100 - lab_lc.find(&q).unwrap_or(0) as i32
                } else if id_lc.contains(&q) {
                    50 - id_lc.find(&q).unwrap_or(0) as i32
                } else {
                    return None;
                };
                Some((i, score))
            })
            .collect();
        scored.sort_by(|a, b| b.1.cmp(&a.1));
        scored.into_iter().map(|(i, _)| i).collect()
    }
}

// ── builder ───────────────────────────────────────────────────────

/// Inspector / properties / signals / schema — the four panels that
/// describe the *currently selected* document node. Each panel is a
/// distinct binding, but every shape ultimately reads from the same
/// `selection` cursor + the document tree, so they all live on one
/// slot. Cross-binding consistency (selecting a node updates all four
/// panels) is enforced by the slot owning the resolution code path
/// once.
///
/// Four bindings consume this slot:
/// `shell.inspector-tree`, `shell.properties-panel`,
/// `shell.signals-panel`, `shell.schema-designer`. Per-row blocks
/// (`shell.signal-connection-row`, `shell.schema-row`,
/// `shell.inspector-row`, `shell.field-editor`) stay stubs — their
/// data flows down inside the parent's `rows` / `connections` /
/// `fields` JSON arrays, never through their own binding row.
///
/// `selected_connection` / `schema.selected_field` are the chevron-row
/// cursors that drive the trash affordances on
/// `shell.signal-connection-row` / `shell.schema-row`. They sit
/// disjoint from any per-row state because the rows themselves are
/// projected from `signal_connections` / `schema.fields` every frame;
/// the cursor lives on the owning slot so a stateless `cmd <id>` body
/// has a target.
#[derive(Clone, Debug, Default)]
pub struct BuilderSlot {
    pub inspector: Vec<InspectorNode>,
    pub property_rows: Vec<PropertyRow>,
    pub signal_connections: Vec<SignalConnection>,
    pub selected_connection: Option<String>,
    pub schema: SchemaDoc,
}

#[derive(Clone, Debug)]
pub struct InspectorNode {
    pub id: String,
    pub label: String,
    pub depth: u32,
    pub selected: bool,
}

/// One row in the properties panel. The block already speaks a
/// generic `{component, props}` shape (see `properties_panel.rs`'s
/// example), so the slot stores it as typed pairs and the JSON
/// emitter folds them.
#[derive(Clone, Debug)]
pub struct PropertyRow {
    pub component: String,
    pub props: Value,
}

/// One row in the signals panel. The `id` field is the cursor key the
/// signals trash button targets via
/// `signals.delete-selected-connection`; mirror the
/// `prism_builder::Connection::id` when projecting from the canvas
/// document.
#[derive(Clone, Debug)]
pub struct SignalConnection {
    pub id: String,
    pub source_signal: String,
    pub action_kind: String,
    pub target_label: String,
}

impl CursorKey for SignalConnection {
    fn cursor_key(&self) -> &str {
        &self.id
    }
}

/// `selected_field` is the cursor the schema-designer trash button
/// targets via `schema.delete-selected-field`; it stores the field
/// `name` because `SchemaField` has no separate id and the name is
/// the registry-facing identifier.
#[derive(Clone, Debug, Default)]
pub struct SchemaDoc {
    pub title: String,
    pub schema_name: String,
    pub fields: Vec<SchemaField>,
    pub selected_field: Option<String>,
}

#[derive(Clone, Debug)]
pub struct SchemaField {
    pub name: String,
    pub kind: String,
    pub required: bool,
}

impl CursorKey for SchemaField {
    fn cursor_key(&self) -> &str {
        &self.name
    }
}

impl BuilderSlot {
    pub fn inspector_tree_props(&self) -> Value {
        json!({ "nodes": self.inspector_json() })
    }

    pub fn properties_panel_props(&self) -> Value {
        self.properties_panel_props_with(None)
    }

    /// Same shape as [`Self::properties_panel_props`] but stamps
    /// `focused: true` onto whichever row matches `focus.target_id +
    /// focus.key`. The properties-panel binding passes the current
    /// `state.field_focus` here so the field-editor lowering can paint
    /// an outline / cursor without re-deriving rows on every keystroke.
    pub fn properties_panel_props_with(&self, focus: Option<&FieldFocus>) -> Value {
        json!({ "rows": self.property_rows_json_with(focus) })
    }

    pub fn signals_panel_props(&self) -> Value {
        json!({
            "title": "Signals",
            "connections": self.connections_json(),
        })
    }

    pub fn schema_designer_props(&self) -> Value {
        json!({
            "title": self.schema.title,
            "schema-name": self.schema.schema_name,
            "fields": self.schema_fields_json(),
        })
    }

    /// Move the chevron cursor (`selected_connection`) onto `id`.
    /// Returns `true` when the cursor actually moved. Mirrors the
    /// `NavigationSlot::select_row` shape — all three cursor pairs in
    /// `state.rs` delegate to [`select_cursor_row`] / [`delete_cursor_row`]
    /// over their respective `(items, cursor)` pair.
    pub fn select_signal_connection(&mut self, id: &str) -> bool {
        select_cursor_row(&self.signal_connections, &mut self.selected_connection, id)
    }

    /// Remove the currently-cursored connection from the slot mirror
    /// and clear the cursor. Returns `true` when a row actually went
    /// away. The canvas document's `Connection` list is the source of
    /// truth; the resync pass that follows a structural mutation is
    /// expected to rebuild the slot mirror — for now the slot edit is
    /// the visible effect (the panel re-paints without the row).
    pub fn delete_selected_signal_connection(&mut self) -> bool {
        delete_cursor_row(&mut self.signal_connections, &mut self.selected_connection)
    }

    /// Move the schema-designer chevron cursor (`schema.selected_field`)
    /// onto `name`. Returns `true` when the cursor moved.
    pub fn select_schema_field(&mut self, name: &str) -> bool {
        select_cursor_row(&self.schema.fields, &mut self.schema.selected_field, name)
    }

    /// Remove the currently-cursored schema field. Clears the cursor.
    pub fn delete_selected_schema_field(&mut self) -> bool {
        delete_cursor_row(&mut self.schema.fields, &mut self.schema.selected_field)
    }

    fn inspector_json(&self) -> Value {
        Value::Array(
            self.inspector
                .iter()
                .map(|n| {
                    json!({
                        "node-id": n.id,
                        "label": n.label,
                        "depth": n.depth,
                        "selected": n.selected,
                    })
                })
                .collect(),
        )
    }

    /// JSON shape for the properties-panel rows with optional focus
    /// folding. When `focus` matches a row's `target-id + key`, the
    /// emitted props carry `"focused": true`; rows without focus are
    /// untouched. The field-editor block reads the flag and paints an
    /// outline.
    fn property_rows_json_with(&self, focus: Option<&FieldFocus>) -> Value {
        Value::Array(
            self.property_rows
                .iter()
                .map(|r| {
                    let mut props = r.props.clone();
                    if let Some(f) = focus {
                        let target_matches = props
                            .get("target-id")
                            .and_then(|v| v.as_str())
                            .map(|s| s == f.target_id)
                            .unwrap_or(false);
                        let key_matches = props
                            .get("key")
                            .and_then(|v| v.as_str())
                            .map(|s| s == f.key)
                            .unwrap_or(false);
                        if target_matches && key_matches {
                            if let Value::Object(map) = &mut props {
                                map.insert("focused".into(), Value::Bool(true));
                            }
                        }
                    }
                    json!({ "component": r.component, "props": props })
                })
                .collect(),
        )
    }

    fn connections_json(&self) -> Value {
        Value::Array(
            iter_with_cursor(
                &self.signal_connections,
                self.selected_connection.as_deref(),
            )
            .map(|(c, is_selected)| {
                json!({
                    "connection-id": c.id,
                    "source-signal": c.source_signal,
                    "action-kind": c.action_kind,
                    "target-label": c.target_label,
                    "selected": is_selected,
                    "show-delete": is_selected,
                })
            })
            .collect(),
        )
    }

    fn schema_fields_json(&self) -> Value {
        Value::Array(
            iter_with_cursor(&self.schema.fields, self.schema.selected_field.as_deref())
                .map(|(f, is_selected)| {
                    json!({
                        // `field-id` doubles as the cursor key — no
                        // separate id exists on `SchemaField`, and the
                        // name is unique within a schema.
                        "field-id": f.name,
                        "field-name": f.name,
                        "field-kind": f.kind,
                        "required": f.required,
                        "selected": is_selected,
                        "show-delete": is_selected,
                    })
                })
                .collect(),
        )
    }
}

// ── navigation ────────────────────────────────────────────────────

/// Page list + graph — the two lenses on the multi-page authoring
/// model. `pages_json` is the load-bearing helper consumed by both
/// bindings; the graph adds positions and cross-page edges on top.
///
/// Two bindings consume this slot:
/// `shell.nav-page-list`, `shell.nav-graph`. The per-row block
/// (`shell.nav-page-row`) stays a stub — page rows render inside the
/// parent list's `pages` array.
///
/// `selected_page` is the cursor the inspector-style chevron / trash
/// buttons on `shell.nav-page-row` target — disjoint from
/// `NavPage::is_active`, which tracks "currently open" rather than
/// "selected in the page list."
#[derive(Clone, Debug, Default)]
pub struct NavigationSlot {
    pub pages: Vec<NavPage>,
    pub edges: Vec<NavEdge>,
    pub selected_page: Option<String>,
}

#[derive(Clone, Debug)]
pub struct NavPage {
    pub id: String,
    pub title: String,
    pub route: String,
    pub x: f32,
    pub y: f32,
    pub node_count: u32,
    pub link_count: u32,
    pub is_active: bool,
}

impl CursorKey for NavPage {
    fn cursor_key(&self) -> &str {
        &self.id
    }
}

#[derive(Clone, Debug)]
pub struct NavEdge {
    pub from: usize,
    pub to: usize,
    pub kind: NavEdgeKind,
}

#[derive(Clone, Copy, Debug)]
pub enum NavEdgeKind {
    Href,
    Signal,
}

impl NavEdgeKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Href => "href",
            Self::Signal => "signal",
        }
    }
}

impl NavigationSlot {
    /// JSON for `shell.nav-page-list`. The block reads only the page
    /// list — graph positions and edges are dropped here.
    pub fn nav_page_list_props(&self) -> Value {
        json!({ "pages": self.pages_list_json() })
    }

    /// Append a fresh untitled page and mark it as active. Used by
    /// the menu-bar "+page" button via `navigation.add-page`. The
    /// new id is `page-<n>` where `n` is the smallest natural number
    /// that doesn't collide with an existing page.
    pub fn add_page(&mut self) -> &NavPage {
        let mut n = self.pages.len() + 1;
        let id = loop {
            let candidate = format!("page-{n}");
            if !self.pages.iter().any(|p| p.id == candidate) {
                break candidate;
            }
            n += 1;
        };
        let title = format!("Page {n}");
        let route = format!("/{}", id);
        for p in &mut self.pages {
            p.is_active = false;
        }
        self.pages.push(NavPage {
            id,
            title,
            route,
            x: 0.0,
            y: 0.0,
            node_count: 0,
            link_count: 0,
            is_active: true,
        });
        self.pages.last().expect("just pushed")
    }

    /// Move the chevron-cursor (`selected_page`) onto `id`. The cursor
    /// is disjoint from `is_active` — selecting a row in the page list
    /// surfaces the move-up / move-down / delete affordances without
    /// switching the open page. Returns `true` when the cursor
    /// actually moved. Delegates to [`select_cursor_row`], the shared
    /// helper that drives every cursor-keyed row in the shell.
    pub fn select_row(&mut self, id: &str) -> bool {
        select_cursor_row(&self.pages, &mut self.selected_page, id)
    }

    /// Swap the currently-selected page with its `dir`-neighbour
    /// (-1 = previous, +1 = next). Used by the nav-page-row chevrons
    /// via `navigation.move-page-{up,down}`. The cursor follows the
    /// page so the chevron stays under the user's pointer.
    pub fn reorder_selected(&mut self, dir: i32) -> bool {
        if dir == 0 {
            return false;
        }
        let Some(id) = self.selected_page.clone() else {
            return false;
        };
        let Some(idx) = self.pages.iter().position(|p| p.id == id) else {
            return false;
        };
        let new_idx = idx as i32 + dir;
        if new_idx < 0 || new_idx as usize >= self.pages.len() {
            return false;
        }
        self.pages.swap(idx, new_idx as usize);
        true
    }

    /// Remove the currently-selected page. Clears the cursor and, if
    /// the page was the active one, transfers `is_active` onto the
    /// next surviving page (or none when the list empties).
    pub fn delete_selected(&mut self) -> bool {
        // Peek the row before delegating so we can detect "was the
        // dropped page the active one" — the shared helper already
        // does the cursor + remove dance, but the active-promotion is
        // navigation-specific bookkeeping.
        let was_active = self
            .selected_page
            .as_deref()
            .and_then(|id| self.pages.iter().find(|p| p.id == id))
            .map(|p| p.is_active)
            .unwrap_or(false);
        let Some(idx) = pop_cursor_row(&mut self.pages, &mut self.selected_page) else {
            return false;
        };
        if was_active && !self.pages.is_empty() {
            // Promote the next-best page to active so the workspace
            // doesn't end up in a "no active page" state.
            let promote = idx.min(self.pages.len() - 1);
            self.pages[promote].is_active = true;
        }
        true
    }

    /// Mark `id` as the active nav page (clears `is_active` on every
    /// other entry). Returns `true` when the active page actually
    /// moved, so the event router can flag the frame as dirty. Pages
    /// without a matching id leave the slot unchanged.
    pub fn select_page_by_id(&mut self, id: &str) -> bool {
        let Some(target_idx) = self.pages.iter().position(|p| p.id == id) else {
            return false;
        };
        if self.pages[target_idx].is_active {
            return false;
        }
        for (i, p) in self.pages.iter_mut().enumerate() {
            p.is_active = i == target_idx;
        }
        true
    }

    /// JSON for `shell.nav-graph`. Composes the same page list with
    /// graph positions + edges. The shared subset (page-title, route,
    /// is-active) flows through the same `iter().map()` shape — no
    /// duplicated emitter.
    pub fn nav_graph_props(&self) -> Value {
        json!({
            "title": "Pages",
            "pages": self.pages_graph_json(),
            "edges": self.edges_json(),
        })
    }

    fn pages_list_json(&self) -> Value {
        Value::Array(
            iter_with_cursor(&self.pages, self.selected_page.as_deref())
                .map(|(p, is_selected)| {
                    json!({
                        "page-id": p.id,
                        "page-title": p.title,
                        "route": p.route,
                        "node-count": p.node_count,
                        "link-count": p.link_count,
                        "is-active": p.is_active,
                        "selected": is_selected,
                        "show-delete": is_selected,
                    })
                })
                .collect(),
        )
    }

    fn pages_graph_json(&self) -> Value {
        Value::Array(
            self.pages
                .iter()
                .map(|p| {
                    json!({
                        "label": p.title,
                        "route": p.route,
                        "x": p.x,
                        "y": p.y,
                        "is-active": p.is_active,
                    })
                })
                .collect(),
        )
    }

    fn edges_json(&self) -> Value {
        Value::Array(
            self.edges
                .iter()
                .map(|e| {
                    json!({
                        "from": e.from,
                        "to": e.to,
                        "kind": e.kind.as_str(),
                    })
                })
                .collect(),
        )
    }
}

// ── catalog ───────────────────────────────────────────────────────

/// Browse-style panels — launchpad apps, explorer files, component
/// palette items. Three bindings, three disjoint shapes; no shared
/// item type because the underlying data is genuinely different
/// (app cards vs file rows vs draggable component descriptors). Each
/// emitter is a plain `iter().map()` fold (rule-of-three: zero
/// consumers in common, no helper).
///
/// Three bindings consume this slot:
/// `shell.launchpad`, `shell.explorer`, `shell.component-palette`.
/// The per-row block `shell.app-card` stays a stub — its data flows
/// down inside the launchpad's `apps` array.
#[derive(Clone, Debug, Default)]
pub struct CatalogSlot {
    pub launchpad_title: String,
    pub apps: Vec<AppCard>,
    pub files: Vec<FileNode>,
    pub palette: Vec<PaletteItem>,
    pub palette_selected: Option<String>,
    pub palette_drag: Option<PaletteDrag>,
}

#[derive(Clone, Debug)]
pub struct AppCard {
    pub id: String,
    pub label: String,
    pub icon: String,
    pub summary: String,
}

#[derive(Clone, Debug)]
pub struct FileNode {
    pub id: String,
    pub label: String,
    pub depth: u32,
    pub kind: FileKind,
}

#[derive(Clone, Copy, Debug)]
pub enum FileKind {
    Directory,
    File,
}

impl FileKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Directory => "directory",
            Self::File => "file",
        }
    }
}

#[derive(Clone, Debug)]
pub struct PaletteItem {
    pub id: String,
    pub label: String,
    pub icon: String,
    pub category: String,
}

/// Wave 3.2 of `docs/dev/composable-builder-plan.md` — in-flight
/// palette-driven drop. Set by `begin_palette_drag` when the user
/// pointer-downs on the canvas with a palette item armed; updated
/// per pointer-move with the cursor position and the canvas node
/// under it (if any); consumed by `end_palette_drag` on release,
/// which inserts a fresh node of `kind` under `drop_target` (or
/// under the document root when None).
#[derive(Clone, Debug)]
pub struct PaletteDrag {
    pub kind: String,
    pub pointer: (f32, f32),
    pub drop_target: Option<NodeId>,
}

impl CatalogSlot {
    /// JSON for `shell.launchpad`. The block reads `title` and an
    /// `apps` array of `AppCard`-shaped objects.
    pub fn launchpad_props(&self) -> Value {
        json!({
            "title": self.launchpad_title,
            "apps": self.apps_json(),
        })
    }

    /// JSON for `shell.explorer`. The block reads a `nodes` array of
    /// row props (label / depth / kind) — flat list, the tree shape
    /// is encoded in `depth`.
    pub fn explorer_props(&self) -> Value {
        json!({ "nodes": self.files_json() })
    }

    /// JSON for `shell.component-palette`. The block reads `items` and
    /// the optional `selected-id`. Selection flows here from the
    /// builder canvas's "place mode" (set when the user picks an item
    /// to place into a grid cell).
    pub fn component_palette_props(&self) -> Value {
        let mut props = json!({ "items": self.palette_json() });
        if let Some(id) = &self.palette_selected {
            props["selected-id"] = json!(id);
        }
        props
    }

    fn apps_json(&self) -> Value {
        Value::Array(
            self.apps
                .iter()
                .map(|a| {
                    json!({
                        "app-id": a.id,
                        "label": a.label,
                        "icon": a.icon,
                        "summary": a.summary,
                    })
                })
                .collect(),
        )
    }

    fn files_json(&self) -> Value {
        Value::Array(
            self.files
                .iter()
                .map(|f| {
                    json!({
                        "node-id": f.id,
                        "label": f.label,
                        "depth": f.depth,
                        "kind": f.kind.as_str(),
                    })
                })
                .collect(),
        )
    }

    fn palette_json(&self) -> Value {
        Value::Array(
            self.palette
                .iter()
                .map(|p| {
                    json!({
                        "item-id": p.id,
                        "label": p.label,
                        "icon": p.icon,
                        "category": p.category,
                    })
                })
                .collect(),
        )
    }
}

// ── docs ──────────────────────────────────────────────────────────

/// Documentation view + sidebar — both bindings render the *same*
/// `DocsTopic` (title / summary / body) and only differ in `mode`
/// (the sidebar embeds extra chrome). The shared shape is extracted
/// into `topic_props` honestly: rule-of-three threshold met at two
/// consumers with byte-identical JSON keys.
///
/// Two bindings consume this slot:
/// `shell.docs-view`, `shell.docs-sidebar`. The per-row block
/// `shell.docs-content` stays a stub — its props are routed through
/// the parent's container by the existing `lower_ui` (each docs
/// component already builds its own content child).
#[derive(Clone, Debug, Default)]
pub struct DocsSlot {
    pub topic: DocsTopic,
    pub sidebar_mode: String,
}

#[derive(Clone, Debug, Default)]
pub struct DocsTopic {
    pub title: String,
    pub summary: String,
    pub body: String,
}

impl DocsSlot {
    /// JSON for `shell.docs-view`. Full-page topic render — `mode`
    /// is fixed `"full"` because the view block hard-codes that
    /// dispatch in its `lower_ui`.
    pub fn docs_view_props(&self) -> Value {
        let mut props = self.topic_props();
        props["mode"] = json!("full");
        props
    }

    /// JSON for `shell.docs-sidebar`. Same topic shape with the
    /// sidebar-specific `mode` (e.g. `"summary"` / `"outline"`).
    /// Defaults to `"sidebar"` when empty.
    pub fn docs_sidebar_props(&self) -> Value {
        let mode = if self.sidebar_mode.is_empty() {
            "sidebar"
        } else {
            self.sidebar_mode.as_str()
        };
        let mut props = self.topic_props();
        props["mode"] = json!(mode);
        props
    }

    /// Shared shape — title / summary / body. Lives on the slot so
    /// neither binding closure invents its own keys (the §19
    /// no-duplication rule applied to a private helper).
    fn topic_props(&self) -> Value {
        json!({
            "title": self.topic.title,
            "summary": self.topic.summary,
            "body": self.topic.body,
        })
    }
}

// ── menus ─────────────────────────────────────────────────────────

/// Menu items for the open dropdown and the active context menu.
/// Both bindings consume the same `MenuItem` array shape, so the
/// JSON emitter is a single private helper (`items_json`) — exactly
/// the shared-shape extraction the rule-of-three justifies (two
/// consumers, identical keys, no plausible per-binding deviation).
///
/// Two bindings consume this slot:
/// `shell.menu-dropdown`, `shell.context-menu`. The per-row block
/// `shell.menu-item` stays a stub — items render inside the parent
/// menu's `items` array.
#[derive(Clone, Debug, Default)]
pub struct MenuSlot {
    pub dropdown: Vec<MenuItem>,
    pub context: Vec<MenuItem>,
}

#[derive(Clone, Debug)]
pub struct MenuItem {
    pub label: String,
    pub shortcut: Option<String>,
    pub command: Option<String>,
    pub separator: bool,
    pub enabled: bool,
}

impl MenuItem {
    pub fn separator() -> Self {
        Self {
            label: String::new(),
            shortcut: None,
            command: None,
            separator: true,
            enabled: false,
        }
    }
}

impl MenuSlot {
    pub fn menu_dropdown_props(&self) -> Value {
        json!({ "items": Self::items_json(&self.dropdown) })
    }

    pub fn context_menu_props(&self) -> Value {
        json!({ "items": Self::items_json(&self.context) })
    }

    /// Shared menu-item shape. Extracted on landing because both
    /// callers emit identical keys today *and* will continue to —
    /// the `MenuItem` struct is the sole vocabulary.
    fn items_json(items: &[MenuItem]) -> Value {
        Value::Array(
            items
                .iter()
                .map(|m| {
                    let mut o = json!({
                        "label": m.label,
                        "separator": m.separator,
                        "enabled": m.enabled,
                    });
                    if let Some(sc) = &m.shortcut {
                        o["shortcut"] = json!(sc);
                    }
                    if let Some(cmd) = &m.command {
                        o["command"] = json!(cmd);
                    }
                    o
                })
                .collect(),
        )
    }
}

// ── canvas ────────────────────────────────────────────────────────

/// The active document, the selection's resolved transform, the active
/// tool mode, and the small piece of capture state needed to translate
/// a pointer drag into a typed transform delta. Six bindings read from
/// this slot:
///
/// * `shell.code-editor` (source + caret + lang)
/// * `shell.builder-canvas` (doc + viewport + place-mode)
/// * `shell.gizmo-{move,rotate,scale}` (three lenses on one shape)
/// * `shell.resize-handle` (eight handles around selection bbox)
/// * `shell.component-picker` (palette popup)
///
/// `drag` is *private* — bindings cannot read it. Visibility flows
/// through data shape (the mutated `document` and `selection`'s
/// transform), not a "drag in progress" flag. Same discipline as
/// [`OverlaySlot::help_tooltip_props`]'s `visible` collapse.
///
/// See `docs/dev/clay-migration-plan.md` §22.
#[derive(Clone, Debug, Default)]
pub struct CanvasSlot {
    pub document: BuilderDocument,
    pub selection: Option<NodeId>,
    pub tool: ToolMode,
    pub viewport: CanvasViewport,
    pub picker: PickerState,
    pub code_buffer: CodeBuffer,
    /// §43 D2: active responsive preview mode. Drives the
    /// Desktop/Tablet/Mobile button cluster in `shell.builder-toolbar`
    /// and (eventually) constrains the canvas page width when the
    /// builder is in preview mode.
    pub device: Device,
    drag: Option<DragState>,
    /// **Phase 4b** of `docs/dev/dioxus-inspiration.md`: per-canvas
    /// reactive prop store. Threaded into `LowerCtx::with_bindings(..)`
    /// on every render walk so block `lower_ui` bodies that read props
    /// through `ctx.prop_*` subscribe automatically; threaded into
    /// `NodeMutator::with_bindings(..)` on every prop write so the
    /// matching subscribers wake. One field, two consumers, zero
    /// per-block plumbing.
    pub(crate) bindings: prism_builder::DocumentBindings,
    /// Wave 3.3 — bounding box of the currently-selected canvas node,
    /// captured from `Surface::hit_test_at` at the moment of click.
    /// Drives the `selection-rect` JSON prop on `shell.builder-canvas`,
    /// which paints the selection outline + 8-handle ring around the
    /// node. `None` when no canvas node is selected or the last
    /// click pre-dated the bbox capture (headless tests / non-pointer
    /// programmatic selection); the canvas still renders, just without
    /// the gizmo overlay.
    pub selection_bbox: Option<SelectionBbox>,
    /// Wave 3.3 — in-flight resize-handle drag against the current
    /// selection. The pointer-down on a `data-role="resize-handle"`
    /// hit captures direction + the node's pre-drag transform; each
    /// pointer-move applies a position delta along the handle's axes;
    /// pointer-up commits.
    pub(crate) resize_drag: Option<ResizeDragState>,
}

/// Wave 3.3 — pixel rect carried as the `selection-rect` prop on
/// `shell.builder-canvas`. Kept as a separate struct (rather than
/// reusing `prism_ui_runtime::command::Rect`) so the canvas
/// emission stays JSON-stable and the slot doesn't take a hard
/// dep on a transitive runtime type for what's essentially a
/// four-float carrier.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SelectionBbox {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// Wave 3.3 — pointer-driven resize session against a canvas node.
/// `direction` is the handle's `data-direction` (`tl|t|tr|r|br|b|bl|l`);
/// `origin` is the pointer-down position; `snapshot` captures the
/// node's `Transform2D` at session start so each pointer-move
/// applies a pristine delta (matches `apply_handle_delta`'s
/// snapshot discipline).
#[derive(Clone, Debug)]
pub(crate) struct ResizeDragState {
    pub direction: String,
    pub node_id: NodeId,
    pub origin: (f32, f32),
    pub snapshot: prism_core::foundation::spatial::Transform2D,
}

/// Responsive preview target for the builder canvas. Three discrete
/// sizes mirror the DaVinci-style toolbar's device cluster; future
/// "custom width" entries land as a new enum variant rather than a
/// free-form numeric prop.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Device {
    #[default]
    Desktop,
    Tablet,
    Mobile,
}

impl Device {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Desktop => "desktop",
            Self::Tablet => "tablet",
            Self::Mobile => "mobile",
        }
    }

    /// Reverse of [`Self::as_str`] — parses the kebab-case id the
    /// toolbar emits in `data-device`. Returns `None` for unknown
    /// strings rather than silently falling back to `Desktop`, so the
    /// caller can decide whether to ignore the click or surface an
    /// error.
    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "desktop" => Some(Self::Desktop),
            "tablet" => Some(Self::Tablet),
            "mobile" => Some(Self::Mobile),
            _ => None,
        }
    }
}

/// Canonical page-rect pixel dimensions per device preset. Drives the
/// `page-width` / `page-height` emission on `shell.builder-canvas`.
/// The toolbar's device cluster flips between these without touching
/// the document; `zoom` multiplies independently on top.
///
/// Sizes chosen so a Desktop preview fits inside a ~600px-wide builder
/// dock panel at zoom 1.0 without clipping (the previous default of
/// 1280×800 overflowed every realistic viewport at 1.0×).
pub(crate) fn device_page_dims(device: Device) -> (u32, u32) {
    match device {
        Device::Desktop => (960, 600),
        Device::Tablet => (768, 1024),
        Device::Mobile => (375, 667),
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ToolMode {
    #[default]
    Move,
    Rotate,
    Scale,
}

impl ToolMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Move => "move",
            Self::Rotate => "rotate",
            Self::Scale => "scale",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct CanvasViewport {
    pub width: f32,
    pub height: f32,
    pub zoom: f32,
    pub pan_x: f32,
    pub pan_y: f32,
}

impl Default for CanvasViewport {
    fn default() -> Self {
        Self {
            width: 1280.0,
            height: 800.0,
            zoom: 1.0,
            pan_x: 0.0,
            pan_y: 0.0,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct PickerState {
    pub open: bool,
    pub anchor_x: f32,
    pub anchor_y: f32,
    pub candidates: Vec<PickerCandidate>,
}

#[derive(Clone, Debug)]
pub struct PickerCandidate {
    pub id: String,
    pub label: String,
    pub icon: String,
}

#[derive(Clone, Debug, Default)]
pub struct CodeBuffer {
    pub source: String,
    pub language: String,
    pub caret: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DragKind {
    /// Captured a gizmo arm — the active tool mode determines what the
    /// delta means (`apply_gizmo_delta`).
    Gizmo,
    /// Captured one of eight resize handles around the selection bbox.
    Handle(HandleSide),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HandleSide {
    TopLeft,
    Top,
    TopRight,
    Right,
    BottomRight,
    Bottom,
    BottomLeft,
    Left,
}

impl HandleSide {
    fn cursor(self) -> &'static str {
        match self {
            Self::TopLeft | Self::BottomRight => "nwse-resize",
            Self::TopRight | Self::BottomLeft => "nesw-resize",
            Self::Top | Self::Bottom => "ns-resize",
            Self::Left | Self::Right => "ew-resize",
        }
    }

    fn id(self) -> &'static str {
        match self {
            Self::TopLeft => "tl",
            Self::Top => "t",
            Self::TopRight => "tr",
            Self::Right => "r",
            Self::BottomRight => "br",
            Self::Bottom => "b",
            Self::BottomLeft => "bl",
            Self::Left => "l",
        }
    }

    /// (dx, dy) sign applied to (x, y, w, h) when this handle drags
    /// by `(dx, dy)`. Returns `(dx_pos, dy_pos, dx_size, dy_size)`.
    fn deltas(self) -> (f32, f32, f32, f32) {
        match self {
            Self::TopLeft => (1.0, 1.0, -1.0, -1.0),
            Self::Top => (0.0, 1.0, 0.0, -1.0),
            Self::TopRight => (0.0, 1.0, 1.0, -1.0),
            Self::Right => (0.0, 0.0, 1.0, 0.0),
            Self::BottomRight => (0.0, 0.0, 1.0, 1.0),
            Self::Bottom => (0.0, 0.0, 0.0, 1.0),
            Self::BottomLeft => (1.0, 0.0, -1.0, 1.0),
            Self::Left => (1.0, 0.0, -1.0, 0.0),
        }
    }
}

/// Pre-drag values for the selected node. `commit_drag` reads this to
/// push exactly one undo snapshot per drag (rather than one per
/// `pointer_move` tick). Same shape as the pre-§22 `DragSnapshot` /
/// `ResizeSnapshot` halves of `app/`, ported *into* the slot rather
/// than duplicated next to it.
#[derive(Clone, Debug, PartialEq)]
pub struct TransformSnapshot {
    pub node_id: NodeId,
    pub transform: Transform2D,
}

impl TransformSnapshot {
    /// Capture the selected node's pre-drag transform. Returns `None`
    /// when there's no selection or the selection's id has gone stale.
    fn capture(doc: &BuilderDocument, selection: Option<&str>) -> Option<Self> {
        let id = selection?;
        let node = doc.root.as_ref()?.find(id)?;
        Some(Self {
            node_id: node.id.clone(),
            transform: node.transform.clone(),
        })
    }
}

#[derive(Clone, Debug)]
struct DragState {
    kind: DragKind,
    snapshot: TransformSnapshot,
    origin: (f32, f32),
}

impl CanvasSlot {
    // ── read side ─────────────────────────────────────────────────

    /// JSON for `shell.code-editor`. Source text + caret offset + the
    /// language the syntax provider speaks.
    pub fn code_editor_props(&self) -> Value {
        json!({
            "source": self.code_buffer.source,
            "caret": self.code_buffer.caret,
            "language": if self.code_buffer.language.is_empty() {
                "slint"
            } else {
                self.code_buffer.language.as_str()
            },
        })
    }

    /// JSON for `shell.builder-canvas`. The block reads the selection
    /// id (so it can paint the selection rectangle), the canvas
    /// viewport, and the picker's place-mode flag. The full document
    /// tree is *not* serialised here — the binding lowers it through
    /// `lower_document_to_ui` and threads the result into the canvas
    /// via the `host_children_by_tag` injection seam (§43 B2). Keeping
    /// this method JSON-only stays compatible with every other binding
    /// shape (props only) without inventing a parallel emission type.
    pub fn builder_canvas_props(&self) -> Value {
        // `page-width` / `page-height` drive the canvas-page rect inside
        // the canvas frame. The block defaults to 1280x800 — way too
        // big for the panel column it lives in (the Edit page allocates
        // ~60% of viewport width to the builder, so a 1280-wide page
        // overflows on every realistic viewport). Picking the device
        // preset here keeps the canvas page reasonable across devices.
        let (page_w, page_h) = device_page_dims(self.device);
        let mut props = json!({
            "selection-id": self.selection.clone().unwrap_or_default(),
            "tool": self.tool.as_str(),
            "viewport-width": self.viewport.width,
            "viewport-height": self.viewport.height,
            "page-width": page_w,
            "page-height": page_h,
            "zoom": self.viewport.zoom,
            "pan-x": self.viewport.pan_x,
            "pan-y": self.viewport.pan_y,
            "place-mode": self.picker.open,
            "device": self.device.as_str(),
            "node-count": self.node_count(),
        });
        // Wave 3.3 — emit the selection bbox so `build_selection_layer`
        // paints the outline + 8-handle ring around the live click
        // rect. Absent when no canvas node is selected or the
        // selection was set programmatically (no pointer hit to
        // sample bounds from).
        if let Some(bbox) = self.selection_bbox {
            props["selection-rect"] = json!({
                "x": bbox.x,
                "y": bbox.y,
                "width": bbox.width,
                "height": bbox.height,
            });
        }
        props
    }

    /// Wave 3.2 polish — emit the in-flight palette-drag state so the
    /// canvas overlay layer can paint a ghost rect at the cursor.
    /// Lives on the canvas binding's host emission rather than the
    /// `builder_canvas_props` JSON because the drag state belongs to
    /// `CatalogSlot`, not `CanvasSlot` — `props.rs` merges this into
    /// the canvas prop bag in one row.
    pub fn palette_drag_overlay(drag: Option<&PaletteDrag>) -> Value {
        match drag {
            Some(d) => json!({
                "active": true,
                "kind": d.kind,
                "x": d.pointer.0,
                "y": d.pointer.1,
                "drop-target": d.drop_target.clone().unwrap_or_default(),
            }),
            None => json!({ "active": false }),
        }
    }

    /// JSON for `shell.builder-toolbar` (§43 D2). The toolbar emits
    /// the alignment buttons, device-mode cluster, zoom, and node
    /// count — every datum derives from this slot, so a single
    /// binding row keeps every other panel out of the toolbar's
    /// shape. The block reads:
    /// - `device`: active responsive preview mode
    /// - `zoom`: 0..n multiplier for the page rect
    /// - `node-count`: total node count in the document
    /// - `tool`: active tool mode (move/rotate/scale)
    pub fn builder_toolbar_props(&self) -> Value {
        json!({
            "device": self.device.as_str(),
            "zoom": self.viewport.zoom,
            "node-count": self.node_count(),
            "tool": self.tool.as_str(),
            // Pre-formatted percent label so the DSL doesn't need a
            // `floor()` / `* 100` arithmetic helper.
            "zoom-label": format!("{}%", (self.viewport.zoom * 100.0).round() as i32),
        })
    }

    /// Lower `self.document` to a runtime `Vec<UiNode>` against a
    /// live registry. The binding for `shell.builder-canvas` calls
    /// this and forwards the result via `PropEmission::with_children`
    /// — the render pipeline then injects those nodes into the
    /// resolver scope's `host_children_by_tag` map, so the canvas
    /// block sees the document as its `host_children`.
    ///
    /// Returns an empty vector when the document has no root (the
    /// canvas falls through to its grid + selection overlay only) or
    /// when no registry is available (headless / no-DI render paths
    /// keep working with metadata-only canvas emission).
    pub fn lower_document_to_ui(
        &self,
        registry: Option<&prism_builder::ComponentRegistry>,
    ) -> Vec<prism_ui_runtime::layout::Node> {
        self.lower_document_to_ui_full(registry, None, None)
    }

    /// Phase 3b / 4b shim kept for callers that don't carry the
    /// modifier registry. New code uses [`Self::lower_document_to_ui_full`].
    pub fn lower_document_to_ui_with_invalidator(
        &self,
        registry: Option<&prism_builder::ComponentRegistry>,
        invalidator: Option<prism_builder::ui_lower::BlockInvalidator>,
    ) -> Vec<prism_ui_runtime::layout::Node> {
        self.lower_document_to_ui_full(registry, invalidator, None)
    }

    /// Phase 3b + Phase 4b of `docs/dev/dioxus-inspiration.md` +
    /// Wave 1 of `docs/dev/composable-builder-plan.md`: lower the
    /// builder document with an optional [`BlockInvalidator`]
    /// (per-block dirty subscriptions), the canvas's
    /// [`prism_builder::DocumentBindings`] (per-NodeId reactive prop
    /// bags), and an optional [`prism_builder::ModifierRegistry`]
    /// (Wave 1 render fold of `node.modifiers` over each block's
    /// output). All three install on the same `LowerCtx` so reactive
    /// reads subscribe, signal writes invalidate, and attached
    /// behaviours wrap the rendered subtree without per-block
    /// plumbing.
    pub fn lower_document_to_ui_full(
        &self,
        registry: Option<&prism_builder::ComponentRegistry>,
        invalidator: Option<prism_builder::ui_lower::BlockInvalidator>,
        modifier_registry: Option<&prism_builder::ModifierRegistry>,
    ) -> Vec<prism_ui_runtime::layout::Node> {
        let Some(reg) = registry else {
            return Vec::new();
        };
        let Some(root) = self.document.root.as_ref() else {
            return Vec::new();
        };
        let cascade = prism_builder::StyleProperties::default();
        let mut ctx = prism_builder::ui_lower::LowerCtx::new(Some(reg), &cascade)
            .with_bindings(&self.bindings);
        if let Some(inv) = invalidator {
            ctx = ctx.with_block_invalidator(inv);
        }
        if let Some(mod_reg) = modifier_registry {
            ctx = ctx.with_modifier_registry(mod_reg);
        }
        vec![ctx.lower(root)]
    }

    /// JSON for `shell.gizmo-move`. Forwards through the shared
    /// [`Self::gizmo_props`] helper — the only emitter for the gizmo
    /// shape across all three tool modes.
    pub fn gizmo_move_props(&self) -> Value {
        self.gizmo_props(ToolMode::Move)
    }

    /// JSON for `shell.gizmo-rotate`. See [`Self::gizmo_props`].
    pub fn gizmo_rotate_props(&self) -> Value {
        self.gizmo_props(ToolMode::Rotate)
    }

    /// JSON for `shell.gizmo-scale`. See [`Self::gizmo_props`].
    pub fn gizmo_scale_props(&self) -> Value {
        self.gizmo_props(ToolMode::Scale)
    }

    /// JSON for `shell.resize-handle`. Eight handles around the
    /// selection bbox; each carries its `id` (`"tl"`, `"t"`, …), pixel
    /// position, and a CSS-style cursor name. Visibility collapses to
    /// `visible: false` + an empty handle list when nothing is
    /// selected — same data-shape pattern as gizmos.
    pub fn resize_handle_props(&self) -> Value {
        let visible = self.selection.is_some();
        let handles = if visible {
            self.handle_positions_json()
        } else {
            Value::Array(Vec::new())
        };
        json!({
            "visible": visible,
            "handles": handles,
        })
    }

    /// JSON for `shell.component-picker`. Pop-up palette anchored at
    /// `(anchor-x, anchor-y)` listing droppable components. The block
    /// reads `open` to decide whether to paint at all.
    pub fn component_picker_props(&self) -> Value {
        json!({
            "open": self.picker.open,
            "anchor-x": self.picker.anchor_x,
            "anchor-y": self.picker.anchor_y,
            "candidates": self.candidates_json(),
        })
    }

    /// Rule-of-three trigger: three gizmo bindings emit byte-identical
    /// key sets and only differ in the `tool` discriminator. Helper
    /// extracts on landing — same justification as
    /// [`DocsSlot::topic_props`] and [`MenuSlot::items_json`]. A drift
    /// in the gizmo shape edits *one* site, not three.
    fn gizmo_props(&self, kind: ToolMode) -> Value {
        let visible = self.selection.is_some() && self.tool == kind;
        let center = self.selection_center().unwrap_or((0.0, 0.0));
        json!({
            "visible": visible,
            "center-x": center.0,
            "center-y": center.1,
            "tool": kind.as_str(),
        })
    }

    /// Selected node's center in canvas coordinates. `pub(crate)` so
    /// tests + future cross-binding consumers (snap-line overlay,
    /// alignment guides) hit the same code path. Two consumers today
    /// (`gizmo_props`, `resize_handle_props`); rule-of-three's
    /// "imminent third" justifies the helper.
    pub(crate) fn selection_center(&self) -> Option<(f32, f32)> {
        let id = self.selection.as_deref()?;
        let node = self.document.root.as_ref()?.find(id)?;
        Some((node.transform.position[0], node.transform.position[1]))
    }

    /// Friendly label for the current selection, or `"No selection"`
    /// when nothing is selected. Consumed by the §43 D5 multi-segment
    /// status bar and (eventually) by any future selection-aware
    /// breadcrumb. Same `inspector_label_for` heuristic the inspector
    /// tree uses, so the two read sites can never disagree.
    pub(crate) fn selection_label(&self) -> String {
        let Some(id) = self.selection.as_deref() else {
            return "No selection".to_string();
        };
        let Some(root) = self.document.root.as_ref() else {
            return id.to_string();
        };
        match root.find(id) {
            Some(node) => inspector_label_for(node),
            None => id.to_string(),
        }
    }

    /// Total node count in the active document (root + every
    /// descendant). Consumed by the §43 D5 status bar. One walk, one
    /// integer — no allocation.
    pub(crate) fn node_count(&self) -> usize {
        fn walk(node: &prism_builder::Node) -> usize {
            1 + node.children.iter().map(walk).sum::<usize>()
        }
        self.document.root.as_ref().map(walk).unwrap_or(0)
    }

    fn handle_positions_json(&self) -> Value {
        let Some((cx, cy)) = self.selection_center() else {
            return Value::Array(Vec::new());
        };
        // Bbox is approximated by the transform position + a default
        // 100×100 region; once the layout pass exposes computed rects
        // per-node, this reads from `ComputedLayout::rect(id)` instead
        // (same emitter shape, different source).
        let half = 50.0;
        let (x0, y0, x1, y1) = (cx - half, cy - half, cx + half, cy + half);
        let mx = (x0 + x1) * 0.5;
        let my = (y0 + y1) * 0.5;
        let sites = [
            (HandleSide::TopLeft, x0, y0),
            (HandleSide::Top, mx, y0),
            (HandleSide::TopRight, x1, y0),
            (HandleSide::Right, x1, my),
            (HandleSide::BottomRight, x1, y1),
            (HandleSide::Bottom, mx, y1),
            (HandleSide::BottomLeft, x0, y1),
            (HandleSide::Left, x0, my),
        ];
        Value::Array(
            sites
                .into_iter()
                .map(|(side, x, y)| {
                    json!({
                        "id": side.id(),
                        "x": x,
                        "y": y,
                        "cursor": side.cursor(),
                    })
                })
                .collect(),
        )
    }

    fn candidates_json(&self) -> Value {
        Value::Array(
            self.picker
                .candidates
                .iter()
                .map(|c| {
                    json!({
                        "candidate-id": c.id,
                        "label": c.label,
                        "icon": c.icon,
                    })
                })
                .collect(),
        )
    }

    // ── write side ────────────────────────────────────────────────

    /// Pointer-down: hit-test what's under the cursor and capture it.
    /// Returns `false` because no observable state changed yet (the
    /// drag is purely captured) — the next `pointer_move` is the
    /// first redraw trigger.
    pub(crate) fn pointer_down(&mut self, x: f32, y: f32) -> bool {
        let Some(kind) = self.hit_test(x, y) else {
            return false;
        };
        let Some(snapshot) = TransformSnapshot::capture(&self.document, self.selection.as_deref())
        else {
            return false;
        };
        self.drag = Some(DragState {
            kind,
            snapshot,
            origin: (x, y),
        });
        false
    }

    /// Pointer-move: if a drag is captured, translate the delta into a
    /// transform mutation through the *single* dispatch over
    /// `(tool, drag-target)` that lives on this slot. The router never
    /// grows tool-mode awareness.
    pub(crate) fn pointer_move(&mut self, x: f32, y: f32) -> bool {
        // Pull origin + kind out without holding a borrow across the
        // mutation — `apply_*_delta` need `&mut self`.
        let (kind, origin) = match self.drag.as_ref() {
            Some(d) => (d.kind, d.origin),
            None => return false,
        };
        let zoom = self.viewport.zoom.max(f32::EPSILON);
        let dx = (x - origin.0) / zoom;
        let dy = (y - origin.1) / zoom;
        match kind {
            DragKind::Gizmo => self.apply_gizmo_delta(self.tool, dx, dy),
            DragKind::Handle(side) => self.apply_handle_delta(side, dx, dy),
        }
        true
    }

    /// Pointer-up: commit the drag. One undo snapshot per drag, never
    /// per tick.
    pub(crate) fn pointer_up(&mut self, _x: f32, _y: f32) -> bool {
        let Some(_drag) = self.drag.take() else {
            return false;
        };
        // commit_drag here would push the snapshot onto the undo stack
        // once that lands on the new shell. Today the *effect* (mutated
        // node transform) is already in the document; the undo
        // contract simply has no consumer to feed.
        true
    }

    /// Single dispatch over `(tool, drag-target=Gizmo)`. This is the
    /// *only* place in the codebase that knows what a gizmo delta
    /// means under each tool. Adding a new tool mode (e.g. `Skew`) is
    /// one variant on [`ToolMode`] + one arm here. The router doesn't
    /// move.
    fn apply_gizmo_delta(&mut self, tool: ToolMode, dx: f32, dy: f32) {
        let Some(snapshot) = self.drag.as_ref().map(|d| d.snapshot.clone()) else {
            return;
        };
        let Some(node) = self
            .document
            .root
            .as_mut()
            .and_then(|root| root.find_mut(&snapshot.node_id))
        else {
            return;
        };
        match tool {
            ToolMode::Move => {
                node.transform.position[0] = snapshot.transform.position[0] + dx;
                node.transform.position[1] = snapshot.transform.position[1] + dy;
            }
            ToolMode::Rotate => {
                // Godot-standard: 0.5°/px horizontal drag. Convert to
                // radians for the canonical `rotation` field.
                let degrees = dx * 0.5;
                node.transform.rotation = snapshot.transform.rotation + degrees.to_radians();
            }
            ToolMode::Scale => {
                node.transform.scale[0] = (snapshot.transform.scale[0] + dx / 100.0).max(0.01);
                node.transform.scale[1] = (snapshot.transform.scale[1] + dy / 100.0).max(0.01);
            }
        }
    }

    fn apply_handle_delta(&mut self, side: HandleSide, dx: f32, dy: f32) {
        let Some(snapshot) = self.drag.as_ref().map(|d| d.snapshot.clone()) else {
            return;
        };
        let Some(node) = self
            .document
            .root
            .as_mut()
            .and_then(|root| root.find_mut(&snapshot.node_id))
        else {
            return;
        };
        let (px, py, _sx, _sy) = side.deltas();
        // Resize maps to position-only on this slot until the layout
        // engine exposes per-node `width`/`height` mutators; once it
        // does, the `_sx`/`_sy` returns from `HandleSide::deltas()`
        // become the size mutation.
        node.transform.position[0] = snapshot.transform.position[0] + px * dx;
        node.transform.position[1] = snapshot.transform.position[1] + py * dy;
    }

    /// What's under the cursor? `Some(DragKind)` if anything draggable
    /// is hit; `None` otherwise. The single dispatch over drag-target
    /// geometry — six bindings *cannot* disagree about hit regions
    /// because they don't compute them; the slot does, once.
    fn hit_test(&self, x: f32, y: f32) -> Option<DragKind> {
        let (cx, cy) = self.selection_center()?;
        // Resize handles take precedence over the gizmo when the
        // pointer lands on one — handles are the smaller target.
        let half = 50.0;
        let edge = 6.0;
        let (x0, y0, x1, y1) = (cx - half, cy - half, cx + half, cy + half);
        let on = |a: f32, b: f32| (a - b).abs() <= edge;
        let near_x = (x - x0).abs() <= edge || (x - x1).abs() <= edge;
        let near_y = (y - y0).abs() <= edge || (y - y1).abs() <= edge;
        let on_left = on(x, x0);
        let on_right = on(x, x1);
        let on_top = on(y, y0);
        let on_bottom = on(y, y1);
        let mid_x = on(x, (x0 + x1) * 0.5);
        let mid_y = on(y, (y0 + y1) * 0.5);
        let in_x = x >= x0 - edge && x <= x1 + edge;
        let in_y = y >= y0 - edge && y <= y1 + edge;
        if near_x && near_y {
            let side = match (on_left, on_top) {
                (true, true) => HandleSide::TopLeft,
                (false, true) if on_right => HandleSide::TopRight,
                (true, false) if on_bottom => HandleSide::BottomLeft,
                _ => HandleSide::BottomRight,
            };
            return Some(DragKind::Handle(side));
        }
        if (on_top || on_bottom) && mid_x && in_x {
            return Some(DragKind::Handle(if on_top {
                HandleSide::Top
            } else {
                HandleSide::Bottom
            }));
        }
        if (on_left || on_right) && mid_y && in_y {
            return Some(DragKind::Handle(if on_left {
                HandleSide::Left
            } else {
                HandleSide::Right
            }));
        }
        // Anything else inside the selection bbox captures the gizmo.
        if x >= x0 && x <= x1 && y >= y0 && y <= y1 {
            return Some(DragKind::Gizmo);
        }
        None
    }

    /// Test-only seam: did the slot capture a drag? Bindings cannot
    /// see this — the cross-binding parity tests use it to assert
    /// pointer events route through the slot correctly.
    #[cfg(test)]
    pub(crate) fn drag_active(&self) -> bool {
        self.drag.is_some()
    }

    // ── §25: keyboard-driven mutators ─────────────────────────────

    /// Translate the selected node's position by `(dx, dy)` (canvas
    /// units). Used by `selection.move-{up,down,left,right}`. The
    /// `(dx, dy)` table lives on `SelectionService`; this method is
    /// the single mutator that interprets it.
    pub fn nudge_selection(&mut self, dx: f32, dy: f32) {
        let Some(id) = self.selection.clone() else {
            return;
        };
        let Some(node) = self.document.root.as_mut().and_then(|r| r.find_mut(&id)) else {
            return;
        };
        node.transform.position[0] += dx;
        node.transform.position[1] += dy;
    }

    /// Shift+arrow extension — single-selection today is a synonym
    /// for `nudge_selection`. When `SelectionModel::Multi` lands
    /// (post-§25 wave), this dispatches to a "grow the marquee"
    /// variant; the service interface stays one method per command.
    pub fn extend_selection(&mut self, dx: f32, dy: f32) {
        self.nudge_selection(dx, dy);
    }

    /// Serialise the selected sub-tree as a `serde_json::Value`. The
    /// only wire format the clipboard speaks. Returns `None` when no
    /// selection exists or the id has gone stale.
    pub fn serialize_selection(&self) -> Option<Value> {
        let id = self.selection.as_deref()?;
        let node = self.document.root.as_ref()?.find(id)?;
        serde_json::to_value(node).ok()
    }

    /// Insert a previously-serialised sub-tree at `offset` positions
    /// past the current selection within its parent's children.
    /// `offset = 0` means "append as last child of selection's
    /// parent" (paste); `offset = 1` means "insert as next sibling"
    /// (duplicate). Returns the new node id when successful.
    ///
    /// Always assigns a fresh id (and recursively rewrites child
    /// ids) so paste/duplicate never collide with the source. Uses
    /// the simplest unique-suffix scheme — sufficient until a
    /// genuinely-clashing scenario forces a UUID/ULID move.
    pub fn insert_at_offset(&mut self, value: Value, offset: usize) -> Option<NodeId> {
        let mut node: prism_builder::Node = serde_json::from_value(value).ok()?;
        rename_subtree(&mut node, &self.document, "paste");
        let new_id = node.id.clone();
        let target = self.selection.clone();
        let root = self.document.root.as_mut()?;
        // Inserting against the root with no selection: append as
        // child of root.
        let Some(target_id) = target else {
            root.children.push(node);
            return Some(new_id);
        };
        // Walk to find the parent of `target_id` and the index of the
        // selected child within it.
        if root.id == target_id {
            // Selection is the root — insert as first/next child.
            if offset == 0 {
                root.children.push(node);
            } else {
                root.children.insert(0, node);
            }
            return Some(new_id);
        }
        let inserted = insert_under_parent(root, &target_id, node, offset);
        inserted.then_some(new_id)
    }

    /// Wave 3.2 — insert a fresh node tree as the last child of
    /// `target` (or of the document root when `target` is `None`).
    /// Used by the palette-drop pipeline: a drop on top of an existing
    /// canvas node parents the new node under it; a drop on empty
    /// canvas appends at the root. Always assigns a fresh id
    /// (`rename_subtree`) so repeated drops of the same palette item
    /// never collide.
    pub fn insert_under_node(&mut self, value: Value, target: Option<&str>) -> Option<NodeId> {
        let mut node: prism_builder::Node = serde_json::from_value(value).ok()?;
        rename_subtree(&mut node, &self.document, "drop");
        let new_id = node.id.clone();
        let root = self.document.root.as_mut()?;
        let dest = target.unwrap_or(root.id.as_str()).to_string();
        if root.id == dest {
            root.children.push(node);
            return Some(new_id);
        }
        push_under(root, &dest, node).then_some(new_id)
    }

    /// Remove the currently-selected node from the document. Used by
    /// `clipboard.cut`. The selection cursor is cleared because the
    /// node it pointed at no longer exists.
    pub fn delete_selection(&mut self) {
        let Some(id) = self.selection.take() else {
            return;
        };
        let Some(root) = self.document.root.as_mut() else {
            return;
        };
        if root.id == id {
            // Deleting the root collapses the document.
            self.document.root = None;
            return;
        }
        delete_under(root, &id);
    }

    /// Swap the selected node with its previous (`dir = -1`) or next
    /// (`dir = +1`) sibling within its parent's `children` vec. Used
    /// by the inspector-row up / down chevrons. Returns `true` when
    /// the document actually changed.
    pub fn reorder_selection(&mut self, dir: i32) -> bool {
        if dir == 0 {
            return false;
        }
        let Some(id) = self.selection.clone() else {
            return false;
        };
        let Some(root) = self.document.root.as_mut() else {
            return false;
        };
        // Selection at the root has no siblings to swap with.
        if root.id == id {
            return false;
        }
        swap_sibling_under(root, &id, dir)
    }

    /// Multiply the canvas zoom by `factor`, clamped to the
    /// `[0.1, 8.0]` range the toolbar's schema declares. Returns
    /// `true` when the zoom actually moved.
    pub fn zoom_by(&mut self, factor: f32) -> bool {
        let new_zoom = (self.viewport.zoom * factor).clamp(0.1, 8.0);
        if (new_zoom - self.viewport.zoom).abs() < f32::EPSILON {
            return false;
        }
        self.viewport.zoom = new_zoom;
        true
    }

    /// Set a `text-align` prop on the selected node. The string is
    /// passed through verbatim — the lowering layer interprets
    /// `"left"` / `"center"` / `"right"` against `Node::Text`'s
    /// horizontal-alignment prop. Returns `true` when the prop
    /// actually changed.
    pub fn set_selection_align(&mut self, align: &str) -> bool {
        let Some(id) = self.selection.clone() else {
            return false;
        };
        let Some(node) = self.document.root.as_mut().and_then(|r| r.find_mut(&id)) else {
            return false;
        };
        let value = Value::String(align.to_string());
        if let Value::Object(map) = &mut node.props {
            if map.get("text-align") == Some(&value) {
                return false;
            }
            map.insert("text-align".into(), value);
        } else {
            node.props = Value::Object([("text-align".into(), value)].into_iter().collect());
        }
        true
    }
}

/// Recursively rewrite ids in `node` so they don't collide with any
/// id already present in `doc`. Stable suffix scheme: `<old>-<tag>-<n>`.
fn rename_subtree(node: &mut prism_builder::Node, doc: &BuilderDocument, tag: &str) {
    let mut existing: std::collections::HashSet<String> = std::collections::HashSet::new();
    if let Some(root) = doc.root.as_ref() {
        collect_ids(root, &mut existing);
    }
    rewrite_ids(node, tag, &mut existing);
}

fn collect_ids(node: &prism_builder::Node, out: &mut std::collections::HashSet<String>) {
    out.insert(node.id.clone());
    for c in &node.children {
        collect_ids(c, out);
    }
}

fn rewrite_ids(
    node: &mut prism_builder::Node,
    tag: &str,
    existing: &mut std::collections::HashSet<String>,
) {
    let mut candidate = format!("{}-{}", node.id, tag);
    let mut n = 1u32;
    while existing.contains(&candidate) {
        n += 1;
        candidate = format!("{}-{}-{}", node.id, tag, n);
    }
    existing.insert(candidate.clone());
    node.id = candidate;
    for c in &mut node.children {
        rewrite_ids(c, tag, existing);
    }
}

/// Wave 4.3 — kebab-case a free-form label for use in a stable
/// connection id. Lowercases ASCII letters, replaces every
/// non-alphanumeric run with a single `-`, and trims edge dashes.
/// Empty input yields `"item"` so the resulting id never collapses
/// to a bare separator.
fn sanitise_id(label: &str) -> String {
    let mut out = String::with_capacity(label.len());
    let mut prev_sep = true;
    for ch in label.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            prev_sep = false;
        } else if !prev_sep {
            out.push('-');
            prev_sep = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() {
        "item".into()
    } else {
        out
    }
}

/// Wave 4.3 — find a unique id for a new connection given the
/// existing list. Mirrors the `rename_subtree` pattern: try the
/// raw id, then `<id>-2`, `<id>-3`, … until it doesn't collide.
fn uniquify_connection_id(existing: &[SignalConnection], raw: &str) -> String {
    if !existing.iter().any(|c| c.id == raw) {
        return raw.to_string();
    }
    let mut n = 2u32;
    loop {
        let candidate = format!("{raw}-{n}");
        if !existing.iter().any(|c| c.id == candidate) {
            return candidate;
        }
        n += 1;
    }
}

/// Wave 3.3 — map a handle direction to the `(px, py)` sign pair
/// that drives the resize delta. Mirrors `HandleSide::deltas` on
/// the existing transform-based handle drag (which uses an enum);
/// the string-keyed version lives here because the route comes in
/// as a kebab-case attr (`tl|t|tr|...`) and the enum mapping is
/// internal to the synthetic hit-test path. Top/left handles
/// translate the node negatively; bottom/right translate
/// positively; the four cardinal edges pin the orthogonal axis at
/// zero.
fn resize_delta_signs(dir: &str) -> (f32, f32) {
    match dir {
        "tl" => (-1.0, -1.0),
        "t" => (0.0, -1.0),
        "tr" => (1.0, -1.0),
        "r" => (1.0, 0.0),
        "br" => (1.0, 1.0),
        "b" => (0.0, 1.0),
        "bl" => (-1.0, 1.0),
        "l" => (-1.0, 0.0),
        _ => (0.0, 0.0),
    }
}

/// Wave 3.4 — build the canvas context-menu rows for the current
/// `(selection, clipboard, document)` state. When a node is
/// selected the menu carries the node-mutation triad (move up /
/// down / delete) plus a duplicate row when the clipboard would
/// allow it; when no node is selected the menu falls back to the
/// document-level actions (paste, add page). Returns an empty
/// vector when no action would be meaningful so the open-menu
/// route can short-circuit.
fn canvas_context_menu_items(state: &AppState) -> Vec<MenuItem> {
    let mut items: Vec<MenuItem> = Vec::new();
    let has_selection = state.canvas.selection.is_some();
    if has_selection {
        items.push(MenuItem {
            label: "Move Up".into(),
            shortcut: None,
            command: Some("builder.move-selected-up".into()),
            separator: false,
            enabled: true,
        });
        items.push(MenuItem {
            label: "Move Down".into(),
            shortcut: None,
            command: Some("builder.move-selected-down".into()),
            separator: false,
            enabled: true,
        });
        items.push(MenuItem::separator());
        items.push(MenuItem {
            label: "Duplicate".into(),
            shortcut: Some("Cmd+D".into()),
            command: Some("clipboard.duplicate".into()),
            separator: false,
            enabled: true,
        });
        items.push(MenuItem {
            label: "Copy".into(),
            shortcut: Some("Cmd+C".into()),
            command: Some("clipboard.copy".into()),
            separator: false,
            enabled: true,
        });
        items.push(MenuItem {
            label: "Cut".into(),
            shortcut: Some("Cmd+X".into()),
            command: Some("clipboard.cut".into()),
            separator: false,
            enabled: true,
        });
        items.push(MenuItem::separator());
        items.push(MenuItem {
            label: "Delete".into(),
            shortcut: Some("Delete".into()),
            command: Some("builder.delete-selected".into()),
            separator: false,
            enabled: true,
        });
    } else {
        items.push(MenuItem {
            label: "Paste".into(),
            shortcut: Some("Cmd+V".into()),
            command: Some("clipboard.paste".into()),
            separator: false,
            // `clipboard.paste` itself is a no-op against an empty
            // clipboard cell — surface the row enabled so the
            // visual affordance matches every other paste site (the
            // existing menu-bar Paste entry follows the same
            // discipline).
            enabled: true,
        });
    }
    items
}

/// Wave 3.2 — append `incoming` as the last child of `target_id`,
/// searched recursively from `parent`. Returns true on first match.
fn push_under(
    parent: &mut prism_builder::Node,
    target_id: &str,
    incoming: prism_builder::Node,
) -> bool {
    if parent.id == target_id {
        parent.children.push(incoming);
        return true;
    }
    let mut moving = Some(incoming);
    for child in &mut parent.children {
        if let Some(node) = moving.take() {
            if push_under(child, target_id, node.clone()) {
                return true;
            }
            moving = Some(node);
        }
    }
    false
}

/// Wave 3.2 — `kind` → serializable `Node` template. The palette
/// catalogue is seeded from `starter::BUILTINS` plus the `card`
/// prefab, so the only two paths needed are (1) the prefab
/// materialiser for `card` and (2) a vanilla `Node` for every
/// regular component id. Block-specific lower funcs fall back to
/// their schema defaults when props are empty — the dropped node
/// renders with sensible chrome on the first frame.
fn palette_node_template(kind: &str) -> Option<Value> {
    if let Some(def) = prism_builder::builtin_prefab(kind) {
        // Use a counter seeded from the kind so repeated drops
        // produce unique ids before `rename_subtree` runs.
        let mut counter = 0u64;
        let node = prism_builder::materialize_prefab(&def, &mut counter);
        return serde_json::to_value(node).ok();
    }
    let node = prism_builder::Node {
        id: format!("{kind}-new"),
        component: prism_builder::ComponentId::from(kind),
        props: Value::Object(Default::default()),
        ..Default::default()
    };
    serde_json::to_value(node).ok()
}

/// Insert `incoming` next to (or after) the child `target_id` under
/// any descendant of `root`. Returns true when the insertion landed.
fn insert_under_parent(
    parent: &mut prism_builder::Node,
    target_id: &str,
    incoming: prism_builder::Node,
    offset: usize,
) -> bool {
    if let Some(idx) = parent.children.iter().position(|c| c.id == target_id) {
        let pos = (idx + offset).min(parent.children.len());
        parent.children.insert(pos, incoming);
        return true;
    }
    let mut moving = Some(incoming);
    for child in &mut parent.children {
        if let Some(node) = moving.take() {
            if insert_under_parent(child, target_id, node.clone(), offset) {
                return true;
            }
            moving = Some(node);
        }
    }
    false
}

fn delete_under(parent: &mut prism_builder::Node, target_id: &str) -> bool {
    if let Some(idx) = parent.children.iter().position(|c| c.id == target_id) {
        parent.children.remove(idx);
        return true;
    }
    for child in &mut parent.children {
        if delete_under(child, target_id) {
            return true;
        }
    }
    false
}

/// Swap the child `target_id` with its `dir`-neighbour (negative =
/// previous, positive = next) under any descendant of `parent`.
/// Returns true once the swap landed.
fn swap_sibling_under(parent: &mut prism_builder::Node, target_id: &str, dir: i32) -> bool {
    if let Some(idx) = parent.children.iter().position(|c| c.id == target_id) {
        let len = parent.children.len();
        let new_idx = idx as i32 + dir;
        if new_idx < 0 || new_idx as usize >= len {
            return false;
        }
        parent.children.swap(idx, new_idx as usize);
        return true;
    }
    for child in &mut parent.children {
        if swap_sibling_under(child, target_id, dir) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
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
            draft: "hi".into(),
            original: "hi".into(),
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
                },
                FileNode {
                    id: "src/lib.rs".into(),
                    label: "lib.rs".into(),
                    depth: 1,
                    kind: FileKind::File,
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
            code_buffer: CodeBuffer {
                source: "Window {}".into(),
                language: "slint".into(),
                caret: 7,
            },
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
        assert_eq!(props["source"], "Window {}");
        assert_eq!(props["caret"], 7);
        assert_eq!(props["language"], "slint");
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
}
