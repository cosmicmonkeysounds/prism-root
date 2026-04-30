//! Subcommand surface for the `prism` binary.
//!
//! Each module in here owns one top-level verb (`test`, `build`,
//! `dev`, `lint`, `fmt`). The parent module wires them into the
//! clap enum and routes [`run`] to the right handler. Handlers
//! take `&Workspace` so they can resolve paths without re-walking
//! the filesystem, and return an exit code so `main.rs` can
//! propagate it verbatim.

use anyhow::Result;
use clap::{Parser, Subcommand};

use crate::workspace::Workspace;

pub mod build;
pub mod clean;
pub mod dev;
pub mod e2e;
pub mod fmt;
pub mod lint;
pub mod test;
pub mod visual;

/// Top-level argument parser for `prism`.
#[derive(Debug, Parser)]
#[command(
    name = "prism",
    about = "Prism Framework unified CLI — test, build, and run every Prism package.",
    version
)]
pub struct Cli {
    /// Print every command the CLI would spawn without running it.
    /// Useful for auditing what `prism build` actually runs.
    #[arg(long, global = true)]
    pub dry_run: bool,

    #[command(subcommand)]
    pub command: Command,
}

/// The top-level verb the user invoked.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Run `cargo test` across the workspace (or a single package).
    Test(test::TestArgs),
    /// Build one or many targets (desktop / web / relay / all).
    Build(build::BuildArgs),
    /// Run one or many dev servers behind a supervisor.
    Dev(dev::DevArgs),
    /// Run `cargo clippy --workspace --all-targets -- -D warnings`.
    Lint,
    /// Run `cargo fmt --all`.
    Fmt {
        /// Pass `--check` to `cargo fmt` — fails if formatting drifted.
        #[arg(long)]
        check: bool,
    },
    /// Run visual regression tests — capture screenshots of predefined
    /// shell scenes for visual diff review.
    Visual(visual::VisualArgs),
    /// Run end-to-end tests — automated input sequences + state
    /// assertions through the same code paths a human uses.
    E2e(e2e::E2eArgs),
    /// Run `cargo clean` to remove all build artefacts.
    Clean,
}

/// Dispatch a parsed [`Cli`] to the right subcommand.
///
/// Returns the child exit code (or a synthetic 0 on `--dry-run`).
pub fn run(cli: &Cli, workspace: &Workspace) -> Result<u8> {
    match &cli.command {
        Command::Test(args) => test::run(args, workspace, cli.dry_run),
        Command::Build(args) => build::run(args, workspace, cli.dry_run),
        Command::Dev(args) => dev::run(args, workspace, cli.dry_run),
        Command::Lint => lint::run(workspace, cli.dry_run),
        Command::Fmt { check } => fmt::run(*check, workspace, cli.dry_run),
        Command::Visual(args) => visual::run(args, workspace, cli.dry_run),
        Command::E2e(args) => e2e::run(args, workspace, cli.dry_run),
        Command::Clean => clean::run(workspace, cli.dry_run),
    }
}

/// Execute a plan of builders sequentially, stopping on the first
/// non-zero exit code. Every subcommand that is "run a series of
/// cargo commands" funnels through here so dry-run handling lives
/// in exactly one place.
pub fn execute_plan(plan: &[crate::builder::CommandBuilder], dry_run: bool) -> Result<u8> {
    for cmd in plan {
        if dry_run {
            println!("$ {}", cmd.display());
            continue;
        }
        println!("$ {}", cmd.display());
        let status = cmd
            .build()
            .status()
            .map_err(|e| anyhow::anyhow!("failed to spawn `{}`: {e}", cmd.display()))?;
        if !status.success() {
            let code = status.code().unwrap_or(1) as u8;
            return Ok(code);
        }
    }
    Ok(0)
}
