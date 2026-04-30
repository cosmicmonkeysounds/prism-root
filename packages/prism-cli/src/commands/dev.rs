//! `prism dev` — spawn one or many dev servers through the supervisor.
//!
//! The legal targets match the top-level `pnpm dev:*` scripts in
//! the root `package.json`:
//!
//! - `shell`  — `cargo run -p prism-shell` (native Slint dev bin).
//! - `studio` — `cargo run -p prism-studio` (packaged desktop shell).
//! - `web`    — two preflight steps followed by a static server:
//!     1. `cargo build --target wasm32-unknown-unknown
//!        -p prism-shell --no-default-features --features web`,
//!     2. `wasm-bindgen --target web --out-dir packages/prism-shell/web
//!        target/wasm32-unknown-unknown/<profile>/prism_shell.wasm`,
//!     3. `python3 -m http.server 1420 --directory packages/prism-shell/web`
//!        as the long-running foreground child.
//! - `relay`  — `cargo run -p prism-relay` (Rust axum Sovereign
//!   Portal SSR server; replaced the Hono TS relay 2026-04-15).
//! - `all`    — every target above, spawned in parallel behind the
//!   supervisor (web's preflight runs synchronously first).
//!
//! ## Hot-reload
//!
//! **`.rs` files** — single-target `prism dev shell` runs the
//! cargo child inside a [`crate::dev_loop::DevLoop`] which watches
//! `packages/prism-shell/src/` (plus any extra roots the supervisor
//! adds) for `.rs` changes and kills + respawns the child when a
//! batch lands. cargo's incremental compilation keeps iteration fast.
//!
//! **Slint live-preview is disabled.** The interpreter's
//! `ChangeTracker` drops `VRc<ItemTree>` during Flickable geometry
//! binding evaluation, triggering "Recursion detected" panics.
//! Confirmed on both Slint 1.15.1 and 1.16.0 — this is an upstream
//! interpreter bug. `.slint` changes require a rebuild (the `.rs`
//! respawn loop handles this automatically). The interpreter remains
//! available for `prism-builder`'s runtime compilation of isolated
//! component fragments, which doesn't hit this bug.
//!
//! `--no-hot-reload` disables the `.rs` respawn loop.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use clap::{Args, ValueEnum};

use crate::builder::CommandBuilder;
use crate::dev_loop::DevLoop;
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

    /// Disable the unified Slint + Rust hot-reload path. Drops the
    /// `SLINT_LIVE_PREVIEW=1` env var, the
    /// `--features prism-shell/live-preview` flag, and the `.rs`
    /// respawn loop. Use this when the interpreter's compile cost is
    /// unacceptable or when debugging something the extra wiring
    /// obscures.
    #[arg(long = "no-hot-reload", default_value_t = false)]
    pub no_hot_reload: bool,
}

impl DevArgs {
    /// True when hot-reload is active for this invocation.
    pub fn hot_reload(&self) -> bool {
        !self.no_hot_reload
    }
}

/// Resolve the dev target into a list of labeled command builders.
///
/// The web target expands into *three* builders: the cargo wasm
/// build, the wasm-bindgen post-process, and the python static
/// server. The first two are synchronous preflight handled inside
/// [`run`]; the static server is the long-running foreground child.
///
/// Slint live-preview is disabled (interpreter VRc panic on both
/// 1.15.1 and 1.16.0). The `.rs` respawn half is wired up separately
/// in [`run`] — only single-target `prism dev shell` dispatches
/// through [`crate::dev_loop::DevLoop`].
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
        for b in builders_for(t, workspace, args.hot_reload()) {
            out.push(b);
        }
    }
    out
}

fn builders_for(target: DevTarget, workspace: &Workspace, hot_reload: bool) -> Vec<CommandBuilder> {
    match target {
        DevTarget::Shell => vec![cargo_run_dev_builder(
            "prism-shell",
            "shell",
            workspace,
            hot_reload,
        )],
        DevTarget::Studio => vec![cargo_run_dev_builder(
            "prism-studio",
            "studio",
            workspace,
            hot_reload,
        )],
        DevTarget::Web => vec![
            web_build_builder(workspace),
            super::build::web_bindgen_builder(workspace, false),
            web_serve_builder(workspace),
        ],
        DevTarget::Relay => vec![CommandBuilder::cargo()
            .arg("run")
            .package("prism-relay")
            .cwd(workspace.root())
            .label("relay")],
        DevTarget::All => unreachable!("expanded above"),
    }
}

fn cargo_run_dev_builder(
    package: &str,
    label: &str,
    workspace: &Workspace,
    _hot_reload: bool,
) -> CommandBuilder {
    CommandBuilder::cargo()
        .arg("run")
        .package(package)
        .cwd(workspace.root())
        .label(label)
}

fn web_build_builder(workspace: &Workspace) -> CommandBuilder {
    CommandBuilder::cargo()
        .arg("build")
        .arg("--target")
        .arg("wasm32-unknown-unknown")
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

    if dry_run {
        for cmd in &plan {
            println!("$ [{}] {}", cmd.label_str().unwrap_or("?"), cmd.display());
        }
        return Ok(0);
    }

    // Single-target web dev is three steps: cargo build, wasm-bindgen,
    // python serve. The cargo + wasm-bindgen pair are synchronous
    // preflight; the serve is the long-running foreground exec that
    // Ctrl+C drops onto.
    if args.target == DevTarget::Web {
        let (build_cmd, bindgen_cmd, serve_cmd) = single_web_trio(&plan);
        run_cmd_sync(build_cmd)?;
        run_cmd_sync(bindgen_cmd)?;
        crate::gc::trim_incremental(&workspace.target_dir());
        return exec_foreground(serve_cmd);
    }

    // Single-target shell or studio with hot-reload on: wrap the
    // cargo child in a DevLoop so `.rs` changes kill + respawn the
    // process. The `.slint` half is already handled in-process by
    // Slint's live-preview (enabled via env var + feature above).
    if args.target == DevTarget::Shell && args.hot_reload() && plan.len() == 1 {
        return exec_dev_loop(&plan[0], vec![workspace.shell_src_dir()]);
    }
    if args.target == DevTarget::Studio && args.hot_reload() && plan.len() == 1 {
        return exec_dev_loop(
            &plan[0],
            vec![workspace.shell_src_dir(), workspace.studio_src_dir()],
        );
    }

    // Single-target (non-web) dev is just a foreground exec — no
    // supervisor overhead, so Ctrl+C still lands on the child
    // directly.
    if plan.len() == 1 {
        return exec_foreground(&plan[0]);
    }

    // Multi-target dev. Web needs its preflight (cargo + wasm-bindgen)
    // to finish before the supervisor starts fanning out workers, so
    // the supervisor sees a clean list of long-running children:
    // shell, studio, web-serve, relay.
    let web_included = matches!(args.target, DevTarget::Web | DevTarget::All);
    let supervisor_plan: Vec<CommandBuilder> = if web_included {
        let mut remaining = Vec::with_capacity(plan.len().saturating_sub(2));
        for cmd in plan {
            match cmd.label_str() {
                Some("web-build") | Some("web-bindgen") => run_cmd_sync(&cmd)?,
                _ => remaining.push(cmd),
            }
        }
        crate::gc::trim_incremental(&workspace.target_dir());
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

/// Pull the `web-build`, `web-bindgen`, and `web` builders out of a
/// single-target web plan. Panics if the plan shape doesn't match
/// — the only caller is `run`, which has already verified the target.
fn single_web_trio(plan: &[CommandBuilder]) -> (&CommandBuilder, &CommandBuilder, &CommandBuilder) {
    let build = plan
        .iter()
        .find(|c| c.label_str() == Some("web-build"))
        .expect("web plan must include web-build");
    let bindgen = plan
        .iter()
        .find(|c| c.label_str() == Some("web-bindgen"))
        .expect("web plan must include web-bindgen");
    let serve = plan
        .iter()
        .find(|c| c.label_str() == Some("web"))
        .expect("web plan must include web serve");
    (build, bindgen, serve)
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

/// Drive a cargo child through the `DevLoop` respawn supervisor.
/// Watches the given source trees for `.rs` changes and kills +
/// respawns the child on every debounced batch. `.slint` changes
/// also trigger a respawn since live-preview is disabled (interpreter
/// VRc panic — see ADR-007).
fn exec_dev_loop(cmd: &CommandBuilder, watch_paths: Vec<PathBuf>) -> Result<u8> {
    println!("$ {} (hot-reload)", cmd.display());
    let dev_loop =
        DevLoop::new(cmd.clone(), watch_paths).with_sink(Arc::new(crate::supervisor::StdoutSink));

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async move {
        let outcome = dev_loop.run().await?;
        Ok(outcome.exit_code)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ws() -> Workspace {
        Workspace::new("/tmp/fake")
    }

    fn args(target: DevTarget) -> DevArgs {
        DevArgs {
            target,
            no_hot_reload: false,
        }
    }

    fn args_no_reload(target: DevTarget) -> DevArgs {
        DevArgs {
            target,
            no_hot_reload: true,
        }
    }

    #[test]
    fn shell_is_the_default_target() {
        let a = args(DevTarget::Shell);
        let p = plan(&a, &ws());
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].label_str(), Some("shell"));
        let argv = p[0].argv().1;
        assert_eq!(argv, vec!["run", "--package", "prism-shell"]);
    }

    #[test]
    fn shell_no_hot_reload_same_as_default() {
        let a = args_no_reload(DevTarget::Shell);
        let p = plan(&a, &ws());
        assert_eq!(p.len(), 1);
        let argv = p[0].argv().1;
        assert_eq!(argv, vec!["run", "--package", "prism-shell"]);
    }

    #[test]
    fn shell_never_sets_slint_live_preview_env() {
        let a = args(DevTarget::Shell);
        let p = plan(&a, &ws());
        let built = p[0].build();
        let envs: Vec<_> = built
            .get_envs()
            .filter_map(|(k, _)| k.to_str().map(|s| s.to_string()))
            .collect();
        assert!(
            !envs.iter().any(|k| k == "SLINT_LIVE_PREVIEW"),
            "live-preview disabled: interpreter VRc panic on 1.15.1 and 1.16.0"
        );
    }

    #[test]
    fn studio_runs_without_live_preview() {
        let a = args(DevTarget::Studio);
        let p = plan(&a, &ws());
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].label_str(), Some("studio"));
        let argv = p[0].argv().1;
        assert_eq!(argv, vec!["run", "--package", "prism-studio"]);
    }

    #[test]
    fn studio_no_hot_reload_same_as_default() {
        let a = args_no_reload(DevTarget::Studio);
        let p = plan(&a, &ws());
        assert_eq!(p.len(), 1);
        let argv = p[0].argv().1;
        assert_eq!(argv, vec!["run", "--package", "prism-studio"]);
    }

    #[test]
    fn web_expands_into_build_bindgen_plus_python_serve() {
        let a = args(DevTarget::Web);
        let p = plan(&a, &ws());
        assert_eq!(p.len(), 3);

        assert_eq!(p[0].label_str(), Some("web-build"));
        assert_eq!(p[0].program(), crate::builder::Program::Cargo);
        let build_argv = p[0].argv().1;
        assert_eq!(build_argv[0], "build");
        assert!(build_argv.contains(&"wasm32-unknown-unknown".to_string()));
        assert!(build_argv.contains(&"prism-shell".to_string()));
        assert!(build_argv.contains(&"--no-default-features".to_string()));
        assert!(build_argv.contains(&"web".to_string()));

        assert_eq!(p[1].label_str(), Some("web-bindgen"));
        assert_eq!(p[1].program(), crate::builder::Program::WasmBindgen);
        let bindgen_argv = p[1].argv().1;
        assert_eq!(bindgen_argv[0], "--target");
        assert_eq!(bindgen_argv[1], "web");
        assert_eq!(bindgen_argv[2], "--out-dir");
        assert!(bindgen_argv[3].ends_with("packages/prism-shell/web"));
        assert!(bindgen_argv[4].ends_with("wasm32-unknown-unknown/debug/prism_shell.wasm"));

        assert_eq!(p[2].label_str(), Some("web"));
        assert_eq!(p[2].program(), crate::builder::Program::Python3);
        let serve_argv = p[2].argv().1;
        assert_eq!(serve_argv[0], "-m");
        assert_eq!(serve_argv[1], "http.server");
        assert_eq!(serve_argv[2], WEB_DEV_PORT);
        assert_eq!(serve_argv[3], "--directory");
        assert!(serve_argv[4].ends_with("packages/prism-shell/web"));
    }

    #[test]
    fn relay_runs_cargo_run_on_prism_relay() {
        let a = args(DevTarget::Relay);
        let p = plan(&a, &ws());
        assert_eq!(p[0].argv().1, vec!["run", "--package", "prism-relay"]);
        assert_eq!(p[0].label_str(), Some("relay"));
    }

    #[test]
    fn all_target_fans_out_to_six_labeled_commands() {
        // shell + studio + web-build + web-bindgen + web (serve) + relay
        let a = args(DevTarget::All);
        let p = plan(&a, &ws());
        assert_eq!(p.len(), 6);
        let labels: Vec<_> = p.iter().map(|c| c.label_str().unwrap()).collect();
        assert_eq!(
            labels,
            vec![
                "shell",
                "studio",
                "web-build",
                "web-bindgen",
                "web",
                "relay"
            ]
        );
    }

    #[test]
    fn all_target_does_not_enable_live_preview() {
        let a = args(DevTarget::All);
        let p = plan(&a, &ws());
        let shell = p
            .iter()
            .find(|c| c.label_str() == Some("shell"))
            .expect("shell slot in all plan");
        assert!(
            !shell.argv().1.contains(&"live-preview".to_string()),
            "live-preview disabled: interpreter VRc panic"
        );
    }
}
