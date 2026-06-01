# packages/loom

All Loom v3 code lives here, split into sibling crates so the
parser stays independently usable (LSP, codegen, future external
tools) without dragging the runtime + scheduler + Luau bridge along.

| Crate                  | Role                                                            |
|------------------------|-----------------------------------------------------------------|
| [`parser`](./parser)   | Lexer, AST, diagnostics, keyword table, span-preserving structural `edit` API for the `.loom` surface |
| [`runtime`](./runtime) | Bundle, resolver, playhead, ledger, reactive graph, scheduler, directive registry, Luau bridge |
| [`lsp`](./lsp)         | Stdio JSON-RPC server backed by `loom-parser` + a workspace-wide name index |
| [`syntax`](./syntax)   | TextMate grammar generator (driven by `loom-parser::keywords`) + Zed / VSCode extension shells |
| [`wasm`](./wasm)       | `wasm-bindgen` surface for the parser — `parse` / `diagnose` / `emit_tmgrammar` + `apply_beat_property` / `apply_move_beat` structural edits, consumed by the React editor |
| [`server`](./server)   | Multi-user backbone — `loom-relayd` axum server hosting per-workspace Loro CRDTs over `prism-core::network::relay`. See [`docs/dev/loom-multiuser.md`](../../docs/dev/loom-multiuser.md). |
| [`editor`](./editor)   | React/Vite/CodeMirror web IDE — the user-facing front end |
| [`examples`](./examples) | Reference `.loom` projects used by `loom-runtime` integration tests and as authoring tutorials |

The canonical design lives in [`docs/dev/loom-v3.html`](../../docs/dev/loom-v3.html).
Per-crate `lib.rs` docstrings carry the module roadmap and the spec
section each module implements.

## Status

Phase 4 in progress: parser stitches headers / declarations
(structured CHARACTER / TRAIT / STATS / TREE bodies + raw fallback for
every other kind) / beats / dialogue / choices / diverts / fences /
conditional chains (`<if:>/<else if:>/<else>`) / block-opening
directives. Runtime plays the §16 worked example end-to-end with
`Bundle` + `Playhead`, resolves cross-file diverts, dispatches
directive calls (`sfx`/`cue`/`pause`/`anchor`/`fire`/`set`), evaluates
reactive `let` bindings against a `World` scope, runs `<if:>` arms,
expands inline `{expr}` substitutions inside action / dialogue text,
tracks sticky vs. once-only choice consumption, answers `played(name)`
/ `visits(name)` / `since(name)` ledger queries from expressions,
compiles CHARACTER / TRAIT bodies into a `CharacterStore`
(disposition, knowledge, `reacts` tags, goals, hooks), routes
`<set: Character.knows.X …>` and `<set: Character.trusts.Target …>`
mutations through the character store with goal-lifecycle +
threshold-cross hook bookkeeping, and compiles STATS / TREE
declarations into `StatsProfile` / `StatsInstance` / `Tree` with
attribute / axis (`xp_curve` + `narrative_trigger`) / pool (`max` /
`regen` / `cost` with `tick(dt, in_combat)`) / stat (lazy expression)
/ node (`requires` gate) primitives surfaced under dotted paths
(`Wren.strength`, `Wren.level`, `Wren.health`, `Wren.health.max`,
`Wren.damage`, `Wren.tree.armsman_1`).

Phase 5 (Luau bridge) landed 2026-05-25: `loom_runtime::LuauRegistry`
owns an `mlua::Lua` state with a `loom` global (read/write views into
`World` + `Ledger`) and dispatches every non-syntactic directive
(`sfx`, `cue`, `spawn`, `cancel`, `goal`, `broadcast`, `enroll`,
`goto`, `compose`, `heal`, `flash`) through a single Luau call
following the spec §14.1 argument convention (positional first,
single trailing table for named args). Extension authors write
`directive name(args) … end` in a `.luau` file and load it via
`LuauRegistry::load_extension`. The syntactic forms
(`if`/`else`/`match`/`for`/`each visit`/`after`/`otherwise`/
`anchor`/`let`/`set`/`fire`/`shuffle`/`cycle`) stay hand-handled in
the playhead — they affect playhead structure, not user-callable
side effects.

Live-performance layer landed 2026-05-25: `loom_parser` recognises
COHORT / LOCATION structured bodies (label, capacity, ambient,
contains) and the `(improv duration: …, advance on: …)` parenthetical
attached to dialogue cues (spec §13.3). `loom_runtime::LiveStage`
owns participants, cohorts, locations, and an `ImprovController`
with pluggable advancement (`all` / `any` / `quorum(N)`),
emitting `ParticipantJoined` / `ParticipantEnteredLocation` /
`CohortEnrolled` / `ImprovBeatStarted` / `ImprovBeatAdvanced`
ledger events. `BroadcastScope` is an algebraic expression
(`participant(X) | cohort(X) | location(X)` joined with `and` /
`but`) parsed from `<broadcast: …>` directive bodies and evaluated
against the stage's live membership. `broadcast` and `enroll` are
registered as core directive builtins.

SCENE / GENERATOR coroutines + tiered scheduler landed 2026-05-25:
`loom_parser` recognises top-level `SCENE name(params)` declarations
with labelled inner state blocks (lowered to `SceneBody.states`) and
top-level `GENERATOR` declarations with `tier:` / `priority:` / `on
boot` metadata (lowered to `GeneratorBody`). `loom_runtime::coroutine`
lowers both into a flat `Program` of opcodes (`YieldBark`, `YieldChance`,
`WaitUntil`, `WaitDuration`, `Goto`, `Return`, `LoopHead`, `ForBegin`/
`ForEnd`, `EmitLine`) and the three-tier `Scheduler` (focal ≈ 16ms,
active ≈ 100ms, ambient ≈ 2s budgets) drives them round-robin with
focal stealing from active/ambient under load. The bundle pre-lowers
every SCENE/GENERATOR into `bundle.scene_programs` / `generator_programs`
/ `bound_generators` (character-bound, qualified `Character.generator`).
Hook drain at playhead yield points + `spawn`/`run` directive wiring
remain TODO at `packages/loom/runtime/src/playhead.rs` (see follow-up
task).

LSP request loop landed 2026-05-25: `loom_lsp` runs a stdio JSON-RPC
loop backed by a `Workspace` index that reparses on every
`textDocument/didChange` and rebuilds project-wide indices
(characters, traits, beats, anchors, ```todo``` fences). Handlers:
`textDocument/completion` (divert targets / directives / `is` mixins
— spec §14.3), `textDocument/hover` (directive signature stubs,
character property summaries, beat cast+setting), `textDocument/
definition` (jump from divert / cue to declaration), `textDocument/
documentSymbol` (per-file outline). Diagnostics flow straight from
`loom_parser::Diagnostic` to `publishDiagnostics`.

Syntactic-form fillers landed 2026-05-25: `<match: expr>` dispatches
to the first bare-word arm matching the scrutinee's display form
(falls through silently when no arm matches); `<each visit>` picks
`first` / `then` / `finally` based on the enclosing beat's visit
count (1→first, 2→then, 3+→finally); `<after: cond> … <otherwise>`
latches the post-condition body to subsequent visits via per-beat
`Event::AfterLatched`; `<let: name = expr>` introduces an inline
scope-local binding into the surrounding world scope; `<shuffle: a |
b | c>` emits a deterministic pseudo-random variant (ledger-length
modulo until the workspace adds the `rand` crate); `<cycle: a | b |
c>` advances a per-anchor counter keyed by the directive's source
byte offset and emits variants in order.

Typed-slot grammar + ITEM/FACTION composition landed 2026-05-25:
`loom_parser` lifts every property line (`voice: any`,
`home: any of LOCATION`, `range 0 to 100 = 50`,
`unknown | suspects | confirmed`, `list of RUMOUR`,
`map of CHARACTER to int`, `text?`) into a structured `SlotType` on
the new `Property` carrier (spec §8). ITEM and FACTION are first-class
kinds now (`ItemBody` / `FactionBody`) with `is X, Y` inheritance
resolved by a shared `merge_properties`; `Bundle::items` /
`Bundle::factions` index the merged result. Beats split their
parameter list at parse time (`== ask_about(topic, NPC)` →
`Beat.params == ["topic", "NPC"]`) so diagnostics + LSP completion
have the declared param surface. Spec §8's required-hole rule is
enforced at materialisation: `Bundle::rebuild_simulacra` skips any
CHARACTER whose inherited + own typed slots leave an unfilled
`any`-shaped hole and reports
`ProjectDiagnostic::RequiredSlotUnfilled { character, slot }`.
Answer-slot fill at divert call sites (spec §7 + §16) is captured
into `Divert::To::slots` — an indented `<name>:` block under the
divert lowers as a `Vec<BodyItem>` keyed by slot name; the
matching beat-side `slot: <name>` line lands as
`BodyItem::SlotPlaceholder` ready for `<match:>` arm expansion
(playhead wiring is a follow-up — TODO at `lower_item`).

Expression + scoping closures landed 2026-05-25: list comprehensions
(`[c for c in Characters where c.faction == Player.faction]`, spec
§12.1) parse + evaluate through the native expression engine, with
virtual world collections (`Characters` / `Participants` / `Items`)
published via `World::set_collection`. Ledger queries grew the
scoped `since(scope, name)` form and `last(target, speaker)` lookup
(spec §12.2). Coroutines lower `at 6am` / `at noon` / `at 6:30am`
into a `Step::WaitUntilClock` opcode that polls `Time.hour` /
`Time.minute` (spec §10.5), and GENERATOR `start_when` is wired into
`Program.start_when` so the coroutine sits in `Waiting` until the
predicate clears. Multi-speaker cues (`DOCKHAND | FISHER`) split
into `DialogueBlock.speakers: Vec<String>` and ride alongside
`Event::Dialogue.speakers` so live booths can address every
performer. Beat-scope modifier `-> orientation as Participant`
parses into `Divert::To.scope_as` and the playhead overlays
`<scope>.<name>` aliases when evaluating expressions inside the
scoped beat (spec §13.1). Parser surface for `milestones:` on axis
declarations populates `AxisDecl.milestones`, which threads through
to `AxisState.milestones` at instance time.

Simulacra composition + hook coverage landed 2026-05-26: hook events
grew the symmetric `on <verb> drops below N` downward-cross kind, the
`on Participant exits LOCATION` pair to `enters`, and the top-level
`on participant joins` hook fed by `Event::ParticipantJoined`. The
exits derivation is synthesised at hook-drain time from consecutive
`ParticipantEnteredLocation` envelopes (the live stage owns the
state). CHARACTER / TRAIT inheritance learned `on <event>: none`
suppression (spec §9.5) and a `super` body marker (spec §9.4) — both
resolved at `Bundle::rebuild_simulacra` merge time so the runtime
hook list is already composed. Knowledge writes are schema-validated
at `apply_set` time: `bool` and sum (`unknown | suspects | confirmed`)
slots reject out-of-band values via `DirectiveError::BadArgs`, and a
new `Event::KnowledgeChanged { character, field, value }` envelope
replaces the generic `WorldSet` for `Character.knows.*` writes
(spec §10.2). Non-knowledge writes keep `WorldSet` so disposition
threshold crossings continue to fire.

Phase 8 closures (2026-05-26): the playhead now ticks the scheduler
at every yield boundary so `<spawn:>`-launched coroutines genuinely
interleave with the visible beat, and character-bound generators
(spec §10.5) auto-spawn at playhead startup. `<run:>` became a real
awaiting form (`Step::Awaiting { coroutine }`) — it spawns at focal
tier, surfaces ambient yields from the coroutine, and resumes the
surrounding beat once `SceneCompleted` lands; the return value is
stashed on `World["__last_run"]`. Answer-slot fill at divert call
sites (spec §7 + §16) wires through `Frame.slots` →
`Yield::SlotPlaceholder` → caller-body lowering at step time.
`<let:>` bindings are now scope-local (spec §12.1): each frame
captures prior values on first write and restores them when the
frame pops, so a tunnel-local `<let:>` no longer leaks past `<-`.
`<shuffle:>` picks via `rand::seq::SliceRandom::choose` instead of
ledger-length modulo. Booth live-patch substrate (spec §13.4) is in
place: `Playhead::booth_skip_beat` / `booth_force_directive` /
`booth_hot_reload` plus `LiveStage::recast`, with `BeatSkipped` /
`BundleReloaded` / `ParticipantRecast` ledger envelopes for audit.
Divert ambiguity now resolves by preferring a same-file candidate
(spec §18 heuristic) before erroring out. The editor's
`cm-loro.ts` applies remote Loro commits as a minimal
`(from, to, insert)` change (longest common prefix + suffix diff)
so local cursors and selections survive remote edits.

Still to come: the editor-side booth panel (UI scaffold on top of
the new `booth_*` runtime APIs) and the live SCENE / coroutine
tracker view in the web client.

## Multi-user (Phases 1–8 landed)

`loom-server` is the sibling crate that backs collaborative authoring
of `.loom` projects. It depends only on `prism-core` (not `prism-relay`)
so the binary stays small. Phases 1–5 cover the wire layer (module
wiring + `/api/health`, auth + multi-workspace REST + capability
tokens, WebSocket sync, presence fan-out, client glue), Phase 6 adds
the FSA export/import fallback, Phase 7 hosts the play loop server-side,
and **Phase 8** turns the relay into a single-binary deployment: it
serves the React editor's `dist/` alongside the API / WS routes, and
the editor defaults to `window.location.origin` when same-origin. Full
roadmap: [`docs/dev/loom-multiuser.md`](../../docs/dev/loom-multiuser.md).

### Self-hosting the editor (Phase 8)

A complete Loom deployment is one binary:

```
prism loom build            # vite build + cargo build -p loom-server
prism loom serve            # boots loom-relayd serving editor + API + /ws
# → open http://127.0.0.1:7878 in any browser, register, author
```

Useful flags on `prism loom serve`:

- `--bind 0.0.0.0:7878` — accept connections from the LAN.
- `--editor-dist <path>` — override the default
  `packages/loom/editor/dist`.
- `--cors permissive` — opt in to cross-origin requests (only needed
  when driving the relay from the Vite dev server on `:5173`).
- `--build` / `--ship` — rebuild first; `--ship` switches to the
  release-profile binary.

Equivalent bare-cargo invocation (no CLI wrapper): `cargo run -p
loom-server --bin loom-relayd -- --editor-dist
packages/loom/editor/dist`.

### Dev-loop (no self-hosting)

One command starts both servers:

```
prism loom dev
# → Editor (HMR): http://127.0.0.1:5173
# → Relay (API+WS): http://127.0.0.1:7878
```

The CLI prebuilds the wasm bundle + relay binary, then runs Vite
(`pnpm dev`) + `loom-relayd --cors permissive` under the prism
supervisor (colored prefixed logs, Ctrl+C fan-out). The editor reads
`VITE_LOOM_RELAY` so the API + WS URLs always hit the right port even
with `--ui-port` / `--relay-port` overrides.

Useful flags: `--host 0.0.0.0` (LAN), `--ui-only` / `--relay-only`,
`--no-wasm` (skip the wasm preflight), `--ship` (release relay).

Manual two-process equivalent, if you want to drive each half
yourself:

```
pnpm --filter loom-app dev    # Vite at :5173 (HMR)
cargo run -p loom-server --bin loom-relayd -- --cors permissive
                              # API + WS at :7878
```

If you change `parser` / `runtime` / `lsp` / `wasm` without going
through `prism loom dev`, rebuild the editor's wasm bundle so the
LSP / lint surfaces pick up the change:

```
pnpm --filter loom-app wasm:build:dev   # fast, larger output
pnpm --filter loom-app wasm:build       # release, slower
```

### Full IDE walkthrough

The React editor under `packages/loom/editor` now hosts every
Run/Debug surface the simulator used to (Transcript / Ledger /
Timeline / World / Inspector / Detail / Cast / Booth / Graph /
Outline / References) plus workspace presets, the focus + projection
bus, multi-head branching play, and booth live-patch over the relay.

**Shell (v2, phases 1–4 landed):** those surfaces now live inside a
modal **Studio** shell — five modes (Writing / Editing / Simulating /
Performing / Production) on a bottom Mode Bar (`⌘1..⌘5`), each a fixed
`allotment` layout — replacing the dockview activity-bar + workspace
presets. A tabbed, cursor-following Properties tray; a clips-on-tracks
Run-facet Timeline; and an Editing facet where editing tray fields or
dragging beat chips rewrites `.loom` source through the parser's
`edit` ops. See [`docs/dev/loom-ide-redesign.md` Part II](../../docs/dev/loom-ide-redesign.md).

See [`docs/dev/loom-ide-redesign.md` §0](../../docs/dev/loom-ide-redesign.md#0-running-the-ide)
for the launch flows, the step-by-step "drive the full simulator
inside the IDE" walkthrough, and the keybinding reference.
