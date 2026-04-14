//! `prism build` — the unified builder.
//!
//! Targets map 1:1 to the deployables the workspace cares about:
//!
//! - `desktop` — `cargo build -p prism-shell` (the native dev bin).
//! - `studio`  — `cargo tauri build` against the Studio config (the
//!   packaged desktop app).
//! - `web`     — `trunk build [--release]` against prism-shell's
//!   Trunk.toml (the WASM entry point).
//! - `relay`   — `pnpm --filter @prism/relay build` (TypeScript).
//! - `all`     — every target above, in the order listed.

use anyhow::Result;
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
                let mut cmd = CommandBuilder::tauri()
                    .arg("build")
                    .arg("--config")
                    .arg(workspace.tauri_config().to_string_lossy().into_owned())
                    .label("studio-build");
                if args.debug {
                    cmd = cmd.arg("--debug");
                }
                plan.push(cmd.cwd(workspace.root()));
            }
            BuildTarget::Web => {
                let mut cmd = CommandBuilder::trunk()
                    .arg("build")
                    .arg("--config")
                    .arg(workspace.trunk_config().to_string_lossy().into_owned())
                    .label("web-build");
                if !args.debug {
                    cmd = cmd.arg("--release");
                }
                plan.push(cmd.cwd(workspace.root()));
            }
            BuildTarget::Relay => {
                let cmd = CommandBuilder::pnpm()
                    .arg("--filter")
                    .arg("@prism/relay")
                    .arg("run")
                    .arg("build")
                    .cwd(workspace.root())
                    .label("relay-build");
                plan.push(cmd);
            }
            BuildTarget::All => unreachable!("expanded above"),
        }
    }
    plan
}

pub fn run(args: &BuildArgs, workspace: &Workspace, dry_run: bool) -> Result<u8> {
    let plan = plan(args, workspace);
    super::execute_plan(&plan, dry_run)
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
    fn studio_uses_tauri_build_with_config() {
        let p = plan(&args(BuildTarget::Studio), &ws());
        let argv = p[0].argv().1;
        assert_eq!(argv[0], "tauri");
        assert_eq!(argv[1], "build");
        assert_eq!(argv[2], "--config");
        assert!(argv[3].ends_with("packages/prism-studio/src-tauri/tauri.conf.json"));
    }

    #[test]
    fn studio_debug_passes_through() {
        let mut a = args(BuildTarget::Studio);
        a.debug = true;
        let p = plan(&a, &ws());
        let argv = p[0].argv().1;
        assert!(argv.contains(&"--debug".to_string()));
    }

    #[test]
    fn web_uses_trunk_build_with_config() {
        let p = plan(&args(BuildTarget::Web), &ws());
        let argv = p[0].argv().1;
        assert_eq!(argv[0], "build");
        assert_eq!(argv[1], "--config");
        assert!(argv[2].ends_with("packages/prism-shell/Trunk.toml"));
        assert!(argv.contains(&"--release".to_string()));
    }

    #[test]
    fn relay_uses_pnpm_filter_build() {
        let p = plan(&args(BuildTarget::Relay), &ws());
        assert_eq!(
            p[0].argv().1,
            vec!["--filter", "@prism/relay", "run", "build"]
        );
    }
}
