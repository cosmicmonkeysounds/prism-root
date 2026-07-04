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
- **Rehearse locally**: **Sim** mode (`⌘3`) runs the story on an
  in-browser `@loom/core` `Sim` — no server, no event, no account. Act
  as personas making choices, fire model-enumerated named events, and
  watch the runtime graph light up (`store/sim.ts` + the shared
  cockpit, `store/cockpit.ts`).
- **Run events from the editor**: **Operate** mode (`⌘4`) is the shared
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
              #   mode (modal shell), graph (node-editor state), settings,
              #   cockpit (shared Sim/Run contract + context),
              #   sim (local simulator), operate (live event)
  components/ # studio/ (StudioShell + ModeBar + per-mode regions),
              #   graph/ (the Editing-mode node editor), files, editor,
              #   cockpit/ (shared Sim/Run surfaces), sim/, operate/,
              #   runner (Outline/References), detail, shell
  App.tsx     # top bar + StudioShell + status bar + overlays
```

## Shell (v2 modal topology)

The IDE is one app with four **modes** — Writing / Editing / **Sim** /
**Run** — switched from a bottom **Mode Bar** (`⌘1` … `⌘4`),
DaVinci-Resolve-style. Writing + Editing are the authoring facets; Sim
(`components/sim/` + `store/sim.ts`) rehearses the story on a **local
in-browser simulator**; Run (`components/operate/`) is the live
event-control surface. Sim and Run are the same instrument panel — see
**The cockpit** below. Each mode is a fixed, resizable `allotment`
layout (left rail · center stage · right properties tray · optional
bottom Timeline dock) that composes the leaf panels; `store/mode.ts`
owns the active mode + per-mode region sizes (persisted to
`localStorage["loom.studio"]`). The right **Properties tray**
(`components/studio/PropertiesTray.tsx`) is tabbed: in Writing it
follows the editor cursor (active-file AST via `lib/loom-ast.ts`); in
Editing it follows the **canvas selection** (beat/entity inspector with
editable contract + In/Out link lists); plus a workspace References
panel; in Sim/Run it is the cockpit Inspector. This replaced the old
`dockview` activity-bar + free-docking model. Full design:
[`docs/dev/loom-ide-redesign.md` Part II](../../../docs/dev/loom-ide-redesign.md).

### The cockpit — Sim and Run share one surface

`store/cockpit.ts` defines the **cockpit contract** (`CockpitState`):
the state + action surface behind both live-control modes (roster,
factions/locations, channels + messages, beats, **named events**,
selection, and the whole mod-command vocabulary — say / capture /
setStat / fireBeat / fireSignal / scanAs / reveal / broadcast /
choose). Shared components read it through `useCockpit(selector)`,
resolved from a React context (`CockpitContext`) whose value is
whichever store the mode provides (`components/cockpit/providers.tsx`):

- **Run** → `store/operate.ts` — the mod SSE + `/e/:eventId/api/mod/*`.
- **Sim** → `store/sim.ts` — a real `@loom/core` `Sim` compiled from
  the indexed project and driven entirely in the browser. Message
  composition + snapshots reuse the event server's pure modules
  (`@loom/core/chat`'s `composeGuestMessages`/`ChatStore`,
  `@loom/core/views`' `modView`) so a simulated run reads exactly like
  the live event. A 1 s ticker drives `Sim.tick` while running.

Shared cockpit components (`components/cockpit/`): `tabs.tsx` (Chat /
Roster / World / Director pages), `Rail.tsx` (status + the
**perspective lens picker** + rooms navigator), `Inspector.tsx` (guest
/ character / faction / location inspectors + the `PendingChoice` card
+ **View/act-as** buttons), `rooms.ts` (the pure, perspective-aware
`buildRooms` room model — unit-tested in `rooms.test.ts`),
`inspect.ts`, `ui.tsx`, `format.ts`. Everything is **enum-driven**
(const-object enums — `erasableSyntaxOnly` forbids TS `enum`):
`CockpitTab`, `SelectionKind`, `CockpitPhase`, `SimStatus`, `LensKind`,
plus the core-side `SimEventType` / `BuiltinVerb`. The Director's
"fire named event" picker is a **closed list enumerated from the
compiled model** (`namedEvents(model)` — authored hook verbs,
builtins/timers excluded), with a custom… escape hatch.

**The conversation surface** (design:
[`docs/dev/loom-conversation-model.md`](../../../docs/dev/loom-conversation-model.md)):
the rooms rail lists the lobby, faction channels, **location rooms**
(`loc:<Id>` — where a beat's `setting:` routes its narration), authored
SPACE/CHANNEL rooms, and per-(character × guest) DM threads. The
cockpit's `perspective` lens (`OPERATOR_LENS` god view / a guest id /
a character id, set from the rail picker or an Inspector's "View + act
as") filters rooms + feed to what that identity can see and becomes
the composer's default voice. The Chat composer's "post as" picker is
grouped **Story (Operator · Narrator) · Guests · Cast**; guest speech
goes through the same journaled `say` path the play app uses and is
presence-gated in location rooms (the engine's `canPost` rule).
Messages render by `kind` — Narrator `narration` blocks, dim `system`
notices, amber `signal`s, sender-run-grouped `line`s — and every
scripted line carries a `⤷ beat` link that reveals its node on the
Story tab's map. The lens persona's pending choice docks as a
**decision tray** above the composer; answering it hits the local
engine in Sim and the journaled `/api/mod/choose` in Run (the mod
snapshot carries `ModView.choices`), so both modes can resolve a
stuck guest identically.

**Sim mode specifics** (`components/sim/`): the **Sim** tab owns
lifecycle (Start / Pause / Reset — Reset recompiles from the current
sources, with a stale-sources hint keyed off the LSP index generation —
plus Replay entry) and **personas** — local guests the writer acts as
(`Sim.createPerson`; one is auto-created on start, and the entry beat
auto-fires). Pending choices surface on the persona card, in the guest
Inspector, and as a global card when a menu suspends unbound; answering
one resumes the engine's saved continuation. The **Log** tab is the raw
sim ledger (every `SimEvent`, formatted per `SimEventType`). The
**Story** tab mounts the story-graph canvas (`variant="run"`), lit by
the same `RuntimeOverlay` contract Run uses; a **quick-fire** control
in the header fires any named event from anywhere. Speaking *as a
character* is the Chat composer's "post as" picker.

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
- **Nodes genuinely move**: `onNodesChange` applies React Flow's
  changes back into the positioned state (`applyNodeChanges`), so drags
  stick across decoration re-renders (they used to snap back on the
  next selection/runtime update), work in the drill-in too
  (session-local there), and children use `expandParent` — dragging a
  beat past its file container's edge grows the container instead of
  clamping at it.
- **Word blocks** (`graph/word-blocks.ts`): every beat card expands
  (header ▸/▾ chevron, context menu, or the toolbar `blocks`
  expand-all toggle) to show its full body as typed blocks — prose,
  dialogue, directives, choices, branch arms, diverts, slots
  (`WordBlockKind` enum), nesting rendered as indent — and the card
  stretches to fit (the block list drives the ELK size estimate;
  `expanded` lives in `store/graph.ts`). Collapse restores the compact
  3-line preview. **Word blocks author source 1:1**: each block
  carries exact anchors (`spanStart`/`spanEnd`/`topIndex`, pure
  `blockEditRange`) — on a file beat, double-click edits the block's
  literal source lines in place (`replaceExact`, shared `InlineEdit`
  textarea; dialogue = cue + merged prose, choice = its `* text` line,
  branch heads display-only); right-click inserts a line above/below
  (`insertBodyLines`) or deletes the item (`removeBodyItem`); the card
  menu adds lines/choices; file-container + pane menus create beats
  and CHARACTER/LOCATION/FACTION declarations (`appendDeclaration`) —
  so whole stories can be written from the canvas and read back
  identically in Writing mode.
- **Floating connectors** (`graph/FloatingEdge.tsx`): project-view
  edges anchor to the closest border point of each node instead of
  fixed left/right handles, so links stay sensible however the map is
  rearranged. Drill-in body flows keep fixed top/bottom ports (that
  layout is strictly top-down).
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
- **Runtime overlay**: the same canvas mounts in the Sim and Run
  modes' Story tabs (`variant="run"`, read-only) and lights up from
  `store/graph.ts`'s `RuntimeOverlay` — visit badges, current-beat
  pulse, and amber **traversal heat** on the edges a run actually took
  (`traversed`, best-effort beat→beat hops). Run feeds it from the mod
  SSE `sim` feed; Sim feeds it from the local simulator — the
  identical contract. In a run canvas, right-clicking a beat offers
  **Fire beat ▶** straight into the hosting cockpit.
- **Ergonomics** (all in `StoryGraphPanel` + `store/graph.ts`):
  - **Hover tooltips** on every node + edge (beat preview + `file:line`,
    entity summary, edge kind/text/guard/route), 220 ms delay; node
    hover also feeds the **focus bus** (`useFocus.setHover`).
  - **Edges are selectable** — clicking one (label included) opens the
    tray's **connection inspector**: route (jump either end), choice
    text/stickiness, guard, plus a **Rewire to** beat picker that
    rewrites the divert target in source. File containers select too
    (file inspector: beats/declarations, Open in Writing).
  - **`reveal(id)` centering** — Story Bin rows, tray link lists, and
    toolbar search all center+zoom the canvas on the target, retrying
    across relayouts, auto-enabling the entity overlay (or popping out
    of a drill-in) when the target is hidden.
  - **True inline editing** in the drill-in: double-click (or context
    menu → Edit in place) a prose/dialogue/choice/directive node to get
    an in-node textarea over the item's **raw source slice** — Enter
    commits via `replaceExact`, Shift+Enter newline, Esc cancels.
  - **Fit-to-content layout**: after first paint, React Flow's measured
    node sizes feed a second ELK pass (`onNodesChange` dimension
    changes → one re-layout per flow), so boxes always fit their text.
  - **MiniMap** (project view, pannable/zoomable); selecting a node
    **emphasises its connections** and dims the rest; **ghost nodes
    double-click to create the missing beat** (every dangling divert
    then resolves); file-container double-click opens the file (or
    expands a collapsed one).
  - **Collapsible file containers**: the header chevron / context menu
    folds a file to a compact leaf — its beats hide, every edge with a
    hidden endpoint re-routes to the file node (parallel edges merge
    into one `N links` aggregate; pure `collapseFlowEdges` in
    `flow.ts`), the runtime pulse lands on the collapsed node when the
    current beat is inside, and `reveal` auto-expands. Session-local
    (`collapsedFiles` in `store/graph.ts`).
  - **Live filter**: the toolbar input dims non-matching nodes (and
    edges between them) as you type — beat key / owner / entity / file
    path substring (`graph/filter.ts`); Enter still jumps to the first
    hit, Esc clears.
  - **Keyboard**: arrow keys walk selection to the geometrically
    nearest node (`graph/navigation.ts`), centering it; Enter drills
    into a beat / expands a collapsed file; Esc backs out of a
    drill-in / clears selection; F2 renames the selected beat; Delete
    removes it (file beats).
- Pipeline regression test: `components/graph/graph-pipeline.test.ts`
  (core graph → flow projection → ELK + collapsed-container coverage,
  over `escape-the-internet`); pure-logic suites in
  `flow-collapse.test.ts`, `navigation.test.ts`, `filter.test.ts`.

## Conventions
- Path alias `@/*` → `src/*`.
- Keep components small; put logic in `lib/` or `store/`.
- Two persistence backends: the SaaS API (server projects) and FS handles (local folders). New persistence goes through `store/workspace.ts`'s backend-aware paths, not a third mechanism.
- Browser support: Chromium-based (File System Access API).

## Loom integration

`.loom` files are first-class. The editor consumes the Loom engine as
**native TypeScript** from `@loom/core` — **no wasm**. (`@loom/core` is
wired in via path aliases in `vite.config.ts` + `tsconfig.app.json`:
`@loom/core/parser`, `@loom/core/lsp`, `@loom/core/sim` (the ecosystem
runtime behind Sim mode), and the server's pure projection modules
`@loom/core/chat` + `@loom/core/views`, all resolving straight to `.ts`
source.) Everything here is synchronous; there is no bundle to load or
rebuild.

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

## Local play is Sim mode — collaboration stays external

The editor is an authoring tool with a **local rehearsal runtime**:
open a folder, edit, highlight, lint, full **in-buffer LSP** (hover /
go-to-definition on ⌘/Ctrl-Click + F12 / completion / find-references
on ⇧F12 / occurrence highlight / project diagnostics, plus the Outline
/ References panels and go-to-symbol), structural beat edits, the
project-wide story-graph node editor, and **Sim** mode (`⌘3`) — the
`@loom/core` TS `Sim` running in-browser (no wasm; the old wasm
`LoomSession` stayed dead — this is the native-TS successor). The
editor still does **not** co-edit over a relay (`LoomDoc` / Loro CRDT
went with the wasm cutover), and **live events run in the sibling
packages**: `packages/loom/core` (the TS engine + SSE/REST event
server) and `packages/loom/play` (the participant app). The editor does
not render the guest chat; instead **Run** mode (`⌘4`) *controls +
moderates* events on that server (launch, codes/QR, roster,
moderation) — the participant view stays in the `play` app. The Mode
Bar is four modes — **Writing** (`⌘1`), **Editing** (`⌘2`), **Sim**
(`⌘3`), **Run** (`⌘4`).

## Hosting

- **`pnpm dev`** — Vite at `http://localhost:5173`, with HMR.
- **`pnpm build`** — produces `dist/` (a static SPA; the engine is
  bundled in, no wasm asset). Serve `dist/` from any static host.
