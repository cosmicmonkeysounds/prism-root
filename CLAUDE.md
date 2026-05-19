# Prism Framework

Distributed Visual Operating System. All-Rust Cargo monorepo, post-Slint
on the `prism-ui-runtime` stack (Taffy layout + `prism-ui` DSL +
`Surface` retained-mode renderer; backends: femtovg native, semantic
HTML for SSR). The Slint runtime, source emitter, and binding derive
were all deleted in the Phase 5 cutover; see `docs/dev/clay-migration-plan.md`. Workspace license is GPL-3.0-or-later (Loro
CRDT inheritance), wired through `license.workspace = true` on every crate.

## Layout
- `packages/prism-daemon` — Rust library + `prism-daemond` stdio bin. The local physics engine.
- `packages/prism-core` — Rust. Shared foundations: design tokens, shell mode, boot config, kernel store + `Shell` wrapper.
- `packages/prism-builder` — Rust. The page builder that replaced Puck. Owns the component registry, document tree, Taffy-backed layout engine, and the unified `lower_ui` render pipeline that drives both the shell renderer and relay SSR.
- `packages/prism-ui-runtime` — Rust. Retained-mode UI runtime: `Surface` + Taffy layout + `RenderCommand` stream + the femtovg / semantic-HTML / hit-testing backends. Consumed by `prism-shell` (live render) and `prism-relay` (SSR).
- `packages/prism-ui-build` — Rust. Build-time helpers for the `.prism-ui` DSL (parser, codegen).
- `packages/prism-shell` — Rust (`cdylib` + `rlib`). Single source of truth for the UI tree. Renders via `prism-ui-runtime::Surface` over winit + femtovg natively; browser build goes through `wasm32-unknown-unknown` + `wasm-bindgen` (the `web` feature), producing `prism_shell.js` + `prism_shell_bg.wasm` next to a hand-written `web/index.html`.
- `packages/prism-studio/src-tauri` — Rust. Packaged desktop shell that spawns `prism-daemon` as a sibling process over `interprocess` + `postcard` and then hands control to `prism_shell::Shell`. The `prism-ui-runtime` backend owns windowing + GPU end-to-end — no Tauri / wry / webview / tao / wgpu anywhere. The `src-tauri/` directory name is a pre-cutover historical artefact; renaming it is a cleanup followup.
- `packages/prism-cli` — Rust. The unified `prism` binary — one front door for `test`, `build`, `dev`, `lint`, `fmt` across every Rust crate. `prism dev all` spawns every dev server behind a tokio process supervisor with colored prefixed logs and Ctrl+C fan-out. See its `CLAUDE.md` for the full subcommand surface.
- `packages/prism-relay` — Rust. Axum-based Sovereign Portal SSR server (`prism-relayd`) that renders `prism_builder::BuilderDocument` trees to semantic HTML via `ui_runtime::lower_semantic_html`. Replaced the Hono TS relay 2026-04-15.

## Commands
**Installing the CLI.** `cargo build -p prism-cli` does NOT put `prism`
on PATH. Either run `./scripts/install-cli.sh` once (installs a real
`prism` binary via `cargo install --path packages/prism-cli`), or use
the committed cargo alias with zero setup: `cargo prism <subcommand>`
(e.g. `cargo prism dev shell`). Both routes go through the same CLI;
the bare `cargo run -p prism-shell` / `cargo build` paths skip build
acceleration + GC and are the "ludicrous build times" trap — don't use
them for the core loop.

**Always go through the unified `prism` CLI — never invoke `cargo
build`/`test`/`clippy`/`clean` directly.** The CLI is not just a
dispatcher: it injects build acceleration (sccache + lld), defaults to
the fast iteration profile, and runs smart `target/` GC on every run.
Raw `cargo` skips all of that and is the main reason builds felt
"ludicrous". Only drop to raw `cargo` for something the CLI genuinely
doesn't wrap yet (e.g. `cargo tree`, `cargo expand`), never for the
core build/test/lint loop. Every subcommand has `--dry-run` to print
the expanded argv, so anything here can be audited with e.g.
`prism --dry-run build`.

- `cargo run -p prism-cli -- test [-p <pkg>]` — `cargo test --workspace` (or scoped to a single crate). Legacy Playwright e2e flags were retired alongside the Hono TS relay on 2026-04-15.
- `cargo run -p prism-cli -- build [--target desktop|studio|web|relay|all] [--ship]` — **defaults to the fast debug profile.** `--ship` opts into the slow runtime-optimised release profile (`codegen-units = 1` + thin LTO, ~10x slower to compile — only for actual release artifacts). `web` runs `cargo build --target wasm32-unknown-unknown -p prism-shell --no-default-features --features web` followed by `wasm-bindgen --target web --out-dir packages/prism-shell/web` over the emitted cdylib. No separate copy step — wasm-bindgen writes `prism_shell.js` + `prism_shell_bg.wasm` directly next to `web/index.html`.
- `cargo run -p prism-cli -- dev [shell|studio|web|relay|all]` — run one or many dev servers. **Native run targets (`shell`/`studio`/`relay`, single or `all`) are compiled by ONE combined `cargo build -p prism-shell -p prism-studio -p prism-relay`, then the dev child *execs the prebuilt binary* (`target/debug/<bin>`) directly.** This is deliberate: a single fixed package set + feature resolution means only genuinely-changed crates recompile — alternating `dev shell` / `dev studio` / `dev all` no longer ping-pongs shared deps through different feature sets, and `dev all` no longer runs N concurrent `cargo run`s fighting the single `target/.cargo-lock`. Hot-reload re-runs that same combined build before each re-exec (via the `DevLoop` prebuild hook). Exceptions: `dev shell --hot=subsecond` keeps the legacy `cargo run -p prism-shell --features hot-reload` path (it needs the feature on the run itself); `dev web` is a separate wasm pipeline (cargo + wasm-bindgen preflight, then `python3 -m http.server 1420`).
- `cargo run -p prism-cli -- lint` — `cargo clippy --workspace --all-targets -- -D warnings`.
- `cargo run -p prism-cli -- fmt [--check]` — `cargo fmt --all`.
- `cargo run -p prism-cli -- gc [--hard]` — **smart, size-aware reclamation.** The default prunes stale incremental sessions (>3d), an idle `wasm32-unknown-unknown` tree (>7d), and an idle build profile (>14d) **without ever touching the active profile's 772-crate dependency cache.** Runs automatically (silently) after every successful `build`/`test`/`dev`. `--hard` is the nuclear `cargo clean` — every subsequent build is a ~30-minute cold rebuild, so it is opt-in, never scheduled. `prism clean` is a back-compat alias for `gc --hard`. **Do not run frequent full cleans — that is the single biggest build-time anti-optimisation on this workspace.**

### Build acceleration
- **`sccache`** → `RUSTC_WRAPPER`, auto-wired by the CLI when on
  `PATH` (install once: `cargo install sccache`; `PRISM_NO_ACCEL=1`
  to disable). Safe to gate at the CLI because `RUSTC_WRAPPER` does
  **not** change the artifact fingerprint — present-via-CLI /
  absent-via-raw-cargo can't cause a recompile.
- **Linker (lld)** is **opt-in via committed `.cargo/config.toml`**,
  not the CLI. A CLI-only `RUSTFLAGS` linker override changes the
  rustc fingerprint, so any cargo run that bypassed the CLI (raw
  `cargo`, rust-analyzer) would see a different fingerprint and
  trigger a full workspace recompile every time you switched tools.
  Build-time flags must be applied uniformly or not at all — see the
  commented opt-in block + rationale at the top of `.cargo/config.toml`.

The root `package.json` exposes the same surface via pnpm scripts
(`pnpm test`, `pnpm dev`, `pnpm build`, `pnpm lint`, `pnpm format`) for users
who prefer that entry point. The web target additionally expects
`wasm-bindgen` on PATH: `cargo install wasm-bindgen-cli` with a version
that matches the `wasm-bindgen` crate pinned in the workspace manifest.

## Style
- Rust 2021 edition, strict clippy.
- Conventional Commits: `type(scope): description`.
- Kebab-case files in docs/scripts; Rust modules follow `snake_case` as usual.
- Never deprecate. Rename, move, break, fix. `cargo check --workspace` is the safety net.

## Architecture
- Loro CRDT = source of truth (via the `loro` Rust crate).
- `prism-ui-runtime` = sole UI runtime. Taffy-backed CSS Grid / Flex / Block layout, retained-mode `Surface`, declarative `.prism-ui` DSL compiled at build time for hand-written components and walked at runtime for the drag-droppable blocks that `prism-builder` materialises from a `BuilderDocument`. No React, no Tailwind, no Puck, no Slint, no Clay, no hand-vendored wgpu renderer.
- Rust → WASM for web (`wasm32-unknown-unknown` + `wasm-bindgen`); Rust → native binary for desktop/mobile.
- Desktop shell is pure `prism-shell` on top of `winit` + `femtovg` — no external windowing or GPU layer to wrangle. Packaging/signing/updater land via `cargo-packager` + `self_update` + the standalone shell crates (`tray-icon`, `notify-rust`, `rfd`, `arboard`, `keyring`).
- Daemon runs as a sibling process launched by the Studio shell on desktop (IPC via `interprocess` + `postcard`), as an in-process tokio subsystem on mobile, and is remote (WebSocket relay) on web.
- Ephemeral state (cursors, drags) lives behind the same `AppState` struct but is serialized out on hot-reload snapshots.

## Workflow
After every implementation:
1. Write/update Rust tests in `src/**/*.rs` (`#[cfg(test)]`).
2. Run `prism test` (and `prism lint` if you touched anything non-trivial) — via the CLI, never raw `cargo`, so acceleration + GC apply.
3. Update the affected package's `CLAUDE.md` if the public API changed.
4. Update `docs/dev/clay-migration-plan.md` if phasing or decisions moved.
5. **For UI / `prism-ui-runtime` changes**: use the visual testing harness to verify.
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
- Migration plan: `docs/dev/clay-migration-plan.md` (Slint plan archived)
- Decisions: `docs/adr/`
- Package context: each package's `CLAUDE.md`
