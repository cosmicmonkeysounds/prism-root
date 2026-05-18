//! Canvas slot: types, `impl CanvasSlot`, canvas-tree helpers.
//! Split out of `state/mod.rs` (Phase B.4). `use super::*`
//! inherits intra-`state` types + crate imports; cross-module
//! callers reach widened `pub(crate)` items.

use super::*;

mod parts;
pub use parts::*;

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
    /// Per-tab metadata for the currently active buffer (path,
    /// title, dirty flag). Mirrors what would otherwise live on
    /// `code_buffer` itself; kept separate so the `CodeBuffer` type
    /// can stay a pure editor state without file-system concerns.
    pub code_buffer_meta: EditorTabMeta,
    /// Inactive editor tabs. The active tab's live editing state is
    /// in `code_buffer`; this vector carries every *other* open
    /// file (or untitled scratch buffer) as a snapshot pair.
    /// Visual tab order is `[..code_tabs[..active]] + active +
    /// [code_tabs[active..]]` — i.e. the active tab logically sits
    /// at `code_active_tab` in the combined display order.
    pub code_tabs: Vec<EditorTab>,
    /// Logical index of the active tab in the *display* list (which
    /// is `code_tabs` with the active tab spliced in at this index).
    /// In `[0..=code_tabs.len()]` — `0` means active is first;
    /// `code_tabs.len()` means active is last. Always in range.
    pub code_active_tab: usize,
    /// §43 D2: active responsive preview mode. Drives the
    /// Desktop/Tablet/Mobile button cluster in `shell.builder-toolbar`
    /// and (eventually) constrains the canvas page width when the
    /// builder is in preview mode.
    pub device: Device,
    pub(crate) drag: Option<DragState>,
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
impl CanvasSlot {
    // ── editor tabs ───────────────────────────────────────────────

    /// Total number of open editor tabs (active + inactive).
    pub fn editor_tab_count(&self) -> usize {
        self.code_tabs.len() + 1
    }

    /// JSON shape of every open tab in display order, for the tab-
    /// strip binding. Each entry carries `title`, `dirty`, and a
    /// position index the click router uses to switch active.
    pub fn editor_tab_strip(&self) -> Vec<Value> {
        let mut out = Vec::with_capacity(self.editor_tab_count());
        for (idx, tab) in self.code_tabs.iter().enumerate() {
            if idx == self.code_active_tab {
                out.push(self.tab_entry(idx, &self.code_buffer_meta, true));
            }
            out.push(self.tab_entry(out.len(), &tab.meta, false));
        }
        // Active tab at the end?
        if self.code_active_tab >= self.code_tabs.len() {
            out.push(self.tab_entry(out.len(), &self.code_buffer_meta, true));
        }
        out
    }

    pub(crate) fn tab_entry(&self, idx: usize, meta: &EditorTabMeta, active: bool) -> Value {
        json!({
            "index": idx,
            "title": meta.title,
            "dirty": meta.dirty,
            "active": active,
            "path": meta.path.as_ref().map(|p| p.display().to_string()),
        })
    }

    /// **IDE Phase 7** — serialise the open editor session (every
    /// path-backed tab + its caret byte, with the active one flagged)
    /// so the host can persist it next to the project and restore on
    /// reopen. Untitled scratch buffers are skipped — they have
    /// nowhere to reload from. Shape:
    /// `{ "tabs": [{ "path": "...", "caret": N, "active": bool }] }`.
    pub fn editor_session_snapshot(&self) -> Value {
        let mut tabs: Vec<Value> = Vec::new();
        let active_disp = self.code_active_tab.min(self.code_tabs.len());
        // The inactive vec, plus the live active buffer spliced at
        // its display index — mirrors `editor_tab_strip` ordering.
        let mut emit = |meta: &EditorTabMeta, caret: usize, active: bool| {
            if let Some(p) = meta.path.as_ref() {
                tabs.push(json!({
                    "path": p.to_string_lossy(),
                    "caret": caret,
                    "active": active,
                }));
            }
        };
        for (idx, tab) in self.code_tabs.iter().enumerate() {
            if idx == active_disp {
                emit(
                    &self.code_buffer_meta,
                    self.code_buffer.editor.caret_byte(),
                    true,
                );
            }
            emit(&tab.meta, tab.buffer.editor.caret_byte(), false);
        }
        if active_disp >= self.code_tabs.len() {
            emit(
                &self.code_buffer_meta,
                self.code_buffer.editor.caret_byte(),
                true,
            );
        }
        json!({ "tabs": tabs })
    }

    /// **IDE Phase 7** — restore an [`editor_session_snapshot`] value.
    /// `read` resolves a path's source (host passes a VFS- or
    /// `std::fs`-backed closure); unreadable paths are skipped.
    /// Non-active tabs open first, then the active one last so it
    /// ends focused with its caret placed.
    ///
    /// [`editor_session_snapshot`]: Self::editor_session_snapshot
    pub fn restore_editor_session(
        &mut self,
        snapshot: &Value,
        read: impl Fn(&std::path::Path) -> Option<String>,
    ) {
        let Some(rows) = snapshot.get("tabs").and_then(|t| t.as_array()) else {
            return;
        };
        let mut ordered: Vec<(std::path::PathBuf, usize, bool)> = Vec::new();
        for r in rows {
            let Some(p) = r.get("path").and_then(|v| v.as_str()) else {
                continue;
            };
            let caret = r.get("caret").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            let active = r.get("active").and_then(|v| v.as_bool()).unwrap_or(false);
            ordered.push((std::path::PathBuf::from(p), caret, active));
        }
        // Stable: inactive first (in recorded order), then the active.
        ordered.sort_by_key(|(_, _, active)| *active);
        for (path, caret, _) in ordered {
            let Some(src) = read(&path) else {
                continue;
            };
            let language = crate::services::editor_files::language_from_path(&path);
            self.open_editor_tab(path, src, language);
            self.code_buffer.editor.place_caret_at(caret, false);
        }
    }

    /// Snapshot the live editing state into the active slot of
    /// `code_tabs`. Cheap (`code_buffer.clone()`), invoked on every
    /// tab switch + every save so the inactive vec stays current.
    pub(crate) fn stash_active_into_tabs(&mut self) {
        let tab = EditorTab {
            meta: self.code_buffer_meta.clone(),
            buffer: self.code_buffer.clone(),
        };
        let active = self.code_active_tab.min(self.code_tabs.len());
        if active < self.code_tabs.len() {
            self.code_tabs[active] = tab;
        } else {
            self.code_tabs.push(tab);
        }
    }

    /// Switch the active tab to display index `target`. Saves the
    /// current live buffer back into its slot, then pulls the
    /// target slot's buffer + meta into the active position.
    /// Returns `true` when the switch actually moved.
    pub fn switch_editor_tab(&mut self, target: usize) -> bool {
        if target >= self.editor_tab_count() || target == self.code_active_tab {
            return false;
        }
        // Snapshot the live buffer into the inactive vec at the
        // *current* active slot, then take the target tab out of the
        // vec and install it as the new active buffer. The inactive
        // vec ends up with: [old_inactive_before, …, snapshot of
        // previous active, …, old_inactive_after] minus the target.
        let prev_active = self.code_active_tab;
        let snapshot = EditorTab {
            meta: self.code_buffer_meta.clone(),
            buffer: self.code_buffer.clone(),
        };
        // Convert the display-list index `target` to a slot in
        // `code_tabs`. Display indices below the active map 1:1;
        // display indices above the active are off by one.
        let pop_at = if target < prev_active {
            target
        } else {
            target - 1
        };
        let target_tab = self.code_tabs.remove(pop_at);
        // Insert the snapshot at the position the previously-active
        // tab logically occupied in `code_tabs`. That's `prev_active`
        // when active was first (or in the middle below the target),
        // and `prev_active - 1` when target sat below active.
        let insert_at = if target < prev_active {
            // The active is shifting up — its old display slot is at
            // `prev_active - 1` after the removal.
            prev_active.saturating_sub(1)
        } else {
            prev_active
        };
        self.code_tabs
            .insert(insert_at.min(self.code_tabs.len()), snapshot);
        self.code_buffer = target_tab.buffer;
        self.code_buffer_meta = target_tab.meta;
        self.code_active_tab = target;
        true
    }

    /// Open a fresh untitled tab and make it active.
    pub fn new_editor_tab(&mut self) {
        self.stash_active_into_tabs();
        // Insert the new tab right after the current active.
        let insert_at = self.code_active_tab + 1;
        self.code_tabs
            .insert(insert_at.min(self.code_tabs.len()), EditorTab::untitled());
        // Pull the new tab into active.
        let new_tab = self.code_tabs.remove(insert_at.min(self.code_tabs.len()));
        self.code_buffer = new_tab.buffer;
        self.code_buffer_meta = new_tab.meta;
        self.code_active_tab = insert_at;
    }

    /// Open a file as a new tab. If the same path is already open,
    /// switches to that tab instead of opening a duplicate.
    pub fn open_editor_tab(
        &mut self,
        path: std::path::PathBuf,
        source: impl Into<String>,
        language: impl Into<String>,
    ) {
        // De-dupe: if the path is already open, just switch.
        for (idx, tab) in self.code_tabs.iter().enumerate() {
            if tab.meta.path.as_ref() == Some(&path) {
                let display_idx = if idx < self.code_active_tab {
                    idx
                } else {
                    idx + 1
                };
                self.switch_editor_tab(display_idx);
                return;
            }
        }
        if self.code_buffer_meta.path.as_ref() == Some(&path) {
            return;
        }
        self.stash_active_into_tabs();
        let mut buffer = CodeBuffer::default();
        buffer.load(source, language);
        let insert_at = self.code_active_tab + 1;
        let new_tab = EditorTab {
            meta: EditorTabMeta::for_path(path),
            buffer,
        };
        self.code_tabs
            .insert(insert_at.min(self.code_tabs.len()), new_tab);
        let pulled = self.code_tabs.remove(insert_at.min(self.code_tabs.len()));
        self.code_buffer = pulled.buffer;
        self.code_buffer_meta = pulled.meta;
        self.code_active_tab = insert_at;
    }

    /// Close the active tab. If it's the only one open, leaves a
    /// fresh untitled tab in its place (the editor always has at
    /// least one tab — the panel never goes "empty"). Returns
    /// `true` when something actually closed.
    pub fn close_active_editor_tab(&mut self) -> bool {
        if self.editor_tab_count() <= 1 {
            // Last tab — reset to a fresh untitled.
            self.code_buffer = CodeBuffer::default();
            self.code_buffer_meta = EditorTabMeta::untitled();
            self.code_active_tab = 0;
            return true;
        }
        // Pull the next tab in. Prefer the one that visually slid
        // into the active position (the one *after* the closed
        // tab); fall back to the previous tab when the active was
        // the last in the display list.
        let new_active = if self.code_active_tab < self.code_tabs.len() {
            self.code_tabs.remove(self.code_active_tab)
        } else {
            self.code_tabs.pop().unwrap()
        };
        self.code_buffer = new_active.buffer;
        self.code_buffer_meta = new_active.meta;
        if self.code_active_tab > self.code_tabs.len() {
            self.code_active_tab = self.code_tabs.len();
        }
        true
    }

    pub fn next_editor_tab(&mut self) -> bool {
        let total = self.editor_tab_count();
        if total < 2 {
            return false;
        }
        let next = (self.code_active_tab + 1) % total;
        self.switch_editor_tab(next)
    }

    pub fn prev_editor_tab(&mut self) -> bool {
        let total = self.editor_tab_count();
        if total < 2 {
            return false;
        }
        let prev = if self.code_active_tab == 0 {
            total - 1
        } else {
            self.code_active_tab - 1
        };
        self.switch_editor_tab(prev)
    }

    /// Mark the active tab as dirty. Called by every editor
    /// mutation (key route / IME commit / file load is the inverse
    /// — it clears).
    pub fn mark_active_tab_dirty(&mut self) {
        self.code_buffer_meta.dirty = true;
    }

    /// Apply a successful save: the active tab's path is now `path`
    /// and its dirty flag clears. Title rederives from the new path.
    pub fn record_active_tab_saved(&mut self, path: std::path::PathBuf) {
        self.code_buffer_meta = EditorTabMeta::for_path(path);
    }

    // ── read side ─────────────────────────────────────────────────

    /// JSON for `shell.code-editor`. Wave 11.4 migration to DSL —
    /// the binding pre-derives `lines` (one row per `\n`-split line
    /// of `source` carrying `{number, text}`), the cursor position
    /// (`cursor-line` / `cursor-column`, both 1-based, computed from
    /// the byte-offset `caret`), and `status-label` (the pre-
    /// formatted "lang · Ln X, Col Y" string the status strip
    /// renders). The DSL block iterates `lines` with a `for=` loop
    /// and renders the status strip as a single `<text>{status-label}</text>`.
    pub fn code_editor_props(&self) -> Value {
        // Render the *display* text — buffer + any active IME preedit
        // spliced in at the caret. The shell renderer underlines the
        // preedit range via the `selection` channel (the colour layer
        // already separates from caret/selection in the runtime).
        let source = self.code_buffer.editor.display_text().into_owned();
        let caret = self
            .code_buffer
            .editor
            .display_caret_byte()
            .min(source.len());
        let language = if self.code_buffer.language.is_empty() {
            "prui"
        } else {
            self.code_buffer.language.as_str()
        };
        // Per-line entries still feed the gutter (one numbered row
        // per source line). The editor body itself now renders the
        // whole buffer through a single multi-line input — the
        // shaped glyph run carries the caret + selection — so the
        // DSL no longer needs the per-line `text` payload.
        let mut line_numbers: Vec<Value> = Vec::new();
        for (idx, _) in source.split('\n').enumerate() {
            line_numbers.push(json!({ "number": (idx + 1) as i64 }));
        }
        let (cursor_line, cursor_column) =
            prism_ui_runtime::editor::byte_offset_to_line_col(&source, caret);
        // Preedit range — when present, the underlying selection
        // attribute carries it so the renderer paints a highlight
        // through the in-progress composition. Cosmic-text doesn't
        // expose a per-glyph underline directly; selection's
        // translucent fill stands in until we wire a proper
        // underline span. (User selections are suppressed while a
        // preedit is active — pressing arrows during composition
        // is OS-level UB anyway.)
        let preedit_range = self.code_buffer.editor.preedit_range();
        let status_label = if cursor_line > 0 {
            format!("{language} · Ln {cursor_line}, Col {cursor_column}")
        } else {
            language.to_string()
        };
        // Selection lowers as a comma-pair attr the runtime input
        // parser already consumes; omit when empty so the resting
        // render has no inert `data-selection="0,0"` noise.
        let selection_attr = self
            .code_buffer
            .editor
            .selection()
            .map(|(s, e)| format!("{s},{e}"));
        // Preedit rides on a separate `underline="start,end"` attr
        // so the in-progress IME composition reads as "tentative"
        // (a thin underline) without overloading the selection
        // highlight. Selection + underline coexist freely — pressing
        // arrows during composition leaves the previous selection
        // alone.
        let underline_attr = preedit_range.map(|(s, e)| format!("{s},{e}"));
        // Matching-bracket highlight — when the caret sits adjacent
        // to a bracket, the editor knows the matching partner. Emit
        // both byte offsets as a comma-separated `bracket-match`
        // attr so the runtime input parser can pick them up. Omit
        // when no match exists.
        let bracket_pair =
            self.code_buffer
                .editor
                .matching_bracket_for(caret)
                .map(|partner| {
                    let own =
                        if caret < source.len()
                            && source.as_bytes().get(caret).copied().is_some_and(|b| {
                                matches!(b, b'(' | b')' | b'[' | b']' | b'{' | b'}')
                            })
                        {
                            caret
                        } else {
                            caret.saturating_sub(1)
                        };
                    (own, partner)
                });
        let tabs = self.editor_tab_strip();
        let mut props = json!({
            "source": source,
            "caret": caret,
            "caret-byte": caret,
            "language": language,
            "lines": line_numbers,
            "cursor-line": cursor_line,
            "cursor-column": cursor_column,
            "status-label": status_label,
            "scroll-x": self.code_buffer.scroll_x,
            "scroll-y": self.code_buffer.scroll_y,
            "highlight-current-line": true,
            "tabs": tabs,
            "active-tab": self.code_active_tab,
        });
        if let Some(sel) = selection_attr {
            props["selection"] = Value::String(sel);
        }
        if let Some(ul) = underline_attr {
            props["underline"] = Value::String(ul);
        }
        if let Some((a, b)) = bracket_pair {
            props["bracket-match"] = Value::String(format!("{a},{b}"));
        }
        props
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
        // Wave 11.4 — substrate fields the DSL `shell.builder-canvas`
        // consumes alongside the geometry props above.
        //
        // * `handle-directions` — the closed 8-row direction list the
        //   resize-handle ring iterates. The DSL block uses it via
        //   `for="dir in handle-directions"` so adding a 9th handle
        //   (e.g. a center pivot) is one row here, no DSL change.
        // * `gizmo-tag` — pre-resolved tag dispatched into the
        //   `<dispatch component="{gizmo-tag}"/>` element. Empty
        //   when the tool is something other than move/rotate/scale.
        // * `show-gizmo` — boolean visibility flag mirroring the
        //   selection presence. The previous Rust block read this
        //   prop verbatim.
        props["handle-directions"] = json!(["tl", "t", "tr", "r", "br", "b", "bl", "l"]);
        props["gizmo-tag"] = json!(match self.tool {
            ToolMode::Move => "shell.gizmo-move",
            ToolMode::Rotate => "shell.gizmo-rotate",
            ToolMode::Scale => "shell.gizmo-scale",
        });
        props["show-gizmo"] = json!(self.selection.is_some() && self.selection_bbox.is_some());
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
        // Facet templates are now real `node.children` — the facet
        // block's own `lower_ui` repeats them per data item. No canvas
        // pre-pass: the normal walk renders + tags them like any node.
        let lower_root: &prism_builder::Node = root;
        let cascade = prism_builder::StyleProperties::default();
        let mut ctx = prism_builder::ui_lower::LowerCtx::new(Some(reg), &cascade)
            .with_bindings(&self.bindings);
        if let Some(inv) = invalidator {
            ctx = ctx.with_block_invalidator(inv);
        }
        if let Some(mod_reg) = modifier_registry {
            ctx = ctx.with_modifier_registry(mod_reg);
        }
        vec![ctx.lower(lower_root)]
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
    pub(crate) fn gizmo_props(&self, kind: ToolMode) -> Value {
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

    pub(crate) fn handle_positions_json(&self) -> Value {
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

    pub(crate) fn candidates_json(&self) -> Value {
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
    pub(crate) fn apply_gizmo_delta(&mut self, tool: ToolMode, dx: f32, dy: f32) {
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

    pub(crate) fn apply_handle_delta(&mut self, side: HandleSide, dx: f32, dy: f32) {
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
    pub(crate) fn hit_test(&self, x: f32, y: f32) -> Option<DragKind> {
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
