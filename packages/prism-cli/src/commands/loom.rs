//! `prism loom` — language tools for the Loom storytelling language.
//!
//! Today there's exactly one subcommand: `prism loom lsp`, which runs
//! the in-process Language Server Protocol implementation against
//! stdio. Editors that auto-discover the binary (Zed via the bundled
//! extension, Neovim via `mason-lspconfig`, etc.) launch it for
//! `.loom` files and get diagnostics + hover + completion +
//! semantic-token highlights straight from `loom_parser`.

use anyhow::Result;
use clap::{Args, Subcommand};

use crate::workspace::Workspace;

#[derive(Debug, Args)]
pub struct LoomArgs {
    #[command(subcommand)]
    pub kind: LoomKind,
}

#[derive(Debug, Subcommand)]
pub enum LoomKind {
    /// Run the Loom Language Server Protocol implementation against
    /// stdio. Editors connect by spawning this process and speaking
    /// JSON-RPC over its stdin/stdout.
    Lsp,
}

pub fn run(args: &LoomArgs, _workspace: &Workspace, dry_run: bool) -> Result<u8> {
    match args.kind {
        LoomKind::Lsp => {
            if dry_run {
                println!("$ loom-lsp  # would run the LSP loop on stdio");
                return Ok(0);
            }
            loom_lsp::run_stdio()?;
            Ok(0)
        }
    }
}
