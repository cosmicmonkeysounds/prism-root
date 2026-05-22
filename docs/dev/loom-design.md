# Loom — Narrative Scripting Language & Runtime

> **It reads like a screenplay. It thinks like a programming language.
> It runs on Prism.**

Loom is a screenplay-inspired narrative scripting language for branching
dialogue, ambient barks, quests, cutscenes, and interactive prose. It is
the **first smoke test of Prism** — a single language whose
implementation exercises every load-bearing subsystem of the framework:
the `Scanner`, the codegen pipeline, the Luau VM, the reactive
substrate, the CRDT store, the builder, the daemon, and the SSR relay.
If Loom can be built cleanly on top of these pieces, Prism is fit for
purpose. If it can't, the friction tells us where to fix Prism first.

**Status:** initial design (2026-05-21). Greenfield Rust
reimplementation of the legacy TypeScript Loom at
`LEGACY-CODEBASE/loom-lang/`. Target crate: `packages/prism-loom/`.

**Related docs:** [`loom-grammar.md`](loom-grammar.md) (the formal
grammar this doc references), `luau-integration-plan.md` (the Luau
substrate Loom expressions compile to), `prui-reference.md` /
`prss-reference.md` (sibling DSLs sharing Prism's `Scanner`),
`dsl-self-bootstrap.md` (the registry pattern Loom's language extensions
follow).

**Legacy reference:** the v1 TypeScript implementation, with its docs,
lives at `LEGACY-CODEBASE/loom-lang/`. It is the design parent. This
doc calls out — section by section — what it kept, what it cut, and
why.

---

## 1. Why Loom, why now

Prism needs a real workload, not a synthetic benchmark. A workload
that:

1. **Parses a non-trivial surface syntax** — pushes the `Scanner` and
   recursive-descent patterns of `prism-core::language::syntax` past
   their current users (PRUI tags, PRSS TOML, Luau via full-moon).
2. **Generates multi-target output** — exercises the
   `SymbolDef` / `CodegenPipeline` DSL with Rust, Luau type stubs, and
   later TS/C# for editor/game integrations.
3. **Drives a stateful, reactive runtime** — exercises `Signal<T>`,
   `Memo<T>`, `Effect`, and the Loro-backed `Store` as the substrate for
   live "this variable changed → that bark fires" wiring.
4. **Embeds a sandboxed scripting VM** — exercises the Luau host
   surface (`prism-daemon::modules::luau_module`) under realistic
   conditions: thousands of small condition expressions, many short
   action chains, occasional larger handler scripts.
5. **Plugs into the builder** — exercises
   `prism_builder::ComponentRegistry` as a registry for *narrative*
   blocks alongside visual ones, validating the "everything is a
   builder component" memory note.
6. **Renders to both native and HTML** — exercises
   `prism-ui-runtime`'s two backends with a non-form-shaped surface
   (typewriter reveal, inline triggers, ranged styling).
7. **Hot-reloads** — exercises the `.prui` Phase 10 fingerprint-cache
   substrate generalized to a third DSL.

Loom v1 (the TypeScript implementation) gave us a working spec, a tuned
language design, and a ledger of which features matter. v2 is what
happens when that design lands on Prism's substrate instead of an
ad-hoc TS+Lua stack.

---

## 2. Design philosophy (carried from v1, sharpened)

These are the load-bearing claims. They are not negotiable in v2 — when
later sections look like they violate one, that's a bug in the section.

### 2.1 Human text has no brackets

If a character says it, it is plain text — no quoting, no escaping, no
sigils. A reader who has never seen Loom should be able to read a
`.loom` file as a screenplay and understand 90% of it.

### 2.2 Each bracket has exactly one job

| Bracket | Job | Mnemonic |
|---|---|---|
| `$`, `${ }`, `$( )` | **Resolve** — runtime value lookup / evaluation | "dollar = data value" |
| `@` | **Static ref** — compile-time package-validated reference | "at = asset" |
| `[[ ]]` | **Content link** — design-time soft ref (codex, hypertext) | "double-bracket = wiki-link" |
| `< >` | **Effect** — triggers, mutations, active links | "angles = actions" |
| `[ ]` | **Variation** — text selection patterns | "brackets = alternatives" |
| `( )` | **Computation** — s-expressions, grouping | "parens = math/logic" |
| `{ }` | **Dictionary** — speaker blocks, data literals | "braces = data" |
| `''' '''` | **Metadata** — structural docstrings | "triple-quote = documentation" |

No bracket pulls double duty. v1 occasionally muddled this (e.g. `{ }`
both as character block and as text interpolation); v2 resolves all such
overlap below.

### 2.3 Three sigils, three time horizons

| Sigil | Resolves at | Question it answers |
|---|---|---|
| `$` | Runtime | What is this value **right now**? |
| `@` | Build time | Does this named thing **exist** in the project? |
| `[[ ]]` | Design time | What is this **related to**? (codex/hypertext) |

### 2.4 Three operators, three meanings, zero overlap

| Operator | Meaning | Valid contexts |
|---|---|---|
| `=` | Binding ("this **is** that") | `let`, `define`, `defn`, initial `var` set |
| `:=` | Mutation ("set this **to** that") | `~` action lines, `<>` inline assigns |
| `==` | Comparison ("**is** this equal?") | conditions, expressions |

`? $trust = 50` is a parse error, not a runtime footgun. C-style
`if (x = 50)` bugs are unrepresentable.

### 2.5 Keywords for logic, sigils for structure

Sigils (`#`, `--`, `*`, `+`, `->`, `<-`, `>`, `@`, `//`) carry
**structure** — you can scan the shape of a `.loom` file in seconds
without reading a word. Keywords (`if`, `and`, `not`, `var`, `let`,
`fire`, `each visit`, `after`, `match`) carry **logic** — you read them
left-to-right and they read like English.

The two layers never overlap. A line begins with a sigil **or** a
keyword **or** a SPEAKER name **or** plain prose. There is no fifth
case.

### 2.6 Progressive enhancement: four layers

A writer who never needs computation never sees a paren. The language
scales by layers; each layer is optional:

```
Layer 0  pure screenplay      SPEAKER, indented text, --section, * choice, -> divert
Layer 1  + branching state    if / var / fire / has
Layer 2  + reactivity         let / each visit / after / otherwise / match
Layer 3  + computation        ( ... s-expressions ... ) / defn / defmacro / Luau handlers
```

Delete every `$`, `<>`, `[]`, and `if` from a Loom file. If what's left
still reads as a coherent script, the language is doing its job. Strip
the codepoint, keep the meaning.

### 2.7 The language is **defined in Rust**, **extended in Luau**

This is the single biggest change from v1.

v1's headline was *"the language is defined in Lua"* — every keyword
came from a Lua module, the parser was a generic engine. In practice
this was aspirational: the parser hardcoded document-type dispatch, the
"Lua-driven keyword registry" had a parallel TypeScript fallback, and
the spec/implementation gap widened with every release.

v2 inverts this. The **core language** — every sigil, every bracket,
every keyword in this spec, every built-in trigger — is **defined in
Rust**, in a single registry initialized at crate-init time. The parser
is closed over the core grammar.

**Luau remains the extension surface.** Studios can add new actions,
guards, triggers, blocks, inline delimiters, structural variation modes,
and resolve roles by registering them with the `LoomRegistry` from Luau
at workspace-init time. Extensions are *additive*, never overriding
core. The escape valve from v1 (`override = true`) is gone — if you need
to change `if`, fork the crate.

This trade is deliberate. We lose total runtime malleability and gain:
- Build-time grammar validation that actually validates the grammar.
- A formal grammar (see `loom-grammar.md`) that fully describes what
  parses.
- IDE support that doesn't need to boot a Luau VM to highlight
  keywords.
- Hot-reload safety: a Luau extension can't break a `.loom` file that
  was working five seconds ago.

---

## 3. What v2 keeps from v1

Direct lifts. These designs paid off in v1 and survive intact.

- **Speaker detection by ALLCAPS or `$UPPER`.** Cheap, unambiguous,
  reads like a screenplay.
- **Character block inference chain.** `ELENA { worried }` resolves
  `worried` against ordered registries (emotion → dynamic → profile →
  studio-registered). Bare words for the common case, explicit
  `emotion: worried` when inference isn't enough.
- **Choice modifiers via `.keyword`** — `* .once`, `+ .sticky`,
  `* .show("[Insight]")`.
- **Tunnels with parameters.** `-> (ask_wren "the bell" "…") ->` —
  parametric subroutines with a real return. Recursion permitted.
- **Each-visit modes.** `each visit .stopping / .cycle / .shuffle /
  .once`, with a sub-block grammar (`first` / `then` / `finally` /
  `/`-separated alternates).
- **State morphing.** `after $trust > 50` … `otherwise` for
  conversation-wide branching without rewriting every entry.
- **Match blocks.** `match $quest_stage` with literal/`_` arms — flat
  alternative to cascading `if` chains.
- **Saliency scoring** (the Left 4 Dead model). More conditions = more
  specific = wins. No manual priority numbering. Studios can register
  a custom scorer to add domain heuristics.
- **Resolve sigil with safe nav, presence check, indexing, single
  trailing method call.** Reads naturally, fails closed (`$elena?` is
  `false` if elena isn't present, `$elena?.trust > 50` is `false` not
  a crash).
- **VO + director annotations.** `@vo`, `@director`, `@status`,
  `@note`, `@hint`. These feed the recording script and the runtime
  audio bank with no duplicate documentation step.
- **Localization with per-variant keys.** Each text variation gets its
  own stable `locId` so translators see `conv.entry#0`,
  `conv.entry#1`, … independently.
- **Append-only narrative ledger.** Every entry display, every choice,
  every quest event. Reversals, not deletions. Time-travel falls out
  for free.
- **Inline ranged triggers with named anchors.**
  `<speed:0.7 %tension> … </%tension>` — overlapping effects without
  ambiguity.

---

## 4. What v2 cuts from v1

Each of these complicated the spec, the parser, or the runtime out of
proportion to its payoff. Cut with prejudice.

| Cut | Why |
|---|---|
| **`.loom.yaml` data-only format** | Two formats for one language. v1 needed it because its parser couldn't reach a stable AST; v2's `Scanner`-based parser doesn't have that excuse. All Loom content lives in `.loom`. |
| **`weave` keyword** | Conceptually overlaps with `<- target` (thread pull). v2 keeps only `<-` for return-to-caller and thread-pull-by-id. |
| **`->>` soft divert** | A `.return` modifier on `->` covers the same semantics with one less sigil to memorize. |
| **`do` as optional prefix for namespace calls** | Either always or never. v2 picks never: `Camera.pan(@lighthouse, 2)` is a bare expression line; if you need to be explicit it's `~ Camera.pan(...)`. |
| **Macro parameters via `{name}` string substitution** | Untyped, brittle, no checking. v2 macros are real parametric blocks compiled to Luau closures. Call-site arg checking is a build-time error. |
| **`override = true` on language extensions** | The single most dangerous knob in v1. Removed. Extensions are additive only; conflicts are build errors. |
| **Multiple "side-quest" sigil forms** (`>>` dispatcher, `<-` thread pull, `<- name` thread pull-by-id, soft divert) | Collapsed to: `->` divert (with optional `.return` / `.dispatch` modifiers), `<-` return, `<- name` pull-by-id. Three constructs, three sigils. |
| **Built-in typewriter profiles hardcoded in compiler** | v2 ships them as `.loom` typewriter docs in `prism-loom/builtin/`. Studios subclass via `extends`, no recompile of Prism required. |
| **Implicit / unspecified pin-condition `false_action`** | All `NextLink` conditions declare `.skip` or `.block` explicitly. The historical default (`skip`) becomes a parse-time warning prompting the writer to make intent explicit. |

---

## 5. What v2 adds

### 5.1 Reactive `let`, for real

In v1, `let trust_ready = $trust > 50 and $met_wren` was *spec'd* as a
"live formula" but *implemented* as a re-evaluated expression on each
read. v2 makes `let` a real reactive binding backed by
`prism-core::reactive::Memo<Value>`. A condition that depends on
`$trust_ready` registers as a subscriber; mutating `$trust` schedules a
single re-evaluation; downstream effects fire once.

This matters because barks listen on `let` bindings constantly
(`if ready_to_enter`). Without memoization, every bark filter on every
tick re-evaluates the full expression chain. With memoization, it's a
flag lookup.

### 5.2 Snapshots & time travel from day one

Loro CRDT is Prism's source of truth for mutable state. The narrative
ledger is *already* an append-only log of CRDT ops. Combining the two,
the runtime exposes:

```rust
let snapshot = engine.snapshot();         // O(1) — Loro version vector
engine.advance(/* play through some choices */);
engine.restore(snapshot);                 // rewind exactly
```

This lights up debug scrubbing in the editor, deterministic playback
for tests, and "rewind to the last choice" as a UX primitive without
the engine having to model its own undo stack.

### 5.3 Strongly-typed pin conditions

Every condition expression — on a guard line (`if …`), on a `NextLink`
(`* … if …`), on an inline trigger (`<? sane <heartbeat:60>>`) — is
parsed into a typed `LoomExpr` (see grammar §10). Free identifiers must
resolve at build time to a var, a stat, an entity, a quest-flag, or an
event name. Unknown identifiers are hard errors, not warnings.

v1 already aspired to this through the operand registry; v2 *enforces*
it because the parser owns the expression AST end-to-end (no Luau
round-trip required to validate).

### 5.4 Ledger query expressions

The narrative ledger is queryable from any condition context:

```loom
? played(harbor_intro) and visits(wren_talk) >= 2
? last(speaker) == @wren
? since(bell_rung) < 30s
```

These compile to bounded ledger scans, not full table scans. The
expression grammar reserves `played`, `visits`, `last`, `since`,
`count`, and `chose` as ledger predicates (see grammar §10.5).

### 5.5 Hot reload, fingerprinted

Each `.loom` document gets two hashes during codegen: a
`STRUCTURAL_HASH` (entry IDs, flow topology, declared names) and a
`FULL_HASH` (everything, including dialogue text). Edits that change
only `FULL_HASH` patch the live runtime without resetting conversation
state. Edits that change `STRUCTURAL_HASH` trigger a controlled
reload — the active conversation is unwound to its last hub or `START`
and resumed.

Same substrate as `.prui` Phase 10. Generalized to a third DSL is the
proof.

### 5.6 Builder-native authoring

A `.loom` document compiles to a `prism_builder::Component` impl
(`lower_ui` returns a `Surface` tree representing a conversation
playhead, a choice list, a bark queue, etc.). This means a Loom
conversation drops into a PRUI scene like any other widget:

```prui
<scene>
  <stage class="bg-tide">
    <loom-conversation src="@harbor_arrival" />
  </stage>
  <hud>
    <loom-bark-feed channel="ambient" />
  </hud>
</scene>
```

No bespoke "narrative renderer" pipeline. Loom rides on the same
`Surface` → femtovg/HTML lowering everything else uses.

### 5.7 Single-source codegen

Loom's `LoomDatabase` lowers to `SymbolDef[]` and runs through
`prism-core::language::codegen::CodegenPipeline`. The emitters
(`SymbolTypeScriptEmitter`, `SymbolCSharpEmitter`,
`SymbolEmmyDocEmitter`, `SymbolGDScriptEmitter`) are already in
`prism-core` — Loom just calls them. No bespoke per-target emission
code.

---

## 6. The runtime model

A Loom runtime is four engines sharing one resolver, one ledger, and one
state store.

```
        ┌──────────────────────────────────────────┐
        │            LoomDatabase (IR)             │
        │  entries, conversations, barks, quests,  │
        │  cutscenes, typewriter profiles, locids  │
        └──────────────────────────────────────────┘
                            │
        ┌───────────┬───────┴───────┬───────────┐
        ▼           ▼               ▼           ▼
   Conversation   Bark           Quest      Cutscene
     Engine      Engine          Engine      Engine
        │           │               │           │
        └───────────┴───────┬───────┴───────────┘
                            ▼
                  ┌───────────────────┐
                  │   EntryResolver   │  filter → score → select
                  └───────────────────┘
                            │
            ┌───────────────┼───────────────┐
            ▼               ▼               ▼
       Luau VM       NarrativeLedger    Loro Store
     (conditions /    (append-only      (vars, stats,
      action body)     event log)        entities)
```

### 6.1 Engines

| Engine | Drives | Notable behaviors |
|---|---|---|
| `ConversationEngine` | branching dialogue trees | tunnels (call stack), threads (`<-`), hub resumption, `each visit` / `after` morphing |
| `BarkEngine` | ambient one-shot lines | cooldowns, priority bands, saliency scoring, per-speaker queues |
| `QuestEngine` | objective tracking | pluggable objective types via `IObjectiveTypePlugin`-style registry |
| `CutsceneEngine` | timecode-driven sequences | track lanes (audio/camera/event), `at TIMECODE` headers, skippable / non-skippable |

All four are pure Rust. None of them call into Luau **except** to:
1. evaluate a condition expression that exceeded the build-time
   compilable subset (rare; see §10);
2. dispatch a user-registered action handler;
3. resolve a user-registered variation mode.

### 6.2 EntryResolver

Single algorithm, used by every engine that has to pick one entry from a
candidate set:

1. **Gather** — produce a slice of `&Entry` candidates by context.
2. **Filter** — drop entries failing `once`, cooldown, or condition
   checks. Conditions failing with a runtime error fail-closed (drop
   the entry, log a warning) — same as v1.
3. **Score** — `specificity * 10 + explicit_weight + priority_band`
   where `specificity = #(condition operators)`. Custom scorer can
   add any signed delta.
4. **Select** — by mode: `first_valid` (conversations), `priority`
   (barks default), `weighted_random`, `sequential`.

### 6.3 NarrativeLedger

Append-only event log over an adapter:

```rust
trait NarrativeLedgerAdapter {
    fn append(&mut self, event: LedgerEvent) -> LedgerSeq;
    fn query(&self, filter: LedgerFilter) -> Vec<LedgerEvent>;
    fn snapshot(&self) -> LedgerSnapshot;
    fn restore(&mut self, snap: LedgerSnapshot);
}
```

Production adapter: SQLite via `@core/db` analogue (or its Rust
equivalent inside Prism). In-memory adapter for tests + simulator.

### 6.4 TypewriterEngine

Character-by-character reveal driven by a `TypewriterProfile` (per-char
delay, punctuation pause table, emphasis speed multipliers, skip
behavior). Rich-text spans (`<speed:>`, `<shake:>`, `<emotion:>`)
register lifecycle callbacks: `on_char`, `on_pause`, `on_span_enter`,
`on_span_exit`. The shell renders these into a `Surface` via
`prism-ui-runtime`.

Inline triggers fire at exact character offsets — same as v1, but the
character-position tracker is now part of the Rust parser's output, not
recomputed in the runtime.

---

## 7. Crate layout

```
packages/prism-loom/
├── Cargo.toml
├── builtin/                        # ships with the crate
│   ├── core-language.loom.toml     # built-in registrations (declarative)
│   ├── typewriter.default.loom     # default typewriter profile
│   └── typewriter.fast_chat.loom
├── lua-types/                      # generated by codegen
│   └── Loom.d.luau
└── src/
    ├── lib.rs
    ├── syntax/                     # parsing
    │   ├── mod.rs
    │   ├── line.rs                 # line classification (sigil/keyword/speaker/text)
    │   ├── inline.rs               # inline token stream within text
    │   ├── expr.rs                 # expression sublanguage (Pratt over Scanner)
    │   ├── parser.rs               # recursive descent → ast::Document
    │   └── tokens.rs
    ├── ast/
    │   ├── mod.rs
    │   ├── document.rs             # Document, Section, Entry, Annotation
    │   ├── entry.rs                # Dialogue, Choice, Divert, Action, Block
    │   └── expr.rs                 # LoomExpr enum
    ├── registry/
    │   ├── mod.rs                  # LoomRegistry struct
    │   ├── core.rs                 # built-in guards/actions/blocks/triggers
    │   └── luau_extend.rs          # Luau-facing registration verbs
    ├── sema/
    │   ├── mod.rs
    │   ├── resolver.rs             # name resolution against operand registry
    │   ├── validator.rs            # cross-document checks
    │   └── lint.rs                 # style/structure lints
    ├── ir/
    │   ├── mod.rs
    │   ├── bundle.rs               # LoomDatabase: compiled artifact
    │   └── fingerprint.rs          # STRUCTURAL_HASH / FULL_HASH
    ├── codegen/
    │   ├── mod.rs
    │   ├── symbols.rs              # build SymbolDef[] from LoomDatabase
    │   └── vo.rs                   # vo_script.csv + manifest
    ├── runtime/
    │   ├── mod.rs
    │   ├── engine/
    │   │   ├── conversation.rs
    │   │   ├── bark.rs
    │   │   ├── quest.rs
    │   │   └── cutscene.rs
    │   ├── ledger.rs
    │   ├── typewriter.rs
    │   ├── resolver.rs             # EntryResolver
    │   └── luau_eval.rs            # bridge to prism-daemon Luau VM
    ├── luau/
    │   ├── mod.rs
    │   ├── bindings.rs             # Loom.*, Var.*, Resolve.*, Ledger.*
    │   └── extension.rs            # registration verbs callable from Luau
    └── component.rs                # prism_builder::Component impls
```

**Dependencies:**

```toml
[dependencies]
prism-core      = { path = "../prism-core",   features = ["syntax", "codegen", "reactive", "luau"] }
prism-builder   = { path = "../prism-builder" }
prism-daemon    = { path = "../prism-daemon" }   # for the Luau VM
prism-luau-derive = { path = "../prism-luau-derive" }
loro            = { workspace = true }            # CRDT store
serde           = { workspace = true }
serde_json      = { workspace = true }
thiserror       = { workspace = true }
```

---

## 8. Pipeline: source → bundle → runtime

```
.loom source
    │
    │  syntax::parse_document(scanner)
    ▼
ast::Document
    │
    │  sema::resolve  +  sema::validate
    ▼
ast::Document  (annotated, name-resolved)
    │
    │  ir::lower(docs[])
    ▼
LoomDatabase
    │
    ├──► codegen::emit_symbols → SymbolDef[]
    │                       │
    │                       │  CodegenPipeline (prism-core)
    │                       ▼
    │                  Loom.g.rs, Loom.d.luau, Loom.g.ts, Loom.g.cs, …
    │
    └──► serialized .loom.bundle.postcard (runtime input)
                                │
                                ▼
                       runtime::LoomRuntime
                                │
                                ▼
                       conversation engine + bark engine + …
                                │
                                ▼
                     prism_builder::Component  ──►  Surface  ──►  femtovg / HTML
```

**Important:** the `LoomDatabase` is the IR boundary. Anything that
wants to consume Loom — the editor, a remote relay, a saved-game
loader — reads `LoomDatabase` (or the codegen artifacts derived from
it), never raw `.loom` source. Source is for humans.

The bundle is serialized with `postcard` (not JSON) for binary size and
load speed — matches Prism's daemon IPC choice. A JSON debug form is
available on `--debug-bundle`.

---

## 9. Lua → Luau, and what changes

### 9.1 Why Luau

v1 used Lua 5.4 via Wasmoon. Prism uses Luau via `mlua`. Luau is a
superset: it adds gradual typing, sandboxing-first design, and
performance work that Roblox shipped at scale. v2 inherits all of that
for free.

For Loom this means:
- **Condition expressions can be typed.** A `?` line is parsed to a
  `LoomExpr`, lowered to Luau, and *type-checked* against the operand
  registry. Most condition errors become build errors.
- **Sandboxing is the default.** Studio extensions get a restricted
  global table — no `os`, no `io`, no `require` of arbitrary paths.
- **Coroutines are first-class** (Luau preserves them). The yield/resume
  pattern from v1 (`-> YIELD ... resume(vars)`) maps directly.

### 9.2 The Luau surface

Three globals, each scoped to a context (from `prism-core`'s context
model: `runtime`, `workspace`, `build`).

| Global | Context | Purpose |
|---|---|---|
| `Loom` | runtime + workspace | playback control: `Loom.start("conv_id")`, `Loom.resume(...)`, `Loom.fire("event")` |
| `Var` | runtime | `Var.get("trust")`, `Var.set("trust", 50)` — the operand store |
| `Resolve` | runtime | `Resolve.role("SPEAKER")`, `Resolve.safe("elena", "trust")` — `$`-sigil semantics |
| `Ledger` | runtime | `Ledger.played("intro")`, `Ledger.visits("hub")` — the ledger query API |
| `Entity` | runtime | `Entity.getField("elena", "trust")` — entity-namespaced state |
| `Loom.Language` | workspace | extension registration (`registerAction`, `registerTrigger`, …) — see §11 |

EmmyDoc stubs (`Loom.d.luau`) emitted by codegen give IDE autocomplete
for every conversation ID, entry ID, var name, trigger type, and
registered role.

### 9.3 What runs in Luau vs Rust

A Luau VM call is **never** on the inner loop of the resolver. The
resolver is a hot path; every iteration calls it once per candidate
entry, and a bark filter may have dozens of candidates per tick. Going
through Luau for each would be a perf catastrophe and a sandboxing
boundary crossing for nothing.

Instead:

```
Condition compilation:

   parse "if $trust > 50 and met_wren"
       │
       ▼
   LoomExpr (Rust AST, fully typed)
       │
       ├─ subset compilable in Rust?
       │     ├─ yes  → emit Rust eval closure  ── used by resolver
       │     └─ no   → emit Luau source       ── invoked when filter reaches this entry
```

The **build-time compilable subset** covers:
- Numeric / string / boolean literals.
- Var reads (`$trust`).
- Entity field reads with up to one safe nav (`$elena?.trust`).
- Role reads (`$SPEAKER.trust`).
- Built-in operators (`and / or / not / is / is not / has / has not /
  > >= < <= == !=`).
- Built-in ledger predicates (`played`, `visits`, `last`, `since`,
  `count`, `chose`).

This subset is intentionally large enough to cover ~95% of `.loom`
conditions in the legacy corpus.

Anything outside the subset — user-defined `defn`-bound helpers,
inline `${ ... }` expressions in text, action chains with arbitrary
side effects — compiles to Luau. The Luau VM is invoked **once per
evaluation, batched per conversation tick**.

---

## 10. State, persistence, hot reload

### 10.1 State store

All mutable runtime state lives in a `loro::LoroDoc` (Prism's standard
choice). Top-level shape:

```
loom.vars        : Map<String, LoroValue>     // $trust, $met_wren, …
loom.stats       : Map<String, LoroValue>     // $health, $max_health, …  (Meridian-equivalent)
loom.entities    : Map<EntityId, Map<…>>      // $elena.trust, $wren.mood, …
loom.ledger      : List<LedgerEvent>          // append-only
loom.flags       : Map<String, bool>          // event-fired flags
loom.session     : Map<String, LoroValue>     // ephemeral, cleared per session
```

Because the store is a `LoroDoc`, every mutation is an op with a
version vector. `snapshot()` captures the version; `restore(snap)` is a
diff-and-apply, **O(ops since snapshot)** not O(total ops).

### 10.2 Save/load

A save file is the `LoroDoc` serialized to its native binary format
plus the current playhead (`(conversation_id, entry_id, tunnel_stack)`).
That's it. The bundle is content-addressed by `STRUCTURAL_HASH` so a
save written against bundle v1.2 will refuse to load against v1.3 if
the structure changed.

### 10.3 Hot reload

When a `.loom` file changes during dev:

```
fingerprint old vs new:
  STRUCTURAL_HASH unchanged  ── full-hash differs only in text
     ─► patch live: swap entry text, typewriter retriggers from
        current char offset if mid-reveal
  STRUCTURAL_HASH changed
     ─► soft reset: unwind the conversation to its last hub or
        SECTION boundary, replay no events from the ledger, resume
```

Bark queues are always rebuilt from the new bundle — there's no notion
of "the bark mid-play" because barks are one-shot.

Same fingerprint substrate as `.prui` literal-only updates (Phase 10).

---

## 11. Extension model

Studios extend Loom **only by registration** — never by modifying the
parser, never by overriding core keywords.

### 11.1 Registerable elements

| Kind | Registered shape | Example |
|---|---|---|
| Action | `LoomAction { keyword, parse_args, handler_name }` | `remember journal_entry_X` |
| Guard | `LoomGuard { keyword, negate }` | `unless betrayed` (just `if not betrayed`, so this is illustrative — most studios won't add guards) |
| Block | `LoomBlock { keyword, sub_keywords, group, apply_to_children }` | `flashback "three days ago"` |
| Trigger | `LoomTrigger { name, rangeable, category, parse_args }` | `<vfx:sparkle,0.5>` |
| Inline delimiter | `LoomInlineDelim { open, close, name, visible }` | `<<stage whisper>>` |
| Variation mode | `LoomVariationMode { name, select_fn }` | `[a / b / c].weighted(0.7, 0.2, 0.1)` |
| Resolve role | `LoomRole { name, readonly, indexed }` | `$NARRATOR`, `$COMPANION[0]` |
| Char-block category | `LoomCharCategory { name, values, aliases, priority }` | `{ aggressive }` matches a `stance` category |
| Annotation | `LoomAnnotation { keyword, body_shape }` | `@status final` |

### 11.2 What is *not* registerable

These are invariant. Modifying them would change what a `.loom` file
*is*.

- Structural sigils: `#`, `--`, `*`, `+`, `->`, `<-`, `>`, `@`, `//`.
- Bracket families: `{}`, `<>`, `[]`, `[[]]`, `()`, `${…}`, `$(…)`.
- Speaker detection (ALLCAPS or `$UPPER`).
- Indentation semantics.
- The three sigils `$ @ [[…]]` and the three operators `= := ==`.
- The four engine types (conversation / bark / quest / cutscene).

### 11.3 The Rust ↔ Luau extension dance

Registration verbs are exposed as Luau bindings on the workspace
context. A studio writes:

```luau
-- workspace/loom/extensions/horror.luau
local L = Loom.Language

L.registerGuard("if sane", { negate = false, body = "$sanity > 0" })

L.registerAction("drain", {
  parse = function(line)
    local resource, amount = line:match("(%S+)%s+(%d+)")
    return { handler = "Horror.drain", args = { resource, tonumber(amount) } }
  end,
})

L.registerTrigger("heartbeat", { rangeable = true, category = "internal" })

L.registerInlineDelim("~~", "~~", { name = "corrupted", visible = true })
```

At workspace boot, the Luau VM runs every `*.luau` file in
`workspace/loom/extensions/` against a *staging registry*. After all
extensions run, the staging registry is **frozen**, diff'd against the
core registry, and merged. Conflicts are build errors with the offending
file:line.

The frozen registry is then handed to the parser. The parser is closed
over the merged registry for that workspace session. There is no
mid-session registration. Extensions reload only with the workspace.

---

## 12. Integration with the rest of Prism

### 12.1 As a builder component

```rust
impl prism_builder::Component for LoomConversationPlayhead {
    fn schema(&self) -> Vec<FieldSpec> {
        vec![
            FieldSpec::reference("src", "@conv").required(),
            FieldSpec::reference("typewriter", "@typewriter").optional(),
        ]
    }

    fn signals(&self) -> Vec<SignalDef> {
        vec![
            SignalDef::new("on_choice", &["choice_id: string"]),
            SignalDef::new("on_complete", &[]),
        ]
    }

    fn lower_ui(&self, ctx: RenderContext, node: &Node, style: StyleProperties)
        -> Result<prism_ui_runtime::layout::Node>
    {
        let conv_id = ctx.resolve_ref(&node.props["src"])?;
        let handle = ctx.loom_runtime().start(conv_id)?;
        ctx.subscribe(&handle, "on_choice", node.signals["on_choice"]);
        Ok(self.render_playhead(handle, style))
    }
}
```

Three blocks ship: `loom-conversation` (playhead),
`loom-bark-feed` (ambient queue), `loom-quest-tracker`
(active-objective HUD). Studios register more.

### 12.2 As a daemon module

`prism-daemon` exposes `loom.start`, `loom.choose`,
`loom.fire_event`, `loom.snapshot`, `loom.restore` as IPC verbs.
The desktop Studio shell drives a `LoomRuntime` through this surface;
the SSR relay drives the same surface remotely for the Sovereign
Portal.

### 12.3 As an SSR target

`prism-ui-runtime`'s `lower_semantic_html` already serializes a
`Surface` to semantic HTML. A Loom conversation lowering to a
`Surface` lowers to HTML for free. The HTML form uses CSS animations
for typewriter reveal and inline triggers map to `<span>` classes with
data attributes — graceful degradation for non-JS clients.

### 12.4 As a CLI subcommand

```
prism loom check   <path>     # parse + validate, no codegen
prism loom build   [--target rust|luau|all]
prism loom run     <path>     # spawn a CLI playthrough simulator
prism loom export  --format csv  # vo script + loc bundle
prism loom fmt     <path>
prism loom lint    <path>
```

Wired into `prism-cli` alongside `dev`, `build`, `test`. Inherits
sccache + lld + GC.

---

## 13. Testing strategy

Loom is the smoke test for Prism; Loom itself needs tests. Layered:

1. **Unit (in-crate)** — every parser nonterminal has at least one
   positive and one negative test. Every IR lowering has a snapshot.
   Every runtime engine has a deterministic playthrough.
2. **Golden files** — `tests/golden/*.loom` paired with
   `tests/golden/*.expected.bundle.postcard`. CI fails if either side
   drifts.
3. **Property tests** — round-trip: parse → ast::print → parse →
   compare. Idempotent.
4. **Playthrough simulator** — drives every conversation in
   `tests/corpus/` to completion via every choice combination,
   asserting the ledger is well-formed.
5. **Cross-DSL** — a PRUI scene that embeds a Loom conversation, that
   triggers a PRSS state variant, that mutates a signal — proves the
   substrate is real.

All run under `cargo prism test`.

---

## 14. Roadmap

Phased, each phase ends with `cargo prism test` green and one demo
narrative played end-to-end.

| Phase | Scope | Demo |
|---|---|---|
| **0** | Crate skeleton, dependency wiring, `prism loom check` parses empty files. | `prism loom check /dev/null` returns 0. |
| **1** | Lexer + line classifier + speaker/text + `--section` + `->` divert. No conditions, no actions. | A linear screenplay plays to completion. |
| **2** | Choices (`*`, `+`), modifiers, hubs, returns. Still no state. | A branching conversation with hubs works. |
| **3** | Vars (`var`, `=`, `:=`), `if` guards, expression grammar §10. | "Tide Bell" intro plays with branching by `$trust`. |
| **4** | `let` reactive bindings (Memo-backed). `each visit`, `after`/`otherwise`. | Conversations evolve across visits. |
| **5** | Barks + BarkEngine + saliency scoring. | Ambient harbor chatter responds to state. |
| **6** | Inline tokens: `${…}`, `<sfx:>`, `<speed:>`, ranged triggers, named anchors. | Typewriter renders a full Tide Bell scene with juice. |
| **7** | Quests + cutscenes. | The full Tide Bell scenario from `LEGACY-CODEBASE/loom-lang/example.md` plays end-to-end. |
| **8** | Codegen (SymbolDef → Rust/Luau/TS), `Loom.d.luau` stubs. | IDE autocomplete on conversation IDs. |
| **9** | Luau extension surface (`Loom.Language.*`). | Horror dialect from v1 extensibility.md ports verbatim. |
| **10** | Hot reload, snapshots, time travel. | Editor scrubber rewinds a playthrough. |
| **11** | Builder integration. SSR via relay. | Loom conversation in a PRUI scene, served by `prism-relay`. |
| **12** | CLI polish, lint rules, fmt, perf. | Bench gates: 10k entries parse < 200ms, 1k-tick bark filter < 1ms. |

---

## 15. Open questions

These are unresolved as of 2026-05-21. Each is parked here, not in
implementation, until we have a reason to pick.

1. **Conversation history as a queryable view.** The ledger answers
   `played(X)` cheaply, but "every line $elena has said this session"
   needs an index. Build one eagerly, or compute lazily and cache?
2. **Stat-vs-var precedence under conflict.** v1 said vars win; v2
   inherits that, but Meridian-equivalent stat registration in Prism
   isn't designed yet. The collision rule may want revision.
3. **Streaming bundle load.** A `LoomDatabase` for a large game is
   tens of MB. Should the runtime mmap a postcard bundle and lazy-load
   conversations, or accept the up-front load?
4. **Multi-author conflict resolution.** Loro merges CRDT ops
   trivially, but Loom *content* edits are line-grained text edits.
   What does "two writers edited the same dialogue" look like in the
   editor? Defer until co-edit is a real concern.
5. **GDScript / Unity-C# emitter parity.** v1 emitted both for the
   game-engine bridge. v2 inherits the emitters from Prism, but the
   downstream Godot/Unity integrations are out of scope for the smoke
   test. Re-evaluate when the first non-Prism consumer asks.

---

## 16. References

- Legacy v1 spec (the design parent): `LEGACY-CODEBASE/loom-lang/`,
  in particular `README.md`, `reference.md`, `resolve-sigil.md`,
  `extensibility.md`, `IMPLEMENTATION.md`.
- The formal grammar: [`loom-grammar.md`](loom-grammar.md).
- Prism's `Scanner`: `packages/prism-core/src/language/syntax/`.
- Prism's codegen DSL: `packages/prism-core/src/language/codegen/`.
- Luau bindings substrate:
  `packages/prism-daemon/src/modules/luau_module.rs`,
  `packages/prism-core/src/language/luau/`.
- Reactive substrate: `packages/prism-core/src/reactive/`,
  `packages/prism-core/src/kernel/atom.rs`.
- Builder registry: `packages/prism-builder/src/registry.rs`,
  `packages/prism-builder/src/component.rs`.
- Sibling DSL docs: [`prui-reference.md`](prui-reference.md),
  [`prss-reference.md`](prss-reference.md),
  [`luau-integration-plan.md`](luau-integration-plan.md).
