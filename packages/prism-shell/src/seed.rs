//! Boot-time hydration of `AppState`.
//!
//! `AppState::default()` is the zero-data starting point — every slot
//! empty, every list zero-length. That shape is the §17 contract for
//! tests and for the headless render path, but a fresh
//! `cargo run -p prism-shell` window starting from it renders as a
//! field of bare panel rectangles: there is no content for the
//! launchpad, the component palette, the explorer tree, the docs
//! sidebar, or the canvas surface to render.
//!
//! `initial_state()` is the single place that hydrates those slots
//! with realistic boot data so the shell looks like Studio the moment
//! it opens. Per §43 A1, this replaces the Slint-era
//! `app/samples.rs` + `app/inner.rs` boot path that was deleted in
//! the Phase 5 cutover without a replacement landing alongside it.
//!
//! Everything here is *static seed data* — populating live document
//! state, project files, search indexes, etc. is the job of the
//! services in `crate::services`. The seed only provides the
//! "first-frame" shape every binding can render against.

use prism_builder::layout::{FlexDirection, FlowDisplay, FlowProps};
use prism_builder::{starter, BuilderDocument, Node, StyleProperties};
use prism_core::foundation::spatial::Transform2D;
use serde_json::json;

use crate::app_loader::LoadedApp;
use crate::state::{
    AppCard, AppState, CatalogSlot, DocsTopic, FileKind, FileNode, MenuLabel, NavButton, NavEdge,
    NavEdgeKind, NavPage, NavigationSlot, PaletteItem, ProjectSlot, SchemaDoc, SchemaField,
    SignalConnection,
};

/// Build the hydrated boot state. Called from `Shell::new` instead of
/// `AppState::default()`. Tests that need the zero-data shape can
/// still construct `AppState::default()` directly.
pub fn initial_state() -> AppState {
    initial_state_with_apps(&[])
}

/// Variant of [`initial_state`] that lets the caller seed the
/// launchpad from `apps/*/manifest.toml` instead of the hardcoded
/// fallback. An empty slice falls back to the built-in list — keeping
/// every existing test path unchanged.
pub fn initial_state_with_apps(apps: &[LoadedApp]) -> AppState {
    let mut state = AppState {
        chrome: seed_chrome(),
        catalog: seed_catalog_with_apps(apps),
        project: seed_project(),
        navigation: seed_navigation(),
        ..AppState::default()
    };
    state.docs.topic = seed_welcome_topic();
    state.canvas.document = seed_document();
    state.builder.schema = seed_schema();
    state.builder.signal_connections = seed_signal_connections();
    // §43 C1: pre-select the demo heading so the properties panel
    // boots with a populated form rather than the empty "select a
    // block" state. Inspector tree is derived here without a registry;
    // the property-row derivation runs in `Shell::new` after the
    // registry is constructed.
    state.canvas.selection = Some("demo-heading".into());
    state.resync_builder_for_selection(None);
    state
}

/// Five-pill menu bar (File / Edit / View / Window / Help) + four
/// activity-bar nav buttons. Mirrors the Slint chrome row that the
/// migration plan §16 panel-by-panel translation captured.
fn seed_chrome() -> crate::state::ChromeSlot {
    crate::state::ChromeSlot {
        app_name: "Studio".into(),
        status: "Ready".into(),
        nav_buttons: vec![
            NavButton {
                id: "home".into(),
                icon: "icons/home.svg".into(),
                selected: true,
            },
            NavButton {
                id: "folder".into(),
                icon: "icons/folder.svg".into(),
                selected: false,
            },
            NavButton {
                id: "search".into(),
                icon: "icons/search.svg".into(),
                selected: false,
            },
            NavButton {
                id: "settings".into(),
                icon: "icons/settings.svg".into(),
                selected: false,
            },
        ],
        menus: ["File", "Edit", "View", "Window", "Help"]
            .into_iter()
            .map(|l| MenuLabel {
                id: l.to_lowercase(),
                label: l.into(),
            })
            .collect(),
        active_menu: None,
    }
}

/// Apps (launchpad), files (explorer), and the component palette.
/// Palette items derive from `prism_builder::starter::BUILTINS` plus
/// the `card` prefab — one source of truth for "what blocks exist."
///
/// When `apps` is non-empty, the launchpad tiles come from
/// `AppLoader::discover` output instead of the hardcoded fallback —
/// the path the DSL self-bootstrap plan wires up. The hardcoded list
/// stays as a last-resort fallback so `cargo test` keeps working
/// without an `apps/` directory checked in.
fn seed_catalog_with_apps(apps: &[LoadedApp]) -> CatalogSlot {
    let tiles = if apps.is_empty() {
        fallback_app_tiles()
    } else {
        apps.iter()
            .map(|a| AppCard {
                id: a.manifest.id.clone(),
                label: a.manifest.label.clone(),
                icon: a.manifest.icon.clone(),
                summary: a.manifest.summary.clone(),
            })
            .collect()
    };
    CatalogSlot {
        launchpad_title: "Apps".into(),
        apps: tiles,
        files: seed_files(),
        palette: seed_palette(),
        palette_selected: None,
        palette_drag: None,
    }
}

/// Hardcoded launchpad tiles used when no `apps/` directory is
/// present. Mirrors what `seed_catalog` returned before the
/// DSL self-bootstrap plan landed.
fn fallback_app_tiles() -> Vec<AppCard> {
    vec![
        AppCard {
            id: "lattice".into(),
            label: "Lattice".into(),
            icon: "icons/grid.svg".into(),
            summary: "Collaborative workspace with real-time CRDT sync.".into(),
        },
        AppCard {
            id: "musica".into(),
            label: "Musica".into(),
            icon: "icons/music.svg".into(),
            summary: "Audio workstation with timeline and MIDI.".into(),
        },
        AppCard {
            id: "flux".into(),
            label: "Flux".into(),
            icon: "icons/zap.svg".into(),
            summary: "Visual dataflow editor for creative coding.".into(),
        },
        AppCard {
            id: "studio".into(),
            label: "Studio".into(),
            icon: "icons/sliders.svg".into(),
            summary: "The page builder you are looking at right now.".into(),
        },
    ]
}

/// Placeholder project tree. The real population lands when
/// `ProjectService` opens a folder; the seed gives the explorer
/// panel something to render against until then.
fn seed_files() -> Vec<FileNode> {
    vec![
        FileNode {
            id: "project".into(),
            label: "untitled-project".into(),
            depth: 0,
            kind: FileKind::Directory,
        },
        FileNode {
            id: "app-flux".into(),
            label: "Flux".into(),
            depth: 1,
            kind: FileKind::Directory,
        },
        FileNode {
            id: "app-flux/home".into(),
            label: "Home".into(),
            depth: 2,
            kind: FileKind::File,
        },
        FileNode {
            id: "app-flux/settings".into(),
            label: "Settings".into(),
            depth: 2,
            kind: FileKind::File,
        },
        FileNode {
            id: "app-musica".into(),
            label: "Musica".into(),
            depth: 1,
            kind: FileKind::Directory,
        },
    ]
}

/// Walk `prism_builder::starter::BUILTINS` and project each spec onto
/// a `PaletteItem`. One source of truth for "what blocks exist" — the
/// palette automatically tracks new builtins as they land.
///
/// `card` is added explicitly because it is a prefab, not a `BlockSpec`
/// (and therefore not in `BUILTINS`); it still belongs in the palette.
fn seed_palette() -> Vec<PaletteItem> {
    let mut items: Vec<PaletteItem> = starter::BUILTINS
        .iter()
        .map(|spec| PaletteItem {
            id: spec.id.into(),
            label: title_case(spec.id),
            icon: palette_icon(spec.id).into(),
            category: palette_category(spec.id).into(),
        })
        .collect();
    items.push(PaletteItem {
        id: "card".into(),
        label: "Card".into(),
        icon: "icons/credit-card.svg".into(),
        category: "Layout".into(),
    });
    items.sort_by(|a, b| a.label.cmp(&b.label));
    items
}

/// `"text"` → `"Text"`, `"graph-view"` → `"Graph View"`. The single
/// place block ids get translated into UI labels; tests pin the
/// kebab-case → Title Case contract.
fn title_case(id: &str) -> String {
    id.split('-')
        .map(|word| {
            let mut c = word.chars();
            match c.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().collect::<String>() + c.as_str(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Per-builtin icon path. Kept inline rather than embedded on each
/// `BlockSpec` because the icon-asset story belongs to the shell, not
/// the builder registry — promoting it onto `BlockSpec` would couple
/// the platform-neutral builder crate to shell icon assets.
fn palette_icon(id: &str) -> &'static str {
    match id {
        "text" => "icons/type.svg",
        "image" => "icons/image.svg",
        "container" => "icons/box.svg",
        "form" => "icons/form-input.svg",
        "input" => "icons/edit.svg",
        "button" => "icons/mouse-pointer.svg",
        "code" => "icons/code.svg",
        "divider" => "icons/minus.svg",
        "spacer" => "icons/space.svg",
        "columns" => "icons/columns.svg",
        "list" => "icons/list.svg",
        "table" => "icons/table.svg",
        "tabs" => "icons/folder.svg",
        "accordion" => "icons/menu.svg",
        "graph-view" => "icons/share-2.svg",
        _ => "icons/box.svg",
    }
}

/// Section grouping for the palette. Kept here for the same reason
/// `palette_icon` is — block grouping is a shell-side concern.
fn palette_category(id: &str) -> &'static str {
    match id {
        "text" | "code" => "Text",
        "image" => "Media",
        "container" | "columns" | "divider" | "spacer" => "Layout",
        "form" | "input" | "button" => "Form",
        "list" | "table" | "tabs" | "accordion" => "Collections",
        "graph-view" => "Visualization",
        _ => "Other",
    }
}

/// Welcome topic shown in the docs panel when nothing else is
/// selected. Plain text — the docs renderer handles paragraph breaks
/// from `\n\n`.
fn seed_welcome_topic() -> DocsTopic {
    DocsTopic {
        title: "Welcome to Prism Studio".into(),
        summary: "Drag a block from the Components palette onto the canvas to start building. \
             Select a block to edit its properties on the right."
            .into(),
        body: "Prism is an all-Rust visual operating system. \
            The shell you are looking at is rendered through prism-ui-runtime \
            on top of Taffy layout — no Slint, no React, no Tailwind. \
            Press Ctrl+Shift+P to open the command palette."
            .into(),
    }
}

/// Project slot with no live file — Studio boots Untitled. The recent
/// list is empty until `PersistenceService` records its first open.
fn seed_project() -> ProjectSlot {
    ProjectSlot {
        current_file: None,
        root: None,
        dirty: false,
        recent: Vec::new(),
    }
}

/// Starter document for the canvas — a page-shell plus a heading,
/// paragraph, and button so the canvas renders something rather than
/// an empty page. Mirrors the demo document that the Slint shell used
/// to seed at boot.
///
/// The nodes are simple `Flow` children of the root container; the
/// page-shell sets up a 1×1 grid with sensible margins and gaps so
/// the document renders as a centred column inside the canvas page.
fn seed_document() -> BuilderDocument {
    let mut doc = BuilderDocument::page_shell();
    if let Some(root) = doc.root.as_mut() {
        root.children = vec![
            Node {
                id: "demo-heading".into(),
                component: "text".into(),
                props: json!({
                    "body": "Welcome to Studio",
                    "level": "h1",
                }),
                layout_mode: flow_block(),
                style: StyleProperties::default(),
                transform: Transform2D::default(),
                modifiers: Vec::new(),
                children: Vec::new(),
            },
            Node {
                id: "demo-paragraph".into(),
                component: "text".into(),
                props: json!({
                    "body": "Drag components from the palette on the left to begin. \
                             Selecting a block reveals its editable properties on the right.",
                }),
                layout_mode: flow_block(),
                style: StyleProperties::default(),
                transform: Transform2D::default(),
                modifiers: Vec::new(),
                children: Vec::new(),
            },
            Node {
                id: "demo-button".into(),
                component: "button".into(),
                props: json!({
                    "label": "Get started",
                    "variant": "primary",
                }),
                layout_mode: flow_block(),
                style: StyleProperties::default(),
                transform: Transform2D::default(),
                modifiers: Vec::new(),
                children: Vec::new(),
            },
        ];
    }
    doc
}

/// Starter schema for the Data workflow page. Three fields exercise
/// the trash → `schema.delete-selected-field` flow end-to-end the
/// moment a user clicks one of them.
fn seed_schema() -> SchemaDoc {
    SchemaDoc {
        title: "Post".into(),
        schema_name: "post".into(),
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
            SchemaField {
                name: "tags".into(),
                kind: "list".into(),
                required: false,
            },
        ],
        selected_field: None,
    }
}

/// Starter signal connections that mirror the demo document — the
/// signals panel renders one row per entry, and the trash on the
/// selected row routes through `signals.delete-selected-connection`.
fn seed_signal_connections() -> Vec<SignalConnection> {
    vec![
        SignalConnection {
            id: "c-button-clicked".into(),
            source_signal: "clicked".into(),
            action_kind: "EmitSignal".into(),
            target_label: "demo-button".into(),
        },
        SignalConnection {
            id: "c-heading-hover".into(),
            source_signal: "hovered".into(),
            action_kind: "SetProperty".into(),
            target_label: "demo-heading".into(),
        },
    ]
}

/// Starter navigation pages so the page list (and graph) render
/// something interactive on the Navigation workflow page. The
/// per-row chevron / trash affordances need a cursor row to surface;
/// the boot leaves the cursor empty so the user picks one by clicking.
fn seed_navigation() -> NavigationSlot {
    NavigationSlot {
        pages: vec![
            NavPage {
                id: "home".into(),
                title: "Home".into(),
                route: "/".into(),
                x: 60.0,
                y: 60.0,
                node_count: 3,
                link_count: 1,
                is_active: true,
            },
            NavPage {
                id: "about".into(),
                title: "About".into(),
                route: "/about".into(),
                x: 260.0,
                y: 60.0,
                node_count: 1,
                link_count: 1,
                is_active: false,
            },
            NavPage {
                id: "contact".into(),
                title: "Contact".into(),
                route: "/contact".into(),
                x: 460.0,
                y: 60.0,
                node_count: 1,
                link_count: 0,
                is_active: false,
            },
        ],
        edges: vec![
            NavEdge {
                from: 0,
                to: 1,
                kind: NavEdgeKind::Href,
            },
            NavEdge {
                from: 1,
                to: 2,
                kind: NavEdgeKind::Href,
            },
        ],
        selected_page: None,
    }
}

fn flow_block() -> prism_builder::layout::LayoutMode {
    prism_builder::layout::LayoutMode::Flow(FlowProps {
        display: FlowDisplay::Block,
        flex_direction: FlexDirection::Row,
        gap: 0.0,
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_state_seeds_every_user_facing_slot() {
        // §43 A1 contract: a fresh boot must produce non-empty data for
        // every panel a user sees on the first frame.
        let s = initial_state();
        assert!(!s.catalog.apps.is_empty(), "launchpad needs apps");
        assert!(!s.catalog.palette.is_empty(), "palette needs items");
        assert!(!s.catalog.files.is_empty(), "explorer needs files");
        assert!(!s.docs.topic.title.is_empty(), "docs needs a topic");
        assert!(s.canvas.document.root.is_some(), "canvas needs a document");
        assert!(!s.chrome.menus.is_empty(), "menu bar needs pills");
        assert!(
            !s.chrome.nav_buttons.is_empty(),
            "activity bar needs buttons"
        );
    }

    #[test]
    fn palette_matches_builder_builtins_plus_card() {
        // Source-of-truth pin: the palette is derived from
        // `prism_builder::starter::BUILTINS`. New builtins must show
        // up automatically; the `card` prefab is the only manual row.
        let s = initial_state();
        let ids: Vec<&str> = s.catalog.palette.iter().map(|p| p.id.as_str()).collect();
        for spec in starter::BUILTINS {
            assert!(
                ids.contains(&spec.id),
                "palette missing builtin `{}`",
                spec.id
            );
        }
        assert!(ids.contains(&"card"), "palette missing `card` prefab");
    }

    #[test]
    fn title_case_handles_kebab_ids() {
        assert_eq!(title_case("text"), "Text");
        assert_eq!(title_case("graph-view"), "Graph View");
        assert_eq!(title_case("code-editor"), "Code Editor");
    }

    #[test]
    fn seed_populates_schema_signals_and_navigation_for_interactive_panels() {
        // The Data, Edit, and Navigation workflow pages render
        // `shell.schema-designer`, `shell.signals-panel`, and
        // `shell.nav-page-list` respectively. Without seed data those
        // panels appear blank and the row-click / trash routes (B6
        // fifth wave) have nothing to fire against.
        let s = initial_state();
        assert!(
            !s.builder.schema.fields.is_empty(),
            "schema designer needs starter fields for the trash + cursor flow"
        );
        assert!(
            !s.builder.signal_connections.is_empty(),
            "signals panel needs starter rows for the trash + cursor flow"
        );
        assert!(
            !s.navigation.pages.is_empty(),
            "navigation page list needs starter pages for the chevron flow"
        );
        // Cursors start empty — the user picks the row whose chevrons /
        // trash they want to expose; mirrors the nav-page-row pattern.
        assert!(s.builder.schema.selected_field.is_none());
        assert!(s.builder.selected_connection.is_none());
        assert!(s.navigation.selected_page.is_none());
    }

    #[test]
    fn demo_document_has_three_children() {
        // Pin the demo content shape so a future seed edit doesn't
        // silently produce an empty canvas again.
        let doc = seed_document();
        let root = doc.root.expect("root");
        assert_eq!(root.children.len(), 3);
        assert_eq!(root.children[0].component, "text");
        assert_eq!(root.children[2].component, "button");
    }

    #[test]
    fn boot_state_has_realistic_seed_data() {
        // §43 E2: the named verification test for Phase A. The boot
        // state must hydrate every panel a user sees on the first
        // frame — apps, palette, files, docs topic, canvas document,
        // chrome pills, activity-bar buttons — and pre-select the
        // demo heading so the right rail boots populated.
        let s = initial_state();

        // Launchpad: four named app cards.
        assert!(s.catalog.apps.len() >= 4, "launchpad needs four apps");
        let app_ids: Vec<&str> = s.catalog.apps.iter().map(|a| a.id.as_str()).collect();
        for required in ["lattice", "musica", "flux", "studio"] {
            assert!(
                app_ids.contains(&required),
                "app {required} missing from launchpad"
            );
        }

        // Palette: every builtin block plus the `card` prefab.
        assert!(
            s.catalog.palette.len() > starter::BUILTINS.len(),
            "palette must cover every builtin + card"
        );

        // Explorer + docs + canvas + chrome — non-empty by contract.
        assert!(!s.catalog.files.is_empty(), "explorer needs files");
        assert!(!s.docs.topic.title.is_empty(), "docs needs a topic");
        assert!(s.canvas.document.root.is_some(), "canvas needs a document");
        assert!(!s.chrome.menus.is_empty(), "menu bar needs pills");
        assert!(
            !s.chrome.nav_buttons.is_empty(),
            "activity bar needs buttons"
        );

        // §43 C1: boot pre-selects `demo-heading` so the inspector
        // tree carries a `selected` flag for it.
        assert_eq!(s.canvas.selection.as_deref(), Some("demo-heading"));
        let selected_row = s
            .builder
            .inspector
            .iter()
            .find(|n| n.selected)
            .expect("seed must pre-select a node in the inspector");
        assert_eq!(selected_row.id, "demo-heading");
    }

    #[test]
    fn loaded_apps_drive_launchpad_when_present() {
        // DSL self-bootstrap Loop 1: when manifests are discovered,
        // their `id` / `label` / `icon` / `summary` flow through
        // unchanged. The hardcoded fallback is bypassed.
        let apps = vec![LoadedApp {
            manifest: prism_core::AppManifest {
                id: "custom-app".into(),
                label: "Custom App".into(),
                icon: "icons/custom.svg".into(),
                summary: "A loaded-from-disk app.".into(),
                ..Default::default()
            },
            base_dir: std::path::PathBuf::from("/tmp/custom-app"),
        }];
        let s = initial_state_with_apps(&apps);
        assert_eq!(s.catalog.apps.len(), 1);
        assert_eq!(s.catalog.apps[0].id, "custom-app");
        assert_eq!(s.catalog.apps[0].label, "Custom App");
        assert_eq!(s.catalog.apps[0].icon, "icons/custom.svg");
    }

    #[test]
    fn empty_loaded_apps_falls_back_to_hardcoded_list() {
        // Existing tests construct `initial_state()` with no manifest
        // input. The fallback must still produce the canonical four
        // tiles so every downstream assertion keeps passing.
        let s = initial_state_with_apps(&[]);
        let ids: Vec<&str> = s.catalog.apps.iter().map(|a| a.id.as_str()).collect();
        for required in ["lattice", "musica", "flux", "studio"] {
            assert!(
                ids.contains(&required),
                "fallback should keep `{}` tile",
                required
            );
        }
    }
}
