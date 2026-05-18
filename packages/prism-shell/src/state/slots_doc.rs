//! BuilderSlot / NavigationSlot / CatalogSlot / DocsSlot / MenuSlot.
//! Split out of `state/mod.rs` (Phase B.4). `use super::*`
//! inherits intra-`state` types + crate imports; cross-module
//! callers reach widened `pub(crate)` items.

use super::*;

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

    pub(crate) fn inspector_json(&self) -> Value {
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
    pub(crate) fn property_rows_json_with(&self, focus: Option<&FieldFocus>) -> Value {
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

    pub(crate) fn connections_json(&self) -> Value {
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

    pub(crate) fn schema_fields_json(&self) -> Value {
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
    pub(crate) fn as_str(self) -> &'static str {
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

    pub(crate) fn pages_list_json(&self) -> Value {
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

    pub(crate) fn pages_graph_json(&self) -> Value {
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

    pub(crate) fn edges_json(&self) -> Value {
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
    /// Absolute filesystem path. The id field is the path *relative*
    /// to the project root (for display + de-dupe); this carries the
    /// full path so the explorer's click router can hand it to the
    /// editor-file VFS read.
    ///
    /// IDE-mode Phase 1: `code-editor.open-path` reads this; without
    /// it, the explorer rows have nowhere to route to. Seeded test
    /// fixtures may leave this empty — the explorer click handler
    /// treats an empty path as "no-op" rather than panicking.
    pub path: std::path::PathBuf,
}

#[derive(Clone, Copy, Debug)]
pub enum FileKind {
    Directory,
    File,
}

impl FileKind {
    pub(crate) fn as_str(self) -> &'static str {
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

    pub(crate) fn apps_json(&self) -> Value {
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

    pub(crate) fn files_json(&self) -> Value {
        Value::Array(
            self.files
                .iter()
                .map(|f| {
                    json!({
                        "node-id": f.id,
                        "label": f.label,
                        "depth": f.depth,
                        "kind": f.kind.as_str(),
                        // IDE-mode Phase 1: the explorer's click
                        // router reads this to hand the absolute
                        // path to `editor.file.open-path`.
                        "path": f.path.to_string_lossy(),
                    })
                })
                .collect(),
        )
    }

    pub(crate) fn palette_json(&self) -> Value {
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
    pub(crate) fn topic_props(&self) -> Value {
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
    pub(crate) fn items_json(items: &[MenuItem]) -> Value {
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
