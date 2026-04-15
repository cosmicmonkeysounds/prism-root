# prism-cli

Unified Rust CLI for the Prism Framework workspace. Produces a
single `prism` binary that replaces the ad-hoc mix of `cargo`,
`pnpm`, and `trunk` commands the workspace used to require.

## Build & Test
- `cargo build -p prism-cli` — build the binary (`target/debug/prism`).
- `cargo test -p prism-cli` — 49 lib unit tests + 14 e2e integration tests.
- `PRISM_CLI_E2E_HEAVY=1 cargo test -p prism-cli --test e2e` —
  also runs the heavy path that actually shells out to
  `cargo test -p prism-core` through the `prism` binary.

## Subcommands
Every subcommand supports `--dry-run` (global) to print the
expanded argv without executing anything.

### `prism test [--package <name>] [--rust|--e2e|--all] [-- <extra>]`
- No flags → `cargo test --workspace`.
- `--all` → Rust + relay Playwright e2e.
- `--package prism-core` → `cargo test --package prism-core`.
- Extra args after `--` are forwarded to the underlying runner.
- Relay TS unit tests (`vitest`) were removed from the CLI surface
  pending the relay rewrite; see `packages/prism-relay/CLAUDE.md`.

### `prism build [--target desktop|studio|web|relay|all] [--debug]`
- Defaults to `--target all` + release builds.
- `desktop` → `cargo build -p prism-shell`.
- `studio` → `cargo build -p prism-studio`. Phase 5 will wrap this
  in `cargo-packager` for installer bundles; today it's a plain
  cargo build.
- `web` → `trunk build --config packages/prism-shell/Trunk.toml`.
- `relay` → `pnpm --filter @prism/relay run build`.

### `prism dev [shell|studio|web|relay|all]`
- Defaults to `shell`.
- Single targets exec directly so Ctrl+C lands on the child.
- `all` spawns every target behind the process supervisor:
  - Each child gets a colored label prefix on every output line.
  - First non-zero exit tears down all siblings.
  - Ctrl+C kills every child (kill_on_drop + tokio::signal::ctrl_c).

### `prism lint`
`cargo clippy --workspace --all-targets -- -D warnings`.

### `prism fmt [--check]`
`cargo fmt --all`, optionally `--check`.

## Library surface
The crate is split into a library + a thin binary so tests and
sibling crates can reach into it without going through `std::process`.

- `builder::CommandBuilder` — fluent argv builder with
  `cargo()` / `pnpm()` / `trunk()` constructors,
  `package()` / `workspace()` / `release()` / `arg()` / `args()`
  / `cwd()` / `env()` / `label()` combinators, and
  `argv()` / `display()` / `build()` / `build_tokio()` outputs.
- `workspace::Workspace` — filesystem discovery; walks up from
  the current directory until it finds a `Cargo.toml` that lists
  `packages/prism-cli` as a workspace member.
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
