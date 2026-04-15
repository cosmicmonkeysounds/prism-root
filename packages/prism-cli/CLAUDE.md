# prism-cli

Unified Rust CLI for the Prism Framework workspace. Produces a
single `prism` binary that replaces the ad-hoc mix of `cargo`,
`pnpm`, and `trunk` commands the workspace used to require. Trunk
is permanently retired; the web target now goes through a plain
`wasm32-unknown-unknown` cargo build followed by `wasm-bindgen`
(see `build`/`dev` below).

## Build & Test
- `cargo build -p prism-cli` — build the binary (`target/debug/prism`).
- `cargo test -p prism-cli` — unit tests + e2e integration tests.
- `PRISM_CLI_E2E_HEAVY=1 cargo test -p prism-cli --test e2e` —
  also runs the heavy path that actually shells out to
  `cargo test -p prism-core` through the `prism` binary.

## Subcommands
Every subcommand supports `--dry-run` (global) to print the
expanded argv without executing anything.

### `prism test [--package <name>] [-- <extra>]`
- No flags → `cargo test --workspace`.
- `--package prism-core` → `cargo test --package prism-core`.
- Extra args after `--` are forwarded to `cargo test`.
- The legacy `--rust|--e2e|--all` flags were retired 2026-04-15 when
  the Hono TS relay's Playwright suite was deleted. `prism test` is
  now a thin Rust-only wrapper; the Rust axum relay's integration
  tests in `packages/prism-relay/tests/routes.rs` run under the
  default `cargo test --workspace` path.

### `prism build [--target desktop|studio|web|relay|all] [--debug]`
- Defaults to `--target all` + release builds.
- `desktop` → `cargo build -p prism-shell`.
- `studio` → `cargo build -p prism-studio`. Phase 5 will wrap this
  in `cargo-packager` for installer bundles; today it's a plain
  cargo build.
- `web` → a two-step pipeline:
  1. `cargo build --target wasm32-unknown-unknown -p prism-shell
     --no-default-features --features web` emits the cdylib
     `target/wasm32-unknown-unknown/<profile>/prism_shell.wasm`.
  2. `wasm-bindgen --target web --out-dir packages/prism-shell/web
     target/wasm32-unknown-unknown/<profile>/prism_shell.wasm`
     post-processes the cdylib and writes the ESM loader pair
     (`prism_shell.js` + `prism_shell_bg.wasm`) directly next to
     `index.html`. No separate copy step.

  Requires `wasm-bindgen` on `PATH` — install with
  `cargo install wasm-bindgen-cli` using the version that matches
  the `wasm-bindgen` crate pinned in the workspace manifest.
- `relay` → `cargo build -p prism-relay`. The Rust axum SSR server
  replaced the Hono TS relay on 2026-04-15.

### `prism dev [shell|studio|web|relay|all]`
- Defaults to `shell`.
- Single targets exec directly so Ctrl+C lands on the child.
- `web` is special: runs the `wasm32-unknown-unknown` cargo build
  and the wasm-bindgen post-process as a synchronous preflight,
  then execs `python3 -m http.server 1420 --directory
  packages/prism-shell/web` as the long-running child. Re-invoke
  `prism dev web` for a rebuild — there is no file-watch loop yet
  (Phase 0 limitation; `subsecond` hot-reload lands later).
- `all` spawns every target behind the process supervisor:
  - Each child gets a colored label prefix on every output line.
  - First non-zero exit tears down all siblings.
  - Ctrl+C kills every child (kill_on_drop + tokio::signal::ctrl_c).
  - Web's preflight (cargo build + wasm-bindgen) runs once before
    the supervisor starts, so the web child is just the static
    server.

### `prism lint`
`cargo clippy --workspace --all-targets -- -D warnings`.

### `prism fmt [--check]`
`cargo fmt --all`, optionally `--check`.

## Library surface
The crate is split into a library + a thin binary so tests and
sibling crates can reach into it without going through `std::process`.

- `builder::CommandBuilder` — fluent argv builder with
  `cargo()` / `pnpm()` / `python3()` / `wasm_bindgen()`
  constructors, `package()` / `workspace()` / `release()` /
  `arg()` / `args()` / `cwd()` / `env()` / `label()` combinators,
  and `argv()` / `display()` / `build()` / `build_tokio()` outputs.
- `workspace::Workspace` — filesystem discovery; walks up from
  the current directory until it finds a `Cargo.toml` that lists
  `packages/prism-cli` as a workspace member. Also exposes
  `shell_web_dir()` (the `packages/prism-shell/web/` directory
  the static server serves), `wasm_artifact_dir(release)` (where
  cargo drops the `wasm32-unknown-unknown` cdylib per profile),
  and `shell_wasm_artifact(release)` (the concrete
  `prism_shell.wasm` path fed to `wasm-bindgen`).
- `supervisor::Supervisor` — multi-process runner for `prism dev all`.
  Accepts a user-supplied `LineSink` so tests can capture output
  instead of writing to stdout, and a user-supplied shutdown
  future so tests can simulate Ctrl+C deterministically.
- `commands::{test, build, dev, lint, fmt}` — each exposes a
  `plan(args, workspace) -> Vec<CommandBuilder>` pure function
  and a `run(...)` wrapper. Everything shell-worthy funnels
  through `commands::execute_plan` so `--dry-run` lives in one
  place.

## package.json integration
The root `package.json` scripts all delegate to `prism` via
`cargo run -q -p prism-cli --`, so `pnpm test`, `pnpm dev`, and
`pnpm build` stay available for users who prefer the pnpm surface.
