//! `prism dev` — spawn one or many dev servers through the supervisor.
//!
//! The legal targets match the top-level `pnpm dev:*` scripts in
//! the root `package.json`:
//!
//! - `shell`  — `cargo run -p prism-shell` (native tao+wgpu dev bin).
//! - `studio` — `cargo run -p prism-studio` (packaged desktop shell).
//! - `web`    — `cargo build --target wasm32-unknown-emscripten
//!   -p prism-shell --features web`, copy artifacts into
//!   `packages/prism-shell/web/`, then serve that directory via
//!   `python3 -m http.server 1420`.
//! - `relay`  — `cargo run -p prism-relay` (Rust axum Sovereign
//!   Portal SSR server; replaced the Hono TS relay 2026-04-15).
//! - `all`    — every target above, spawned in parallel behind the
//!   supervisor.

use anyhow::Result;
use clap::{Args, ValueEnum};

use crate::builder::CommandBuilder;
use crate::supervisor::Supervisor;
use crate::workspace::Workspace;

/// TCP port the web static server listens on during `prism dev web`.
/// Matches the port `packages/prism-relay` historically squatted on
/// so the dev experience stays uniform across targets.
const WEB_DEV_PORT: &str = "1420";

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum DevTarget {
    Shell,
    Studio,
    Web,
    Relay,
    All,
}

/// Flags for `prism dev`.
#[derive(Debug, Clone, Args)]
pub struct DevArgs {
    /// Which dev server(s) to run. Defaults to `shell`.
    #[arg(value_enum, default_value_t = DevTarget::Shell)]
    pub target: DevTarget,
}

/// Resolve the dev target into a list of labeled command builders.
///
/// The web target expands into *two* builders in dry-run order: the
/// emscripten cargo build followed by the static server. The copy
/// step that lives between them is an in-process `std::fs` call
/// handled inside [`run`] — it's not representable as an argv, so
/// dry-run prints it as a `cp …` pseudo-command in
/// [`run`]/[`prepare_web_artifacts`].
pub fn plan(args: &DevArgs, workspace: &Workspace) -> Vec<CommandBuilder> {
    let targets: Vec<DevTarget> = match args.target {
        DevTarget::All => vec![
            DevTarget::Shell,
            DevTarget::Studio,
            DevTarget::Web,
            DevTarget::Relay,
        ],
        one => vec![one],
    };

    let mut out = Vec::new();
    for t in targets {
        for b in builders_for(t, workspace) {
            out.push(b);
        }
    }
    out
}

fn builders_for(target: DevTarget, workspace: &Workspace) -> Vec<CommandBuilder> {
    match target {
        DevTarget::Shell => vec![CommandBuilder::cargo()
            .arg("run")
            .package("prism-shell")
            .cwd(workspace.root())
            .label("shell")],
        DevTarget::Studio => vec![CommandBuilder::cargo()
            .arg("run")
            .package("prism-studio")
            .cwd(workspace.root())
            .label("studio")],
        DevTarget::Web => vec![web_build_builder(workspace), web_serve_builder(workspace)],
        DevTarget::Relay => vec![CommandBuilder::cargo()
            .arg("run")
            .package("prism-relay")
            .cwd(workspace.root())
            .label("relay")],
        DevTarget::All => unreachable!("expanded above"),
    }
}

fn web_build_builder(workspace: &Workspace) -> CommandBuilder {
    CommandBuilder::cargo()
        .arg("build")
        .arg("--target")
        .arg("wasm32-unknown-emscripten")
        .package("prism-shell")
        .arg("--no-default-features")
        .arg("--features")
        .arg("web")
        .cwd(workspace.root())
        .label("web-build")
}

fn web_serve_builder(workspace: &Workspace) -> CommandBuilder {
    CommandBuilder::python3()
        .arg("-m")
        .arg("http.server")
        .arg(WEB_DEV_PORT)
        .arg("--directory")
        .arg(workspace.shell_web_dir().to_string_lossy().into_owned())
        .cwd(workspace.root())
        .label("web")
}

pub fn run(args: &DevArgs, workspace: &Workspace, dry_run: bool) -> Result<u8> {
    let plan = plan(args, workspace);
    let web_included = matches!(args.target, DevTarget::Web | DevTarget::All);

    if dry_run {
        for cmd in &plan {
            println!("$ [{}] {}", cmd.label_str().unwrap_or("?"), cmd.display());
        }
        if web_included {
            println!(
                "$ [web-build] cp {}/prism_shell_wasm.{{js,wasm}} {}",
                workspace.wasm_artifact_dir(false).display(),
                workspace.shell_web_dir().display()
            );
        }
        return Ok(0);
    }

    // Single-target web dev is three steps: cargo build, copy, serve.
    // The build + copy are synchronous preflight; the serve is the
    // long-running foreground exec that Ctrl+C drops onto.
    if args.target == DevTarget::Web {
        let (build_cmd, serve_cmd) = single_web_pair(&plan);
        run_cmd_sync(build_cmd)?;
        super::build::copy_wasm_artifacts(workspace, false)?;
        return exec_foreground(serve_cmd);
    }

    // Single-target (non-web) dev is just a foreground exec — no
    // supervisor overhead, so Ctrl+C still lands on the child
    // directly.
    if plan.len() == 1 {
        return exec_foreground(&plan[0]);
    }

    // Multi-target dev. Web needs its preflight (cargo + copy) to
    // finish before the supervisor starts fanning out workers, so
    // the supervisor sees a clean list of long-running children:
    // shell, studio, web-serve, relay.
    let supervisor_plan: Vec<CommandBuilder> = if web_included {
        // Extract the web-build step, run it synchronously, copy,
        // then hand the remaining builders to the supervisor.
        let mut remaining = Vec::with_capacity(plan.len().saturating_sub(1));
        let mut ran_web_build = false;
        for cmd in plan {
            if cmd.label_str() == Some("web-build") {
                run_cmd_sync(&cmd)?;
                ran_web_build = true;
            } else {
                remaining.push(cmd);
            }
        }
        if ran_web_build {
            super::build::copy_wasm_artifacts(workspace, false)?;
        }
        remaining
    } else {
        plan
    };

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async move {
        let mut s = Supervisor::new();
        for cmd in supervisor_plan {
            s.add(cmd)?;
        }
        let outcome = s.run().await?;
        Ok(outcome.exit_code)
    })
}

/// Pull the `web-build` and `web` builders out of a single-target
/// web plan. Panics if the plan shape doesn't match — the only
/// caller is `run`, which has already verified the target.
fn single_web_pair(plan: &[CommandBuilder]) -> (&CommandBuilder, &CommandBuilder) {
    let build = plan
        .iter()
        .find(|c| c.label_str() == Some("web-build"))
        .expect("web plan must include web-build");
    let serve = plan
        .iter()
        .find(|c| c.label_str() == Some("web"))
        .expect("web plan must include web serve");
    (build, serve)
}

fn run_cmd_sync(cmd: &CommandBuilder) -> Result<()> {
    println!("$ {}", cmd.display());
    let status = cmd
        .build()
        .status()
        .map_err(|e| anyhow::anyhow!("failed to spawn `{}`: {e}", cmd.display()))?;
    if !status.success() {
        anyhow::bail!(
            "`{}` exited with code {}",
            cmd.display(),
            status.code().unwrap_or(1)
        );
    }
    Ok(())
}

fn exec_foreground(cmd: &CommandBuilder) -> Result<u8> {
    println!("$ {}", cmd.display());
    let status = cmd
        .build()
        .status()
        .map_err(|e| anyhow::anyhow!("failed to spawn `{}`: {e}", cmd.display()))?;
    Ok(status.code().unwrap_or(1) as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ws() -> Workspace {
        Workspace::new("/tmp/fake")
    }

    #[test]
    fn shell_is_the_default_target() {
        let a = DevArgs {
            target: DevTarget::Shell,
        };
        let p = plan(&a, &ws());
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].label_str(), Some("shell"));
        assert_eq!(p[0].argv().1, vec!["run", "--package", "prism-shell"]);
    }

    #[test]
    fn studio_runs_cargo_run_on_prism_studio() {
        let a = DevArgs {
            target: DevTarget::Studio,
        };
        let p = plan(&a, &ws());
        let argv = p[0].argv().1;
        assert_eq!(argv, vec!["run", "--package", "prism-studio"]);
        assert_eq!(p[0].label_str(), Some("studio"));
    }

    #[test]
    fn web_expands_into_build_plus_python_serve() {
        let a = DevArgs {
            target: DevTarget::Web,
        };
        let p = plan(&a, &ws());
        assert_eq!(p.len(), 2);
        assert_eq!(p[0].label_str(), Some("web-build"));
        assert_eq!(p[0].program(), crate::builder::Program::Cargo);
        let build_argv = p[0].argv().1;
        assert_eq!(build_argv[0], "build");
        assert!(build_argv.contains(&"wasm32-unknown-emscripten".to_string()));
        assert!(build_argv.contains(&"prism-shell".to_string()));
        assert!(build_argv.contains(&"--no-default-features".to_string()));
        assert!(build_argv.contains(&"web".to_string()));

        assert_eq!(p[1].label_str(), Some("web"));
        assert_eq!(p[1].program(), crate::builder::Program::Python3);
        let serve_argv = p[1].argv().1;
        assert_eq!(serve_argv[0], "-m");
        assert_eq!(serve_argv[1], "http.server");
        assert_eq!(serve_argv[2], WEB_DEV_PORT);
        assert_eq!(serve_argv[3], "--directory");
        assert!(serve_argv[4].ends_with("packages/prism-shell/web"));
    }

    #[test]
    fn relay_runs_cargo_run_on_prism_relay() {
        let a = DevArgs {
            target: DevTarget::Relay,
        };
        let p = plan(&a, &ws());
        assert_eq!(p[0].argv().1, vec!["run", "--package", "prism-relay"]);
        assert_eq!(p[0].label_str(), Some("relay"));
    }

    #[test]
    fn all_target_fans_out_to_five_labeled_commands() {
        // shell + studio + web-build + web (serve) + relay
        let a = DevArgs {
            target: DevTarget::All,
        };
        let p = plan(&a, &ws());
        assert_eq!(p.len(), 5);
        let labels: Vec<_> = p.iter().map(|c| c.label_str().unwrap()).collect();
        assert_eq!(labels, vec!["shell", "studio", "web-build", "web", "relay"]);
    }
}
