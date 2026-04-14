# Prism Framework

Distributed Visual Operating System. Hybrid Cargo (Rust) + pnpm (Hono JSX SSR relay) monorepo, mid-way through the **Clay migration** — see `docs/dev/clay-migration-plan.md`.

## Layout
- `packages/prism-daemon` — Rust library + `prism-daemond` stdio bin. The local physics engine. Unchanged by the migration.
- `packages/prism-core` — Rust. Shared foundations: design tokens, shell mode, boot config, (port in progress).
- `packages/prism-builder` — Rust. The Clay-native Puck replacement (registry, document tree, Puck-JSON reader).
- `packages/prism-shell` — Rust (`rlib` + `cdylib`). Single source of truth for the UI tree. Native bin under `src/bin/native.rs`, wasm entry under `src/web.rs`.
- `packages/prism-studio/src-tauri` — Rust. Tauri 2 desktop shell in **no-webview** configuration (§4.5 Option B). Embeds `prism-shell` as a library. Spawns `prism-daemon` as a sidecar.
- `packages/prism-cli` — Rust. The unified `prism` binary — one front door for `test`, `build`, `dev`, `lint`, `fmt` across every Rust crate + the relay. `prism dev all` spawns every dev server behind a tokio process supervisor with colored prefixed logs and Ctrl+C fan-out. See its `CLAUDE.md` for the full subcommand surface.
- `packages/prism-relay` — Hono JSX SSR. Out of scope for the Clay migration; slated for its own rewrite.

## Commands
The preferred front door is the unified `prism` CLI — every subcommand has a
`--dry-run` flag that prints the expanded argv without executing, so anything in
this list can be audited with e.g. `prism --dry-run test --all`.

- `cargo run -p prism-cli -- test [--all|--rust|--e2e] [-p <pkg>]` — run Rust unit tests and the relay Playwright e2e suite in one go. (Relay vitest units are unwired until the relay rewrite.)
- `cargo run -p prism-cli -- build [--target desktop|studio|web|relay|all] [--debug]` — build every deployable.
- `cargo run -p prism-cli -- dev [shell|studio|web|relay|all]` — run one or many dev servers. `all` goes through the process supervisor.
- `cargo run -p prism-cli -- lint` — `cargo clippy --workspace --all-targets -- -D warnings`.
- `cargo run -p prism-cli -- fmt [--check]` — `cargo fmt --all`.

The root `package.json` exposes the same surface via pnpm scripts
(`pnpm test`, `pnpm dev`, `pnpm build`, `pnpm lint`, `pnpm format`) for users
who prefer that entry point. Everything still decomposes to raw `cargo` /
`pnpm` / `trunk` / `cargo tauri` under the hood — `prism` is purely a
dispatcher, not a new layer of abstraction.

## Style
- Rust 2021 edition, strict clippy.
- Conventional Commits: `type(scope): description`.
- Kebab-case files in docs/scripts; Rust modules follow `snake_case` as usual.
- Never deprecate. Rename, move, break, fix. `cargo check --workspace` is the safety net.

## Architecture
- Loro CRDT = source of truth (via the `loro` Rust crate).
- Clay layout DSL = sole UI tree. No React, no Tailwind, no Puck.
- Rust → WASM for web; Rust → native binary for desktop/mobile.
- Tauri 2 provides packaging / updater / signing / sidecars, but **wry is not loaded**. Windowing via `tao`, rendering via `wgpu`.
- Daemon runs as a Tauri sidecar on desktop, as an in-process tokio subsystem on mobile, and is remote (WebSocket relay) on web.
- Ephemeral state (cursors, drags) lives behind the same `AppState` struct but is serialized out on hot-reload snapshots.

## Workflow
After every implementation:
1. Write/update Rust tests in `src/**/*.rs` (`#[cfg(test)]`).
2. Run `cargo test --workspace` (and `cargo clippy` if you touched anything non-trivial).
3. Update the affected package's `CLAUDE.md` if the public API changed.
4. Update `docs/dev/clay-migration-plan.md` if phasing or decisions moved.

## Navigation
- Migration plan: `docs/dev/clay-migration-plan.md`
- Decisions: `docs/adr/`
- Package context: each package's `CLAUDE.md`
