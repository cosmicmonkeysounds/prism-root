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
focus-driven `InspectorPanel`. This
replaced the old `dockview` activity-bar + free-docking +
workspace-preset model (`components/dock/*`, `store/presets.ts` — still
in-tree, unused, kept one release as a fallback). Full design:
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

## Running the full simulator (Run / Debug surfaces)

See [`docs/dev/loom-ide-redesign.md` §0 "Running the IDE"](../../../docs/dev/loom-ide-redesign.md#0-running-the-ide)
for the full walkthrough: launch flows, the per-preset panel set,
keybindings, the side-drawer / popover / modal / panel projection
sinks, branching Timeline head tabs + right-click "Fork from here",
and booth live-patch. The fast path:

```
prism loom dev
```

That one command rebuilds the wasm bundle, builds `loom-relayd`, and
starts the Vite editor (HMR) on `:5173` + the relay (API + WS) on
`:7878` under one supervised process — Ctrl+C stops both. Open the
printed editor URL, register, create a workspace in the Cloud panel,
hit `⌘3` for **Simulating** mode, click **Start play** in the Choices
panel — Transcript / World / Timeline / Inspector populate together and
every hover/click links all panels through the focus + projection
bus.
