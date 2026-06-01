//! `prism loom` — language + hosting tools for the Loom storytelling
//! language.
//!
//! Subcommands:
//!
//! - `prism loom lsp` — run the in-process Language Server Protocol
//!   implementation against stdio. Editors that auto-discover the
//!   binary (Zed via the bundled extension, Neovim via
//!   `mason-lspconfig`, etc.) launch it for `.loom` files and get
//!   diagnostics + hover + completion + semantic-token highlights
//!   straight from `loom_parser`.
//! - `prism loom build` — produce the artifacts needed for a
//!   self-hosted deploy: the React editor's `dist/` (via
//!   `pnpm build` in `packages/loom/editor`) and the
//!   `loom-relayd` binary (`cargo build -p loom-server`).
//! - `prism loom serve` — run the relay binary, pointed at the
//!   editor's `dist/`, so one process serves the editor + API + WS.
//!
//! See `docs/dev/loom-multiuser.md` (Phase 8) for the topology.

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use clap::{Args, Subcommand, ValueEnum};

use crate::builder::CommandBuilder;
use crate::workspace::Workspace;

#[derive(Debug, Args)]
pub struct LoomArgs {
    #[command(subcommand)]
    pub kind: LoomKind,
}

#[derive(Debug, Subcommand)]
pub enum LoomKind {
    /// Run the Loom Language Server Protocol implementation against
    /// stdio. Editors connect by spawning this process and speaking
    /// JSON-RPC over its stdin/stdout.
    Lsp,
    /// Build the artifacts needed to self-host the Loom editor:
    /// `packages/loom/editor/dist/` (Vite build) and the
    /// `loom-relayd` binary.
    Build(LoomBuildArgs),
    /// Serve the Loom editor + API + WS from a single `loom-relayd`
    /// process. Pass `--build` to rebuild the editor + binary first.
    Serve(LoomServeArgs),
    /// Launch the PySide6 Loom Runtime / Simulator GUI against a
    /// project directory. Builds the `loom-play` driver first and
    /// then execs `python3 packages/loom/simulator/simulator.py`.
    /// Friends only need `pip install pyside6` once.
    Sim(LoomSimArgs),
    /// One-shot dev environment: prebuilds the wasm bundle + relay
    /// binary, then runs the Vite editor (HMR) and `loom-relayd` side
    /// by side under the prism supervisor. Prints both URLs so you
    /// can click through to either.
    Dev(LoomDevArgs),
}

#[derive(Debug, Args)]
pub struct LoomSimArgs {
    /// Project directory to load on startup. Defaults to
    /// `packages/loom/examples/saltmere`.
    pub project: Option<PathBuf>,
    /// Skip rebuilding the `loom-play` driver before launching.
    #[arg(long)]
    pub skip_build: bool,
    /// Build the release-profile driver (slower compile, faster
    /// playback). Defaults to the debug profile.
    #[arg(long)]
    pub ship: bool,
    /// Override the Python interpreter. Defaults to `python3`.
    #[arg(long, default_value = "python3")]
    pub python: String,
}

#[derive(Debug, Args)]
pub struct LoomBuildArgs {
    /// Build the runtime-optimised release profile (`cargo build
    /// --release`). Defaults to the fast debug profile.
    #[arg(long)]
    pub ship: bool,
    /// Skip the editor build step (only build the relay binary).
    /// Useful when iterating on the relay against an already-built
    /// editor.
    #[arg(long)]
    pub skip_editor: bool,
    /// Skip the relay-binary build step (only build the editor).
    #[arg(long)]
    pub skip_server: bool,
}

#[derive(Debug, Args)]
pub struct LoomServeArgs {
    /// Run `prism loom build` first.
    #[arg(long)]
    pub build: bool,
    /// Use the release-profile binary (and pass `--ship` to
    /// `--build` when set).
    #[arg(long)]
    pub ship: bool,
    /// Address to bind the listener on. Defaults to
    /// `127.0.0.1:7878` (set `0.0.0.0:7878` to expose on the LAN).
    #[arg(long, default_value = "127.0.0.1:7878")]
    pub bind: String,
    /// Override the editor `dist/` path. Defaults to
    /// `packages/loom/editor/dist` inside the workspace.
    #[arg(long)]
    pub editor_dist: Option<PathBuf>,
    /// CORS posture for the API + WS routes. `same-origin` is the
    /// right default for self-hosting; pass `permissive` when
    /// driving the relay from the Vite dev server.
    #[arg(long, value_enum, default_value_t = LoomCorsArg::SameOrigin)]
    pub cors: LoomCorsArg,
    /// DID the relay advertises as its issuer (forwarded as
    /// `--relay-did` to the binary).
    #[arg(long)]
    pub relay_did: Option<String>,
    /// Don't auto-build the editor when its `dist/` is missing —
    /// instead, abort with an actionable error. Defaults to
    /// auto-building.
    #[arg(long)]
    pub no_auto_build: bool,
}

#[derive(Debug, Args)]
pub struct LoomDevArgs {
    /// TCP port for the Vite editor (HMR). Defaults to 5173 — the
    /// editor's runtime auto-detection treats 5173/4173 as dev ports
    /// and points its API+WS URLs at `127.0.0.1:<relay-port>`.
    #[arg(long, default_value_t = 5173)]
    pub ui_port: u16,
    /// TCP port for `loom-relayd`. Defaults to 7878.
    #[arg(long, default_value_t = 7878)]
    pub relay_port: u16,
    /// Bind address. Defaults to `127.0.0.1`; pass `0.0.0.0` to
    /// expose both servers on the LAN.
    #[arg(long, default_value = "127.0.0.1")]
    pub host: String,
    /// Run only the Vite editor (skip the relay).
    #[arg(long, conflicts_with = "relay_only")]
    pub ui_only: bool,
    /// Run only the relay (skip Vite).
    #[arg(long)]
    pub relay_only: bool,
    /// Skip the wasm-bundle preflight rebuild. The editor falls back
    /// to whatever is committed in `editor/src/loom-wasm`.
    #[arg(long)]
    pub no_wasm: bool,
    /// Use the release-profile relay binary (slow compile, fast
    /// runtime). Defaults to debug.
    #[arg(long)]
    pub ship: bool,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum LoomCorsArg {
    SameOrigin,
    Permissive,
}

impl LoomCorsArg {
    fn as_flag(self) -> &'static str {
        match self {
            LoomCorsArg::SameOrigin => "same-origin",
            LoomCorsArg::Permissive => "permissive",
        }
    }
}

pub fn run(args: &LoomArgs, workspace: &Workspace, dry_run: bool) -> Result<u8> {
    match &args.kind {
        LoomKind::Lsp => run_lsp(dry_run),
        LoomKind::Build(args) => run_build(args, workspace, dry_run),
        LoomKind::Serve(args) => run_serve(args, workspace, dry_run),
        LoomKind::Sim(args) => run_sim(args, workspace, dry_run),
        LoomKind::Dev(args) => run_dev(args, workspace, dry_run),
    }
}

fn run_sim(args: &LoomSimArgs, workspace: &Workspace, dry_run: bool) -> Result<u8> {
    let mut plan = Vec::new();
    if !args.skip_build {
        let mut build = CommandBuilder::cargo()
            .arg("build")
            .package("loom-runtime")
            .arg("--bin")
            .arg("loom-play")
            .label("loom-play-build");
        if args.ship {
            build = build.release();
        }
        plan.push(build.cwd(workspace.root()));
    }

    let simulator = workspace
        .package("loom")
        .join("simulator")
        .join("simulator.py");
    let project = args
        .project
        .clone()
        .unwrap_or_else(|| workspace.package("loom").join("examples").join("saltmere"));

    let mut launch = CommandBuilder::exec(PathBuf::from(&args.python))
        .arg(simulator.to_string_lossy().into_owned())
        .arg(project.to_string_lossy().into_owned())
        .label("loom-sim")
        .cwd(workspace.root());
    // The simulator finds the binary via `target/{debug,release}/loom-play`;
    // pass a hint env var so a `--ship` build is picked up first.
    if args.ship {
        launch = launch.env("LOOM_PLAY_PROFILE", "release");
    }
    plan.push(launch);

    super::execute_plan(&plan, dry_run)
}

fn run_lsp(dry_run: bool) -> Result<u8> {
    if dry_run {
        println!("$ loom-lsp  # would run the LSP loop on stdio");
        return Ok(0);
    }
    loom_lsp::run_stdio()?;
    Ok(0)
}

fn run_build(args: &LoomBuildArgs, workspace: &Workspace, dry_run: bool) -> Result<u8> {
    let plan = build_plan(args, workspace);
    super::execute_plan(&plan, dry_run)
}

/// Plan for `prism loom build`. Exposed for unit tests.
pub fn build_plan(args: &LoomBuildArgs, workspace: &Workspace) -> Vec<CommandBuilder> {
    let mut plan = Vec::new();
    if !args.skip_editor {
        plan.push(editor_build_builder(workspace));
    }
    if !args.skip_server {
        plan.push(server_build_builder(workspace, args.ship));
    }
    plan
}

fn editor_build_builder(workspace: &Workspace) -> CommandBuilder {
    CommandBuilder::pnpm()
        .arg("build")
        .cwd(loom_editor_dir(workspace))
        .label("loom-editor-build")
}

fn server_build_builder(workspace: &Workspace, release: bool) -> CommandBuilder {
    let mut cmd = CommandBuilder::cargo()
        .arg("build")
        .package("loom-server")
        .label("loom-server-build");
    if release {
        cmd = cmd.release();
    }
    cmd.cwd(workspace.root())
}

fn run_serve(args: &LoomServeArgs, workspace: &Workspace, dry_run: bool) -> Result<u8> {
    // Resolve editor dist (default = packages/loom/editor/dist).
    let editor_dist = args
        .editor_dist
        .clone()
        .unwrap_or_else(|| loom_editor_dist(workspace));

    let mut plan = Vec::new();

    let need_editor_build = args.build || (!editor_dist_ready(&editor_dist) && !args.no_auto_build);
    let need_server_build = args.build;

    if need_editor_build {
        plan.push(editor_build_builder(workspace));
    }
    if need_server_build {
        plan.push(server_build_builder(workspace, args.ship));
    } else if !workspace.bin_path("loom-relayd", args.ship).is_file() {
        // No prebuilt binary — build the server anyway so `serve`
        // works as a one-shot.
        plan.push(server_build_builder(workspace, args.ship));
    }

    // After the prebuilds, exec the binary directly. We use
    // `CommandBuilder::exec` so cargo isn't re-invoked at runtime
    // (matches how `prism dev` execs prebuilt binaries).
    let binary = workspace.bin_path("loom-relayd", args.ship);
    let mut serve = CommandBuilder::exec(&binary)
        .arg("--bind")
        .arg(&args.bind)
        .arg("--editor-dist")
        .arg(editor_dist.to_string_lossy().into_owned())
        .arg("--cors")
        .arg(args.cors.as_flag())
        .label("loom-serve")
        .cwd(workspace.root());
    if let Some(did) = &args.relay_did {
        serve = serve.arg("--relay-did").arg(did);
    }
    plan.push(serve);

    // Up-front error when `--no-auto-build` is set and dist is
    // missing. We still print the plan for `--dry-run`.
    if !need_editor_build && !editor_dist_ready(&editor_dist) && !dry_run {
        return Err(anyhow!(
            "editor dist {} is missing or incomplete — run `prism loom build` first \
             (or drop `--no-auto-build` to auto-build it)",
            editor_dist.display()
        ));
    }

    super::execute_plan(&plan, dry_run)
}

fn run_dev(args: &LoomDevArgs, workspace: &Workspace, dry_run: bool) -> Result<u8> {
    if args.ui_only && args.relay_only {
        return Err(anyhow!("--ui-only and --relay-only are mutually exclusive"));
    }

    let editor_dir = loom_editor_dir(workspace);
    let mut preflight: Vec<CommandBuilder> = Vec::new();
    let mut supervised: Vec<CommandBuilder> = Vec::new();

    // Preflight: wasm bundle so the editor's LSP / lint flows light
    // up against the latest parser+runtime+lsp on a cold checkout.
    if !args.no_wasm && !args.relay_only {
        preflight.push(
            CommandBuilder::pnpm()
                .arg("wasm:build:dev")
                .cwd(editor_dir.clone())
                .label("loom-wasm-build"),
        );
    }

    // Preflight: build the relay binary so the supervisor execs an
    // already-warm binary (no cold cargo compile inside the noisy
    // multi-process log stream).
    if !args.ui_only {
        preflight.push(server_build_builder(workspace, args.ship));
    }

    // Supervised: Vite editor.
    let editor_url = format!("http://{}:{}", args.host, args.ui_port);
    let relay_url = format!("http://{}:{}", args.host, args.relay_port);
    if !args.relay_only {
        // `pnpm dev` defaults to vite; we forward `--host` + `--port`
        // so the editor + relay can co-bind cleanly.
        let mut vite = CommandBuilder::pnpm()
            .arg("dev")
            .arg("--host")
            .arg(args.host.clone())
            .arg("--port")
            .arg(args.ui_port.to_string())
            .cwd(editor_dir.clone())
            .label("loom-editor");
        // VITE_LOOM_RELAY lets the editor short-circuit its
        // origin-based default for non-standard relay ports.
        vite = vite.env("VITE_LOOM_RELAY", relay_url.clone());
        supervised.push(vite);
    }

    // Supervised: relay binary.
    if !args.ui_only {
        let binary = workspace.bin_path("loom-relayd", args.ship);
        let bind = format!("{}:{}", args.host, args.relay_port);
        let serve = CommandBuilder::exec(&binary)
            .arg("--bind")
            .arg(bind)
            .arg("--cors")
            .arg("permissive")
            .label("loom-relay")
            .cwd(workspace.root());
        supervised.push(serve);
    }

    // Headline so the URLs land before the supervisor's interleaved
    // logs make them hard to spot.
    if !dry_run {
        eprintln!();
        eprintln!("  Loom IDE dev session");
        if !args.relay_only {
            eprintln!("    Editor (HMR): {editor_url}");
        }
        if !args.ui_only {
            eprintln!("    Relay (API+WS): {relay_url}");
        }
        eprintln!("    Press Ctrl+C to stop both.");
        eprintln!();
    }

    // Preflights run sequentially through the shared `execute_plan`
    // so a wasm-build failure aborts before the supervisor starts.
    let code = super::execute_plan(&preflight, dry_run)?;
    if code != 0 {
        return Ok(code);
    }

    if dry_run {
        for cmd in &supervised {
            println!("$ {}", cmd.display());
        }
        return Ok(0);
    }

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async move {
        let mut s = crate::supervisor::Supervisor::new();
        for cmd in supervised {
            s.add(cmd)?;
        }
        let outcome = s.run().await?;
        Ok(outcome.exit_code)
    })
}

fn loom_editor_dir(workspace: &Workspace) -> PathBuf {
    workspace.package("loom").join("editor")
}

/// Default location of the editor's Vite build output.
pub fn loom_editor_dist(workspace: &Workspace) -> PathBuf {
    loom_editor_dir(workspace).join("dist")
}

fn editor_dist_ready(dir: &Path) -> bool {
    dir.is_dir() && dir.join("index.html").is_file()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ws() -> Workspace {
        Workspace::new("/tmp/fake")
    }

    #[test]
    fn build_plan_default_runs_both_steps() {
        let plan = build_plan(
            &LoomBuildArgs {
                ship: false,
                skip_editor: false,
                skip_server: false,
            },
            &ws(),
        );
        assert_eq!(plan.len(), 2);
        assert_eq!(plan[0].label_str(), Some("loom-editor-build"));
        assert_eq!(plan[0].argv().0, "pnpm");
        assert_eq!(plan[0].argv().1, vec!["build"]);
        assert_eq!(plan[1].label_str(), Some("loom-server-build"));
        assert_eq!(plan[1].argv().1, vec!["build", "--package", "loom-server"]);
    }

    #[test]
    fn build_plan_ship_adds_release() {
        let plan = build_plan(
            &LoomBuildArgs {
                ship: true,
                skip_editor: false,
                skip_server: false,
            },
            &ws(),
        );
        assert_eq!(
            plan[1].argv().1,
            vec!["build", "--package", "loom-server", "--release"]
        );
    }

    #[test]
    fn build_plan_skip_editor_only_server() {
        let plan = build_plan(
            &LoomBuildArgs {
                ship: false,
                skip_editor: true,
                skip_server: false,
            },
            &ws(),
        );
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].label_str(), Some("loom-server-build"));
    }

    #[test]
    fn build_plan_skip_server_only_editor() {
        let plan = build_plan(
            &LoomBuildArgs {
                ship: false,
                skip_editor: false,
                skip_server: true,
            },
            &ws(),
        );
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].label_str(), Some("loom-editor-build"));
    }

    #[test]
    fn editor_dist_default_path() {
        let ws = ws();
        let dist = loom_editor_dist(&ws);
        assert!(dist.ends_with("packages/loom/editor/dist"));
    }

    #[test]
    fn cors_arg_flag_value() {
        assert_eq!(LoomCorsArg::SameOrigin.as_flag(), "same-origin");
        assert_eq!(LoomCorsArg::Permissive.as_flag(), "permissive");
    }
}
