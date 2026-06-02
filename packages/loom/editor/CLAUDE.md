# Loom

A local-first IDE with a visual canvas, running entirely in the browser.

## Intent
- **Edit real files on disk** via the File System Access API — no server, no upload.
- **See structure visually** alongside code: every open file is also a node on a React Flow canvas, and edges represent relationships between them.
- **Stay minimal**: a thin shell over CodeMirror + xyflow, with a small Zustand store as the single source of truth.

## Stack
- React 19 + TypeScript + Vite
- Tailwind CSS v4 (`@tailwindcss/vite`)
- `@uiw/react-codemirror` (one-dark theme, per-language extensions)
- `@xyflow/react` for the canvas
- `allotment` for the fixed modal-shell region splits (IDE redesign v2)
- `@dnd-kit` (core + sortable) for the Editing-facet beat reorder
- `zustand` for state
- `pnpm` for package management

## Layout
```
src/
  lib/        # fs (File System Access), language (CM extensions)
  store/      # zustand stores: workspace (files), session (relay/play),
              #   focus (projection bus), mode (modal shell)
  components/ # studio/ (StudioShell + ModeBar + per-mode regions),
              #   files, editor, canvas, runner, cloud, detail, shell
  App.tsx     # top bar + StudioShell + status bar + overlays
```

## Shell (v2 modal topology)

The IDE is one app with five **modes** — Writing / Editing /
Simulating / Performing / Production — switched from a bottom **Mode
Bar** (`⌘1..⌘5`), DaVinci-Resolve-style. Each mode is a fixed,
resizable `allotment` layout (left rail · center stage · right
properties tray · optional bottom Timeline dock) that composes the
existing leaf panels; `store/mode.ts` owns the active mode + per-mode
region sizes (persisted to `localStorage["loom.studio"]`). The right
**Properties tray** (`components/studio/PropertiesTray.tsx`) is tabbed
and context-sensitive: author modes follow the editor cursor and read
the active file's AST via `lib/loom-ast.ts`; runtime modes reuse the
focus-driven `InspectorPanel`. A global `TopBar` adds a
play/stop/fork/snapshot transport, relay status, and a presence strip.
This replaced the old `dockview` activity-bar + free-docking +
workspace-preset model — `components/dock/*` + `store/presets.ts` are
removed and `dockview-react` is uninstalled. Full design:
[`docs/dev/loom-ide-redesign.md` Part II](../../../docs/dev/loom-ide-redesign.md).

## Conventions
- Path alias `@/*` → `src/*`.
- Keep components small; put logic in `lib/` or `store/`.
- No backend. Anything persistent goes through the FS handles the user grants.
- Browser support: Chromium-based (File System Access API).

## Loom integration

`.loom` files are first-class. Highlighting + diagnostics flow through
`packages/loom/wasm` (built with `pnpm wasm:build`, output to
`src/loom-wasm/`):

- `src/lib/loom-language.ts` — CodeMirror `StreamLanguage` mirroring
  the Rust `loom_parser::lexer` line classifier. Token shapes match
  the TextMate grammar shipped to Zed/VSCode by `loom_syntax::emit_tmgrammar`.
- `src/lib/loom-lint.ts` — `linter()` extension that calls the
  wasm-compiled real parser (`diagnose(source)`) and maps byte spans
  to CodeMirror diagnostics. Loaded lazily on first `.loom` open.
- `src/lib/loom-ast.ts` — author-time AST access (wasm `parse`) for the
  Properties tray, plus `useLoomEdit` wrapping the wasm
  `apply_beat_property` / `apply_move_beat` structural edits (Phase 4 —
  editable tray fields + `BeatTimeline` drag-to-reorder rewrite source).

To regenerate the wasm bundle after editing `packages/loom/parser` or
`packages/loom/wasm`, run `pnpm wasm:build` (release) or
`pnpm wasm:build:dev` (faster compile, larger output).

## Hosting

Two ways to run the editor:

1. **`pnpm dev`** — Vite at `http://localhost:5173`, with HMR. The
   editor detects the dev port and points `relayUrl` at
   `http://127.0.0.1:7878`. The relay needs `--cors permissive` to
   accept the cross-origin handshake — `prism loom serve --cors
   permissive` (or `cargo run -p loom-server --bin loom-relayd --
   --cors permissive`).
2. **`prism loom serve`** — the production topology. `pnpm build`
   produces `dist/`; `loom-relayd` serves that directory plus the
   `/api/*` REST surface plus `/ws` over a single TCP listener. The
   editor picks `window.location.origin` as the relay URL by default
   because the bundle is loaded from the same origin. No CORS needed.

The default relay URL is computed in `src/store/session.ts`. To force
a specific relay regardless of origin, set `localStorage["loom.relayUrl"]`
or use the Cloud panel's "Relay URL" form.

## Playing — local (no relay) vs cloud

Play works out of the box with **no relay and no account**. On boot the
editor activates a `kind: "local"` workspace (`store/session.ts`
`activateLocal`): it targets the open local folder's `.loom` files —
preferring live, unsaved editor buffers — and falls back to a bundled
example (`lib/example-project.ts`, the Saltmere tutorial) when no folder
is open. **Start play** then runs the show entirely in the browser via
the wasm `LoomSession` (`lib/local-play.ts`), which wraps
`loom_runtime::session::PlaySession` compiled to wasm. Every transport
action (`choose` / `fork` / `snapshot` / `restore` / head ops / booth
live-patch) is dispatched against that local engine and writes the same
`active.play` (`PlayStatePayload`) the relay would, so every Runner
panel works identically.

Opening a workspace from the **Cloud** panel (Production mode, `⌘5`)
switches `active.kind` to `"cloud"` and routes the same transport
through the relay WebSocket for collaboration. Closing it drops back to
local play. The TopBar shows a `local` / `cloud` chip on the active
workspace.

Caveat: the Luau VM can't target wasm, so Lua-defined directives
(`goal`/`heal`/`flash`/…) and `.luau` extensions degrade to logged
envelopes in local play (the runtime registry is `lenient` there); the
core narrative + the Rust directives are full-fidelity. Run the cloud
relay for full Luau. A pure-Rust Lua VM for the browser is a follow-up.

## Running the full simulator (Run / Debug surfaces)

See [`docs/dev/loom-ide-redesign.md` §0 "Running the IDE"](../../../docs/dev/loom-ide-redesign.md#0-running-the-ide)
for the full walkthrough: launch flows, the per-preset panel set,
keybindings, the side-drawer / popover / modal / panel projection
sinks, branching Timeline head tabs + right-click "Fork from here",
and booth live-patch. The fastest path needs **nothing running** — open
the editor (`pnpm dev`), hit `⌘3` for **Simulating** mode, and click
**Start play** in the Choices panel; Transcript / World / Timeline /
Inspector populate together off the local engine. For collaboration:

```
prism loom dev
```

That one command rebuilds the wasm bundle, builds `loom-relayd`, and
starts the Vite editor (HMR) on `:5173` + the relay (API + WS) on
`:7878` under one supervised process — Ctrl+C stops both. Register and
create a workspace in the Cloud panel to co-play; otherwise local play
already works on `:5173` alone.
