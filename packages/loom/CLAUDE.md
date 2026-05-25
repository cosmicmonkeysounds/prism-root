# packages/loom

All Loom v3 code lives here, split into sibling crates so the
parser stays independently usable (LSP, codegen, future external
tools) without dragging the runtime + scheduler + Luau bridge along.

| Crate                  | Role                                                            |
|------------------------|-----------------------------------------------------------------|
| [`parser`](./parser)   | Lexer, AST, diagnostics, keyword table for the `.loom` surface  |
| [`runtime`](./runtime) | Bundle, resolver, playhead, ledger, reactive graph, scheduler, directive registry, Luau bridge |
| [`lsp`](./lsp)         | Stdio JSON-RPC server backed by `loom-parser` + a workspace-wide name index |
| [`syntax`](./syntax)   | TextMate grammar generator (driven by `loom-parser::keywords`) + Zed / VSCode extension shells |
| [`wasm`](./wasm)       | `wasm-bindgen` surface for the parser — `parse` / `diagnose` / `emit_tmgrammar` consumed by the React editor |
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

Still to come: SCENE / GENERATOR coroutines, tiered scheduler,
booth live-patching UX (spec §13.4), the four remaining axis modes
(`use_tracking` / `point_buy` / `milestone` / `sdk_controlled` — see
TODO at `packages/loom/runtime/src/meridian.rs` ~`AxisMode::PointBuy`),
and the LSP request loop.

## Multi-user (Phases 1–5 landed)

`loom-server` is the sibling crate that backs collaborative authoring
of `.loom` projects. It depends only on `prism-core` (not `prism-relay`)
so the binary stays small. Phase 1 (module wiring + `/api/health`),
Phase 2 (auth + multi-workspace REST + capability tokens), Phase 3
(WebSocket sync over `/ws`, broadcast fan-out), Phase 5 (server-side
presence fan-out on the same socket), and Phase 4 (client glue:
`loom-wasm::LoomDoc` wraps `loro::LoroDoc`; `editor/src/lib/sync.ts`
speaks the envelope protocol over `WebSocket`; `editor/src/lib/auth.ts`
+ `presence.ts` + a minimal `cm-loro.ts` CodeMirror binding) are in.
The Zustand workspace store still owns IDB / FSA persistence — the
`LoomDoc` capability is exposed but not yet the canonical store; that
swap is a follow-up. Full roadmap:
[`docs/dev/loom-multiuser.md`](../../docs/dev/loom-multiuser.md).

Run locally: `cargo run -p loom-server --bin loom-relayd` (defaults to
`127.0.0.1:7878`).
