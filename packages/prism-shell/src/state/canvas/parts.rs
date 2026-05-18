//! Canvas standalone types + canvas-tree helper fns. Split from
//! `canvas/mod.rs` (Phase B.4). `use super::*` ties back to
//! `CanvasSlot` + the rest of `state`.

use super::*;

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
    pub(crate) fn as_str(self) -> &'static str {
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

/// Per-tab metadata — path on disk, display title, dirty flag.
/// Sits *next to* [`CodeBuffer`] (which carries the live editor
/// state) so the buffer type stays free of file-system concerns.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EditorTabMeta {
    /// Filesystem path the tab loaded from / saves to. `None` for
    /// untitled scratch buffers and freshly-opened `editor.file.new`
    /// tabs.
    pub path: Option<std::path::PathBuf>,
    /// Display label shown on the tab pill. Defaults to the path's
    /// file stem or `"Untitled"` for untitled tabs.
    pub title: String,
    /// `true` when the buffer has changes that haven't been flushed
    /// to disk. Cleared on save / load; set by any edit through the
    /// shell's mutator surface.
    pub dirty: bool,
}

impl EditorTabMeta {
    pub fn untitled() -> Self {
        Self {
            path: None,
            title: "Untitled".into(),
            dirty: false,
        }
    }

    /// Construct meta from a path; the title defaults to the file's
    /// stem (or full file_name when no stem can be extracted).
    pub fn for_path(path: std::path::PathBuf) -> Self {
        let title = path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| "Untitled".into());
        Self {
            path: Some(path),
            title,
            dirty: false,
        }
    }
}

/// One inactive open file. Pairs [`EditorTabMeta`] (path / title /
/// dirty) with [`CodeBuffer`] (the buffer's caret / selection /
/// undo / scroll). The shell snapshots the active tab into one of
/// these on every switch, so cycling through tabs round-trips every
/// editor concern.
#[derive(Clone, Debug)]
pub struct EditorTab {
    pub meta: EditorTabMeta,
    pub buffer: CodeBuffer,
}

impl EditorTab {
    pub fn untitled() -> Self {
        Self {
            meta: EditorTabMeta::untitled(),
            buffer: CodeBuffer::default(),
        }
    }
}

/// Backing buffer for the in-shell code editor. Owns a multi-line
/// `TextEditor` plus a language tag (used by the status strip + the
/// future syntax-highlight pass). Single editor type powers both
/// this and `FieldFocus`, so the same caret / selection / undo /
/// key-handling code paths drive every editable text surface in the
/// shell.
#[derive(Clone, Debug)]
pub struct CodeBuffer {
    pub editor: prism_ui_runtime::editor::TextEditor,
    pub language: String,
    /// Horizontal viewport scroll in CSS pixels — auto-updated by
    /// [`Self::ensure_caret_visible`] after edits / nav so the
    /// caret stays inside the editor's text area, and by the wheel
    /// event path for explicit user scrolling. Clamped to
    /// non-negative.
    pub scroll_x: f32,
    pub scroll_y: f32,
    /// Cached syntax-highlight spans keyed by a `(text, language)`
    /// fingerprint. The render walk lifts spans straight off this
    /// cache instead of re-running the tokenizer on every frame —
    /// once a buffer is tokenized, idle redraws (caret blink, scroll,
    /// hover) cost zero tokenization work. Invalidated by any
    /// mutation that changes the text or the language.
    pub(crate) cached_spans: std::cell::RefCell<SpansCache>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct SpansCache {
    /// `(text_hash, language)` fingerprint of the cache entry. A
    /// real hash (not just length + endpoints) so mid-buffer edits
    /// that preserve length still bust the cache. `None` means the
    /// cache is empty.
    pub(crate) fingerprint: Option<(u64, String)>,
    pub(crate) spans: Vec<prism_ui_runtime::command::TextSpan>,
}

impl Default for CodeBuffer {
    fn default() -> Self {
        Self {
            editor: prism_ui_runtime::editor::TextEditor::new_multi_line(),
            language: String::new(),
            scroll_x: 0.0,
            scroll_y: 0.0,
            cached_spans: std::cell::RefCell::new(SpansCache::default()),
        }
    }
}

impl CodeBuffer {
    pub fn source(&self) -> &str {
        self.editor.text()
    }

    pub fn caret(&self) -> usize {
        self.editor.caret_byte()
    }

    /// Replace the entire buffer (e.g. on file open). Resets undo
    /// history *and* the viewport scroll — the new content is the
    /// new baseline. Invalidates the syntax-highlight cache. Turns
    /// on bracket-pair auto-close so the editor behaves like a
    /// proper code surface.
    pub fn load(&mut self, source: impl Into<String>, language: impl Into<String>) {
        self.editor.set_text(source);
        self.editor.set_bracket_pairs(true);
        self.language = language.into();
        self.scroll_x = 0.0;
        self.scroll_y = 0.0;
        self.cached_spans.borrow_mut().fingerprint = None;
    }

    /// Toggle line comment using the language's prefix, falling back
    /// to `--` for unknown languages so the gesture still does
    /// something visible.
    pub fn toggle_line_comment(&mut self) -> bool {
        let prefix = prism_ui_runtime::editor::line_comment_prefix(&self.language).unwrap_or("--");
        self.editor.toggle_line_comment(prefix).mutated()
    }

    /// Highlight spans for the current buffer + language. Memoised:
    /// re-uses the previous span list when neither the buffer nor
    /// the language have changed (the typical case once a buffer
    /// has been tokenized once and is repainting on caret blinks /
    /// scroll). Returns a clone of the cached list — the runtime
    /// node consumes it by value.
    pub fn highlight_spans(&self) -> Vec<prism_ui_runtime::command::TextSpan> {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.editor.text().hash(&mut hasher);
        let text_hash = hasher.finish();
        {
            let cache = self.cached_spans.borrow();
            if let Some((hash, lang)) = &cache.fingerprint {
                if *hash == text_hash && lang == &self.language {
                    return cache.spans.clone();
                }
            }
        }
        // Cache miss — retokenize.
        let language = if self.language.is_empty() {
            "luau"
        } else {
            self.language.as_str()
        };
        let spans = prism_ui_runtime::syntax::highlight(self.editor.text(), language);
        let mut cache = self.cached_spans.borrow_mut();
        cache.fingerprint = Some((text_hash, self.language.clone()));
        cache.spans = spans.clone();
        spans
    }

    /// Auto-scroll the viewport so the caret stays visible after an
    /// edit or navigation. `viewport_width` / `viewport_height` are
    /// the editor's text-area dimensions in CSS pixels (the input's
    /// inner area, padding subtracted). Returns `true` when the
    /// scroll changed.
    ///
    /// The estimate uses the same `font_size * 0.55` / `font_size *
    /// 1.2` heuristic the runtime measure pass uses — perfect for
    /// the monospace font the code editor renders with, and good
    /// enough for variable-width fallbacks.
    pub fn ensure_caret_visible(
        &mut self,
        viewport_width: f32,
        viewport_height: f32,
        font_size: f32,
    ) -> bool {
        let (line, col) = self.editor.caret_line_col();
        if line == 0 {
            return false;
        }
        let caret_x = (col.saturating_sub(1)) as f32 * font_size * 0.55;
        let caret_y_top = (line.saturating_sub(1)) as f32 * font_size * 1.2;
        let caret_y_bottom = caret_y_top + font_size * 1.2;
        let prev_x = self.scroll_x;
        let prev_y = self.scroll_y;
        // Horizontal: scroll right if caret is past the right edge;
        // scroll left if it's behind the left edge. Leave a small
        // gutter so the caret doesn't sit flush against either side.
        let h_gutter = font_size * 2.0;
        if caret_x > self.scroll_x + viewport_width - h_gutter {
            self.scroll_x = (caret_x - viewport_width + h_gutter).max(0.0);
        } else if caret_x < self.scroll_x + h_gutter {
            self.scroll_x = (caret_x - h_gutter).max(0.0);
        }
        // Vertical: scroll down if the caret's row would land below
        // the viewport bottom; scroll up otherwise. One-line gutter
        // top + bottom.
        let v_gutter = font_size * 1.2;
        if caret_y_bottom > self.scroll_y + viewport_height - v_gutter {
            self.scroll_y = (caret_y_bottom - viewport_height + v_gutter).max(0.0);
        } else if caret_y_top < self.scroll_y + v_gutter {
            self.scroll_y = (caret_y_top - v_gutter).max(0.0);
        }
        self.scroll_x != prev_x || self.scroll_y != prev_y
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DragKind {
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
    pub(crate) fn cursor(self) -> &'static str {
        match self {
            Self::TopLeft | Self::BottomRight => "nwse-resize",
            Self::TopRight | Self::BottomLeft => "nesw-resize",
            Self::Top | Self::Bottom => "ns-resize",
            Self::Left | Self::Right => "ew-resize",
        }
    }

    pub(crate) fn id(self) -> &'static str {
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
    pub(crate) fn deltas(self) -> (f32, f32, f32, f32) {
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
    pub(crate) fn capture(doc: &BuilderDocument, selection: Option<&str>) -> Option<Self> {
        let id = selection?;
        let node = doc.root.as_ref()?.find(id)?;
        Some(Self {
            node_id: node.id.clone(),
            transform: node.transform.clone(),
        })
    }
}

#[derive(Clone, Debug)]
pub(crate) struct DragState {
    pub(crate) kind: DragKind,
    pub(crate) snapshot: TransformSnapshot,
    pub(crate) origin: (f32, f32),
}

/// Recursively rewrite ids in `node` so they don't collide with any
/// id already present in `doc`. Stable suffix scheme: `<old>-<tag>-<n>`.
pub(crate) fn rename_subtree(node: &mut prism_builder::Node, doc: &BuilderDocument, tag: &str) {
    let mut existing: std::collections::HashSet<String> = std::collections::HashSet::new();
    if let Some(root) = doc.root.as_ref() {
        collect_ids(root, &mut existing);
    }
    rewrite_ids(node, tag, &mut existing);
}

pub(crate) fn collect_ids(node: &prism_builder::Node, out: &mut std::collections::HashSet<String>) {
    out.insert(node.id.clone());
    for c in &node.children {
        collect_ids(c, out);
    }
}

pub(crate) fn rewrite_ids(
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
pub(crate) fn sanitise_id(label: &str) -> String {
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
pub(crate) fn uniquify_connection_id(existing: &[SignalConnection], raw: &str) -> String {
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
pub(crate) fn resize_delta_signs(dir: &str) -> (f32, f32) {
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
pub(crate) fn canvas_context_menu_items(state: &AppState) -> Vec<MenuItem> {
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
pub(crate) fn push_under(
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

/// Wave 3.2 — `kind` → serializable `Node` template. Every palette
/// entry (including `card`, now a `BlockSpec` rather than a prefab —
/// §4.4) drops as a single vanilla `Node`; the block's own `lower_ui`
/// fills in chrome from schema defaults when props are empty, so the
/// dropped node renders sensibly on the first frame.
pub(crate) fn palette_node_template(kind: &str) -> Option<Value> {
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
pub(crate) fn insert_under_parent(
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

pub(crate) fn delete_under(parent: &mut prism_builder::Node, target_id: &str) -> bool {
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
pub(crate) fn swap_sibling_under(
    parent: &mut prism_builder::Node,
    target_id: &str,
    dir: i32,
) -> bool {
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
