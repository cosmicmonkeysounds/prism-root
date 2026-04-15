//! `prism-shell` native dev binary.
//!
//! Standalone entry point used during the inner dev loop — builds a
//! [`Shell`] (which owns both the reloadable `AppState` store and the
//! root Slint `AppWindow`) and hands control to Slint's event loop.
//! The packaged desktop build lives in
//! `packages/prism-studio/src-tauri` and embeds this crate as a
//! library; both entry points route through `Shell::run`.

use prism_shell::Shell;

fn main() -> Result<(), slint::PlatformError> {
    let shell = Shell::new()?;
    shell.run()
}
