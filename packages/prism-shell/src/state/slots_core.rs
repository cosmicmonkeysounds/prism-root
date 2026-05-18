//! ProjectSlot / DevToolsSlot / SearchSlot / ChromeSlot / WorkspaceSlot.
//! Split out of `state/mod.rs` (Phase B.4). `use super::*`
//! inherits intra-`state` types + crate imports; cross-module
//! callers reach widened `pub(crate)` items.

use super::*;

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

// ── devtools / inspector ──────────────────────────────────────────

/// IDE-mode Phase 4 — `shell.devtools` panel state.
///
/// The panel hosts four lenses; only one renders at a time. The
/// `active_lens` field drives which one. The probe + presence buffers
/// are append-only history (capped FIFO) populated by the host:
/// future router wiring will fire probes off `data-probe-*` pointer
/// hits; future presence service ingest will push remote-peer
/// snapshots from the `PresenceManager` event bus. Today both are
/// seeded by tests + scenes so the panel renders with realistic data.
///
/// The `filter` field is a single-line [`TextEditor`] routed through
/// the declarative text-input system (one
/// [`TextInputDeclaration`](crate::services::text_input::TextInputDeclaration)
/// row) — it filters the probe / binding lists by substring.
#[derive(Clone, Debug, Default)]
pub struct DevToolsSlot {
    pub active_lens: DevToolsLens,
    /// Last-N probe events. New events push to the back; the buffer
    /// caps at [`Self::PROBE_BUFFER_LIMIT`].
    pub probes: std::collections::VecDeque<ProbeEvent>,
    /// Remote peer presence snapshots. The presence-service ingest
    /// (future) replaces stale entries by `peer_id`; today the buffer
    /// is whatever the scene seeded.
    pub presence: Vec<PresencePeer>,
    /// Filter substring — applied to probe names + binding tags +
    /// presence display names. Empty = no filtering.
    pub filter: prism_ui_runtime::editor::TextEditor,
    /// When `true` the panel is in keyboard-focus (typing routes to
    /// the filter field through the declarative dispatch). Flipped by
    /// the filter input's click route.
    pub filter_focused: bool,
    /// Cached snapshot of `ShellPropBindings::snapshot` at the last
    /// render. The bindings lens reads from this. Populated by the
    /// `shell.devtools` prop binding closure each frame.
    pub binding_snapshots: indexmap::IndexMap<String, serde_json::Value>,
}

impl DevToolsSlot {
    /// FIFO cap for the probe stream. Keeps memory bounded; recent
    /// events stay visible while older ones drop off the back.
    pub const PROBE_BUFFER_LIMIT: usize = 200;

    /// Record one probe event. Appends to the buffer, evicting the
    /// oldest if we'd cross the limit. Future event-router wiring
    /// calls this with the live (name, payload, source) tuple.
    pub fn record_probe(&mut self, event: ProbeEvent) {
        self.probes.push_back(event);
        while self.probes.len() > Self::PROBE_BUFFER_LIMIT {
            self.probes.pop_front();
        }
    }

    /// Clear every recorded probe event. Bound to a
    /// `devtools.clear-probes` command.
    pub fn clear_probes(&mut self) {
        self.probes.clear();
    }

    /// IDE Phase D / cross-cutting §4.3 — ingest one
    /// `prism_core::network::presence::PresenceChange` into the
    /// presence lens. `Joined`/`Updated` replace-or-insert by
    /// `peer_id`; `Left` removes. `now_ms` is the host's monotonic
    /// clock so the row's "last seen" is render-friendly. Returns
    /// `true` when the buffer actually changed.
    pub fn apply_presence_change(
        &mut self,
        change: &prism_core::network::presence::PresenceChange,
        now_ms: u64,
    ) -> bool {
        use prism_core::network::presence::PresenceChangeKind;
        match change.kind {
            PresenceChangeKind::Joined | PresenceChangeKind::Updated => {
                let Some(st) = change.state.as_ref() else {
                    return false;
                };
                let selection = st
                    .selections
                    .first()
                    .map(|s| s.object_id.clone())
                    .or_else(|| st.cursor.as_ref().map(|c| c.object_id.clone()));
                let peer = PresencePeer {
                    peer_id: st.identity.peer_id.clone(),
                    display_name: st.identity.display_name.clone(),
                    color: st.identity.color.clone(),
                    selection,
                    active_view: st.active_view.clone(),
                    last_seen_ms: now_ms,
                };
                match self.presence.iter_mut().find(|p| p.peer_id == peer.peer_id) {
                    Some(slot) => {
                        if *slot == peer {
                            return false;
                        }
                        *slot = peer;
                    }
                    None => self.presence.push(peer),
                }
                true
            }
            PresenceChangeKind::Left => {
                let before = self.presence.len();
                self.presence.retain(|p| p.peer_id != change.peer_id);
                before != self.presence.len()
            }
        }
    }

    /// Switch lenses. Side-effect-free — the binding closure does the
    /// rendering work.
    pub fn switch_lens(&mut self, lens: DevToolsLens) {
        self.active_lens = lens;
    }

    /// Filter text projected from the underlying editor buffer.
    pub fn filter_text(&self) -> &str {
        self.filter.text()
    }

    /// JSON snapshot for `shell.devtools`. Renders the tab strip,
    /// the active lens body, and the filter field. Each lens is a
    /// data-driven list — the DSL's `for` loop walks the array.
    pub fn devtools_props(&self, doc: &prism_builder::BuilderDocument) -> Value {
        let filter = self.filter_text().to_lowercase();
        let active = self.active_lens.id();
        let tabs = json!([
            { "tab-id": "document",  "label": "Document",  "active": active == "document" },
            { "tab-id": "presence",  "label": "Presence",  "active": active == "presence" },
            { "tab-id": "probes",    "label": "Probes",    "active": active == "probes" },
            { "tab-id": "bindings",  "label": "Bindings",  "active": active == "bindings" },
        ]);
        let body = match self.active_lens {
            DevToolsLens::Document => json!({
                "kind": "document",
                "items": Value::Array(flatten_doc_tree(doc.root.as_ref())),
            }),
            DevToolsLens::Presence => json!({
                "kind": "presence",
                "items": Value::Array(self.presence_items(&filter)),
            }),
            DevToolsLens::Probes => json!({
                "kind": "probes",
                "items": Value::Array(self.probe_items(&filter)),
            }),
            DevToolsLens::Bindings => json!({
                "kind": "bindings",
                "items": Value::Array(self.binding_items(&filter)),
            }),
        };
        json!({
            "active-lens": active,
            "tabs": tabs,
            "body": body,
            "filter": self.filter_text(),
            "filter-caret": self.filter.caret_byte(),
            "filter-focused": self.filter_focused,
        })
    }

    pub(crate) fn presence_items(&self, filter: &str) -> Vec<Value> {
        self.presence
            .iter()
            .filter(|p| filter.is_empty() || p.display_name.to_lowercase().contains(filter))
            .map(|p| {
                json!({
                    "peer-id": p.peer_id,
                    "display-name": p.display_name,
                    "color": p.color,
                    "selection": p.selection.clone().unwrap_or_default(),
                    "active-view": p.active_view.clone().unwrap_or_default(),
                    "last-seen-ms": p.last_seen_ms,
                })
            })
            .collect()
    }

    pub(crate) fn probe_items(&self, filter: &str) -> Vec<Value> {
        self.probes
            .iter()
            .rev() // newest first
            .filter(|e| filter.is_empty() || e.name.to_lowercase().contains(filter))
            .map(|e| {
                json!({
                    "name": e.name,
                    "payload": e.payload,
                    "timestamp-ms": e.timestamp_ms,
                    "source-node-id": e.source_node_id.clone().unwrap_or_default(),
                })
            })
            .collect()
    }

    pub(crate) fn binding_items(&self, filter: &str) -> Vec<Value> {
        // The Bindings lens enumerates every registered shell binding
        // tag from `crate::props::builtin_binding_tags()` (the
        // SLOT_BINDINGS table). The cached snapshot (if populated by
        // a future host hook) wins; otherwise the value row is empty
        // — the tag-list view alone is enough for "are my bindings
        // even registered" debugging.
        crate::props::builtin_binding_tags()
            .into_iter()
            .filter(|tag| filter.is_empty() || tag.to_lowercase().contains(filter))
            .map(|tag| {
                let value = self
                    .binding_snapshots
                    .get(tag)
                    .cloned()
                    .unwrap_or(serde_json::Value::Null);
                json!({
                    "tag": tag,
                    "value": value,
                })
            })
            .collect()
    }
}

/// Which inspector lens is currently rendered. Mirrors the four-tab
/// surface: `Document` = CRDT / builder tree, `Presence` = remote
/// peers, `Probes` = probe event stream, `Bindings` = live shell
/// binding snapshots.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DevToolsLens {
    #[default]
    Document,
    Presence,
    Probes,
    Bindings,
}

impl DevToolsLens {
    /// Stable kebab-case id matching the tab data attribute.
    pub fn id(self) -> &'static str {
        match self {
            Self::Document => "document",
            Self::Presence => "presence",
            Self::Probes => "probes",
            Self::Bindings => "bindings",
        }
    }

    /// Parse from a `data-tab-id` attribute value. Returns `None` for
    /// unknown ids so the click router can short-circuit safely.
    pub fn from_id(s: &str) -> Option<Self> {
        match s {
            "document" => Some(Self::Document),
            "presence" => Some(Self::Presence),
            "probes" => Some(Self::Probes),
            "bindings" => Some(Self::Bindings),
            _ => None,
        }
    }
}

/// One probe event captured from the runtime / router. `name` is the
/// `prism.probes:on(name, …)` registration key; `payload` is the
/// JSON value the firing site emitted; `source_node_id` is the
/// `data-probe-source` (or hit-tested node id) when the event was
/// fired from a pointer interaction.
#[derive(Clone, Debug, PartialEq)]
pub struct ProbeEvent {
    pub name: String,
    pub payload: serde_json::Value,
    pub timestamp_ms: u64,
    pub source_node_id: Option<String>,
}

/// Snapshot of one remote peer's presence. Future
/// `PresenceService::ingest` will replace these per
/// `prism_core::network::presence::PresenceChange` event.
#[derive(Clone, Debug, PartialEq)]
pub struct PresencePeer {
    pub peer_id: String,
    pub display_name: String,
    /// CSS-ish color string (e.g. `"#4a90e2"`). The presence overlay
    /// uses this to tint each peer's cursor; the devtools panel uses
    /// it for the swatch in the peer row.
    pub color: String,
    /// Optional selected canvas node id — mirrors the local
    /// `state.canvas.selection`.
    pub selection: Option<String>,
    /// Optional active panel / view tag, for "which lens is the peer
    /// looking at" awareness.
    pub active_view: Option<String>,
    pub last_seen_ms: u64,
}

/// Flatten a builder document into a depth-encoded list of rows.
/// Reused by the Document lens; matches the inspector-tree row shape
/// (label / id / depth) so the same row-render block can be reused.
pub(crate) fn flatten_doc_tree(root: Option<&prism_builder::Node>) -> Vec<Value> {
    fn walk(node: &prism_builder::Node, depth: u32, out: &mut Vec<Value>) {
        let label = if node.id.is_empty() {
            node.component.clone()
        } else {
            format!("{} · {}", node.component, node.id)
        };
        out.push(json!({
            "node-id": node.id,
            "label": label,
            "depth": depth,
            "component": node.component,
        }));
        for child in &node.children {
            walk(child, depth + 1, out);
        }
    }
    let mut out = Vec::new();
    if let Some(root) = root {
        walk(root, 0, &mut out);
    }
    out
}

// ── search ────────────────────────────────────────────────────────

/// Search overlay state — query buffer, ranked hits, modal-open flag.
/// `SearchService` owns every mutator; bindings only read.
///
/// The `query` field is a full [`TextEditor`] (single-line) so the
/// overlay gets caret, selection, arrow nav, Ctrl+A, IME, and
/// clipboard for free — the same engine the property-row fields and
/// the code-editor panel use. Read access is through
/// [`Self::query_text`] which projects the underlying buffer.
///
/// See `docs/dev/clay-migration-plan.md` §26.
/// What the find overlay searches. `Document` is the original
/// builder-tree scorer; `Project` (IDE Phase 6) greps every text
/// file in `state.catalog.files`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchScope {
    #[default]
    Document,
    Project,
}

impl SearchScope {
    pub fn label(&self) -> &'static str {
        match self {
            SearchScope::Document => "Document",
            SearchScope::Project => "Project",
        }
    }

    pub fn toggled(&self) -> Self {
        match self {
            SearchScope::Document => SearchScope::Project,
            SearchScope::Project => SearchScope::Document,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct SearchSlot {
    pub open: bool,
    pub query: prism_ui_runtime::editor::TextEditor,
    pub results: Vec<SearchHit>,
    pub selected_index: usize,
    pub scope: SearchScope,
    /// IDE Phase 6 — the replacement string (project scope only).
    pub replace: prism_ui_runtime::editor::TextEditor,
    /// `true` while the replace field owns the keyboard. Routes the
    /// two single-line inputs to disjoint text-input declarations so
    /// they don't both claim a keystroke.
    pub replace_focused: bool,
}

#[derive(Clone, Debug, Default)]
pub struct SearchHit {
    /// Builder `NodeId` for `Document`-scope hits; empty for project
    /// file hits (those carry `path` + `offset` instead).
    pub node_id: String,
    pub label: String,
    pub snippet: String,
    pub score: f32,
    /// `Project`-scope hits carry the file + byte offset of the match
    /// so activation routes through `editor_files::open_at_offset`.
    pub path: Option<std::path::PathBuf>,
    pub offset: usize,
}

impl SearchSlot {
    /// Current query text — projected from the underlying
    /// [`TextEditor`] buffer.
    pub fn query_text(&self) -> &str {
        self.query.text()
    }

    pub fn replace_text(&self) -> &str {
        self.replace.text()
    }

    pub fn search_overlay_props(&self) -> Value {
        let mut props = json!({
            "open": self.open,
            "query": self.query_text(),
            "caret": self.query.caret_byte(),
            "selected-index": self.selected_index,
            "scope": self.scope.label(),
            "project-scope": self.scope == SearchScope::Project,
            "replace": self.replace_text(),
            "replace-caret": self.replace.caret_byte(),
            "replace-focused": self.replace_focused,
            "results": Value::Array(
                self.results.iter().map(|h| json!({
                    "node-id": h.node_id,
                    "label": h.label,
                    "snippet": h.snippet,
                    "score": h.score,
                    "path": h.path.as_ref().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default(),
                    "offset": h.offset,
                    "has-path": h.path.is_some(),
                })).collect()
            ),
        });
        // Optional `selection="start,end"` byte range — only emitted
        // when a non-empty selection is live so the input renderer's
        // missing-attr branch (no highlight) is the default.
        if let Some((a, b)) = self.query.selection() {
            props["selection"] = json!(format!("{a},{b}"));
        }
        if let Some((a, b)) = self.replace.selection() {
            props["replace-selection"] = json!(format!("{a},{b}"));
        }
        props
    }
}

// ── symbol index (IDE Phase 2) ────────────────────────────────────

/// IDE-mode Phase 2 — the project-wide Luau symbol table plus the
/// `shell.symbol-palette` UI state (Ctrl+Shift+O "Go to Symbol").
///
/// `symbols` is rebuilt per-file on save (`editor.file.save`) and
/// wholesale on `project.open-folder`. The palette is a single-line
/// [`TextEditor`] routed through the declarative text-input system,
/// exactly like search / the command palette; `results` is the cached
/// fuzzy projection the block renders, refreshed on every keystroke.
#[derive(Clone, Debug, Default)]
pub struct IndexSlot {
    pub symbols: prism_core::language::symbol_index::SymbolIndex,
    pub palette_open: bool,
    pub query: prism_ui_runtime::editor::TextEditor,
    pub results: Vec<prism_core::language::symbol_index::Symbol>,
    pub selected_index: usize,
}

impl IndexSlot {
    /// Largest result set the palette renders — keeps the list bounded
    /// regardless of project size.
    pub const RESULT_LIMIT: usize = 50;

    pub fn query_text(&self) -> &str {
        self.query.text()
    }

    /// Recompute `results` from the current query against the live
    /// index. An empty query lists everything (capped) so opening the
    /// palette shows the whole symbol surface.
    pub fn refresh_results(&mut self) {
        self.results = self
            .symbols
            .fuzzy(self.query_text(), Self::RESULT_LIMIT)
            .into_iter()
            .cloned()
            .collect();
        if self.selected_index >= self.results.len() {
            self.selected_index = self.results.len().saturating_sub(1);
        }
    }

    pub fn symbol_palette_props(&self) -> Value {
        let mut props = json!({
            "open": self.palette_open,
            "query": self.query_text(),
            "caret": self.query.caret_byte(),
            "selected-index": self.selected_index,
            "results": Value::Array(
                self.results.iter().map(|s| json!({
                    "name": s.name,
                    "kind": format!("{:?}", s.kind).to_lowercase(),
                    "path": s.path.to_string_lossy(),
                    "line": s.line,
                    "offset": s.offset,
                })).collect()
            ),
        });
        if let Some((a, b)) = self.query.selection() {
            props["selection"] = json!(format!("{a},{b}"));
        }
        props
    }
}

// ── diagnostics (IDE Phase 3) ─────────────────────────────────────

/// One Luau diagnostic resolved to a jump target. `severity` is a
/// lowercase string so the panel DSL can key a colour off it without
/// a renderer-side enum.
#[derive(Clone, Debug)]
pub struct DiagEntry {
    pub message: String,
    pub severity: &'static str,
    /// 1-based line / 0-based column of the diagnostic's start.
    pub line: usize,
    pub column: usize,
    /// Byte offset of the start — the jump target.
    pub offset: usize,
}

/// IDE-mode Phase 3 — the project-wide Luau "Problems" table, keyed
/// by file so one file rebuilds on save. Squiggle rendering is
/// femtovg-blocked (no wavy-underline primitive); this is the
/// panel-list half (ide-mode-plan.md open question (b)), refreshed on
/// the same cadence as the symbol index.
#[derive(Clone, Debug, Default)]
pub struct DiagnosticsSlot {
    pub per_file: std::collections::BTreeMap<std::path::PathBuf, Vec<DiagEntry>>,
}

impl DiagnosticsSlot {
    /// Re-diagnose one file via `LuauSyntaxProvider`. Clean files are
    /// dropped from the map so the panel only lists files with
    /// problems.
    pub fn rebuild_file(&mut self, path: impl Into<std::path::PathBuf>, source: &str) {
        use prism_core::language::syntax::{pos_at, DiagnosticSeverity, SyntaxProvider};
        let path = path.into();
        let provider = prism_core::language::luau::LuauSyntaxProvider::new();
        let diags = provider.diagnose(source, None);
        if diags.is_empty() {
            self.per_file.remove(&path);
            return;
        }
        let mut rows: Vec<DiagEntry> = diags
            .into_iter()
            .map(|d| {
                let p = pos_at(source, d.range.start);
                DiagEntry {
                    message: d.message,
                    severity: match d.severity {
                        DiagnosticSeverity::Error => "error",
                        DiagnosticSeverity::Warning => "warning",
                        DiagnosticSeverity::Info => "info",
                        DiagnosticSeverity::Hint => "hint",
                    },
                    line: p.line,
                    column: p.column,
                    offset: d.range.start,
                }
            })
            .collect();
        rows.sort_by_key(|r| r.offset);
        self.per_file.insert(path, rows);
    }

    pub fn remove_file(&mut self, path: &std::path::Path) {
        self.per_file.remove(path);
    }

    pub fn clear(&mut self) {
        self.per_file.clear();
    }

    pub fn total(&self) -> usize {
        self.per_file.values().map(|v| v.len()).sum()
    }

    pub fn error_count(&self) -> usize {
        self.per_file
            .values()
            .flatten()
            .filter(|d| d.severity == "error")
            .count()
    }

    pub fn diagnostics_panel_props(&self) -> Value {
        let mut rows: Vec<Value> = Vec::new();
        for (path, diags) in &self.per_file {
            let file = path
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.to_string_lossy().into_owned());
            for d in diags {
                rows.push(json!({
                    "path": path.to_string_lossy(),
                    "file": file,
                    "line": d.line,
                    "column": d.column,
                    "offset": d.offset,
                    "severity": d.severity,
                    "message": d.message,
                    "label": format!("{file}:{}", d.line),
                }));
            }
        }
        json!({
            "total": self.total(),
            "errors": self.error_count(),
            "summary": if rows.is_empty() {
                "No problems".to_string()
            } else {
                format!("{} problem(s), {} error(s)", self.total(), self.error_count())
            },
            "rows": Value::Array(rows),
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

    pub(crate) fn menus_json(&self) -> Value {
        Value::Array(
            self.menus
                .iter()
                .map(|m| json!({ "id": m.id, "label": m.label }))
                .collect(),
        )
    }

    pub(crate) fn nav_buttons_json(&self) -> Value {
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
    pub(crate) fn active_menu_index(&self) -> i64 {
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
        // Wave 11.3 — emit the same enriched shape the catalog
        // variant produces, just with empty `content-tag` /
        // `tabs` placeholders. Keeping a single JSON shape on
        // both paths means the DSL `shell.dock-node` block reads
        // identical fields whether a catalog is wired or not
        // (headless tests run through this no-catalog path).
        let enriched =
            enrich_dock_node(&self.workspace.active_dock().root, &DockCatalog::default());
        json!({ "dock": enriched })
    }

    /// Catalog-enriched variant: emits the same `dock` tree plus two
    /// sidecar maps `labels` (panel-id → friendly label) and `tags`
    /// (panel-id → shell content tag). The lower fn reads from these
    /// instead of consulting a catalog at lower time, so app-registered
    /// panels surface in the dock with no further plumbing. See
    /// `docs/dev/dsl-self-bootstrap.md` Loop 2 / Loop 4.
    pub fn dock_workspace_props_with_catalog(&self, catalog: &DockCatalog) -> Value {
        let root = &self.workspace.active_dock().root;
        let enriched = enrich_dock_node(root, catalog);
        let mut labels = serde_json::Map::new();
        let mut tags = serde_json::Map::new();
        for panel_id in collect_panel_ids(root) {
            if let Some(p) = catalog.get(&panel_id) {
                labels.insert(panel_id.clone(), Value::String(p.label.to_string()));
                if let Some(t) = p.tag {
                    tags.insert(panel_id, Value::String(t.to_string()));
                }
            }
        }
        json!({ "dock": enriched, "labels": labels, "tags": tags })
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

    pub(crate) fn pages_json(&self) -> Value {
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

/// Wave 11.3 — recursive enrichment of a `DockNode` JSON shape with
/// the pre-resolved fields the DSL `shell.dock-node` block consumes:
/// for every `TabGroup` leaf we add `panel-id` (active tab),
/// `content-tag` (catalog lookup), and `tabs` (label-annotated tab
/// row for multi-tab groups). Splits round-trip with their existing
/// `axis` / `ratio` / `first` / `second` fields. Without this
/// enrichment the DSL block would need array-indexing
/// (`node.tabs[node.active]`) and per-frame catalog lookups, neither
/// of which is in scope for the runtime's expression evaluator.
pub(crate) fn enrich_dock_node(node: &DockNode, catalog: &DockCatalog) -> Value {
    match node {
        DockNode::Split {
            axis,
            ratio,
            first,
            second,
        } => json!({
            "type": "split",
            "axis": match axis {
                prism_dock::Axis::Horizontal => "horizontal",
                prism_dock::Axis::Vertical => "vertical",
            },
            "ratio": ratio,
            "first": enrich_dock_node(first, catalog),
            "second": enrich_dock_node(second, catalog),
        }),
        DockNode::TabGroup { tabs, active } => {
            let active_idx = *active;
            let panel_id = tabs
                .get(active_idx)
                .cloned()
                .unwrap_or_else(|| tabs.first().cloned().unwrap_or_default());
            let content_tag = catalog
                .get(&panel_id)
                .and_then(|p| p.tag)
                .map(str::to_string)
                .unwrap_or_default();
            let tab_entries = if tabs.len() > 1 {
                tabs.iter()
                    .enumerate()
                    .map(|(i, t)| {
                        let label = catalog
                            .get(t)
                            .map(|p| p.label.to_string())
                            .unwrap_or_else(|| t.clone());
                        json!({
                            "tab-id": t,
                            "label": label,
                            "active": i == active_idx,
                        })
                    })
                    .collect()
            } else {
                Vec::new()
            };
            json!({
                "type": "tab-group",
                "panel-id": panel_id,
                "content-tag": content_tag,
                "tabs": tab_entries,
            })
        }
    }
}

/// Walk a `DockNode` tree and return every panel id reachable from
/// it. Used by [`WorkspaceSlot::dock_workspace_props_with_catalog`] to
/// build the labels / tags sidecar maps without scanning the full
/// catalog.
pub(crate) fn collect_panel_ids(root: &DockNode) -> Vec<String> {
    let mut out = Vec::new();
    walk_panel_ids(root, &mut out);
    out
}

pub(crate) fn walk_panel_ids(node: &DockNode, out: &mut Vec<String>) {
    match node {
        DockNode::Split { first, second, .. } => {
            walk_panel_ids(first, out);
            walk_panel_ids(second, out);
        }
        DockNode::TabGroup { tabs, .. } => {
            for t in tabs {
                if !out.contains(t) {
                    out.push(t.clone());
                }
            }
        }
    }
}

#[cfg(test)]
mod presence_tests {
    use super::*;
    use prism_core::network::presence::{
        PeerIdentity, PresenceChange, PresenceChangeKind, PresenceState,
    };

    fn state_for(peer: &str, view: &str) -> PresenceState {
        PresenceState {
            identity: PeerIdentity {
                peer_id: peer.into(),
                display_name: format!("Peer {peer}"),
                color: "#abc".into(),
                avatar_url: None,
            },
            cursor: None,
            selections: Vec::new(),
            active_view: Some(view.into()),
            last_seen: "2026-05-18T00:00:00Z".into(),
            data: Default::default(),
        }
    }

    #[test]
    fn joined_then_updated_then_left() {
        let mut dt = DevToolsSlot::default();
        let joined = PresenceChange {
            kind: PresenceChangeKind::Joined,
            peer_id: "p1".into(),
            state: Some(state_for("p1", "builder")),
        };
        assert!(dt.apply_presence_change(&joined, 100));
        assert_eq!(dt.presence.len(), 1);
        assert_eq!(dt.presence[0].active_view.as_deref(), Some("builder"));

        // Idempotent re-apply of the same state → no change.
        assert!(!dt.apply_presence_change(&joined, 100));

        let updated = PresenceChange {
            kind: PresenceChangeKind::Updated,
            peer_id: "p1".into(),
            state: Some(state_for("p1", "code")),
        };
        assert!(dt.apply_presence_change(&updated, 200));
        assert_eq!(dt.presence.len(), 1);
        assert_eq!(dt.presence[0].active_view.as_deref(), Some("code"));

        let left = PresenceChange {
            kind: PresenceChangeKind::Left,
            peer_id: "p1".into(),
            state: None,
        };
        assert!(dt.apply_presence_change(&left, 300));
        assert!(dt.presence.is_empty());
        // Left for an unknown peer is a no-op.
        assert!(!dt.apply_presence_change(&left, 400));
    }
}
