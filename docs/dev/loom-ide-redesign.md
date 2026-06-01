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
