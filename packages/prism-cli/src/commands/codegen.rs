//! `prism codegen` — emit derived artefacts (today: Luau type stubs)
//! from `#[luau_expose]`-annotated types across the workspace.
//!
//! Phase 1 of `docs/dev/luau-integration-plan.md` only walks the
//! `prism-core` registry, since that's the only crate annotated so
//! far. Later phases bolt the `prism-builder` registry on with the
//! same shape.

use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Args, Subcommand};

use crate::workspace::Workspace;

#[derive(Debug, Args)]
pub struct CodegenArgs {
    #[command(subcommand)]
    pub kind: CodegenKind,
}

#[derive(Debug, Subcommand)]
pub enum CodegenKind {
    /// Emit `.d.luau` type stubs for every `#[luau_expose]` type in
    /// the workspace. By default writes one consolidated stub file
    /// per source crate under `<workspace>/types/`.
    LuauTypes(LuauTypesArgs),
}

#[derive(Debug, Args)]
pub struct LuauTypesArgs {
    /// Output directory. Defaults to `<workspace>/types`.
    #[arg(long)]
    pub out: Option<PathBuf>,
    /// Print the generated content to stdout instead of writing files.
    #[arg(long)]
    pub stdout: bool,
}

pub fn run(args: &CodegenArgs, workspace: &Workspace, dry_run: bool) -> Result<u8> {
    match &args.kind {
        CodegenKind::LuauTypes(args) => luau_types(args, workspace, dry_run),
    }
}

fn luau_types(args: &LuauTypesArgs, workspace: &Workspace, dry_run: bool) -> Result<u8> {
    let core_stub = prism_core::luau_types::render_type_stubs();

    if args.stdout || dry_run {
        println!("// types/core.d.luau\n{core_stub}");
        return Ok(0);
    }

    let out_dir = args
        .out
        .clone()
        .unwrap_or_else(|| workspace.root().join("types"));
    fs::create_dir_all(&out_dir)
        .with_context(|| format!("create luau types output dir at {}", out_dir.display()))?;
    let core_path = out_dir.join("core.d.luau");
    fs::write(&core_path, &core_stub).with_context(|| format!("write {}", core_path.display()))?;
    println!("wrote {}", core_path.display());
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn luau_types_writes_a_core_stub() {
        let tmp = tempdir().unwrap();
        let args = LuauTypesArgs {
            out: Some(tmp.path().to_path_buf()),
            stdout: false,
        };
        // Workspace is irrelevant when `out` is set; use the discovered
        // one to keep the call ergonomic.
        let ws = Workspace::discover().unwrap();
        let code = luau_types(&args, &ws, false).unwrap();
        assert_eq!(code, 0);
        let written = fs::read_to_string(tmp.path().join("core.d.luau")).unwrap();
        assert!(written.starts_with("--!strict\n"));
        assert!(written.contains("export type DesignTokens"));
        assert!(written.contains("export type Rgba"));
        assert!(written.contains("export type ShellMode = \"Use\" | \"Build\" | \"Admin\""));
    }
}
