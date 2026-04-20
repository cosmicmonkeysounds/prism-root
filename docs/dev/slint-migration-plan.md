# Slint Migration Plan

> Migrating Prism's UI layer off React / TypeScript / Tailwind onto
> [Slint](https://github.com/slint-ui/slint) so that Studio, the page
> builder, and every Lens can run cross-platform from a single
> codebase — while still shipping a first-class web target via WASM.

**Status:** **Phase 0 closed 2026-04-15.** **Phase 1 closed 2026-04-15**
end-to-end — `prism-shell::telemetry::FirstPaint` (first-paint
telemetry wired into `Shell::run` via Slint's rendering notifier),
`prism-cli::watch::WatchLoop` (notify-driven debounced file watcher),
and `prism-cli::dev_loop::DevLoop` (rebuild-and-respawn supervisor
that wraps the cargo child + WatchLoop into a single reload loop)
are all live behind unit tests. `prism dev shell` now defaults to a
unified hot-reload path: Slint's native `live-preview` feature
(via `SLINT_LIVE_PREVIEW=1` + `--features prism-shell/live-preview`)
reloads `.slint` files in-process through `slint-interpreter`, and
the dev loop kills + respawns the cargo child on any `.rs` change
under `packages/prism-shell/src/`. `--no-hot-reload` opts out of
both legs. `prism-cli`'s web pipeline already targets
`wasm32-unknown-unknown` + `wasm-bindgen`. **Phase 2 substantially
closed 2026-04-15** — every leaf subtree the Slint migration put on
`prism-core`'s plate (`foundation`, `identity`, `language`, `kernel::{store,
state_machine, config}`, `interaction::{notification, activity, query}`)
is ported and green under `cargo test -p prism-core` (655 tests, clippy
clean). **Phase 2b closed 2026-04-15** as well — the ADR-002 `kernel`
orchestration kit (`actor`, `intelligence`, `automation`, `plugin`,
`plugin_bundles`, `builder`, `initializer`) and its `PrismKernel`
wiring layer landed, alongside the `domain` subtree (`flux`, `timeline`,
`graph_analysis`) and the `statig` rewrite of `kernel::state_machine::tool`.
`cargo test -p prism-core --lib` now runs 1033 unit tests green. The
`network` subtree's relay layer (`network::relay` — 17 composable modules
+ module system, `network::relay_manager`, `network::presence`) is fully
ported; remaining stubs are `discovery`, `session`, `server`. `prism-relay`
ships the full 17-module HTTP/WS surface (~80 API endpoints + WebSocket
relay protocol + middleware stack + persistence + config). Phase 3 is the
active porting surface. Rust workspace scaffolded; `prism-daemon`,
`prism-cli`, and `prism-relay` shipping; `prism-core` closed on its
Phase-2a scope; `prism-builder`, `prism-shell` under active port.
`prism-studio/src-tauri` is the canonical desktop shell — a ~30-line
launcher that spawns the daemon sidecar and hands control to
`prism_shell::Shell`, which runs Slint's native winit + femtovg
backend (see §4.5). Per-package `CLAUDE.md` files carry live status.

**Owner:** TBD
**Created:** 2026-04-14
**Last updated:** 2026-04-19

---

## 0. Why Slint (and not Clay)

Phase 0 originally targeted [Clay](https://github.com/nicbarker/clay),
an immediate-mode C layout library, hosted on a hand-vendored wgpu
renderer running on bare `tao` for desktop and on an emscripten
`wasm32-unknown-emscripten` Canvas2D adapter for web. That stack
booted end-to-end on 2026-04-15 but exposed three problems:

1. **Two renderers to maintain.** Desktop (wgpu) and web
   (Canvas2D) disagreed on everything — text metrics, DPI, input
   coordinates, frame scheduling. Every Clay component had to be
   validated on both renderers manually.
2. **No declarative surface.** Clay is pure immediate-mode C; every
   panel was a long hand-written Rust function emitting layout
   commands. Studio's property-panel matrix (per-component prop
   editors, live re-render, drag-and-drop) would have required
   inventing a declarative layer from scratch.
3. **Puck replacement was a greenfield bet.** `prism-builder`'s
   runtime walker — the thing that materialises a `BuilderDocument`
   into a live widget tree — had no natural home in the Clay model.

[Slint](https://github.com/slint-ui/slint) resolves all three:

- **One engine.** Slint ships its own winit-based windowing and a
  `femtovg`-backed renderer that runs identically on desktop
  (native) and web (`<canvas>`). One render path, one input model,
  one DPI policy.
- **Declarative `.slint` DSL.** Hand-written panels stay declarative
  and compile-time type-checked via `slint-build`. Studio's property
  panels will drive the same DSL through `slint-interpreter` for
  runtime compilation of `BuilderDocument`s.
- **First-class runtime compiler.** `slint-interpreter` takes a
  `.slint` source string and materialises a live component tree —
  exactly what `prism-builder` needs for Phase 3. No greenfield
  walker, no separate widget library.

The trade-off is licensing: Slint's royalty-free terms require
**GPL-3.0-or-later** on our side. The workspace switched to GPL on
2026-04-15 via `license.workspace = true`; every Prism crate now
carries that header.

## 1. Non-goals

- Not rewriting the daemon. `prism-daemon` is already Rust and stays.
  Its own `wasm32-unknown-emscripten` build (driven by `mlua`'s
  vendored Luau) is independent of the UI layer and keeps using
  emscripten.
- Not changing the data layer. Loro CRDT remains source of truth.
- Not porting the Relay's *client UI* — the relay has none, it
  renders HTML for unauthenticated visitors. The relay's *server*
  was rewritten from Hono JSX SSR to Rust axum on 2026-04-15 and
  now calls `prism_builder::render_document_html` directly. Slint
  is **not** involved on the server side.

## 2. Scope inventory (what has to move)

| Surface | Status | Notes |
|---|---|---|
| `prism-daemon` | ✅ shipping | Rust, transport-agnostic kernel. Unchanged by pivot. |
| `prism-cli` | ✅ shipping | Unified `prism` CLI. Web build path retargeted `wasm32-unknown-unknown` + `wasm-bindgen` 2026-04-15. |
| `prism-relay` | ✅ shipping | Rust axum SSR, walks `BuilderDocument` via `prism_builder::render_document_html`. |
| `prism-core` | ✅ Phase 2a closed / 🚧 Phase 2b pending | Leaf-first port of `@prism/core` to Rust. **Landed:** `foundation::{batch, clipboard, date, object_model, persistence, template, undo, vfs}`, `identity::{did, encryption, manifest, trust}`, `language::{syntax, expression, registry, document, forms, markdown, codegen}` + the `luau` contribution stub, `kernel::{store, state_machine::machine, config}`, `interaction::{notification, activity, query}`. 655 unit tests, clippy clean. **Pending (Phase 2b):** `kernel::{actor, automation, intelligence, plugin, plugin_bundles, builder, initializer}` (ADR-002 §Part C `PrismKernel` orchestration), `network`, `domain`, and the `statig` rewrite of the xstate tool machine. None are on Phase 3's critical path. |
| `prism-builder` | ✅ Phase 3 closed | `Component` trait (two render targets — Slint DSL via `SlintEmitter` + HTML SSR), `ComponentRegistry`, field factories (`FieldSpec` / `FieldKind` / `NumericBounds` / `SelectOption` / `FieldValue`), `slint_source::SlintEmitter` on `prism_core::language::codegen::SourceBuilder`, `render_document_slint_source` + `compile_slint_source` + `instantiate_document` (the last two gated on the `interpreter` cargo feature), `starter::register_builtins` catalog shared with `prism-relay`. **ADR-003 layout engine landed 2026-04-19**: `layout.rs` adds Taffy-backed CSS Grid / Flexbox / Block layout with `PageLayout` (page dimensions, margins, bleed, grid template), `LayoutMode` (Flow | Free per node), `FlowProps`, and `compute_layout` pass integrating Taffy layout + `prism_core::foundation::spatial::Transform2D` propagation. `Node` extended with `layout_mode` + `transform`; `BuilderDocument` extended with `page_layout`. 80 unit tests green (+ 1 interpreter round-trip). |
| `prism-shell` | ✅ Phase 3 closed | Four-panel `Shell` on Slint: Identity / Builder / Inspector / Properties. `AppState` holds a serializable `BuilderDocument` + selected-node id; the shell owns an `Arc<ComponentRegistry>` rebuilt via `prism_builder::register_builtins` on boot. `ui/app.slint` exposes `nav-items` + `select_panel(int)` callback and dispatches on `active_panel` to render identity actions, generated Slint DSL source, an indented tree, or a schema-driven field-row list. Native bin + `wasm32-unknown-unknown` cdylib both in scope. |
| `prism-studio/src-tauri` | ✅ thin launcher | ~30-line `main.rs`: spawn daemon sidecar, build `Shell`, call `Shell::run()`. Slint owns the event loop. |
| Studio panels (~40+) | 🚧 partial | Phase 3 landed four (Identity, Builder, Inspector, Properties). The remaining ~36 panels follow the same `Panel` trait + `ActivePanel` variant + `Shell::sync_ui` branch + Slint conditional surface. |
| Legacy Puck component impls (~30+) | 🚧 partial | Phase 3 moved the starter catalog (heading/text/link/image/container) into `prism_builder::starter` — the shared home for both the shell builder panel and the relay's SSR route. Remaining legacy impls re-register against `prism_builder::ComponentRegistry` via the same pattern. |
| `prism-admin-kit` | ⏳ pending | Port to Slint under Phase 3 / 4 depending on ordering. |

## 3. Host language: Rust (decided)

Slint is Rust-first and the ecosystem around it (winit, femtovg,
wasm-bindgen, slint-interpreter) is all Rust. Host language is
unchanged from the Clay plan: **Rust, 2021 edition, workspace
clippy = deny warnings**. Same justification — WASM target, native
binaries, Loro binding, no hidden runtime.

## 4. Runtime topology

### 4.1 Desktop / packaged Studio (`prism-studio/src-tauri`)
- Rust binary. ~30 lines of `main.rs` — spawn the `prism-daemond`
  sidecar, build a `prism_shell::Shell`, hand off to `Shell::run()`.
- Slint owns windowing (winit), layout, rendering (femtovg), input,
  and DPI. No `tao`, no `wgpu`, no hand-vendored renderer.
- Daemon runs as a sibling process over `interprocess` +
  `postcard` per §4.5. The `src-tauri/` directory name is a
  historical artefact from the pre-Slint Tauri spike.

### 4.2 Standalone dev shell (`prism-shell`)
- Native dev binary under `src/bin/native.rs`: same
  `Shell::new()?.run()` shape as Studio, no daemon sidecar.
- Web build via `wasm32-unknown-unknown` + `wasm-bindgen` — crate
  type is `["cdylib", "rlib"]`; the `web` feature exposes a
  `#[wasm_bindgen(start)]` entry point that boots the same `Shell`
  into a `<canvas id="canvas">` on the page.
- Both targets share `src/app.rs`, `src/panels/*`, and
  `ui/app.slint`.

### 4.3 Mobile
- Slint exposes iOS (UIKit-based winit backend) and Android
  (android-activity + winit) targets. Mobile binaries re-export the
  same `Shell` via `cargo-mobile2` C ABI. Phase 5 work.

### 4.4 Web target
- `cargo build --target wasm32-unknown-unknown -p prism-shell
  --no-default-features --features web` emits
  `target/wasm32-unknown-unknown/<profile>/prism_shell.wasm`.
- `wasm-bindgen --target web --out-dir packages/prism-shell/web`
  post-processes the cdylib into `prism_shell.js` +
  `prism_shell_bg.wasm` next to the hand-written `web/index.html`.
- `prism dev web` serves that directory via
  `python3 -m http.server 1420` after the cargo + wasm-bindgen
  preflight. `prism build --target web` runs the same pipeline.
- `wasm-bindgen-cli` must be installed with a version matching the
  `wasm-bindgen` crate pinned in the workspace manifest.

### 4.5 Desktop shell model — Slint owns everything

The desktop shell is **pure Slint**. There is no separate windowing
layer (no `tao`), no separate renderer (no `wgpu`), no webview (no
`wry`, no Tauri), and no hand-vendored `UiRenderer`. Slint's bundled
winit + femtovg backend handles window creation, pointer / keyboard
/ scroll input, DPI / scale factor, redraw scheduling, and font
rasterisation in one crate.

Studio's only job on top of that is to supervise the daemon
sidecar:

1. `spawn_dev()` in `prism-studio/src-tauri/src/sidecar.rs` locates
   `prism-daemond` next to the Studio binary in
   `target/<profile>/` (via `std::env::current_exe`), spawns it
   with `--ipc-socket prism-daemon-<pid>.sock`, retries
   `connect_client` for up to 2 s, and reads the handshake banner.
2. The returned `DaemonSidecar` handle is held for the lifetime of
   the Slint event loop; its `Drop` impl best-effort-kills + waits
   the child so the supervise/kill path runs even on panic.
3. `DaemonSidecar::invoke` is the single entry point — sends an
   `IpcRequest`, reads an `IpcResponse`, asserts the id matches,
   returns the JSON result. Pipelining is available by design
   (request IDs are correlated) but unused today.

Phase 5 will replace the dev-time "find the binary next to the
Studio binary" dance with a `cargo-packager`-bundled
`prism-daemond-<triple>` resource shipped inside the Studio
installer.

## 5. Licensing

The workspace moved to **GPL-3.0-or-later** on 2026-04-15 to comply
with Slint's royalty-free terms. Every crate declares
`license.workspace = true`; the root `LICENSE` is the FSF GPLv3
text. Dual-licensing or a commercial Slint license are open
follow-ups for Phase 5 / packaging if the team decides the GPL
terms are unworkable for a shipped installer.

## 6. `prism-core` — foundations port

Phase 2. Pure data + pure functions land here leaf-first; the TS
`@prism/core` package has been deleted. Port order follows ADR-002
and per-module `//!` headers track live status in the crate. The
`language::*` subtree (syntax scanner, expression parser, forms,
markdown dialect, codegen pipeline) is on the critical path for
Phase 3 — the Studio builder depends on `LanguageContribution<R, E>`
with `R` bound to a Slint component handle.

### 6.0 Phase 2a — landed (2026-04-15)

Everything Phase 3 depends on is green. Per-module status lives in
`packages/prism-core/CLAUDE.md`; the summary below is the migration-plan
view.

**Foundations** — `foundation::{batch, date, object_model,
undo, vfs, clipboard, template, persistence}`. `persistence` is gated
behind the `crdt` feature and wraps Loro (`CollectionStore` +
`VaultManager<A>` + `MemoryAdapter`).

**Identity** — `identity::{did, encryption, manifest, trust}`. Ed25519
identities with multi-sig, AES-GCM-256 vault key manager with HKDF
derivation, privilege-set manifest parser/enforcer, and the sovereign
immune system (Luau sandbox, schema poison-pill validator, hashcash PoW,
peer trust graph, Shamir secret sharing, encrypted escrow, PBKDF2
password auth).

**Language** — `language::{syntax, expression, registry, document,
forms, markdown, codegen}` + the `luau` contribution stub.
`LanguageContribution<R, E>` is generic over renderer and editor slots
so the Slint builder can specialise without leaking Slint types into
`prism-core`. The Luau parser is stubbed (`parse` returns an empty
`RootNode`) — the full-moon Rust port is Phase 4 scope; the mlua-backed
execution runtime already lives in `prism-daemon::modules::luau_module`
and does not need a parser.

**Kernel** — `kernel::{store, state_machine::machine, config}`.
`Store<S>` is the reducer + subscription bus from §6.1. `machine` is
the flat hand-rolled FSM; the xstate-backed `tool.machine.ts` is
deliberately not ported — it will return as a `statig` rewrite in Phase
2b. `config` is the layered `ConfigRegistry` + `ConfigModel` +
`FeatureFlags` trio with the 17 built-in settings, JSON-Schema-subset
validator, and post-borrow listener dispatch so callbacks can re-enter
the model safely.

**Interaction** — `interaction::{notification, activity, query}`.
Notification registry + debounced dedup queue (timer-agnostic via a
`TimerProvider` trait), append-only activity log + formatter + date
bucketing, and the pure filter/sort/group pipeline over `GraphObject`.
`query` is the renamed half of the legacy `view` subtree; `SavedView` /
`LiveView` / `ViewMode` are intentionally dropped — every "view" is a
`prism_builder::Component`.

**Tests & safety net** — 655 unit tests across `prism-core`, clippy
clean under `-D warnings`. `cargo test --workspace` is the check the
CI loop runs.

### 6.2 Phase 2b — landed (2026-04-15)

Phase 2b closed the residual Phase-2 surface. None of it was on
Phase 3's critical path, so it ran in parallel with the builder/shell
port.

1. ✅ **`kernel` orchestration kit** (ADR-002 §Part C). All seven
   subtrees landed:
   - ✅ `kernel::actor` — `ProcessQueue` + `ActorRuntime` trait +
     `TestRuntime`, synchronous `process_next` / `process_all` (Rust
     deviation from TS async), per-runtime `CapabilityScope` allow-lists,
     priority queue, subscription bus. 14 unit tests.
   - ✅ `kernel::intelligence` — `AiProviderRegistry` +
     `AiProvider` trait + `OllamaProvider` / `ExternalProvider` /
     `TestAiProvider` + `AiHttpClient` trait + `ContextBuilder`. Split
     out of the legacy `actor/` folder per ADR-002 §Part C. 11 unit
     tests.
   - ✅ `kernel::automation` — trigger / condition / action engine.
     `AutomationEngine::new(store, handlers, options)` takes
     host-supplied `Arc<dyn AutomationStore>` + handler map, so it is
     **not** wired into `PrismKernel` by default. `DelaySleeper` trait
     (`SystemSleeper` + `FakeSleeper`), path-walking condition evaluator,
     `{{…}}` template interpolation, cron-style `tick(now_ms)`. 24 unit
     tests.
   - ✅ `kernel::plugin` — `PluginRegistry` wraps four inner
     `ContributionRegistry<T>` buckets (views / commands / keybindings /
     context menus) with synchronous listener buses. `PrismPlugin` trait
     + idempotent-by-id registration. 7 unit tests.
   - ✅ `kernel::plugin_bundles` — six built-in bundles (`work` /
     `finance` / `crm` / `life` / `assets` / `platform`) + `flux_types`
     submodule, `PluginInstallContext<'a>` fan-out helper,
     `create_builtin_bundles()` + `install_plugin_bundles()`. 8 unit
     tests.
   - ✅ `kernel::builder` — `BuilderManager` + `AppProfile` +
     `BuildPlan` + `BuildStep` + `BuildTarget`. Two concrete
     `BuildExecutor` impls: `DryRunExecutor` + `CallbackExecutor` (the
     latter wraps an IPC closure for hosts talking to `prism-daemon`).
     Six builtin profiles (studio / flux / lattice / cadence / grip /
     relay) via `BuiltInProfileId`. `materialize_starter_app` seeds a
     fresh workspace's app-shell + page-shell + route tree from a
     `StarterAppTemplate`. Not to be confused with the `prism-builder`
     crate. 22 unit tests.
   - ✅ `kernel::initializer` — `KernelInitializer<TKernel>` generic
     trait, `install_initializers(&[...], kernel) -> Disposer`, composite
     reverse-order teardown, `noop_disposer()`. 3 unit tests.

   The big consumer, ✅ `kernel::prism_kernel::PrismKernel`, landed as
   the canonical wiring layer — a **narrowed** port of `studio-kernel.ts`
   (the legacy 2431-line class). It composes only framework-free
   Layer-1 primitives: `ObjectRegistry`, `PluginRegistry`,
   `ConfigRegistry` + `ConfigModel` + `FeatureFlags`, `NotificationStore`,
   `ActivityStore`, `BuilderManager`, `ProcessQueue`,
   `AiProviderRegistry`. `PrismKernelOptions` knobs let hosts swap the
   config registry, skip the six built-in bundles, override the build
   executor, or inject extra app profiles. `PrismKernel` is
   **single-threaded by design** (`ConfigModel` / `FeatureFlags` /
   `ActivityStore` are `!Send` — matches the TS main-thread invariant).
   Intentional omissions: `CollectionStore` (gated on the `crdt` feature,
   host-owned), `AutomationEngine` (host supplies store + handlers),
   domain engines (stateless / per-document), and `network::*` (composes
   differently per host). Puck / Lens / React / Tauri wiring stays out
   of `prism-core` entirely and lives in `prism-shell` + the studio host
   crate. 10 unit tests.
2. ✅ **`domain`** — app-level content and domain entities (ADR-002
   §Part B). `domain::flux` (Flux registry, 11 entity + 7 edge + 8
   automation-preset defs, CSV / JSON export/import, 38 tests),
   `domain::timeline` (pure-data NLE / show-control engine with
   `TempoMap` + `ManualClock` + transport / track / clip / automation /
   marker CRUD, 67 tests), `domain::graph_analysis` (dependency graphs,
   Kahn topo sort, cycle detection, BFS impact analysis, CPM forward /
   backward pass, 30 tests).
3. ✅ **`kernel::state_machine::tool`** — the `statig`-backed rewrite
   of the xstate tool-mode FSM.
4. ✅ **`network`** (relay layer) — `presence` (39 tests),
   `relay` (17 composable modules + module system, 29 tests), and
   `relay_manager` (23 tests) are fully ported. The full 17-module
   feature surface is wired end-to-end in `prism-relay` (~80 HTTP
   endpoints, WebSocket relay protocol, middleware, persistence, config).
   **Remaining stubs:** `discovery`, `session`, `server` — these are
   not consumed by `prism-relay` and do not gate anything else.

Phase 2b did **not** block Phase 3. The Slint walker (`slint-interpreter`
on top of `prism-builder`'s `render_slint`), the Studio panel ports,
and the property panel + field factories all depend on `language::*`,
`kernel::store`, `kernel::config`, and `interaction::*` — all of which
were already green before Phase 2b began.

### 6.1 Kernel store — zustand replacement

`kernel::store::Store<S>` is the hand-rolled reducer + subscription
bus that replaces `zustand`. It satisfies the §7 hot-reload
constraints directly:

1. **One root state struct.** `Store<S>` is parameterised on a
   single `S`; everything reloadable lives inside that `S`.
2. **No global mutable state.** No `OnceCell`, no `static mut` —
   the store owns state by value and is itself a plain struct.
3. **Owned subscribers.** Listeners live in a `Vec` inside the
   store, not a global registry.
4. **`snapshot` / `restore` via serde.** A single serde call
   round-trips the store through bytes, which is what the
   hot-reload watcher consumes.

`prism_shell::Shell` wraps a `Store<AppState>` and exposes the
exact same snapshot/restore surface.

## 7. Hot-reload

`prism dev shell` ships a unified hot-reload path as of 2026-04-15.
Two orthogonal mechanisms compose:

1. **`.slint` files — Slint's native `live-preview` feature.** The
   cargo command `prism dev shell` produces adds
   `--features prism-shell/live-preview` and sets
   `SLINT_LIVE_PREVIEW=1` at compile time. `slint-build` notices
   the env var and swaps the baked `AppWindow` codegen for a
   `LiveReloadingComponent` wrapper that parses `ui/app.slint` at
   runtime via `slint-interpreter` and reloads it whenever the file
   changes on disk. The public `AppWindow` API (`new`, getters,
   setters, callbacks) is unchanged, so `Shell::sync_ui` keeps
   working — the migration is wire-compatible. State held in
   `Store<AppState>` is untouched because the process never exits.
2. **`.rs` files — `prism dev shell`'s respawn loop.** Rust source
   changes can't be hot-swapped without something like `subsecond`
   (Phase 4). Until then the cargo child runs inside
   `prism_cli::dev_loop::DevLoop`, a kill-and-respawn supervisor
   that wraps `WatchLoop` over `packages/prism-shell/src/`. Any
   `.rs` batch kills the child and re-execs `cargo run`; cargo's
   incremental compilation keeps iteration fast. Ctrl+C tears the
   loop down cleanly via an injectable shutdown future (same
   pattern the multi-process `Supervisor` uses).

`--no-hot-reload` opts out of both legs (drops the env var, the
feature flag, and the respawn supervisor) and falls back to a plain
`cargo run` exec — useful when the interpreter's compile cost is
unacceptable or when debugging something the extra wiring obscures.

`prism dev all` includes the shell slot with the Slint live-preview
half enabled (pure cargo build flags, cheap to propagate), but the
`.rs` respawn half is **inactive in multi-target mode** — the
`Supervisor` doesn't know how to kill + respawn individual children
mid-run. Users who want the full loop should run `prism dev shell`
alone.

Phase 4 follow-on:

- **`subsecond`** — a rustc/lld integration that swaps Rust code
  without tearing down the process. Replaces the `.rs` respawn leg
  above with something closer to Slint's in-process reload story.
  Deferred because it's a large dependency and the dev loop ships
  without it.

Every reloadable structure must stay serde-serializable and live
under a single root type. The §6.1 store is the enforcement point.
`AppState` already satisfies this; the respawn path rebuilds the
store from `AppState::default()` on every restart, so any field
that should survive across a `.rs` reload must be persisted
externally.

## 8. `prism-builder` — Puck replacement on Slint

`prism-builder` owns the component-type registry, the
`BuilderDocument` tree, and two render targets — semantic HTML for
Sovereign Portals and a `.slint` DSL emitter that feeds the
`slint-interpreter` compile loop.

### 8.1 Component trait — two render targets

Every block implements `prism_builder::Component`:

```rust
pub trait Component: Send + Sync {
    fn id(&self) -> &ComponentId;
    fn schema(&self) -> Vec<FieldSpec>;
    fn render_slint(
        &self, ctx: &RenderSlintContext<'_>, props: &Value,
        children: &[Node], out: &mut SlintEmitter,
    ) -> Result<(), RenderError> { /* default: Rectangle { children } */ }
    fn render_html(
        &self, ctx: &RenderHtmlContext<'_>, props: &Value,
        children: &[Node], out: &mut Html,
    ) -> Result<(), RenderError> { /* default: <div data-component="id"> */ }
}
```

- `render_html` drives the Sovereign Portal SSR path. Default impl
  emits `<div data-component="id">` + recursive children.
- `render_slint` drives the Studio builder surface. Components emit
  `.slint` DSL snippets into a shared
  [`SlintEmitter`][prism_builder::slint_source::SlintEmitter] —
  itself a thin wrapper around `prism_core::language::codegen::SourceBuilder`
  so the walker slots into the existing codegen pipeline. The
  document-level walker
  [`render_document_slint_source`][prism_builder::render::render_document_slint_source]
  wraps every walked tree in an `export component BuilderRoot
  inherits Window` shell; [`compile_slint_source`] and
  [`instantiate_document`] (gated behind the `interpreter` cargo
  feature) hand the synthesized source to `slint_interpreter::Compiler`
  and return a live `ComponentDefinition` / `ComponentInstance`.

### 8.2 Registry as the single DI surface

New block types go through `ComponentRegistry::register(Arc<dyn
Component>)`. No side registries, no per-component singletons, no
hand-wired `Node` factories. Field factories are live in
`registry.rs`: `FieldSpec` / `FieldKind` / `NumericBounds` /
`SelectOption` / `FieldValue`, with typed builders (`text`,
`textarea`, `number`, `integer`, `boolean`, `select`, `color`) that
preserve default + required invariants. `Component::schema` returns
`Vec<FieldSpec>`, which the Studio Properties panel walks to paint
one field row per entry (read-only today; `Store<AppState>`-backed
editing lands with the first interactive panel in Phase 4).

### 8.3 Starter catalog

`prism_builder::starter::register_builtins(&mut registry)` seeds
seventeen components — `heading`, `text`, `link`, `image`,
`container`, `form`, `input`, `button`, `card`, `code`, `divider`,
`spacer`, `columns`, `list`, `table`, `tabs`, `accordion` — each
implementing both render targets. The catalog is shared: both
the Sovereign Portal relay (`prism_relay::AppState::new`) and the
Studio shell (`prism_shell::Shell::from_state`) call it on boot.
Adding a new default block means adding one file under
`prism_builder::starter` and registering it in `register_builtins`;
both crates pick it up automatically.

## 9. Phase 0 exit (2026-04-15)

- Workspace on Slint 1.8; every crate compiles `cargo check
  --workspace`.
- Workspace license is GPL-3.0-or-later.
- `prism-shell` boots a Slint `AppWindow` (compiled from
  `ui/app.slint`) via `Shell::new()?.run()`. Native dev binary
  confirmed end-to-end.
- `prism-studio/src-tauri` is a ~30-line launcher that spawns the
  daemon sidecar over `interprocess` + `postcard` and hands the
  Slint event loop to `prism_shell::Shell`.
- `prism-builder` compiles with `render_slint` stubbed and
  `render_html` live; the Rust axum relay in `prism-relay` uses
  `render_document_html` in production.
- `prism-cli`'s `build`/`dev` web paths target `wasm32-unknown-unknown`
  + `wasm-bindgen`; dry-run tests pin the new argv shape.

## 10. Test parity

Two levels of coverage through Phase 2 and 3:

1. **Per-module unit tests** (`#[cfg(test)]`) with `insta` snapshots
   for text emitters, HTML SSR output, and any serde round-trip.
   Target is byte-identical to the TS fixture set where one
   exists.
2. **`cargo test --workspace`** via `prism test`. The legacy
   Playwright E2E suite was retired 2026-04-15 alongside the Hono
   TS relay; the Rust axum relay's integration tests in
   `packages/prism-relay/tests/routes.rs` run under the default
   `cargo test --workspace` path.

Mobile FFI sanity checks (iOS xcframework + per-ABI Android
cdylibs) are re-added as Phase 0 spike tasks when the
`cargo-mobile2` host lands.

## 11. Phase roadmap

| Phase | Scope | Status |
|---|---|---|
| 0 | Workspace scaffold, Slint pivot, license switch, `Shell::new()?.run()` end-to-end | ✅ closed 2026-04-15 |
| 1 | `prism-cli` web pipeline retarget, first-paint telemetry, unified Slint live-preview + `.rs` respawn dev loop | ✅ closed 2026-04-15 |
| 2a | `prism-core` leaf port: `foundation`, `identity`, `language::{syntax, expression, registry, document, forms, markdown, codegen}`, `kernel::{store, state_machine::machine, config}`, `interaction::{notification, activity, query}` | ✅ closed 2026-04-15 (655 tests, clippy clean) |
| 2b | ADR-002 `kernel` orchestration kit (`actor`, `automation`, `intelligence`, `plugin`, `plugin_bundles`, `builder`, `initializer`) + `PrismKernel` wiring + `network` (relay layer: 17 modules, relay_manager, presence) + `domain` + `kernel::state_machine::tool` (`statig` rewrite of the xstate tool machine) | ✅ closed 2026-04-18 (1033 tests, clippy clean; residual stubs: `network::{discovery, session, server}`) |
| 3 | `prism-builder` Slint walker via `slint-interpreter`, Studio panel ports, property panel + field factories | ✅ closed 2026-04-15 (767 tests, clippy clean) |
| B1 | Builder unification: shell std-widgets rewrite (`Button`, `LineEdit`, `Switch`, `Palette` theming) | ✅ closed 2026-04-18 |
| B2–B5 | Builder unification: reactivity cleanup, builder/shell merge, HTML SSR separation, interactive builder | ⏳ pending (see `docs/dev/builder-unification.md`) |
| 4 | `language::luau` full-moon parser, advanced lenses, `subsecond` hot-reload | ⏳ pending |
| 5 | `cargo-packager` bundling, `self_update` auto-update, mobile targets, tray/notification/clipboard/keyring wiring | ⏳ pending |

## 12. Decision log

- **2026-04-14** — Phase 0 started. Initial stack: Clay + wgpu +
  tao + emscripten. Workspace scaffolded; `prism-daemon`,
  `prism-cli`, `prism-relay` shipped first.
- **2026-04-14** — Tauri 2 "no-webview" spike (Option B) landed
  and was retired same day: `tauri-runtime-wry-2.10.1/src/lib.rs:552`
  unconditionally drops every tao event except
  `Resized`/`Moved`/`Destroyed`/`ScaleFactorChanged`/`Focused`/
  `ThemeChanged` before it reaches the user callback.
- **2026-04-15 (morning)** — Option C landed: bare `tao` + `wgpu`
  + hand-vendored `UiRenderer` + `clay-layout` (stopgap). Phase 0
  spike #6 (`tests/ipc_bin.rs` in `prism-daemon`) validated the
  daemon sidecar wire end-to-end.
- **2026-04-15 (afternoon)** — Clay → Slint pivot. Same-day retirement
  of `clay-layout` + the hand-vendored `UiRenderer` + the
  emscripten/Canvas2D web path. `prism-shell` crate type switched
  to `["cdylib", "rlib"]`; `build.rs` now drives `slint-build`;
  `ui/app.slint` is the hand-written declarative root. Workspace
  license switched to GPL-3.0-or-later. `prism-studio/src-tauri`
  collapsed to a ~30-line launcher. `prism-cli` web pipeline
  retargeted `wasm32-unknown-unknown` + `wasm-bindgen`. Phase 0
  re-closed on the new stack.
- **2026-04-15** — Hono TS relay retired, Rust axum relay promoted
  to production. Legacy Playwright E2E suite retired. `prism test`
  becomes a thin Rust-only wrapper around `cargo test --workspace`.
- **2026-04-15** — Phase 2 leaf ports landed in `prism-core`:
  `interaction::{notification, activity, query}` and
  `kernel::config`. `interaction::query` is the deliberate rename
  of the legacy `view` subtree — every view is a
  `prism_builder::Component`, never a `ViewMode` enum, so only the
  filter / sort / group half is ported; `SavedView` / `LiveView`
  are intentionally dropped. `kernel::config` ports the layered
  `ConfigRegistry` + `ConfigModel` + `FeatureFlags` trio with the
  17 built-in settings and the JSON Schema subset validator; the
  `pattern` field was cut since no built-in setting needs it and
  pulling regex into a leaf module costs more than the feature is
  worth. The `ConfigStore` trait intentionally omits `subscribe` —
  file watchers / IPC callbacks belong in the host crate, not in
  `prism-core`. `ConfigModel` dispatches change events *after*
  releasing the inner borrow so listener callbacks can read or
  mutate config during dispatch. 655 unit tests across `prism-core`
  (up from 623), clippy clean.
- **2026-04-15** — Phase 1 closed on scaffolds. `prism-shell` grew a
  `telemetry::FirstPaint` slot that `Shell::from_state` starts before
  the `AppWindow` is built; `Shell::run` installs a Slint
  `set_rendering_notifier` hook that stamps the first `AfterRendering`
  frame into the telemetry and logs a one-shot
  `prism-shell: first-paint Nms` line. `prism-cli` grew a
  `watch::WatchLoop` scaffold that wraps `notify::RecommendedWatcher`
  behind a debounced `next_batch` / `try_next_batch` API — 3 unit
  tests (tempfile write, idle non-block, quiet-dir timeout) pin the
  contract. `notify` moved to a workspace pin so `prism-cli::watch`
  and `prism-daemon::watcher_module` track the same major.
- **2026-04-15** — Phase 1 closed end-to-end with a unified hot-reload
  path. `prism-shell` grew a `live-preview` cargo feature that enables
  `slint/live-preview`; when `prism dev shell` compiles with
  `SLINT_LIVE_PREVIEW=1` + `--features live-preview`, `slint-build`
  replaces the baked `AppWindow` codegen with a
  `LiveReloadingComponent` wrapper that parses `ui/app.slint` at
  runtime via `slint-interpreter` and reloads it whenever the file
  changes. The public `AppWindow` API is wire-compatible, so
  `Shell::sync_ui` keeps working untouched. The `.rs` half lives in
  a new `prism_cli::dev_loop::DevLoop` module — a rebuild-and-respawn
  supervisor that wraps `WatchLoop` + a tokio child, filters batches
  to `.rs` files, and kills + respawns the cargo child on every
  batch. `prism dev shell` dispatches single-target runs through
  `DevLoop` by default; `--no-hot-reload` drops the env var, the
  feature flag, and the respawn supervisor and falls back to a plain
  `cargo run` exec. `prism dev all` still propagates the live-preview
  cargo flags to its shell slot so the `.slint` half stays active,
  but the respawn half is inactive in multi-target mode (the
  `Supervisor` can't kill + respawn individual children). 60 unit
  tests across `prism-cli` (up from 49), including 6 new `dev_loop`
  tests covering empty-path rejection, stdout routing, shutdown
  interruption, `.rs` → respawn, `.md` → no-op, and the
  extension-filter helper. Workspace clippy clean under
  `-D warnings`.
- **2026-04-15** — Phase 2 split into 2a / 2b and **2a closed**.
  `prism-core` shipped every leaf subtree Phase 3 depends on:
  `foundation` (batch, clipboard, date, object_model, persistence,
  template, undo, vfs), `identity` (did, encryption, manifest, trust),
  `language` (syntax, expression, registry, document, forms, markdown,
  codegen + the `luau` contribution stub), `kernel::{store,
  state_machine::machine, config}`, and `interaction::{notification,
  activity, query}`. 655 unit tests, clippy clean under `-D warnings`.
  `language::syntax` (2231 LOC, 21 tests) and `language::expression`
  (1859 LOC, 29 tests) closed in the same cycle — the "🚧 in progress"
  labels in `prism-core/CLAUDE.md` were stale and have been flipped.
  Phase 2b is the residual ADR-002 scope: the `kernel` orchestration
  kit (`actor`, `automation`, `intelligence`, `plugin`,
  `plugin_bundles`, `builder`, `initializer`) that `PrismKernel` will
  compose, plus `network`, `domain`, and the `statig` rewrite of the
  xstate tool machine. None of 2b is on Phase 3's critical path, so it
  runs in parallel with the builder/shell port rather than gating it.
  The Luau parser (`language::luau::parse`) remains a stub until the
  full-moon Rust port in Phase 4; mlua execution already lives in
  `prism-daemon::modules::luau_module` so no parser is needed at
  runtime.
- **2026-04-15** — **Phase 2b closed** (modulo the `network` subtree,
  which is porting on a background track and gates nothing). The
  ADR-002 §Part C `kernel` orchestration kit landed end-to-end:
  `kernel::actor` (14 tests) — `ProcessQueue` + `ActorRuntime` trait +
  `CapabilityScope`; `kernel::intelligence` (11 tests) — split off
  the legacy `actor/` folder with `AiProviderRegistry` + three providers
  (`OllamaProvider` / `ExternalProvider` / `TestAiProvider`) behind an
  `AiHttpClient` trait, plus `ContextBuilder`; `kernel::automation`
  (24 tests) — trigger / condition / action engine with host-supplied
  `AutomationStore` + `ActionHandler`s, path-walking condition
  evaluator, `{{…}}` interpolation, cron-style `tick`; `kernel::plugin`
  (7 tests) — `PluginRegistry` over four `ContributionRegistry<T>`
  buckets; `kernel::plugin_bundles` (8 tests) — six built-in bundles
  (work / finance / crm / life / assets / platform) plus `flux_types`
  and `PluginInstallContext<'a>` fan-out; `kernel::builder` (22 tests)
  — `BuilderManager` + `AppProfile` + `BuildPlan` with `DryRunExecutor`
  / `CallbackExecutor` impls of the `BuildExecutor` trait, six built-in
  profiles, and `materialize_starter_app`; `kernel::initializer`
  (3 tests) — `KernelInitializer<TKernel>` generic trait with composite
  reverse-order disposer. `kernel::prism_kernel::PrismKernel` landed
  on top (10 tests) as a **narrowed** port of the legacy 2431-line
  `studio-kernel.ts` — it composes only framework-free Layer-1
  primitives (`ObjectRegistry`, `PluginRegistry`, `ConfigRegistry` +
  `ConfigModel` + `FeatureFlags`, `NotificationStore`, `ActivityStore`,
  `BuilderManager`, `ProcessQueue`, `AiProviderRegistry`) and is
  single-threaded by design (`ConfigModel` / `FeatureFlags` /
  `ActivityStore` are `!Send`, matching the TS main-thread invariant).
  Intentional omissions: `CollectionStore` (gated on the `crdt` feature,
  host-owned), `AutomationEngine` (requires host-supplied store +
  handlers), domain engines (stateless / per-document), and `network::*`
  (composes differently per host). Puck / Lens / React / Tauri wiring
  stays out of `prism-core` and lives in `prism-shell` + the studio
  host crate. The `domain` subtree landed alongside: `domain::flux`
  (38 tests), `domain::timeline` (67 tests), `domain::graph_analysis`
  (30 tests). `kernel::state_machine::tool` landed as the `statig`
  rewrite of the xstate tool-mode FSM. `cargo test -p prism-core --lib`
  now runs 897+ tests green under `-D warnings`.
- **2026-04-18** — **Relay 17-module feature surface closed.** The full
  relay feature set from the legacy Hono JSX relay (commit `8426588`) is
  now ported into Rust: 17 composable `RelayModule` impls in
  `prism-core::network::relay::modules` (blind_mailbox, relay_router,
  timestamper, blind_ping, capability_tokens, webhooks, sovereign_portals,
  signaling, collection_host, vault_host, hashcash, peer_trust, escrow,
  federation, password_auth, acme, portal_templates), wired through
  `RelayBuilder` → `RelayInstance` → `FullRelayState` in `prism-relay`.
  HTTP surface: ~80 API routes across 25 route modules under `/api/*`,
  admin dashboard at `/admin`, Prometheus metrics at `/metrics`, ACME
  challenges at `/.well-known/acme-challenge/`. WebSocket relay protocol
  at `/ws` (auth, envelope, collect, ping, CRDT sync, hashcash,
  presence). Tower middleware: CSRF header check, token-bucket rate
  limiting, body size limit, request metrics. Multi-source config
  (`RelayConfig` with `Server` / `P2p` / `Dev` modes + env var
  overrides). JSON file persistence via `FileStore`. `prism-relayd` bin
  accepts `--mode` and `--relay-did` CLI flags. 1033 `prism-core` tests +
  29 `prism-relay` tests (21 unit + 8 integration), zero clippy warnings.
  Phase 2b marked closed (residual stubs: `network::{discovery, session,
  server}` — not consumed by `prism-relay`).
- **2026-04-20** — **ADR-006 source-first architecture implemented.** The
  builder flipped from `BuilderDocument`-as-truth to `.slint`-source-as-truth
  (ADR-006). `LiveDocument` in `prism-builder::live` is now source-first:
  constructors `from_source` (primary) and `from_document` (migration/import),
  `BuilderDocument` derived on demand via `derive_document_from_source`.
  GUI mutations are surgical source text edits (`edit_prop_in_source`,
  `insert_node_in_source`, `remove_node_from_source`, `move_node_in_source`)
  located via `SourceMap` + `PropSpan`. New `source_parse.rs` module
  implements the roundtrip: `derive_document_from_source` + `parse_slint_value`
  / `format_slint_value`. `Page.source` is the persisted field;
  `Page.document` is derived and `skip_serializing`. Shell undo/redo
  snapshots source text (`SourceSnapshot`). All 15+ mutation callbacks in
  `prism-shell/src/app.rs` rewritten; 10 dead document-manipulation helpers
  and 8 dead tests removed. 1975+ workspace tests green, zero clippy warnings.
