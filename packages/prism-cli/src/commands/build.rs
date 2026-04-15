//! `prism build` — the unified builder.
//!
//! Targets map 1:1 to the deployables the workspace cares about:
//!
//! - `desktop` — `cargo build -p prism-shell` (the tao dev bin).
//! - `studio`  — `cargo build -p prism-studio` (the packaged
//!   desktop shell; bundling/signing lives in Phase 5 via
//!   `cargo-packager`).
//! - `web`     — `cargo build --target wasm32-unknown-emscripten
//!               -p prism-shell --no-default-features --features web`
//!   followed by a copy step that drops the emitted
//!   `prism_shell_wasm.{js,wasm}` pair into
//!   `packages/prism-shell/web/` next to `index.html` / `loader.js`.
//! - `relay`   — `cargo build -p prism-relay` (the Rust axum SSR
//!   server). The Hono TS relay was retired 2026-04-15.
//! - `all`     — every target above, in the order listed.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use clap::{Args, ValueEnum};

use crate::builder::CommandBuilder;
use crate::workspace::Workspace;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum BuildTarget {
    Desktop,
    Studio,
    Web,
    Relay,
    All,
}

/// Flags for `prism build`.
#[derive(Debug, Clone, Args)]
pub struct BuildArgs {
    /// Which target to build. Defaults to `all`.
    #[arg(long, value_enum, default_value_t = BuildTarget::All)]
    pub target: BuildTarget,

    /// Force a debug build. By default `prism build` produces
    /// release artifacts; use this flag for fast iteration.
    #[arg(long)]
    pub debug: bool,
}

/// Pure-data plan for `prism build`.
pub fn plan(args: &BuildArgs, workspace: &Workspace) -> Vec<CommandBuilder> {
    let targets: Vec<BuildTarget> = match args.target {
        BuildTarget::All => vec![
            BuildTarget::Desktop,
            BuildTarget::Studio,
            BuildTarget::Web,
            BuildTarget::Relay,
        ],
        one => vec![one],
    };

    let mut plan = Vec::new();
    for target in targets {
        match target {
            BuildTarget::Desktop => {
                let mut cmd = CommandBuilder::cargo()
                    .arg("build")
                    .package("prism-shell")
                    .label("desktop-build");
                if !args.debug {
                    cmd = cmd.release();
                }
                plan.push(cmd.cwd(workspace.root()));
            }
            BuildTarget::Studio => {
                let mut cmd = CommandBuilder::cargo()
                    .arg("build")
                    .package("prism-studio")
                    .label("studio-build");
                if !args.debug {
                    cmd = cmd.release();
                }
                plan.push(cmd.cwd(workspace.root()));
            }
            BuildTarget::Web => {
                let mut cmd = CommandBuilder::cargo()
                    .arg("build")
                    .arg("--target")
                    .arg("wasm32-unknown-emscripten")
                    .package("prism-shell")
                    .arg("--no-default-features")
                    .arg("--features")
                    .arg("web")
                    .label("web-build");
                if !args.debug {
                    cmd = cmd.release();
                }
                plan.push(cmd.cwd(workspace.root()));
            }
            BuildTarget::Relay => {
                let mut cmd = CommandBuilder::cargo()
                    .arg("build")
                    .package("prism-relay")
                    .label("relay-build");
                if !args.debug {
                    cmd = cmd.release();
                }
                plan.push(cmd.cwd(workspace.root()));
            }
            BuildTarget::All => unreachable!("expanded above"),
        }
    }
    plan
}

pub fn run(args: &BuildArgs, workspace: &Workspace, dry_run: bool) -> Result<u8> {
    let plan = plan(args, workspace);
    let code = super::execute_plan(&plan, dry_run)?;
    if code != 0 {
        return Ok(code);
    }
    // `cargo build -p prism-shell --target wasm32-unknown-emscripten`
    // drops `prism_shell_wasm.js` + `prism_shell_wasm.wasm` under
    // `target/wasm32-unknown-emscripten/<profile>/`. `loader.js`
    // imports `./prism_shell_wasm.js` from next to `index.html`, so
    // the shipping layout expects the pair to live in
    // `packages/prism-shell/web/`. The supervisor path and the
    // single-target path both route through here, so the copy
    // happens exactly once per `prism build`.
    let web_included = matches!(args.target, BuildTarget::Web | BuildTarget::All);
    if web_included && !dry_run {
        copy_wasm_artifacts(workspace, !args.debug)?;
    }
    Ok(0)
}

/// Copy the emscripten-produced `prism_shell_wasm.{js,wasm}` pair
/// into `packages/prism-shell/web/` so the loader.js next to
/// `index.html` can `import "./prism_shell_wasm.js"`. Returns the
/// destination paths for logging/tests.
pub(crate) fn copy_wasm_artifacts(workspace: &Workspace, release: bool) -> Result<()> {
    let src_dir = workspace.wasm_artifact_dir(release);
    let dst_dir = workspace.shell_web_dir();
    fs::create_dir_all(&dst_dir)
        .with_context(|| format!("creating web artifact dir {}", dst_dir.display()))?;
    for name in ["prism_shell_wasm.js", "prism_shell_wasm.wasm"] {
        copy_one(&src_dir.join(name), &dst_dir.join(name))?;
    }
    Ok(())
}

fn copy_one(src: &Path, dst: &Path) -> Result<()> {
    fs::copy(src, dst)
        .with_context(|| format!("copying {} -> {}", src.display(), dst.display()))?;
    println!("$ cp {} {}", src.display(), dst.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ws() -> Workspace {
        Workspace::new("/tmp/fake")
    }

    fn args(target: BuildTarget) -> BuildArgs {
        BuildArgs {
            target,
            debug: false,
        }
    }

    #[test]
    fn all_target_fans_out_to_four() {
        let p = plan(&args(BuildTarget::All), &ws());
        assert_eq!(p.len(), 4);
        let labels: Vec<_> = p.iter().map(|c| c.label_str().unwrap()).collect();
        assert_eq!(
            labels,
            vec!["desktop-build", "studio-build", "web-build", "relay-build"]
        );
    }

    #[test]
    fn desktop_defaults_to_release() {
        let p = plan(&args(BuildTarget::Desktop), &ws());
        assert_eq!(
            p[0].argv().1,
            vec!["build", "--package", "prism-shell", "--release"]
        );
    }

    #[test]
    fn desktop_debug_omits_release_flag() {
        let mut a = args(BuildTarget::Desktop);
        a.debug = true;
        let p = plan(&a, &ws());
        assert_eq!(p[0].argv().1, vec!["build", "--package", "prism-shell"]);
    }

    #[test]
    fn studio_uses_cargo_build_release_by_default() {
        let p = plan(&args(BuildTarget::Studio), &ws());
        assert_eq!(
            p[0].argv().1,
            vec!["build", "--package", "prism-studio", "--release"]
        );
    }

    #[test]
    fn studio_debug_omits_release_flag() {
        let mut a = args(BuildTarget::Studio);
        a.debug = true;
        let p = plan(&a, &ws());
        assert_eq!(p[0].argv().1, vec!["build", "--package", "prism-studio"]);
    }

    #[test]
    fn web_uses_cargo_emscripten_target_with_web_feature() {
        let p = plan(&args(BuildTarget::Web), &ws());
        let argv = p[0].argv().1;
        assert_eq!(argv[0], "build");
        assert_eq!(argv[1], "--target");
        assert_eq!(argv[2], "wasm32-unknown-emscripten");
        assert_eq!(argv[3], "--package");
        assert_eq!(argv[4], "prism-shell");
        assert!(argv.contains(&"--no-default-features".to_string()));
        assert!(argv.contains(&"--features".to_string()));
        assert!(argv.contains(&"web".to_string()));
        assert!(argv.contains(&"--release".to_string()));
        assert_eq!(p[0].label_str(), Some("web-build"));
    }

    #[test]
    fn web_debug_omits_release_flag() {
        let mut a = args(BuildTarget::Web);
        a.debug = true;
        let p = plan(&a, &ws());
        let argv = p[0].argv().1;
        assert!(!argv.contains(&"--release".to_string()));
    }

    #[test]
    fn relay_uses_cargo_build_release_by_default() {
        let p = plan(&args(BuildTarget::Relay), &ws());
        assert_eq!(
            p[0].argv().1,
            vec!["build", "--package", "prism-relay", "--release"]
        );
        assert_eq!(p[0].label_str(), Some("relay-build"));
    }

    #[test]
    fn relay_debug_omits_release_flag() {
        let mut a = args(BuildTarget::Relay);
        a.debug = true;
        let p = plan(&a, &ws());
        assert_eq!(p[0].argv().1, vec!["build", "--package", "prism-relay"]);
    }
}
