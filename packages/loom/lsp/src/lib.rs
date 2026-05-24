//! Loom v3 Language Server Protocol implementation.
//!
//! Stdio JSON-RPC server backed by [`loom_parser`] plus a workspace-
//! wide name index (every `CHARACTER`, every `TRAIT`, every registered
//! directive function, every beat, every anchor, every ` ```todo``` `
//! fence — spec §17).
//!
//! Phase-1 stub: returns immediately. The full request loop lands
//! once the parser + project loader are in.

use anyhow::Result;

/// Run the LSP loop on stdin/stdout. Phase-1: refuses to start with
/// an informative error so editors don't silently hang on an empty
/// transport.
pub fn run_stdio() -> Result<()> {
    Err(anyhow::anyhow!(
        "loom-lsp: the v3 language server is not implemented yet. \
         See docs/dev/loom-v3.html for the design."
    ))
}
