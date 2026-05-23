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

### `prism visual [--scene <name>] [--list] [--output <dir>]`
- Automated visual regression suite. Runs predefined test scenes
  through the `prism-shell` binary with `--screenshot`, capturing
  PNGs to `screenshots/` (or `--output <dir>`).
- `--scene builder-grid` — run a single scene.
- `--list` — list available scenes and exit.
- No flags → runs all 8 built-in scenes (launchpad, builder-empty,
  builder-grid, builder-tablet, builder-mobile, inspector,
  code-editor, explorer).
- Each scene spawns the shell binary with `--scene <name>
  --screenshot <path>`, which auto-exits after capturing. No
  manual interaction needed.
- Screenshot capture uses `screencapture` on macOS.

### `prism build [--target desktop|studio|web|relay|all] [--ship]`
- Defaults to `--target all` + **fast debug** builds. `--ship` opts
  into the slow runtime-optimised release profile (`codegen-units = 1`
  + thin LTO; ~10x slower to compile — only for real release
  artifacts). The old `--debug` flag is gone: debug is the default
  now, `--ship` is the explicit slow path.
- `desktop` → `cargo build -p prism-shell`.
- `studio` → two cargo builds in order: first
  `cargo build -p prism-daemon --bin prism-daemond --features
  transport-ipc` (the sidecar prism-studio spawns on startup; it
  has to land in the same `target/<profile>/` directory or studio
  aborts with "daemon sidecar unavailable"), then
  `cargo build -p prism-studio`. The `transport-ipc` feature is
  not in the daemon's `default`/`full` preset (mobile/wasm/embedded
  builds explicitly drop it), so the CLI opts in here at the
  studio entry point. Phase 5 will wrap the studio half in
  `cargo-packager` for installer bundles; today it's a plain cargo
  build.
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

### `prism dev [shell|studio|web|relay|all] [--no-hot-reload] [--hot=respawn|subsecond]`
- Defaults to `shell`.
- **Native run targets (`shell` / `studio` / `relay`, single or `all`)
  are compiled by one combined `cargo build` and the dev child execs
  the prebuilt binary directly** (see `prism-shell` CLAUDE.md for why:
  a fixed package set + feature resolution keeps `target/.cargo-lock`
  contention down and avoids feature-flag ping-ponging between
  alternating dev invocations).
- **Hot-reload is on by default.** Single-target `prism dev shell`
  runs the cargo child inside [`crate::dev_loop::DevLoop`], which
  watches `packages/prism-shell/src/` for `.rs` changes and kills +
  respawns the child when a batch lands. cargo's incremental
  compilation keeps iteration fast. `.prui` skeleton edits are
  picked up on the next respawn — the source-first runtime parses
  the file at boot. `--no-hot-reload` disables the respawn loop.
- **`--hot=subsecond`** (Phase 9 of `docs/dev/dioxus-inspiration.md`)
  compiles the shell with `--features hot-reload` so `subsecond::call`
  wraps the render walk, letting the patch pipeline swap in a changed
  `lower_ui` body without dropping the `Surface` tree or reactive
  `Owner` graph. Falls back to `respawn` for changes subsecond can't
  patch (struct-layout edits, public-API breaks).
- `web` is special: runs the `wasm32-unknown-unknown` cargo build
  and the wasm-bindgen post-process as a synchronous preflight,
  then execs `python3 -m http.server 1420 --directory
  packages/prism-shell/web` as the long-running child. Re-invoke
  `prism dev web` for a rebuild — the dev loop's `.rs` respawn
  half is native-only.
- `all` spawns every target behind the process supervisor:
  - Each child gets a colored label prefix on every output line.
  - First non-zero exit tears down all siblings.
  - Ctrl+C kills every child (kill_on_drop + tokio::signal::ctrl_c).
  - Web's preflight (cargo build + wasm-bindgen) runs once before
    the supervisor starts, so the web child is just the static
    server.
  - The `.rs` respawn half is inactive in multi-target mode — the
    `Supervisor` can't kill + respawn individual children mid-run.
    Users who want the full loop should run `prism dev shell`
    alone.

### `prism e2e [--test <name>] [--list] [--record] [--output <dir>]`
- End-to-end test suite. Runs built-in test scripts through the
  `prism-shell` binary in `--e2e` mode. Each script drives the shell
  through the same callback-level paths a human uses — key combos,
  command dispatch, grid cell clicks, viewport switches — then
  asserts expected `AppState` outcomes.
- `--test viewport-switching` — run a single test.
- `--list` — list available tests (12 built-in).
- `--record` — capture baseline screenshots for visual diff.
- `--output /tmp/e2e` — custom screenshot directory.
- `--os-input` — inject real OS keyboard/mouse events via `enigo`
  (requires `prism-shell/e2e` feature + display + accessibility).
- No flags → runs all 12 built-in tests. Exit code 1 if any fail.

### `prism gc [--hard]`
Smart, size-aware reclamation of `target/`. Default (no flag) is the
same sweep that runs automatically after every build (see § Automatic
build-artefact GC): stale incremental sessions, an idle wasm tree, an
idle build profile — **never the active profile's dependency cache**.
Prints a one-line summary of what it reclaimed. `--hard` is the
nuclear `cargo clean` (delegates to `commands::clean::plan`, so the
argv lives in one place).

### `prism clean`
Back-compat alias for `prism gc --hard` — full `cargo clean`, removes
the entire `target/` tree. The nuclear option: every subsequent build
is a ~30-minute cold rebuild of all 772 crates. Use only when
artefacts are corrupt, **never** for routine housekeeping (that is
what the default `prism gc` / automatic sweep is for).

### Build acceleration (`accel` module)
Every `cargo` command the CLI spawns is transparently accelerated,
runtime-detected with graceful degradation:
- `sccache` on `PATH` (and no pre-existing `RUSTC_WRAPPER`) →
  `RUSTC_WRAPPER=sccache`. Install: `cargo install sccache`.
- rustup's `gcc-ld/ld64.lld` (macOS) / `ld.lld` (else) under
  `<sysroot>/lib/rustlib/<host>/bin/` → injected as
  `CARGO_TARGET_<HOST>_RUSTFLAGS=-Clink-arg=-fuse-ld=<path>`. Scoped
  to the host triple so the wasm leg (already wasm-ld) is untouched.
`PRISM_NO_ACCEL=1` disables both. Detection is memoised per process
and only applied at `CommandBuilder::build{,_tokio}()` time for the
`Cargo` program, so `argv()` / `--dry-run` / unit tests are
unaffected. Runtime-gated (not in `.cargo/config.toml`) so a missing
tool can never break a plain `cargo` call.

### `prism lint [--types]`
`cargo clippy --workspace --all-targets -- -D warnings`.

`--types` additionally runs the Luau type pass (§3.3 of
`docs/dev/prism-cross-cutting-systems.md`): locates `luau-analyze`
(`LUAU_ANALYZE` env override → `PATH`) and runs it in strict mode
over every `.luau` source in the workspace, returning its exit code
as a real CI gate. When `luau-analyze` is not installed it prints an
install hint and **skips** (exit 0) — never a fake pass, never a hard
fail; the gate activates the moment the binary is on `PATH`.

### `prism fmt [--check]`
`cargo fmt --all`, optionally `--check`.

### `prism new widget <name> [--dir <path>] [--single-file] [--force]`
Scaffold a new widget in the §5.10-canonical PRUI/PRSS surface
(`docs/dev/prui-luau-fusion.md` Wave H.5). Default: the
sibling-paired trio `<name>.prui` + `<name>.prss` + `<name>.luau`
(auto-attached by basename convention — no `<import>`).
`--single-file` emits one `.prui` with inline `<script>` /
`<style>` blocks instead. The Luau opens `--!strict`. `--dry-run`
lists the files without writing; existing files are refused unless
`--force`; path-traversal names are rejected. Pure filesystem
scaffold — does not shell out. Lives in `commands::new`.

### `prism rewrite-canonical <paths>... [--stdout] [--force] [--check]`
Phase 2 of the expressiveness roadmap
(`docs/dev/prui-expressiveness-roadmap.md` §6.24 + §8). Walks one or
more `.prui` files (or directories — `target/` / `.git/` /
`node_modules/` / `dist/` / `.next/` are skipped) and rewrites
their XML-shape declarations into the canonical surface. The
rewriter is **idempotent** — running it twice over a tree changes
nothing on the second pass — so it's safe to wire into a pre-commit
hook. `--stdout` prints the rewritten content instead of overwriting
the source (single-file review); `--check` (or the global
`--dry-run`) reports what would change without touching disk;
`--force` commits the best-effort output for files that surfaced XML
parse errors (without `--force` those files are skipped with an
informational warning and the CLI exits 2). The rewriter delegates
all transformation logic to `prism_core::language::prism_ui::
rewrite_xml_to_canonical` (the migration tool's payload), so the
behaviour is identical whether invoked via the CLI or library.
Lives in `commands::rewrite_canonical`.

## Automatic build-artefact GC
After every successful `prism build`, `prism test`, or `prism dev`
(web preflight), the CLI runs `gc::sweep` over `target/` and returns a
`SweepReport`. The sweep reclaims, in increasing order of blast radius
(all provably regenerable, none touching the active dep cache):
1. incremental session dirs under `target/{debug,release}/incremental/`
   (+ cross-compile paths) not modified in **3 days**;
2. the whole `target/wasm32-unknown-unknown/` tree if nothing in it
   was touched in **7 days** (only live during web work);
3. one of `debug/` / `release/` if **both** exist and one is idle
   **14 days** — the freshly-built profile always has a recent mtime
   so it is never the victim; this reclaims the *other* profile you
   stopped using while leaving the active profile's 772-crate
   dependency cache fully intact.

**Why we don't dedup `deps/` / `.fingerprint/`.** An earlier draft
deduped `<crate>-<hash>` artefacts down to the newest hash per
`(lib_prefix, crate)`. That broke on workspaces where a single dep
is compiled with different feature sets for different consumers —
each consumer's dep-info pins a specific hash, and dropping a
"duplicate" leaves the next compile with `extern location for X
does not exist: …`. For deeper reclaim use `prism clean`
(`cargo clean`) or `cargo clean -p <crate>` — both are cargo-aware
and honour per-consumer hash pinning.

The sweep is silent and best-effort — individual removal errors are
ignored.

## Library surface
The crate is split into a library + a thin binary so tests and
sibling crates can reach into it without going through `std::process`.

- `builder::CommandBuilder` — fluent argv builder with
  `cargo()` / `pnpm()` / `python3()` / `wasm_bindgen()`
  constructors, `package()` / `workspace()` / `release()` /
  `arg()` / `args()` / `cwd()` / `env()` / `label()` combinators,
  and `argv()` / `display()` / `build()` / `build_tokio()` outputs.
  `build{,_tokio}()` layer `accel::cargo_env()` onto `Cargo`
  commands for keys the caller didn't set explicitly; `argv()` is
  intentionally left pure so `--dry-run` and unit tests are stable.
- `accel::cargo_env()` — memoised, runtime-detected build
  acceleration env (`RUSTC_WRAPPER=sccache`, host-scoped lld
  `RUSTFLAGS`). Empty when tools are absent or `PRISM_NO_ACCEL` is
  set. See § Build acceleration.
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
- `watch::WatchLoop` — notify-driven file watcher (§11 of the
  original `docs/dev/slint-migration-plan.md`, lifted into the
  unified dev loop). Wraps `notify::RecommendedWatcher` and exposes
  `next_batch(timeout)` / `try_next_batch()` that return
  deduplicated `WatchBatch { paths }` values debounced over a
  150ms default window. Drops pure access events
  (`EventKind::Access`) so the scaffold doesn't fire on reads. Not
  wired into `prism dev` yet — Phase 2/3 plug it in once the shell
  has a store-preserving reload path. Tests in `src/watch.rs`
  cover a tempfile round-trip, an idle non-block, and a quiet-dir
  timeout.
- `gc::sweep(target_dir) -> SweepReport` — post-build GC. Trims
  stale incremental sessions (3d), an idle wasm tree (7d), and an
  idle build profile (14d), never the active dep cache. Called
  automatically on successful `build`, `test`, and `dev` (web
  preflight) runs; see § Automatic build-artefact GC.
  `gc::trim_incremental` is preserved as a thin (return-discarding)
  alias for older callers.
- `commands::{test, build, dev, lint, fmt, gc, clean}` — each
  exposes a `plan(args, workspace) -> Vec<CommandBuilder>` pure
  function and a `run(...)` wrapper. `gc` (soft) is the exception:
  it sweeps the filesystem directly and its `plan` is empty unless
  `--hard` (which reuses `clean`'s plan). Everything shell-worthy
  funnels through `commands::execute_plan` so `--dry-run` lives in
  one place.

## package.json integration
The root `package.json` scripts all delegate to `prism` via
`cargo run -q -p prism-cli --`, so `pnpm test`, `pnpm dev`, and
`pnpm build` stay available for users who prefer the pnpm surface.
