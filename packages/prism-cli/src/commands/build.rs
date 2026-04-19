//! `prism build` — the unified builder.
//!
//! Targets map 1:1 to the deployables the workspace cares about:
//!
//! - `desktop` — `cargo build -p prism-shell` (the native Slint dev bin).
//! - `studio`  — `cargo build -p prism-studio` (the packaged
//!   desktop shell; bundling/signing lives in Phase 5 via
//!   `cargo-packager`).
//! - `web`     — two steps:
//!     1. `cargo build --target wasm32-unknown-unknown
//!        -p prism-shell --no-default-features --features web`,
//!     2. `wasm-bindgen --target web --out-dir packages/prism-shell/web
//!        target/wasm32-unknown-unknown/<profile>/prism_shell.wasm`.
//!
//!   wasm-bindgen writes `prism_shell.js` + `prism_shell_bg.wasm`
//!   directly next to `index.html` — no post-copy step.
//! - `relay`   — `cargo build -p prism-relay` (the Rust axum SSR
//!   server). The Hono TS relay was retired 2026-04-15.
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
                plan.push(build_cargo_target(
                    "prism-shell",
                    "desktop-build",
                    workspace,
                    !args.debug,
                ));
            }
            BuildTarget::Studio => {
                plan.push(build_cargo_target(
                    "prism-studio",
                    "studio-build",
                    workspace,
                    !args.debug,
                ));
            }
            BuildTarget::Web => {
                let mut cargo_cmd = CommandBuilder::cargo()
                    .arg("build")
                    .arg("--target")
                    .arg("wasm32-unknown-unknown")
                    .package("prism-shell")
                    .arg("--no-default-features")
                    .arg("--features")
                    .arg("web")
                    .label("web-build");
                if !args.debug {
                    cargo_cmd = cargo_cmd.release();
                }
                plan.push(cargo_cmd.cwd(workspace.root()));
                plan.push(web_bindgen_builder(workspace, !args.debug));
            }
            BuildTarget::Relay => {
                plan.push(build_cargo_target(
                    "prism-relay",
                    "relay-build",
                    workspace,
                    !args.debug,
                ));
            }
            BuildTarget::All => unreachable!("expanded above"),
        }
    }
    plan
}

fn build_cargo_target(
    package: &str,
    label: &str,
    workspace: &Workspace,
    release: bool,
) -> CommandBuilder {
    let mut cmd = CommandBuilder::cargo()
        .arg("build")
        .package(package)
        .label(label);
    if release {
        cmd = cmd.release();
    }
    cmd.cwd(workspace.root())
}

/// `wasm-bindgen --target web --out-dir <shell-web-dir> <cargo-wasm>`.
/// Exposed `pub(crate)` so `dev.rs` can reuse the same builder
/// without duplicating the argv.
pub(crate) fn web_bindgen_builder(workspace: &Workspace, release: bool) -> CommandBuilder {
    CommandBuilder::wasm_bindgen()
        .arg("--target")
        .arg("web")
        .arg("--out-dir")
        .arg(workspace.shell_web_dir().to_string_lossy().into_owned())
        .arg(
            workspace
                .shell_wasm_artifact(release)
                .to_string_lossy()
                .into_owned(),
        )
        .cwd(workspace.root())
        .label("web-bindgen")
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
    fn all_target_fans_out_to_five() {
        // desktop + studio + web-build + web-bindgen + relay
        let p = plan(&args(BuildTarget::All), &ws());
        assert_eq!(p.len(), 5);
        let labels: Vec<_> = p.iter().map(|c| c.label_str().unwrap()).collect();
        assert_eq!(
            labels,
            vec![
                "desktop-build",
                "studio-build",
                "web-build",
                "web-bindgen",
                "relay-build"
            ]
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
    fn web_cargo_step_uses_wasm32_unknown_unknown_with_web_feature() {
        let p = plan(&args(BuildTarget::Web), &ws());
        assert_eq!(p.len(), 2);
        let argv = p[0].argv().1;
        assert_eq!(argv[0], "build");
        assert_eq!(argv[1], "--target");
        assert_eq!(argv[2], "wasm32-unknown-unknown");
        assert_eq!(argv[3], "--package");
        assert_eq!(argv[4], "prism-shell");
        assert!(argv.contains(&"--no-default-features".to_string()));
        assert!(argv.contains(&"--features".to_string()));
        assert!(argv.contains(&"web".to_string()));
        assert!(argv.contains(&"--release".to_string()));
        assert_eq!(p[0].label_str(), Some("web-build"));
    }

    #[test]
    fn web_bindgen_step_targets_web_out_dir_and_release_wasm() {
        let p = plan(&args(BuildTarget::Web), &ws());
        assert_eq!(p[1].program(), crate::builder::Program::WasmBindgen);
        assert_eq!(p[1].label_str(), Some("web-bindgen"));
        let argv = p[1].argv().1;
        assert_eq!(argv[0], "--target");
        assert_eq!(argv[1], "web");
        assert_eq!(argv[2], "--out-dir");
        assert!(argv[3].ends_with("packages/prism-shell/web"));
        assert!(argv[4].ends_with("wasm32-unknown-unknown/release/prism_shell.wasm"));
    }

    #[test]
    fn web_debug_omits_release_flag_and_reads_debug_wasm() {
        let mut a = args(BuildTarget::Web);
        a.debug = true;
        let p = plan(&a, &ws());
        assert!(!p[0].argv().1.contains(&"--release".to_string()));
        let bindgen_argv = p[1].argv().1;
        assert!(bindgen_argv[4].ends_with("wasm32-unknown-unknown/debug/prism_shell.wasm"));
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
