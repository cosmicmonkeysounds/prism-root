# Clay Migration Plan

> Migrating Prism's UI layer off React / TypeScript / Tailwind onto
> [Clay](https://github.com/nicbarker/clay) so that Studio, the page
> builder, and every Lens can run cross-platform from a single
> codebase — while still shipping a first-class web target via WASM.

**Status:** Phase 0–2 in flight on branch `rust` (2026-04-15). Rust
workspace scaffolded; `prism-daemon` and `prism-cli` shipping;
`prism-core`, `prism-builder`, `prism-shell` under active port;
`prism-studio/src-tauri` is the canonical desktop shell (bare `tao` +
`wgpu` + `prism-shell`, no Tauri). §4.5 Option C confirmed 2026-04-15
— see decision log. Per-package `CLAUDE.md` files carry live status.
The narrative in §0 / §2 below was written before the rewrite landed —
read it as historical framing, not current state.
**Owner:** TBD
**Created:** 2026-04-14
**Last updated:** 2026-04-15

---

## 0. Why

- Today: Prism = React 19 SPA (Vite) + Tailwind 4 + Puck + a long
  tail of React-only libraries (xyflow, react-leaflet, recharts,
  react-moveable, kbar, react-markdown, react-resizable-panels). The
  daemon is Rust (Tauri). Mobile is Capacitor (web view).
- The page builder we are shipping (Puck-based + custom widgets) is
  the heart of Prism — and it is locked to React DOM. That means
  every "app" a user builds in Prism is also locked to React DOM.
- Goal: a single immediate-mode UI tree (Clay) that renders
  identically on web (WASM + HTML/Canvas renderer), desktop (Tauri /
  native), and mobile, so a user-built Prism app is genuinely
  portable.
- Web stays the primary target. Web ships as WASM, not as a fallback.

## 1. Non-goals

- Not rewriting the daemon. `prism-daemon` is already Rust and stays.
- Not changing the data layer. Loro CRDT remains source of truth.
- Not changing the IPC contract with Tauri. Frontend↔daemon stays
  Tauri commands.
- Not migrating the Relay package. Relays render HTML for unauthenticated visitors and have no client UI worth porting. Relay stays as-is.

## 2. Scope inventory (what has to move)

Counts pulled from the pre-Clay tree (see git log for migration
progress — the Rust crates below now exist):

| Surface | Files | Notes |
|---|---|---|
| `.tsx` files (all packages) | 127 | Every one is a React render target. |
| `.ts` files (all packages) | 638 | Most are pure logic (state, parsing, CRDT bridge) and survive the move. |
| Studio panels | ~40+ in `prism-studio/src/panels/` | Each panel = one Clay layout function. |
| Studio renderers | ~30+ in `prism-studio/src/components/` | Puck component implementations; reimplement against Clay. |
| Puck (page builder) | `@measured/puck@0.20.2` | **No replacement** — must build a Clay-native equivalent. See §8. |
| `prism-admin-kit` | React UI | Rebuild on Clay. |
| `prism-puck-playground` | React harness | Becomes the Clay-builder playground or is deleted. |
| `prism-relay` | Hono JSX SSR | **Out of scope.** We'll have to re-write this (Sovereign Portal system), but not today. |
| `prism-core` | TS, mostly logic | Logic survives; the host language change determines whether it ports to Rust or stays as a TS sibling reachable via JS interop. See §3. |

## 3. Host language: Rust (decided)

Clay is a single-header C library. The host language must:

1. Compile to WASM for web.
2. Produce native binaries for desktop / mobile.
3. Have a Loro binding (so we don't lose the CRDT layer).
4. Already exist in this repo so we don't fork the toolchain.

**Decision: Rust.** Locked 2026-04-14.

- `prism-daemon` is already Rust → one toolchain across the stack.
- `loro` has a first-class Rust crate (we currently use the JS
  `loro-crdt` wrapper around it).
- **Clay binding pinned to [`clay-ui-rs/clay`](https://github.com/clay-ui-rs/clay)**.
  It tracks upstream Clay closely and exposes Clay's macro-style
  layout DSL via Rust macros / builders. Locked 2026-04-14.
- Rust → WASM via `wasm-bindgen` + `wasm-pack` is already in use
  inside `prism-core/src/language/luau` (there is a `pkg/`
  artifact), so the build infrastructure exists.

**Alternatives considered and rejected:**

- **Zig** — Clay's reference language. Cleanest Clay ergonomics but
  introduces a third toolchain and has no Loro binding.
- **C++** — works, but no team affinity and no upside over Rust.
- **Pure C with Emscripten** — compiles, but we lose every existing
  TS abstraction and gain nothing.
- **AssemblyScript / TS-to-WASM** — would let us keep more of
  `prism-core`, but Clay bindings don't exist and we'd be alone
  maintaining them.

**Locked 2026-04-14: `prism-core` ports fully to Rust too.**
Keeping it as TS behind a JS-interop boundary would mean a
permanent FFI tax at every panel, every store, every subsystem
call site, forever. Cleaner to do the painful one-time port and
live without the boundary. The existing Vitest suites stay alive
only as parity oracles, deleted module-by-module as the Rust
equivalent ships.

This is the largest single chunk of work in the migration after
the page builder. ~638 `.ts` files in `prism-core` will be ported.
Most are pure logic (CRDT bridges, parsers, registries, state
machines) so the port is mechanical, but the volume is real.

## 4. Target topology (after migration)

```
┌─────────────────────────────────────────────────┐
│   prism-app  (Rust crate, one source of truth)  │
│   ├── clay-ui-rs layout tree                    │
│   ├── lens / panel / page-builder logic         │
│   └── loro (rust crate) — CRDT state            │
└─────────────────────────────────────────────────┘
        │                │                │
        ▼                ▼                ▼
   wasm-bindgen      tao window       cargo-mobile2
   + clay HTML/      + wgpu render    + winit
   canvas            (no Tauri,       + wgpu render
   renderer          no wry)
        │                │                │
        ▼                ▼                ▼
   Web (trunk         Desktop          iOS / Android
   serves .wasm       (single Rust     (single Rust
   into <canvas>)     binary;          binary;
                      cargo-packager   cargo-mobile2
                      for installers)  builds)
```

Web target stays first-class: a thin `index.html` + JS shim boots
the WASM module and forwards DOM events into Clay. The Clay HTML
renderer (or a custom Canvas2D / WebGL renderer) emits the actual
pixels.

Native targets ship a single Rust binary with **no webview and no
Tauri**. The desktop shell uses `tao` directly for windowing and
`wgpu` for rendering; `cargo-packager` + `self_update` handle
packaging and auto-update. See §4.5 for the full rationale.

## 4.5 Native shell strategy

The post-migration client is a single Rust binary on every
platform. That makes Tauri's webview pointless — bundling a JS
engine to host a Rust app is the very thing the migration is
supposed to fix. But Tauri's *non-webview* surface area
(packaging, updater, code signing, plugin system, sidecar
process management) is still real value we don't want to
reimplement from scratch.

**Decision: Option C — bare `tao` + `wgpu`, no Tauri.**
**Supersedes: Option B, retired 2026-04-15** (raw pointer input dropped
at the `wry` wrapper — see decision log).

Locked 2026-04-15.

#### Per-target topology

| Target | Window | Renderer | Daemon |
|---|---|---|---|
| **Desktop** (mac / win / linux) | `tao` direct (no Tauri) | Clay → `wgpu` | `prism-daemon` as a sibling process; IPC over `interprocess` (unix sockets / named pipes) |
| **Mobile** (iOS / Android) | `cargo-mobile2` + `winit` (iOS UIKit / Android Activity host) | Clay → `wgpu` | Embedded in the shell binary on a tokio runtime — iOS and Android process models don't permit real sidecars |
| **Web** | Browser canvas | Clay HTML or custom Canvas2D / WebGL renderer | Remote daemon over WebSocket (existing relay path) |

#### Why Option C (and why Option B was retired)

Option B was initially confirmed viable by Phase 0 spike #5
(2026-04-14): `tauri::window::WindowBuilder` (behind the `unstable`
feature) creates a `tao::Window` with no webview, and `wry` stays in
the dep graph only as `tauri-runtime-wry` — the sole `Runtime` impl
Tauri ships. Raw pointer input is what killed it: `wry`'s event
wrapper drops stylus pressure and pointer-ID fields that Clay's input
model needs for the page builder (drag, resize, multi-touch). Patching
`wry` upstream is nontrivial; the team is not prioritizing it.

Option C costs us Tauri's batteries (one-command `tauri build`, the
auto-updater plugin, sidecar lifecycle). We pay that cost with
standalone crates instead:

- **`cargo-packager`** — signed `.dmg` / `.msi` / `.deb` / `.AppImage`
  from one command. Comparable effort to `tauri build`.
- **`self_update`** — auto-update from a static release feed.
- **`tray-icon`**, **`notify-rust`**, **`rfd`**, **`arboard`**,
  **`keyring`** — system tray, OS notifications, file dialogs,
  clipboard, credential store.
- Daemon process management is hand-rolled (spawn + supervise + kill)
  instead of the Tauri sidecar feature — the IPC spike already proved
  the `interprocess` transport; only the lifecycle wrapper is new work.
- Mobile: `cargo-mobile2` + `winit` (iOS UIKit / Android Activity),
  same `wgpu` renderer and Clay layout code.

The outermost shell layer changes. Nothing below it does.

#### Daemon process model

Today `prism-daemon` is a separate Rust process Tauri spawns and
talks to over `invoke()`. Post-migration (Tauri entirely removed):

- **Desktop** — daemon is a sibling process spawned directly by the
  Studio shell (`std::process::Command` or equivalent). The shell owns
  spawn / supervise / kill — no Tauri sidecar feature. Communication
  is plain Rust↔Rust IPC over `interprocess` (unix domain sockets on
  mac/linux, named pipes on Windows), length-prefixed `postcard` frames.
  No JS boundary, no `invoke()`.
- **Mobile** — separate processes are not viable. iOS denies
  persistent background processes; Android services are
  constrained and platform-specific. The daemon code runs as a
  tokio-based subsystem inside the shell binary. The IPC layer
  becomes plain function calls / `crossbeam-channel` over the
  same trait surface as desktop.
- **Web** — the daemon is remote (a relay or a self-hosted
  local server) accessed over WebSocket. Existing path; no
  change.

The daemon trait stays identical across all three transports.
Panels and lenses don't know whether they're talking to a local
sibling process, an in-process subsystem, or a remote relay — only the
transport changes. This is critical: it's what makes the
write-once-run-anywhere story honest.

## 5. Dependency triage (what's coming out)

Every load-bearing React / DOM library has to be either reimplemented
on Clay, embedded as a hybrid escape hatch, or deleted. Decide
per-library before Phase 2 starts.

| Today | Verdict | Replacement strategy |
|---|---|---|
| **React 19 / react-dom** | Delete | Replaced by Clay layout pass. |
| **Tailwind 4** | Delete | Clay has its own layout DSL; design tokens become Rust constants under `prism-core::design-tokens`. |
| **@measured/puck** | Reimplement | Build `prism-builder` (Rust) — see §8. This is the largest single piece of work in the migration. |
| **CodeMirror 6** | Reimplement *or* hybrid | CM6 is contenteditable-DOM-bound; cannot run inside Clay. Two options: (a) write a Clay-native text editor on top of `ropey` + a tokenizer (large, but unblocks native targets); (b) keep CM6 as a DOM overlay positioned by Clay coordinates on web only, and accept that desktop/mobile need option (a) eventually. **Recommend (b) for Phase 1, (a) for Phase 4.** |
| **@xyflow/react** (graph editor) | Reimplement | Node-graph rendering on Clay is straightforward (rects + bezier draw commands via `lyon`). Port the graph-panel logic, drop xyflow. |
| **react-leaflet / leaflet** | Hybrid | Maps need tile servers + projection math. Either embed Leaflet via DOM overlay (web only) or pull in a Rust map renderer (`maplibre-rs`). Leaflet survives Phase 1 as a hybrid; native build needs maplibre. |
| **recharts** | Reimplement | Chart rendering on Clay = a few hundred lines of draw commands per chart type. Already partially abstracted in `chart-data.ts`. |
| **react-moveable / react-selecto / @scena/react-guides** | Reimplement | Selection + drag handles are immediate-mode-friendly; rebuild on Clay's input model. |
| **react-resizable-panels** | Reimplement | Trivial in Clay (split layouts are first-class). |
| **react-markdown / remark-gfm** | Replace | Use `pulldown-cmark` (Rust) → emit Clay text/heading/list nodes. |
| **kbar** (command palette) | Reimplement | One panel; small. |
| **zustand** | Delete | State lives in Rust. The store layer in `prism-core` needs an idiomatic Rust equivalent. |
| **loro-crdt (JS)** | Swap to `loro` (Rust crate) | Free upgrade — same CRDT, native bindings. |
| **@tauri-apps/api** (JS) | Delete | No JS frontend; no Tauri. Daemon IPC uses `interprocess` directly. |
| **@tauri-apps/cli** | Delete | Tauri removed entirely. `cargo-packager` drives installers instead. |
| **@capacitor/\*** | Delete | Replaced by `cargo-mobile2` + `winit` (§4.5 Option C). |

## 6. Rust replacements for every JS/TS dep

Two views of the same information. The first is a 1:1 mapping
from every JS/TS package we depend on today to its Rust
replacement. The second is the same libraries grouped by
purpose with a bit more colour. Float versions for now; lock
them at Phase 0 exit.

### 6.1 Direct dependency mapping

Confidence column: **Very high** = drop-in or trivial.
**High** = work, but well-understood and de-risked.
**Medium** = nontrivial design or one-off engineering.
**Low** = open research problem, or no Rust analog exists.

| Today's dep | Used in | Rust replacement | Confidence |
|---|---|---|---|
| `react`, `react-dom` | all UI packages | `clay-ui-rs/clay` | High |
| `tailwindcss`, `@tailwindcss/vite` | studio, playground | Clay layout DSL + Rust design-token constants | High |
| `@measured/puck` | studio, admin-kit, playground, core | New `prism-builder` Rust crate (§8) | High — work, not risk |
| `zustand` | core, studio, admin-kit | Hand-rolled `AppState` + reducer + subscription bus (no framework) | High |
| `xstate` | core | [`statig`](https://github.com/mdeloof/statig) — hierarchical statecharts, ergonomic Rust derive macros | High |
| `loro-crdt` | core, studio, playground | [`loro`](https://crates.io/crates/loro) — same author, same wire format, Rust-native | Very high |
| `@codemirror/{view,state,language,commands,autocomplete,lint,search,lang-javascript,lang-json}`, `codemirror` | core | **Phase 1:** DOM-overlay hybrid via `web-sys`. **Phase 4:** Clay-native editor on `ropey` + `tree-sitter` + `@prism/core/syntax` Scanner | Medium — Phase 4 is its own project |
| `@lezer/common`, `@lezer/highlight` | core | `tree-sitter` Rust binding + custom highlight pipeline | High |
| `react-markdown`, `remark-gfm` | core, studio | `pulldown-cmark` (CommonMark + GFM extensions, no allocations) | Very high |
| `@xyflow/react` | core, studio | Custom node graph on Clay: `petgraph` for the data, `lyon` for the bezier curves, hand-rolled hit testing | High — well-understood domain |
| `elkjs` | core | [`layout-rs`](https://crates.io/crates/layout-rs) (Sugiyama layered layout) or hand-rolled on `petgraph` if elkjs's exact algorithm matters | Medium |
| `kbar` | core, studio | Reimplement directly on Clay (small) | Very high |
| `recharts` | studio, admin-kit, playground | [`plotters`](https://crates.io/crates/plotters) with a custom Clay drawing backend (plotters supports pluggable backends) | High |
| `react-leaflet`, `leaflet` | studio, playground | **Phase 1:** DOM hybrid via `web-sys`. **Phase 5:** [`maplibre-rs`](https://github.com/maplibre/maplibre-rs) — vector tiles, GPU accelerated, works on web + native | High |
| `react-moveable`, `react-selecto`, `@scena/react-guides` | studio | Custom Rust on Clay's input layer (selection, drag, resize, snap guides). No off-the-shelf Rust lib; the math is well-understood | High |
| `react-resizable-panels` | studio | Native Clay split layouts | Very high |
| `luau-web` | core | [`mlua`](https://crates.io/crates/mlua) with the `luau` feature flag — compiles the actual Luau runtime into the Rust binary | High |
| `@opendaw/{lib-dom,lib-dsp,lib-std,studio-adapters,studio-boxes,studio-core,studio-sdk}` | core | **The hardest single port.** No Rust port of OpenDAW exists. Reference replacements: `cpal` (audio I/O), `symphonia` (decoders), `fundsp` (DSP graph), `dasp` (DSP primitives), `creek` (disk streaming), `rubato` (sample-rate conversion), `rtrb` (realtime ringbuffer). For DAW engine reference: [`meadowlark-engine`](https://github.com/MeadowlarkDAW) (active Rust DAW project — closest analog to OpenDAW). The timeline / NLE proper has to be built. | **Low** — needs its own design phase before Phase 1 |
| `@capacitor/core`, `@capacitor/android`, `@capacitor/ios`, `@capacitor/cli` | studio | **`cargo-mobile2`** + `winit` (iOS UIKit / Android Activity host). Renders Clay → `wgpu` directly. | Medium — mobile shells via cargo-mobile2 |
| `@tauri-apps/api` (JS) | studio | Delete (no JS frontend). Daemon IPC uses **`interprocess`** | Very high |
| `@tauri-apps/cli` | studio | **Delete.** No longer needed — Tauri removed entirely. | Very high |
| *new:* `tao` (Rust crate) | new in `prism-shell` | Cross-platform windowing and event loop. Used **directly** (not via Tauri). | High |
| *new:* `wgpu` (Rust crate) | new in `prism-shell` | GPU-accelerated rendering of Clay's draw commands on every platform (Metal / Vulkan / D3D12 / WebGPU / WebGL2 fallback) | High |
| *new:* `interprocess` (Rust crate) | new in `prism-shell` | Cross-platform local IPC (unix sockets / named pipes) for the desktop shell ↔ daemon sibling channel | Very high |
| *new:* `cargo-packager` | new in build | Signed `.dmg` / `.msi` / `.deb` / `.AppImage` — replaces `tauri build` | High |
| *new:* `self_update` | new in `prism-studio` | Application auto-update from a static release feed — replaces Tauri's updater plugin | High |
| `@hono/node-server`, `@hono/node-ws`, `hono` | relay | `axum` (Tokio-team Rust web framework, idiomatic) — *if* Relay ever migrates. Out of scope today | n/a — Relay stays Hono JSX SSR (pending its own rewrite) |
| `vitest` | core, admin-kit | `cargo test` + `insta` (snapshots) + `proptest` (property tests) | Very high |
| `@playwright/test` | studio, relay | **Keep.** E2E still runs against a real browser even when the client is WASM. No reason to swap | Very high |
| `vite`, `@vitejs/plugin-react`, `vite-plugin-wasm`, `vite-plugin-top-level-await`, `vite-plugin-singlefile` | studio, playground | [`trunk`](https://trunkrs.dev) for Rust+WASM bundling and dev server. Vite can stay as the outer dev server during the migration if it helps | High |
| `@types/{react,react-dom,leaflet}` | dev | Delete | Very high |
| `pnpm`, `turbo` | repo root | `cargo workspace` for the Rust half. Turbo can stay as the umbrella runner wrapping `cargo` and `pnpm` until the JS half is gone | High |
| `typescript` | all | Delete (replaced by Rust) | Very high |

### 6.2 Library catalog grouped by purpose

#### Core UI

- **[`clay-ui-rs/clay`](https://github.com/clay-ui-rs/clay)** —
  Rust binding for Clay. **Pinned.** Resolves spike #1 from §11.
- **`cosmic-text`** — text shaping + layout. Clay handles box
  layout but punts on glyph shaping; cosmic-text fills the gap and
  is what every serious Rust UI lib (Iced, Floem, Linebender
  Xilem) uses today.
- **`tiny-skia`** — CPU 2D vector rasterizer; fallback path if
  `wgpu` is unavailable or overkill on a given target.
- **`lyon`** — bezier tessellation for the graph editor curves
  and any custom chart paths.
- **`image`** + **`resvg`** — raster + SVG decoding for asset
  rendering inside components.
- **`palette`** — color space math for design tokens (HSL/OKLCH
  conversions, contrast checks).
- **`plotters`** — chart rendering. Has a pluggable backend
  trait; we write a Clay backend once and get every chart type
  for free. Replaces `recharts`.
- **`wgpu`** — primary GPU renderer for Clay draw commands on
  *every* native target from Phase 1 onward. Backs Metal /
  Vulkan / D3D12 / WebGPU. Promoted out of Phase 5 by the §4.5
  decision.

#### Web / WASM glue

- **`wasm-bindgen`** + **`wasm-pack`** — Rust↔JS bridge and build
  tool. Already in use under `prism-core/src/language/luau`.
- **`web-sys`** / **`js-sys`** — typed bindings to browser APIs
  for the DOM-overlay escape hatches (CodeMirror, Leaflet) and
  any browser-API glue the web target needs.
- **`gloo-events`** / **`gloo-storage`** — ergonomic helpers over
  `web-sys`.
- **`console_error_panic_hook`** — readable panics in DevTools.
- **`trunk`** — Rust+WASM bundler with autoreload. Likely
  replaces or fronts the Vite pipeline for the Clay shell.

#### State + data

- **`loro`** (Rust crate) — CRDT, native. Drops `loro-crdt` JS.
- **`serde`** + **`serde_json`** — Puck JSON interop and on-disk
  doc format.
- **`postcard`** — compact binary serialization for hot-reload
  state snapshots and IPC.
- **`slotmap`** / **`generational-arena`** — stable IDs for
  component tree nodes without lifetime gymnastics.
- **`imbl`** — immutable persistent data structures; fast undo /
  redo without cloning the world.
- **`indexmap`** — order-preserving map for the property panel
  field ordering.
- **`crossbeam-channel`** — actor message channels that survive
  hot reload (see §7).
- **`statig`** — hierarchical statecharts, the closest Rust
  analog to xstate. Replaces our `xstate` usage in
  `prism-core::kernel::state-machine`.
- **`petgraph`** — graph data structures + traversal algorithms.
  Backs the node-graph editor and any dependency-graph work
  inside `prism-core::domain::graph-analysis`.
- **`layout-rs`** — Sugiyama layered graph layout. Replaces
  `elkjs` for "draw me a DAG nicely" cases. If elkjs's exact
  algorithm matters somewhere, we hand-roll on `petgraph`.

#### Text editing

- **`ropey`** — rope buffer for the eventual native text editor
  (Phase 4).
- **`pulldown-cmark`** — markdown → AST → Clay nodes. Replaces
  `react-markdown` / `remark-gfm`.
- **`tree-sitter`** (Rust bindings) — paired with
  `@prism/core/syntax`'s Scanner for highlighting, per
  `feedback_prism_syntax_parsing.md`. Also replaces
  `@lezer/common` + `@lezer/highlight`.

#### Scripting

- **`mlua`** with the **`luau`** feature flag — embeds the actual
  Luau runtime into the Rust binary, no separate WASM blob.
  Replaces `luau-web`. The mlua API is mature and covers our
  current Luau usage in `prism-core::language::luau`.

#### Builder ergonomics

- **`notify`** — filesystem watcher; the engine of the hot-reload
  loop and any "live edit from disk" workflows.
- **`arboard`** — cross-platform clipboard.
- **`rfd`** — native file dialogs.
- **`keyring`** — OS credential storage.
- **`egui`** — *not* a product dependency, but a viable
  developer-tools / inspector overlay during the migration. Cheap
  to add, cheap to delete. Decide in Phase 0.

#### Hot reload (see §7)

- **`subsecond`** — Dioxus team's general-purpose Rust hot
  patching. Strongest fit for our use case if it pans out under
  WASM.
- **`hot-lib-reloader`** — older, battle-tested cdylib reloader
  for native dev loops.
- **`cargo-watch`** — file-watch + rebuild on save (the dumb but
  reliable baseline).

#### Test + quality

- **`insta`** — snapshot tests for Clay layout output (assert
  that a panel serializes to the expected node tree).
- **`proptest`** — property tests around CRDT operations.
- **`criterion`** — perf benchmarks for layout + render.

#### Mapping (Phase 5)

- **`maplibre-rs`** — vector tile rendering. Replaces Leaflet on
  native targets.

#### Media / DAW (replaces the OpenDAW JS stack)

This is the hardest single replacement in the migration. OpenDAW
has no Rust port, so we assemble a stack from the broader Rust
audio ecosystem and build the timeline / NLE on top.

- **`cpal`** — cross-platform audio I/O (the foundation).
- **`symphonia`** — pure-Rust decoders (mp3, flac, wav, ogg,
  aac, opus).
- **`kira`** — high-level playback engine, mixer, basic effects.
- **`fundsp`** — graph-based DSP, idiomatic Rust DSL.
- **`dasp`** — DSP primitives (sample types, conversions,
  windowing).
- **`creek`** — disk-streaming reader/writer for long audio
  files (the OpenDAW lib-dom equivalent for file I/O on the
  audio thread).
- **`rubato`** — high-quality sample-rate conversion.
- **`rtrb`** — realtime-safe ringbuffer for audio thread ↔ UI
  thread comms.
- **`meadowlark-engine`** — *reference only*. The closest active
  Rust DAW project to OpenDAW; worth reading their source for
  the timeline / NLE / clip-launching design before we build ours.

#### Native shell (per §4.5 Option C)

The desktop / mobile app shell. Tauri is not used. No `tauri`, no
`wry`, no `tauri-runtime-wry` anywhere in the tree.

- **`tao`** — cross-platform windowing and event-loop (a fork of
  `winit` with extra platform integration). Used directly — not
  routed through Tauri.
- **`interprocess`** — cross-platform local IPC (unix domain
  sockets on mac/linux, named pipes on Windows). The transport
  for desktop shell ↔ `prism-daemon` sibling process communication.
- **`tokio`** — async runtime. Runs the embedded daemon
  subsystem on mobile (where sibling processes are not viable)
  and the IPC client on desktop.
- **`cargo-packager`** — signed `.dmg` / `.msi` / `.deb` /
  `.AppImage` / `.app` from one command. Replaces `tauri build`.
- **`self_update`** — application auto-update from a static
  release feed. Replaces Tauri's updater plugin.
- **`tray-icon`**, **`notify-rust`** — system tray and OS
  notifications. Wired explicitly rather than via Tauri plugins.
- **`cargo-mobile2`** — mobile cross-compile + Xcode/Gradle
  scaffolding for iOS/Android. Replaces Tauri Mobile.
- **`winit`** — mobile windowing host (iOS UIKit / Android
  Activity). Used via `cargo-mobile2`.
- **`rfd`**, **`arboard`**, **`keyring`** — native file dialogs,
  clipboard, OS credential storage (same as before, but now
  wired explicitly rather than via Tauri plugins).

#### Server-side (out of scope; listed for completeness)

- **`axum`** — Rust web framework (Tokio team). What we'd reach
  for if Relay ever migrates off Hono JSX SSR. Not in scope.

## 7. Hot reloading strategy

Hot reload is non-negotiable for productive UI iteration. The bar
is "change a Clay layout function, see the result in under 2
seconds without losing state."

#### Three layers of reload

The builder has three things you might want to reload, and they
need different mechanisms.

| What changes | Mechanism | State preserved? |
|---|---|---|
| **Component schema / JSON** (a Puck doc, a saved page) | `notify` watches the file → re-deserialize → re-render. Pure data. | Yes, trivially. |
| **Layout / style code** (a Rust function that emits Clay nodes) | `trunk serve --watch` for web; `hot-lib-reloader` or `subsecond` for native. | Yes if `AppState` is snapshotted across reload. |
| **Component implementations** (new Rust types added to the registry) | Full rebuild + state-snapshot restore. | Yes via serde snapshot. |

This three-tier model also tells us how to design the component
registry: the more components live as *data* rather than *code*,
the cheaper iteration is. Lean on the data-driven side wherever a
component can be expressed as composition over primitives.

#### Web target (primary)

- **`trunk serve --watch`** rebuilds the WASM module on file
  change and reloads the browser. Baseline.
- **State preservation** across reloads: on `beforeunload`,
  serialize `AppState` (serde + postcard) into `localStorage`. On
  boot, restore. Loro doc state already round-trips through the
  daemon, so docs survive for free.
- **Sub-second loop**: evaluate **`subsecond`** as a Phase 0
  spike. If it works under WASM, it patches function bodies in a
  running module without a full page reload — the closest thing
  to React fast-refresh that the Rust ecosystem has.

#### Native target (desktop)

- **`cargo watch -x run`** is the dumb baseline (rebuild +
  restart).
- **`hot-lib-reloader`** is the productive inner loop: extract
  the UI code into a `cdylib` and reload it on change while the
  host binary holds `AppState`. Same pattern Bevy / Fyrox / a few
  Linebender experiments use.
- **`subsecond`** is the future-state replacement; evaluate
  alongside the WASM spike.

#### Design constraints the hot-reload story imposes

- **One root state struct.** Everything reloadable lives behind
  a single `AppState` so snapshot/restore is one serde call.
- **No global mutable state.** No `lazy_static`, no `OnceCell`
  holding mutable data. Constants are fine.
- **Re-create the Clay arena every frame.** Already required by
  Clay's immediate-mode model — worth restating because it makes
  reloads free.
- **Actor channels rebuildable.** `prism-core::kernel::actor`
  needs to support tearing down + rebuilding the actor graph
  without losing pending messages. Use `crossbeam-channel`.
- **Component schemas serializable.** Anything in the registry
  must round-trip through serde so we can snapshot the *current
  document* across reload, not just the runtime state.

## 8. The page builder (the hard part)

Puck is the load-bearing piece, both for Studio and for every
Prism app a user authors. Replacing it is the migration. Sketch:

- **`prism-builder` crate** — Rust. Owns the component registry,
  drag/drop, selection, property panel, and serialization.
- **Component model** — a Clay component is a Rust trait with
  `render(&self, ctx, props) -> ClayElement` plus a schema for
  the property panel. Today's `ObjectRegistry` + shared field
  factories (see `feedback_puck_component_patterns.md` in memory)
  map cleanly onto this.
- **Serialization** — keep the current Puck JSON shape on disk
  so existing user content loads. Write a Rust deserializer that
  maps Puck's `{ type, props, children }` tree onto our component
  registry. This is a one-way compatibility door — we read Puck
  JSON forever, but new content is written in our schema.
- **DI** — every component must flow through the registry; no
  hand-wired Clay configs. (Same rule as today, just enforced in
  Rust.)
- **Builder UX** — drag handles, snap guides, resize, marquee
  select: all reimplemented on Clay's input layer. This is the
  work `react-moveable` / `react-selecto` / `@scena/react-guides`
  were doing for us.
- **Hot reload alignment** — bias the registry toward
  data-defined composite components (per §7) so adding a new
  component doesn't always mean a full rebuild.

Estimate this piece alone at one full phase (§9, Phase 3).

## 9. Phasing

Each phase is a real milestone with a working artifact. Don't
start the next phase until the current one ships.

### Phase 0 — Spike (1–2 weeks)

Goal: prove the architecture before committing.

- [ ] Stand up an empty `prism-shell` Rust crate.
- [ ] Wire `clay-ui-rs/clay` into it.
- [ ] Build it to WASM via `wasm-bindgen` + `wasm-pack`, served by
      `trunk`.
- [ ] Render *one* hard-coded panel (a sidebar with three buttons)
      in a browser.
- [ ] Forward `mousedown` / `mousemove` / `keydown` / `wheel` from
      the JS shim into Clay's input API.
- [ ] Validate the hot-reload loop end-to-end (see §7): edit a
      layout function, save, see the change in <2s without losing
      state.
- [ ] Spike `subsecond` under WASM. If it works, adopt it as the
      primary fast-reload mechanism. If not, document why and fall
      back to `trunk` + state snapshot.
- [x] **Desktop shell spike (B vs C).** *Resolved 2026-04-15 —
      Option C.* Phase 0 confirmed Option B viable (Tauri no-webview),
      but `tauri-runtime-wry` drops raw pointer input events, forcing
      Option C: bare `tao::EventLoop` + `wgpu` + `prism-shell`,
      no Tauri. See decision log.
- [x] **Daemon sibling-process IPC spike.** Spawn `prism-daemond`
      directly, connect over `interprocess` with length-prefixed
      `postcard` frames. Confirmed lifecycle (spawn, supervise, kill)
      works. *Resolved 2026-04-14 — see decision log.*
- [ ] Measure: bundle size, first paint, idle CPU, input latency
      vs. today's React Studio.

**Exit criteria:** the spike convinces us (or doesn't) that the
performance, ergonomics, and toolchain are workable. If it doesn't,
stop here and write a postmortem.

### Phase 1 — Foundation (4–6 weeks)

Goal: the smallest possible Studio that boots and renders one real
panel end-to-end on web.

- [ ] Create `prism-shell` (Rust → WASM) as the new SPA entry
      point alongside `prism-studio` (the React one keeps running
      in parallel until Phase 5).
- [ ] Port `@prism/core/design-tokens` to Rust constants.
- [ ] Port the lens/shell-mode resolver (`shell-mode.ts`,
      `load-boot-config.ts`, `boot-config-defaults.ts`) to Rust.
      These are pure functions with strong tests — straightforward.
- [ ] Build a Clay HTML renderer integration (or pick one) and pin
      its version.
- [ ] Define the daemon trait once and implement three
      transports behind it: WebSocket (web), `interprocess`
      (desktop sidecar), in-process channels (mobile, Phase 5).
      Web is the only target Phase 1 has to wire end-to-end;
      stub the other two.
- [ ] Port one simple panel (e.g. `identity-panel.tsx`) to a
      `panels::identity` Rust module.
- [ ] Get `pnpm dev` to serve both the React Studio and the Clay
      shell side by side under different routes (`/legacy`,
      `/clay`).

**Exit criteria:** a real user can load `/clay` in a browser and
see the identity panel, backed by the same daemon, the same Loro
state, and the same boot config as the React Studio.

### Phase 2 — Full `prism-core` port (10–14 weeks)

Goal: every TS subsystem in `prism-core` has a Rust equivalent
running in production, with the TS version retained only as a
parity oracle. This is the longest non-builder phase because the
volume is real (~638 `.ts` files in `prism-core`), even though
most of it is mechanical.

Order matters — port leaf modules first, then the modules that
depend on them.

- [x] **State pattern.** `prism_core::kernel::store::Store<S>` +
      `Action<S>` trait + `Subscription` handle. Single owning
      container for `S`, reducer-style dispatch, a synchronous
      subscription bus, and serde-backed `snapshot` / `restore`
      for the §7 hot-reload loop. `prism_shell::Shell` wraps
      `Store<AppState>` and is the sole mutation entry point for
      the native dev bin and the Studio main loop. Resolved
      2026-04-15.
- [ ] **`foundation/`** — `vfs`, `clipboard`, `undo`, `batch`,
      `template`, `persistence`, `date`, `object-model`,
      `crdt-stores`. Pure logic, no UI.
- [ ] **`foundation/loro-bridge.ts`** → port to use the `loro`
      Rust crate directly. This is the single biggest unlock:
      every other subsystem stops going through JS interop the
      moment Loro is native.
- [ ] **`identity/`** — `did`, `encryption`, `trust`, `manifest`.
      Pure crypto + data shape work; `ed25519-dalek` /
      `x25519-dalek` / `chacha20poly1305` cover what's needed.
- [ ] **`language/`** — `syntax` (the Scanner — keep its API),
      `document`, `registry`, `codegen`, `markdown` (now
      `pulldown-cmark`-backed), `expression`, `forms`, `facet`,
      and `luau` (now `mlua`-backed).
- [ ] **`kernel/`** — `actor`, `automation`, `state-machine`
      (now `statig`), `config`, `plugin`, `plugin-bundles`,
      `builder`, `initializer`.
- [ ] **`interaction/`** — `lens`, `shell-mode`, `atom`,
      `layout`, `input`, `activity`, `notification`, `search`,
      `view`, `design-tokens`, `page-builder` (the data
      half — the React rendering half lives in studio and gets
      replaced in Phase 3).
- [ ] **`network/`** — `relay`, `relay-manager`, `presence`,
      `session`, `discovery`, `server` (client). HTTP via
      `reqwest`, WS via `tokio-tungstenite`.
- [ ] **`domain/`** — `flux`, `timeline`, `graph-analysis`. The
      timeline is where the OpenDAW question lands; see §12.
- [ ] **`bindings/`** — `react-shell`, `puck`, `kbar`, `xyflow`,
      `codemirror`, `viewport3d`, `audio`. These are by
      definition framework-coupled; they are deleted, not
      ported, and their consumers in studio are rewritten in
      Phase 3.
- [ ] Every module ported gets a parity test that runs the same
      fixtures through both the TS and Rust implementations and
      asserts equality.

**Exit criteria:** `prism-core` (Rust) is the source of truth.
The TS `@prism/core` package still builds for the legacy React
Studio at `/legacy`, but no new code targets it.

### Phase 3 — Page builder (8–12 weeks, the long phase)

- [ ] Stand up `prism-builder` crate per §8.
- [ ] Port the component registry and shared field factories.
- [ ] Port every Puck component renderer (the ~30+ files in
      `prism-studio/src/components/`) into Rust component
      implementations. Group: form inputs, data display, content,
      cards, charts, calendar, dynamic widgets.
- [ ] Implement drag/drop, selection, marquee, resize, snap guides
      directly on Clay's input layer.
- [ ] Implement the property panel.
- [ ] Deserialize existing Puck JSON into the new component tree.
- [ ] E2E test against a real saved Puck doc and confirm visual
      parity with the React version.

**Exit criteria:** an end user can open a saved Studio app built
with the React Puck builder, and edit it inside the Clay builder
without data loss.

### Phase 4 — Editors and special widgets (6–8 weeks)

- [ ] CodeMirror 6 — Phase 1 strategy: keep CM6 as a DOM overlay
      on web; Clay reserves a "hole" and the JS shim positions a
      contenteditable element over it. Phase 4 strategy: replace
      with a Clay-native text editor on `ropey` + the existing
      `@prism/core/syntax` Scanner.
- [ ] Charts — port `chart-data.ts` driven renderer to Clay draw
      commands. Drop `recharts`.
- [ ] Graph editor — replace `@xyflow/react` with a Clay-native
      node graph renderer (`lyon` for the curves).
- [ ] Markdown — replace `react-markdown` with `pulldown-cmark`
      emitting Clay nodes.
- [ ] Maps — keep Leaflet as a DOM hybrid for web in this phase;
      schedule `maplibre-rs` for Phase 5 (native).
- [ ] Command palette — reimplement kbar.

**Exit criteria:** every "specialty" widget that React Studio
exposes also exists in the Clay shell, with the same panel API.

### Phase 5 — Cutover + native targets (4–6 weeks)

- [ ] Flip the default route in `prism-studio` from React to the
      Clay shell. The React tree becomes `/legacy` for two
      release cycles.
- [ ] **Desktop shell.** Ship the Option-C desktop build per §4.5:
      `tao` direct + `wgpu` renderer + `prism-daemon` as a sibling
      process over `interprocess`. Wire `cargo-packager` to produce
      signed `.dmg` / `.msi` / `.deb` / `.AppImage` artifacts and
      `self_update` for auto-update. No Tauri, no `wry`.
- [ ] **Mobile shell.** `cargo-mobile2` + `winit` for iOS and
      Android. Daemon code embedded in the same binary on a tokio
      runtime (the mobile process model doesn't allow sibling
      processes). Validate `cargo-mobile2` Xcode/Gradle builds
      produce installable artifacts.
- [ ] Replace the Leaflet hybrid with `maplibre-rs` for native
      builds.
- [ ] Delete the React Studio (`prism-studio/src/` React tree),
      `prism-admin-kit` React, `prism-puck-playground`,
      `@tauri-apps/api`, `@tauri-apps/cli`, and the Capacitor
      packages. Move anything still useful into the Rust crates.
- [ ] Update `CLAUDE.md`, `SPEC.md`, every package `CLAUDE.md`,
      and `docs/dev/current-plan.md` to reflect the new stack.

**Exit criteria:** there is no React in the client, no Tauri, and no
webview anywhere. Web ships as WASM into a canvas. Desktop ships as a
single Rust binary (`tao` + `wgpu`) packaged by `cargo-packager`.
Mobile ships as a single Rust binary (`winit` + `wgpu`) via
`cargo-mobile2`. Tests pass on all three.

## 10. Test strategy

- **Logic parity** — Vitest suites in `prism-core` get mirrored as
  Rust tests during the port, run side-by-side until the TS suite
  is deleted in Phase 5. Same fixtures, same assertions.
- **Visual parity** — Playwright suites today (263 studio + 169
  relay) keep running against the React Studio at `/legacy`. Add a
  parallel suite that exercises the Clay shell at `/clay` and
  compares screenshots panel-by-panel.
- **Layout snapshots** — `insta` snapshot tests over the
  Clay-element tree any panel emits. Cheap, fast, catches
  regressions before pixels.
- **CRDT parity** — write a fixture suite that opens the same
  saved doc in both Studios and asserts byte-identical Loro state
  after a fixed sequence of edits.
- **Bundle / perf budget** — set targets in Phase 0 (e.g. WASM gz
  ≤ 4 MB, first paint ≤ 1.5 s on M1, panel switch ≤ 16 ms). Treat
  as CI gates from Phase 1 onward.

## 11. Spikes to do before committing

Each is small enough to fit in 1–3 days. Run them all in Phase 0.

1. **`clay-ui-rs/clay` end-to-end spike.** Render one panel,
   forward input, measure bundle size and input latency. Confirm
   the binding's API surface is ergonomic enough to express the
   panels we have today. (The binding is decided; this validates
   it.)
2. **Loro-Rust spike.** Open one of our existing Loro doc files
   from disk in a Rust binary using the `loro` crate; confirm we
   read what `loro-crdt` writes.
3. **Puck-JSON deserializer spike.** Take one real saved Puck doc
   from `prism-puck-playground` and write a Rust deserializer that
   round-trips it.
4. **CodeMirror DOM-overlay spike.** Prove we can position a CM6
   instance over a Clay-reserved rect on web with reasonable
   ergonomics (cursor, focus, selection).
5. **Desktop shell spike (B vs C — resolved).** *Resolved
   2026-04-15 — Option C.* Option B (Tauri no-webview) was confirmed
   technically viable, but `tauri-runtime-wry` drops raw pointer
   input. Adopted Option C: bare `tao::EventLoop` + `wgpu` +
   `prism-shell`, no Tauri. See decision log.
6. **Daemon sibling-process IPC spike (resolved).** Spawn
   `prism-daemond` directly and exchange messages over
   `interprocess` (length-prefixed `postcard` frames). Confirmed
   spawn / supervise / kill works on mac, win, linux. *Resolved
   2026-04-14 — see decision log.*
7. **Hot-reload spike (`subsecond` + `trunk`).** Stand up the
   sub-2-second edit loop from §7 and prove state survives a
   reload. This is the make-or-break for developer experience.

## 12. Risks

- **Puck reimplementation is the migration.** If §8 takes longer
  than estimated, the whole thing slips. Front-load the spikes.
- **`prism-core` port volume.** ~638 `.ts` files. Most port
  mechanically, but the long tail (network, identity, language,
  domain) is months of work. The parity-oracle strategy is the
  only thing that keeps this honest — every Rust module ships
  with a fixture suite proving byte-equivalence to its TS
  predecessor before the TS version is deleted.
- **OpenDAW has no Rust analog.** The timeline / NLE / DSP graph
  has to be assembled from `cpal` + `symphonia` + `fundsp` +
  `kira` + `creek` + custom code, with `meadowlark-engine` as
  the only reference Rust DAW. This is its own project sitting
  inside Phase 2, and it should get its own design doc before
  any code is written. If the timeline isn't ready when Phase 2
  needs it, the rest of `prism-core` can still ship; the
  timeline becomes a Phase 4 widget instead.
- **CodeMirror is sticky.** A native text editor is months of
  work. The DOM-overlay escape hatch is real and load-bearing for
  Phase 1; do not underestimate it on native.
- **Hot reload may be worse than React fast-refresh.** That's a
  productivity tax on every contributor for the duration of the
  migration. The Phase 0 spike has to validate the loop is
  actually tolerable, not just functional.
- **Toolchain churn for contributors.** Adding a Rust → WASM build
  to the SPA edit loop slows the iteration cycle vs. Vite HMR.
  Mitigation: invest in `trunk` watch + (if it works) `subsecond`.
- **Third-party widget loss.** Every React-only library we depend
  on becomes work for us instead of an upstream maintainer. Some
  (recharts, xyflow) are nontrivial.
- **Mobile story is currently aspirational.** Capacitor exists
  today; replacing it with a real native shell + Clay renderer
  is its own project and should not block the web cutover.
- **Option C shell integration is DIY.** `cargo-packager`,
  `self_update`, `tray-icon`, and `notify-rust` work well in
  isolation; wiring them into a single coherent shell is our
  responsibility. Budget time in Phase 5 for integration testing
  across mac/win/linux.
- **`cargo-mobile2` is less mature than Tauri Mobile.** Mobile
  shells may ship behind desktop. Don't let mobile block the
  web + desktop cutover.
- **`clay-ui-rs/clay` is community-maintained.** If upstream
  stalls, we own the binding. Mitigate by tracking it closely and
  being ready to fork or rewrite the FFI layer.
- **No going back.** Once Phase 5 deletes the React tree, the
  fallback is gone. Keep `/legacy` alive for two release cycles
  minimum.

## 13. Open questions

- [ ] Which Clay renderer for web — official HTML, or a custom
      Canvas2D/WebGL one? Trade-off: HTML renderer = familiar
      a11y, easy text selection; Canvas = pixel-identical to
      native, no DOM overhead.
- [ ] Do we keep `prism-relay` as Hono JSX SSR indefinitely, or
      eventually serve a server-rendered Clay snapshot? (For now:
      rewrite Hono JSX SSR in place as the Sovereign Portal system.)
- [ ] What's the accessibility story? React + DOM gives us
      ARIA / screen readers for free. A canvas-based Clay
      renderer does not. The HTML renderer mitigates this.
      Decide before Phase 5.
- [ ] Do we ship a Prism-app *runtime* separate from the Studio,
      so end-user-built apps don't need the full builder?
      Probably yes — `prism-runtime` crate, smaller WASM bundle.
- [ ] Does `subsecond` work under WASM today, or only native?
      Spike #6 answers this.
- [ ] Do we ship an `egui` inspector overlay during the
      migration, or build the inspector in Clay itself?
- [x] IPC wire format for the daemon sibling-process channel: plain
      length-prefixed `postcard`, or a typed RPC layer like
      `tarpc`? **Resolved 2026-04-14 by Phase 0 spike #6 — plain
      postcard.** The daemon kernel's entire surface is
      `invoke(name, payload_json) -> result_json`; a typed RPC
      layer would be mostly dead weight against a shape that
      narrow. Frames carry a request id so the wire can grow
      pipelining later without a format break, and the `tarpc`
      door stays open for the day the surface widens.
- [x] Desktop shell strategy: Option B (Tauri no-webview) vs
      Option C (bare `tao` + standalone crates)? **Resolved
      2026-04-15 — Option C.** Option B confirmed technically
      viable by Phase 0 spike #5, but `wry`'s event wrapper drops
      raw pointer input fields that Clay needs. Option C adopted.
- [ ] OpenDAW replacement: do we build the timeline / NLE
      ourselves on top of `cpal` + `kira` + `fundsp`, or fork
      `meadowlark-engine` and reshape it into a library?
      Resolve in a dedicated DAW design doc before Phase 2
      starts on `domain/timeline`.

## 14. Decision log

(Append entries here as the migration progresses.)

- **2026-04-14** — Plan drafted.
- **2026-04-14** — Host language locked: **Rust**.
- **2026-04-14** — Clay binding pinned: **`clay-ui-rs/clay`**.
- **2026-04-14** — Hot-reload approach: `trunk` baseline, evaluate
  `subsecond` in Phase 0; native dev loop uses `hot-lib-reloader`
  until/unless `subsecond` proves out.
- **2026-04-14** — `prism-core` ports fully to Rust. No permanent
  JS-interop boundary. TS suites kept only as parity oracles
  during the port.
- **2026-04-14** — Library replacements pinned per §6.1.
  Notable: `loro` (CRDT), `mlua` w/ luau (scripting), `statig`
  (statecharts), `pulldown-cmark` (markdown), `tree-sitter`
  (parsing), `plotters` (charts), `petgraph` + `lyon` (graph
  editor), `layout-rs` (graph layout), `maplibre-rs` (maps,
  Phase 5), `ropey` (text editor, Phase 4), `cpal` + `kira` +
  `fundsp` + `creek` + `symphonia` (audio/DAW stack — replaces
  OpenDAW; needs its own design doc).
- **2026-04-14** — Native shell strategy locked: **Option B —
  Tauri 2.0 without the webview** (`tauri` crate + `tao`
  windowing + `wgpu` rendering, no `wry`). Mobile via Tauri
  Mobile in the same configuration. Daemon runs as a Tauri
  sidecar on desktop (`interprocess` IPC) and embedded
  in-process on mobile. Web target unchanged (WASM into
  canvas, remote daemon over WebSocket). **Fallback: Option C**
  — `winit` + `cargo-packager` + `cargo-mobile2` + standalone
  shell crates (`self_update`, `tray-icon`, `notify-rust`,
  `rfd`, `arboard`, `keyring`) — if Phase 0 spike #5 disproves
  the no-webview path. `@tauri-apps/api` (JS) deleted;
  `@tauri-apps/cli` and the `tauri` Rust crate kept under B.
- **2026-04-14** — Phase 0 scaffold landed. Cargo workspace
  created at the repo root with five members: `prism-daemon`
  (unchanged), `prism-core` (new — design tokens, shell mode,
  boot config; 4 tests green), `prism-builder` (new — component
  trait, registry, document tree, Puck-JSON reader; 2 tests
  green), `prism-shell` (new — `rlib` + `cdylib`, `AppState` +
  `panels::identity` stub, `native` feature gates wgpu/tao/winit,
  `web` feature gates wasm-bindgen; 1 test green), and
  `prism-studio/src-tauri` (rewritten for the no-webview
  shell). The React Studio, `@prism/core` TS, `@prism/shared`,
  `@prism/admin-kit`, `@prism/puck-playground`, Capacitor iOS
  and Android scaffolds, Vite, Tailwind, and Puck are **all
  deleted from the tree**. `prism-relay` (Hono JSX SSR) is the only
  TypeScript package left. Clay binding intentionally **not
  pinned** — the community `clay-ui-rs/clay` repo does not
  currently have a stable branch to track against, so
  `prism-shell` carries a `clay` cargo feature that stays off
  until Phase 0 spike #1 resolves which binding (or fork) to
  use. Everything else is ready for spikes #2–#7 to plug in.
- **2026-04-14** — **Phase 0 spike #1 resolved — Clay binding
  pinned to `clay-layout` 0.4 on crates.io.** `clay-ui-rs/clay`
  published a stable 0.4.0 with a working wgpu example, so
  `prism-shell` flipped its `clay` cargo feature on by default,
  vendored the example's `GraphicsContext` + `UiRenderer` + WGSL
  shader into `src/render/`, and ported `panels::identity` to
  real `Declaration` builders instead of the count-of-commands
  stub. The vendored renderer is windowing-library-agnostic
  (`raw-window-handle 0.6` behind `SharedWindow`), so the same
  stack drives both the Studio shell and the dev bin (both bare
  `tao::EventLoop` under Option C). `render_app` now returns live
  Clay commands; a unit test in `panels::identity` asserts it
  emits ≥2 rectangles plus ≥1 text command. Text measurement
  uses glyphon via `Rc<RefCell<UiRenderer>>` as Clay's
  measure-text user data; a stub measurer
  (`install_stub_text_measurer`) is still exposed for headless
  tests. `cargo test --workspace` and `cargo clippy
  --workspace --all-targets -- -D warnings` both green.
- **2026-04-14** — **Phase 0 spike #5 resolved — Option B
  confirmed viable.** `tauri::window::WindowBuilder` (gated
  behind the `unstable` cargo feature) creates a pure
  `tao::Window` with no webview attached — the explicit
  "no-webview" path Option B was betting on. `tauri::Window<Wry>`
  implements `raw_window_handle::HasWindowHandle +
  HasDisplayHandle`, so Studio wraps it in `SharedWindow`, builds
  the same `GraphicsContext` + `UiRenderer` + `Clay` triple the
  dev bin uses, installs a glyphon-backed measure-text callback,
  and drives frames off `RunEvent::MainEventsCleared` and
  window-resize events. `wry` stays in the dependency graph
  because `tauri-runtime-wry` is how Tauri reaches `tao` (it's
  the only `Runtime` impl shipping with Tauri 2 today), but the
  webview code path is never executed. Fallback to Option C
  (`winit` + `cargo-packager` + standalone shell crates) is
  therefore dropped. Studio and the `prism-shell` dev bin share
  one render path end-to-end; the remaining Phase 0 work is
  wiring `prism_shell::input` into the tao-event stream
  (a follow-on task from spike #5, not a separate §11 spike).
- **2026-04-14** — **Phase 0 spike #6 resolved — daemon sidecar
  IPC wire landed.** `prism-daemon` now carries a
  `transport-ipc` cargo feature that pulls in `interprocess 2` +
  `postcard 1` and adds `src/transport/ipc_local.rs`: a synchronous
  `serve_blocking(kernel, display)` server plus matching
  `bind_listener` / `connect_client` / `read_frame` / `write_frame`
  helpers and the shared wire types `IpcRequest` / `IpcResponse`
  (re-exported at the crate root so the client side can use them
  without reaching into a private module). The `prism-daemond`
  binary learned a `--ipc-socket <display>` flag that hands off to
  `serve_blocking` instead of running the stdio JSON loop; parsing
  lives in a new `parse_args` helper and is covered by 12 unit
  tests. Socket-name handling is cross-platform: the abstract
  namespace on Linux, named pipes on Windows, a filesystem socket
  under `std::env::temp_dir()` on macOS/BSDs (with a stale-file
  unlink on bind so a crashed daemon doesn't block the next
  launch). Frames are `u32 LE length ⧺ postcard(body)`; payloads
  stay JSON-as-string inside postcard because `serde_json::Value`
  is `#[serde(untagged)]` and postcard's non-self-describing
  format can't round-trip it, which also keeps the ipc and stdio
  transports byte-identical at the payload level. **Studio side:**
  `prism-studio/src-tauri/src/sidecar.rs` was rewritten from a
  TODO stub into a real client. `spawn_dev` locates the in-tree
  `prism-daemond` binary next to `prism-studio` in
  `target/<profile>/`, spawns it with `--ipc-socket
  prism-daemon-<pid>.sock`, retries `connect_client` for up to
  2 s while the server thread comes up, reads the handshake
  banner, and returns a `DaemonSidecar { child, stream, next_id,
  socket_name }`. `DaemonSidecar::invoke` sends a request / reads
  a response / asserts the id matches; its `Drop` impl
  best-effort-kills and waits the child so the supervise/kill
  path runs even on panic. `main.rs` stashes the handle in a
  `let mut daemon: Option<DaemonSidecar>` captured by the
  `app.run` closure and calls `daemon.take()` on `RunEvent::Exit`
  so the child gets reaped before the runtime unwinds. **Tests:**
  4 unit tests in `transport::ipc_local` (in-process bind +
  connect, unknown-command error path, clean EOF, cursor
  roundtrip) and 2 integration tests in `tests/ipc_bin.rs` (gated
  on `cli,transport-ipc`) that spawn the real binary, drive the
  banner + capabilities + `crdt.write` + unknown-command paths
  through postcard frames, and reap the child. The second
  integration test runs two sequential sessions with fresh
  socket names to prove the cleanup path rebinds cleanly on
  macOS. **Open question "postcard vs tarpc"** closed in favor
  of plain postcard. **Packaging** (bundling a signed
  `prism-daemond` binary alongside the Studio artifact via
  `cargo-packager`) is a Phase 5 follow-up; the spike only had
  to prove spawn / connect / postcard / supervise / kill works,
  which it does.
- **2026-04-14** — **`language::document` + `language::luau`
  landed in `prism-core` (ADR-002 §A1 / §A4, Phase 4 wiring).**
  Port source: commit `8426588` —
  `packages/prism-core/src/language/document/prism-file.ts` and
  `packages/prism-core/src/language/luau/{contribution,luau-provider}.ts`.
  `language::document::PrismFile` is the unified file record
  (path + optional `language_id`/`surface_id` + `FileBody::{Text,
  Graph, Binary}` + opaque `schema` / `metadata`); `FileBody::Graph`
  is boxed so the enum doesn't inflate to the `GraphObject` size.
  Narrowing helpers (`is_text_body`/`is_graph_body`/`is_binary_body`)
  and keyword-struct builders (`TextFileParams`/`GraphFileParams`/
  `BinaryFileParams`) mirror the TS API. `DocumentSchema` lives
  in the still-unported `language::forms` subtree; until that
  lands the `schema` field is `Option<serde_json::Value>` so
  form-driven files round-trip without blocking on the forms port.
  `language::luau::create_luau_contribution::<R,E>()` returns a
  `LanguageContribution` wired with `.luau`/`.lua` extensions,
  `text/x-luau` mime, a stub `parse` (empty `RootNode` until a
  full-moon Rust port lands — matches the TS behaviour when
  `isLuauParserReady()` was false), a `LuauSyntaxProvider` stub
  (empty diagnostics/completion/hover, wired so `SyntaxEngine`
  can register it today), and a `LanguageSurface` with
  `default_mode = SurfaceMode::Code` + `available_modes =
  [Code, Preview]` matching the TS debugger panel's second-mode
  contract. The registry gained `LanguageRegistry::resolve_file`
  — the Phase-4 wiring that lets Studio go `PrismFile →
  LanguageContribution` in one call, with `language_id` override
  winning over filename extension. `mlua`-backed execution
  continues to live in `prism-daemon::modules::luau_module`; the
  core contribution is intentionally framework-free so host
  crates (Studio, tests) specialise the `R`/`E` slots. 12 new
  tests (4 on `PrismFile` narrowing / schema handling, 4 on the
  contribution registry wiring, 2 on `LuauSyntaxProvider`, 4 on
  `resolve_file` covering text/graph/binary/unknown). Full
  `cargo test -p prism-core` green at 361 tests.
- **2026-04-15** — **Option B retired, Option C locked.** The
  Tauri 2 no-webview path from the 2026-04-14 §4.5 decision
  fell over the moment we tried to wire raw pointer input
  through `RunEvent::WindowEvent`. Root cause:
  `tauri-runtime-wry-2.10.1/src/lib.rs:552`
  (`WindowEventWrapper::map_from_tao`) unconditionally drops
  every `tao::WindowEvent` except `Resized` / `Moved` /
  `Destroyed` / `ScaleFactorChanged` / `Focused` /
  `ThemeChanged` before it reaches the user callback —
  `CursorMoved`, `MouseInput`, `MouseWheel`, and
  `KeyboardInput` are thrown away on the assumption that a
  webview is handling them. Combined with the tao version
  split the Tauri dep forced (`prism-shell` on `tao 0.30`,
  `tauri-runtime-wry` dragging in `tao 0.34`), there was no
  clean way to intercept raw input without patching
  `tauri-runtime-wry` itself. **Resolution:** drop Tauri
  entirely and adopt Option C — bare `tao::EventLoop` +
  `wgpu` + `prism-shell` in `prism-studio/src-tauri` (the
  directory name is now a historical artefact; a rename to
  plain `src/` is a cleanup followup). `prism-daemon` still
  rides in as a sibling process over the same
  `transport-ipc` postcard wire spike #6 validated; the IPC
  side didn't need to change. Packaging / signing / updater
  / tray / notifications / file dialogs / clipboard /
  keychain are all now Phase 5 items under their
  standalone-shell replacements: `cargo-packager`,
  `self_update` (or equivalent), `tray-icon`, `notify-rust`,
  `rfd`, `arboard`, `keyring`. Mobile follows: `cargo-mobile2`
  + `winit` on iOS/Android instead of the original "Tauri
  Mobile in no-webview configuration" plan. **Workspace
  impact:** `tauri` and `tauri-build` deleted from
  `[workspace.dependencies]`; `packages/prism-studio/src-tauri`
  loses `build.rs`, `tauri.conf.json`, `gen/`, and
  `Cargo.lock`; `src/main.rs` rewritten to mirror
  `prism-shell/src/bin/native.rs` exactly (bare
  `tao::EventLoop`, same `GraphicsContext` / `UiRenderer` /
  `Clay` triple, same `input::pump_clay` per-frame pump, same
  `CursorMoved` / `MouseInput` / `MouseWheel` handlers).
  `prism-cli` loses `Program::Tauri` and the `tauri()`
  builder constructor — `prism build --target studio` and
  `prism dev studio` now both funnel through `cargo
  build/run -p prism-studio` like any other Rust crate.
  Single `tao v0.30.8` across the workspace, verified with
  `cargo tree -p prism-studio | grep -iE "tao|wry|tauri"`.
  `cargo build --workspace`, `cargo test --workspace`
  (611 passed), and `cargo clippy --workspace --all-targets
  -- -D warnings` all green after the rip-out. The §4.5
  decision gate is closed: Option C is the shipping path.
- **2026-04-15** — **Phase 2 §9 state pattern landed.**
  `prism_core::kernel::store::Store<S>` + `Action<S>` trait +
  `Subscription` handle ships as the zustand replacement the
  migration plan has been promising since 2026-04-14. The store
  owns a single `S` by value, exposes reducer-style `dispatch`
  (for structured `Action<S>` implementations), an ad-hoc
  `mutate` escape hatch for unstructured updates, a `replace`
  path used by hot-reload restore, a synchronous subscription
  bus (listeners run in registration order, notified on every
  mutation path), and `into_inner` / `snapshot` / `restore` for
  the §7 hot-reload cycle (serde-json backed; the bound is
  on the `where` clause so hosts that don't need snapshotting
  don't pay). **Shell integration:** `prism_shell::app::Shell`
  wraps `Store<AppState>` and is now the sole mutation entry
  point for the packaged Studio binary and the `prism-shell`
  dev bin. Hosts call `Shell::dispatch_input(InputEvent)` in
  their tao-event handlers instead of the old
  `input::dispatch(&mut AppState, ...)` free function — the
  helper runs the existing input reducer inside
  `store.mutate` so every pointer move, button press, wheel
  tick, and resize fires the subscription bus exactly once.
  `Shell::subscribe` / `unsubscribe` / `snapshot` / `restore`
  are the hot-reload handles (`AppState` gained `Serialize +
  Deserialize`, as did `PointerState` and `SurfaceSize` inside
  `prism_shell::input`). `render_app(&AppState, &mut Clay)`
  still takes a borrowed state so renderers just call
  `shell.state()`; the public signature is unchanged.
  **Tests:** 16 new unit tests in
  `prism_core::kernel::store::tests` (new / default, dispatch
  + notify, mutate, replace, multiple listeners in order,
  unsubscribe, unsubscribe-unknown-is-noop, unsubscribe-one-
  leaves-others, post-dispatch state in listener, snapshot/
  restore round-trip, restore notifies, restore rejects bad
  bytes, `into_inner`, ids unique across unsubscribe, and a
  full hot-reload cycle simulation — serialise, rebuild fresh
  `Store`, subscribe, dispatch) plus 6 new tests in
  `prism_shell::app::tests` covering the `Shell` wrapper end
  to end. `cargo test --workspace` now at **627 passed, 0
  failed**; `cargo clippy --workspace --all-targets -- -D
  warnings` clean; `cargo fmt --all -- --check` clean. **Phase
  2 §9 "State pattern" checkbox flipped to ✅.** `kernel::actor`,
  `kernel::state_machine` (statig), and the rest of `kernel/`
  remain open — the store landing just unblocks the modules
  that want a place to live inside `AppState` and a
  subscription bus to notify from.
