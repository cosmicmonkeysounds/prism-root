# Loom IDE Redesign

Status: **Phases 1–6 landed** (2026-05-31). The React editor owns
every Run / Debug surface; the focus + projection bus links every
Runner panel into a single hover-highlight / click-to-detail surface;
the runtime is multi-head end-to-end (server `PlaySession` holds
HashMap&lt;HeadId, Mesh&gt; + snapshot registry; editor `useSession.play`
is head-keyed; Timeline grows head tabs + fork affordance); workspace
presets (Author / Direct / Debug / Perform / Read + user-saved) cycle
via ⌘⌥1..5 and a StatusBar switcher; Cast / Booth / Outline /
References panels round out the dock. The PySide6 simulator is
deprecated but kept runnable as a local debugger.

> **Direction change (2026-05-31).** The free-docking `dockview` shell
> (activity bar + freely dockable panels + preset dropdown) proved
> too chaotic: every panel toggle mutates the layout, there is no
> stable "home," and the user must reassemble a workspace each
> session. It is being replaced by a **modal, DaVinci-Resolve-style
> topology** — a bottom **Mode Bar** switching five fixed,
> purpose-built layouts (**Writing / Editing / Simulating /
> Performing / Production**), a persistent context-sensitive
> **Properties tray** on the right, and a **clips-on-tracks Timeline**
> dock along the bottom. The full v2 spec is **Part II** at the end of
> this document. Parts 0–8 describe the dockview shell that v2
> supersedes; the focus/projection bus (§4), session model, detail
> registry, and every leaf panel are **reused unchanged** as region
> contents — v2 is a shell rewrite, not a panel rewrite.

All Phase 1–6 follow-ups landed in a final pass (2026-05-31):

- **Side sink** — additive right-edge drawer that stacks multiple
  pinned details; Shift-click on any focusable promotes to side,
  Alt-click forces modal. Esc clears the popover/modal sinks; the
  drawer is dismissed per-entry via the × button.
- **Right-click "Fork from here"** — Timeline envelopes open a
  context menu with `Snapshot @ #N`, `Fork from #N`, and
  `Open in detail panel`. Powered by the reusable
  `useContextMenu`/`openContextMenu` primitive in
  `@/store/context-menu` + `<ContextMenuHost>` mounted in App.
- **Booth live-patch over relay** — new WS envelopes
  `play-booth-skip` / `play-booth-force` / `play-booth-reload` route
  to `PlayHub::booth_skip` / `booth_force` / `booth_hot_reload`,
  which thread through `Playhead::booth_*` on every head (hot-reload
  re-seeds tracks + advances each head past its first pause). Booth
  panel exposes skip / hot-reload buttons and a directive entry
  form.
- **LSP-driven Outline / References** — Outline calls
  `LspWorkspace.documentSymbols` and renders the LSP
  `DocumentSymbol[]`; References calls the new
  `LspWorkspace.referencesByName` (backed by a new
  `loom-lsp::references` module + `Workspace::references_at`). The
  wasm bundle was rebuilt to expose the new surface.

Related design docs:
- [`loom-v3.html`](./loom-v3.html) — language spec.
- [`loom-editor.html`](./loom-editor.html) — original editor concept (Arrangement view §4.2 is the timeline ancestor).
- [`loom-multiuser.md`](./loom-multiuser.md) — relay + collab protocol that the head-keyed messages extend.

---

## 0. Running the IDE

### The one-shot — `prism loom dev`

```
prism loom dev
```

That's the whole launch. The CLI:
1. Builds the wasm bundle (`pnpm wasm:build:dev`).
2. Builds the relay binary (`cargo build -p loom-server`).
3. Starts both servers under the prism supervisor with colored,
   prefixed logs and Ctrl+C fan-out:
   - **Editor (HMR)**: http://127.0.0.1:5173
   - **Relay (API+WS)**: http://127.0.0.1:7878

The Vite editor receives `VITE_LOOM_RELAY=http://127.0.0.1:7878` so
its API + WS URLs always hit the right port even at non-default
configurations. Open the editor URL in any Chromium browser, register,
and you're playing.

Useful flags:

- `--ui-port 5174 --relay-port 7879` — non-default ports.
- `--host 0.0.0.0` — expose both servers on the LAN.
- `--ui-only` / `--relay-only` — run one half only (e.g. an external
  relay you've already started).
- `--no-wasm` — skip the wasm preflight (use the committed bundle).
- `--ship` — release-profile relay (slow compile, fast steady state).
- `--dry-run` — print every command without running.

### Other launch flows

Single-binary, closest to production (one Rust process serves the
prebuilt editor `dist/` alongside the API + WS):

```
prism loom build              # vite build + cargo build -p loom-server
prism loom serve --build      # boot loom-relayd → http://127.0.0.1:7878
```

`prism loom serve` flags: `--bind 0.0.0.0:7878`, `--ship`,
`--editor-dist <path>`, `--cors permissive`. Equivalent bare-cargo
invocation:

```
cargo run -p loom-server --bin loom-relayd -- \
  --editor-dist packages/loom/editor/dist
```

Local debugger (no relay, no editor):

```
prism loom sim packages/loom/examples/saltmere
```

That's the PySide6 simulator — deprecated as the primary surface but
still runnable for offline debugging.

### Walking through the full simulator inside the IDE

Once the relay + editor are up (Option A or B):

1. **Open the editor** at the URL the launch command printed.
2. **Register** a user and pick a username (capability tokens live in
   `localStorage`).
3. **Cloud panel (⌘K)** — create a workspace, link a local folder
   (File System Access API → grants the editor read/write rights to a
   directory of `.loom` files), or paste sources directly.
4. **Workspace preset** — pick *Debug* from the StatusBar switcher
   (or hit ⌘⌥3) to open Editor + Timeline + Ledger + World + Detail
   panels at once. *Direct* (⌘⌥2) is the live-performance preset.
5. **Choices panel (⌘⇧C)** — click **Start play**. The server boots a
   `PlaySession` from the workspace's `.loom` files, advances to the
   first choice or `Step::Awaiting`, and broadcasts `play-state` to
   every subscriber.
6. **Drive the show** by clicking choice buttons. Every panel updates
   in lockstep: Timeline grows new envelopes on per-track rows,
   Ledger appends rows, World re-renders mutated keys, Transcript
   scrolls.
7. **Hover anywhere** — every other panel dims unrelated elements and
   highlights related ones in one frame.
8. **Click anywhere** — opens the entity's detail in the configured
   sink (most surfaces default to a popover; row labels go to the
   dock Detail panel). **Shift-click** adds to the right-edge side
   drawer (stack multiple); **Alt-click** opens a modal.
9. **Right-click a Timeline envelope** for `Snapshot @ #N` /
   `Fork from #N` / `Open in detail panel`.
10. **Booth panel (⌘⇧B)** — head table with promote/drop/snapshot,
    snapshot list with fork-from/restore, live-patch (skip beat,
    hot-reload bundle, force-fire directive).
11. **Save your layout** as a named preset via the **+** button next
    to the StatusBar switcher.

If you have collaborators running the same relay, every panel stays in
sync over the WS broadcast — including head tabs (everyone sees the
same set of live heads) and snapshots.

### Quick reference: keybindings

| Combo  | Panel                  | Combo    | Surface           |
|--------|------------------------|----------|-------------------|
| ⌘B     | Files                  | ⌘⇧F      | Search            |
| ⌘1     | Editor                 | ⌘J       | Canvas            |
| ⌘K     | Cloud                  | ⌘2       | Remote            |
| ⌘⇧P    | Play (composite)       | ⌘⇧T      | Transcript        |
| ⌘⇧C    | Choices                | ⌘⇧L      | Ledger            |
| ⌘⇧W    | World                  | ⌘⇧M      | Timeline (mesh)   |
| ⌘⇧I    | Inspector              | ⌘⇧D      | Detail            |
| ⌘⇧G    | Graph                  | ⌘⇧A      | Cast              |
| ⌘⇧B    | Booth                  | ⌘⇧O      | Outline           |
| ⌘⇧R    | References             |          |                   |
| ⌘⌥1..5 | Cycle built-in workspace presets (Author / Direct / Debug / Perform / Read) |

All shortcuts toggle (open if absent, close if focused).

### Verifying a working build

The full test surface, all green at last check:

```
cargo test -p loom-runtime --test snapshot_fork   # 3/3 — branching invariants
cargo test -p loom-server --lib play              # 4/4 — multi-head session
cargo test -p loom-lsp --lib                      # 6/6 — completion/hover/refs
cd packages/loom/editor && npx tsc -b             # type-check
cd packages/loom/editor && pnpm lint              # 0 errors, 0 warnings
cd packages/loom/editor && pnpm build             # vite production build
```

---

## 1. North star

One React/dockview surface that is simultaneously **Author**, **Run**,
and **View**. Modality is expressed as **workspace presets** (saved
dock layouts + visible panel set + keybinding profile), not separate
apps. The Timeline is the centerpiece of Run/Debug workspaces and is
fundamentally a **DAG view of a branching ledger**, not a tape.

Hover anywhere — timeline block, graph node, world entry, transcript
line — and the rest of the IDE *focuses* on that entity. Click to
open its detail view in one of several projection sinks (inline pane,
floating popover, modal, dock panel). The same selection plumbing
serves the keyboard, screen reader, and presence layer.

---

## 2. Layered architecture

```
┌─────────────────────────────────────────────────────────────────┐
│  WORKSPACE PRESETS  ──  Author · Direct · Debug · Perform · Read │
├─────────────────────────────────────────────────────────────────┤
│  PANEL REGISTRY  ──  ~18 panels, each declares                   │
│  { id, kind, default placement, required session caps }          │
├─────────────────────────────────────────────────────────────────┤
│  SELECTION + PROJECTION BUS                                      │
│  - one global "focused entity" (entity ref + provenance)         │
│  - panels subscribe to focus changes, dim non-related elements   │
│  - openDetail(ref, sink: 'panel'|'popover'|'modal'|'side')       │
├─────────────────────────────────────────────────────────────────┤
│  SESSION + RUNTIME BRIDGE                                        │
│  - many playheads (heads) on one shared bundle                   │
│  - relay or in-browser WASM both implement the same trait        │
└─────────────────────────────────────────────────────────────────┘
```

---

## 3. Panel catalogue

| Kind | Panel | Replaces / status |
|---|---|---|
| **Project** | `files`, `search`, `cloud`, `diagnostics` | existing + sim Diagnostics tab |
| **Author** | `editor`, `outline`, `references` | existing editor + new LSP-driven |
| **Structure** | `canvas`, `graph`, `tree` | xyflow + sim Graph + new |
| **Runner** | `transcript`, `choices`, `timeline`, `ledger`, `world`, `inspector` | sim left pane + sim tabs + sim InspectorDialog |
| **Live** | `cast`, `booth`, `presence` | new (uses `LiveStage`) + sim top bar + existing |

Each panel module exports a `PanelDescriptor`:

```ts
type PanelDescriptor = {
  id: PanelId
  title: string
  icon: ReactNode
  kind: 'project' | 'author' | 'structure' | 'runner' | 'live'
  defaultPlacement: PlacementHint  // referenceId + direction
  requires?: SessionCapability[]   // e.g. 'playhead', 'relay'
  component: FunctionComponent
}
```

Presets store `{ name, layout: DockviewSerialized, visiblePanels: PanelId[] }`.

---

## 4. Unified focus + projection model

The piece tying every view together. Two primitives:

### 4.1 Focus

A single global `FocusRef` lives in the session store:

```ts
type FocusRef =
  | { kind: 'envelope'; head: HeadId; idx: number }
  | { kind: 'character'; name: string }
  | { kind: 'beat'; ref: BeatRef }
  | { kind: 'cohort' | 'location' | 'item' | 'faction'; name: string }
  | { kind: 'track'; head: HeadId; track: TrackId }
  | { kind: 'world-key'; key: string }
  | null

type FocusState = {
  hover: FocusRef          // set on pointerenter, cleared on pointerleave
  pinned: FocusRef         // set on click, persists until explicitly cleared
}
```

`hover` drives the **dimmed-non-related** highlight pass. `pinned`
drives detail panels and survives mouse movement. UI rule: if
`hover` is set, render with hover emphasis; else fall back to
`pinned` for emphasis. This avoids the classic "I clicked then
moved my mouse and lost the highlight" trap.

### 4.2 Connection resolver

Every view that wants to participate calls
`useRelated(ref): RelatedSet` once. The resolver is a pure function
of (`focused ref`, `bundle`, `active head ledger`):

```ts
type RelatedSet = {
  entities: Set<string>      // canonicalised refs as strings
  envelopes: Set<number>     // ledger indices
  edges:    Set<string>      // "<from>→<to>" canonical edge keys
  summary:  string           // one-line "Wren · 3 beats · 2 hooks fired"
}
```

Resolver rules (initial):
- `envelope` → its cause chain + descendants, the speaker/character it mentions, the beat it lives in, the track row.
- `character` → every envelope they speak or were targeted by; every beat with them in `cast`; every divert that lands in such a beat; trust/knowledge edges to other characters.
- `beat` → its cast, setting, outbound diverts, every BeatEntered envelope for it.
- `track` → every envelope on that track + the spawner envelope.
- `world-key` → every WorldSet/KnowledgeChanged that wrote it, every `let` that reads it, every hook keyed on it.

Views render an element in one of three states:
- **highlighted** — in `RelatedSet`
- **dimmed** — not in `RelatedSet` and `hover` is set
- **normal** — `hover` is null

Each view owns its own opacity classes; they all read from the same
hook. No view-pair coupling.

### 4.3 Detail projection

Clicking pins focus and opens a detail. Sinks:

| Sink | Use case | Container |
|---|---|---|
| `panel` | Long-lived inspection during debug | A `detail` dock panel that swaps content on focus change |
| `popover` | Quick peek near the click point | Floating, dismissed on outside-click or `Esc` |
| `modal` | Edit-and-confirm (e.g. rewrite a beat's cast) | Centered, blocks background |
| `side` | Compare two entities side-by-side | Slides in from the right, additive |

```ts
openDetail(ref, { sink: 'popover', anchor: domRect })
```

The detail *content* is registered per entity-kind via a second
registry (`DetailRegistry`), so a `character` detail is the same
React tree whether shown in a popover or the detail panel.

User preference for default sink lives in settings; views can
override per call (timeline blocks default to `popover`, graph nodes
to `panel`, world keys to `popover`).

---

## 5. Branching Timeline

The runtime today emits a single linear `Ledger` from one `Playhead`.
Branching requires runtime support (Phase 1, below) and an editor
overhaul (Phase 3).

### 5.1 Runtime model

- `Playhead::snapshot() -> PlayheadSnapshot` clones world + ledger + frame stack + scheduler.
- `Playhead::restore(&snapshot)` overwrites `self`.
- `Mesh::fork()` produces a fresh `Mesh` with an independent playhead but shared `Arc<Bundle>` / `Arc<Registry>`.

Snapshots are cheap because the bundle is shared; only mutable
runtime state needs cloning.

### 5.2 Driver protocol additions

`loom-play` grows a `HashMap<HeadId, Mesh>`:

```
{"cmd":"snapshot","head":"h0"}          → {"type":"snapshot","id":"s7","head":"h0","at":42}
{"cmd":"restore","head":"h0","id":"s7"} → {"type":"restored","head":"h0"}
{"cmd":"fork","from":"s7"}              → {"type":"forked","head":"h1","from":"s7"}
{"cmd":"fork","head":"h0"}              → fork live (no snapshot) → {"type":"forked","head":"h1"}
{"cmd":"drop","head":"h1"}              → {"type":"dropped","head":"h1"}
{"cmd":"heads"}                         → {"type":"heads","heads":[{"id":"h0",...}]}
```

Every existing event/choice/world/tracks/ended/awaiting payload gains
a `"head": "h0"` field. Existing clients that ignore unknown fields
keep working.

### 5.3 Editor model

```
active.play = {
  heads: Record<HeadId, PlayheadState>
  primary: HeadId
  snapshots: Record<SnapshotId, { headId, atIdx, label, createdAt, parent?: SnapshotId }>
}
```

### 5.4 Timeline panel

```
┌─ Timeline ─────────────────────────────────────────────────────┐
│ heads: ▸ main  ◇ what-if:bell-tower  ◇ what-if:no-ring  [+]   │
│        [⤴ fork]  [⤵ merge]  [⏮ restore]  [📌 snapshot]         │
├────────────────────────────────────────────────────────────────┤
│ Booth   ───●─●──┐                                              │
│ Main    ─●─●─●─◇─●─●─                                          │
│ Wren    ─────●──┤  ╲                                           │
│ Reeve   ─────●──┤   ╲●─●─       ← fork "what-if:no-ring"       │
│ ambient ─────●──┤                                              │
│         t=0           t=now ───────────────►                   │
└────────────────────────────────────────────────────────────────┘
```

Three primitives:
1. **Head tabs** — one tab per live `Playhead`; switching changes which envelopes are "active".
2. **Fork point (◇)** — a ledger index where a head spawned. Drawn as a diamond bridging two track bands.
3. **Scrub-and-fork** — right-click any past envelope on the active head → "Fork from here".

Reuses the simulator's track layout, event colour map, and cause arcs.
Adds: head tabs, dim-non-active-head rendering, fork affordance.
Hover/click flows through the global focus bus from §4.

### 5.5 Graph + Timeline cohabitation

Both views observe the same focus bus. Hovering a Wren envelope on
the timeline dims every non-Wren-related node on the graph and every
non-Wren-related block on the timeline simultaneously. Clicking
opens the character detail (default sink: `panel`).

---

## 6. Workspace presets

Five shipped presets:

- **Author** — files · editor · outline · canvas · diagnostics.
- **Direct** (live performance) — transcript · cast · booth · world · presence.
- **Debug** — editor · timeline · ledger · world · inspector.
- **Perform** — transcript · choices · graph.
- **Read** — transcript only, full-bleed; share-link mode.

Saved in `localStorage["loom.workspaces"]`; users can save current
layout as a new preset. Keybindings cycle `⌘1`–`⌘5` (configurable).

---

## 7. Migration phases

1. **Runtime: snapshot/restore + multi-head driver** (`packages/loom/runtime`). Add `Playhead::snapshot/restore`, multi-head dispatch in `loom_play.rs`, mesh tests. No editor change.
2. **Editor: port simulator panels** to the existing dock shell. New panels: `transcript`, `ledger`, `world`, `inspector`, `graph`, `timeline-linear` (single head, sim parity). Retire `simulator.py`.
3. **Editor: focus + projection bus.** Implement `FocusState`, `useRelated`, `DetailRegistry`, projection sinks. Wire all Runner + Structure panels.
4. **Editor: multi-head session model.** Refactor `useSession.active.play` to head-keyed shape; relay protocol gains `head` field; `timeline` grows head tabs and fork action; `transcript`/`world`/`inspector` follow active head.
5. **Workspace presets.** Save/load + five defaults + keybindings.
6. **Polish.** `cast`/`booth`/`outline`/`references` panels; share-link Read preset; presence integration with heads.

---

## 8. Phase 1 plan (in flight)

| Step | File | Notes |
|---|---|---|
| Add `Clone` to `Frame`, `LetSlot`, `Scheduler`, `CoroutineHandle` | `playhead.rs`, `scheduler.rs` | Bundle/Registry already `Arc`-wrapped; Ledger/World already derive `Clone` |
| Add `Clone for Playhead` | `playhead.rs` | Manual impl (or derive once members are Clone) |
| Add `Mesh::fork(&self) -> Mesh` | `mesh.rs` | Deep clone with shared bundle |
| Add `Playhead::snapshot/restore` | `playhead.rs` | Just `self.clone()` / `*self = snap.clone()` |
| Multi-head loom-play driver | `bin/loom_play.rs` | HashMap<HeadId, Mesh>; all emissions gain `"head"` |
| Integration tests | `tests/snapshot_restore.rs` | Restore-determinism + fork-divergence |

Phase 1 ships when:
- `cargo run -p prism-cli -- test -p loom-runtime` passes including the new tests.
- `loom-play` accepts `snapshot`/`restore`/`fork`/`drop`/`heads` and round-trips them in a manual stdio session.
- `mesh_phase_a` / `mesh_phase_b` still pass unchanged.

---

# Part II — Modal topology (v2): the Studio shell

Status: **Phases 1–5 landed** (2026-06-01) — all migration phases, plus
the §13 Timeline **clock axis** + **multi-head lanes** and the deeper
**edit ops** (insert / remove beat, reorder body items). The dockview
shell in Parts 0–8 is fully removed (dead files deleted,
`dockview-react` uninstalled). Decisions locked: layout via
**`allotment`**, Timeline via **`dnd-timeline` + `@dnd-kit`
(headless)**.

**Landed 2026-06-01:**
- **Studio shell (Phase 1).** `editor/src/store/mode.ts` (mode +
  per-mode region sizes, persisted to `localStorage["loom.studio"]`) +
  `editor/src/components/studio/{StudioShell,ModeBar,regions}.tsx`.
  `App.tsx` renders `<StudioShell>` instead of `<DockShell>`; the five
  modes render the existing leaf panels into fixed `allotment` regions;
  `⌘1..⌘5` switch modes; the Mode Bar carries the rail/tray collapse
  toggles. The StatusBar preset `<select>` is gone. `DockShell` /
  `panels` / `panel-registry` / `presets` remain in-tree but unused
  (dockview fallback for one release).
- **Properties tray (Phase 2).** `editor/src/components/studio/PropertiesTray.tsx`
  + `editor/src/lib/loom-ast.ts`. Tabbed, context-sensitive right rail
  with per-mode tab sets + defaults. Author modes follow the editor
  cursor — the tray shows the beat / declaration the cursor is inside,
  sourced from the wasm `parse` AST (the runtime `DetailFor` needs a
  play head, so it can't serve author time); clicking the title pins
  focus to drive the References tab. Runtime modes reuse
  `InspectorPanel`; Performing gets a Booth tab, Production a Deploy
  tab. Editable write-back stays Phase 4.
- **Parser edit primitive (Phase 4 groundwork).** `loom-parser` grew a
  public `edit` module: `TextEdit`, `apply_edits`, `move_beat`,
  `set_beat_property`, `Anchor`. Span-based byte-range splices that
  leave untouched lines byte-identical (the §10 round-trip invariant),
  with the minimal-diff guarantee covered by `parser/tests/edit_api.rs`.
- **Timeline v2 — Run facet (Phase 3).** `runner/Timeline.tsx` rebuilt
  as clips-on-tracks: a fixed track gutter + a zoomable/pannable lane
  with a ruler, clip extents (beats span to the next `BeatEntered`),
  a playhead (tracks the focused envelope), cause-arc toggle, and
  viewport culling. X-axis is ledger index; the hybrid clock-master
  axis (§13) and simultaneous multi-head lanes are deferred (need a
  per-envelope clock the runtime doesn't emit yet). Built on `@dnd-kit`
  directly rather than `dnd-timeline` (see §13 note).
- **Editable tray + Editing facet (Phase 4).** The wasm crate exposes
  `apply_beat_property` / `apply_move_beat` (over `loom-parser::edit`),
  rebuilt into the committed bundle; `lib/loom-ast.ts` wraps them as
  `useLoomEdit`. The Properties tray's beat-contract fields are now
  editable (+ "add field"), and the Editing-mode bottom dock is
  `studio/BeatTimeline.tsx` — author beats as `@dnd-kit/sortable` chips
  that rewrite `.loom` source on reorder. Both paths write via
  `useWorkspace.updateContents`, so a tray edit or a beat drag *is* a
  source edit (the §10 invariant, end to end).
- **Polish & cleanup (Phase 5).** `studio/TopBar.tsx` — a global
  transport (play / stop / fork / snapshot wired to the session), live
  relay-status dot, and a presence strip — replaces the static header.
  Per-mode layout **reset** (`useMode.reset`); the Production Deliver
  stage shows live relay / workspace / collaborator status. The
  `dockview` shell is gone: `components/dock/*` + `store/presets.ts`
  deleted, the `dockview-react` dependency + its CSS import removed
  (editor CSS bundle 149 KB → 55 KB).

## 9. Why v2

The v1 shell (`editor/src/components/dock/DockShell.tsx`) is VS Code's
model: a vertical **activity bar** of 19 flat panel icons, plus a
`dockview` free-docking canvas. Clicking an icon calls `toggle(id)`,
which `addPanel`/`removePanel`s into the live grid, and dockview's
auto-placement (`positionFor`) drops each new panel into a fresh
column or split. Consequences:

- **Every toggle mutates the spatial layout.** No stable home; the
  arrangement drifts as you work — the "spawns new tabs / inserts new
  columns" annoyance.
- **19 sibling panels, no hierarchy.** Files, Editor, Timeline, Booth,
  Cloud, Graph all rank equally; the user assembles a workspace from
  scratch every session.
- **Presets are an afterthought** — a `<select>` in the StatusBar
  (`store/presets.ts` + `shell/StatusBar.tsx`) that still resolves to
  free-floating dockview arrangements.

DaVinci Resolve's page model is the fix: the workspace is **not** a
freeform canvas but a small set of hand-designed, single-purpose
"pages" switched from a persistent bottom bar. You never drag a panel
in Resolve; each page is built for one job, with the Inspector (right)
and Timeline (bottom) as recurring fixtures.

Crucially, **almost all the content already exists.** The
focus/projection bus (`store/focus.ts`), the detail registry
(`detail/registry.tsx`), the session model (`store/session.ts`), and
every leaf panel are reused as **region contents**. v2 replaces the
frame and keeps the furniture.

## 10. North star

> One app, **five modes**, switched from a persistent **Mode Bar** at
> the very bottom. Each mode is a **fixed, purpose-built layout** of
> resizable regions — not free-floating panels. Switching modes is
> instant and preserves the open project, the live play session, and
> the current focus selection. A **context-sensitive Properties tray**
> lives on the right of every mode; a **clips-on-tracks Timeline**
> docks along the bottom of the modes that need it.

| Mode | Job | Resolve analog |
|---|---|---|
| **Writing** | Author prose + `.loom` source | Edit (text / scripting) |
| **Editing** | Arrange story structure — beats/scenes on tracks + the story graph | Cut/Edit + Fusion |
| **Simulating** | Run & debug the story solo | Playback / preview |
| **Performing** | Live multi-user performance + booth control | Live / multicam |
| **Production** | Export, share, deploy, workspace management | Deliver |

**Single source of truth — edits round-trip to `.loom`.** The project
source stays canonical in every mode. Structural edits made through
*any* surface — dragging a beat on the Editing timeline, editing cast /
disposition in the Properties tray, rewiring diverts on the Graph —
serialize back into `.loom` text (through the parser AST + the Loro
doc), exactly as typing in the editor does. No mode keeps a private
structural model that can drift from source.

## 11. The fixed shell

```
┌──────────────────────────────────────────────────────────────────────────┐
│ TOP BAR   Loom · «workspace»     ‹ transport ⏮ ⏯ ⏭ ⑂ 📌 ›    peers  ⌘K  ⚙ │
├───────────┬──────────────────────────────────────────┬─────────────────────┤
│           │                                          │                     │
│  LEFT     │            CENTER STAGE                  │   RIGHT: PROPERTIES  │
│  RAIL     │      (mode-specific main surface)        │   TRAY  (Inspector)  │
│ (browser/ │                                          │   context-sensitive  │
│   bin)    │                                          │   driven by focus bus│
│           │                                          │                     │
├───────────┴──────────────────────────────────────────┴─────────────────────┤
│  BOTTOM DOCK: TIMELINE — clips on tracks   (Editing · Simulating · Performing) │
├──────────────────────────────────────────────────────────────────────────┤
│  MODE BAR   ✎ Writing   ▦ Editing   ▶ Simulating   ◉ Performing   ⇪ Production │
└──────────────────────────────────────────────────────────────────────────┘
```

Five regions, each driven by a mode-keyed component map:

- **Top bar** — workspace name; global **transport** (play / pause /
  step / stop / fork / snapshot, wired to `session.ts`'s
  `startPlay` / `sendChoice` / `stopPlay` / `forkPlay` / `snapshotPlay`);
  presence avatars; command palette; settings. Mode-agnostic.
- **Left rail ("Bin")** — a mode-specific source browser. Collapsible.
- **Center Stage** — the mode's primary work surface.
- **Right tray (Properties / Inspector)** — persistent, context-sensitive,
  driven by the focus bus (`useFocus.pinned ?? hover`). Collapsible.
- **Bottom dock (Timeline)** — present only in Editing / Simulating /
  Performing. Resizable height.
- **Mode Bar** — the bottom-most strip; the primary navigation.
  Large labelled icon+text targets, active mode lit. Bound to `⌘1`–`⌘5`.

Region sizes persist **per mode** (Editing remembers its tall
timeline; Writing remembers its narrow file rail).

## 12. The five modes

Each row: left rail · center · right-tray default · bottom timeline ·
which **existing** components compose it.

### ✎ Writing
- **Left:** `files/Sidebar` + `files/FileTree` (file bin) + an Outline
  tab (`runner/Outline.tsx`).
- **Center:** `editor/Editor` (`Tabs` + `Breadcrumbs` + CodeMirror).
  Optional vertical split with `canvas/Canvas` as a relationship map.
- **Right tray:** symbol / character / beat properties from the cursor
  via LSP, plus `runner/References.tsx`. (Author-time facet of
  `DetailFor`.)
- **Bottom timeline:** none (optional thin diagnostics strip).
- *The old "Author" preset, made permanent and spatially stable.*

### ▦ Editing — the structural heart (Resolve "Edit"/"Fusion")
- **Left:** new **Story Bin** — beats, scenes, characters, items,
  cohorts, locations (from the bundle / LSP `documentSymbols`). Drag a
  beat → timeline.
- **Center:** the **Story Graph** (`runner/Graph.tsx`, xyflow) as the
  macro "viewer" — beats and diverts as a DAG.
- **Right tray:** editable inspector for the selected beat / clip /
  character (cast, setting, triggers, disposition).
- **Bottom dock:** **Timeline — Editing facet.** Beats/scenes as
  *clips* on character / cohort / location *tracks*; drag to reorder,
  set durations / triggers, retime `at 6am`-style clock gates — every
  such edit rewrites `.loom` source (§10).
- *New mode. Graph + structural Timeline + editable Inspector — where
  "clips on tracks" lives most literally.*

### ▶ Simulating — run/debug solo (the old "Debug" preset, elevated)
- **Left:** `runner/Choices.tsx` + heads list + snapshots.
- **Center:** `runner/Transcript.tsx` (the "program monitor") above
  `runner/World.tsx`.
- **Right tray:** Inspector — latest or pinned envelope / world-key /
  character (`DetailFor`, read facet).
- **Bottom dock:** **Timeline — Run facet** (read-only ledger playback +
  scrubber + right-click fork; today's `runner/Timeline.tsx` evolved).

### ◉ Performing — live multi-user (old "Direct" + "Perform" + booth)
- **Left:** `runner/Cast.tsx` + heads.
- **Center:** live `Transcript` + `Choices`, presence-aware; multi-head
  monitor.
- **Right tray:** `runner/Booth.tsx` live-patch (skip / reload / force) +
  selected head / participant inspector.
- **Bottom dock:** **Timeline — multi-head lanes** (the head-tab + fork
  model already in `Timeline.tsx` / `HeadTabs`).

### ⇪ Production — deliver
- **Left:** `cloud/CloudPanel.tsx` workspaces + relay.
- **Center:** export / share / deploy config (`lib/export.ts`, the
  share-link "Read" preview, `prism loom build` / `serve` affordances,
  `cloud/RemoteEditor`).
- **Right tray:** export & deploy settings, relay status, peers.
- **Bottom timeline:** none.
- *New framing around existing cloud / export plumbing.*

## 13. The Timeline (centerpiece)

Today's `runner/Timeline.tsx` already renders **tracks as rows**
(130px label + lane) and **events as clips** (12px blocks) with cause
arcs and head tabs — but blocks are uniform-width and packed
sequentially (`nextX` per track): no time axis, no zoom, no durations,
no playhead. Target:

| Capability | Today | Target |
|---|---|---|
| X-axis | sequential packing | a **clock ruler** (story time) as master, with **zoom + pan**; ledger index recorded per clip |
| Clips | uniform 12px squares | **extents** (a beat spans BeatEntered→exit), labels, kind-colored |
| Playhead | none | a **scrubber** driving "current envelope" / replay |
| Track headers | label only | collapse / mute-solo / reorder, kind grouping |
| Interaction | hover→focus, right-click fork | + **drag / reorder / retrigger** (Editing facet); **scrub / fork** (Run facet) |
| Branching | head tabs + diamond | head **lane bands** or DAG forks across heads |

**Time semantics (hybrid).** The **story clock is the master ruler** —
clips are positioned by the story time the runtime tracks
(`Time.hour` / `Time.minute`; it already lowers `at 6am` / `at noon`
into `WaitUntilClock`), so the x-axis reads like a real schedule. Each
clip *also* records its **ledger index** (logical emission order) for
stable identity, tie-breaking simultaneous events, and the Run-facet
scrubber. **Cross-track tunnels** — a beat that diverts into another
track and returns (`<-`; the `Tunneled` event) — are drawn as
connectors spanning rows, distinct from ordinary `cause` arcs. Clips =
beats (span), dialogue lines, directives, improv windows (`improv
duration:`); tracks = characters / cohorts / locations / system;
heads = branches.

**Two facets, one component, headless via `dnd-timeline`:**

- **Run facet** (Simulating / Performing) — read-only over the live
  `head.transcript` / `meta` / `tracks`, + scrubber + fork. Drag
  disabled. Low-risk evolution of the existing SVG renderer.
- **Editing facet** (Editing) — authorable: drag beats to reorder /
  retime; resize to set durations / triggers. Every edit rewrites
  `.loom` source (§10) — beats serialize back through the parser AST +
  Loro doc.

`dnd-timeline` is **headless** — it owns the timeframe / pan / zoom /
drag math and sortable rows; we render our own beat/envelope clip
components. That is the right call because our "clips" are narrative
events, not media. Sketch:

```tsx
<TimelineContext range={{ start: 0, end: ledgerLength }}>
  {tracks.map((track) => (
    <Row id={track.id} key={track.id}>          {/* useRow() */}
      {clipsFor(track).map((clip) => (
        <Clip key={clip.id} item={clip}          {/* useItem() */}
              editable={mode === 'editing'} />
      ))}
    </Row>
  ))}
</TimelineContext>
```

Long ledgers/tracks are kept at 60fps via viewport culling (only clips
intersecting the visible scroll range render); `@tanstack/react-virtual`
is the upgrade path if culling proves insufficient.

**Status / deviation (2026-06-01).** The Run facet landed
(`runner/Timeline.tsx`): track gutter + zoomable lane, ruler, clip
extents, playhead, cause-arc toggle, culling. The Editing facet landed
as `studio/BeatTimeline.tsx` (author beats as sortable chips → source
rewrite). **`dnd-timeline` was *not* adopted** — its README is too
sparse to integrate safely and our axis is ledger-index, not
wall-clock, so it bought little; we use **`@dnd-kit` directly** (the
other half of the chosen stack) with a custom renderer.

**Clock axis + multi-head lanes — landed 2026-06-01.** The runtime now
stamps each envelope with a story clock: `EnvelopeMeta.clock` (minutes
since midnight, `runtime/src/ledger.rs`) is filled from the world's
`Time.hour` / `Time.minute` at each `playhead.step` and flows through
the server's play-state into the editor's `meta.clock`. The Timeline
offers a **clock / index axis toggle** (clock auto-selected when ≥2
distinct clock values exist; ruler labelled `H:MM`; beats span by
elapsed time) and a **lanes toggle**: `tracks` (the per-track view of
the primary head) vs `heads` (one lane per live head, a fork diamond at
each head's `forkedFrom` snapshot index, click a lane to make it
primary).

## 14. Properties tray — promoting `DetailFor`

The tray's brain already exists: `detail/registry.tsx` exports
`DetailFor`, a per-`FocusRef`-kind renderer (Envelope / Character /
Track / WorldKey / Beat), and `runner/Inspector.tsx` already delegates
to `DetailFor(pinned)`. v2:

1. **Persistent right region in every mode** (not a summoned dock
   panel / popover). Bound to `useFocus.pinned ?? hover` — hover
   previews, click pins (the bus already works this way).
2. **Per-mode default** when nothing is selected: Writing → symbol at
   cursor; Editing → selected beat; Simulating → latest envelope;
   Performing → selected head / participant; Production → export
   settings.
3. **Editable in author modes** — rename a beat's cast, edit a
   character's disposition, set a clip trigger. Author-time edits
   serialize back to `.loom` source (§10) via the Loro doc; live tweaks
   during a play session go through the booth / runtime. This turns the
   tray from a debugger into Resolve's actual Inspector.
4. **Tray tabs** (Resolve-style): e.g. *Properties · References ·
   History* for one entity.

The popover / modal / side sinks in `focus.ts` stay for *secondary*
peeks; the right tray becomes the *primary* sink.

**Status (2026-06-01):** the read surface landed — tabbed tray (1),
per-mode defaults (2), and tray tabs (4), with author-time detail
sourced from the AST that follows the cursor
(`components/studio/PropertiesTray.tsx` + `lib/loom-ast.ts`). Editable
fields (3) are Phase 4, gated on the wasm `edit` binding.

## 15. Code model

A small new store + a shell component; everything else is reused.

```ts
// store/mode.ts  (new)
type Mode = 'writing' | 'editing' | 'simulating' | 'performing' | 'production'
type ModeUi = {
  cols: number[]          // allotment horizontal sizes [left, center, right]
  rows: number[]          // allotment vertical sizes [stage, timeline]
  leftTab: string
  trayTab: string
  trayOpen: boolean
  timelineZoom: number
}
type ModeState = {
  mode: Mode
  setMode(m: Mode): void
  ui: Record<Mode, ModeUi>     // persisted per mode (localStorage)
  setUi(m: Mode, patch: Partial<ModeUi>): void
}
```

```tsx
// StudioShell.tsx — replaces DockShell
<div className="flex flex-col h-full">
  <TopBar />
  <div className="flex-1 min-h-0">
    <Allotment defaultSizes={ui.cols} onChange={(c) => setUi(mode, { cols: c })}>
      <Allotment.Pane minSize={180} preferredSize={260} snap>
        <LeftRail mode={mode} />
      </Allotment.Pane>
      <Allotment.Pane>
        <Allotment vertical defaultSizes={ui.rows}
                   onChange={(r) => setUi(mode, { rows: r })}>
          <Allotment.Pane><CenterStage mode={mode} /></Allotment.Pane>
          {hasTimeline(mode) && (
            <Allotment.Pane minSize={120} preferredSize={240} snap>
              <TimelineDock mode={mode} />
            </Allotment.Pane>
          )}
        </Allotment>
      </Allotment.Pane>
      <Allotment.Pane minSize={240} preferredSize={320} snap visible={ui.trayOpen}>
        <PropertiesTray mode={mode} />
      </Allotment.Pane>
    </Allotment>
  </div>
  <ModeBar mode={mode} onPick={setMode} />
</div>
```

(`allotment` has no built-in `autoSaveId`; sizes persist via `onChange`
→ `store/mode.ts` → localStorage. Pane collapse uses `visible`; `snap`
gives the snap-to-collapse feel for rails.)

- **Reused unchanged:** `store/focus.ts`, `store/session.ts`,
  `store/workspace.ts`, `detail/registry.tsx`, and every leaf panel
  (`Transcript`, `Choices`, `Ledger`, `World`, `Cast`, `Booth`,
  `Graph`, `Editor`, `FileTree`, …) — now rendered into fixed regions
  instead of dockview groups.
- **Retired:** the activity bar + per-panel toggle keybindings
  (`DockShell.tsx`), the StatusBar preset `<select>`
  (`StatusBar.tsx`), and `presets.ts`'s free-layout concept (becomes
  "saved region sizes within a mode," or is dropped). `dockview` is
  removed once all five modes ship.
- **Keybindings migrate:** `⌘1..5` → modes; transport keys
  (space = play/pause, `[` / `]` = step); per-panel `⌘⇧X` toggles
  retire or become "focus region."

## 16. External libraries (decisions)

| Need | Choice | Notes |
|---|---|---|
| Fixed region layout | **`allotment`** | Resizable split views, min/max/preferred size, snap-to-collapse, `visible` panes; sizes persisted via `onChange` into `store/mode.ts`. |
| Timeline interaction | **`dnd-timeline` + `@dnd-kit/core`** (+ `sortable`, `modifiers`) | Headless timeline (pan / zoom / drag / sortable rows) on dnd-kit; we render our own beat/envelope clips. Fits the DAG/branch model; powers the Editing facet's drag-to-retime. |
| Big-ledger performance | **`@tanstack/react-virtual`** | Headless horizontal + vertical windowing for long ledgers in Timeline / Transcript / Ledger. |
| Keep | `@xyflow/react`, CodeMirror, `zustand`, `loro` | Graph stays xyflow; mode store stays zustand (no `xstate` needed). |
| Rejected | `dockview` (free docking), Twick / Remotion / React Video Editor | Free docking is the problem being removed; the video SDKs are wrong-domain and heavy. `react-resizable-panels` was a viable layout alternative; `allotment` chosen. |

Net new deps: `allotment`, `@dnd-kit/core` (+ `sortable`, `modifiers`),
`dnd-timeline`, `@tanstack/react-virtual`. All small, MIT/permissive,
tree-shakeable.

## 17. Migration phases

1. **Shell swap.** ✅ *Landed 2026-06-01.* `store/mode.ts` +
   `<StudioShell>` (`allotment`) + Mode Bar (`⌘1..5`); each mode's
   existing panel set renders into fixed regions. No leaf-panel
   changes; dockview kept importable for one release as a fallback.
2. **Properties tray.** ✅ *Landed 2026-06-01.* Tabbed right rail with
   per-mode tab sets + defaults; author modes follow the cursor via the
   AST (`PropertiesTray.tsx` + `lib/loom-ast.ts`); runtime modes reuse
   `InspectorPanel`. Editable write-back deferred to phase 4.
3. **Timeline v2 — Run facet.** ✅ *Landed 2026-06-01.* Ruler + zoom +
   pan + playhead + clip extents over the live ledger (read-only) with
   viewport culling (`runner/Timeline.tsx`); ledger-index axis. Built on
   `@dnd-kit`, not `dnd-timeline` (see §13). Clock axis + multi-head
   lanes deferred.
4. **Editing facet + editable Inspector.** ✅ *Landed 2026-06-01.*
   Editing-mode dock is `studio/BeatTimeline.tsx` (author beats as
   draggable chips → `apply_move_beat` → source); the tray's beat
   contract fields edit via `apply_beat_property`. Deferred:
   run-timeline clip drag, declaration-body edits, cloud-doc minimal
   Loro splices.
5. **Polish & deliver.** ✅ *Landed 2026-06-01.* Global transport
   `TopBar` (play / stop / fork / snapshot + relay status + presence
   strip); per-mode layout reset; Production/Deliver status panel;
   `dockview`, the activity bar, and `store/presets.ts` removed +
   `dockview-react` uninstalled.

## 18. Resolved decisions & deferred work

- **Timeline x-axis — hybrid (decided).** Story clock is the master
  ruler; ledger index is recorded per clip; cross-track tunnels are
  drawn as connectors. See §13.
- **Editing-facet write-back — source is canonical (decided; primitive
  landed).** Structural edits in any mode rewrite `.loom` source via the
  parser AST + Loro doc (§10) — no parallel structural model. The
  foundational primitive shipped 2026-06-01 in `loom-parser::edit`:
  spans were already byte-accurate and the comment pre-pass blanks
  rather than drops, so structural edits are pure byte-range splices
  (`TextEdit` / `apply_edits`) that leave untouched lines identical.
  `move_beat` and `set_beat_property` are implemented and tested for the
  minimal-diff invariant. **Wired end-to-end:** the wasm crate's
  `apply_beat_property` / `apply_move_beat` / `apply_insert_beat` /
  `apply_remove_beat` drive editable tray fields, the `BeatTimeline`
  drag, and its `+ beat` / delete affordances; `move_body_item`
  (reorder body items within a beat) is implemented + tested at the
  parser level, UI pending. **Remaining:** `retime` clock gates (no
  clear beat-level clock-gate construct yet — deferred pending a spec),
  a cloud-doc path that applies edits as minimal Loro splices (today
  author edits go through the FSA `updateContents`), and — only for
  edits that genuinely *reformat* rather than splice — a
  trivia-preserving re-emit path.
- **Collab/presence under fixed modes — deferred.** Out of scope for
  now; `mode` is local UI state. Revisit whether peers should see each
  other's mode once the shell lands.
