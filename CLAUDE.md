# Prism Framework

Distributed Visual Operating System. All-Rust Cargo monorepo, mid-way through the **Slint migration** — see `docs/dev/slint-migration-plan.md`. Workspace license is GPL-3.0-or-later (Slint's royalty-free terms), wired through `license.workspace = true` on every crate.

## Layout
- `packages/prism-daemon` — Rust library + `prism-daemond` stdio bin. The local physics engine. Unchanged by the migration.
- `packages/prism-core` — Rust. Shared foundations: design tokens, shell mode, boot config, kernel store + `Shell` wrapper (port in progress).
- `packages/prism-builder` — Rust. The Slint-native Puck replacement (registry, document tree, Puck-JSON reader, HTML SSR render path). Phase-3 target of the Slint migration — the builder's Slint render walker lands on top of `slint-interpreter`.
- `packages/prism-shell` — Rust (`cdylib` + `rlib`). Single source of truth for the UI tree. Renders via Slint: `ui/app.slint` is compiled at build time by `slint-build`, re-exported through `slint::include_modules!()`, and instantiated by `Shell::new()`. Native bin under `src/bin/native.rs`; browser build goes through `wasm32-unknown-unknown` + `wasm-bindgen` (the `web` feature), producing `prism_shell.js` + `prism_shell_bg.wasm` next to a hand-written `web/index.html`.
- `packages/prism-studio/src-tauri` — Rust. Packaged desktop shell that spawns `prism-daemon` as a sibling process over `interprocess` + `postcard` and then hands control to `prism_shell::Shell`. Slint owns windowing and the renderer end-to-end — no Tauri / wry / webview / tao / wgpu anywhere. The `src-tauri/` directory name is a pre-Slint historical artefact; renaming it is a cleanup followup.
- `packages/prism-cli` — Rust. The unified `prism` binary — one front door for `test`, `build`, `dev`, `lint`, `fmt` across every Rust crate. `prism dev all` spawns every dev server behind a tokio process supervisor with colored prefixed logs and Ctrl+C fan-out. See its `CLAUDE.md` for the full subcommand surface.
- `packages/prism-relay` — Rust. Axum-based Sovereign Portal SSR server (`prism-relayd`) that renders `prism_builder::BuilderDocument` trees to semantic HTML via `Component::render_html`. Replaced the Hono TS relay 2026-04-15.

## Commands
The preferred front door is the unified `prism` CLI — every subcommand has a
`--dry-run` flag that prints the expanded argv without executing, so anything in
this list can be audited with e.g. `prism --dry-run test`.

- `cargo run -p prism-cli -- test [-p <pkg>]` — `cargo test --workspace` (or scoped to a single crate). Legacy Playwright e2e flags were retired alongside the Hono TS relay on 2026-04-15.
- `cargo run -p prism-cli -- build [--target desktop|studio|web|relay|all] [--debug]` — build every deployable. `web` runs `cargo build --target wasm32-unknown-unknown -p prism-shell --no-default-features --features web` followed by `wasm-bindgen --target web --out-dir packages/prism-shell/web` over the emitted cdylib. No separate copy step — wasm-bindgen writes `prism_shell.js` + `prism_shell_bg.wasm` directly next to `web/index.html`.
- `cargo run -p prism-cli -- dev [shell|studio|web|relay|all]` — run one or many dev servers. `dev web` runs the cargo + wasm-bindgen preflight, then serves `packages/prism-shell/web/` via `python3 -m http.server 1420`. `all` goes through the process supervisor.
- `cargo run -p prism-cli -- lint` — `cargo clippy --workspace --all-targets -- -D warnings`.
- `cargo run -p prism-cli -- fmt [--check]` — `cargo fmt --all`.

The root `package.json` exposes the same surface via pnpm scripts
(`pnpm test`, `pnpm dev`, `pnpm build`, `pnpm lint`, `pnpm format`) for users
who prefer that entry point. Everything still decomposes to raw `cargo`
under the hood — `prism` is purely a dispatcher, not a new layer of
abstraction. The web target additionally expects `wasm-bindgen` on PATH:
`cargo install wasm-bindgen-cli` with a version that matches the
`wasm-bindgen` crate pinned in the workspace manifest.

## Style
- Rust 2021 edition, strict clippy.
- Conventional Commits: `type(scope): description`.
- Kebab-case files in docs/scripts; Rust modules follow `snake_case` as usual.
- Never deprecate. Rename, move, break, fix. `cargo check --workspace` is the safety net.

## Architecture
- Loro CRDT = source of truth (via the `loro` Rust crate).
- Slint UI = sole UI tree. No React, no Tailwind, no Puck, no Clay, no hand-vendored wgpu renderer. Slint's declarative `.slint` DSL is compiled at build time for hand-written components and at runtime (via `slint-interpreter`) for the drag-droppable blocks that `prism-builder` materialises from a `BuilderDocument`.
- Rust → WASM for web (`wasm32-unknown-unknown` + `wasm-bindgen`); Rust → native binary for desktop/mobile.
- Desktop shell is pure `prism-shell` on top of Slint's bundled `winit` backend + `femtovg` renderer — no external windowing or GPU layer to wrangle. Packaging/signing/updater land in Phase 5 via `cargo-packager` + `self_update` + the standalone shell crates (`tray-icon`, `notify-rust`, `rfd`, `arboard`, `keyring`).
- Daemon runs as a sibling process launched by the Studio shell on desktop (IPC via `interprocess` + `postcard`), as an in-process tokio subsystem on mobile, and is remote (WebSocket relay) on web.
- Ephemeral state (cursors, drags) lives behind the same `AppState` struct but is serialized out on hot-reload snapshots.

## Workflow
After every implementation:
1. Write/update Rust tests in `src/**/*.rs` (`#[cfg(test)]`).
2. Run `cargo test --workspace` (and `cargo clippy` if you touched anything non-trivial).
3. Update the affected package's `CLAUDE.md` if the public API changed.
4. Update `docs/dev/slint-migration-plan.md` if phasing or decisions moved.
5. **For UI/Slint changes**: use the visual testing harness to verify.
   - `cargo run -p prism-shell -- --app lattice --panel builder` — quick visual check.
   - `cargo run -p prism-shell -- --scene builder-tablet` — check at different viewports.
   - `prism visual --scene <name>` — capture screenshot for review.
   - Add new scenes to `BuiltinScene` in `prism-shell/src/testing.rs` when adding major visual features.
6. **For input/interaction changes**: run the e2e test suite.
   - `cargo run -p prism-cli -- e2e` — run all e2e tests (callback-level, no display needed).
   - `cargo run -p prism-cli -- e2e --test <name>` — run a single test.
   - `cargo run -p prism-cli -- e2e --list` — list all available tests.
   - `cargo run -p prism-shell -- --e2e` — run directly via the shell binary.
   - Add new test scripts in `prism-shell/src/e2e.rs` via the `TestScript` builder API.

## Navigation
- Migration plan: `docs/dev/slint-migration-plan.md`
- Decisions: `docs/adr/`
- Package context: each package's `CLAUDE.md`
