# Loom Editor — Web Shipping Roadmap

Draft · 2026-05-26

## 1. Thesis

The Loom editor ships as a **web app**. That means:

- Front end: React/Vite SPA in `packages/loom/editor`.
- Heavy logic that has to feel local (parse, diagnose, play, LSP-ish
  features) compiles to **WASM** and runs in the browser.
- The only Rust binary we ship is `loom-relayd` — a small slice of
  Prism's relay carved out for Loom (auth, CRDT host, capability
  tokens, play hub, static editor `dist/`).
- Luau ships **inside** the wasm bundle via the
  [`mluau`](https://github.com/mluau/mluau) fork of `mlua`. There is
  no desktop-runner asterisk — every `.luau` extension a project
  loads runs in the browser, against the same `LuauRegistry` we use
  natively.
- `pnpm dev` boots Vite for HMR plus a single `loom-relayd` process
  for the API/WS surface. The same `loom-relayd` is the production
  backend binary; in prod it also serves the editor's prebuilt
  `dist/` so deployment is one process.

Non-goals for shipping: native desktop Loom, Tauri packaging,
embedding inside `prism-studio`, any wasm dependency on
`prism-ui-runtime` / Taffy / femtovg. Loom on web does not render
through `prism-ui`; CodeMirror + dockview-react + React Flow are the
view layer.

## 2. Current state (what's already wired)

### Rust crates

| Crate | Web role | Native role | Status |
|------|---------|------------|--------|
| `loom-parser` | compiled to wasm via `loom-wasm` | LSP, runtime tests | ✅ |
| `loom-runtime` | compiled to wasm via `loom-wasm` (Phase 7 transcript also runs server-side in `play.rs`) | server play hub, runtime tests | ✅ feature-complete through §16 |
| `loom-syntax` | wasm — `emit_tmgrammar()` | TextMate generator | ✅ |
| `loom-wasm` | **the shippable wasm surface** | n/a | ✅ `parse` / `diagnose` / `emit_tmgrammar` / `LoomDoc` (Loro doc handle + subscriptions) |
| `loom-lsp` | **not on web today** — stdio JSON-RPC | desktop editors (Zed/VSCode) | ⚠️ not in the web bundle |
| `loom-server` | n/a | **the shippable backend** | ✅ Phases 1–8 (auth, REST workspaces, WS sync, presence, FSA import/export, server play, editor `dist/` mount) |

### Front end

`packages/loom/editor` already imports `loom-wasm` (see
`editor/src/loom-wasm/`), wires `LoomDoc` through `cm-loro.ts`, talks
the relay over `lib/sync.ts` + `lib/auth.ts`, and falls back to IDB
(`lib/idb.ts`) / FSA (`lib/fs.ts`) for offline projects. Dev mode
detects `:5173` and points at `127.0.0.1:7878`; same-origin in prod.

### Build flow

- `pnpm --filter loom-app wasm:build` → `wasm-pack` on `loom-wasm` →
  emits `editor/src/loom-wasm/`.
- `pnpm --filter loom-app build` → `tsc -b && vite build` →
  `editor/dist`.
- `cargo build -p loom-server --bin loom-relayd` → backend binary.
- `prism loom build` / `prism loom serve` already wrap the pair
  (per `packages/loom/CLAUDE.md`).

## 3. Target shipping shape

```
┌─────────────────────────── browser ───────────────────────────┐
│ React + Vite SPA (editor/dist)                                │
│   ├── CodeMirror w/ loom-language + loom-lint                 │
│   ├── dockview-react shell, React Flow canvas                 │
│   ├── Zustand stores                                          │
│   └── loom-wasm.wasm  ◀── parse / diagnose / LoomDoc / play   │
│                          (parser + runtime + syntax + Loro)   │
└──────────────────────────────────┬────────────────────────────┘
                                   │ HTTPS / WSS, same origin
                                   ▼
┌──────────────────── loom-relayd (one Rust binary) ────────────┐
│   /                      → ServeDir(editor/dist)              │
│   /api/auth/*            → password_auth module               │
│   /api/workspaces/*      → workspace REST                     │
│   /ws                    → CRDT sync + presence + play        │
│                                                               │
│   modules: collection_host · password_auth · capability_tokens│
│   subsystems: WsHub · PlayHub · WorkspaceRegistry             │
└───────────────────────────────────────────────────────────────┘
```

Single binary, single port (7878 default), single origin in prod.

## 4. Slice rule: what runs where

**Run in the browser (wasm)** — anything stateless or per-doc:

- Parsing, diagnostics, TM grammar generation.
- AST walks for outline / hover / definition (the LSP feature set,
  reimplemented as wasm calls — see §6).
- `LoomDoc` (Loro CRDT) for local mutation, snapshot, subscription.
- A *local* `Playhead` for solo authoring previews. Optional, future.

**Run on the server** — anything inherently multi-tenant:

- Auth, identity, workspace ownership.
- Authoritative CRDT host (`collection_host` module) — survives
  client refresh, fans out updates, persists snapshots.
- Capability tokens for share links.
- Server-hosted live play (`play.rs`) — already there, used when a
  scene needs multiple performers on one transcript.
- Editor static assets (one less reverse proxy to operate).

**Explicitly excluded from the web bundle** — these crates must not
be reachable from `loom-wasm`'s dep tree:

- `prism-builder`, `prism-ui-runtime`, `prism-shell`, `prism-relay`,
  `prism-daemon`. None are needed; pulling any of them re-introduces
  Taffy / femtovg / wgpu and balloons the wasm to tens of MB.
- `loom-server` (it has a tokio runtime + axum — server-only).

The Luau bridge is **not** excluded — see §7.

## 5. Dev loop after shipping

```bash
# one-shot, watches both:
pnpm --filter loom-app dev          # vite :5173, HMR
prism loom serve --cors permissive  # loom-relayd :7878, API+WS
```

…or, when iterating on parser/runtime:

```bash
pnpm --filter loom-app wasm:build:dev   # rebuild wasm artifact
```

Production deploy = `prism loom build && prism loom serve --bind
0.0.0.0:7878` behind TLS. No nginx, no Node, no PM2 required;
`self_update` from Prism's shell crates can do in-place upgrades
later if we want.

## 6. Gaps before shipping

Ordered by blocking-ness. Each item has a clear owner-crate.

### 6.1 Bundle-size budget for `loom-wasm`

Target: gzipped wasm under **3 MB** with Luau enabled. Workflow:

1. Measure baseline (parser + runtime, no Luau yet — current state).
2. Swap `mlua` → `mluau` workspace-wide (see §7).
3. Build `loom-wasm` for `wasm32-unknown-unknown` with `mluau`
   enabled; record size delta.
4. If over budget, the first lever is `wasm-opt -Oz` (we currently
   disable it in `loom-wasm/Cargo.toml` due to a macOS aarch64
   validator bug — revisit; the bug was in a wasm-pack-bundled
   binary, a fresh wasm-opt on PATH may be clean).
5. Second lever: trim `mluau` features (drop `serialize` if the
   only consumer is the native `LuauRegistry::value_to_lua` and we
   can replace it with hand rolling).

### 6.2 LSP feature parity in wasm

`loom-lsp` today only runs over stdio. The browser doesn't have
stdio, and we don't want to embed a JSON-RPC pump just to call
functions in the same process. Approach: **lift the handler logic
out of `loom-lsp` into a `loom_lsp::workspace` module** that's pure
Rust, then expose it from `loom-wasm` as direct functions
(`hover(uri, pos)` → `JsValue`, `completion(uri, pos)`,
`definition(uri, pos)`, `documentSymbol(uri)`). The CodeMirror side
already has hooks for each. The stdio binary stays as a thin shim
for Zed/VSCode users.

### 6.3 Editor wasm artifact in source control

`editor/src/loom-wasm/` is currently a wasm-pack output directory.
Decide: commit the generated artifacts (simple, but bloats git), or
make `pnpm dev` / `vite build` run `wasm:build` automatically as a
prebuild script. Recommendation: **automatic prebuild** via a tiny
Vite plugin that runs `wasm-pack` when source mtimes change.

### 6.4 Production server build path

`prism loom build` already does `vite build` + `cargo build -p
loom-server`. Confirm the binary statically embeds `editor/dist` via
`include_dir!` *optionally* (config-flag), so `--editor-dist` stays
overridable but the default deploy is a literally one-file binary.
This is the only change needed for "ship as one tarball."

### 6.5 Persistence story for the relay

Phase 1–8 of the multi-user plan focuses on the wire surface. We
need to lock down:

- Where does `WorkspaceRegistry` persist? (SQLite via `prism-core`
  storage? flat JSON? Loro snapshot files on disk?)
- Backup / export. FSA export already covers per-user; server-side
  backup is unsolved.
- Snapshot compaction cadence for long-lived CRDTs.

Audit current state of `workspaces.rs` and `play.rs` and write a
short follow-up doc — this is the highest-risk operational gap.

### 6.6 Hardening

- TLS termination: today expectation is "put a reverse proxy in
  front." That's fine for v1 but flag in deploy docs.
- Rate limiting on `/api/auth/register` — easy oversight.
- WS reconnect/backoff in `lib/sync.ts` (verify; if missing, add).
- CSP for the served `index.html`.

### 6.7 Editor UX polish (non-blocking)

- Remote-cursor preservation in `cm-loro.ts` (open TODO in
  `packages/loom/CLAUDE.md`).
- Booth live-patching UX (spec §13.4, also open).
- Web-friendly "extensions explorer" that surfaces *which* directives
  are wasm-native vs require a desktop runner — i.e. the Luau gap
  from §4.

## 7. Luau in wasm — via `mluau`

We use [`mluau`](https://github.com/mluau/mluau), a fork of `mlua`
maintained with first-class wasm support. The fork keeps the same
public API surface (`mlua::Lua`, `Function`, `Table`, `Variadic`,
`UserData`, `mlua::Error`), so the swap is mostly a workspace
manifest change plus a path rename in `loom-runtime`.

Plan:

1. **Workspace dep.** Replace `mlua = "0.10"` with a `mluau` git
   dep in the root `Cargo.toml`. Keep the same feature set
   (`luau`, `vendored`, `serialize`).
2. **Path alias.** `mluau` re-exports under the `mlua` name (the
   fork preserves the crate name), so source files in
   `loom-runtime/src/luau.rs` keep their `use mlua::{…}` imports.
   If the fork uses a different crate name, add
   `mluau = { package = "mluau", … }` and run a one-pass
   `s/mlua::/mluau::/` in `luau.rs` + `prism-luau-derive` if that
   crate references mlua paths directly.
3. **Native parity.** `cargo test -p loom-runtime` must still pass
   — this is the safety net before touching wasm.
4. **Wasm build.** Add the `luau-wasm` feature to `loom-runtime`
   only if `mluau` needs different feature flags on
   `wasm32-unknown-unknown`; otherwise build straight through.
   `loom-wasm` adds `loom-runtime` to its deps (already there) and
   `wasm-pack build --release` should produce one artifact.
5. **Editor surface.** No change. The wasm-exposed `LoomDoc` /
   `parse` / `diagnose` API already covers the editor's needs;
   Luau dispatch happens inside `Playhead::step` when (later) we
   expose a local play surface to wasm.

The 13 syntactic forms + core builtins still run as native Rust
inside `Playhead`; Luau is invoked only when a project loads a
`.luau` extension. That keeps the hot path fast and means
size-sensitive deployments could later strip Luau out via a
feature flag — but the default web bundle ships it.

## 8. Phased plan

| Phase | Scope | Done when |
|------|------|----------|
| **W1** | Baseline wasm size; swap `mlua` → `mluau`; build `loom-wasm` with Luau enabled | `wasm-pack build --release` < 3 MB gz with Luau in the bundle |
| **W2** | Lift `loom-lsp` handler logic into a reusable module; expose via `loom-wasm` | CodeMirror hover/completion/def/outline backed by wasm |
| **W3** | Auto-rebuild wasm in Vite plugin; commit `.gitignore` rule | `pnpm dev` cold-start regenerates wasm without manual step |
| **W4** | Persistence audit + decision doc for `loom-server` (storage backend, backups, compaction) | Decision doc merged, implementation issue filed |
| **W5** | `include_dir!` static-embed option; single-binary deploy | `loom-relayd` standalone binary serves editor with no extra files |
| **W6** | Hardening sweep (TLS docs, rate limits, CSP, WS backoff) | Deploy checklist green |
| **W7** | UX polish + open TODOs (remote cursors, booth live-patch) | Tracked in follow-ups, not shipping-blocking |

W1–W3 are the only true blockers for "web editor ships from a Vite
build + a Rust binary." W4–W5 are required before a public
deployment. W6–W7 can ship incrementally.

## 9. Open questions

- Do we want the wasm artifact to expose a *full* `Playhead` for
  local preview, or only parse/diagnose, with all play running
  through the server? Local would be snappier; server is simpler
  and keeps one source of truth.
- Should `loom-server` also serve the wasm artifact, or does the
  editor `dist/` already cover it? (It should — wasm-pack output
  lives under `editor/src/loom-wasm/` and gets bundled by Vite.)
- Is there a future Loom-as-embedded-component path back into
  `prism-shell`? If yes, the slice rule in §4 needs revisiting; if
  no, we can simplify further.
