# Loom — A Narrative Engine

> **Reads like a screenplay. Thinks like a programming language.
> Runs anywhere people perform.**

Loom is one language for the three things every narrative system
needs: **characters** (who exists), **stats** (what state they're in),
and **story** (what happens). It compiles a single `.loom` source to
runtimes for branching game dialogue, traditional theatre and film
scripts, ambient barks, quests, cutscenes, and live immersive shows
where the audience moves through a shared world and actors improvise
around prepared beats.

The legacy companion packages — Simulacra (characters) and Meridian
(stats) — are absorbed. There is no `.sim`, no `.mdn`. Each is a
`.loom` document archetype: `# elena :character`, `# combat :stats`,
`# barbarian_path :tree`. One language, one source tree, one bundle.

**Status:** initial design (2026-05-22). Target crate:
`packages/prism-loom/`.

**Companion docs:** [`loom-grammar.md`](loom-grammar.md) — the formal
grammar. Implementation notes (crate layout, phased roadmap, test
strategy) live in `loom-impl.md` once the work starts; this doc is
intentionally about *what the language is*, not how to build it.

---

## 1. What Loom is

Four media, four runtimes, one language:

| Medium | What changes | What Loom provides |
|---|---|---|
| **Game** | Branching reactive playback, full agency, save/load. | Dialogue trees, barks, quests, reactive state, per-player ledger. |
| **Film** | Linear playback, no runtime branching, alt-takes as production variants. | Timecode blocks, scene/shot headers, branches as variants. |
| **Traditional theatre** | One-pass performance, named cues for crew, prompter for performers. | Cue declarations, beat tracking, prompter rendering. |
| **Immersive theatre** | Many participants, shared world, improv around beats, live director patching. | First-class participants, locations, broadcast scopes, improv directives, hot patching from the booth. |

The four share more than they differ. All four have a **cast**
(performers / NPCs / animation rigs), a **crew** (technical channels —
lights, sound, camera, particles, haptics), an **audience** (player /
viewers / attendees / participants), and a **story** (the entries,
sections, and flow). Loom makes each first-class.

Loom is also the first non-trivial workload on Prism. Every load-
bearing subsystem — the `Scanner`, the codegen pipeline, the Luau VM,
the reactive substrate, the Loro CRDT store, the builder, the daemon,
the SSR relay — has to hold up under it. That's the smoke test. It is
not the headline; the headline is shipping a great narrative
engine.

---

## 2. Design philosophy

These are the load-bearing claims.

### Human text has no brackets

If a character says it, it is plain text — no quoting, no escaping, no
sigils. A reader who has never seen Loom should read a `.loom` file
like a screenplay and understand 90% of it.

### Each bracket has exactly one job

| Bracket | Job |
|---|---|
| `$`, `${ }`, `$( )` | **Resolve** — runtime value lookup / evaluation |
| `@` | **Static ref** — build-time validated reference |
| `[[ ]]` | **Backlink** — author-time soft reference (codex, hypertext) |
| `< >` | **Effect** — triggers, cues, mutations, active links |
| `[ ]` | **Variation** — text selection patterns |
| `( )` | **Computation** — s-expressions, grouping, parentheticals |
| `{ }` | **Dictionary** — speaker blocks, data literals |
| `''' '''` | **Metadata** — structural docstrings |

No bracket pulls double duty. The three reference sigils get §7.

### Three operators, three meanings

| Op | Meaning | Valid contexts |
|---|---|---|
| `=` | Bind ("this **is** that") | `let`, `define`, initial `var` set, properties |
| `:=` | Mutate ("set this **to** that") | actions, `<>` inline assigns |
| `==` | Compare ("**is** this equal?") | conditions, expressions |

`? $trust = 50` is a parse error. Footguns don't lex.

### Keywords for logic, sigils for structure

Sigils carry shape — you scan a `.loom` file in seconds. Keywords
carry meaning — they read like English. A line begins with a sigil,
*or* a keyword, *or* a SPEAKER, *or* prose. There is no fifth case.

### Progressive enhancement

```
Layer 0  pure screenplay      SPEAKER, indented text, --section
Layer 1  + characters & cues  cast, cue, named cue dispatch
Layer 2  + branching & state  * / + choices, ->, if, var, fire
Layer 3  + reactivity         let, each visit, after, when, match
Layer 4  + live performance   participants, broadcast, improv, locations
Layer 5  + dynamics           generators, scenes, every, wait, list comprehensions
Layer 6  + computation        ( ... s-expr ... ), defn, defmacro, Luau handlers
```

Delete every sigil and keyword. If the remaining text reads as a
script, the language is doing its job.

### Rust core, Luau extension

The core language — every sigil, every keyword, every built-in trigger
type — is defined in Rust, in a registry initialized at crate init.
The parser is closed over the core grammar; the grammar in
`loom-grammar.md` is enforceable.

Studios extend by registering new actions, guards, triggers, blocks,
inline delimiters, variation modes, scene templates, cue targets,
generators, and resolve roles via Luau at workspace boot. Extensions
are **additive only**. They cannot override core. There is no
`override = true` knob.

---

## 3. The world model — characters, stats, factions, story

A Loom project is one tree of `.loom` files. Every document opens
with a header tag declaring its **archetype**:

```
# elena            :character        ← who exists
# combat           :stats            ← what state
# rebels           :faction          ← who's with whom
# harbor_intro     :conversation     ← what happens
```

Four archetypes, four layers of the same world. They cross-reference
by `@id`: a story line names a `@character`; a character slot binds a
`@stats` profile; a faction lists members by `@character`; a stats
expression reads a character's `$attributes`. The dependency graph is
one-way:

```
story  ──reads──▶  factions  ──reads──▶  characters  ──reads──▶  stats
                       ▲                     ▲
                       │                     │
   live performance ───┴─────────────────────┘   (cast slots, participants, cohorts)
```

The runtime composes the four. A `LoomDatabase` is built once from
all four archetypes; at runtime, the conversation engine, the
character lifecycle manager, the stat system, and the faction
registry share one Loro store and one ledger.

> **Why four, not three.** Characters, stats, and story alone get
> you a complete RPG or screenplay. Factions are the load-bearing
> fourth layer for *political simulation* — alliances, betrayals,
> uprisings, crowds — and for *live immersive theatre*, where
> participants self-organize during the show and the narrative must
> respond to groups it didn't know would exist at write time. See
> [§10](#10-factions) for the full treatment; emergence is what
> makes them a peer archetype, not a sub-feature of characters.

What goes in which archetype is a content-organization decision, not
a language decision — a tiny game can put everything in one file:

```loom
# tiny_game

cast WREN
  .label "Wren the Fisher"
  .stats { attack: 8, perception: 14 }
  disposition $PLAYER
    trust = 30

faction lighthouse_keepers
  members @WREN
  stance @PLAYER = neutral

# intro :conversation

WREN { wary }
  Who are you?
```

A large game splits along the archetype seams.

---

## 4. Characters

> **The Simulacra layer.** Characters are first-class — typed,
> declarable, extendable, addressable. Every speaker is a character;
> every cast slot resolves to one.

### Declaration

```loom
# elena :character
  .type humanoid                       # extends a registered type
  .label "Elena Voss"
  .voice female_alto
  .home @lighthouse_interior
  .bio """
  Lifelong keeper's apprentice. Reads the tides and the brass.
  """
```

A character declaration extends the type hierarchy (see §4.1) and
hosts any number of **slots** (§4.4) — units of state and behavior
that other layers contribute. The body is a sequence of slot blocks.

### 4.1 Type hierarchy

Loom ships one base hierarchy out of the box, with `extends` for
project-specific subtypes:

```
@character                              (abstract base)
├── @humanoid
├── @creature
├── @prop                               (carriable / placeable)
└── @trigger                            (invisible volume)
```

Project subtypes:

```loom
# demonic_creature :type extends @creature
  .field corruption  : int = 0
  .field affinity    : enum(@void, @flame, @rot)
  .slot horror       : required
```

Subtype fields are inherited; a `# X :character .type demonic_creature`
must supply `affinity` (required), may supply `corruption` (defaulted),
and is auto-fitted with a `horror` slot.

### 4.2 Identity

Every character has:

- `@id` — kebab-case, globally unique.
- `.label` — display name shown to the audience.
- `.type` — dot-path into the type hierarchy.
- `.home` — `@location` of origin (optional).
- `.unique` — `true` if exactly-one (the player character is always
  unique).

These are the universal fields. Everything else is a slot.

### 4.3 Disposition

The single most-asked-for "character depth" feature: **how does this
character feel about that one?** A first-class declaration:

```loom
disposition $PLAYER
  trust   = 0..100, init 30
  respect = 0..100, init 50
  fear    = 0..100, init 0
  
  reacts trust > 60  -> warm
  reacts fear  > 40  -> guarded
  reacts trust < 10 and respect < 30  -> hostile
```

A disposition declares numeric axes specific to a relationship, an
initial value, optional bounds, and **reactions** — patterns whose
match produces a runtime tag readable in conditions:

```loom
ELENA
  if $elena.disposition($PLAYER).is(warm)
    Come in. Stay a while.
```

Disposition is symmetric by default (Elena's view of Alice and Alice's
view of Elena are independent variables on independent axes), but
**mirror** binds two sides:

```loom
disposition $PLAYER
  trust = 0..100, init 30 mirror $PLAYER.disposition.trust
```

Now mutating one half updates the other. Mirrors are the right tool
for symmetric relationships (friendship, marriage); independent
dispositions for asymmetric ones (admiration, fear).

### 4.4 Slots

Slots are how *other layers* contribute state to a character. The
stats layer adds a `stats` slot; live theatre adds a `performer` slot;
audio adds a `voice` slot. Slots are declared by the contributing
layer's registry; on a character they are populated by name:

```loom
# elena :character
  stats { profile: @scholar, attributes: { perception: 14 } }
  voice { profile: female_alto, room: @lighthouse }
  inventory { starting: [@journal, @brass_key] }
```

Built-in slots:

| Slot | Provided by | Purpose |
|---|---|---|
| `stats` | the stats layer | axes / pools / attributes attached |
| `voice` | the audio layer | typewriter profile + TTS hints |
| `inventory` | the items layer | what they're carrying |
| `performer` | the live layer | actor-binding metadata |
| `dialogue` | the story layer | default greeting entry, hub section |

A slot key that no registry knows is a build error.

### 4.5 Knowledge

What this character has learned. Declared as a closed schema of typed
slots; mutated by story.

```loom
knowledge
  met_player       : bool = false
  knows_about_bell : { unknown, suspects, confirmed } = unknown
  keeper_name      : string?                           # nilable
  rumours_heard    : list<@rumour> = []
  saw_the_keeper   : bool = false
```

Field types are restricted to a small set: `bool`, `int`, `float`,
`string`, closed enums (`{ a, b, c }`), nilable forms (trailing `?`),
and `list<T>`. Anything more complex belongs in `var` or a slot — the
restriction is deliberate. Knowledge is the simple "what does this
character know" map and stays parseable as a flat table.

**Querying.** Inside expressions:

```loom
if $elena.knows_about_bell is suspects        # enum equality
if $elena.met_player                          # bool field
if $elena.keeper_name?                        # nilable presence
if $elena.rumours_heard has @rumour_singer    # list membership
if $elena.rumours_heard.count > 2             # list shape
```

Knowledge is read-only from outside the owning character; treating it
as freely mutable from any conversation would let one NPC overwrite
another's beliefs. The mutation surface is exactly:

```loom
~ $elena.knows_about_bell := suspects                 # bind/overwrite
~ $elena.rumours_heard += @rumour_singer              # list-append
~ $elena.rumours_heard -= @rumour_singer              # list-remove
~ $elena.met_player := true
~ $elena.keeper_name := nil                           # clear nilable
```

Only `:=`, `+=` (lists), and `-=` (lists). No `++`, no arithmetic on
ints. If you need a counter, declare a `var` — not knowledge.

**Ledger contract.** Every knowledge mutation writes one
`KnowledgeChanged { character, key, before, after, at_ms }` entry
through the same ledger as conversations. Hooks (§4.8) subscribe to
this stream; save/load round-trips it byte-for-byte.

Knowledge is per-character (Elena's beliefs aren't Alice's); the
runtime maintains one knowledge map per `@character` instance. For
shared world facts, use `var` at document scope.

### 4.6 Goals

What this character is *trying to do*. A small declarative state
machine — every character can hold many goals; one is active at a
time.

```loom
goal find_keeper
  priority       = 0.8                  # static; for runtime override see below
  active_when    = $time.hour > 6am and not $elena.exhausted
  completes_when = knowledge.saw_the_keeper
  fails_when     = $elena.health < 20
  drives generator search_routine       # what behavior runs while pursuing
  on_complete    -> $elena.knows_about_bell := confirmed
  on_fail        -> log "abandoned find_keeper"
```

**Knobs.** Five declarative predicates + two effect hooks:

| Knob | Re-evaluated when | Effect |
|---|---|---|
| `priority` | (constant) | tie-break ordering; higher wins |
| `active_when` | any read variable changes | gates entry into the pursuing state |
| `completes_when` | any read variable changes | transitions to `complete` |
| `fails_when` | any read variable changes | transitions to `failed` |
| `drives` | (constant) | which generator runs while pursuing |
| `on_complete` | edge transition | action chain fired on success |
| `on_fail` | edge transition | action chain fired on failure |

The four predicates are reactive `let`-bound expressions (§9.1) and
share the same `Memo<bool>` substrate as `let`. A predicate is
evaluated once at goal-declaration time and re-checked only when a
dependency invalidates.

**State machine.** Each goal occupies one of four states per
character: `dormant` → `pursuing` → (`complete` | `failed`).
Transitions:

- `dormant → pursuing`: `active_when` becomes true and this is the
  highest-priority goal whose `active_when` is true.
- `pursuing → complete`: `completes_when` becomes true.
- `pursuing → failed`: `fails_when` becomes true.
- `pursuing → dormant`: a higher-priority goal's `active_when` becomes
  true (the lower goal *suspends*; its driver generator is paused
  rather than cancelled).
- `complete`, `failed`: terminal.

**Priority resolution.** When two goals tie on `priority`, declaration
order in the source decides. Studios that want runtime priority
override register a Luau hook that emits a `priority_for(goal,
character) -> float`; the resolver consults the hook before the static
value.

**Querying.** Goals are first-class condition atoms:

```loom
if $elena.pursuing(find_keeper)
if $elena.completed(find_keeper)
if $elena.failed(find_keeper)
if $elena.active_goal == find_keeper
if $elena.goals has find_keeper             # any state
```

**Imperative control.** Story can force the state machine when
narrative demands it:

```loom
~ $elena.goal(find_keeper).start             # force into pursuing
~ $elena.goal(find_keeper).complete          # short-circuit
~ $elena.goal(find_keeper).fail
~ $elena.goal(find_keeper).reset             # back to dormant; clears history
```

These bypass the reactive predicates — useful when the writer needs
to override the simulation for a key story beat.

### 4.7 Generators on characters

Inline generators (§9.3) tied to the character — the most direct way
to express "what this character does when nobody is talking to them":

```loom
generator daily_routine
  at 6am   go_to @home
  at 8am   go_to @harbor
  at noon  go_to @market
  at 6pm   go_to @home

  every random(20m, 45m)
    if at @harbor and $weather.fog
      yield bark from harbor_fog_chatter
```

A character with no generators is a static prop; one with a few is a
small village inhabitant; one with a dozen is a routine-driven
inhabitant of an immersive show.

### 4.8 Hooks

Event-driven character reactions, fired by the world. A hook is a
declarative pattern matched against the ledger stream; when the
pattern fires, the body runs.

```loom
on meeting $PLAYER                           # one-shot pattern (encounter)
  if not knowledge.met_player
    knowledge.met_player := true
    -> introduce_self as $elena

on $elena.disposition($PLAYER).trust passes 80   # threshold-crossing
  -> reveal_secret as $elena

on $time.hour == 22                          # in-world clock predicate
  -> retire_for_night as $elena

on cue bell_strike_loud                      # crew bus event
  $elena.knowledge.heard_the_bell := true

on $PARTICIPANT enters @lighthouse_interior  # location event
  -> greet_visitor as $elena
```

**Trigger vocabulary.** A hook's leading clause is matched against
the built-in event registry. The core registry exposes:

| Form | Fires when | Bindings inside body |
|---|---|---|
| `on meeting $X` | any character first comes into contact with `$X` | `$X` is the other party |
| `on $expr passes N` | `$expr` crosses `N` upward | `$prev`, `$now` |
| `on $expr drops below N` | `$expr` crosses `N` downward | `$prev`, `$now` |
| `on $expr == V` | edge-triggered equality | `$prev`, `$now` |
| `on $time.hour == H` | in-world clock at hour `H` | — |
| `on cue X` | named cue (§8.2) fires | `$payload` |
| `on $P enters @L` | participant `$P` enters location `@L` | `$P` |
| `on $P exits @L` | participant `$P` exits location `@L` | `$P` |
| `on event X` | a user-fired `fire X` event | `$payload` |

Studios register additional hook predicates through the extension API
(§11). A hook predicate that the registry doesn't recognize is a
build error (`unknown-hook-pred`).

**Scheduling.** Hooks are *queued*, not preemptive. When a ledger
write matches a hook's pattern, the runtime appends a
`HookFired { character, hook_id, payload }` envelope to the
character's pending-actions queue. The queue drains on the
character's next turn — which for a player-facing NPC is "when the
camera/control returns to them," and for a background NPC is "next
generator tick." This guarantees a hook cannot interrupt the active
conversation playhead.

**Ordering.** When multiple hooks on one character match the same
event, their bodies execute in *source order* — the order they were
declared in the character body. Across characters, hook order matches
the character iteration order of the ledger subscriber list, which is
declaration order in the project.

**Idempotence.** Each hook tracks whether its leading predicate is
*edge-triggered* (`passes`, `drops below`, `==`, `meeting`,
`enters`, `exits`) or *level-triggered* (`>`, `<`, `is`). Edge
triggers fire once per crossing; level triggers fire once and then
require the predicate to become false before re-arming. There is no
"fire every tick the condition is true" mode — that's what generators
are for.

**Lifecycle.** A hook is active while its host character is
*instantiated* in the world. Despawning a character cancels its hooks
(their pending entries in the action queue are dropped). Hot-reload
re-installs hooks at the next ledger flush; in-flight hook bodies
finish under the old definition.

---

## 5. Stats & progression

> **The Meridian layer.** Five primitives — Axes, Pools, Attributes,
> Stats, Trees — cover every RPG progression model from D&D 5e to
> Skyrim to Path of Exile, plus "no stats at all" for narrative-only
> shows.

Stats can be declared on a character directly or in a shared
`# foo :stats` document that characters reference.

```loom
# combat :stats

attribute strength = 10, range 1..30
attribute agility  = 10, range 1..30

axis level
  mode xp_curve
  curve $level * $level * 50
  on advance fire level_up

pool health
  max = $max_health
  regen 2/s when not $in_combat

stat max_health = 50 + $strength * 5 + lookup(@combat:level, $level)
stat damage     = 8 + $strength * 0.5 + (equipped?.bonus or 0)
```

### 5.1 The five primitives

| Primitive | "What is it?" | Example |
|---|---|---|
| **Attribute** | A static per-character number set at creation. | `strength = 10` |
| **Axis** | A dimension that advances at runtime. | `axis level (mode xp_curve)` |
| **Pool** | A spendable supply with max / regen / cost. | `pool health (max=$max_health regen 2/s)` |
| **Stat** | A computed or tracked value. | `stat damage = ...` |
| **Tree** | A DAG of unlockable nodes. | `# warrior_path :tree` |

### 5.2 Axes — six advancement modes

```loom
axis level
  mode xp_curve              # earn XP, spend automatically
  curve $level * 100

axis one_handed
  mode use_tracking          # advance from using it
  on use $weapon
  curve $level * $level * 10

axis attribute_points
  mode point_buy             # spend a pool
  buy from pool_attribute_points

axis approval_act_1
  mode milestone             # gate by narrative
  milestones
    1: played(intro)
    2: chose("help_wren")
    3: knowledge has_truth

axis quest_progress
  mode narrative_trigger     # advanced only by story
  advance on event quest_step_done

axis enemy_difficulty
  mode sdk_controlled        # Luau handler decides
  handler @enemy.scaling
```

### 5.3 Stats — four types

```loom
stat damage = 8 + $strength + (equipped?.bonus or 0)    # expression

stat carry_capacity                                      # lookup table
  lookup $strength
  table { 1: 20, 5: 35, 10: 60, 20: 120 }
  interpolate linear

stat health pool max=$max_health regen 2/s              # pool
stat shield pool max=$max_shield regen $reflux/s when not $in_combat

stat threat_score derived from $damage, $position, $aggression
  formula $damage * 0.5 + $position.exposure + $aggression * 10
```

### 5.4 Trees — progression DAGs

A separate archetype because they're often large and shared across
characters:

```loom
# warrior_path :tree

node armsman_1
  cost { skill_points: 1 }
  requires axis(one_handed) >= 20
  effect stat(damage) +5
  effect var(unlocked_armsman_1) := true

node armsman_2
  cost { skill_points: 1 }
  requires node(armsman_1)
  effect stat(damage) +5
  effect ability @power_attack
```

Tree nodes have cost, prerequisites, unlock conditions, and effects
(stat modifier, attribute change, var set, pool grant, ability
unlock, Luau hook). Multi-rank nodes use `rank N` modifier.

### 5.5 Modifiers

Runtime stat adjustments with operation, optional duration, optional
condition:

```loom
~ modify $player.damage +2 add for 30s
~ modify $elena.fear -10 multiply 0.5 while @lantern.lit
```

Modifiers compose. The runtime resolves to a single effective value
per (entity, stat) tuple on read.

### 5.6 The unified namespace

Every stat-system primitive — attributes, axes, pools, stats — is
exposed under `$`:

```loom
$elena.strength            # attribute
$elena.level               # axis
$elena.health              # pool, current value
$elena.health.max          # pool, max
$elena.damage              # stat
$elena.tree.armsman_1?     # tree-node presence test
```

Loom doesn't distinguish bracketed prefixes (`[stat:X]`, `[axis:X]`,
`[pool:X]`) at the surface — the resolver knows which registry owns
the name. Disambiguation qualifiers exist for the rare collision (see
[grammar §11.1](loom-grammar.md#111-resolve-reference-)).

---

## 6. Story

The familiar narrative parts. Conversations, choices, diverts,
dialogue, evolution.

```loom
# harbor_intro :conversation

-- start

WREN { worried }
  The bell went silent three days ago.

  * I'll help.  -> investigate
  * Not my problem.  -> leave  if not $trusted

-- investigate

after $trusted
  WREN { warm }
    [[object:maren|Maren]] taught me to listen to <speed:0.7>the water</>.

otherwise
  WREN
    She just... stopped.
```

`--` declares a section; `*` / `+` are once-only / sticky choices;
`->` is a hard divert; `<-` is a return; `<- target` pulls a thread
by name. Diverts take modifiers: `-> @harbor .return`,
`-> notify_wren .dispatch`. Tunnels with parameters:
`-> (ask_wren "the bell" "...") ->`.

Sections evolve:

```loom
each visit
  first
    WREN
      Morning. New face.
  then
    WREN
      Back again.
  finally
    WREN
      You're practically furniture now.
```

Multi-way branching: `match $quest_stage` with arms. State-morphing
across an entire section: `after $betrayed` / `otherwise`. Both
exist; the right one is the one that reads more clearly for the
shape of the branch.

### 6.1 The playhead

A `:conversation` (or any of its specializations) has one
**playhead** at any given moment — the section currently executing.
The playhead is the unit of state save/load: snapshotting a Loom
runtime is fundamentally "snapshot the ledger + snapshot the playhead
stack."

The playhead moves through three kinds of step:

| Step | Causes |
|---|---|
| **Fall-through** | end of a section reached → enter the textually-next section, unless one of the modifiers below applies |
| **Divert** | `->` jumps to a named target; the previous section's playhead is discarded |
| **Return** | `<-` pops one frame off the tunnel stack and resumes the caller |

Section modifiers change fall-through behavior:

- `.hub` — at fall-through end, jump back to the top of this section
  rather than the next textual section. Hubs are how you express
  conversation menus that "stay" until the player explicitly leaves.
- `.return` — at fall-through end, behave as if the player executed
  `<-`. Used inside tunnels and at the end of side-conversations.
- `.once` — entering this section a second time is a no-op divert;
  the playhead falls through immediately. Combine with `.hub` for
  one-shot menus.

**Tunnel stack.** A divert of the form `-> (target) ->` (a *tunnel
call*) pushes the *caller's continuation* onto a tunnel stack before
diverting. The matching `<-` (or end of a `.return` section) pops one
frame. Tunnels nest; depth is bounded only by host memory. The stack
is part of the playhead state and is snapshotted with it.

### 6.2 Visit counter and ledger writes

Every section carries a per-save **visit counter**. The counter
increments on entry (after `Guard` evaluation, before body execution).
It's the substrate that powers `visits(section_id)`, `played(...)`,
and `each visit { first / then / finally }`.

Every player-visible step also writes a ledger envelope:

| Step | Ledger entry |
|---|---|
| Section entered | `SectionEntered { section, visit_n }` |
| Choice taken | `ChoiceTaken { section, choice_id, label, at_ms }` |
| Dialogue line played | `LinePlayed { speaker, text, section }` |
| Divert fired | `Diverted { from, to, kind }` (kind ∈ hard/tunnel/return) |
| Cue fired | `CueFired { name, payload }` |
| Var/knowledge/disposition mutation | `Mutated { lvalue, before, after }` |
| Hook fired | `HookFired { character, hook_id, payload }` |

Ledger entries are the wire format for save/load, replay, multi-user
playback in the booth, and `since(event)` / `last(event)` queries
(§7's `LedgerPred` set, grammar §11.6). Studios that need analytics
subscribe to the same stream — there is no parallel "telemetry"
channel.

### 6.3 Conversation engine state

Beyond the playhead and the ledger, the per-conversation runtime
holds:

- **Once-flags.** Per-save, per-choice. A `*` choice in
  `start.choice_5` is suppressed on subsequent entries to `start` once
  taken. The flag set is part of the snapshot.
- **Sticky-flags.** `+` choices are never suppressed; the absence of
  their flag is informational only.
- **Active threads.** Names of in-flight tunnels (the tunnel stack
  serialized).
- **Pending `each visit` index.** Per-section, the index into the
  visit-branch list. Saved so reload doesn't restart the cycle.
- **Reactive subscription handles.** `let`-bindings hold live
  `Memo<T>` handles into `prism-core::reactive`. These do not
  serialize — they're rebuilt on load by re-evaluating the binding's
  source against the freshly hydrated ledger.

The runtime exposes the snapshot as `serde`-derivable
`ConversationState` so hosts can persist with whatever storage they
already use.

### 6.4 Quests, cutscenes, barks

All three are document archetypes (`:quest`, `:cutscene`, `:barks`)
with the same content language as conversations but specialized
top-level shapes — see grammar §6.

Quests have stages with objectives:

```loom
# find_maren :quest

-- investigate "Investigate the Lighthouse"
  > The bell at Saltmere has gone silent.
  .objective talk : talk @WREN
  .objective enter : reach @lighthouse_interior
  .on-complete -> choose_approach
```

Cutscenes are timecode-driven:

```loom
# bell_rings :cutscene .skippable

at 0.0  :audio @ambience.wind .fade-out 2.0
at 2.0  The first strike shakes dust from the rafters.
at 2.0  :audio @sfx.bell_strike_first
at 5.0  WREN { wonder, whispering }
          The bell.
```

Barks — historically a separate archetype — are now just **generators**
that yield dialogue (§9.3). The `:barks` tag still exists as sugar
for "this file is a generator collection".

---

## 7. The three sigils

Loom's most-asked clarification, in one table:

| Sigil | Question | Resolves at | If undefined |
|---|---|---|---|
| `$x` | "What is this **right now**?" | Runtime | Build error: unknown var/role/entity |
| `@x` | "Does this **exist** in the project?" | Build time | Build error: unknown asset / scene / character / cue |
| `[[x]]` | "What is this **related to**?" | Author time | Warning: dead link (build still succeeds) |

Three commitment levels. **The test: delete the codepoint.** If the
build now fails, it was `@`. If the script behaves differently at
runtime, it was `$`. If the show plays correctly, it was `[[ ]]`.

`@` and `[[ ]]` are kept separate because the **failure contracts
differ**: `@elena` declares *this script needs Elena*; `[[elena]]`
declares *this prose mentions Elena*. Unifying would force one
failure mode and break either the build (too brittle for `[[ ]]`'s
drafting use case) or the safety (too loose for `@`'s execution use
case). Two sigils, two contracts.

Inside text, `@` typically appears where a script-level reference is
needed (`<sfx:@bell>`), and `[[ ]]` where a hyperlink should
render:

```loom
WREN
  $LISTENER.name, you should ask [[elena|Elena]] about the keeper.
  She was at @lighthouse_interior the night it happened.
  The [[Bell of Tides]] hasn't rung since.
```

Resolution chain for `$x` (first match wins):
1. Participant scope (`as participant` sections — `$x` tries
   `$PARTICIPANT.x` first)
2. Conversation roles (`$SPEAKER`, `$LISTENER`, `$PLAYER`,
   `$PARTICIPANT`, …)
3. Local `let` bindings
4. Vars / stats / character fields
5. Cohort membership

---

## 8. Live performance

> Live immersive theatre is what falls out of the rest of the language
> when you make **participants** first-class. The pieces in this
> section are all that's needed; everything else (cast, story, stats)
> works the same on stage as it does on a controller.

### 8.1 Cast

```loom
cast BELLKEEPER
  .label "The Bellkeeper"
  .open                      # any performer can be bound at runtime
  .improv .latitude(0.5)     # 0=strict, 1=fully improvised
```

`SPEAKER` in dialogue refers to a cast slot, not a performer. Binding
is a runtime op via `Cast.assign(slot, performer)`.

In a game variant, performers are entity references
(`Cast.assign("WREN", @elena)`). In theatre, they're actor IDs. In an
immersive show with floating ensembles, `.open` cast slots accept
whoever the booth assigns.

### 8.2 Cues

Two ways to address the crew bus:

```loom
# inline (one-shot, anonymous)
WREN
  The bell.<sfx:distant_bell>

# named (declared once, fired anywhere)
cue lights_warm
  .target lighting
  .preset warm_amber
  .fade 2.5s

# elsewhere:
~ cue lights_warm
WREN
  Listen.<cue:lights_warm> The light changes.
```

Inline triggers for moment-of-juice. Named cues for show-flow. Both
land on the same `CueFired { name, payload }` event on the crew bus.

### 8.3 Participants, cohorts, locations

```loom
cohort initiate
  .label "The Initiates"
  .capacity 24

location BELL_TOWER
  .label "The Bell Tower"
  .capacity 12

when participant joins
  enroll $PARTICIPANT into initiate
  -> orientation as $PARTICIPANT

when participant enters @BELL_TOWER
  -> bell_first_visit as $PARTICIPANT
```

The `as participant` modifier scopes a section's state to one
audience member. Inside such a section, `$trust` means
`$PARTICIPANT.trust`, not a show-global var. Many participants can be
in the same `as participant` section simultaneously without
interfering.

### 8.4 Broadcast scopes

```loom
broadcast :participant($PARTICIPANT)
  NARRATOR { whispering }
    You hear it differently. You always have.

broadcast :location(@BELL_TOWER) but :participant($PARTICIPANT)
  NARRATOR
    The others look up.

broadcast :cohort(singers) and :location(@BELL_TOWER)
  ~ cue private_choir
```

Scope atoms compose with `and` (intersection) and `but` (set
difference). The runtime applies the scope filter to the broadcast's
content stream.

### 8.5 Improv

```loom
BELLKEEPER (improv .duration(45s))
  > Greet warmly. Find out why they came. Don't reveal the singing —
  > that's act two.

  -> next_beat
```

The indented body is a directive to the performer (the leading `>`
makes it a flavor line, never spoken aloud). The runtime:

- displays the directive on the performer's prompter (a Luau-driven
  PRUI surface served by `prism-relay` to their device);
- holds the playhead until the stage manager / performer advances it
  (foot pedal, tap, speech-recognition trigger), *or* until
  `.duration` elapses;
- writes `ImprovBeatStarted` / `ImprovBeatAdvanced` to the ledger.

Mid-line improv between scripted lines:

```loom
BELLKEEPER
  Why are you here?
  (improv ~30s about: "what you might do")
  And what will you do about it?
```

### 8.6 Live patching

The booth UI — a Prism app driven by the same `LoomRuntime` — can:
- skip a participant past a beat;
- force-fire a cue;
- re-cast a role;
- retire a participant who left;
- hot-reload a `.loom` patch without stopping the show.

Every booth action is a Loro CRDT op on the same store the rest of the
runtime reads. The booth is just another client.

---

## 9. Reactivity & dynamics

> The goodies that turn Loom from a static branching tree into a
> living system. Reactive bindings, coroutine-based generators,
> scene state machines, and time-based scheduling — all built from
> three primitives: `let`, `wait`, `yield`.

### 9.1 Reactive `let`

`let` is a **reactive binding** — a live formula whose value updates
whenever its dependencies change.

```loom
let trusted    = $elena.disposition($PLAYER).trust > 50
let in_danger  = $health < $max_health * 0.3
let crowd_size = count(@all_participants in @bell_tower)
```

Backed by `prism-core::reactive::Memo<T>`. Conditions that read
`$trusted` subscribe to it. Mutating `$elena.disposition.trust`
schedules a single re-evaluation; downstream effects fire once.

### 9.2 Time primitives

```loom
wait 30s                       # pause
wait until $bell_rung          # gate on condition
wait until $weather is storm   # gate on equality
at 6am                         # schedule absolute (in-world clock)
every 15m                      # schedule periodic
every random(20s, 60s)         # randomised interval
```

These are statements that yield control to the scheduler. They appear
inside generators (§9.3) and scenes (§9.4); outside them they're
parse errors.

### 9.3 Generators

A **generator** is a long-running coroutine that yields content to
the runtime. Generators replace the legacy bark-set archetype with
something far more expressive.

```loom
generator harbor_chorus

  loop
    wait random(20s, 60s)

    if $storm_active
      [DOCKHAND / FISHER] { worried }
        [Storm's close. / Sky's wrong. / Time to tie down.].shuffle

    if $bell_ringing
      VILLAGER { surprised }
        The bell! It's ringing!

    otherwise
      [FISHER / VILLAGER]
        [Quiet night. / Stars are out. / Tide's calm.].cycle
```

A generator runs until explicitly cancelled. The runtime starts named
generators on world boot or via `~ start <generator>`. Multiple
generators run concurrently. They are character-bound (declared
inside a `# X :character` body) or world-bound (top-level).

Inline generators inside characters are how routines and idle
behaviors are expressed:

```loom
# elena :character

  generator daily_routine
    at 6am   go_to @home
    at 8am   go_to @bakery
    at noon  go_to @harbor
    every random(15m, 45m)
      if at @harbor and $weather.fog
        yield bark from @harbor_fog_chatter

  generator stress_reactions
    when $elena.stress > 70
      yield with_chance(0.3)
        ELENA { exhausted }
          I need a moment.
```

`yield` produces content (a dialogue line, a bark, an event) and
hands control back to the scheduler. `yield bark from @set` picks
from a named bark set with saliency scoring. `yield with_chance(P)`
yields the body P fraction of the time, skipping otherwise.

### 9.4 Scenes

A **scene** is a labelled coroutine — a multi-step interaction with
explicit state transitions. Where a generator yields ambient content,
a scene drives a focused exchange.

> **The word "scene" pulls double duty.** In `:script` and `:film`
> documents, `##` introduces a *screenplay* scene header — a typeset
> slugline like `## act_2.scene_3 "INT. HARBOR — DUSK"`. That's a
> sub-section heading, parsed by [grammar §5.9](loom-grammar.md#59-scene-for-script-and-film-archetypes).
> Here in §9.4 the keyword `scene` declares a *reactive* scene — a
> coroutine. The two forms never collide: the screenplay form takes a
> `##` slug; the coroutine form takes a parameter list (`|...|`) and a
> body of labelled state blocks. The shared term reflects shared
> intent — both are "a focused unit of action" — and disambiguation
> falls out of position alone.

```loom
scene patrol |character, route|

  for waypoint in route
    $character.go_to(waypoint)
    wait until $character.at(waypoint)
    wait random(3s, 8s)

    if $character.spotted_intruder
      -> investigate as $character
      return

scene investigate |character|
  approach
    $character.disposition($PLAYER).suspicion += 5
    wait until $character.at($PLAYER.position)
    -> examine

  examine
    $character { focused }
      Hmm. Something's not right.
    wait random(2s, 4s)
    if $character.deduction > 60  -> confront
    -> withdraw

  confront
    fire $character.investigated($PLAYER, found_clue)
    return found_clue

  withdraw
    fire $character.investigated($PLAYER, none)
    return none
```

Scenes have parameters, labelled states (the inner bare-name
sections), `wait`, `yield`, `return`. They're invoked from story or
other scenes: `-> investigate as $elena` (fire-and-forget) or `let
result = run investigate($elena)` (await return value).

A scene compiles to a Luau coroutine; the runtime schedules it.
Cancellation is explicit (`cancel @scene_id`) or implicit (cancelling
the owner — when Elena despawns, all her scenes cancel).

### 9.5 List comprehensions

```loom
let allies   = [x for x in @characters where x.faction == @PLAYER.faction]
let nearby   = [x for x in @characters where x.distance($PLAYER) < 10]
let hostile  = [x for x in nearby where x.disposition($PLAYER).is(hostile)]

if any(hostile)
  -> retreat
```

The comprehension grammar (`[expr for x in iter where pred]`) covers
the queries narrative writers actually run. Aggregate functions —
`any`, `all`, `count`, `min`, `max`, `closest`, `first`, `last` — are
the ledger-predicate family extended to live collections.

### 9.6 Procedural text

For text that varies more than `[a / b / c].mode` can express,
**compose** invokes a grammar:

```loom
compose harbor_threat

  pattern $intensity
    low    : "[A whisper / A murmur / A trace] of trouble [at / by] [the docks / the breakwater]"
    high   : "[Hell / Chaos / Ruin] is breaking loose [at / by] [the docks / the breakwater]"

DOCKHAND { afraid }
  ${compose harbor_threat intensity:$storm.intensity}
```

`compose` is a named context-free grammar with conditional patterns.
The output is a string suitable for splicing into dialogue. Useful
for procedural quest descriptions, headlines, rumours, system
messages.

### 9.7 Spawning and joining

```loom
let scene = spawn investigate($elena)        # start a scene, don't wait
let result = await scene                     # join later

spawn harbor_chorus                          # generator
spawn daily_routine($wren)

cancel @scene_id                             # stop a running scene/generator
```

The runtime's scheduler is fair-share across concurrent generators
and scenes. Per-character generators have a single-process invariant
(only one routine generator active per character at a time) — others
queue.

---

## 10. Factions

> Factions are the **fourth layer** of the world model — the
> political dimension. A faction is a *named, addressable collective*
> with members, state, stance toward other factions, collective
> goals, and a life cycle. They can be declared up front; they can
> also *emerge at runtime*, especially in immersive theatre where
> participants form their own.

The trinity gets you everything a single character can do; factions
get you everything multiple characters can do *as a group*. That
includes alliances, betrayals, uprisings, congregations, mobs,
guilds, parties, crews, cohorts-with-teeth — every collective whose
state and stance need to be referenced from story.

### 10.1 Declaration

The simplest faction: an id, a label, a member list, and a default
stance toward everyone else.

```loom
# rebels :faction
  .label  "The Bell Rebels"
  .color  #ff6b35
  .sigil  @sigil.broken_bell
  .home   @lighthouse_interior

members
  @WREN, @ELENA                              # explicit characters
  cohort initiate                            # plus everyone in this cohort

state
  morale      = 0..100, init 60
  resources   = 0..100, init 20
  notoriety   = 0..100, init 10

stance
  @loyalists           = hostile
  @merchants           = wary
  @neutral_observers   = neutral
  default              = neutral             # what new factions get

goal restore_the_bell
  priority    = 0.9
  active_when = $rebels.morale > 30
  completes_when = $bell.functional
  drives generator coordinate_rebellion
```

A `:faction` document is the heaviest form. Lightweight factions also
work as inline declarations inside any document:

```loom
faction lighthouse_keepers
  members @WREN
  state morale = 50
  stance default = friendly
```

The body of a faction declaration uses the same building blocks as
characters — `members`, `state`, `stance`, `goal`, `on` (hooks),
`generator`, `scene`. Where a character is *one* entity, a faction is
*many entities + collective state*. The vocabulary is shared on
purpose; the runtime knows which is which from the archetype.

### 10.2 Stance

The **stance** between two factions is the political axis of the
world model — what colour the relationship paints onto every
character belonging to one when interacting with a character in the
other.

```loom
stance
  @loyalists           = hostile
  @merchants           = wary
  @neutral_observers   = neutral
  default              = neutral
```

Stance is drawn from a small named set: `allied`, `friendly`,
`neutral`, `wary`, `hostile`. Studios register more via the
extension API (§11) — `vassal`, `tributary`, `oathbound`, whatever
the show needs. The runtime maintains stance as a *symmetric matrix
by default* (mirror is implicit) — declaring `rebels → loyalists =
hostile` also makes `loyalists → rebels = hostile`. Asymmetric stance
exists for one-sided relationships:

```loom
stance
  @loyalists           = hostile asymmetric    # we hate them; they may not yet know
```

**Mutation.** Story mutates stance with `:=`:

```loom
~ stance($rebels, $loyalists) := allied
~ stance($rebels, @merchants) := hostile asymmetric
```

The mutation writes one `StanceChanged { a, b, before, after, at_ms }`
ledger entry. Hooks (§4.8 / §10.5 / §10.7) subscribe to this stream.

**Querying.** Stance is a first-class condition atom:

```loom
if stance($PLAYER.faction, @rebels) is hostile
if stance($PLAYER.faction, @rebels) is_at_least friendly       # ≥ on the ordinal scale
if $PLAYER.faction has_stance(@rebels)                         # any non-default
```

The ordinal scale (`hostile < wary < neutral < friendly < allied`)
makes `is_at_least` / `is_at_most` work naturally. Extension stances
register their position in the scale.

### 10.3 Membership

Membership is a *live set*. Three sources contribute:

```loom
members
  @WREN, @ELENA                              # explicit characters
  cohort initiate                            # plus a cohort
  match $X.disposition($WREN).trust > 60     # plus a reactive predicate
```

The `match` form is the most powerful: any character matching the
predicate is *automatically* a member while the predicate holds and
leaves when it stops. This is how loyalty mechanics fall out for
free — Elena joins the rebels when her trust in Wren passes 60, and
leaves if Wren betrays her.

**Mutation.** Story can join / kick explicitly:

```loom
~ $rebels.members += @maren                  # add
~ $rebels.members -= @gareth                 # remove
~ $rebels.disband                            # remove everyone, mark dissolved
```

Each mutation writes `MembershipChanged { faction, character, kind }`
(`kind` ∈ `joined` / `left` / `kicked` / `dissolved_with`).

**Inverse query.** A character knows its faction memberships:

```loom
$elena.factions                              # list<faction>
$elena.faction                               # primary; nil if none
if $elena.in_faction(@rebels)
```

A character can belong to multiple factions (e.g. a noble house *and*
a merchant guild). The *primary* faction is the highest-priority one
by `.primary_priority`; ties go to declaration order.

### 10.4 Collective state

A faction's `state` block declares numeric axes — exactly the same
shape as a character's `disposition` axes (§4.3) — that the runtime
tracks per-faction:

```loom
state
  morale      = 0..100, init 60
  resources   = 0..100, init 20
  notoriety   = 0..100, init 10
  cohesion    = 0..100, init 80
```

Story mutates these via the same `:=` / `+=` / `-=` action surface
characters use. Two synthesized accessors come for free on every
faction:

- `$F.size` — the live count of `$F.members`.
- `$F.age` — wall-clock duration since `$F` was instantiated
  (declared factions instantiate at world boot; emergent factions at
  spawn time).

Plus all of `$F.state.<axis>` for declared axes.

### 10.5 Collective goals

A faction's `goal` block is the same state machine as a character
goal (§4.6), except `active_when` / `completes_when` / `fails_when`
have visibility into collective state, member positions, and stance:

```loom
goal restore_the_bell
  priority       = 0.9
  active_when    = $rebels.morale > 30 and $rebels.size >= 5
  completes_when = $bell.functional
  fails_when     = $rebels.size < 3 or $rebels.morale < 10
  drives generator coordinate_rebellion
  on_complete    -> celebration as $rebels
  on_fail        -> dissolve_faction $rebels
```

`drives generator G` here means: while this goal is pursued, every
member runs `G` as a character-scoped generator with `$FACTION` and
`$GOAL` bound. The runtime fans the generator out to current members
and tears it down on removal.

### 10.6 Templates and organic emergence

> **The headline feature.** A faction template is a *recipe* — a
> declared shape that the runtime can instantiate into new factions
> at any time. This is how Loom expresses the live-immersive case:
> participants gather in a corner, a moderator (or the runtime
> itself) calls `Faction.spawn(template, ...)`, and a fresh faction
> exists with state, stance, goals — all wired into the same
> narrative engine.

```loom
# crowd_faction :template :faction
  .min_members 3
  .max_members 30
  .auto_dissolve_when $size < $min_members
  .default_stance neutral
  .primary_priority 0.2          # below permanent factions in the primary slot

state
  cohesion    = 0..100, init 50
  visibility  = 0..100, init 30

on member_count passes 10
  ~ $self.template := @uprising      # template evolution (10.8)

on member_count drops below $min_members
  ~ $self.dissolve
```

Spawning at runtime, from any action context:

```loom
let f = Faction.spawn {
  template: @crowd_faction,
  label:    "The Sea-Wall Crowd",
  founder:  $PARTICIPANT,
  founding_members: [$PARTICIPANT],
  initial_state: { cohesion: 70 },           # override defaults
}
```

`Faction.spawn` returns a *resolve* — `$f` is bindable like any other
participant or character handle. The new faction is immediately
addressable by every part of the narrative engine that already
understood declared factions.

**Dissolution.** Factions go away through:
- explicit `~ $f.dissolve` action,
- `auto_dissolve_when` predicate becoming true,
- last member leaving (configurable per template).

Dissolution writes `FactionDissolved { faction, reason }` and runs
each member-character's `on $faction dissolves` hook (§10.7) before
removing the faction from the registry.

**Template evolution.** A faction can change template at runtime
(`~ $self.template := @new_template`). The new template's `state`
axes are merged onto the existing ones (existing axis values are
preserved; new axes initialize); goals are replaced; default stance
re-resolves. Useful for "the crowd becomes a riot becomes a faction"
arcs.

### 10.7 Narrative pattern-matching against emergent factions

> The hardest design question: *how do you write story that responds
> to a faction the author didn't know would exist?* Loom's answer:
> write against **templates**, **roles**, and **lifecycle events**,
> not against individual `@ids`.

Three handles, ranked from most specific to most general:

**By template:**

```loom
when faction emerges from template @crowd_faction
  let $C = $FACTION
  NARRATOR
    Something is gathering at the harbor.
  -> investigate_crowd as $C
```

**By role:**

Roles are reactive `let` bindings whose values are factions:

```loom
let dominant_faction = first(@factions sorted_by .size desc)
let player_faction   = $PLAYER.faction
let player_enemies   = [f for f in @factions
                        where stance(player_faction, f) is hostile]

if any($player_enemies)
  -> hostility_warning
```

The `dominant_faction` binding recomputes whenever any faction's
size changes. Sections can `as faction` scope to a role:

```loom
-- crisis_moment as $dominant_faction
  ...
```

Inside, `$state.cohesion` means `$FACTION.state.cohesion`.

**By lifecycle event:**

```loom
when faction emerges                         # any faction, any template
when faction dissolves
when faction grows                           # member added
when faction shrinks                         # member removed
when stance changes                          # any stance edge
when stance($A, $B) changes                  # specific pair
```

Inside, `$FACTION` (and `$OTHER`, for stance pairs) are bound.

**Catch-all:**

A `match $faction.template` covers the "we know there will be
*some* uprising and want to write the response":

```loom
when faction emerges
  match $FACTION.template
    @crowd_faction
      -> harbor_crowd_response
    @uprising
      -> uprising_response
    @merchant_guild
      -> trade_response
    _                                       # studio-extended template
      -> generic_emergence_response
```

Writers who want to ignore emergent factions entirely can — every
construct here is opt-in. The default contract is "code that doesn't
mention factions is unaffected by them."

### 10.8 Live performance integration

In `:immersive` documents, factions are the first-class output of the
"audience self-organizes" loop. Three constructs make it work.

**Participants propose factions.**

```loom
when participant proposes faction
  let f = Faction.spawn {
    template: @crowd_faction,
    label:    $PARTICIPANT.proposed_label,
    founder:  $PARTICIPANT,
    founding_members: [$PARTICIPANT],
  }
  broadcast :location($PARTICIPANT.at)
    NARRATOR { whispering }
      Someone is starting to gather followers.
  -> faction_announce as $f
```

The booth UI (or a participant-facing prompter — see §8.5) exposes a
"start a faction" affordance; the runtime fires this lifecycle when
they confirm.

**Participants join existing factions.**

```loom
when participant joins faction
  enroll $PARTICIPANT into $FACTION             # adds to members
  if $FACTION.template == @crowd_faction
    ~ $FACTION.state.cohesion += 5
```

**Faction-scoped broadcast.** Cues, dialogue, and narration can be
scoped to a faction's members:

```loom
broadcast :faction($rebels) and :location(@BELL_TOWER)
  ~ cue private_signal
  NARRATOR { whispering }
    Now.
```

`:faction(F)` is a scope atom alongside `:cohort`, `:location`,
`:participant`, `:cast` (§8.4). It composes with `and` / `but` just
like the others. The runtime evaluates membership live — a
participant who joined a faction mid-show receives faction broadcasts
from the moment of joining.

**Booth controls.** The booth UI (the Prism app reading the same
LoomRuntime) can `Faction.spawn`, `Faction.dissolve`,
`Faction.set_template`, `Faction.add_member`, `Faction.set_stance`,
and live-edit goal predicates — all as Loro CRDT ops on the same
store. A stage manager who sees the crowd splitting into two camps
can spawn a second faction from the console and the narrative engine
picks it up at the next tick.

### 10.9 What factions are *not*

Calling out the design boundary so the next reader doesn't have to
reverse-engineer it:

- **Not cohorts.** Cohorts are flat membership sets without state,
  stance, or goals. A faction *can have* a cohort as a member
  source; the cohort stays simple.
- **Not character traits.** A character's `disposition($PLAYER)` is
  a one-to-one relationship axis. A faction's stance is a *bilateral
  political stance* between two collectives. You can model both
  simultaneously (Elena distrusts the player but is loyal to the
  rebels who are allied with the player).
- **Not type hierarchy.** Templates are runtime instantiation
  recipes, not OO subclasses. Two factions sharing a template don't
  share identity, and a faction can change template at runtime.
- **Not events.** Factions are *entities*. Events about them
  (emergence, growth, stance change) flow through the same ledger
  every other entity uses.

---

## 11. Extension model

Studios extend Loom by **registering** new constructs through Luau at
workspace boot. Extensions are **additive only**.

Registerable:

| Kind | Example |
|---|---|
| Action | `remember journal_X` |
| Guard | `unless betrayed` |
| Block | `flashback "three days ago"` |
| Trigger type | `<vfx:sparkle,0.5>` |
| Inline delimiter | `<<stage whisper>>`, `~~corrupted~~` |
| Variation mode | `[a / b / c].weighted(0.7, 0.2, 0.1)` |
| Scene template | a horror-game `investigation` scene |
| Cue target | a new lighting protocol (sACN, OSC, MIDI) |
| Resolve role | `$NARRATOR`, `$COMPANION[0]`, `$STAGE_MANAGER` |
| Char-block category | `{ aggressive }` matches a `stance` category |
| Annotation | `@status final`, `@vendor:rating teen` |
| Character type | a custom subtype of `@humanoid` |
| Slot | a new component slot (`@studio:psychology`) |
| Stat advancement mode | beyond the built-in five |
| Stance level | a new entry on the ordinal stance scale (`vassal`, `tributary`) |
| Faction template | a runtime-instantiable faction recipe |

Not registerable (invariants):

- Structural sigils, bracket families, the three reference sigils,
  the three operators.
- Speaker detection (ALLCAPS or `$UPPER`).
- Indentation semantics.
- The four archetypes (character / stats / faction / story) and the
  live constructs.

At workspace boot every `*.luau` in `workspace/loom/extensions/` runs
against a staging registry. After all extensions execute, the staging
registry is frozen, diffed against the core, and merged. Conflicts
are build errors. There is no mid-session registration.

---

## 12. Implementation notes

> Brief — full implementation plan in `loom-impl.md` once the work
> starts.

**State store.** All mutable runtime state lives in a
`loro::LoroDoc`. Top-level maps for show, participants, cohorts,
cast, characters, ledger, locations. Per-participant scoping resolves
via the `participants[id]` namespace transparently. Every mutation
is a CRDT op with a version vector — `snapshot()` captures, `restore`
applies a diff in O(ops since).

**Save / load.** Serialize the `LoroDoc` plus playheads. Content-
addressed by `STRUCTURAL_HASH`.

**Hot reload.** Two fingerprints per document: structural
(IDs, topology, declared names) and full (text). Text-only edits
patch live. Structural edits resync at the next section boundary.
Same substrate as `.prui` literal-only updates.

**Rust ↔ Luau split.** The resolver, all engines, and the build-time
compilable subset of expressions run in Rust. Luau handles: condition
expressions outside the subset, user-registered action handlers,
generator / scene bodies, improv prompter UI, extension registration.
A Luau VM call is never on the resolver inner loop.

**Codegen.** `LoomDatabase` lowers to `SymbolDef[]` and runs through
Prism's `CodegenPipeline`. Out: `Loom.g.rs`, `Loom.d.luau`,
`Loom.g.ts`, `Loom.g.cs`, `Loom.g.gd`. IDE autocomplete for every
character / cue / section / slot / axis / stat / pool / tree node.

**Builder integration.** `.loom` compiles to
`prism_builder::Component` impls. Drop a `<loom-conversation
src="@harbor_arrival"/>` into a PRUI scene like any other widget.

---

## 13. Open questions

1. **Generator scheduling fairness.** Many characters with many
   routines + ambient generators — what's the scheduling policy?
   Round-robin per character is the obvious start; whether that
   holds under 100+ NPCs is empirical.
2. **Improv input modality.** Foot pedal, body-mic speech detection,
   stage-manager tap. Probably all of the above; the priority and
   failure modes need a real workshop.
3. **Cue-list interchange.** Theatre crews live in ETC Eos, QLab,
   ChamSys. Loom should export to their cue-list formats and ideally
   re-import after the crew edits in their own UI. Spec the
   round-trip.
4. **Per-character ledger vs single ledger.** Per-character is
   cleaner for immersive (each participant's ledger drives their
   barks) but storage cost is linear in participant count. A single
   ledger with participant-tagged events may be simpler. Benchmark
   before committing.
5. **Faction spawn churn under load.** A 200-participant show that
   lets anyone propose a faction at any time could spawn dozens per
   minute (most dissolving within seconds for failing
   `min_members`). The reactive substrate handles the predicates
   fine; the question is whether the *narrative engine* re-evaluates
   `when faction emerges` matches at acceptable cost when the
   spawn-then-dissolve rate is high. Likely fix: a settling window
   (a faction must persist N seconds before `emerges` fires), but
   N has to come from a workshop.
6. **Symmetric vs asymmetric stance defaults.** Symmetric (mirror)
   matches how most writers think and reads cleanest in the
   declaration grammar. Asymmetric matches *political reality*
   (one side declares hostility before the other knows). The
   current draft picks symmetric default + explicit `asymmetric`
   override. Whether to flip the default — particularly for
   immersive shows where information asymmetry is dramatic — is
   open.
7. **Faction-of-factions.** Can a faction be a member of another
   faction (alliances-as-factions)? The current model says no — a
   member is a character or a cohort, period — but the request will
   come up. Empirical question whether the simpler model holds.

---

## 14. References

- The formal grammar: [`loom-grammar.md`](loom-grammar.md).
- Legacy spec parents (read for context, not parity):
  `LEGACY-CODEBASE/loom-lang/` — the v1 TypeScript Loom and the
  Simulacra / Meridian companion specs.
- Sibling DSL docs: [`prui-reference.md`](prui-reference.md),
  [`prss-reference.md`](prss-reference.md),
  [`luau-integration-plan.md`](luau-integration-plan.md).
- Prism substrate: `prism-core::language::syntax`,
  `prism-core::language::codegen`, `prism-core::reactive`,
  `prism-daemon::modules::luau_module`,
  `prism-builder::ComponentRegistry`.
