//! End-to-end tests for the `prism` binary.
//!
//! These spawn the real compiled `prism` executable (via
//! `CARGO_BIN_EXE_prism`) against the real Prism workspace, so a
//! green run here means the CLI can actually dispatch every
//! subcommand against the concrete project tree — not just in
//! unit-test isolation.
//!
//! The fast checks (`--version`, `--help`, every subcommand under
//! `--dry-run`) run unconditionally. The expensive check
//! (`prism test --package prism-core` actually invoking
//! `cargo test`) is gated behind `PRISM_CLI_E2E_HEAVY=1` so
//! day-to-day `cargo test -p prism-cli` stays fast. CI sets the
//! flag so the heavy path is still exercised on every push.

use std::path::PathBuf;
use std::process::Command;

fn prism_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_prism"))
}

fn workspace_root() -> PathBuf {
    // The test binary lives at <workspace>/target/debug/deps/... —
    // walk up until we find the root Cargo.toml that lists
    // prism-cli as a member.
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    while !p.join("packages/prism-cli").is_dir() {
        assert!(p.pop(), "couldn't find workspace root from manifest dir");
    }
    p
}

fn run(args: &[&str]) -> std::process::Output {
    Command::new(prism_bin())
        .args(args)
        .current_dir(workspace_root())
        .output()
        .expect("spawn prism binary")
}

fn stdout(out: &std::process::Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn version_flag_prints_a_version() {
    let out = run(&["--version"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(stdout(&out).contains("prism"));
}

#[test]
fn help_lists_every_subcommand() {
    let out = run(&["--help"]);
    assert!(out.status.success());
    let help = stdout(&out);
    for expected in ["test", "build", "dev", "lint", "fmt", "clean"] {
        assert!(
            help.contains(expected),
            "--help output missing `{expected}`: {help}"
        );
    }
}

#[test]
fn test_dry_run_prints_cargo_test_workspace() {
    let out = run(&["--dry-run", "test"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(stdout(&out).contains("cargo test --workspace"));
}

#[test]
fn test_dry_run_is_rust_only() {
    // The `--e2e` / `--all` flags were retired alongside the Hono
    // TS relay (2026-04-15). `prism test` is now a thin wrapper
    // around `cargo test`; integration tests for the Rust relay
    // live in `packages/prism-relay/tests/routes.rs` and run under
    // the default `cargo test --workspace` path.
    let out = run(&["--dry-run", "test"]);
    assert!(out.status.success());
    let s = stdout(&out);
    assert!(s.contains("cargo test --workspace"), "{s}");
    assert!(!s.contains("@prism/relay"), "{s}");
    assert!(!s.contains("vitest"), "{s}");
}

#[test]
fn test_package_filter_dry_run() {
    let out = run(&["--dry-run", "test", "-p", "prism-core"]);
    assert!(out.status.success());
    assert!(stdout(&out).contains("cargo test --package prism-core"));
}

#[test]
fn build_target_all_dry_run() {
    let out = run(&["--dry-run", "build"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let s = stdout(&out);
    assert!(
        s.contains("cargo build --package prism-shell --release"),
        "{s}"
    );
    assert!(
        s.contains("cargo build --package prism-studio --release"),
        "{s}"
    );
    assert!(
        s.contains("cargo build --target wasm32-unknown-unknown --package prism-shell"),
        "{s}"
    );
    assert!(s.contains("--no-default-features --features web"), "{s}");
    assert!(s.contains("wasm-bindgen --target web --out-dir"), "{s}");
    assert!(
        s.contains("cargo build --package prism-relay --release"),
        "{s}"
    );
    // trunk + emscripten + the Hono TS relay are all retired.
    assert!(!s.contains("trunk"), "{s}");
    assert!(!s.contains("wasm32-unknown-emscripten"), "{s}");
    assert!(!s.contains("pnpm --filter @prism/relay run build"), "{s}");
}

#[test]
fn build_relay_only_dry_run() {
    let out = run(&["--dry-run", "build", "--target", "relay"]);
    assert!(out.status.success());
    let s = stdout(&out);
    assert!(
        s.contains("cargo build --package prism-relay --release"),
        "{s}"
    );
    // When the user asks for relay only, desktop/web/studio should NOT appear.
    assert!(!s.contains("cargo build --package prism-shell"), "{s}");
    assert!(!s.contains("wasm32-unknown-unknown"), "{s}");
    assert!(!s.contains("wasm-bindgen"), "{s}");
    assert!(!s.contains("cargo build --package prism-studio"), "{s}");
    assert!(!s.contains("pnpm --filter @prism/relay run build"), "{s}");
}

#[test]
fn build_web_only_dry_run_uses_wasm_bindgen_pipeline() {
    let out = run(&["--dry-run", "build", "--target", "web"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let s = stdout(&out);
    assert!(
        s.contains("cargo build --target wasm32-unknown-unknown --package prism-shell"),
        "{s}"
    );
    assert!(s.contains("--features web --release"), "{s}");
    assert!(s.contains("wasm-bindgen --target web --out-dir"), "{s}");
    assert!(
        s.contains("wasm32-unknown-unknown/release/prism_shell.wasm"),
        "{s}"
    );
    assert!(!s.contains("trunk"), "{s}");
    assert!(!s.contains("wasm32-unknown-emscripten"), "{s}");
}

#[test]
fn build_debug_omits_release_flag() {
    let out = run(&["--dry-run", "build", "--target", "desktop", "--debug"]);
    assert!(out.status.success());
    let s = stdout(&out);
    assert!(s.contains("cargo build --package prism-shell"), "{s}");
    assert!(!s.contains("--release"), "{s}");
}

#[test]
fn dev_shell_default_dry_run() {
    let out = run(&["--dry-run", "dev"]);
    assert!(out.status.success());
    assert!(stdout(&out).contains("cargo run --package prism-shell"));
}

#[test]
fn dev_all_dry_run_prints_every_labeled_command() {
    let out = run(&["--dry-run", "dev", "all"]);
    assert!(out.status.success());
    let s = stdout(&out);
    for label in [
        "[shell]",
        "[studio]",
        "[web-build]",
        "[web-bindgen]",
        "[web]",
        "[relay]",
    ] {
        assert!(s.contains(label), "missing {label} in:\n{s}");
    }
    assert!(s.contains("python3 -m http.server"), "{s}");
    assert!(!s.contains("trunk"), "{s}");
    assert!(!s.contains("wasm32-unknown-emscripten"), "{s}");
}

#[test]
fn dev_web_dry_run_prints_build_bindgen_serve() {
    let out = run(&["--dry-run", "dev", "web"]);
    assert!(out.status.success());
    let s = stdout(&out);
    assert!(
        s.contains("cargo build --target wasm32-unknown-unknown --package prism-shell"),
        "{s}"
    );
    assert!(s.contains("wasm-bindgen --target web --out-dir"), "{s}");
    assert!(
        s.contains("wasm32-unknown-unknown/debug/prism_shell.wasm"),
        "{s}"
    );
    assert!(s.contains("python3 -m http.server"), "{s}");
}

#[test]
fn lint_dry_run_prints_clippy() {
    let out = run(&["--dry-run", "lint"]);
    assert!(out.status.success());
    assert!(stdout(&out).contains("cargo clippy --workspace --all-targets -- -D warnings"));
}

#[test]
fn fmt_check_dry_run() {
    let out = run(&["--dry-run", "fmt", "--check"]);
    assert!(out.status.success());
    assert!(stdout(&out).contains("cargo fmt --all -- --check"));
}

#[test]
fn clean_dry_run_prints_cargo_clean() {
    let out = run(&["--dry-run", "clean"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(stdout(&out).contains("cargo clean"));
}

#[test]
fn unknown_subcommand_exits_nonzero() {
    let out = run(&["bogus"]);
    assert!(!out.status.success());
}

/// The heavy test: actually run `cargo test -p prism-core` through
/// the `prism` binary and confirm it exits zero. This is what
/// "make sure it really runs e2e" was asking for. Gated behind
/// `PRISM_CLI_E2E_HEAVY=1` so iteration on other tests stays fast.
#[test]
fn heavy_actually_runs_prism_core_tests() {
    if std::env::var("PRISM_CLI_E2E_HEAVY").is_err() {
        eprintln!("skipping heavy e2e test — set PRISM_CLI_E2E_HEAVY=1 to enable");
        return;
    }
    let out = run(&["test", "-p", "prism-core"]);
    assert!(
        out.status.success(),
        "prism test -p prism-core failed:\nstdout: {}\nstderr: {}",
        stdout(&out),
        String::from_utf8_lossy(&out.stderr)
    );
}
