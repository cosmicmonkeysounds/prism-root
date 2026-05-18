# IDE Mode in PRUI

Scoping doc for turning the existing `shell.code-editor` panel into a
real in-shell IDE — multi-file project navigation, symbol intelligence,
CRDT-document inspection, jump-to-definition, and friends. Written
**2026-05-16** after the editor-unify pass (text-input declarations,
shared dispatch, palette/search through `TextEditor`).

This doc scopes the work; it does not implement it. Phased plan at the
end so chunks can land independently.

> **Cross-cutting alignment.** `docs/dev/prism-cross-cutting-systems.md`
> ranks the substrate work by leverage. Most IDE-mode phases below
> are the **user-visible surface** of cross-cutting tiers: Phase 3
> (Diagnostics panel) materialises Tier 1 §3.3 (type/diagnostics
> toolchain); Phase 4 (Inspector) is the same panel as Tier 2 §4.3
> (Inspector/DevTools) — they should ship as one panel hosting
> multiple lenses (CRDT tree, presence, probe stream, binding
> values), not as two. Phase 2 (Symbol index) is the diagnostics
> toolchain's natural extension into navigation. The combined
> ordering at the end of this doc reflects both this plan's UI
> phasing and the cross-cutting doc's substrate priorities.

## Status

| Phase | State | Landed |
|---|---|---|
| 1 — Project tree + open-path | ✅ shipped | `shell.explorer` rewrite, `FileNode::path`, `explorer-row` route, 5 e2e tests, `IdeExplorer` scene |
| 2 — Symbol index + Go-to-Symbol palette | ✅ shipped | `prism_core::language::symbol_index` (`Symbol`/`SymbolIndex`, 8 tests); `AppState::index` (`IndexSlot`) rebuilt per-file on `editor.file.save` + wholesale on `project.open-folder` / `Shell::{open,poll}_project`; `editor.go-to-symbol` (Ctrl+T) + `editor.symbol-{next,prev}`; `shell.symbol-palette` block + `symbol-palette` text-input declaration (fuzzy, Enter/click jump via `editor_files::open_at_offset`); `symbol-row` pointer route. **Residual:** Ctrl+click jump-to-def in the editor body (the palette is the shipped surface); arrow-key result nav shares the search-overlay limitation. |
| 3 — Diagnostics panel + squiggles | not started (blocked on Tier 1 §3.3 `luau-analyze`) | — |
| **4 — Inspector / DevTools panel** | ✅ **shipped** | `shell.devtools` block with 4 lenses (Document / Presence / Probes / Bindings), tabbed switcher, declarative filter via `TextInputDeclaration`, `DevToolsService` commands, scene + 12 e2e tests, `PanelKind::DEVTOOLS` |
| 5 — Folding + bracket-match + inlays | not started | — |
| 6 — Find/replace across project | not started | — |
| 7 — IDE workflow + split + persistence | not started | — |

### Phase 4 — what's wired vs what's stubbed

**Wired end-to-end:**
- All four lenses render via `DevToolsSlot::devtools_props` (in `state.rs`).
- Tab switching: click → `events.rs::handle_devtools_tab_click` → `DevToolsSlot::switch_lens`.
- Filter field: one `TextInputDeclaration` row in `services/text_input/service.rs`. Typing routes through the shared dispatch primitive; Escape blurs via the declaration's `on_cancel` hook.
- Document lens: walks `state.canvas.document` (the live `BuilderDocument`), each row routes to canvas selection on click.
- Bindings lens: enumerates `props::builtin_binding_tags()` (the SLOT_BINDINGS table).
- Probe buffer: capped-FIFO (`PROBE_BUFFER_LIMIT = 200`), `record_probe` / `clear_probes` API + `devtools.clear-probes` command.
- Presence buffer: `Vec<PresencePeer>`, scene-seeded, ready for `PresenceManager` ingest.
- `DEVTOOLS` panel kind registered in `prism-dock`; reachable via `workspace.ensure_panel_visible("devtools")`.

**Stubbed (future wiring):**
- **Probe firing.** The runtime can fire probes (`LuauScopeFrame::fire_probe`), and the buffer captures events, but the host event-router doesn't yet fire probes off a `data-probe-*` hit. When that wiring lands (cross-cutting §4.3), `record_probe(...)` is the seam.
- **Presence ingest.** `PresenceManager` exists in `prism-core::network::presence`, but no shell service consumes its `PresenceChange` events into `state.devtools.presence`. Adding a `PresenceService` that subscribes to a `PresenceManager` instance and `replace_or_insert`s into the vec is one self-contained service file.
- **Binding values.** The Bindings lens shows tag names; the JSON values are placeholders. A `PropCtx`-aware closure could snapshot the latest emission into `binding_snapshots: IndexMap<String, Value>` for full value rendering.

---

## What already works

The substrate is more complete than the missing UI suggests. The next
few sections are an honest map of "what's here" vs "what's missing"
so the gap list is grounded in reality.

### Editor engine — `prism_ui_runtime::editor::TextEditor`
- String buffer, byte-indexed caret, selection anchor.
- Undo coalescing (`EditGroup::Typing/Backspace/Delete/Atomic`).
- IME preedit + commit + enabled/disabled.
- Bracket-pair auto-close (single-line off, code-panel on).
- Click history (single / double-word / triple-line).
- Page nav with preferred-column preservation.
- Language-aware comment toggle (`line_comment_prefix` covers Luau,
  Rust, JS/TS, Python, etc.).
- 60+ public methods, ~2,400 LOC, exercised by 46 e2e integration
  tests in `prism-shell/tests/code_editor_e2e.rs`.

### Code panel — `shell.code-editor`
- DSL-defined (`packages/prism-shell/ui/components/code-editor.prism-ui`),
  rendered through the runtime's `<input multiline="true" syntax-language="..."/>`.
- Backed by `CanvasSlot::code_buffer` + `code_tabs` (multi-file tab
  strip).
- Syntax highlighting via `prism_ui_runtime::syntax` — Luau / Rust /
  JS / TS tokenizer with a memoised span cache.
- Auto-scroll-to-caret, drag-select, shift-click extend, double / triple
  click word/line selection.
- File ops via `EditorFilesService`:
  `editor.file.{new, open, save, save-as, close, next-tab, prev-tab}`.
- Live status strip (caret line/col, language).

### Text-input dispatch (the unified seam)
- `services::text_input::dispatch_text_input` — pure `Event →
  TextEditor` plumbing.
- `services::text_input::TextInputDeclaration` — declarative spec rows
  (id, slot getter, `is_active`, hooks, bindings, modal capture).
- `DeclarativeTextInputService` walks declarations, dispatches.
- Four `TextEditor`-backed surfaces today: property-row inline edits
  (`FieldFocus`), `shell.code-editor` panel, command palette query,
  search overlay query.

### Luau language intelligence — `prism_core::language::luau`
- **Full-moon-backed AST parser** (`parser.rs`).
- `LuauSyntaxProvider` (`provider.rs`) returns:
  - `diagnostics(source)` — parse errors + simple semantic checks.
  - `completions(source, position)` — keyword + builtin completions
    plus signal-aware `on_{signal}` handler completions when a
    `SchemaContext` is in scope.
  - `hover(source, position)` — builtin docs + signal payload type
    info.
- Bidirectional Luau ↔ visual-graph bridge (`visual.rs`,
  `EventListener` node kind).
- 41 unit tests across parser / provider / visual.

### Other languages
- `prism_core::language::syntax` — generic AST + Scanner used by every
  parser.
- Markdown contribution (`language::markdown`) — narrow in-house dialect.
- PRUI grammar (`language::prism_ui::grammar`) — DSL parser.
- PRSS stylesheet parser.
- No Rust LSP, no TS LSP, no LSP client. That's its own project.

### CRDT substrate
- `prism_core::foundation::persistence::CollectionStore` wraps a
  `LoroDoc` with `objects` + `edges` maps.
- `VaultManager` orchestrates `PrismManifest`'s collections against a
  `PersistenceAdapter`.
- `kernel::crdt_sync::CrdtSync` mirrors object / edge mutations into
  per-id reactive `Atom`s.
- Loro snapshots are JSON-string-bearing maps; round-trip with the
  legacy TS runtime works.

### Project / explorer
- `shell.explorer` panel exists and renders `state.project.files`
  (flat list today).
- `ProjectService` owns `project.{open-folder, close-folder}` and
  ingests files into `state.project.files` via VFS.
- The "Code" workflow page shows `shell.explorer` next to
  `shell.code-editor`.

### Apps
- `apps/<id>/` directories hold per-app source: `manifest.toml`,
  optional `shell.prism-ui`, `main.luau`, asset folders.
- Hot-reload watcher (`hot_reload.rs`) picks up `app.prism-ui` and
  each `apps/<id>/shell.prism-ui` edit; `.luau` source isn't watched
  yet.

---

## Where the gaps are

Honest enumeration. Group by feature area; some are big, some are
half-day landings.

### Multi-file navigation
- **Project tree component.** `shell.explorer` is a flat list; a real
  tree (folders, expand/collapse, drag-reorder, file rename inline)
  doesn't exist. PRUI primitives support nesting; would land as a
  new `shell.project-tree` block reading from a new
  `ProjectSlot::file_tree` field. ~1-2 days.
- **File→buffer routing.** Clicking a tree entry should open it in
  the code panel — today the only way is `editor.file.open` via a
  file picker. Need a `code-editor.open-path` command + routing from
  the explorer. ~½ day.
- **Recent files / quick-open palette.** Ctrl+P "Go to File…" surface,
  driven by a fuzzy-match over the project tree. Builds on the
  declarative text-input system added in the editor-unify pass —
  one `TextInputDeclaration` + a result-renderer block. ~½ day.

### Symbol intelligence
- **Symbol index.** Walk every `.luau` file in `state.project.files`,
  run `LuauSyntaxProvider::parse`, extract a flat `Vec<Symbol { name,
  kind, path, range }>`. Index lives on a new `IndexSlot` or as a
  `kernel::symbol_index` module. Refresh on file save. ~1 day.
- **Jump-to-definition.** Ctrl+click on an identifier → resolve via
  symbol index → open buffer + place caret. Need an `events.rs`
  pointer arm + a `code-editor.jump-to-symbol` command. ~½ day.
- **Find references.** Reverse-index: for each identifier, the list
  of (path, range) sites that reference it. Adds a result-list
  overlay (`shell.references-panel`) reading from a new slot. ~1 day.
- **Hover docs.** Already partially wired — `editor_help` registry
  drives keyword hover; extending to project symbols means looking
  up the hovered ident in the symbol index. ~½ day.
- **Symbol palette.** Ctrl+Shift+O "Go to Symbol in File / Workspace".
  Another `TextInputDeclaration`. ~½ day.

### CRDT / Loro document inspector
- **Doc tree view.** A `shell.crdt-inspector` panel that renders the
  active `CollectionStore`'s objects + edges as an expandable tree.
  Tap an object → inspector reads its JSON-string entry → property
  rows render via the existing builder property panel. ~1-2 days.
- **Live diff overlay.** Subscribe to `kernel::crdt_sync::SyncEvent`
  and paint a brief flash on each mutated node. Wave-9.x animator
  scaffolding can drive this. ~½ day.
- **Peer presence overlay.** `network::presence::PresenceManager`
  already tracks remote cursors + selections. A `shell.presence-overlay`
  paints other peers' cursors in the code panel using their TextEditor-
  shaped state. ~1 day.

### Diagnostics / problems panel
- **Diagnostics surfacing.** `LuauSyntaxProvider::diagnostics` returns
  `Vec<Diagnostic>` per file. Aggregate across `state.project.files`
  into a new `DiagnosticsSlot`. Render as a panel + paint squiggles
  in the code-editor input via a new `diagnostics="..."` attribute
  on `<input>`. ~1-2 days (squiggle rendering is the slow part).
- **Quick-fix actions.** Per-diagnostic actions live on the
  `LuauSyntaxProvider` already (as completion-style codeActions in
  the LSP shape). Surface through a Cmd+. popover. ~1 day.

### Editor surface gaps
- **Code folding.** `TextEditor` doesn't model fold ranges. The dead
  `prism-core::editor::fold` module I deleted had this; adding it
  back as a `Vec<(start_line, end_line, folded: bool)>` on
  `CodeBuffer` + a gutter renderer in `<input>` is ~1-2 days.
- **Minimap.** Need a downsampled rasterisation of the buffer's
  highlight spans. ~1-2 days (depends on whether we want pixel-perfect
  or just span-color blocks).
- **Multi-cursor.** Editor today has one caret + one selection. Multi-
  cursor is a deep refactor — `Vec<(caret_byte, anchor)>` plus
  every edit op needs to be set-aware. ~3-5 days.
- **Bracket / scope highlighting.** Brace under the caret already
  matched via `TextEditor::matching_bracket_for`; need to surface in
  the render through a new `bracket-match` attr (the runtime already
  parses it — just not wired by the host binding). ~½ day.
- **Inlay hints.** Show inferred types / param names inline. Requires
  symbol index + a new render attr. ~1-2 days.

### Layout / workflow surfaces
- **"IDE mode" workflow page.** A new entry in the workflow-page bar
  that arranges `shell.project-tree | shell.code-editor |
  shell.diagnostics-panel` (or whatever final mix). ~½ day.
- **Tabs persistence.** Save the open buffers + caret positions per
  workspace, restore on reopen. Adds rows to the workspace's
  serialized snapshot. ~½ day.
- **Split editor.** Today the code-editor panel is a single split. A
  two-pane split (left buffer / right buffer) needs the dock workspace
  to host two `shell.code-editor` instances pointed at different tabs.
  ~1-2 days.

### Search
- **Find in files.** Today's `shell.search-overlay` searches the
  active builder document only. A real "find in project" reads
  `state.project.files`, runs the same scorer over each file's text,
  presents grouped results. ~1 day.
- **Replace.** A second TextEditor on the search overlay for the
  replacement string + Enter-on-match performs the replace through
  the editor's `insert("…")` path. ~1 day.

---

## Phased plan

Order by dependency + visible payoff. Each phase is roughly 1 week of
focused work; can be parallelized once foundations land.

### Phase 1 — Project tree + file→buffer routing
Block on: nothing.

1. New `ProjectSlot::file_tree: FileTree` struct (nested `Folder { name,
   children: Vec<Node> }` / `File { name, path }`).
2. `prism_shell::components::project_tree` block + `shell.project-tree`
   DSL.
3. `code-editor.open-path` command.
4. Click route in `events.rs` for tree-entry hits.

Unlocks: navigating an app's source tree without using `Ctrl+O`.

### Phase 2 — Symbol index + jump-to-definition
Block on: Phase 1 (need the file list).

1. `prism_core::language::symbol_index` module — walks project files,
   parses via `LuauSyntaxProvider`, emits `Vec<Symbol>`.
2. `IndexSlot::symbols` on `AppState`.
3. Refresh hook on `editor.file.save` + on `project.open-folder`.
4. `code-editor.jump-to-symbol(name)` command.
5. Ctrl+click pointer arm in `events.rs`.
6. **Symbol palette** as a new `TextInputDeclaration` (the system
   added in the editor-unify pass) + a `shell.symbol-palette` block.

Unlocks: Ctrl+P / Ctrl+Shift+O / Ctrl+click in Luau code.

### Phase 3 — Diagnostics panel + inline squiggles
Block on: Phase 2 (symbol-aware diagnostics).

1. `DiagnosticsSlot::per_file: HashMap<PathBuf, Vec<Diagnostic>>`.
2. Refresh hook on save (alongside Phase 2's index refresh).
3. New `<input diagnostics="…"/>` attr + runtime renderer support
   (squiggle paint via `prism_ui_runtime::layout::compute`'s glyph
   walk).
4. `shell.diagnostics-panel` block reading from the slot.
5. Click-on-diagnostic → `code-editor.open-path` + place caret.

Unlocks: errors visible inline + in a panel.

### Phase 4 — CRDT inspector + presence overlay
Block on: nothing (parallelizable with 1–3).

1. `shell.crdt-inspector` block walking `CollectionStore::objects` +
   `::edges`. Renders as an expandable tree against the existing
   property-panel pipeline.
2. Hook `kernel::crdt_sync::SyncEvent` to mark inspector entries
   dirty + flash.
3. `shell.presence-overlay` reads `network::presence::PresenceManager`
   peer cursors and paints them over the active editor.

Unlocks: the Loro doc structure becomes a navigable surface; remote
collaborators visible in real time.

### Phase 5 — Code-folding + bracket-match + inlay hints
Block on: Phase 2 (inlays need symbols).

1. Re-introduce a `FoldState` on `CodeBuffer` — re-port the deleted
   `prism-core::editor::fold` module against the new `TextEditor`
   (the old impl was rope-buffered).
2. Surface `bracket-match` attr on the code-editor binding (runtime
   already parses it).
3. Add an `inlays="…"` attr to `<input>` that takes a sparse vec of
   `(byte_offset, label)` pairs; renderer paints them faintly inline.

Unlocks: standard IDE editor affordances.

### Phase 6 — Find in files + replace
Block on: nothing.

1. Extend `SearchService` with a `search.scope` (active doc /
   project / workspace).
2. New "replace" `TextInputDeclaration` paired with the existing
   search declaration via a shared `is_active`.
3. Grouped result rendering in `shell.search-overlay` (collapsible
   per-file groups).

Unlocks: refactoring across the project.

### Phase 7 — Workflow + split editor + tabs persistence
Block on: 1–3 land.

1. New `WorkflowPage::Ide` enum variant + dock layout (`shell.project-tree
   | shell.code-editor | shell.diagnostics-panel`).
2. Dock workspace gains support for two `shell.code-editor` panes
   reading from disjoint `code_tabs` slots.
3. Workspace serializer rows for open buffers + caret positions.

Unlocks: a polished "IDE" experience.

---

## Open questions

- **LSP integration?** For Rust / TS, do we want to wrap a host LSP
  server or stay Luau-only? Wrapping LSP is a long road (capability
  dance, sync state, etc.); Luau-only keeps the substrate consistent
  but limits the IDE to one language. Not blocking Phase 1–3.

- **Squiggle rendering.** The femtovg backend doesn't have a primitive
  for wavy underlines. Either (a) emit a `RenderCommand::WavyUnderline`
  and rasterise in `paint.rs`, or (b) approximate with the existing
  underline + a dashed dash-pattern. (a) is correct, (b) is faster
  to ship.

- **Symbol index across `.prism` documents.** PRUI / PRSS source has
  its own grammar; should the symbol index unify those into the same
  `Vec<Symbol>` or maintain per-language indices? Probably unify with
  a `language: &'static str` discriminator.

- **Persistence of IDE state.** Tabs, fold state, caret positions per
  buffer — where do they live? Per-workspace, per-app, per-user? The
  CRDT layer makes per-app collaborative state cheap.

---

## What this doc deliberately does NOT cover

- **Visual scripting / node-graph editing.** The `LuauVisualLanguage`
  bridge exists but the graph editor UI is its own surface, not part
  of the IDE-mode work.
- **Debugger.** A Luau debugger (breakpoints, stepping, locals
  inspection) is a multi-week project with its own substrate work
  (mlua hooks, source-map registry, paused-frame UI). Out of scope.
- **AI-assist.** `kernel::intelligence` carries `AiProviderRegistry`
  + `OllamaProvider` / `ExternalProvider`; integrating inline
  completions / chat panels is a separate plan.

---

## Unified sequence (with cross-cutting)

Each IDE phase maps to one or more cross-cutting tiers from
`prism-cross-cutting-systems.md`. The phases that surface substrate
features can't ship before the substrate work; the phases that don't
can land in parallel.

| Phase | UI surface | Substrate (cross-cutting) | Blocking |
|---|---|---|---|
| **1** Project tree + open-path | `shell.project-tree`, `code-editor.open-path` | — | nothing |
| **2a** Symbol index | (no new UI, indexer subsystem) | — | Phase 1 |
| **2b** Ctrl+P / Ctrl+Shift+O / Ctrl+click | new `TextInputDeclaration`s, click router | — | Phase 2a |
| **3a** `luau-analyze` integration | — | Tier 1 §3.3 type/diagnostics toolchain | external bin on PATH |
| **3b** Diagnostics panel + squiggles | `shell.diagnostics-panel`, `<input diagnostics="…"/>` | Tier 1 §3.3 | Phase 3a |
| **4** Inspector panel | `shell.inspector` with lenses: CRDT tree, presence, probe stream, binding values + types | Tier 2 §4.3 Inspector/DevTools | probe event-router wiring (cross-cutting §4.3) |
| **5** Folding + bracket-match + inlays | runtime renderer additions | — | Phase 2a (inlays need symbols) |
| **6** Find/replace across project | extend `shell.search-overlay` with replace + scope | — | Phase 1 (needs file list) |
| **7** IDE workflow + split + persistence | new `WorkflowPage::Ide`, two-pane code-editor, persisted tabs | — | 1–3 land first |

**Combined high-value MVP**: Phases 1 + 2 + 3 (~2-3 weeks). With those,
an app dev can browse their app's source, jump-by-symbol, and see
errors at edit time. Phase 4 lands the cross-cutting Inspector
(arguably higher leverage than 2–3 once the substrate's there).
Phases 5–7 are polish.

## Estimate summary

| Phase | Focus | Effort |
|---|---|---|
| 1 | Project tree + open-path | ~3-4 days |
| 2 | Symbol index + Ctrl+P / Ctrl+Click | ~4-5 days |
| 3a | `luau-analyze` integration | ~3-4 days |
| 3b | Diagnostics panel + squiggles | ~3-4 days |
| 4 | Inspector panel (CRDT + probes + presence + types) | ~4-6 days |
| 5 | Folding + bracket-match + inlays | ~3-5 days |
| 6 | Find/replace across project | ~2-3 days |
| 7 | IDE workflow + split + persistence | ~3-4 days |

**Total**: ~4-6 weeks of focused work to a polished IDE mode. The
unify-editors pass we just landed is the precondition for most of
this — every new text input (Ctrl+P, symbol palette, replace field,
diagnostics filter, etc.) is one `TextInputDeclaration` row, not a
fresh service.
