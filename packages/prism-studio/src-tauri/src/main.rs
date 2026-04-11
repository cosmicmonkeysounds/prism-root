//! Prism Studio — Tauri 2.0 shell.
//!
//! The Tauri shell is one of several possible hosts of the Prism Daemon
//! kernel. It constructs the kernel via the same `DaemonBuilder` API a
//! headless/mobile host would use, then bridges Tauri's `#[command]`
//! attribute macros onto the kernel's transport-agnostic `invoke` surface.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;

use prism_daemon::{DaemonBuilder, DaemonKernel};
use std::sync::Arc;

fn main() {
    // Compose the kernel with every capability Studio needs on desktop.
    // Mobile hosts would swap `.with_build()` + `.with_watcher()` out for
    // their own modules — the wiring is one-line-per-capability either way.
    let kernel: Arc<DaemonKernel> = Arc::new(
        DaemonBuilder::new()
            .with_crdt()
            .with_lua()
            .with_build()
            .with_watcher()
            .build()
            .expect("failed to build Prism Daemon kernel"),
    );

    tauri::Builder::default()
        .manage(kernel)
        .invoke_handler(tauri::generate_handler![
            commands::crdt_write,
            commands::crdt_read,
            commands::crdt_export,
            commands::luau_exec,
            commands::run_build_step,
            commands::daemon_capabilities,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Prism Studio");
}
