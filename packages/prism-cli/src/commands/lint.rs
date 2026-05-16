//! `prism lint` — workspace clippy with `-D warnings`, plus the
//! optional `--types` Luau type pass (§3.3 of
//! `docs/dev/prism-cross-cutting-systems.md`).
//!
//! The type pass is deliberately **not faked**: the doc is explicit
//! that a stubbed typechecker is worse than none because it trains
//! authors to distrust diagnostics. So `prism lint --types`:
//!
//! * runs the real external `luau-analyze` over every `.luau` source
//!   in the workspace (strict mode) and returns its exit code — a
//!   genuine CI gate when the toolchain is installed;
//! * when `luau-analyze` is **absent**, prints an actionable install
//!   hint and *skips* (exit 0) rather than passing silently or
//!   failing the build. The gate activates the moment the binary is
//!   on `PATH`; until then it is honestly inert.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::Result;
use clap::Args;

use crate::builder::CommandBuilder;
use crate::workspace::Workspace;

#[derive(Debug, Args, Default)]
pub struct LintArgs {
    /// Also run the Luau type pass (`luau-analyze`, strict mode) over
    /// every `.luau` source in the workspace. No-op-with-hint when
    /// `luau-analyze` is not installed.
    #[arg(long)]
    pub types: bool,
}

pub fn plan(workspace: &Workspace) -> Vec<CommandBuilder> {
    vec![CommandBuilder::cargo()
        .arg("clippy")
        .workspace()
        .arg("--all-targets")
        .arg("--")
        .arg("-D")
        .arg("warnings")
        .cwd(workspace.root())
        .label("clippy")]
}

pub fn run(args: &LintArgs, workspace: &Workspace, dry_run: bool) -> Result<u8> {
    let plan = plan(workspace);
    let code = super::execute_plan(&plan, dry_run)?;
    if code != 0 || !args.types {
        return Ok(code);
    }
    types_pass(workspace, dry_run)
}

/// Resolve the `luau-analyze` binary: the `LUAU_ANALYZE` env override
/// wins (lets a workspace pin a bundled copy), otherwise the first
/// `luau-analyze` on `PATH`. `None` when neither resolves.
fn locate_luau_analyze() -> Option<PathBuf> {
    if let Ok(explicit) = std::env::var("LUAU_ANALYZE") {
        let p = PathBuf::from(&explicit);
        if p.is_file() {
            return Some(p);
        }
    }
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join("luau-analyze");
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// Every `.luau` source under `root`, skipping regenerable / vendored
/// trees. Order is deterministic (sorted) so the analyzer's output is
/// stable across runs.
fn collect_luau_sources(root: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if path.is_dir() {
                if matches!(
                    name.as_ref(),
                    "target" | ".git" | "node_modules" | "dist" | ".husky"
                ) {
                    continue;
                }
                walk(&path, out);
            } else if path.extension().is_some_and(|e| e == "luau") {
                out.push(path);
            }
        }
    }
    let mut out = Vec::new();
    walk(root, &mut out);
    out.sort();
    out
}

fn types_pass(workspace: &Workspace, dry_run: bool) -> Result<u8> {
    let Some(bin) = locate_luau_analyze() else {
        eprintln!(
            "prism lint --types: `luau-analyze` not found — skipping the Luau type pass.\n\
             \n\
             This gate is intentionally inert until the real toolchain is installed\n\
             (a stubbed typechecker would train you to distrust diagnostics — see\n\
             docs/dev/prism-cross-cutting-systems.md §3.3 / §7). To enable it:\n\
             \n\
             • install Luau and put `luau-analyze` on PATH, or\n\
             • point the `LUAU_ANALYZE` env var at a bundled binary.\n"
        );
        return Ok(0);
    };

    let sources = collect_luau_sources(workspace.root());
    if sources.is_empty() {
        eprintln!("prism lint --types: no `.luau` sources found — nothing to check.");
        return Ok(0);
    }

    let mut cmd = Command::new(&bin);
    cmd.arg("--mode").arg("strict");
    cmd.args(&sources);
    cmd.current_dir(workspace.root());

    if dry_run {
        let argv: Vec<String> = std::iter::once(bin.display().to_string())
            .chain(["--mode".into(), "strict".into()])
            .chain(sources.iter().map(|p| p.display().to_string()))
            .collect();
        println!("[dry-run] {}", argv.join(" "));
        return Ok(0);
    }

    eprintln!(
        "prism lint --types: {} ({} sources, strict)",
        bin.display(),
        sources.len()
    );
    let status = cmd.status()?;
    Ok(status.code().unwrap_or(1) as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clippy_plan_matches_root_claude_md() {
        let p = plan(&Workspace::new("/tmp/fake"));
        assert_eq!(p.len(), 1);
        assert_eq!(
            p[0].argv().1,
            vec![
                "clippy",
                "--workspace",
                "--all-targets",
                "--",
                "-D",
                "warnings"
            ]
        );
    }

    #[test]
    fn collect_luau_sources_finds_dot_luau_and_skips_target() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        std::fs::write(root.join("a.luau"), "--!strict\nreturn 1").unwrap();
        std::fs::create_dir_all(root.join("pkg")).unwrap();
        std::fs::write(root.join("pkg/b.luau"), "return 2").unwrap();
        std::fs::create_dir_all(root.join("target/debug")).unwrap();
        std::fs::write(root.join("target/debug/c.luau"), "return 3").unwrap();
        std::fs::write(root.join("d.rs"), "fn main() {}").unwrap();

        let found = collect_luau_sources(root);
        let names: Vec<String> = found
            .iter()
            .map(|p| p.strip_prefix(root).unwrap().to_string_lossy().into_owned())
            .collect();
        assert!(names.contains(&"a.luau".to_string()));
        assert!(names.contains(&format!("pkg{}b.luau", std::path::MAIN_SEPARATOR)));
        assert!(
            !names.iter().any(|n| n.contains("target")),
            "target/ must be skipped, got {names:?}"
        );
        assert!(!names.iter().any(|n| n.ends_with(".rs")));
    }

    #[test]
    fn types_pass_skips_cleanly_when_analyzer_absent() {
        // With LUAU_ANALYZE pointing nowhere and (assumed) no
        // luau-analyze on the test PATH, the pass must be a clean
        // skip — exit 0, never a fake pass or a hard failure.
        if locate_luau_analyze().is_some() {
            // CI image actually has the toolchain; the skip branch
            // can't be exercised here. The presence path is covered
            // by the real gate when it runs.
            return;
        }
        let ws = Workspace::new("/tmp/prism-lint-types-absent");
        let code = types_pass(&ws, false).unwrap();
        assert_eq!(code, 0, "absent analyzer must skip with exit 0");
    }
}
