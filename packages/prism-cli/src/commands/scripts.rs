//! `prism scripts validate` — compile-only preflight for every Luau
//! glob declared in a project's `.prism.json` `scripts` section.
//!
//! Phase 6d of `docs/dev/luau-integration-plan.md`. The shell wires
//! widgets at runtime through `prism_builder::load_widgets`; this
//! subcommand exists so authors can confirm their scripts parse
//! cleanly without booting the whole shell.

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use prism_builder::{
    load_automations, load_build_steps, load_commands, load_widgets, ComponentRegistry,
    LuauRenderRegistry,
};
use prism_core::identity::manifest::ScriptsConfig;
use serde::Deserialize;

use crate::workspace::Workspace;

#[derive(Debug, Args)]
pub struct ScriptsArgs {
    #[command(subcommand)]
    pub kind: ScriptsKind,
}

#[derive(Debug, Subcommand)]
pub enum ScriptsKind {
    /// Walk every glob declared in `<project>/.prism.json`'s
    /// `scripts` section and compile each Luau file in isolation.
    /// Failures are printed; exit code is non-zero if anything
    /// broke.
    Validate(ValidateArgs),
    /// Run a single Luau script from the project's
    /// `scripts.commands` glob. The script's return value is
    /// printed as JSON.
    Run(RunArgs),
}

#[derive(Debug, Args)]
pub struct ValidateArgs {
    /// Project root directory. Defaults to the current working
    /// directory. The `.prism.json` lives at `<project>/.prism.json`.
    #[arg(long)]
    pub project: Option<PathBuf>,
}

#[derive(Debug, Args)]
pub struct RunArgs {
    /// Name of the command to run (matches the `.luau` file stem in
    /// the `scripts.commands` glob).
    pub name: String,
    /// Project root directory. Defaults to the current working
    /// directory.
    #[arg(long)]
    pub project: Option<PathBuf>,
}

pub fn run(args: &ScriptsArgs, _workspace: &Workspace, dry_run: bool) -> Result<u8> {
    match &args.kind {
        ScriptsKind::Validate(va) => validate(va, dry_run),
        ScriptsKind::Run(ra) => run_script(ra, dry_run),
    }
}

/// Minimal `.prism.json` subset — we only care about the `scripts`
/// section. Decoupled from `prism_core::identity::manifest::PrismManifest`
/// so a malformed unrelated field (legacy collections, missing
/// required keys) doesn't take the validator offline.
#[derive(Debug, Default, Deserialize)]
struct PartialManifest {
    #[serde(default)]
    scripts: Option<ScriptsConfig>,
}

fn validate(args: &ValidateArgs, dry_run: bool) -> Result<u8> {
    let project_root = args
        .project
        .clone()
        .map(Ok)
        .unwrap_or_else(std::env::current_dir)
        .context("resolve project root")?;
    let manifest_path = project_root.join(".prism.json");

    if dry_run {
        println!(
            "$ prism scripts validate (project: {})",
            project_root.display()
        );
        println!("  would read {}", manifest_path.display());
        return Ok(0);
    }

    if !manifest_path.exists() {
        eprintln!(
            "no .prism.json at {} — nothing to validate",
            manifest_path.display()
        );
        return Ok(0);
    }
    let raw = std::fs::read_to_string(&manifest_path)
        .with_context(|| format!("read {}", manifest_path.display()))?;
    let parsed: PartialManifest = serde_json::from_str(&raw)
        .with_context(|| format!("parse {} as JSON", manifest_path.display()))?;
    let scripts = match parsed.scripts {
        Some(s) => s,
        None => {
            println!("{}: no `scripts` section declared", manifest_path.display());
            return Ok(0);
        }
    };

    let mut total_loaded = 0_usize;
    let mut total_failed = 0_usize;

    // Widgets — registered into a throwaway `ComponentRegistry` for
    // validation. The registry doesn't survive the call; this is the
    // same compile-and-discard semantics the other three pipelines
    // already have.
    // `prism_builder::ScriptLoadError` carries `mlua::Error`, which is
    // `!Send + !Sync` — `anyhow::Context` rejects it. Flatten through
    // string conversion at the boundary.
    if let Some(glob) = scripts.widgets.as_deref() {
        let mut luau = LuauRenderRegistry::new();
        let mut comps = ComponentRegistry::new();
        let report = load_widgets(&project_root, glob, &mut luau, &mut comps)
            .map_err(|e| anyhow::anyhow!("load widgets `{glob}`: {e}"))?;
        report_section("widgets", &report.loaded, &report.failures);
        total_loaded += report.loaded.len();
        total_failed += report.failures.len();
    }

    if let Some(glob) = scripts.automations.as_deref() {
        let report = load_automations(&project_root, glob)
            .map_err(|e| anyhow::anyhow!("load automations `{glob}`: {e}"))?;
        report_section("automations", &report.loaded, &report.failures);
        total_loaded += report.loaded.len();
        total_failed += report.failures.len();
    }

    if let Some(glob) = scripts.build_steps.as_deref() {
        let report = load_build_steps(&project_root, glob)
            .map_err(|e| anyhow::anyhow!("load build steps `{glob}`: {e}"))?;
        report_section("build_steps", &report.loaded, &report.failures);
        total_loaded += report.loaded.len();
        total_failed += report.failures.len();
    }

    if let Some(glob) = scripts.commands.as_deref() {
        let report = load_commands(&project_root, glob)
            .map_err(|e| anyhow::anyhow!("load commands `{glob}`: {e}"))?;
        report_section("commands", &report.loaded, &report.failures);
        total_loaded += report.loaded.len();
        total_failed += report.failures.len();
    }

    println!("─");
    println!("total: {total_loaded} ok, {total_failed} failed");

    Ok(if total_failed == 0 { 0 } else { 1 })
}

fn report_section(name: &str, loaded: &[String], failures: &[(PathBuf, String)]) {
    println!("{name}: {} ok, {} failed", loaded.len(), failures.len());
    for id in loaded {
        println!("  ok  {id}");
    }
    for (path, err) in failures {
        println!("  err {} — {err}", path.display());
    }
}

fn run_script(args: &RunArgs, dry_run: bool) -> Result<u8> {
    let project_root = args
        .project
        .clone()
        .map(Ok)
        .unwrap_or_else(std::env::current_dir)
        .context("resolve project root")?;
    let manifest_path = project_root.join(".prism.json");

    if dry_run {
        println!(
            "$ prism scripts run {} (project: {})",
            args.name,
            project_root.display()
        );
        return Ok(0);
    }

    if !manifest_path.exists() {
        anyhow::bail!("no .prism.json at {}", manifest_path.display());
    }
    let raw = std::fs::read_to_string(&manifest_path)
        .with_context(|| format!("read {}", manifest_path.display()))?;
    let parsed: PartialManifest = serde_json::from_str(&raw)
        .with_context(|| format!("parse {} as JSON", manifest_path.display()))?;
    let glob = parsed
        .scripts
        .as_ref()
        .and_then(|s| s.commands.as_deref())
        .ok_or_else(|| anyhow::anyhow!("no `scripts.commands` declared in {}", manifest_path.display()))?;

    // Resolve `<glob-dir>/<name>.luau`. The validator's
    // `resolve_widgets_dir` is private to `prism-builder`; reimplement
    // the trim here so we don't expand the public surface.
    let dir = glob.trim_end_matches("/*.luau").trim_end_matches('/');
    if dir.is_empty() {
        anyhow::bail!("invalid `scripts.commands` glob: `{glob}`");
    }
    let script_path = project_root.join(dir).join(format!("{}.luau", args.name));
    if !script_path.exists() {
        anyhow::bail!("no command `{}` at {}", args.name, script_path.display());
    }
    let source = std::fs::read_to_string(&script_path)
        .with_context(|| format!("read {}", script_path.display()))?;

    let value = prism_daemon::modules::luau_module::exec(&source, None)
        .map_err(|e| anyhow::anyhow!("execute {}: {e}", script_path.display()))?;
    // Pretty-print the result. `JsonValue::Null` collapses to an
    // empty line so a script returning nothing doesn't trail with
    // `null`.
    if value.is_null() {
        return Ok(0);
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&value).context("serialise return value")?
    );
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn write(path: &std::path::Path, body: &str) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, body).unwrap();
    }

    #[test]
    fn validate_with_no_manifest_is_zero() {
        let tmp = tempdir().unwrap();
        let exit = validate(
            &ValidateArgs {
                project: Some(tmp.path().to_path_buf()),
            },
            false,
        )
        .unwrap();
        assert_eq!(exit, 0);
    }

    #[test]
    fn validate_reports_broken_widget_with_nonzero_exit() {
        let tmp = tempdir().unwrap();
        // Manifest declares only the `widgets` glob. We don't drive
        // a full PrismManifest here — `PartialManifest` is what the
        // validator actually parses, and it accepts a minimal blob.
        write(
            &tmp.path().join(".prism.json"),
            r#"{ "scripts": { "widgets": "widgets/*.luau" } }"#,
        );
        write(
            &tmp.path().join("widgets/ok.luau"),
            r#"
            prism.widget {
                id = "ok-luau",
                render = function() return { component = "text" } end,
            }
            "#,
        );
        write(&tmp.path().join("widgets/bad.luau"), "this is not luau");

        let exit = validate(
            &ValidateArgs {
                project: Some(tmp.path().to_path_buf()),
            },
            false,
        )
        .unwrap();
        assert_eq!(exit, 1, "broken widget should produce non-zero exit");
    }

    #[test]
    fn validate_clean_project_returns_zero() {
        let tmp = tempdir().unwrap();
        write(
            &tmp.path().join(".prism.json"),
            r#"{ "scripts": { "automations": "automations/*.luau" } }"#,
        );
        write(
            &tmp.path().join("automations/notify.luau"),
            "return function() return 1 end",
        );
        let exit = validate(
            &ValidateArgs {
                project: Some(tmp.path().to_path_buf()),
            },
            false,
        )
        .unwrap();
        assert_eq!(exit, 0);
    }

    #[test]
    fn validate_no_scripts_section_returns_zero() {
        let tmp = tempdir().unwrap();
        write(&tmp.path().join(".prism.json"), r#"{ "name": "x" }"#);
        let exit = validate(
            &ValidateArgs {
                project: Some(tmp.path().to_path_buf()),
            },
            false,
        )
        .unwrap();
        assert_eq!(exit, 0);
    }

    #[test]
    fn run_script_executes_named_command() {
        let tmp = tempdir().unwrap();
        write(
            &tmp.path().join(".prism.json"),
            r#"{ "scripts": { "commands": "commands/*.luau" } }"#,
        );
        write(
            &tmp.path().join("commands/ping.luau"),
            "return 'pong'",
        );
        let exit = run_script(
            &RunArgs {
                name: "ping".to_string(),
                project: Some(tmp.path().to_path_buf()),
            },
            false,
        )
        .unwrap();
        assert_eq!(exit, 0);
    }

    #[test]
    fn run_script_errors_on_unknown_command() {
        let tmp = tempdir().unwrap();
        write(
            &tmp.path().join(".prism.json"),
            r#"{ "scripts": { "commands": "commands/*.luau" } }"#,
        );
        let err = run_script(
            &RunArgs {
                name: "missing".to_string(),
                project: Some(tmp.path().to_path_buf()),
            },
            false,
        )
        .unwrap_err();
        assert!(err.to_string().contains("no command `missing`"));
    }

    #[test]
    fn run_script_errors_when_no_commands_glob() {
        let tmp = tempdir().unwrap();
        write(&tmp.path().join(".prism.json"), "{}");
        let err = run_script(
            &RunArgs {
                name: "any".to_string(),
                project: Some(tmp.path().to_path_buf()),
            },
            false,
        )
        .unwrap_err();
        assert!(err.to_string().contains("no `scripts.commands`"));
    }
}
