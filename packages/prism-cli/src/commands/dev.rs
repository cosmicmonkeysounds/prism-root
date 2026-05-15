//! `prism dev` — spawn one or many dev servers through the supervisor.
//!
//! The legal targets match the top-level `pnpm dev:*` scripts in
//! the root `package.json`:
//!
//! - `shell`  — `cargo run -p prism-shell` (native dev bin —
//!   prism-ui-runtime + femtovg).
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
//! Single-target `prism dev shell` runs the cargo child inside a
//! [`crate::dev_loop::DevLoop`] which watches
//! `packages/prism-shell/src/` (plus any extra roots the supervisor
//! adds) for `.rs` changes and kills + respawns the child when a
//! batch lands. cargo's incremental compilation keeps iteration fast.
//! `.prism-ui` skeleton edits are picked up on the next respawn —
//! the source-first runtime parses the file at boot. `--no-hot-reload`
//! disables the respawn loop.

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

    /// Disable the `.rs` respawn loop. Useful when debugging
    /// something the watcher otherwise obscures.
    #[arg(long = "no-hot-reload", default_value_t = false)]
    pub no_hot_reload: bool,

    /// Phase 9 of `docs/dev/dioxus-inspiration.md`: select the
    /// hot-reload strategy. `respawn` (default) keeps the existing
    /// kill-and-respawn loop; `subsecond` compiles the shell with
    /// `--features hot-reload` so `subsecond::call` wraps the
    /// render walk, letting the patch pipeline swap in a changed
    /// `lower_ui` body without dropping the `Surface` tree or
    /// reactive `Owner` graph. Falls back to `respawn` for changes
    /// subsecond can't patch (struct-layout edits, public-API
    /// breaks).
    #[arg(long = "hot", value_enum, default_value_t = HotReloadStrategy::Respawn)]
    pub hot: HotReloadStrategy,
}

#[derive(clap::ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotReloadStrategy {
    /// Kill the child on every `.rs` change and re-exec `cargo run`.
    /// Today's default.
    Respawn,
    /// Compile the shell with `--features hot-reload` and lean on
    /// `subsecond::call` to patch the render-walk body in place.
    /// The patch pipeline integration itself (separate cargo
    /// build that emits the runtime patch) is a sibling
    /// `prism dev` follow-up; today this strategy turns on the
    /// subsecond anchor in the shell binary and otherwise falls
    /// through to respawn.
    Subsecond,
}

impl DevArgs {
    /// True when hot-reload is active for this invocation.
    pub fn hot_reload(&self) -> bool {
        !self.no_hot_reload
    }

    /// True when the user opted into the subsecond hot-patch path.
    pub fn use_subsecond(&self) -> bool {
        self.hot == HotReloadStrategy::Subsecond && self.hot_reload()
    }
}

/// Resolve the dev target into a list of labeled command builders.
///
/// The web target expands into *three* builders: the cargo wasm
/// build, the wasm-bindgen post-process, and the python static
/// server. The first two are synchronous preflight handled inside
/// [`run`]; the static server is the long-running foreground child.
///
/// The `.rs` respawn half is wired up separately in [`run`] — only
/// single-target `prism dev shell` dispatches through
/// [`crate::dev_loop::DevLoop`].
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
        for b in builders_for(t, workspace, args.hot_reload(), args.use_subsecond()) {
            out.push(b);
        }
    }
    out
}

fn builders_for(
    target: DevTarget,
    workspace: &Workspace,
    hot_reload: bool,
    use_subsecond: bool,
) -> Vec<CommandBuilder> {
    match target {
        DevTarget::Shell => vec![cargo_run_dev_builder(
            "prism-shell",
            "shell",
            workspace,
            hot_reload,
            use_subsecond,
        )],
        DevTarget::Studio => vec![
            // Studio's `prism-daemond` sidecar lives next to the
            // studio binary in `target/<profile>/`, so the cargo
            // build for it has to land in the same profile before
            // `cargo run -p prism-studio` fires. Treated as
            // synchronous preflight (like web-build / web-bindgen),
            // not a long-running supervised child.
            super::build::daemon_bin_builder(workspace, false),
            cargo_run_dev_builder(
                "prism-studio",
                "studio",
                workspace,
                hot_reload,
                use_subsecond,
            ),
        ],
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
    hot_reload: bool,
    use_subsecond: bool,
) -> CommandBuilder {
    let mut b = CommandBuilder::cargo()
        .arg("run")
        .package(package)
        .cwd(workspace.root())
        .label(label);
    if use_subsecond && package == "prism-shell" {
        // Phase 9: turn on the `subsecond::call` anchor in the
        // shell binary. The patch pipeline itself ships the
        // generated dylib through `subsecond::register_handler` at
        // runtime; that's a follow-up.
        b = b.arg("--features").arg("hot-reload");
    }
    // C3 — pass `--watch-ui` to the shell binary so `prism dev
    // shell` boots with the `.prism-ui` hot-reload watcher
    // attached. Edits apply in place via
    // `Shell::install_default_skeleton` /
    // `install_app_skeleton` without a cargo respawn. The
    // dev_loop's `.prism-ui` extension still triggers a respawn
    // on structural changes that need a fresh cargo build, but
    // the watcher catches literal edits first.
    if hot_reload && package == "prism-shell" {
        b = b.arg("--").arg("--watch-ui");
    }
    b
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
        crate::gc::sweep(&workspace.target_dir());
        return exec_foreground(serve_cmd);
    }

    // Single-target studio dev is two steps: cargo build the daemon
    // sidecar (synchronous preflight, drops `prism-daemond` into
    // target/debug/), then the long-running `cargo run -p
    // prism-studio` — wrapped in a DevLoop when hot-reload is on,
    // foreground exec otherwise.
    if args.target == DevTarget::Studio {
        let daemon = plan
            .iter()
            .find(|c| c.label_str() == Some("daemon-build"))
            .expect("studio plan must include daemon-build");
        let studio = plan
            .iter()
            .find(|c| c.label_str() == Some("studio"))
            .expect("studio plan must include studio");
        run_cmd_sync(daemon)?;
        if args.hot_reload() {
            return exec_dev_loop(
                studio,
                vec![
                    workspace.shell_src_dir(),
                    workspace.shell_ui_dir(),
                    workspace.studio_src_dir(),
                ],
            );
        }
        return exec_foreground(studio);
    }

    // Single-target shell with hot-reload on: wrap the cargo child
    // in a DevLoop so `.rs` and `.prism-ui` changes kill + respawn
    // the process. Wave 11.5 — the skeleton directory is watched
    // alongside `src/` so editing `ui/app.prism-ui` picks up on
    // the next boot.
    if args.target == DevTarget::Shell && args.hot_reload() && plan.len() == 1 {
        return exec_dev_loop(
            &plan[0],
            vec![workspace.shell_src_dir(), workspace.shell_ui_dir()],
        );
    }

    // Single-target (non-web, non-studio) dev is just a foreground
    // exec — no supervisor overhead, so Ctrl+C still lands on the
    // child directly.
    if plan.len() == 1 {
        return exec_foreground(&plan[0]);
    }

    // Multi-target dev. Web needs its preflight (cargo + wasm-bindgen)
    // and Studio needs its daemon-sidecar prebuild to finish before
    // the supervisor starts fanning out workers, so the supervisor
    // sees a clean list of long-running children: shell, studio,
    // web-serve, relay.
    let mut supervisor_plan: Vec<CommandBuilder> = Vec::with_capacity(plan.len());
    let mut had_preflight = false;
    for cmd in plan {
        match cmd.label_str() {
            Some("web-build") | Some("web-bindgen") | Some("daemon-build") => {
                run_cmd_sync(&cmd)?;
                had_preflight = true;
            }
            _ => supervisor_plan.push(cmd),
        }
    }
    if had_preflight {
        crate::gc::sweep(&workspace.target_dir());
    }

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
/// respawns the child on every debounced batch.
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
            hot: HotReloadStrategy::Respawn,
        }
    }

    fn args_no_reload(target: DevTarget) -> DevArgs {
        DevArgs {
            target,
            no_hot_reload: true,
            hot: HotReloadStrategy::Respawn,
        }
    }

    fn args_subsecond(target: DevTarget) -> DevArgs {
        DevArgs {
            target,
            no_hot_reload: false,
            hot: HotReloadStrategy::Subsecond,
        }
    }

    #[test]
    fn shell_is_the_default_target() {
        let a = args(DevTarget::Shell);
        let p = plan(&a, &ws());
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].label_str(), Some("shell"));
        let argv = p[0].argv().1;
        // C3 — when hot-reload is on (the default), pass
        // `--watch-ui` so the shell binary attaches its
        // `.prism-ui` watcher and applies edits in-place.
        assert_eq!(
            argv,
            vec!["run", "--package", "prism-shell", "--", "--watch-ui"]
        );
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
    fn shell_with_subsecond_strategy_injects_hot_reload_feature() {
        // Phase 9 of `docs/dev/dioxus-inspiration.md`. The
        // `--hot=subsecond` flag wires `--features hot-reload` onto
        // the shell's cargo invocation; the in-shell anchor
        // (`subsecond::call` around `render_tree`) goes live.
        let a = args_subsecond(DevTarget::Shell);
        let p = plan(&a, &ws());
        assert_eq!(p.len(), 1);
        assert_eq!(
            p[0].argv().1,
            vec![
                "run",
                "--package",
                "prism-shell",
                "--features",
                "hot-reload",
                "--",
                "--watch-ui"
            ]
        );
    }

    #[test]
    fn subsecond_strategy_disabled_when_no_hot_reload() {
        // `--no-hot-reload --hot=subsecond` is the "drop the whole
        // hot-reload apparatus" combo; the feature flag must not be
        // injected.
        let a = DevArgs {
            target: DevTarget::Shell,
            no_hot_reload: true,
            hot: HotReloadStrategy::Subsecond,
        };
        let p = plan(&a, &ws());
        let argv = p[0].argv().1;
        assert_eq!(argv, vec!["run", "--package", "prism-shell"]);
    }

    #[test]
    fn studio_prebuilds_daemon_sidecar() {
        let a = args(DevTarget::Studio);
        let p = plan(&a, &ws());
        assert_eq!(p.len(), 2);
        assert_eq!(p[0].label_str(), Some("daemon-build"));
        assert_eq!(
            p[0].argv().1,
            vec![
                "build",
                "--package",
                "prism-daemon",
                "--bin",
                "prism-daemond",
                "--features",
                "transport-ipc"
            ]
        );
        assert_eq!(p[1].label_str(), Some("studio"));
        assert_eq!(p[1].argv().1, vec!["run", "--package", "prism-studio"]);
    }

    #[test]
    fn studio_no_hot_reload_same_as_default() {
        let a = args_no_reload(DevTarget::Studio);
        let p = plan(&a, &ws());
        assert_eq!(p.len(), 2);
        assert_eq!(p[0].label_str(), Some("daemon-build"));
        assert_eq!(p[1].argv().1, vec!["run", "--package", "prism-studio"]);
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
    fn all_target_fans_out_to_seven_labeled_commands() {
        // shell + daemon-build + studio + web-build + web-bindgen + web (serve) + relay
        let a = args(DevTarget::All);
        let p = plan(&a, &ws());
        assert_eq!(p.len(), 7);
        let labels: Vec<_> = p.iter().map(|c| c.label_str().unwrap()).collect();
        assert_eq!(
            labels,
            vec![
                "shell",
                "daemon-build",
                "studio",
                "web-build",
                "web-bindgen",
                "web",
                "relay"
            ]
        );
    }
}
