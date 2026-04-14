# Prism Framework

Distributed Visual Operating System. Hybrid Cargo (Rust) + pnpm (Next.js relay) monorepo, mid-way through the **Clay migration** — see `docs/dev/clay-migration-plan.md`.

## Layout
- `packages/prism-daemon` — Rust library + `prism-daemond` stdio bin. The local physics engine. Unchanged by the migration.
- `packages/prism-core` — Rust. Shared foundations: design tokens, shell mode, boot config, (port in progress).
- `packages/prism-builder` — Rust. The Clay-native Puck replacement (registry, document tree, Puck-JSON reader).
- `packages/prism-shell` — Rust (`rlib` + `cdylib`). Single source of truth for the UI tree. Native bin under `src/bin/native.rs`, wasm entry under `src/web.rs`.
- `packages/prism-studio/src-tauri` — Rust. Tauri 2 desktop shell in **no-webview** configuration (§4.5 Option B). Embeds `prism-shell` as a library. Spawns `prism-daemon` as a sidecar.
- `packages/prism-relay` — Next.js. Out of scope for the migration, stays TypeScript.

## Commands
- `cargo build --workspace` — build every Rust crate.
- `cargo test --workspace` — run every Rust unit/integration test.
- `cargo clippy --workspace --all-targets -- -D warnings` — lint.
- `cargo fmt --all` — format.
- `cargo run -p prism-shell` — native dev binary (skips Tauri packaging).
- `cargo tauri dev --config packages/prism-studio/src-tauri/tauri.conf.json` — packaged desktop dev loop.
- `trunk serve --config packages/prism-shell/Trunk.toml` — WASM web dev loop.
- `pnpm --filter @prism/relay dev` / `build` / `test:e2e` — the relay side.

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
4. Update `docs/dev/current-plan.md`.

## Navigation
- Migration plan: `docs/dev/clay-migration-plan.md`
- Full spec: `SPEC.md`
- Decisions: `docs/adr/`
- Current task: `docs/dev/current-plan.md`
- Package context: each package's `CLAUDE.md`
