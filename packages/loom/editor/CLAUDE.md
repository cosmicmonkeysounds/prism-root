# Loom

The Loom SaaS authoring app: sign in, manage projects, author `.loom`,
and run live events — all in the browser. Still works local-first on a
folder with no account.

## Intent
- **Author `.loom` two ways**: server-backed **projects** (BetterAuth
  account → the SaaS backend, `store/projects.ts` + `store/auth.ts`) OR a
  local folder via the File System Access API (no account). The workspace
  store is backend-aware (`OpenFile.backend`, `openServerProject`); saves
  route to the API or the on-disk handle accordingly.
- **Run events from the editor**: **Operate** mode (`⌘3`) is the shared
  run + admin surface — launch preview/live, share the join code + QR,
  pause/resume/end, and moderate the live roster/feed. Backed by
  `store/operate.ts` over the per-event mod SSE + `/e/:eventId/api/mod/*`
  (authorized by the author's session — the author *is* the operator).
- **See structure visually** alongside code: every open file is also a node on a React Flow canvas, and edges represent relationships between them.
- **Stay minimal**: a thin shell over CodeMirror + xyflow, with a small Zustand store as the single source of truth.

The app is gated in `App.tsx`: no workspace open → signed-out shows
`AuthGate`, signed-in shows `ProjectsLaunchpad`; opening a project (or a
local folder) enters the Studio shell. Dev talks to the backend
(`@loom/core` server, :7000) through the Vite proxy (`/api` + `/e`), so
the BetterAuth cookie flows same-origin. Override with `LOOM_SERVER`.

## Stack
- React 19 + TypeScript + Vite
- Tailwind CSS v4 (`@tailwindcss/vite`)
- `@uiw/react-codemirror` (one-dark theme, per-language extensions)
- `@xyflow/react` for the node-editor canvas + `elkjs` (layered
  compound layout for the story graph)
- `allotment` for the fixed modal-shell region splits (IDE redesign v2)
- `@dnd-kit` (core + sortable + utilities) for the BeatStrip clip reorder
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
              #   loom-lint / lsp-client / loom-ast / story-graph (@loom/core)
  store/      # zustand stores: workspace (files), focus (projection bus),
              #   mode (modal shell), graph (node-editor state), settings
  components/ # studio/ (StudioShell + ModeBar + per-mode regions),
              #   graph/ (the Editing-mode node editor), files, editor,
              #   operate, runner (Outline/References), detail, shell
  App.tsx     # top bar + StudioShell + status bar + overlays
```

## Shell (v2 modal topology)

The IDE is one app with three **modes** — Writing / Editing / **Run** —
switched from a bottom **Mode Bar** (`⌘1` / `⌘2` / `⌘3`), DaVinci-Resolve-style.
Run (`components/operate/`) is the live event-control surface; Writing +
Editing are the authoring facets.
(The play/perform/produce modes were removed with the runtime; runtime
lives in the `core` server + `play` app.) Each mode is a fixed,
resizable `allotment` layout (left rail · center stage · right
properties tray · optional bottom Timeline dock) that composes the leaf
panels; `store/mode.ts` owns the active mode + per-mode region sizes
(persisted to `localStorage["loom.studio"]`). The right **Properties
tray** (`components/studio/PropertiesTray.tsx`) is tabbed: in Writing it
follows the editor cursor (active-file AST via `lib/loom-ast.ts`); in
Editing it follows the **canvas selection** (beat/entity inspector with
editable contract + In/Out link lists); plus a workspace References
panel. This replaced the old `dockview` activity-bar + free-docking
model. Full design:
[`docs/dev/loom-ide-redesign.md` Part II](../../../docs/dev/loom-ide-redesign.md).

### Editing mode — the story-graph node editor (`components/graph/`)

Editing (`⌘2`) is a **global node editor over the whole project**, 1:1
with Loom Lang (Articy-Draft-style nesting, Pixel-Crushers-style
dialogue flows). Data comes from `@loom/core/lsp`'s
`Workspace.storyGraph()` (every indexed `.loom` file — never just the
active buffer) via `lib/story-graph.ts`'s `useStoryGraph()`
(recomputes on the `useLspIndexGen` counter).

- **Center — `StoryGraphPanel`.** Two nested levels: the **project
  map** (beats as cards inside collapsible per-file containers; divert /
  choice / tunnel edges cross files; `on <event>` **hook** edges from
  character pills; red **ghost nodes** for unresolved targets; ▶ entry
  badge, END terminal) and the **beat drill-in** (double-click a beat:
  its body as a top-down flow — dialogue cards, choice fan-outs that
  re-merge past the menu, `<if:>`/`<match:>`/`<each visit>`/`<after:>`
  branch heads with labeled arms, divert exit pills that double-click
  through to their target). Breadcrumb (`Story map › beat`) navigates
  back. ELK (`elkjs`) layered layout with compound containers
  (`graph/layout.ts`); manual drags persist per project
  (`localStorage["loom.graph.layouts"]`, `store/graph.ts`).
- **Edits round-trip to `.loom` source** through `@loom/core/parser`'s
  span-preserving `TextEdit` ops (`lib/story-graph.ts` applies per-URI
  batches via `writePathContents` — opens the file as a dirty tab +
  `syncBuffer`s the LSP so the graph re-derives instantly): drag a
  connection between beats → `appendDivert`; drag an edge end onto
  another beat → `retargetDivert` against the edge's exact
  `targetRange`; `+ beat` / context-menu delete → `insertBeat` /
  `removeBeat`; **Rename…** → `Workspace.renameBeat` (declaration +
  every cross-file reference + `entry:`); right-click a body node →
  Edit text… (`replaceExact`). Owned beats retarget only; derived
  (trait-template) beats are honest projections — edit the template.
- **Left rail — `EditingRail`**: Files (the Writing `Sidebar`) ⇄
  **Story Bin** (`StoryBin`, the project-wide navigator: beats grouped
  by file + every declared entity, filterable; click selects on canvas,
  double-click jumps to source).
- **Bottom dock — `BeatStrip`**: the selected beat's body as linear
  clips; drag-reorder → `moveBodyItem`, right-click delete →
  `removeBodyItem`, composer appends raw lines (`appendBodyLines`).
- **Run overlay**: the same canvas mounts in Run mode (Story tab,
  `variant="run"`, read-only) and lights up from `store/graph.ts`'s
  `RuntimeOverlay` — `store/operate.ts` listens to the mod SSE `sim`
  feed (`beatEntered` → visit badges + current-beat pulse). A future
  in-editor simulator drives the identical contract locally.
- Pipeline regression test: `components/graph/graph-pipeline.test.ts`
  (core graph → flow projection → ELK, over `escape-the-internet`).

## Conventions
- Path alias `@/*` → `src/*`.
- Keep components small; put logic in `lib/` or `store/`.
- Two persistence backends: the SaaS API (server projects) and FS handles (local folders). New persistence goes through `store/workspace.ts`'s backend-aware paths, not a third mechanism.
- Browser support: Chromium-based (File System Access API).

## Loom integration

`.loom` files are first-class. The editor consumes the Loom engine as
**native TypeScript** from `@loom/core` — **no wasm**. (`@loom/core` is
wired in via path aliases in `vite.config.ts` + `tsconfig.app.json`,
resolving `@loom/core/parser` and `@loom/core/lsp` straight to the
package's `.ts` source.) Everything here is synchronous; there is no
bundle to load or rebuild.

- `src/lib/loom-language.ts` — CodeMirror `StreamLanguage` mirroring
  the parser's line classifier. It **imports the keyword tables straight
  from `@loom/core/parser`** (`DECLARATIONS`, `SYNTACTIC_DIRECTIVES`,
  `CONTRACT_KEYS`, `RESERVED_INLINE`, `LIVE_KEYWORDS`,
  `SIMULACRA_KEYWORDS`, `MERIDIAN_KEYWORDS`) so the highlighter can never
  drift from the language (`keywords.ts` is the single source of truth for
  the parser AND every editor surface). It is a small per-line state
  machine: each line is classified into a `mode` (prose / value / expr /
  hook / decl / knot / divert) and `<…>` / `{…}` push a nested `ctx`; the
  key invariant is that reserved words (`is`/`with`/`END`/`self`/…) light
  up **only in expression contexts, never in prose/dialogue**. Token names
  map through a `tokenTable` to precise `@lezer/highlight` tags the
  one-dark theme colours reliably (kw→violet, type→yellow, label→blue,
  fn→blue, prop→coral, num→yellow, str→green, atom/speaker/interp→orange).
  Covered by `loom-language.test.ts` (`pnpm --filter loom-app test`),
  which drives the raw `loomStreamParser` over real corpus lines. The Rust
  `loom_syntax::emit_tmgrammar` TextMate grammar (Zed/VSCode) is the
  sibling surface and is behind on hooks / live-keywords (Rust
  deprioritized).
- `src/lib/loom-lint.ts` — CodeMirror `linter()` extensions. `loomLint()`
  is the parser-only fallback (`parse(source)` → CM spans; UTF-16 offsets,
  no remap). `loomLintProject(path)` is the default: it sources
  `Workspace.diagnosticsFor(uri)` so the gutter shows **cross-file project
  diagnostics** (`requiredSlotUnfilled` / `unresolvedTraitArg` /
  `derivedBeatConflict` / …), not just parser errors. `Editor.tsx` picks
  the project linter only when **both** `projectDiagnostics` **and**
  `indexWholeProject` are on (a partial index would emit false cross-file
  errors).
- `src/lib/lsp-client.ts` — the long-lived singleton `Workspace` from
  `@loom/core/lsp` (`lspWorkspaceSync()` / async `lspWorkspace()`), plus
  `uriFor` / `pathForUri` (per-segment encode/decode) and `docText(uri)`.
- `src/lib/loom-lsp.ts` — **the CodeMirror ⇄ LSP glue** — the IDE layer
  that makes CodeMirror behave like VSCode for `.loom`. `loomLspExtensions(path, opts)`
  composes: markdown **hover** tooltips (safe `textContent` render;
  `hoverDelayMs` delay, instant on ⌘/Ctrl-hover), LSP **completion**
  (`autocompletion` override → `completionAt`, replacing only the trailing
  identifier so owner-qualified diverts keep their `self.`), **go-to-definition**
  (⌘/Ctrl-Click + F12, cross-file) with a ⌘/Ctrl-hover **link underline**,
  **find-references** (Shift-F12 → focus bus → References panel), and
  **occurrence highlight**. All read live Workspace/store state at event
  time via getters, so the extension array stays stable across keystrokes
  (`Editor.tsx` memoizes on `file.path`, never the per-keystroke `file`).
- `src/lib/lsp-nav.ts` — position mapping (`offsetToLsp` / `lspToOffset`,
  0-based LSP ↔ CM offset) + cross-file nav: `navigateToLocation(loc)`
  (same-file `revealActive` vs. sibling `revealAt`), `findFileEntryByPath`,
  `firstLocation`.
- `src/lib/lsp-index.ts` + `src/lib/use-lsp-index.ts` — **whole-project
  indexing**. `indexProjectTree` walks the `root` FsEntry tree and pushes
  every `.loom` into the Workspace (server `.content` inline; local read
  lazily, mtime-gated), diffed via a text/mtime cache and batched through
  `Workspace.updateMany` (one rebuild). `syncBuffer` keeps the active
  unsaved buffer live; `dropIndexedPath` / `resetIndexCache` drop docs on
  delete/rename/project-switch (wired from `store/workspace.ts`). A
  `useLspIndexGen` counter bumps whenever indexed text changes, so the
  References panel + Command-Palette symbol lists recompute when async
  indexing lands. `useLspProjectIndex()` (mounted once in `StudioShell`)
  owns the reindex-on-`root` + buffer-sync effects.
  These drive the Outline + References panels **and** the in-buffer LSP
  features above; go-to-symbol lives in the Command Palette (`@` file /
  `#` workspace, `⌘⇧O`). Toggles + `hoverDelayMs` live in Settings
  (`store/settings.ts`, "Loom IDE" section).
- `src/lib/loom-ast.ts` — author-time typed-AST access for the
  Properties tray, plus `useLoomEdit` wrapping `@loom/core/parser`'s
  `applyBeatProperty` / `applyMoveBeat` / `applyInsertBeat` /
  `applyRemoveBeat` structural edits (editable tray fields + beat-flow
  drag rewrite `.loom` source).
- `src/lib/story-graph.ts` — the node-editor data layer:
  `useStoryGraph()` (the project-wide `Workspace.storyGraph()`, memoized
  on the index generation), `writePathContents` / `applyEditMap` (the
  graph-edit write path: open-as-tab + `updateContents` + `syncBuffer`),
  `writtenTargetFor` / `beatPath`. Replaced the old per-file
  `loom-story.ts` (and `runner/Graph.tsx` + `studio/BeatTimeline.tsx`,
  both deleted) — see **Editing mode** above for the `components/graph/`
  surface it feeds.

## Authoring-only — no in-editor play or collaboration

The editor is an **authoring tool**: open a folder, edit, highlight,
lint, full **in-buffer LSP** (hover / go-to-definition on ⌘/Ctrl-Click +
F12 / completion / find-references on ⇧F12 / occurrence highlight /
project diagnostics, plus the Outline / References panels and
go-to-symbol), structural beat edits, and the project-wide story-graph
node editor. It does
**not** run the show or co-edit over a relay — the former wasm
`LoomSession` (local play) and `LoomDoc` (Loro CRDT collaboration) were
removed in the wasm cutover. **Runtime lives in the sibling packages**:
`packages/loom/core` (the TS engine + SSE/REST event server) and
`packages/loom/play` (the participant app). The editor does not render the
guest chat; instead **Run** mode (`⌘3`) *controls + moderates* events on
that server (launch, codes/QR, roster, moderation) — the participant view
stays in the `play` app. The Mode Bar is three modes — **Writing** (`⌘1`),
**Editing** (`⌘2`), **Run** (`⌘3`).

## Hosting

- **`pnpm dev`** — Vite at `http://localhost:5173`, with HMR.
- **`pnpm build`** — produces `dist/` (a static SPA; the engine is
  bundled in, no wasm asset). Serve `dist/` from any static host.
