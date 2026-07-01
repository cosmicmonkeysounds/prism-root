# packages/loom

All Loom v3 code lives here, split into sibling crates so the
parser stays independently usable (LSP, codegen, future external
tools) without dragging the runtime + scheduler + Luau bridge along.

| Crate                  | Role                                                            |
|------------------------|-----------------------------------------------------------------|
| [`parser`](./parser)   | Lexer, AST, diagnostics, keyword table, span-preserving structural `edit` API for the `.loom` surface |
| [`runtime`](./runtime) | Bundle, resolver, playhead, ledger, reactive graph, scheduler, directive registry, multi-head play `session`, Luau bridge |
| [`lsp`](./lsp)         | Stdio JSON-RPC server backed by `loom-parser` + a workspace-wide name index |
| [`syntax`](./syntax)   | TextMate grammar generator (driven by `loom-parser::keywords`) + Zed / VSCode extension shells |
| [`server`](./server)   | Multi-user backbone — `loom-relayd` axum server hosting per-workspace Loro CRDTs over `prism-core::network::relay`. See [`docs/dev/loom-multiuser.md`](../../docs/dev/loom-multiuser.md). |
| [`editor`](./editor)   | React/Vite/CodeMirror web IDE — the user-facing front end |
| [`core`](./core)       | Native **TypeScript** port of Loom (no WASM, no Prism): parser (incl. authored **`SPACE`/`CHANNEL`** chatroom declarations) + a first-principles social-ecosystem `runtime/sim` + a parser-only **`lsp`** language surface (`@loom/core/lsp` — `Workspace` with completion / hover / definition / documentSymbols / references / diagnostics, the in-process replacement for the wasm `LspWorkspace`) + an SSE/REST **event server** (`pnpm serve`) that hosts a live `Sim` for LAN events, composing its `SimEvent` stream into server-authoritative, channel-routed chat (`server/chat.ts`) with spaces + Slack-style threads (`parentSeq`), **access control** (open / private-invite / faction / group / dm channel membership, journaled `inviteToChannel`/`leaveChannel`, guest↔guest invites) + a **pluggable channel-type registry** (`runtime/sim/channel-types.ts` — per-type post policy / threadability / broadcast routing / slow-mode / ephemeral, e.g. a read-only `announcement` feed), a scoped invite roster (`rosterFor`), history + moderation, and a journaled `say` command so participant-typed chat replays deterministically. vitest-tested. |
| [`play`](./play)       | The **participant React app** (Vite, name `loom-play`) guests + performers use at a live event — an **AOL-chatroom-skinned** client with Discord-style **spaces** (sidebar sections, incl. authored `SPACE`s), Slack-style **message threads** + consecutive-sender banner grouping, **hybrid typed chat** (a composer wired to `/api/*/say`), and **access-controlled rooms** (open/private/faction/group/dm with invite + leave) layered over the story-injected lobby + faction + DM channels (decisions docked per-thread, re-login history) on the `core` server's SSE/REST. `pnpm dev` (:5174, proxies to the server on :7000) / `pnpm build` (served by the event server at `/`). |
| [`examples`](./examples) | Reference `.loom` projects used by `loom-runtime` integration tests and as authoring tutorials |

The canonical design lives in [`docs/dev/loom-v3.html`](../../docs/dev/loom-v3.html).
Per-crate `lib.rs` docstrings carry the module roadmap and the spec
section each module implements.

## JS/TS workspace (core · play · editor)

The three JavaScript packages share a **loom-local pnpm workspace**
([`pnpm-workspace.yaml`](./pnpm-workspace.yaml)) — independent of the
prism-root workspace (which only covers `prism-studio`). One install
covers all three; run scripts with `pnpm --filter <name>`:

```bash
cd packages/loom && pnpm install            # core + play + editor
pnpm --filter @loom/core test               # the TS engine suites
pnpm --filter @loom/core serve              # the LAN event server (:7000)
pnpm --filter loom-play dev                 # participant app, HMR (:5174)
pnpm --filter loom-app  dev                 # the editor, HMR (:5173)
```

Each package has its own README: [`core`](./core/README.md) (engine +
event server), [`play`](./play/README.md) (the participant app),
[`editor`](./editor/README.md). The Rust crates are built via Cargo, not
pnpm — they are not in this JS workspace.

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

Client-side local play landed 2026-06-01: the multi-head session
engine moved out of `loom-server` into `loom_runtime::session`
(`PlaySession` + the `PlayStateSnapshot` / `HeadSnapshot` view structs),
so the server's `PlayHub` and the new wasm `LoomSession` drive the
*identical* loop and emit the *identical* `play-state` JSON. The editor
now plays any `.loom` workspace entirely in the browser with no relay
and no account (`store/session.ts` `kind: "local"`, `lib/local-play.ts`,
`lib/example-project.ts`); the relay path stays for collaboration
(`kind: "cloud"`). Because the Luau VM can't target
`wasm32-unknown-unknown`, `directives::Registry` runs in a `lenient`
mode on no-Luau builds: a Lua-defined directive or `.luau` extension
degrades to a logged `Event::Directive` envelope instead of aborting
the session (native `luau` builds stay strict). The narrative engine
itself — beats, choices, diverts, conditionals, world, characters,
stats, the core Rust directives (`set`/`sfx`/`cue`/`broadcast`/`cast`/…)
— is full-fidelity in the browser.

Functional-redesign Slice 1 landed 2026-06-30 (TS `core/` only so far):
the `is X, Y` inheritance merge is now actually run on the sim compile
path — `compileModel` calls `bundle.rebuildSimulacra()` and reads the
*merged* CHARACTER/ROLE body (previously the merge was dead code, only
ever invoked from a unit test, so every `is` clause was silently inert
at runtime). ROLEs are now exempt from the required-slot abstractness
drop (a role is a per-person schema, never an instance, so an
`any of FACTION` slot is not an unfilled hole) — so a ROLE can mix in a
trait. The `SELF`/`ME` dialogue speaker resolves to whoever `self` is
bound to on the frame (upper-cased to match explicit ALL-CAPS speakers),
letting a beat drop the line that restates its owner. `rebuildSimulacra`
is idempotent (clears `projectDiagnostics` too). Design +
roadmap: [`docs/dev/loom-functional-redesign.md`](../../docs/dev/loom-functional-redesign.md).

Slice 2 landed 2026-06-30 (TS `core/` only): **parameterized traits**.
`TRAIT Scanner(beat)` takes params after its name (split off like SCENE onto
`CharacterBody.params`), referenced in the body as `self.<param>`, and applied
with args via the `is` clause — `CHARACTER Crawler is AlgoScanner(crawler_report)`.
`mergeCharacter` parses each `is` entry with `parseMixinRef` (positional + named
args) and runs `substituteParams`: it deep-clones the parent body
(`deepCloneCharacterBody`, so the trait cache is never mutated) and rewrites every
`self.<param>` token across hook events + bodies, method bodies/inline-exprs, and
property values. Forwarding (`is Scanner(beat), Algo` inside a trait) keeps a param
`self.`-qualified until a concrete character supplies it; at the leaf it always
resolves to the bare arg (`-> self.beat` → `-> crawler_report`), so no qualified-
divert resolution is needed yet. The lexer now splits the `is` clause on
*top-level* commas (`splitTopLevelCommas`, moved to `rust.ts`) so a multi-arg
application `CellWatch(loc: Internet, signal: lockdown)` stays one entry. Unfilled
params surface as a `requiredParamUnfilled` project diagnostic.
Part II Slice A + Slice 3 landed 2026-06-30 (TS `core/` only):

- **Class-owned beats + qualified diverts (Slice A).** A CHARACTER body may
  author `beat name(params)` sub-blocks (parsed like the `generator` block);
  they register in `model.beats` under `Owner.name`, so two props may each own a
  `main`/`greet` with no collision. `parseDivertTarget` splits a slash-free
  target on the first `.` into `{qualifier, name}` (`/` still outranks `.`), and
  the sim's new `resolveBeat` honors it: `self.`/`me.` resolve against the `self`
  binding, an explicit `Owner.` is taken as-is, a **bare name stays global**
  (existing `-> lockdown` diverts unchanged), a cross-owner `-> Owner.beat`
  rebinds `self` so the foreign beat's `SELF` speaks as its true owner, and an
  unresolved qualified divert emits a `diagnostic` event. `visits()` resolves the
  beat name owner-first too (`resolveBeatKey`).
- **Slice 3 niceties.** CHARACTER typed-slot defaults are seeded into the world
  (`CharDef.defaults`), so `self.captures` starts at `0` rather than
  implicitly-zero on first `+=`. An **inline opener divert** `on scan guest -> beat`
  is split into a synthetic body divert. A wrapped `is`-clause (trailing top-level
  comma / unbalanced parens) raises `L1008UnterminatedMixinClause`. *Deferred:*
  `UnresolvedTraitArg`, the `SELF`→`cast[0]` fallback, and LSP completion/hover
  (`RequiredParamUnfilled` already covers the missing-arg case).

**The `escape-the-internet` example is migrated** to the new surface:
`cast/kit.loom` holds the shared traits (`Scanner(beat)` + faction badges +
combined `<Faction>Scanner(beat)`); every scanner prop is a one-liner; `TheAdmin`
tallies on `self.captures`; `Sysadmin` and `Firewall_Terminal` **own** their beats
(`interrogation`/`firewall` moved out of `beats/prison.loom` onto the props,
reached via `-> self.beat`); several beats speak as `SELF`. The migration is
behavior-preserving — the full vitest suite stays green.

Part II Slice B (owned-beat forwarding — a trait param naming an owned beat stays
`self.`-qualified) landed alongside a review pass, so `is Scanner(myOwnedBeat)`
routes to the deriver's own beat; a trait can also ship a concrete `beat` block
that each deriver inherits namespaced to itself.

Part II Slice C landed 2026-07-01 (TS `core/` only): **derived beat templates.**
A `TRAIT` ships a `beat name(params)` whose varying lines are `slot: <name>`
holes; each deriving `CHARACTER` supplies `fill <name>` blocks of content. At
compile, `model.ts::fillSlots` splices the fill content in place of each
`slotPlaceholder` on the lowered tree (recursing through every nested
control-flow body), per deriver — so `Interrogation_Booth` and `Bouncer` sharing
one `confront` template get `Booth.confront` / `Bouncer.confront` with their own
lines and no cross-corruption. A hole with no matching `fill` raises
`unfilledDerivedSlot`; two distinct parents shipping a same-named beat raise
`derivedBeatConflict`; a child re-declaring a derived `beat` whole-overrides it.
The `slot:` placeholder keeps its colon (a bare `slot …` line stays prose);
`fill` is a class-body opener like `beat`/`generator`.

Deferred: beat-level `super` (override-then-extend), `UnresolvedTraitArg`, and LSP.
**The Rust mirror (parser + runtime crates) is not yet updated for ANY of Slices
1/2/A/3/B/C — that is the largest remaining gap; the TS and Rust engines will
drift until it is ported.**

Still to come: a pure-Rust Lua VM (piccolo) so Lua-defined directives
and `.luau` extensions execute client-side instead of degrading; the
editor-side booth panel (UI scaffold on top of the new `booth_*`
runtime APIs); and the live SCENE / coroutine tracker view in the web
client.

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

The CLI prebuilds the relay binary, then runs Vite (`pnpm dev`) +
`loom-relayd --cors permissive` under the prism supervisor (colored
prefixed logs, Ctrl+C fan-out). The editor reads `VITE_LOOM_RELAY` so
the API + WS URLs always hit the right port even with `--ui-port` /
`--relay-port` overrides. There is **no wasm preflight** — the editor
consumes the Loom engine as TypeScript (`@loom/core`), so parser / lsp
changes are picked up live by Vite with no rebuild step.

Useful flags: `--host 0.0.0.0` (LAN), `--ui-only` / `--relay-only`,
`--ship` (release relay).

Manual two-process equivalent, if you want to drive each half
yourself:

```
pnpm --filter loom-app dev    # Vite at :5173 (HMR)
cargo run -p loom-server --bin loom-relayd -- --cors permissive
                              # API + WS at :7878
```

### Editor — authoring-only (wasm removed 2026-06-30)

The React editor under `packages/loom/editor` is an **authoring tool**:
file editing, syntax highlighting, lint, LSP (Outline / References /
completion / hover / definition / symbols), structural beat edits, and
the static entity Graph + beat-flow views — all driven by the
**TypeScript** `@loom/core` engine (parser + `lsp`), **no wasm**. The
modal **Studio** shell is two author modes — **Writing** / **Editing**
on a bottom Mode Bar (`⌘1` / `⌘2`), each a fixed `allotment` layout
(left rail · center stage · Properties tray · optional beat-flow dock).

The former in-editor runtime — multi-head branching play, the
Transcript / Ledger / Timeline / World / Cast / Booth surfaces, and
relay-backed cloud collaboration (Loro CRDT) — was removed with the
`wasm` crate. **Runtime now lives in the sibling packages**: the `core`
event server + the `play` participant app. See
[`docs/dev/loom-ide-redesign.md` Part II](../../docs/dev/loom-ide-redesign.md)
for the shell design.
