//! `prism-shell` native dev binary.
//!
//! Standalone entry point used during the inner dev loop — skips the
//! Tauri shell entirely and pops a winit window straight onto the
//! same render pipeline the packaged app uses. The packaged desktop
//! build lives in `packages/prism-studio/src-tauri` and embeds
//! `prism-shell` as a library.
//!
//! Stubbed until Phase 0 spike #1 wires clay-layout + wgpu; the bin
//! exists now so `cargo run -p prism-shell` is a valid command.

use prism_shell::{render_app, AppState};

fn main() {
    let state = AppState::default();
    let count = render_app(&state);
    println!(
        "prism-shell native dev bin — initial frame would emit {count} draw commands (stub)."
    );
}
