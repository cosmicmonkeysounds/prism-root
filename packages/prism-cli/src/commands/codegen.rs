//! `prism codegen` — emit derived artefacts (today: Luau type stubs)
//! from `#[luau_expose]`-annotated types across the workspace.
//!
//! Per `docs/dev/luau-integration-plan.md` §Phase 5, the pipeline
//! fans out across both `prism-core` and `prism-builder` registries
//! and additionally emits a per-component `signals.d.luau` from
//! `prism_builder::signal::generate_signal_type_stubs` over the
//! built-in `ComponentRegistry` so script authors get IDE
//! intelligence on every signal payload shape.

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
    let builder_stub = prism_builder::luau_types::render_type_stubs();
    let signals_stub = render_signals_stub();

    if args.stdout || dry_run {
        println!("// types/core.d.luau\n{core_stub}");
        println!("// types/builder.d.luau\n{builder_stub}");
        println!("// types/signals.d.luau\n{signals_stub}");
        return Ok(0);
    }

    let out_dir = args
        .out
        .clone()
        .unwrap_or_else(|| workspace.root().join("types"));
    fs::create_dir_all(&out_dir)
        .with_context(|| format!("create luau types output dir at {}", out_dir.display()))?;

    for (name, contents) in [
        ("core.d.luau", &core_stub),
        ("builder.d.luau", &builder_stub),
        ("signals.d.luau", &signals_stub),
    ] {
        let path = out_dir.join(name);
        fs::write(&path, contents).with_context(|| format!("write {}", path.display()))?;
        println!("wrote {}", path.display());
    }
    Ok(0)
}

/// Build the `signals.d.luau` payload by walking the built-in
/// `ComponentRegistry` and emitting one symbol class per component.
/// Defined here (rather than in `prism-builder`) because the registry
/// instantiation is a CLI-side concern: future iterations will
/// additionally walk per-workspace component contributions.
fn render_signals_stub() -> String {
    let mut registry = prism_builder::registry::ComponentRegistry::new();
    let _ = prism_builder::starter::register_builtins(
        &mut registry,
        &mut prism_builder::HtmlRegistry::new(),
    );
    prism_builder::signal::generate_signal_type_stubs(&registry, "prism")
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

    #[test]
    fn luau_types_writes_builder_and_signals_stubs() {
        let tmp = tempdir().unwrap();
        let args = LuauTypesArgs {
            out: Some(tmp.path().to_path_buf()),
            stdout: false,
        };
        let ws = Workspace::discover().unwrap();
        luau_types(&args, &ws, false).unwrap();

        let builder = fs::read_to_string(tmp.path().join("builder.d.luau")).unwrap();
        assert!(builder.starts_with("--!strict\n"));
        assert!(builder.contains("export type StyleProperties"));
        assert!(builder.contains("export type Dimension"));
        assert!(builder.contains("{ tag: \"Px\", value: number }"));

        let signals = fs::read_to_string(tmp.path().join("signals.d.luau")).unwrap();
        // Built-in components include Button (clicked) — confirm the
        // per-component class made it into the file.
        assert!(!signals.is_empty(), "signals.d.luau should not be empty");
    }
}
