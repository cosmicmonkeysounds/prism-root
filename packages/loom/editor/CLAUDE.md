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

## Install
This editor (`loom-app`) is part of the **loom pnpm workspace**
(`packages/loom/pnpm-workspace.yaml`, alongside `@loom/core` and
`loom-play`). One install covers all three: `cd packages/loom && pnpm
install`. Run editor scripts with `pnpm --filter loom-app <script>` from
the loom root, or `pnpm <script>` from this directory.

## Layout
```
src/
  lib/        # fs (File System Access), language (CM extensions),
              #   loom-lint / lsp-client / loom-ast / loom-story (@loom/core)
  store/      # zustand stores: workspace (files), focus (projection bus),
              #   mode (modal shell), settings
  components/ # studio/ (StudioShell + ModeBar + per-mode regions),
              #   files, editor, canvas, runner (Outline/References/Graph),
              #   detail, shell
  App.tsx     # top bar + StudioShell + status bar + overlays
```

## Shell (v2 modal topology)

The IDE is one app with two **author modes** — Writing / Editing —
switched from a bottom **Mode Bar** (`⌘1` / `⌘2`), DaVinci-Resolve-style.
(The play/perform/produce modes were removed with the runtime; runtime
lives in the `core` server + `play` app.) Each mode is a fixed,
resizable `allotment` layout (left rail · center stage · right
properties tray · optional bottom Timeline dock) that composes the leaf
panels; `store/mode.ts` owns the active mode + per-mode region sizes
(persisted to `localStorage["loom.studio"]`). The right **Properties
tray** (`components/studio/PropertiesTray.tsx`) is tabbed: it follows
the editor cursor and reads the active file's AST via `lib/loom-ast.ts`,
plus a workspace References panel. This replaced the old `dockview`
activity-bar + free-docking model. Full design:
[`docs/dev/loom-ide-redesign.md` Part II](../../../docs/dev/loom-ide-redesign.md).

## Conventions
- Path alias `@/*` → `src/*`.
- Keep components small; put logic in `lib/` or `store/`.
- No backend. Anything persistent goes through the FS handles the user grants.
- Browser support: Chromium-based (File System Access API).

## Loom integration

`.loom` files are first-class. The editor consumes the Loom engine as
**native TypeScript** from `@loom/core` — **no wasm**. (`@loom/core` is
wired in via path aliases in `vite.config.ts` + `tsconfig.app.json`,
resolving `@loom/core/parser` and `@loom/core/lsp` straight to the
package's `.ts` source.) Everything here is synchronous; there is no
bundle to load or rebuild.

- `src/lib/loom-language.ts` — CodeMirror `StreamLanguage` mirroring
  the parser's line classifier. Token shapes match the TextMate grammar
  shipped to Zed/VSCode by `loom_syntax::emit_tmgrammar`.
- `src/lib/loom-lint.ts` — `linter()` extension that calls
  `@loom/core` `parse(source)` and maps the diagnostics' spans straight
  to CodeMirror (the parser reports UTF-16 offsets, so no byte↔char
  remap).
- `src/lib/lsp-client.ts` — a long-lived `Workspace` from
  `@loom/core/lsp` (completion / hover / definition / documentSymbols /
  references / diagnostics), kept in sync with the open-file map. Drives
  the Outline + References panels.
- `src/lib/loom-ast.ts` — author-time typed-AST access for the
  Properties tray, plus `useLoomEdit` wrapping `@loom/core/parser`'s
  `applyBeatProperty` / `applyMoveBeat` / `applyInsertBeat` /
  `applyRemoveBeat` structural edits (editable tray fields + beat-flow
  drag rewrite `.loom` source).
- `src/lib/loom-story.ts` — pure helper that lifts a parsed `.loom`
  file into a static **story model** (beats / characters / locations /
  cohorts + beat→beat divert/choice/tunnel edges) and a reach-depth
  layout. Feeds the Editing mode's center **`runner/Graph.tsx`** entity
  graph and the dock **`studio/BeatTimeline.tsx`** beat flow-DAG (both
  xyflow). Node double-click jumps to source via
  `workspace.revealActive` + Writing mode.

## Authoring-only — no in-editor play or collaboration

The editor is an **authoring tool**: open a folder, edit, highlight,
lint, LSP (Outline / References / completion / hover / definition),
structural beat edits, and the static graph / beat-flow views. It does
**not** run the show or co-edit over a relay — the former wasm
`LoomSession` (local play) and `LoomDoc` (Loro CRDT collaboration) were
removed in the wasm cutover. **Runtime lives in the sibling packages**:
`packages/loom/core` (the TS engine + SSE/REST event server) and
`packages/loom/play` (the participant app). The Mode Bar is two modes —
**Writing** (`⌘1`) and **Editing** (`⌘2`).

## Hosting

- **`pnpm dev`** — Vite at `http://localhost:5173`, with HMR.
- **`pnpm build`** — produces `dist/` (a static SPA; the engine is
  bundled in, no wasm asset). Serve `dist/` from any static host.
