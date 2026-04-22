//! Prism Studio — packaged desktop shell.
//!
//! Slint owns windowing, layout, and rendering via the `prism-shell`
//! library; Studio's only extra responsibility is spawning the
//! `prism-daemond` sidecar over `interprocess` and holding its
//! handle for the lifetime of the event loop.
//!
//! Packaging/signing/auto-updates land in Phase 5 via
//! `cargo-packager` + `self_update` + the standalone shell crates
//! (`tray-icon`, `notify-rust`, `rfd`, `arboard`, `keyring`).

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod sidecar;

use prism_shell::Shell;

fn main() -> Result<(), slint::PlatformError> {
    // Capture full backtrace on panic so we can diagnose Slint
    // property recursion. Writes to /tmp/prism-panic.txt.
    std::panic::set_hook(Box::new(|info| {
        let bt = std::backtrace::Backtrace::force_capture();
        let msg = format!("{info}\n\nBacktrace:\n{bt}");
        eprintln!("{msg}");
        let _ = std::fs::write("/tmp/prism-panic.txt", &msg);
    }));

    // Spawn the daemon sidecar before the event loop takes over the
    // main thread. If it fails we log and continue — the shell is
    // still useful for UI iteration when the kernel is down. The
    // handle stays alive in a local binding so its `Drop` runs on
    // shutdown and kills the child deterministically.
    let _daemon = match sidecar::spawn_dev() {
        Ok(handle) => Some(handle),
        Err(err) => {
            eprintln!("prism-studio: daemon sidecar unavailable: {err:#}");
            None
        }
    };

    let shell = Shell::new()?;
    shell.run()
}
