//! `prism build` — the unified builder.
//!
//! Targets map 1:1 to the deployables the workspace cares about:
//!
//! - `desktop` — `cargo build -p prism-shell` (the native dev bin —
//!   prism-ui-runtime + femtovg).
//! - `studio`  — `cargo build -p prism-studio` plus
//!   `cargo build -p prism-daemon --bin prism-daemond` (the
//!   packaged desktop shell + the daemon sidecar it spawns at
//!   startup; both have to live in the same `target/<profile>/`
//!   directory or `prism-studio` aborts with "daemon sidecar
//!   unavailable"). Bundling/signing lives in Phase 5 via
//!   `cargo-packager`.
//! - `web`     — two-to-three steps:
//!     1. `cargo build --target wasm32-unknown-unknown
//!        -p prism-shell --no-default-features --features web`,
//!     2. `wasm-bindgen --target web --out-dir packages/prism-shell/web
//!        target/wasm32-unknown-unknown/<profile>/prism_shell.wasm`,
//!     3. (only when `emcc` is on PATH) `cargo build --target
//!        wasm32-unknown-emscripten -p prism-daemon --bin
//!        prism_daemon_wasm --no-default-features --features wasm`
//!        followed by a small filesystem copy that lands
//!        `prism_daemon_wasm.{js,wasm}` next to `index.html`. That
//!        sidecar carries the real `mlua` Luau runtime — see
//!        `packages/prism-daemon/src/wasm.rs` for the C ABI and
//!        `packages/prism-shell/src/services/luau.rs` for the
//!        `JsLuauHost` that calls into it.
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

    /// Build the slow, runtime-optimised ship profile (release +
    /// thin LTO + `codegen-units = 1`). By default `prism build`
    /// produces fast-iteration debug artifacts; only pass `--ship`
    /// when you actually need a release binary — it is ~10x slower
    /// to compile on Prism's 772-crate graph.
    #[arg(long)]
    pub ship: bool,
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
                    args.ship,
                ));
            }
            BuildTarget::Studio => {
                plan.push(daemon_bin_builder(workspace, args.ship));
                plan.push(build_cargo_target(
                    "prism-studio",
                    "studio-build",
                    workspace,
                    args.ship,
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
                if args.ship {
                    cargo_cmd = cargo_cmd.release();
                }
                plan.push(cargo_cmd.cwd(workspace.root()));
                plan.push(web_bindgen_builder(workspace, args.ship));
                // Daemon-wasm step is included unconditionally so
                // `--dry-run` / unit tests see the full plan; the
                // `run()` filter below removes it when `emcc` is not
                // on PATH.
                plan.push(daemon_wasm_builder(workspace, args.ship));
            }
            BuildTarget::Relay => {
                plan.push(build_cargo_target(
                    "prism-relay",
                    "relay-build",
                    workspace,
                    args.ship,
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

/// `cargo build -p prism-daemon --bin prism-daemond --features
/// transport-ipc`. Studio's sidecar resolution looks for
/// `prism-daemond` next to the `prism-studio` binary in
/// `target/<profile>/`, so the two always have to be built into
/// the same profile. The `transport-ipc` feature is what enables
/// the `--ipc-socket <name>` flag the studio host passes when it
/// spawns the daemon — without it, the daemon aborts immediately
/// with `--ipc-socket requires the transport-ipc feature at build
/// time`. It is *not* in the daemon's `default`/`full` preset
/// (mobile/wasm/embedded builds intentionally drop it), so the
/// CLI has to opt in here at the studio entry point.
pub(crate) fn daemon_bin_builder(workspace: &Workspace, release: bool) -> CommandBuilder {
    let mut cmd = CommandBuilder::cargo()
        .arg("build")
        .package("prism-daemon")
        .arg("--bin")
        .arg("prism-daemond")
        .arg("--features")
        .arg("transport-ipc")
        .label("daemon-build");
    if release {
        cmd = cmd.release();
    }
    cmd.cwd(workspace.root())
}

/// `cargo build --target wasm32-unknown-emscripten -p prism-daemon
/// --bin prism_daemon_wasm --no-default-features --features wasm`.
///
/// Emscripten emits the `.js` loader + `.wasm` blob into
/// `target/wasm32-unknown-emscripten/<profile>/`; the post-build
/// [`copy_daemon_wasm_artifacts`] helper drops them next to the
/// shell's `web/index.html` so the browser's
/// `import("./prism_daemon_wasm.js")` resolves. Exposed `pub(crate)`
/// so `dev.rs` can reuse the same builder.
pub(crate) fn daemon_wasm_builder(workspace: &Workspace, release: bool) -> CommandBuilder {
    let mut cmd = CommandBuilder::cargo()
        .arg("build")
        .arg("--target")
        .arg("wasm32-unknown-emscripten")
        .package("prism-daemon")
        .arg("--bin")
        .arg("prism_daemon_wasm")
        .arg("--no-default-features")
        .arg("--features")
        .arg("wasm")
        .label("daemon-wasm-build");
    if release {
        cmd = cmd.release();
    }
    cmd.cwd(workspace.root())
}

/// Detect whether `emcc` is on `PATH`. The daemon-wasm cargo step
/// depends on it because mlua's vendored Luau C++ source needs
/// emscripten's libc++ to link; without `emcc` cargo aborts inside
/// the build script. We pre-check so the user gets a clear "skipping"
/// message instead of a deep emcc-not-found stack trace, and so a
/// machine without the SDK can still finish a normal web build (the
/// shell will run with `NoopLuauHost`).
pub(crate) fn has_emscripten() -> bool {
    std::process::Command::new("emcc")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Filesystem-only post-step that lands the emscripten daemon's
/// `.js` + `.wasm` artifacts next to `index.html` so the browser can
/// `import("./prism_daemon_wasm.js")` them. Pure `std::fs::copy` —
/// we deliberately avoid spawning `cp` so the behaviour is identical
/// on every platform the workspace runs on.
pub(crate) fn copy_daemon_wasm_artifacts(workspace: &Workspace, release: bool) -> Result<()> {
    use anyhow::Context;
    let (js_src, wasm_src) = workspace.daemon_wasm_artifacts(release);
    let out_dir = workspace.shell_web_dir();
    let js_dst = out_dir.join("prism_daemon_wasm.js");
    let wasm_dst = out_dir.join("prism_daemon_wasm.wasm");
    std::fs::copy(&js_src, &js_dst)
        .with_context(|| format!("copying {} → {}", js_src.display(), js_dst.display()))?;
    std::fs::copy(&wasm_src, &wasm_dst)
        .with_context(|| format!("copying {} → {}", wasm_src.display(), wasm_dst.display()))?;
    Ok(())
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
    let mut plan = plan(args, workspace);
    let build_daemon_wasm =
        matches!(args.target, BuildTarget::Web | BuildTarget::All) && has_emscripten();
    if !build_daemon_wasm {
        if matches!(args.target, BuildTarget::Web | BuildTarget::All) {
            eprintln!(
                "[prism build] `emcc` not on PATH — skipping daemon-wasm step; \
                 shell will boot with NoopLuauHost. Install the emscripten SDK \
                 and `source emsdk_env.sh` to enable browser Luau."
            );
        }
        plan.retain(|c| c.label_str() != Some("daemon-wasm-build"));
    }

    let code = super::execute_plan(&plan, dry_run)?;
    if code == 0 {
        if build_daemon_wasm && !dry_run {
            if let Err(e) = copy_daemon_wasm_artifacts(workspace, args.ship) {
                eprintln!("[prism build] copying daemon-wasm artifacts failed: {e}");
                return Ok(1);
            }
        }
        crate::gc::sweep(&workspace.target_dir());
    }
    Ok(code)
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
            ship: false,
        }
    }

    fn ship_args(target: BuildTarget) -> BuildArgs {
        BuildArgs { target, ship: true }
    }

    #[test]
    fn all_target_fans_out_to_seven() {
        // desktop + daemon-build + studio + web-build + web-bindgen
        // + daemon-wasm-build + relay. `daemon-wasm-build` is always
        // in the plan; `run()` filters it back out when `emcc` is
        // missing from PATH.
        let p = plan(&args(BuildTarget::All), &ws());
        assert_eq!(p.len(), 7);
        let labels: Vec<_> = p.iter().map(|c| c.label_str().unwrap()).collect();
        assert_eq!(
            labels,
            vec![
                "desktop-build",
                "daemon-build",
                "studio-build",
                "web-build",
                "web-bindgen",
                "daemon-wasm-build",
                "relay-build"
            ]
        );
    }

    #[test]
    fn desktop_defaults_to_fast_debug() {
        let p = plan(&args(BuildTarget::Desktop), &ws());
        assert_eq!(p[0].argv().1, vec!["build", "--package", "prism-shell"]);
    }

    #[test]
    fn desktop_ship_adds_release_flag() {
        let p = plan(&ship_args(BuildTarget::Desktop), &ws());
        assert_eq!(
            p[0].argv().1,
            vec!["build", "--package", "prism-shell", "--release"]
        );
    }

    #[test]
    fn studio_prebuilds_daemon_sidecar_debug_by_default() {
        let p = plan(&args(BuildTarget::Studio), &ws());
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
        assert_eq!(p[1].label_str(), Some("studio-build"));
        assert_eq!(p[1].argv().1, vec!["build", "--package", "prism-studio"]);
    }

    #[test]
    fn studio_ship_adds_release_flag_on_both_steps() {
        let p = plan(&ship_args(BuildTarget::Studio), &ws());
        assert_eq!(
            p[0].argv().1,
            vec![
                "build",
                "--package",
                "prism-daemon",
                "--bin",
                "prism-daemond",
                "--features",
                "transport-ipc",
                "--release"
            ]
        );
        assert_eq!(
            p[1].argv().1,
            vec!["build", "--package", "prism-studio", "--release"]
        );
    }

    #[test]
    fn web_cargo_step_uses_wasm32_unknown_unknown_with_web_feature() {
        let p = plan(&args(BuildTarget::Web), &ws());
        // web-build + web-bindgen + daemon-wasm-build (always in
        // plan; `run()` filters daemon-wasm when emcc is missing).
        assert_eq!(p.len(), 3);
        let argv = p[0].argv().1;
        assert_eq!(argv[0], "build");
        assert_eq!(argv[1], "--target");
        assert_eq!(argv[2], "wasm32-unknown-unknown");
        assert_eq!(argv[3], "--package");
        assert_eq!(argv[4], "prism-shell");
        assert!(argv.contains(&"--no-default-features".to_string()));
        assert!(argv.contains(&"--features".to_string()));
        assert!(argv.contains(&"web".to_string()));
        // Fast debug by default — no --release.
        assert!(!argv.contains(&"--release".to_string()));
        assert_eq!(p[0].label_str(), Some("web-build"));
    }

    #[test]
    fn web_defaults_to_debug_wasm_artifact() {
        let p = plan(&args(BuildTarget::Web), &ws());
        assert_eq!(p[1].program(), crate::builder::Program::WasmBindgen);
        let argv = p[1].argv().1;
        assert!(argv[3].ends_with("packages/prism-shell/web"));
        assert!(argv[4].ends_with("wasm32-unknown-unknown/debug/prism_shell.wasm"));
    }

    #[test]
    fn web_ship_adds_release_flag_and_reads_release_wasm() {
        let p = plan(&ship_args(BuildTarget::Web), &ws());
        assert!(p[0].argv().1.contains(&"--release".to_string()));
        let bindgen_argv = p[1].argv().1;
        assert!(bindgen_argv[4].ends_with("wasm32-unknown-unknown/release/prism_shell.wasm"));
    }

    #[test]
    fn web_plan_includes_daemon_wasm_cargo_step() {
        let p = plan(&args(BuildTarget::Web), &ws());
        let daemon = p
            .iter()
            .find(|c| c.label_str() == Some("daemon-wasm-build"))
            .expect("web plan must include daemon-wasm-build");
        let argv = daemon.argv().1;
        assert_eq!(argv[0], "build");
        assert!(argv.contains(&"wasm32-unknown-emscripten".to_string()));
        assert!(argv.contains(&"prism-daemon".to_string()));
        assert!(argv.contains(&"prism_daemon_wasm".to_string()));
        assert!(argv.contains(&"--no-default-features".to_string()));
        assert!(argv.contains(&"wasm".to_string()));
        // Fast debug by default — matches the web-build flag.
        assert!(!argv.contains(&"--release".to_string()));
    }

    #[test]
    fn web_ship_passes_release_to_daemon_wasm_step() {
        let p = plan(&ship_args(BuildTarget::Web), &ws());
        let daemon = p
            .iter()
            .find(|c| c.label_str() == Some("daemon-wasm-build"))
            .expect("web ship plan must include daemon-wasm-build");
        assert!(daemon.argv().1.contains(&"--release".to_string()));
    }

    #[test]
    fn relay_defaults_to_fast_debug() {
        let p = plan(&args(BuildTarget::Relay), &ws());
        assert_eq!(p[0].argv().1, vec!["build", "--package", "prism-relay"]);
        assert_eq!(p[0].label_str(), Some("relay-build"));
    }

    #[test]
    fn relay_ship_adds_release_flag() {
        let p = plan(&ship_args(BuildTarget::Relay), &ws());
        assert_eq!(
            p[0].argv().1,
            vec!["build", "--package", "prism-relay", "--release"]
        );
    }
}
