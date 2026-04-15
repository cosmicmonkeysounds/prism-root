# Slint Migration Plan

> Migrating Prism's UI layer off React / TypeScript / Tailwind onto
> [Slint](https://github.com/slint-ui/slint) so that Studio, the page
> builder, and every Lens can run cross-platform from a single
> codebase — while still shipping a first-class web target via WASM.

**Status:** **Phase 0 closed 2026-04-15.** Phases 1–2 now in flight
on branch `rust`. Rust workspace scaffolded; `prism-daemon`,
`prism-cli`, and `prism-relay` shipping; `prism-core`,
`prism-builder`, `prism-shell` under active port. `prism-studio/src-tauri`
is the canonical desktop shell — a ~30-line launcher that spawns the
daemon sidecar and hands control to `prism_shell::Shell`, which runs
Slint's native winit + femtovg backend (see §4.5). Per-package
`CLAUDE.md` files carry live status.

**Owner:** TBD
**Created:** 2026-04-14
**Last updated:** 2026-04-15

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
| `prism-core` | 🚧 porting | Phase 2 — leaf-first port of `@prism/core` to Rust. Kernel store (`kernel::store`) and state machine are in; language surfaces and foundations landing by ADR-002 order. |
| `prism-builder` | 🚧 porting | Phase 3 — `Component` trait + registry + HTML SSR path live; Slint render walker lands on top of `slint-interpreter`. |
| `prism-shell` | 🚧 porting | Phase 3 — single `Shell` wrapper around a `Store<AppState>` and a Slint `AppWindow` compiled from `ui/app.slint`. Native bin + `wasm32-unknown-unknown` cdylib both in scope. |
| `prism-studio/src-tauri` | ✅ thin launcher | ~30-line `main.rs`: spawn daemon sidecar, build `Shell`, call `Shell::run()`. Slint owns the event loop. |
| Studio panels (~40+) | ⏳ pending | Each panel → one `ui/*.slint` component + a Rust struct implementing `Panel`. Phase 3. |
| Legacy Puck component impls (~30+) | ⏳ pending | Re-register against `prism_builder::ComponentRegistry`; share field factories per ADR-002. |
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

## 7. Hot-reload constraints

Slint's `.slint` files are compiled at build time by `slint-build`,
so "hot-reload" for hand-written panels today means re-running
`cargo run`. Two follow-ons bring genuine hot-reload online:

1. **`slint-interpreter` live compilation** (Phase 3): the builder
   panel re-parses `.slint` sources on every change and re-applies
   them to the existing `Store<AppState>` snapshot. Because the
   store is serde-backed, the app state survives the reload.
2. **`subsecond`** (Phase 4): a rustc/lld integration that swaps
   Rust code without tearing down the process. Deferred because
   it's a large dependency and Phase 3 can ship without it.

Every reloadable structure must stay serde-serializable and live
under a single root type. The §6.1 store is the enforcement point.

## 8. `prism-builder` — Puck replacement on Slint

`prism-builder` owns the component-type registry, the
`BuilderDocument` tree, the legacy Puck-JSON reader, and the HTML
SSR render path. The Slint walker lands here in Phase 3.

### 8.1 Component trait — two render targets

Every block implements `prism_builder::Component`:

```rust
pub trait Component: Send + Sync {
    fn id(&self) -> &ComponentId;
    fn schema(&self) -> serde_json::Value;
    fn render_slint(&self, ctx: &RenderContext<'_>, props: &Value) -> Value { /* stub */ }
    fn render_html(&self, ctx: &RenderHtmlContext<'_>, props: &Value,
                   children: &[Node], out: &mut Html) -> Result<(), RenderError> { /* default */ }
}
```

- `render_html` is **live today** — drives the Sovereign Portal
  SSR path. Default impl emits `<div data-component="id">` +
  recursive children.
- `render_slint` is **stubbed until Phase 3** — Phase 3 will
  materialise the returned value tree through `slint-interpreter`
  to produce a live Slint component instance.

### 8.2 Registry as the single DI surface

New block types go through `ComponentRegistry::register(Arc<dyn
Component>)`. No side registries, no per-component singletons,
no hand-wired `Node` factories. Field factories (the reusable
property-panel primitives) will land in `registry` alongside
`register` once Phase 3 brings the property panel over.

### 8.3 Puck-JSON reader is permanent

`puck_json.rs` reads legacy Puck `{ type, props, children }`
documents forever. New documents are written in the
`BuilderDocument` schema; old documents keep booting without a
migration step.

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
| 1 | `prism-cli` web pipeline retarget, first-paint telemetry, notify-driven watch loop scaffold | 🚧 in flight |
| 2 | `prism-core` leaf-first port (foundations, language subtree, kernel) | 🚧 in flight |
| 3 | `prism-builder` Slint walker via `slint-interpreter`, Studio panel ports, property panel + field factories | ⏳ pending |
| 4 | Luau contribution runtime, advanced lenses, `subsecond` hot-reload | ⏳ pending |
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
