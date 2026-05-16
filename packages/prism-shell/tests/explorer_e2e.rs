//! Explorer e2e — IDE-mode Phase 1 coverage.
//!
//! Drives the project explorer through the production
//! [`Shell::dispatch_event`] path: seed `state.catalog.files` with
//! real on-disk paths (via `tempfile`), find the rendered
//! `explorer-row` hit by data-role, synthesize a PointerDown over it,
//! and verify the file opens as a new code-editor tab.
//!
//! This is the first integration test that exercises the file I/O
//! seam end-to-end; previous integration tests only drove keyboard +
//! pointer events against in-memory state.

use std::path::PathBuf;

use prism_shell::state::{FileKind, FileNode};
use prism_shell::Shell;
use prism_ui_runtime::event::{Event, Modifiers, PointerButton};
use prism_ui_runtime::layout::{HitRect, Viewport};

fn pointer_down(hit: &HitRect) -> Event {
    let (x, y) = (hit.bounds.x + 1.0, hit.bounds.y + 1.0);
    Event::PointerDown {
        x,
        y,
        button: PointerButton::Primary,
        modifiers: Modifiers::default(),
    }
}

fn seed_explorer_with(shell: &Shell, files: Vec<FileNode>) {
    shell.with_inner_mut(|inner| {
        // The explorer panel renders from `state.catalog.files`. The
        // workflow-page bar routes the dock to the Code page so the
        // explorer is in the visible tree when we ask for its hit
        // rects.
        inner.state.catalog.files = files;
        inner
            .state
            .workspace
            .workspace
            .navigate_to_panel("explorer");
        inner.viewport = Viewport {
            width: 1280.0,
            height: 800.0,
        };
    });
}

#[test]
fn clicking_a_file_row_opens_it_as_an_editor_tab() {
    // Real on-disk file so the OsVfs read path is exercised end-to-end.
    let tmp = tempfile::tempdir().expect("tempdir");
    let file_path = tmp.path().join("hello.luau");
    std::fs::write(&file_path, "print(\"hello from explorer\")\n").expect("seed file");

    let shell = Shell::new().expect("shell boots");
    seed_explorer_with(
        &shell,
        vec![FileNode {
            id: "hello.luau".into(),
            label: "hello.luau".into(),
            depth: 0,
            kind: FileKind::File,
            path: file_path.clone(),
        }],
    );

    let hit = shell
        .find_hit_by_role("explorer-row")
        .expect("explorer-row hit must exist after seeding");
    shell.dispatch_event(&pointer_down(&hit));

    shell.with_inner(|inner| {
        assert_eq!(
            inner.state.canvas.code_buffer_meta.path,
            Some(file_path.clone()),
            "active tab should point at the clicked file"
        );
        assert!(
            inner
                .state
                .canvas
                .code_buffer
                .source()
                .contains("hello from explorer"),
            "buffer should contain the file's contents"
        );
        assert!(
            inner.state.code_editor_focused,
            "clicking a file should focus the code editor"
        );
    });
}

#[test]
fn clicking_a_file_row_loads_with_correct_language() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let file_path = tmp.path().join("script.rs");
    std::fs::write(&file_path, "fn main() {}\n").expect("seed file");

    let shell = Shell::new().expect("shell boots");
    seed_explorer_with(
        &shell,
        vec![FileNode {
            id: "script.rs".into(),
            label: "script.rs".into(),
            depth: 0,
            kind: FileKind::File,
            path: file_path.clone(),
        }],
    );
    let hit = shell.find_hit_by_role("explorer-row").expect("hit");
    shell.dispatch_event(&pointer_down(&hit));

    shell.with_inner(|inner| {
        assert_eq!(
            inner.state.canvas.code_buffer.language, "rust",
            "language should be inferred from the file extension"
        );
    });
}

#[test]
fn clicking_a_missing_file_path_surfaces_an_error_toast() {
    let nonexistent = PathBuf::from("/tmp/definitely-not-here-prism-ide-test.luau");

    let shell = Shell::new().expect("shell boots");
    seed_explorer_with(
        &shell,
        vec![FileNode {
            id: "ghost.luau".into(),
            label: "ghost.luau".into(),
            depth: 0,
            kind: FileKind::File,
            path: nonexistent,
        }],
    );
    let hit = shell.find_hit_by_role("explorer-row").expect("hit");
    shell.dispatch_event(&pointer_down(&hit));

    shell.with_inner(|inner| {
        let has_open_failed_toast = inner
            .state
            .overlay
            .toasts
            .iter()
            .any(|t| t.title == "Open failed");
        assert!(
            has_open_failed_toast,
            "missing file should produce an Open failed toast"
        );
    });
}

#[test]
fn directory_rows_do_not_open_anything() {
    // Folder click is a no-op until folding lands.
    let shell = Shell::new().expect("shell boots");
    seed_explorer_with(
        &shell,
        vec![FileNode {
            id: "src".into(),
            label: "src".into(),
            depth: 0,
            kind: FileKind::Directory,
            path: PathBuf::from("/tmp/some-dir"),
        }],
    );
    let before = shell.with_inner(|inner| inner.state.canvas.code_buffer_meta.path.clone());
    let hit = shell.find_hit_by_role("explorer-row").expect("hit");
    shell.dispatch_event(&pointer_down(&hit));
    let after = shell.with_inner(|inner| inner.state.canvas.code_buffer_meta.path.clone());
    assert_eq!(
        before, after,
        "directory click must not change the active tab"
    );
}

#[test]
fn rendered_explorer_carries_path_data_attribute() {
    // The explorer's `data-path` attribute is what the click router
    // reads — this test confirms the round-trip from `FileNode.path`
    // through `files_json` and the `.prism-ui` interpolation.
    let shell = Shell::new().expect("shell boots");
    let canonical_path = PathBuf::from("/tmp/prism-ide-test/some-file.luau");
    seed_explorer_with(
        &shell,
        vec![FileNode {
            id: "some-file.luau".into(),
            label: "some-file.luau".into(),
            depth: 0,
            kind: FileKind::File,
            path: canonical_path.clone(),
        }],
    );
    let hit = shell.find_hit_by_role("explorer-row").expect("hit");
    let path_attr = hit
        .attrs
        .iter()
        .find(|(k, _)| k == "data-path")
        .map(|(_, v)| v.as_str())
        .unwrap_or_default();
    assert_eq!(
        path_attr,
        canonical_path.to_string_lossy(),
        "data-path must carry the absolute path"
    );
}
