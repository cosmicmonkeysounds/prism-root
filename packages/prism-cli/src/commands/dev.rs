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
//! `.prui` skeleton edits are picked up on the next respawn —
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
/// Native run targets (single or `all`) are compiled by **one**
/// combined `cargo build -p prism-shell -p prism-studio -p
/// prism-relay`. Because the package set and feature resolution are
/// identical on every invocation, only genuinely-changed crates
/// recompile — switching between `dev shell`, `dev studio`, and
/// `dev all` no longer ping-pongs shared dependencies through
/// different feature sets. The dev children then *exec the prebuilt
/// binaries* rather than each running its own `cargo run -p <pkg>`
/// (which would re-resolve features and fight the single
/// `target/.cargo-lock`).
const RUN_PACKAGES: [&str; 3] = ["prism-shell", "prism-studio", "prism-relay"];

/// `(label, on-disk bin filename)` for each native run target. The
/// relay's `[[bin]]` is `prism-relayd`, not the package name.
fn bin_file(label: &str) -> &'static str {
    match label {
        "shell" => "prism-shell",
        "studio" => "prism-studio",
        "relay" => "prism-relayd",
        other => unreachable!("no bin mapping for dev label `{other}`"),
    }
}

pub fn plan(args: &DevArgs, workspace: &Workspace) -> Vec<CommandBuilder> {
    match args.target {
        DevTarget::All => all_plan(workspace),
        one => single_plan(one, workspace, args.hot_reload(), args.use_subsecond()),
    }
}

/// The single combined build that warms every native run target
/// under one feature resolution.
fn combined_build_builder(workspace: &Workspace) -> CommandBuilder {
    let mut b = CommandBuilder::cargo().arg("build");
    for pkg in RUN_PACKAGES {
        b = b.package(pkg);
    }
    b.cwd(workspace.root()).label("combined-build")
}

/// Exec an already-built native binary (no cargo in front).
fn bin_exec_builder(workspace: &Workspace, label: &str, watch_ui: bool) -> CommandBuilder {
    let mut b = CommandBuilder::exec(workspace.bin_path(bin_file(label), false))
        .cwd(workspace.root())
        .label(label);
    // C3 — the shell binary's own `.prui` watcher. Applies
    // literal skeleton edits in place; structural changes still fall
    // through to the dev-loop's combined rebuild + re-exec.
    if watch_ui && label == "shell" {
        b = b.arg("--watch-ui");
    }
    b
}

fn single_plan(
    target: DevTarget,
    workspace: &Workspace,
    hot_reload: bool,
    use_subsecond: bool,
) -> Vec<CommandBuilder> {
    match target {
        // Subsecond is special: it needs `--features hot-reload`
        // baked into a cargo *run* of the shell, so it keeps the
        // legacy single-crate cargo path (it intentionally accepts
        // the shell-only feature resolution as the price of the
        // in-process patch anchor).
        DevTarget::Shell if use_subsecond => vec![cargo_run_subsecond_shell(workspace, hot_reload)],
        DevTarget::Shell => vec![
            combined_build_builder(workspace),
            bin_exec_builder(workspace, "shell", hot_reload),
        ],
        DevTarget::Studio => vec![
            combined_build_builder(workspace),
            // `prism-daemond` sidecar must sit next to the studio
            // binary in `target/<profile>/` before studio launches.
            super::build::daemon_bin_builder(workspace, false),
            bin_exec_builder(workspace, "studio", false),
        ],
        DevTarget::Relay => vec![
            combined_build_builder(workspace),
            bin_exec_builder(workspace, "relay", false),
        ],
        DevTarget::Web => vec![
            web_build_builder(workspace),
            super::build::web_bindgen_builder(workspace, false),
            web_serve_builder(workspace),
        ],
        DevTarget::All => unreachable!("handled by all_plan"),
    }
}

fn all_plan(workspace: &Workspace) -> Vec<CommandBuilder> {
    vec![
        combined_build_builder(workspace),
        super::build::daemon_bin_builder(workspace, false),
        bin_exec_builder(workspace, "shell", false),
        bin_exec_builder(workspace, "studio", false),
        bin_exec_builder(workspace, "relay", false),
        web_build_builder(workspace),
        super::build::web_bindgen_builder(workspace, false),
        web_serve_builder(workspace),
    ]
}

fn cargo_run_subsecond_shell(workspace: &Workspace, hot_reload: bool) -> CommandBuilder {
    let mut b = CommandBuilder::cargo()
        .arg("run")
        .package("prism-shell")
        .arg("--features")
        .arg("hot-reload")
        .cwd(workspace.root())
        .label("shell");
    if hot_reload {
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

    // Subsecond shell keeps the legacy single-crate `cargo run`
    // path (it needs `--features hot-reload` on the run itself).
    if args.target == DevTarget::Shell && args.use_subsecond() {
        if args.hot_reload() {
            return exec_dev_loop(
                &plan[0],
                vec![workspace.shell_src_dir(), workspace.shell_ui_dir()],
                None,
            );
        }
        return exec_foreground(&plan[0]);
    }

    // `all` — combined build + daemon + web preflight (sequential,
    // one cargo lock at a time), then the supervisor execs the
    // prebuilt binaries + the static web server. No per-child cargo,
    // so no feature re-resolution and no lock contention.
    if args.target == DevTarget::All {
        let mut supervisor_plan: Vec<CommandBuilder> = Vec::new();
        let mut had_preflight = false;
        for cmd in plan {
            match cmd.label_str() {
                Some("combined-build")
                | Some("web-build")
                | Some("web-bindgen")
                | Some("daemon-build") => {
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
        return runtime.block_on(async move {
            let mut s = Supervisor::new();
            for cmd in supervisor_plan {
                s.add(cmd)?;
            }
            let outcome = s.run().await?;
            Ok(outcome.exit_code)
        });
    }

    // Single native target (shell / studio / relay): one combined
    // build warms the shared dependency graph under a single feature
    // resolution; the dev child then execs the prebuilt binary. With
    // hot-reload the DevLoop re-runs that same combined build before
    // every re-exec — so a code change recompiles only the crates
    // that changed, never the whole shared graph.
    let combined = find_label(&plan, "combined-build");
    let bin = plan
        .iter()
        .find(|c| matches!(c.label_str(), Some("shell" | "studio" | "relay")))
        .expect("single native dev plan must include a run binary");

    if let Some(daemon) = plan.iter().find(|c| c.label_str() == Some("daemon-build")) {
        run_cmd_sync(daemon)?;
    }

    if args.hot_reload() {
        return exec_dev_loop(bin, watch_paths(args.target, workspace), Some(combined));
    }
    run_cmd_sync(combined)?;
    crate::gc::sweep(&workspace.target_dir());
    exec_foreground(bin)
}

fn find_label<'a>(plan: &'a [CommandBuilder], label: &str) -> &'a CommandBuilder {
    plan.iter()
        .find(|c| c.label_str() == Some(label))
        .unwrap_or_else(|| panic!("dev plan must include `{label}`"))
}

/// Source trees a single-target dev loop watches for `.rs` /
/// `.prui` / `.prss` changes.
fn watch_paths(target: DevTarget, workspace: &Workspace) -> Vec<PathBuf> {
    match target {
        DevTarget::Shell => vec![workspace.shell_src_dir(), workspace.shell_ui_dir()],
        DevTarget::Studio => vec![
            workspace.shell_src_dir(),
            workspace.shell_ui_dir(),
            workspace.studio_src_dir(),
        ],
        DevTarget::Relay => vec![workspace.package("prism-relay").join("src")],
        DevTarget::Web | DevTarget::All => unreachable!("not a single-target dev-loop target"),
    }
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

/// Drive a child through the `DevLoop` respawn supervisor. Watches
/// the given source trees and, on every debounced batch, runs
/// `prebuild` (the single combined `cargo build`, if any) and then
/// kills + re-execs the child.
fn exec_dev_loop(
    cmd: &CommandBuilder,
    watch_paths: Vec<PathBuf>,
    prebuild: Option<&CommandBuilder>,
) -> Result<u8> {
    println!("$ {} (hot-reload)", cmd.display());
    let mut dev_loop =
        DevLoop::new(cmd.clone(), watch_paths).with_sink(Arc::new(crate::supervisor::StdoutSink));
    if let Some(pre) = prebuild {
        dev_loop = dev_loop.with_prebuild(pre.clone());
    }

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

    fn combined_argv() -> Vec<&'static str> {
        vec![
            "build",
            "--package",
            "prism-shell",
            "--package",
            "prism-studio",
            "--package",
            "prism-relay",
        ]
    }

    #[test]
    fn shell_default_is_combined_build_plus_bin_exec_with_watch_ui() {
        let p = plan(&args(DevTarget::Shell), &ws());
        assert_eq!(p.len(), 2);
        assert_eq!(p[0].label_str(), Some("combined-build"));
        assert_eq!(p[0].program(), crate::builder::Program::Cargo);
        assert_eq!(p[0].argv().1, combined_argv());
        assert_eq!(p[1].label_str(), Some("shell"));
        // Execs the prebuilt binary directly — no cargo in front, so
        // no per-target feature re-resolution on respawn.
        assert_eq!(
            p[1].display(),
            "/tmp/fake/target/debug/prism-shell --watch-ui"
        );
    }

    #[test]
    fn shell_no_hot_reload_drops_watch_ui_but_keeps_bin_exec() {
        let p = plan(&args_no_reload(DevTarget::Shell), &ws());
        assert_eq!(p.len(), 2);
        assert_eq!(p[1].label_str(), Some("shell"));
        assert_eq!(p[1].display(), "/tmp/fake/target/debug/prism-shell");
    }

    #[test]
    fn shell_subsecond_keeps_cargo_run_feature_path() {
        let p = plan(&args_subsecond(DevTarget::Shell), &ws());
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].label_str(), Some("shell"));
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
    fn subsecond_requires_hot_reload_else_falls_back_to_combined_path() {
        // `--no-hot-reload --hot=subsecond` disables subsecond
        // entirely (it needs the hot-reload apparatus), so this is
        // just the normal combined-build + bin-exec path with no
        // `--watch-ui`.
        let a = DevArgs {
            target: DevTarget::Shell,
            no_hot_reload: true,
            hot: HotReloadStrategy::Subsecond,
        };
        let p = plan(&a, &ws());
        assert_eq!(p.len(), 2);
        assert_eq!(p[0].label_str(), Some("combined-build"));
        assert_eq!(p[1].label_str(), Some("shell"));
        assert_eq!(p[1].display(), "/tmp/fake/target/debug/prism-shell");
    }

    #[test]
    fn studio_is_combined_build_then_daemon_then_studio_binary() {
        let p = plan(&args(DevTarget::Studio), &ws());
        assert_eq!(p.len(), 3);
        assert_eq!(p[0].label_str(), Some("combined-build"));
        assert_eq!(p[0].argv().1, combined_argv());
        assert_eq!(p[1].label_str(), Some("daemon-build"));
        assert_eq!(
            p[1].argv().1,
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
        assert_eq!(p[2].label_str(), Some("studio"));
        assert_eq!(p[2].display(), "/tmp/fake/target/debug/prism-studio");
    }

    #[test]
    fn studio_no_hot_reload_keeps_same_plan_shape() {
        let p = plan(&args_no_reload(DevTarget::Studio), &ws());
        assert_eq!(p.len(), 3);
        assert_eq!(p[2].label_str(), Some("studio"));
        assert_eq!(p[2].display(), "/tmp/fake/target/debug/prism-studio");
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
    fn relay_is_combined_build_then_relayd_binary() {
        let p = plan(&args(DevTarget::Relay), &ws());
        assert_eq!(p.len(), 2);
        assert_eq!(p[0].label_str(), Some("combined-build"));
        // The relay's [[bin]] is `prism-relayd`, not the crate name.
        assert_eq!(p[1].label_str(), Some("relay"));
        assert_eq!(p[1].display(), "/tmp/fake/target/debug/prism-relayd");
    }

    #[test]
    fn all_is_one_combined_build_then_preflight_and_supervised_bins() {
        let p = plan(&args(DevTarget::All), &ws());
        let labels: Vec<_> = p.iter().map(|c| c.label_str().unwrap()).collect();
        assert_eq!(
            labels,
            vec![
                "combined-build",
                "daemon-build",
                "shell",
                "studio",
                "relay",
                "web-build",
                "web-bindgen",
                "web"
            ]
        );
        // Exactly one cargo build warms all native run targets.
        assert_eq!(p[0].argv().1, combined_argv());
        // Supervised children are prebuilt binaries, not `cargo run`.
        assert_eq!(p[2].display(), "/tmp/fake/target/debug/prism-shell");
        assert_eq!(p[3].display(), "/tmp/fake/target/debug/prism-studio");
        assert_eq!(p[4].display(), "/tmp/fake/target/debug/prism-relayd");
    }
}
