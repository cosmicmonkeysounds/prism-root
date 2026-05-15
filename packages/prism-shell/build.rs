//! Build-time validation of every `.prism-ui` source shipped by
//! `prism-shell`. Closes C1 of `docs/dev/ui-migration-followups.md`:
//! a parse error in any included skeleton or component file fails
//! the build instead of producing a binary that panics in
//! `Skeleton::load` at boot.
//!
//! The shell's `ui/app.prism-ui` + `ui/components/*.prism-ui` files
//! are all consumed at runtime via `include_str!` + a fresh
//! `prism_ui_runtime::interpret::interpret` call. This script runs
//! the same `prism_core::language::prism_ui::parse` pipeline
//! `prism-ui-build::compile` wraps, so build-time and runtime
//! agree on what "parses" means.
//!
//! Per-app skeletons (`apps/<id>/shell.prism-ui`) are validated at
//! runtime by `app_loader` because they live outside the crate
//! manifest dir and may be added/removed without touching the shell.

use std::path::Path;

fn main() {
    let manifest_dir =
        std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is always set by cargo");
    let ui_dir = Path::new(&manifest_dir).join("ui");

    let mut paths: Vec<std::path::PathBuf> = Vec::new();
    let app = ui_dir.join("app.prism-ui");
    if app.exists() {
        paths.push(app);
    }
    let components = ui_dir.join("components");
    if components.exists() {
        if let Ok(entries) = std::fs::read_dir(&components) {
            let mut files: Vec<_> = entries
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("prism-ui"))
                .collect();
            // Deterministic order so cargo's rerun-if-changed lines
            // are stable across builds.
            files.sort();
            paths.extend(files);
        }
    }

    for path in &paths {
        // Tell cargo to rebuild when any of these files change.
        println!("cargo:rerun-if-changed={}", path.display());
        if let Err(err) = prism_ui_build::compile(path) {
            panic!("prism-shell: failed to parse {}: {}", path.display(), err);
        }
    }

    // Always rerun if the build script itself changes.
    println!("cargo:rerun-if-changed=build.rs");
}
