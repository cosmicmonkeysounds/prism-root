//! Prism Studio — Tauri 2.0 shell.
//! The Universal Host application wrapping the Vite SPA frontend.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;

use prism_daemon::DocManager;
use std::sync::Arc;

fn main() {
    let doc_manager = Arc::new(DocManager::new());

    tauri::Builder::default()
        .manage(doc_manager)
        .invoke_handler(tauri::generate_handler![
            commands::crdt_write,
            commands::crdt_read,
            commands::crdt_export,
            commands::lua_exec,
            commands::run_build_step,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Prism Studio");
}
