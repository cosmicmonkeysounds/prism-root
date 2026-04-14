//! Prism Studio — Tauri 2 desktop shell (no-webview configuration).
//!
//! Per the Clay migration plan §4.5, Studio uses Tauri 2 for packaging,
//! signing, auto-update, and sidecar lifecycle management — but does
//! *not* load `wry` / the webview. Windowing goes through `tao`
//! directly; rendering is handled by `prism-shell` (wgpu → Clay).
//!
//! This file is a Phase-0 scaffold: it builds the Tauri shell, spawns
//! the daemon sidecar, and surfaces the `prism-shell` render loop.
//! The actual Clay wiring lands behind feature flags once the
//! `clay-layout` binding is validated in the spike.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod sidecar;

use prism_shell::{render_app, AppState};

fn main() {
    tauri::Builder::default()
        .setup(|_app| {
            // Kick off the daemon sidecar. Tauri's sidecar API spawns
            // a packaged binary alongside the app bundle; during dev
            // we fall back to the in-tree `cargo run -p prism-daemon`
            // path via `sidecar::spawn_dev`.
            sidecar::spawn_dev();

            // Drive one render just to prove the plumbing. The real
            // event loop attaches tao events to `prism_shell::input`
            // in Phase 0 spike #5.
            let state = AppState::default();
            let count = render_app(&state);
            eprintln!(
                "prism-studio shell: initial frame stub emitted {count} draw commands"
            );

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Prism Studio");
}
