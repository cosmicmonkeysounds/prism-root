# prism-shell

Single source of truth for the Prism UI tree. Every Studio panel,
every lens, and the page builder itself all render through
`render_app(&AppState, &mut Clay)` — downstream backends (wgpu on
native, a Canvas2D painter driven by a hand-rolled C ABI on web)
only know how to walk the returned Clay render commands and turn
them into pixels.

## Build & Test
- `cargo build -p prism-shell` — default features `native` + `clay`.
- `source $HOME/.cache/emsdk/emsdk_env.sh && cargo build -p prism-shell
  --target wasm32-unknown-emscripten --no-default-features --features web`
  — emscripten WASM build. The `.cargo/config.toml` in this crate
  passes the `-s` flags emcc needs to emit a modularised ES module
  (`createPrismShell`).
- `cargo test -p prism-shell` — unit + insta snapshot tests.
- `prism dev shell` — native bin (`src/bin/native.rs`) against
  tao/wgpu without any Tauri packaging.
- `prism dev web` — builds the emscripten bin, copies
  `prism_shell_wasm.{js,wasm}` into `web/`, then serves that
  directory via `python3 -m http.server 1420`. Load
  `http://127.0.0.1:1420/` in a browser to boot the Canvas2D
  painter against the live shell.

## Crate layout
- `crate-type = ["rlib"]` — rlib only. Studio
  (`packages/prism-studio/src-tauri`) pulls this in as a library;
  the `[[bin]] prism_shell_wasm` emcc entry point links against
  the same rlib. A `cdylib` would trigger a second emcc link step
  that clashes with `-sMODULARIZE`/`-sEXPORT_ES6`; the daemon hit
  the same trap and settled on rlib-only, and this crate follows
  suit.
- `[[bin]] prism-shell` — native dev binary at `src/bin/native.rs`.
  Only present when the `native` feature is active (it pulls in
  `wgpu`/`tao`/`glyphon`).
- `[[bin]] prism_shell_wasm` — empty-main shim at
  `src/bin/prism_shell_wasm.rs`. Gated on the `web` feature. Exists
  only so emcc sees a `main` symbol and so rustc's LTO pass keeps
  the `#[no_mangle] extern "C"` exports from `src/web.rs` alive
  across the link.

## Features
- `native` (default) — on-desktop stack: `wgpu`, `tao`, `winit`,
  `glyphon`, `pollster`, `raw-window-handle`, `bytemuck`, plus
  `prism-daemon` with `crdt` / `luau` / `vfs` / `crypto`.
- `web` — activates the emscripten C-ABI adapter in `src/web.rs`.
  Pulls `clay` in transitively; does NOT pull `wasm-bindgen` /
  `web-sys` / `js-sys` / `console_error_panic_hook`. All DOM +
  canvas access lives in the hand-written `web/loader.js`; Rust
  only produces a tagged render-command byte buffer.
- `clay` (default) — pulls in `clay-layout` and flips `panels::*` +
  `render` from the count-of-commands stub onto the real Clay path.

The `native` and `web` features are mutually exclusive at build time
— pick the target triple and feature set together (native cargo
build vs. `wasm32-unknown-emscripten` with `--features web`).

## Public surface
From `src/lib.rs`:
- `render_app(&AppState, &mut Clay) -> Vec<RenderCommand<'_, (), ()>>`
  — the one entry point every backend calls per frame. Takes a
  borrowed `&AppState` so renderers can call it with `shell.state()`.
- `Shell` — owning wrapper around `prism_core::Store<AppState>`. The
  host (native dev bin / Studio / the WASM shim) builds one of these
  on startup and routes every input event through `Shell::dispatch_input`.
  Exposes `state()`, `store_mut()`, `subscribe` / `unsubscribe`, and
  `snapshot` / `restore` for §7 hot-reload.
- `InputAction` — serialisable `Action<AppState>` wrapper around
  `InputEvent`. Exposed so hosts (or tests) that want structured
  action dispatch can hand Inputs to `store.dispatch(InputAction(evt))`
  instead of the ergonomic `Shell::dispatch_input` helper.
- `AppState`, `ActivePanel` — the reloadable root state. Everything
  reloadable must live inside `AppState`; never stash state in
  `lazy_static` / `OnceCell` (that breaks the hot-reload story).
  `AppState` is `Serialize + Deserialize` so `Shell::snapshot` /
  `restore` just works.
- `install_stub_text_measurer(&mut Clay)` — placeholder text
  measurer Phase 0 uses until glyphon is wired into the render pass.
- `panels`, `input`, `render`, `app` — the four top-level modules.
- `web` — feature-gated entry point for the WASM target (`src/web.rs`).

## Phase 0 status
The shell currently renders one hard-coded panel (`panels::identity`)
with a sidebar of three buttons and forwards mouse/keyboard into
Clay's input API. Everything past that is a deliberate TODO until
Phase 1 begins fanning out the full panel set. When adding a panel:

1. Implement `Panel` in `src/panels/<name>.rs`.
2. Wire it into `ActivePanel` and the match in `render_app`.
3. Add an insta snapshot that covers the declare path.
4. Keep per-panel runtime state inside `AppState`.

## Downstream
- `prism-studio/src-tauri` embeds this crate as a library with
  `default-features = false, features = ["native", "clay"]`.
  Studio supplies the `tao::EventLoop` + window (no Tauri, no wry —
  §4.5 Option C locked 2026-04-15); this crate supplies the tree.
- The web target (`prism dev web`) drives `web::*` through
  emscripten + `web/loader.js`. `loader.js` imports the factory
  `createPrismShell`, `cwrap`s the `prism_shell_*` C exports, and
  walks `Module.HEAPU8` each frame to paint the render-command
  buffer into a `<canvas id="prism">` via Canvas2D. No
  `wasm-bindgen`, no trunk, no React, no WebGL (yet — glyphon /
  wgpu on WebGPU land in a later Phase).

## Web render buffer format
`prism_shell_frame(dt)` returns a pointer into `AppState`'s internal
`Vec<u8>`; `prism_shell_frame_len()` returns its length. Every entry
starts with a u8 tag and is followed by a variant-specific payload
(all multi-byte values little-endian):

- `0` Rectangle — `f32 x, y, w, h` + `u8 r, g, b, a`.
- `1` Border — same rect + RGBA + `f32 width`.
- `2` Text — rect + RGBA + `u16 size` + `u32 byte_len` + UTF-8
  bytes (no null terminator).
- `3` ScissorStart — rect.
- `4` ScissorEnd — no payload.

The JS loader in `web/loader.js` walks this exact layout via
`DataView`; keep the two sides in lockstep when adding variants.
