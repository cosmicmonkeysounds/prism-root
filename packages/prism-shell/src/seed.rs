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

use crate::state::{
    AppCard, AppState, CatalogSlot, DocsTopic, FileKind, FileNode, MenuLabel, NavButton,
    PaletteItem, ProjectSlot,
};

/// Build the hydrated boot state. Called from `Shell::new` instead of
/// `AppState::default()`. Tests that need the zero-data shape can
/// still construct `AppState::default()` directly.
pub fn initial_state() -> AppState {
    let mut state = AppState {
        chrome: seed_chrome(),
        catalog: seed_catalog(),
        project: seed_project(),
        ..AppState::default()
    };
    state.docs.topic = seed_welcome_topic();
    state.canvas.document = seed_document();
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
                icon: "icons/home.svg".into(),
                selected: true,
            },
            NavButton {
                icon: "icons/folder.svg".into(),
                selected: false,
            },
            NavButton {
                icon: "icons/search.svg".into(),
                selected: false,
            },
            NavButton {
                icon: "icons/settings.svg".into(),
                selected: false,
            },
        ],
        menus: ["File", "Edit", "View", "Window", "Help"]
            .into_iter()
            .map(|l| MenuLabel { label: l.into() })
            .collect(),
    }
}

/// Apps (launchpad), files (explorer), and the component palette.
/// Palette items derive from `prism_builder::starter::BUILTINS` plus
/// the `card` prefab — one source of truth for "what blocks exist."
fn seed_catalog() -> CatalogSlot {
    CatalogSlot {
        launchpad_title: "Apps".into(),
        apps: vec![
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
        ],
        files: seed_files(),
        palette: seed_palette(),
        palette_selected: None,
    }
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
    fn demo_document_has_three_children() {
        // Pin the demo content shape so a future seed edit doesn't
        // silently produce an empty canvas again.
        let doc = seed_document();
        let root = doc.root.expect("root");
        assert_eq!(root.children.len(), 3);
        assert_eq!(root.children[0].component, "text");
        assert_eq!(root.children[2].component, "button");
    }
}
