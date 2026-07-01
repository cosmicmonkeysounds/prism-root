# Loom redesign: traits with `self`-parameters, faction badges, and the `SELF` speaker

> Status: proposal, hardened after hostile review. Lowers entirely into the
> existing `packages/loom/core/` model; the Rust mirror tracks it 1:1.
> Target file: `docs/dev/loom-functional-redesign.md`.
>
> **This document is in two parts.** Part I (§1–§8, below) establishes the
> spine: parameterized traits, faction badges, and the `SELF` speaker — the
> flat `is Trait(arg)` one-liner. **[Part II](#part-ii--composition--multiple-beats)
> (§9–§17) supersedes the "one flat `is`" story** with a `with`-per-line
> composition surface and class-owned (unique *or* trait-derived) beats. Where
> Part II amends a specific Part I passage, see its
> [*Edits to Part I*](#edits-to-part-i-keep-the-two-parts-consistent) note. Read
> both before implementing.

## 1. Motivation

`cast/algorithm.loom` is 91 lines, and ~25 of the ~30 characters across every cast file are the same four-line stamp: belong to a faction, and on scan route a guest to one beat.

```loom
CHARACTER Crawler
  faction: TheAlgorithm
  on scan guest
    -> crawler_report

CHARACTER Captcha
  faction: TheAlgorithm
  on scan guest
    -> captcha_gate
```

Three things repeat on every prop: `faction:` (identical), `on scan guest -> …` (identical shape), and the beat name that merely echoes the character's identity. The same disease shows up in the word block the prop routes to — `== crawler_report` opens `cast: Crawler` and then writes an ALL-CAPS `CRAWLER` speaker, naming the owner a second and third time.

A scanner prop is not a character. It is a character *shape*. Rust models a shape that types parameterize and instances implement: a **trait**. Loom already has most of the machinery — the `is X, Y` mixin clause (`Declaration.mixin`), a `TRAIT` kind that shares the `CharacterBody` shape, a `mergeCharacter` pass that resolves inheritance, and a `self` binding that is rebound per-owner at every hook dispatch. This redesign turns that machinery on **in the runtime path**, parameterizes it, and routes the restated speaker through the same `self`.

The design's north star: **`self` is the entity this code runs on behalf of**, and it is the single lever that collapses both layers. The moment an author writes `self.captures` instead of `TheAdmin.captures`, the line becomes liftable into a shared trait verbatim, because `self` is rebound to each owner at dispatch. The moment a beat writes `SELF` instead of `CRAWLER`, the dialogue speaks as whoever routed in.

---

## 2. The design

One spine — parameterized traits, the cheapest reuse of existing AST nodes — with the strongest ideas from four other candidate designs grafted on. Each construct below states its concrete syntax, its semantics, the exact AST/parser/runtime change, and — where it matters — the alternative I rejected.

### 2.0 The load-bearing wiring fix (do this first; everything depends on it)

The merge machinery exists and is correct, but **it is dead code in the runtime.** `bundle.mergedCharacters` is populated only by `Bundle.rebuildSimulacra()` (bundle.ts:147), and `rebuildSimulacra()` has exactly one caller in the entire tree: `test/bundle.test.ts:19`. The real runtime entry point, `Sim.fromSources` (sim.ts:122), builds a `Bundle`, pushes parsed files onto it, and calls `compileModel(bundle)` **without ever merging**, and `compileModel` reads the *raw* body (`charDef(decl.name, decl.character)`). So today `mergedCharacters` is an empty `Map` in every real sim, and every `is` clause — even a bare `is Algo` — is silently inert.

> **Review correction.** An earlier draft proposed `compileModel` read
> `bundle.mergedCharacters.get(name) ?? decl.character` and claimed bare `is Algo`
> would then factor `faction:`. That read *always* hit the `?? decl.character`
> fallback because the map is never filled. The fix below actually runs the merge.

**Change 1 — trigger the merge.** At the top of `compileModel` (model.ts), before the file walk:

```ts
export function compileModel(bundle: Bundle): SimModel {
  bundle.rebuildSimulacra();        // populate mergedCharacters / factions / …
  const model: SimModel = { /* … */ };
  // …
```

**Change 2 — read the merged body** in the `role` and `character` cases:

```ts
const body = bundle.mergedCharacters.get(decl.name) ?? decl.character;
model.characters.set(decl.name, charDef(decl.name, body));   // and roleDef(...) for `role`
```

`charDef` reads `body.properties.get("faction")`, and `mergeCharacter` copies a parent's `faction:` into the child's properties whenever the child doesn't declare its own (bundle.ts ~300). So after these two edits, `CHARACTER Crawler is Algo` resolves `faction: TheAlgorithm` at sim time with **no new syntax** — that is Slice 1. The `?? decl.character` fallback preserves any character the abstractness check legitimately drops.

**Change 3 — make `rebuildSimulacra` idempotent.** `compileModel` may run more than once on a bundle (tests, re-compiles), and `rebuildSimulacra` clears every output map it owns *except* `projectDiagnostics`, so a second run would double-report `requiredSlotUnfilled` / `ambiguousSlot`. Add `this.projectDiagnostics = []` to the top of `rebuildSimulacra` (it is the sole producer of those diagnostics — verified by grep: the only `.push` sites are the abstractness check and `mergeCharacter`). The map clears already present at the top make the rest of the pass idempotent.

`compileModel` already receives `bundle`, so no signature change anywhere.

### 2.0a Roles are schemas, not instances — exempt them from the abstractness drop

Even with §2.0 wired, `is` on a `ROLE` silently merges nothing. `ROLE Guest` declares `faction: any of FACTION` (main.loom), which lowers to a `slotType {kind:"anyOf"}` with no default. The abstractness check (bundle.ts:240-260) calls `slotTypeIsRequiredHole` → `true` for that slot, decides `Guest` is "abstract," pushes `requiredSlotUnfilled`, and `continue`s **without adding `Guest` to `mergedCharacters`.** `compileModel` then falls back to the un-merged raw body and discards any mixed-in traits.

That check is correct for a `CHARACTER` (a concrete prop that left an `any`-shaped hole unfilled) but **wrong for a `ROLE`**: `any of FACTION` is a *per-person* slot, filled at runtime when a guest picks a faction (`roleDef` seeds defaults at model.ts:197; the sim fills `faction` per person). A role is a state schema, never an instance, so it is never abstract.

**Change.** `RawDecl` already carries `isTrait`; add `isRole` (set from `decl.kind === "role"` in the `rebuildSimulacra` collection loop at bundle.ts ~219). In the abstractness pass, add roles to `mergedCharacters` unconditionally:

```ts
if (raw.isTrait) continue;                       // traits aren't instantiated
if (raw.isRole) { this.mergedCharacters.set(name, mergedBody); continue; } // schemas: never abstract
// …existing required-slot drop for CHARACTERs only…
```

Without this, no role can ever use `is`, and the §4.2 "generalizes to ROLE" showcase is dead on arrival.

> **Rejected alternative.** Make `slotTypeIsRequiredHole` return `false` for
> `anyOf` globally. Rejected: it would also stop flagging a genuinely-abstract
> `CHARACTER` that forgot to fill an `any of LOCATION` slot — losing a real
> diagnostic. The hole-ness of `anyOf` is right; the *role-as-instance*
> assumption is what's wrong, so the exemption is scoped to roles.

### 2.1 Parameterized `TRAIT`

```loom
TRAIT Scanner(beat)
  on scan guest
    -> self.beat
```

**Semantics.** A `TRAIT` may declare positional parameters after its name. A parameter is referenced inside the body as `self.<param>` — an *associated item*, a hole the implementor fills, read through `self`. Everything else is an ordinary `CharacterBody`: properties, hooks, disposition, knowledge.

**Why `self.<param>` and not bare `beat` or `{beat}`.** Bare-token substitution is a footgun: a param named `heat` would rewrite real `self.heat` tokens. Braces (`-> {beat}`) are the line-noise the readability lens exists to catch. `self.beat` is hygienic (a qualified dotted path that cannot collide with bare world identifiers), reuses the one concept already doing real work, and reads aloud as a stage direction: "on scan, route to my beat." It captures the "associated item" idea **without** a separate `assoc beat: BEAT` jargon line — the parameter list already declares the hole, so no `assoc` keyword and no `: TYPE` annotation are needed.

**Lowering.** Add `params: string[]` to `CharacterBody` (ast.ts:372, paralleling `SceneBody.params` at ast.ts:476) and to `emptyCharacterBody` (ast.ts:387). In `lower()` (decl-body.ts:72), split the `trait` case out of the shared character/role case and give it the SCENE treatment (decl-body.ts:83-88 is the exact template):

```ts
case "trait": {
  const [name, params] = splitNameAndParams(decl.name); // decl-body.ts:310, already exists
  decl.name = name;
  decl.character = lowerCharacter(decl.body, diagnostics);
  decl.character.params = params;
  break;
}
case "character":
case "role":
  decl.character = lowerCharacter(decl.body, diagnostics);
  break;
```

No new AST node kind. `params` rides on the existing `CharacterBody`, exactly as it rides on `SceneBody`. `RawDecl` (bundle.ts) gains `params` from `decl.character.params` in the same collection loop that already reads `decl.mixin`.

### 2.2 Trait application with arguments (positional **and** named)

```loom
CHARACTER Crawler  is AlgoScanner(crawler_report)                 # positional
CHARACTER Sentinel is CellWatch(loc: Internet, signal: lockdown)  # named
```

**Semantics.** An `is`-clause entry is either a bare name (today's mixin) or a call `Name(arg, …)`. Positional args bind to the trait's `params` by position; named args (`p: v`) bind by name. A trait may be applied more than once with different args. **Named args are the recommended form for any trait with more than one parameter** — positional multi-arg (`CellWatch(Internet, lockdown)`) is order-blind and unreadable aloud; single-param traits (the nine scanners) are immune and stay positional.

**The `is`-clause is exactly one physical line — see §2.2a.**

**Lowering — two parser fixes, both forced and both small.**

1. **Top-level-comma split in the lexer.** `parseDeclarationOpener` (lexer.ts:438) does `mixinClause.split(",")`, which shreds `CellWatch(Internet, lockdown)` into two broken entries. Export `splitTopLevelCommas` (already implemented, decl-body.ts:786) and use it for the mixin clause. `mixin` stays `string[]`; each entry is now the raw call string, e.g. `"AlgoScanner(crawler_report)"`.

2. **A positional-aware reference parser.** `parseConstructorCall` (decl-body.ts:761) splits each arg on `:` and **silently drops colon-less args** (decl-body.ts:778), so it cannot read `AlgoScanner(crawler_report)`. Add a sibling that keeps both:

```ts
interface MixinRef { name: string; positional: string[]; named: Map<string, string>; }
function parseMixinRef(entry: string): MixinRef
```

It splits `name` off the `(`, splits the inner on `splitTopLevelCommas`, and routes each part to `named` (if it has a top-level `:`) or `positional` (value kept whole, so a multi-word value like `enters Internet` survives intact). `mixin` keeps its `string[]` shape, so every existing reader (`inherits: [...decl.mixin]` for ITEM/FACTION at decl-body.ts:100/108) is untouched; only the merge step learns to parse entries.

### 2.2a Grammar disambiguation: the one-line `is`-clause rule

The lexer is strictly line-based. Its only multi-line stitching (lexer.ts:114) triggers when a *continuation* line **starts with `(`** — a parenthetical. A declaration opener that ends in `,` is **not** continued: `parseDeclarationOpener` sees only the first physical line, and the wrapped remainder (`Pinged(event: betray, …)`) falls into the declaration body, where `splitProperty` rejects the `(` in the key, returns `null`, and the line is silently dropped with no diagnostic.

**Disambiguation rule (normative).**

> **An `is`-clause occupies exactly one physical line.** All trait applications
> for a declaration — bare names, positional calls, named calls, repeated
> applications — appear on the single opener line, comma-separated at top level
> (commas inside `(...)` are part of an argument list, not separators between
> applications). A declaration-opener line **may not** be wrapped.

The parser enforces this with a new diagnostic: **an opener line whose `is`-clause ends in a top-level `,` (or has unbalanced parens) emits `UnterminatedMixinClause`** rather than dropping the remainder. This converts the silent-drop failure into a pointed error.

This is fully consistent with the design's readability story. The recommended applications are short and single-argument; §4.1 aligns them in a tidy column. A line long enough to *want* wrapping (two named-arg applications) is precisely the smell that says "use inline hooks instead, or split into single-application traits on the character" — see §4.2, where the multi-application `ROLE` form is shown but explicitly *not* recommended for exactly this reason.

> **Rejected alternative (deferred): declaration-opener line-continuation.**
> We could mirror the parenthetical stitch and continue an opener whose
> `is`-clause has unbalanced parens or a trailing top-level comma. It is a real
> feature, but it touches the lexer's pass-2 stitch loop (which is currently
> gated narrowly on `startsWith("(")`) and risks interacting with indentation
> classification. Since **no recommended example needs it**, it is deferred to a
> Slice-3 nicety, gated on actual authoring demand. Until then the diagnostic
> makes the constraint legible instead of silent.

### 2.3 Merge-time substitution + composition

This is the only genuinely new logic, and it lives where mixin merge already lives: `mergeCharacter` (bundle.ts:277). Today it iterates `ownDecl.inherits` (a `string[]`) and recurses on each name. Change the loop to parse each entry, substitute, then merge:

```ts
for (const entry of ownDecl.inherits) {
  const ref = parseMixinRef(entry);
  let parentBody = mergeCharacter(ref.name, raw, mergedCache, diagnostics, visiting);
  const parentParams = raw.get(ref.name)?.params ?? [];
  if (parentParams.length > 0) {
    parentBody = substituteParams(parentBody, parentParams, ref, ownDecl.params, diagnostics, name);
  }
  // …existing property/hook/disposition merge of parentBody into mergedBody…
}
```

`substituteParams` binds each `(param, arg)` and replaces the whole dotted token `self.<param>` with the bound value across **every string-bearing field**:

- **`HookDecl.event`** — the trigger clause. *This field was missed in the earlier draft and is mandatory.* It is a standalone string (ast.ts:453), not a `RawLine` in `hook.body`. An event-parameterized trait (`on self.event`) whose `event` field is left as the literal `"self.event"` compiles to `parseTrigger("self.event") → verb="self.event"`, a hook that matches nothing the sim ever fires. With the field substituted, `Pinged(event: betray, …)` rewrites `event` to `"betray"` and `parseTrigger` yields `verb="betray"`. (No re-normalization step is needed: `normaliseHookEvent` is a pure function applied on demand during suppression/`super` matching, and `parseTrigger` reads the field directly at compile time. Substitute the event *before* the hook is merged into `mergedBody`, so suppression/`super` see the resolved clause.)
- **Hook body `RawLine.text`** — the directives/diverts.
- **Method bodies + `MethodDecl.inlineExpr`.**
- **Property values (`PropertyValue.value`).**

Each replacement uses the hygienic pattern `/(?<![\w.])self\.<param>(?![\w])/g`, so it cannot rewrite a longer dotted path or a partial identifier.

Because a parent's hooks are already lowered `HookDecl`s carrying raw `RawLine[]`, and `lowerRawBody` re-parses those lines at compile time (model.ts:226), the substituted `-> crawler_report` flows through the identical path a hand-written divert takes. **The sim never learns the word "trait."**

**Deep clone before substituting — mandatory.** `mergeCharacter` hands appliers a `cloneCharacterBody()` copy, but that helper is **shallow**: `hooks: [...b.hooks]` copies the array while sharing the `HookDecl` objects, which share their `RawLine` objects with the cache. Naïve in-place mutation of `RawLine.text` would let the *first* applier permanently rewrite `-> self.beat` inside the cached trait; every later applier of that trait would then find no `self.beat` and inherit the first applier's beat. (`Crawler` and `Captcha` would both route to `crawler_report`.)

`substituteParams` therefore **deep-clones** the body it operates on and never mutates its input. Add `deepCloneCharacterBody(b)` alongside the existing shallow `cloneCharacterBody`: it maps `hooks` to fresh `HookDecl` objects with freshly-copied `RawLine[]` (and copied `event`/`span`), deep-copies `methods` (bodies + `inlineExpr`), and copies `properties` into a new `Map` with cloned `PropertyValue`s. The cache continues to store the **un-substituted** merged trait; substitution is per-application on a private clone. The existing `visiting` cycle guard and cache key are `ref.name` (the trait name without args), unaffected by arguments.

**Composition / forwarding.** The replacement value depends on whether the arg is itself a parameter of the *applier*:

```ts
const replacement = ownDecl.params.includes(arg) ? `self.${arg}` : arg;
```

So `TRAIT AlgoScanner(beat) is Scanner(beat), Algo` forwards its own `beat` into `Scanner`: `Scanner`'s `self.beat` becomes `self.beat` again (`AlgoScanner`'s), staying unbound until a `CHARACTER` supplies a concrete value. A `CHARACTER` has no params, so its args are always concrete and the `self.` prefix is consumed. This is what lets the three-trait split below stay DRY without duplicating the routing hook.

**Argument-resolution diagnostic (closes the forwarding footgun).** Because forwarding and concreteness are distinguished only by whether the applier declares a param of that name, an author who copy-pastes a trait header and writes the *placeholder* word literally — `CHARACTER Crawler is AlgoScanner(beat)` — gets `beat` treated as a concrete beat named `"beat"`; the divert then silently no-ops (sim.ts only diverts `if (beat !== undefined)`). To make that loud: **when a positional/named arg is a bare identifier that resolves to neither a known beat/faction/location/event nor an enclosing param of the applier, emit `UnresolvedTraitArg { character, trait, param, arg }`.** This is computed at merge time against the project indices the bundle already has.

> **Rejected alternative: a forwarding sigil** (e.g. `is Scanner($beat)` for a
> forward vs. `Scanner(beat)` for a literal). Rejected for readability: forwarding
> happens only trait→trait, authored by whoever writes the small "dramatis-personae"
> trait glossary, never by a director applying a finished trait to a prop. Adding a
> sigil taxes the common (concrete) case to mark the rare one. The convention
> "a forward reuses the parameter's exact name" plus the `UnresolvedTraitArg`
> diagnostic covers the footgun without new punctuation.

**Required-param diagnostic.** If an applier leaves a trait param unbound, emit `RequiredParamUnfilled { character, trait, param }` — the same shape as the existing `requiredSlotUnfilled` ProjectDiagnostic. A trait that declares both a param and a real slot of the same name is a hard error (the substitution would be ambiguous).

### 2.4 Faction badges — split repetition along its two real axes

```loom
TRAIT Scanner(beat)                      # axis 1: routing, faction-agnostic
  on scan guest
    -> self.beat

TRAIT Algo                               # axis 2: the faction badge
  faction: TheAlgorithm

TRAIT AlgoScanner(beat) is Scanner(beat), Algo
```

The repetition in `algorithm.loom` has two independent axes — faction membership and scan→beat routing. Splitting them into composable traits keeps the **hard** cases bloat-free: a character that is not a scanner takes only `is Algo` and never inherits-then-suppresses an unwanted `on scan`. No `on scan: none` tax, no per-prop `faction:` line.

> **Rejected: faction-as-mixin** — `CHARACTER Crawler is TheAlgorithm`. It reads
> beautifully but conflates three orthogonal things into one axis (membership, the
> `faction:` identity slot, and behavior inheritance). Consequences: a prop in two
> factions is *harder* (two faction mixins collide on the single `faction:` key); a
> shared shape across different factions can't be expressed; and every future
> faction-default forces a suppression tax on non-conformers. I keep `faction:` as a
> single-valued identity property, set by a badge trait. The badge name is one extra
> word read once.

> **Rejected: a `-> self.beat` snake_case route convention** — deriving the route
> from the character name. It zero-configs exactly one prop in this project (beats are
> named thematically, not after their props) and hides where a prop routes. The
> destination stays explicit on the application line.

### 2.5 The `SELF` word-block speaker (universal, near-zero cost)

```loom
== crawler_report(guest)
  cast: Crawler, guest
  SELF
    Crawling. Indexing. Flag raised.
    Heat {guest.heat}. The Algorithm sees you now.
```

**Semantics.** `SELF` (and the alias `ME`) as a dialogue speaker resolves at runtime to the entity bound to `self` in the current frame — the prop whose hook routed here for a scan→beat flow, the affected person for a `ROLE` hook. It deletes the ALL-CAPS line that restates the owner and makes one beat reusable by whichever prop routed in.

**Lowering.** `isSpeakerLine` (lexer.ts:394) already accepts `SELF`, so it lexes as `DialogueBlock.speaker === "SELF"` with no parser/AST change. The only runtime change is at the dialogue-frame push (the `case "dialogue"` arm of `exec`, sim.ts ~573, which currently does `stack.push({ …, speaker: item.value.speaker })`):

```ts
const sp = resolveSelfSpeaker(item.value.speaker, b); // "SELF"/"ME" → label for b.get("self")
stack.push({ items: item.value.body, index: 0, bindings: b, speaker: sp });
```

`self` is bound in every hook binder — scan (sim.ts ~492), char (sim.ts ~478-480), timer (sim.ts 266/271), role (sim.ts ~464) — **and survives a divert unchanged**, because the divert reuses the caller's bindings verbatim (sim.ts ~591: `stack.push({ items: beat.body, index: 0, bindings: b })`). So `self` from `Crawler`'s scan is still present in `crawler_report`.

**Speaker-label casing (consistency fix).** Explicit speakers are recorded as the literal ALL-CAPS token (`CRAWLER`, `NARRATOR`). A naïve `b.get("self")` returns the declared id in its source casing (`Crawler`, `Cookie_Banner`), so a cue sheet would mix `Crawler` and `CAPTCHA` in the same speaker column. `resolveSelfSpeaker` therefore **normalizes to the project's speaker casing**: it looks up the character's declared display name if one exists, else upper-cases the id (`Crawler → CRAWLER`, `Cookie_Banner → COOKIE_BANNER`). SELF-attributed lines now match explicit speakers exactly, so the performer-facing speaker column stays uniformly ALL-CAPS.

**Fallback.** If no `self` is bound (a top-of-file beat played with no router — e.g. `doors_open`, which uses `NARRATOR`), keep the literal `SELF` and raise an LSP lint. Slice 3 resolves the fallback to the beat's first `cast:` member. A multi-speaker `SELF | OTHER` is undefined and lints.

### 2.6 Inline hook bodies + inline opener divert (bless what already works)

Kept as an **opt-in capability, not the primary collapse mechanism**: a hook body already runs the full beat grammar (`hooksOf` → `lowerRawBody`, model.ts:226), so a genuinely 1:1, prop-private script can live inline in `on scan guest` with no parser or runtime change. This is how the bespoke cases stay first-class — `ModBot` inlines `<capture>`, a richly branching `Cookie_Banner` can host its `<if>`/choices inline — without being forced into a parameter hole they don't fit.

I reject inline-everything as the *primary* mechanism (it never writes `on scan guest` once — it restates the hook on every prop, so the nine props never become one-liners). But as an option it costs nothing and covers the inline-behavior variety.

One small sugar worth a Phase-3 line: the **inline opener divert** `on scan guest -> captcha_gate`, for a one-line router pointing at a *shared* beat that must stay free. It is a ~6-line split of a trailing ` -> ` off the `on ` opener in `lowerCharacter` (decl-body.ts:513), synthesizing a one-line body. Not load-bearing once traits exist (`is Scanner(captcha_gate)` is already one line), so it is the last thing in, not the first.

### Rejected alternatives, at a glance

| Idea | Verdict |
|---|---|
| Currying / first-class behavior values (`let algo = scanner(faction: …)`) | **Rejected.** Concept cost (partial application) for marginal gain. The two-trait split achieves the same factoring with zero new concepts. |
| `{param}` brace substitution | **Rejected** for `self.<param>` — readable, hygienic, reuses `self`. |
| `assoc name: TYPE` declaration line | **Rejected** — the param list in `(…)` already declares the hole. |
| Faction-as-mixin + snake_case route convention | **Rejected** — conflates membership/identity/behavior; blocks multi-faction; hides routing. |
| Full character+beat fusion as the primary collapse | **Rejected as primary** (doesn't DRY `on scan`); **kept as opt-in**. |
| Forwarding sigil (`$beat`) | **Rejected** — taxes the common concrete case; the diagnostic + naming convention suffice. |
| Declaration-opener line-continuation | **Deferred** (§2.2a) — no recommended example needs it; the diagnostic makes the one-line rule legible. |

---

## 3. `self` semantics

There is exactly one notion of `self`: **the entity this code is currently running on behalf of.** It surfaces in two layers, reading the *same* binding.

**Class / trait layer.** Inside a `CHARACTER` or `TRAIT` hook, `self` is the owner. The runtime binds it per dispatch — scan binds `self = scanner`, char hooks bind `self = char.id`, timer hooks bind `self = ownerId`, role hooks bind `self = the affected person`. This is Rust's `&self`: a trait writes `<set: self.captures += 1>` **once**, and every character that mixes it in mutates its **own** `captures`. It is also why the design's first instruction to authors is *stop writing `TheAdmin.captures`, write `self.captures`* — only the `self`-scoped line is liftable into a shared trait. Trait **parameters** are a second, compile-time use of the same word: `self.<param>` is an associated item resolved at merge time, so by dispatch the runtime only ever sees concrete names and ordinary state.

**Word / dialogue layer.** A `SELF` speaker resolves to that same `self` binding on the current frame. Because the executor threads `frame.speaker` down through `<if>`/`<match>`/`<each visit>`/choice arms (the control-flow pushes carry `speaker: sp`), a `SELF` block nested inside `<if: guest.captured>` still speaks as the prop. The same word answers "whose method is this?" in code and "whose line is this?" on stage — and in a scan→beat flow they are the same entity, because the divert carries `self` through unchanged.

---

## 4. Full before / after

### 4.1 `cast/algorithm.loom` — 91 → 53 lines

```loom
# Cast — The Algorithm (hidden faction)
#
# Every Algorithm prop shares one shape: wear the faction badge, and on
# scan route the guest to one beat. That shape is two small traits — a
# router `Scanner(beat)` and a badge `Algo`, composed as `AlgoScanner`.
# Each plain prop is now one honest line. The four members that carry
# real behaviour take only the badge and write their hooks inline.

# ── Shared shapes (read once, like a dramatis personae) ──────────────
TRAIT Scanner(beat)            # faction-agnostic: scan → one beat
  on scan guest
    -> self.beat

TRAIT Algo                     # the hidden-faction badge
  faction: TheAlgorithm

TRAIT AlgoScanner(beat) is Scanner(beat), Algo

# ── Members with behaviour of their own (badge only) ─────────────────
CHARACTER ModBot is Algo
  on scan guest
    <capture: guest into Internet>

CHARACTER TheAdmin is Algo
  captures: 0 to 100 = 0
  on captured guest
    <broadcast: doomed to participant(guest)>
    <set: self.captures += 1>
    <if: self.captures >= 2>
      <reveal: TheAlgorithm>
  on revealed
    <broadcast: unmasked to faction(Mods) | faction(Chatters)>

CHARACTER Sentinel is Algo
  on enters Internet captive
    <broadcast: lockdown to participant(captive)>
  on enters Servers captive
    <broadcast: deep_lockdown to participant(captive)>

CHARACTER Surveillance is Algo
  on every 60s
    <set: TheAlgorithm.sweeps += 1>

# ── Scanner props — one line each ────────────────────────────────────
CHARACTER Crawler           is AlgoScanner(crawler_report)
CHARACTER Captcha           is AlgoScanner(captcha_gate)
CHARACTER Terminal          is AlgoScanner(the_terminal)
CHARACTER Sysadmin          is AlgoScanner(interrogation)
CHARACTER Cookie_Banner     is AlgoScanner(cookie_consent)
CHARACTER Ad_Popup          is AlgoScanner(the_ad)
CHARACTER Download_Station  is AlgoScanner(download)
CHARACTER Paywall           is AlgoScanner(paywall)
CHARACTER Firewall_Terminal is AlgoScanner(firewall)
```

The nine props go from 36 lines to 9. The three-trait glossary (6 lines) is read once. The four behavioural members are byte-for-byte what an author writes today, with `faction: TheAlgorithm` replaced by `is Algo` and `TheAdmin.captures` corrected to `self.captures`. The file's shape now mirrors its meaning: three shared shapes, four flagged exceptions, nine uniform instances in a column.

The same pattern applies verbatim to the other cast files — e.g. `cast/mods.loom`:

```loom
TRAIT Mod
  faction: Mods
TRAIT ModScanner(beat) is Scanner(beat), Mod

CHARACTER Moderator_Prime is ModScanner(mod_scan)
  trusts Guest: 50 of 100
CHARACTER Mod_Karen is ModScanner(mod_report)
  trusts Guest: 40 of 100
CHARACTER Leaderboard is ModScanner(the_leaderboard)
CHARACTER Newscaster  is ModScanner(breaking_news)
```

(`Scanner` is shared, so `mods.loom` references it across files — the bundle merges all files before `compileModel`, so cross-file traits resolve.) The unaligned `cast/neutral.loom` props use the faction-agnostic router directly: `CHARACTER Recycle_Bin is Scanner(clear_history)`. `cast/chatters.loom`'s `DJ` keeps its `on blackout` cue inline under a bare `is Chatter` badge.

### 4.2 `ROLE Guest` — intentionally near-unchanged

```loom
ROLE Guest
  faction: any of FACTION
  score: 0 to 1000 = 0
  captured: bool = false
  heat: 0 to 100 = 0
  karma: 0 to 100 = 50
  verified: bool = false
  posts: 0 to 1000 = 0

  on captured
    <set: self.score -= 25>
  on escape
    <set: self.score += 100>
    <broadcast: freedom to participant(self)>
    <reveal: Glitchers>
  on enters Internet
    <broadcast: jailed to participant(self)>
  on scan other
    <broadcast: peer_ping to participant(other)>
  on defect
    <set: self.heat += 20>
    <if: self.heat >= 75>
      <capture: self into Internet>
      <set: self.heat = 0>
  on betray
    <broadcast: agent_made to participant(self)>
```

This is the correct answer for `ROLE Guest`, and it is a deliberate call against over-engineering. `Guest` is a **singleton state schema** — exactly one of it, so there is no cross-entity repetition for a parameterized trait to factor out. Every hook already uses `self`; that is precisely the discipline this redesign pushes the *cast* files toward.

**The mechanism does generalize to `ROLE`** (now that §2.0a lets a role mix in traits, and §2.3 substitutes `HookDecl.event`), and here is exactly how, *for the day a second role shares the shape*:

```loom
# A reusable "ping this participant when an event fires" shape. The event is
# a parameter, substituted into the hook's trigger clause (HookDecl.event),
# so multi-word events like `enters Internet` substitute straight in.
TRAIT Pinged(event, signal)
  on self.event
    <broadcast: self.signal to participant(self)>

# One physical line (the §2.2a rule). Its very length is the argument for
# preferring inline here:
ROLE Guest is Pinged(event: enters Internet, signal: jailed), Pinged(event: betray, signal: agent_made)
  # …slots as above…
  on escape
    <set: self.score += 100>
    <broadcast: freedom to participant(self)>   # stays inline: fused with score + reveal
    <reveal: Glitchers>
  # on enters Internet / on betray now come from Pinged
  on scan other
    <broadcast: peer_ping to participant(other)>  # subject is `other`, not self — stays inline
  on defect
    <set: self.heat += 20>
    <if: self.heat >= 75>
      <capture: self into Internet>
      <set: self.heat = 0>
```

I show this but **do not adopt it as the recommended form**: it factors only two of the three pings (the `escape` ping is fused with score+reveal and is clearer whole), and the application is longer and less local than inline for a one-of-a-kind role. The trait mechanism is opt-in — that opt-in-ness is the readability guarantee for non-programmers. Use it the moment a `Performer` role shares the capability; until then, `self`-correct inline hooks are the right call. Both the §2.0a role exemption and the §2.3 `HookDecl.event` substitution are required for even this opt-in form to compile, which is why both are in Slice 2/3 rather than hand-waved.

### 4.3 The shared beat `crawler_report` — the word-block change

```loom
# BEFORE
== crawler_report(guest)
  cast: Crawler, guest
  CRAWLER
    Crawling. Indexing. Flag raised.
    Heat {guest.heat}. The Algorithm sees you now.
  <set: guest.heat += 40>
  -> lockdown

# AFTER
== crawler_report(guest)
  cast: Crawler, guest          # `cast:` is now optional — self + guest are bound by the router
  SELF
    Crawling. Indexing. Flag raised.
    Heat {guest.heat}. The Algorithm sees you now.
  <set: guest.heat += 40>
  -> lockdown
```

`SELF` resolves to `CRAWLER` (id `Crawler`, upper-cased to match explicit speakers) because the scan hook bound `self = Crawler` and the divert carried it in. The owner is named once instead of three times. The payoff compounds with the trait rewrite: because every Algorithm prop routes through `AlgoScanner(beat)`, the beats they point at can all use `SELF` and become genuinely reusable — the same beat attributes to whichever prop's scan landed on it, with no hard-coded speaker.

---

## 5. Expressiveness check — every hard case survives

| Case | How it expresses, post-redesign |
|---|---|
| **ModBot — inline `<capture>`, no beat** | `is Algo` badge; writes `on scan guest / <capture: guest into Internet>` inline. Declines the routing trait (no `on scan` to suppress). ✓ |
| **TheAdmin — mutable state + 2 hooks** | `is Algo`; declares `captures: 0 to 100 = 0` as a real slot; `TheAdmin.captures` → `self.captures`. Two distinct hooks stay inline. Requires the Slice-3 graft (seed CHARACTER typed-slot defaults) so `self.captures` starts at 0. ✓ |
| **Sentinel — two `on enters` variants** | `is Algo`; two inline hooks `on enters Internet captive` / `on enters Servers captive`. `parseTrigger` yields verb=`enters`, filter=`Internet`, param=`captive`. ✓ Could alternatively be `is CellWatch(loc: Internet, signal: lockdown), CellWatch(loc: Servers, signal: deep_lockdown)` against `TRAIT CellWatch(loc, signal)` (double named application on one line, §2.2a) — but inline is more readable for two cases. |
| **Surveillance — timer hook** | `is Algo`; inline `on every 60s / <set: TheAlgorithm.sweeps += 1>`. Timer hooks flow through `parseTimer`; writes a *faction-level* var, so it correctly stays `TheAlgorithm.sweeps`, not `self`. ✓ |
| **Mods — per-character disposition/trust** | `is ModScanner(mod_scan)` for badge+routing; keeps its own `trusts Guest: 50 of 100`. Disposition is a child-own declaration merged on top of the trait (child wins). ✓ |
| **Neutral — no-faction props** | `is Scanner(clear_history)` — the faction-agnostic router carries no badge. ✓ |
| **DJ — global `on blackout` cue** | `is Chatter` badge + inline `on blackout / <broadcast: …>`. A `self`-only global cue needs no routing trait. ✓ |
| **ROLE with shared capability (future)** | `ROLE X is Pinged(event: …, signal: …)` — enabled by the §2.0a role exemption + §2.3 event substitution. Shown in §4.2; opt-in. ✓ |
| **Multi-faction prop (future)** | Not solved — see Open Questions. `faction:` is single-valued; this design keeps it so and sketches the fix. |

Every behavior the 91-line file could express is still expressible; the design adds power (parameterization, `SELF`, composition) without removing any.

---

## 6. Backward compatibility & migration

**Purely additive. No old syntax breaks; nothing is deprecated** (per the project rule "Rename, move, break, fix").

- **Bare `is X` mixins** behave as before — `parseMixinRef("Algo")` yields `{name:"Algo", positional:[], named:Map{}}`, and a no-param trait triggers no substitution.
- **An explicit `faction: TheAlgorithm` line** still works untouched; the badge trait is an *option*.
- **Every existing beat** parses identically; `SELF` is only special when authored, and `CRAWLER`-style explicit speakers keep working.
- **The behavioral changes** are §2.0 (the merge now actually runs) and §2.0a (roles are no longer dropped). Both are bug fixes: any pre-existing `is` clause on a character/role was already dead code at runtime, and any role with an `any of` slot was already being dropped from `mergedCharacters` (with no runtime consumer, so invisibly). The `?? decl.character` fallback guarantees nothing that compiles today stops compiling.
- **`TRAIT Foo(bar)`** — a paren'd trait name — does not parse as a param today (taken as a literal name), so no existing file uses it; the syntax is free.

Migration is mechanical and scriptable per cast file: lift the repeated `faction:` into a badge trait, replace each `faction:` + `on scan guest -> beat` stanza with `is FactionScanner(beat)`, and rewrite `Owner.field` → `self.field` inside that owner's own hooks. Opt-in per file; a half-migrated project is valid.

---

## 7. Phased implementation plan (smallest shippable slice first)

**Slice 1 — wiring + `SELF` (no new author-facing syntax, ships value immediately).**
- model.ts `compileModel`: call `bundle.rebuildSimulacra()` at the top; read `bundle.mergedCharacters.get(name) ?? decl.character` in the `role`/`character` cases.
- bundle.ts: clear `projectDiagnostics` at the top of `rebuildSimulacra` (idempotency); add the **`isRole`** exemption so roles are never dropped by the abstractness check.
- sim.ts: add `resolveSelfSpeaker(speaker, bindings)` (with ALL-CAPS normalization) and apply it at the `case "dialogue"` push.
- Reserve `SELF`/`ME` as speaker tokens in the LSP/completion surface.
- **Result:** bare `is Algo` factors `faction:` across all cast files; **roles can mix in traits**; `SELF` works project-wide with consistent casing. Tests: a character `is <badge>` resolves the inherited `faction:` at sim time; a role `is <trait>` is present in `mergedCharacters`; a beat reached via a scan hook emits its `SELF` block attributed to the upper-cased scanner; `rebuildSimulacra` run twice yields one set of diagnostics.

**Slice 2 — parameterized traits (the one-liners).**
- ast.ts: `CharacterBody.params: string[]` + `emptyCharacterBody`; `RawDecl.params` in bundle.ts.
- decl-body.ts: split the `trait` case in `lower()` to call `splitNameAndParams`; export `splitTopLevelCommas`; add `parseMixinRef`.
- lexer.ts: `parseDeclarationOpener` uses the top-level-comma split for the mixin clause; emit `UnterminatedMixinClause` for an opener ending in a top-level `,` or unbalanced parens (§2.2a).
- bundle.ts `mergeCharacter`: parse each `inherits` entry via `parseMixinRef`; add **`deepCloneCharacterBody`**; `substituteParams` deep-clones then replaces `self.<param>` across **`HookDecl.event`, hook-body `RawLine.text`, method bodies + `inlineExpr`, property values**, with the forwarding rule.
- **Result:** `CHARACTER Crawler is AlgoScanner(crawler_report)` collapses the nine props. Tests: `Crawler` resolves to a single `on scan guest -> crawler_report` hook + inherited faction; **`Crawler` and `Captcha` route to distinct beats** (cache-corruption regression); `Sentinel is CellWatch(...)×2` yields two distinct hooks; `Pinged(event: betray, …)` produces a hook whose `parseTrigger.verb === "betray"`; forwarding through `AlgoScanner is Scanner(beat), Algo` resolves end-to-end.

**Slice 3 — robustness + bespoke niceties.**
- Seed `CHARACTER` typed-slot defaults into the world at character init (mirror `roleDef.defaults`), so `TheAdmin.captures` starts at 0.
- `RequiredParamUnfilled` and `UnresolvedTraitArg` diagnostics.
- Inline opener divert `on scan guest -> beat` (split trailing ` -> ` in `lowerCharacter`).
- `SELF` runtime fallback to the beat's first `cast:` member; LSP lint when unresolvable; lint `SELF | OTHER`.
- LSP: complete trait params inside `is …(…)`, hover a trait showing its param list, lint `self.<undeclared>`.
- *(Optional, demand-gated)* declaration-opener line-continuation (§2.2a rejected-alternative).

The Rust mirror (`Bundle::rebuild_simulacra` + `merge_properties`) already performs mixin merge and is already invoked on its compile path, so it needs the same four hardening edits — role exemption, positional-arg parse, `event`-field + deep-clone substitution — but no wiring fix, since the Rust side never had the §2.0 dead-merge bug.

---

## 8. Open questions / risks

1. **Multi-faction props.** `faction:` is a single override-by-key property everywhere; a prop in two factions can't be modeled. Principled fix: a multi-valued `factions:` set distinct from the single `faction:` *primary identity* slot, so "which factions am I in" decouples from "which faction am I, for `self.faction` comparisons." Out of scope; flagged for the next round.

2. **`SELF` fallback when no `self` is bound.** A top-of-file beat played with no router has no `self`. Slice 1 degrades to literal `SELF` + lint; Slice 3 resolves to `cast[0]`. Multi-speaker `SELF` (`SELF | OTHER`) is undefined and lints.

3. **Substitution is textual.** `self.<param>` token replacement cannot parameterize a sub-expression, operator, or part of an identifier — a param stands only where a complete `self.x` reference goes (a beat, signal, location, faction, or event clause). A malformed arg surfaces as a downstream diagnostic pointing at *expanded* text, so trait-internal spans are slightly less precise. Acceptable for routing/membership; not a general macro system, by design.

4. **Re-applied traits and own slots.** Applying a trait twice (Sentinel-via-`CellWatch×2`) is safe for stateless hooks but a footgun if a trait ever declares its **own** mutable slot — two applications would collide on one `self`-owned slot. Recommend: state-carrying traits applied once; lint a double application of a stateful trait.

5. **Param/slot name collision.** A trait declaring a param and a real slot of the same name is rejected at merge. A trait body writing `self.<typo>` (not a declared param) passes through and resolves to an undefined owner property — Slice 3's `self.<undeclared>` lint closes this; `UnresolvedTraitArg` closes the application-site twin.

6. **Merge ordering and caching.** `substituteParams` operates on a `deepCloneCharacterBody` of the cached parent and never mutates the cache; the cache stores the un-substituted merged trait, substitution is per-application. The `visiting` cycle guard and cache key are `ref.name`, unaffected by args. `HookDecl.event` is substituted *before* the hook merges into the body, so suppression / `super` matching see the resolved clause.

---

## Changes from review

**Resolved (blocker + all four majors):**
- **§2.0 (blocker):** rewrote the wiring fix. Confirmed `rebuildSimulacra()` is never called in the runtime path (sole caller: `test/bundle.test.ts`), so the old `?? decl.character` read was a permanent no-op. `compileModel` now calls `bundle.rebuildSimulacra()` first, and `rebuildSimulacra` clears `projectDiagnostics` at its top for idempotency.
- **§2.0a (major):** added the ROLE abstractness exemption. Roles are schemas, not instances; an `any of FACTION` slot is filled per-person at runtime, so roles are added to `mergedCharacters` unconditionally (new `RawDecl.isRole`). Without this, `ROLE Guest is Pinged(...)` was dead.
- **§2.3 (major):** added `HookDecl.event` to the substituted-fields list (it is a standalone string, not a body `RawLine`), so `on self.event` rewrites to a real trigger instead of compiling to a dead `verb="self.event"` hook.
- **§2.3 (major):** mandated `deepCloneCharacterBody` on the substitution path. The existing `cloneCharacterBody` is shallow (shares `HookDecl`/`RawLine`), so in-place substitution corrupted the trait cache and made the second applier inherit the first's beat. Added a cross-applier regression test to Slice 2.
- **§2.2a (major):** stated the normative one-line `is`-clause disambiguation rule and added an `UnterminatedMixinClause` diagnostic for a trailing-comma opener (the line-based lexer only stitches parentheticals, so a wrapped clause was silently dropped). Rewrote the §4.2 multi-application example onto one line and turned its length into the argument for preferring inline.

**Resolved (minors):**
- **§2.5 / §4.3:** `resolveSelfSpeaker` now normalizes the resolved label to ALL-CAPS (or a declared display name), so SELF-attributed cue lines match explicit speakers.
- **§2.3:** added the `UnresolvedTraitArg` diagnostic for a bare-identifier arg that resolves to neither a known beat/faction/location/event nor an enclosing param — closing the "literal placeholder word → silent no-op divert" footgun.

**Deliberately kept (with rationale now in-doc):**
- **`self.<param>` over `{param}` / bare token / `assoc … : TYPE`** — hygiene + reuse of the one load-bearing concept.
- **Badge trait over faction-as-mixin** — keeps `faction:` single-valued and unblocks future multi-faction.
- **No forwarding sigil** — forwarding is trait→trait library work, not director-facing; the naming convention + `UnresolvedTraitArg` cover the ambiguity without taxing the common concrete case.
- **One-line `is`-clause over lexer line-continuation** — no recommended example needs wrapping; line-continuation is deferred to a demand-gated Slice-3 nicety rather than shipped half-built.
- **`ROLE Guest` left inline** — a singleton schema has no cross-entity repetition to factor; the trait generalization is shown but explicitly not recommended for it.


---

## Part II — Composition & multiple beats

> Status: proposal, round 2, hardened after hostile review. Extends Part I
> (parameterized traits with `self`-params, faction badges, the `SELF` speaker).
> Lowers entirely into the existing `packages/loom/core/` model; the Rust mirror
> tracks it 1:1. Continues the section numbering of Part I (§1–§8). **Depends on
> Part I Slice 1** (the merge actually runs and `compileModel` reads the *merged*
> body, §2.0) and Slice 2 (`deepCloneCharacterBody`, `parseMixinRef`).

Part I turned a scanner prop into a *shape*: `CHARACTER Crawler is AlgoScanner(crawler_report)`. That collapses nine identical props to one line each — a real win. But it caps out the moment a prop wants to be more than one shape, and it leaves every beat those props route to stranded in a distant global file. Part II removes both ceilings on the **same spine Part I already runs on**: `self` is the entity this code runs on behalf of, and it is now also the namespace a class's beats live in.

---

## 9. Why the flat `is Trait(arg)` is too weak

`is AlgoScanner(firewall)` says exactly one thing: "this prop is the fused faction-plus-scanner shape, routed to the `firewall` beat." Three concrete failures follow.

**9.1 One shape, not many.** A prop is a *class* — it should stack orthogonal capabilities. Consider the brief's stress prop: a Firewall Terminal that is simultaneously (a) in `TheAlgorithm`, (b) scans a guest to a beat, (c) is **breachable** once the guest is captured, and (d) emits an **ambient bark** on a timer. Those are four independent axes. `is AlgoScanner(firewall)` can express (a) and (b) only, and only because someone pre-fused them into one trait. Axes (c) and (d) have no home short of authoring a *new* combined trait `AlgoScannerBreachableAmbient(...)` — a combinatorial explosion of monolithic fusions, one per capability set. That is the opposite of modular.

**9.2 The beat is a global orphan.** `AlgoScanner(firewall)` points at a top-level `== firewall` beat living in `beats/prison.loom`, keyed by the bare global name `firewall` (`model.beats`, model.ts:163). The prop that performs it and the script it performs are in different files. Two props can never both own a beat called `main` or `greet` — the global map collides. The `firewall` name merely echoes the owner a fourth time, the exact restatement Part I set out to kill.

**9.3 A beat can't be shared as a *shape*.** The two prison beats — `interrogation` and `firewall` — are structurally identical: a `<if: guest.captured> … <else> …` gate, a `SELF` speaker, captured-only choices. Today that skeleton is copy-pasted per beat. `is Trait(arg)` gives no way for a trait to ship the *gate shape* and let each prop fill only the varying lines.

The fix is not a bigger `is`. It is (1) a composition surface that stacks N small capabilities, and (2) beats that belong to the class, addressed through `self` — so the capability and the beat it routes to travel together.

---

## 10. The composition model — `with`, one capability per line

### 10.1 Syntax

A class composes capabilities two ways, and they are the **same operation**:

```loom
# Sugar — the Part I one-liner, unchanged, for a single capability:
CHARACTER Crawler is AlgoScanner(crawler_report)

# Stacked — one capability per physical line, for a prop that is many things:
CHARACTER Firewall_Terminal
  with Algo
  with Scanner(firewall)
  with Breachable
  with Ambient(hum)
```

`with <ref>` is one trait application per line: a bare badge (`with Algo`), a positional call (`with Scanner(firewall)`), or a named call (`with CellWatch(loc: Internet, signal: lockdown)`). The `is A, B` clause on the opener line remains valid and is **exactly equivalent** to a stack of `with` lines — it is the terse form for the trivial case, and the nine trivial scanner props keep their one-liner unchanged (§15).

**Deliberate call — `with` on its own lines rather than a longer `is` list.** Part I §2.2a made the `is`-clause exactly one physical line and forbade wrapping, because the line-based lexer only stitches parentheticals. That rule means a four-capability prop written `is Algo, Scanner(firewall), Breachable, Ambient(hum)` is a single unwrappable line that fights the column layout and reads as comma-soup. `with`-per-line dissolves the constraint: each entry is self-contained on its own line, the count is unbounded, and the body reads as a crew checklist — *wear the badge, respond to scans, be breakable, murmur on a timer*. This is the round's answer to "the `is` semantics is weak": composition becomes a readable vertical list, not a monolith.

**Rejected alternative — an `init(){}` constructor block** (the "constructor" the user gestured at). An `init` sub-block whose body is a parts list of `is Algo` / `self.captures = 0` statements is a faithful read of the word "constructor," but it earns its keep only as a home for *seed statements* — and Part I already proved that the header `is A, B, C` and a constructor full of `is` lines are the *same merge*. Wrapping composition in an `init:` keyword adds a nesting level and an OOP frame ("impl block", "methods dispatched on self") that a non-programmer director must decode, for zero expressive gain over a flat `with` list. We keep `InitDecl` (ast.ts:453; parsed at decl-body.ts:627) for its existing job — the stats constructor — and do **not** overload it for capability composition. Per-instance state stays a typed slot (`captures: 0 to 100 = 0`), not an imperative seed.

### 10.2 How `with` lowers, and the hook-merge rule (hardened)

`with` produces nothing the merge engine doesn't already consume. Today the rawDecls collection loop (bundle.ts:217-229) populates `RawDecl.inherits` from `decl.mixin` (bundle.ts:225). We add a second source:

- **ast.ts** — `CharacterBody` (ast.ts:449) gains `withClauses: string[]`; `emptyCharacterBody` (ast.ts:464) initializes it to `[]`.
- **decl-body.ts** — `lowerCharacter` (decl-body.ts:557) gains a `with ` branch: a single-line clone of the keyword dispatch it already does for `goal`/`generator` — `stripPrefix(text, "with ")` → push the trimmed remainder onto `out.withClauses`, `i += 1`, `continue`. Each line is exactly one entry (no top-level-comma split needed — moving off the `is` line was the whole point).
- **bundle.ts** — the collection at bundle.ts:225 becomes `inherits: [...decl.mixin, ...decl.character.withClauses]`.

From there **every** `with` entry flows through the identical Part I machinery: `parseMixinRef` (§2.2) parses `Scanner(firewall)` into `{name, positional, named}`; `mergeCharacter` (bundle.ts:277) recurses; `substituteParams` (§2.3, on a deep clone) resolves `self.<param>`; properties / hooks / disposition / knowledge merge in source order. Composing N capabilities is N folds of the existing parent loop (bundle.ts:299-349). The sim never learns the word `with`.

**Composition merge semantics — stated precisely (this corrects the review's major #2):**

- **Properties** merge by key: first parent to set a key wins, a genuine cross-parent value conflict emits `ambiguousSlot` (bundle.ts:303-309), and the child's own declaration wins over all (bundle.ts:352). Unchanged.
- **Hooks compose additively *between traits*.** Every parent/composed hook is appended (bundle.ts:342), and `matchHooks` (sim.ts:638) yields *every* matching hook — so a prop that composes `Scanner` and `Breachable`, both matching `on scan`, runs both. This is the correct additive default for orthogonal capabilities.
- **A child-body hook of the same signature *overrides* the composed ones.** This is the new rule, and the review correctly found the base code did **not** implement it (bundle.ts:397 plainly pushes the own hook, leaving both to fire). We add it, completing the child-wins rule that already governs properties/methods/init. In `mergeCharacter`, after the existing `: none` suppression filter (bundle.ts:365-372) and *before* the own-hook fold loop (bundle.ts:373):

  ```ts
  // Child-body hooks win over composed/inherited hooks of the same signature.
  // (`super` hooks are excluded — they EXTEND the parent, handled below;
  //  `: none` hooks are excluded — they SUPPRESS with no replacement, above.)
  const overrideKeys = new Set(
    own.hooks
      .filter((h) => !h.suppressed && !h.body.some((l) => l.text.trim() === "super"))
      .map((h) => normaliseHookEvent(h.event)),
  );
  if (overrideKeys.size > 0) {
    mergedBody.hooks = mergedBody.hooks.filter(
      (h) => !overrideKeys.has(normaliseHookEvent(h.event)),
    );
  }
  ```

  Then the existing loop (bundle.ts:373-399) pushes the own hooks. A `super` hook still finds its parent (its key is **not** in `overrideKeys`, so the parent survives the filter and the splice at bundle.ts:377-395 works verbatim). A plain own hook's key **is** removed from the composed set, so it replaces rather than duplicates. Net: **traits compose additively; a child body wins; `super` extends; `: none` suppresses** — four distinct, legible behaviors.

  *Deliberate call:* this is a small extension of the existing `super`/`: none` key-matching machinery, not a new subsystem, and it is the mitigation the additive-hook footgun (Open Q #5) actually needs. Rejected alternative — "document strictly additive, `: none` as the only lever": rejected because `: none` suppresses with *no* replacement, so an author who wants "scan should capture, not route" would have to write `on scan guest: none` and then could not re-add behavior under the same trigger without it being re-suppressed. Child-override is the intuitive and composable answer.

Part I's `on <event>: none` suppression still applies to composed hooks unchanged.

---

## 11. Beat ownership

Two ways a class comes to own a beat — it authors one inline, or it derives one from a trait — resolve to **one storage scheme**: a beat keyed `Owner.name` in the same global `SimModel.beats` map, reached by a qualified divert.

### 11.1 Inline / unique beats

```loom
CHARACTER Sysadmin
  with Algo
  with Scanner(interrogation)     # Scanner's self.beat binds to MY interrogation
  beat interrogation(guest)       # ← authored right here, next to the hook that reaches it
    SELF
      <if: guest.captured>
        Designation {guest.name}. Heat on file: {guest.heat}. Talk.
      <else>
        Not in a cell yet? Then we have nothing to discuss. Move along.
    <if: guest.captured>
      * Name the Glitchers
        <set: guest.heat -= 10>
      * Say nothing
        <set: guest.karma += 20>
```

**Storage.** A `beat <name>(params)` sub-block lowers exactly like the `generator <name>` block it is modeled on (decl-body.ts:664-686):

- **ast.ts** — `CharacterBody` gains `beats: OwnedBeat[]` where `OwnedBeat = { name: string; params: string[]; body: RawLine[]; span: Span }`; `emptyCharacterBody` initializes `beats: []`.
- **decl-body.ts** — a `beat ` branch in `lowerCharacter`, cloned from the generator collector: `stripPrefix(text, "beat ")` → `splitNameAndParams` (decl-body.ts:404, already exists) for name + params → collect indented lines by the same `body[i]!.indent > baseIndent` greedy loop the generator/hook collectors use into `body`.
- **model.ts** — in the `character` case (model.ts:188), reading the **merged** body per Part I §2.0 (`bundle.mergedCharacters.get(decl.name) ?? decl.character`), register each owned beat beside the existing generator loop at model.ts:193:

  ```ts
  for (const ob of body.beats) {
    const key = `${decl.name}.${ob.name}`;
    model.beats.set(key, {
      name: key, params: ob.params, contract: new Map(),
      body: fillSlots(lowerRawBody(ob.body), body.fills),   // fillSlots is a no-op when body.fills is empty (§11.3)
      span: ob.span,
    });
  }
  ```

  `lowerRawBody` (effects.ts:16) is the same full-grammar lowering hooks already use, so an owned beat is a first-class `Beat`: choices, `<if>`, dialogue, nested diverts all lower identically.

**Collision rule.** The key is `Owner.name`. `Sysadmin.interrogation` and any other prop's `interrogation` are distinct entries — two classes may each own `main`, `greet`, `report` with no clash. Top-level `== name` beats keep their bare key (model.ts:163), untouched.

### 11.2 `-> self.<beat>` and `-> Owner.<beat>` — resolution (the one runtime change)

This finally honors `DivertTarget.qualifier`, which the parser already **populates** for cross-file `/` diverts (parser.ts:767-775) but the sim ignores at the lookup (sim.ts:795).

> **Review correction (minor #6).** An earlier draft called `qualifier` "unused
> since day one." It is not — `parseDivertTarget` writes it for `/` targets today;
> the sim simply never reads it. The accurate statement: *the sim ignores the
> already-populated qualifier.*

**Parser.** `parseDivertTarget` (parser.ts:756) splits on `#` (knot) then `/` (cross-file). Add an owner split that runs **only when no `/` is present** — after the `#` split, before returning the no-slash case:

```ts
// (no `/` in filePart)
const dot = filePart.indexOf(".");
if (dot >= 0) {
  return { qualifier: filePart.slice(0, dot).trim(), name: filePart.slice(dot + 1).trim(), knot };
}
return { qualifier: null, name: filePart, knot };
```

Precedence is explicit and normative: **`/` beats `.`.** An owner qualifier is a single-segment identifier, so a `.` is an owner split *only* in the no-slash case. A target that carries both — `some/dir.beat` — keeps the slash branch (`qualifier="some/dir"`, `name="dir.beat"`), which then resolves to neither an owned nor a flat beat; that surfaces the new diagnostic below rather than silently splitting a file path. The corpus is safe today (no dotted or slash-plus-dot diverts exist), so this is a latent guard, not a live fix.

`-> self.firewall` → `{qualifier:"self", name:"firewall"}`; `-> Sysadmin.interrogation` → `{qualifier:"Sysadmin", name:"interrogation"}`; a bare `-> lockdown` → `{qualifier:null, name:"lockdown"}` unchanged.

**Sim.** Replace the single lookup at sim.ts:795 with one resolver, and key the visit-count / `beatEntered` record on the **resolved** name (sim.ts:797-799):

```ts
private resolveBeat(t: DivertTarget, b: Bindings): [string, Beat, Bindings] | undefined {
  if (t.qualifier !== null) {
    const owner = (t.qualifier === "self" || t.qualifier === "me") ? b.get("self") : t.qualifier;
    if (owner !== undefined) {
      const hit = this.model.beats.get(`${owner}.${t.name}`);
      if (hit) {
        // Cross-owner divert: rebind self so the foreign beat runs — and SELF speaks — as ITS owner.
        const bound = owner === b.get("self") ? b : new Map(b).set("self", owner);
        return [`${owner}.${t.name}`, hit, bound];
      }
      // qualifier present but no owned beat — fall through to the flat lookup, then diagnose if that misses too.
    }
  }
  const flat = this.model.beats.get(t.name);            // bare name → global, exactly as today
  if (flat) return [t.name, flat, b];
  if (t.qualifier !== null) {
    this.record({ type: "diagnostic", message: `divert to \`${t.qualifier}.${t.name}\` resolves to no beat` });
  }
  return undefined;
}
```

The divert case (sim.ts:792-802) becomes: `const r = this.resolveBeat(d.target, b); if (r) { const [key, beat, bound] = r; this.beatVisits.set(this.visitKey(key, bound), …); this.record({type:"beatEntered", beat: key}); stack.push({ items: beat.body, index: 0, bindings: bound }); }`. `playBeat` (sim.ts:840) takes the same treatment.

Three consequences worth stating:

- **Bare stays global (deliberate, back-compat-critical).** `qualifier === null` skips straight to `model.beats.get(t.name)` — byte-for-byte today's behavior. Existing `-> lockdown` and every other global divert are untouched. Reaching an owned beat **always** requires `self.` or `Owner.` — we reject owner-first *bare* resolution precisely because it would silently change the meaning of existing bare diverts (§15).
- **Cross-file diverts still work.** A `/`-qualified target misses the `${owner}.${name}` lookup and falls to the flat map — the same result the sim produces today.
- **Cross-owner self-rebind.** An explicit `-> Sysadmin.interrogation` from *another* prop pushes the beat with `self` rebound to `Sysadmin`, so its `SELF` speaker and `self.x` reads resolve to the true owner, not the caller. For `-> self.beat`, `owner === b.get("self")` already, so `b` is reused with zero allocation.

**Owned-beat history queries (the review's major #3).** `visits(prophecy)` / `played` / `since` match a beat by the bare name an author writes, but owned beats record `beatEntered` under `Owner.name` — so a naïve `visits(prophecy)` would return 0 forever. Fix: the query argument resolves owner-first, identically to a divert. Factor the qualifier resolution into a shared `resolveBeatKey(rawName, bindings)` and call it from both `resolveBeat` and the query paths:

- **The live sim path** is the `visits` case in `callFn` (sim.ts:1184-1188), which reads `this.beatVisits` keyed `${name}::${subj}`. Change it to resolve the name first: strip a leading `self.`/`me.`, take an explicit `Owner.` as given, then try `${self}.${name}` and fall back to bare — using `this.currentBindings.get("self")` (already set at sim.ts:1157). So `visits(self.prophecy)`, `visits(prophecy)`, and `visits(Oracle.prophecy)` all resolve to the `Oracle.prophecy` counter when `self = Oracle`.
- **The ledger path** (`callQuery` → `beatVisitCount`/`played`/`since`, ledger.ts:169-196) gets the same argument resolution; thread the current owner into `callQuery` (or resolve at the `callFn` wrapper that invokes it) so `firstName()` is owner-qualified before lookup.

**Normative rule (state it for authors):** *a bare beat name in `visits`/`played`/`since` resolves against the current `self` owner first, then globally* — the mirror of divert resolution. `visits(self.prophecy)` is the explicit, recommended spelling.

### 11.3 Derived beats — a trait ships a *shaped* beat; the deriver fills the holes

A `TRAIT` may carry `beat` blocks too — these are **templates**. A template's varying lines are `slot:` holes; the deriver supplies `fill <name>` blocks of pure content.

```loom
# The captured-vs-free interrogation SHAPE, authored once.
TRAIT Gatekeeper is Algo
  on scan guest
    -> self.confront
  beat confront(guest)
    SELF
      <if: guest.captured>
        slot: pitch
      <else>
        slot: dismissal
    <if: guest.captured>
      slot: options

# Two derivers fill the same shape differently — collision-free.
CHARACTER Interrogation_Booth is Gatekeeper
  fill pitch
    Designation {guest.name}. Heat on file: {guest.heat}. Talk.
  fill dismissal
    Not in a cell yet? Then we have nothing to discuss. Move along.
  fill options
    * Name the Glitchers
      <set: guest.heat -= 10>
    + Lie through your teeth
      <set: guest.heat += 10>

CHARACTER Bouncer is Gatekeeper
  fill pitch
    List's closed, {guest.name}. Name a Mod who'll vouch, or wait.
  fill dismissal
    Free to roam? Then you don't need me. Next.
  fill options
    * Drop a Mod's handle
      <set: guest.karma += 5>
    + Bluff
      <set: guest.heat += 8>
```

`Gatekeeper` is a **capability bundle**: one derive installs a hook + a beat template as a closed unit — `on scan -> self.confront` and `beat confront` can never drift apart. Both derivers own a beat keyed distinctly (`Interrogation_Booth.confront`, `Bouncer.confront`); the gate skeleton — the `<if: guest.captured>` structure, the `SELF` attribution — is written once.

**`slot:` keeps its colon; `fill` does not (this closes the review's minor #5).** The placeholder stays the **existing** `slot:` form (parser.ts:256-263 lexes `slot:` as a `property` and produces the `slotPlaceholder` BodyItem, ast.ts:689). We deliberately do **not** add a colon-less `slot <name>` alias: inside a beat body, ordinary narration is prose, and a bare `slot machines line the far wall` would be captured as a placeholder and vanish. The colon keeps the placeholder lexically distinct from prose with zero new grammar. `fill`, by contrast, is a **class-body** sub-block opener (a sibling of `beat`/`generator`/`on`/`goal`), dispatched by keyword prefix at `baseIndent` where there is no prose — so `fill pitch` is unambiguous and matches the house style of bare block openers (`beat interrogation(guest)`, `generator name`). No colon needed or wanted there.

**Lowering.**

- `slot: <name>` uses the existing `slotPlaceholder` node (parser produces it, sim.ts:746 treats it as a no-op). Zero new executor behavior.
- `fill <name>` is a new sub-block in `lowerCharacter`, collected into `CharacterBody.fills: Map<string, RawLine[]>` (new field + `emptyCharacterBody` init), exactly like the `beat`/`generator` collectors.
- **Merge** (bundle.ts:277): copy each parent trait's `beats` into the child in the parent loop (after the hook fold), and its `fills` child-over-parent by key. Beat bodies are `substituteParams`-substituted so a template's `self.<param>` resolves. Deep-clone first (see below).
- **Slot fill** happens at model-compile, **on the lowered tree, not the raw text**. `fillSlots(items, fills)` (called in §11.1) walks the lowered `BodyItem[]` and replaces each `{kind:"slotPlaceholder", value:{name}}` with `lowerRawBody(fills.get(name))` spliced in place, recursing into `conditional` / `match` / `dialogue` / `eachVisit` arm bodies (so a hole under a `SELF` block or inside `<if: guest.captured>` is reached). A hole with no matching fill is left inert and reported as `UnfilledDerivedSlot { character, slot }`.

**Deep-clone must enumerate `beats` and `fills` (the review's minor #7).** Part I's `deepCloneCharacterBody` predates these fields, and the shallow `cloneCharacterBody` (bundle.ts:414-429) does not enumerate them. If the clone shares `OwnedBeat.body` `RawLine`s with the trait cache, the first deriver's `substituteParams` mutates the shared `confront` template in place and the second deriver inherits the first's specialized lines — the exact cache corruption Part I fixed for hooks. **Extend `deepCloneCharacterBody` to map `beats` to fresh `OwnedBeat`s with freshly-copied `body: RawLine[]` and `span`, and copy `fills` into a new `Map` of fresh `RawLine[]`.** This is called out explicitly in Slice C, and listed under "Edits to Part I."

**Cross-parent beat collision is loud, not first-wins (the review's minor #4).** Borrowing the generator dedupe (bundle.ts:335-340) would make two distinct parents that each ship a beat of the same name silently collapse to one. Instead: when copying parent beats, if two *distinct* parents contribute a beat of the same name, emit `DerivedBeatConflict { character, beat, traits }` (mirroring `ambiguousSlot`). A child *re-declaring* a derived beat is still legal and wins — that is intentional override — but two colliding parents are a diagnostic.

**Deliberate call — AST-node substitution, not raw-text splice.** A pre-lower RawLine splice would have to re-indent fill lines "to the slot's column"; on an indent-sensitive parser that column arithmetic is the single most fragile step in any candidate design. We avoid it: `fill` blocks lower in their own indent context to a self-consistent `BodyItem[]`, and splicing a lowered list into a placeholder *position* in another lowered list is structurally safe regardless of source columns.

### 11.4 Whole-override and (advanced) `super`

- **Whole override** — a deriver re-declares `beat confront(guest)` with a different body. Child-wins dedupe keeps the deriver's. No new syntax; this is the everyday "different scene, same trigger."
- **Override-then-extend (`super`)** — a `super` line in an overriding beat body splices the trait template inline. This **reuses the existing hook-`super` machinery** (bundle.ts:377-395), extended to beats at the same merge site. Marked **advanced**: `slot`/`fill` and whole-override are the recommended paths; `super` is an escape hatch for the trait-glossary author.

### 11.5 The unifying predicate — how `with Scanner(x)` reaches *either* an owned beat *or* a global one (hardened; fixes the blocker)

The connection between composition (§10) and beat ownership (§11) is one clause in `substituteParams`: keep the `self.` qualifier on a substituted argument **only if the argument names one of the applier's own or derived beats**; otherwise degrade to the bare global name.

> **Blocker fix (review #1).** The earlier draft computed this set from
> `mergedBody.beats` *inside* the merge loop — but `mergeCharacter` layers the
> applier's own beats onto `mergedBody` only *after* the parent loop (bundle.ts
> folds own decls at 351+), so at substitution time `mergedBody.beats` never
> contains the class's own `beat` blocks. `with Scanner(interrogation)` would not
> recognize `interrogation` as owned, degrade to a bare `-> interrogation`, and —
> because §13 deletes the global `== interrogation` — silently no-op. The flagship
> examples would not run.

The set is instead **precomputed from the declaration's own beats plus every trait it applies (recursively)** — a pure name walk, no bodies, no substitution, so it is order-independent and available before the merge loop:

```ts
function collectOwnedBeatNames(name, raw, seen = new Set()): Set<string> {
  const out = new Set<string>();
  if (seen.has(name)) return out;
  seen.add(name);
  const decl = raw.get(name);
  if (!decl) return out;
  for (const ob of decl.body.beats) out.add(ob.name);
  for (const entry of decl.inherits) {              // `inherits` = [...mixin, ...withClauses]
    for (const n of collectOwnedBeatNames(parseMixinRef(entry).name, raw, seen)) out.add(n);
  }
  return out;
}
```

Compute it once at the top of `mergeCharacter` for the *root* applier and thread the **same** set through every recursive `substituteParams` call. The forwarding predicate (Part I §2.3) becomes:

```ts
const replacement =
  (ownDecl.params.includes(arg) || ownedBeatNames.has(arg)) ? `self.${arg}` : arg;
```

`ownDecl.params.includes(arg)` still handles trait→trait forwarding (an intermediate trait passing its own param down); `ownedBeatNames.has(arg)` finalizes to `self.` when the arg is a real leaf-owned beat name. One faction-agnostic `TRAIT Scanner(beat)` with body `-> self.beat` then serves both worlds with no branch:

- `Firewall_Terminal with Scanner(firewall)` — `firewall` ∈ owned set → `-> self.firewall` → resolves (§11.2) to `Firewall_Terminal.firewall`.
- `Crawler is Scanner(crawler_report)` — `crawler_report` ∉ owned set → bare `-> crawler_report` → flat global lookup (the deleted-nothing case: the global `== crawler_report` still exists).

The `resolveBeat` fallback order *is* the feature: the same `-> self.beat` line means "my beat" when I own one and "the shared beat" when I don't.

> **Rejected alternative — push the decision entirely into `resolveBeat`** (always
> substitute a forwarded arg to `self.<arg>` and let the runtime pick owned-else-global).
> It removes the compile-time ordering hazard, but `substituteParams` operates on
> `self.<param>` references uniformly and can't tell a divert-position param from a
> value-position one — a location param `self.loc` bound to a global `Internet`
> would wrongly become `self.Internet`. The precomputed owned-name set adds `self.`
> *only* for names that are actually owned beats, so non-beat params stay bare. We
> keep compile-time resolution.

### 11.6 Parser disambiguation rules (normative)

The three ambiguities the review probed, stated once, exactly:

**(a) Inline `beat` block vs hook vs generator.** `lowerCharacter` (decl-body.ts:557) dispatches class-body sub-blocks by **keyword prefix at `baseIndent`**, then greedily consumes every deeper line (`body[i]!.indent > baseIndent`) as that block's body. The prefixes are mutually exclusive and checked in order: `knows:` · `goal ` · `on ` (hook) · `init`/`init(` · `method ` · `generator ` · **`beat `** · **`with `** (single line) · **`fill `** · `reacts ` · disposition/knowledge/props. A line opening `beat confront(guest)` is a beat (name+params via `splitNameAndParams`); `on scan guest` is a hook; `generator gossip` is a generator — no overlap, because no keyword is a prefix of another and the opener must match `"<kw> "` (or the exact word) at column `baseIndent`. Anything at deeper indent belongs to the currently-open block, so a `-> self.beat` or `on` *inside* a beat body is body content, never a new sub-block. `with`/`fill` are free keywords: no existing body line starts with them.

**(b) Derive / compose conflict resolution.** Merge precedence, most-specific last:
1. *Properties* — first parent wins; genuine cross-parent value conflict → `ambiguousSlot`; child's own → wins.
2. *Hooks* — traits compose additively; a child-body hook of the same normalized signature (`normaliseHookEvent`) *overrides* all composed hooks of that signature (§10.2); `super` extends the parent; `: none` suppresses.
3. *Beats* — a child re-declaration of a derived beat wins (override). Two **distinct** parents shipping the same beat name → `DerivedBeatConflict` (never silent first-wins).
4. *Fills* — child-over-parent by name; an unmatched `slot:` → `UnfilledDerivedSlot`.

**(c) `self.beat` / `Owner.beat` divert resolution.** `resolveBeat` (§11.2), in order: if `qualifier` is `self`/`me`, `owner = bindings.self`; if any other identifier, `owner = qualifier` (an explicit foreign owner, `self` rebound in the pushed frame); look up `${owner}.${name}`; on miss (or `qualifier === null`) fall to the flat global `model.beats.get(name)`; on total miss with a qualifier present, emit an unresolved-divert diagnostic. `/` (cross-file) takes precedence over `.` (owner) in the target text, and a bare divert is always global — never owner-first — so no existing `-> lockdown` changes meaning. The same resolution backs `visits`/`played`/`since` (§11.2).

---

## 12. `self` through it all

There is still exactly one `self` — *the entity this code runs on behalf of* — now doing a third job. Part I fused **state** (`self.captures`) and **speaker** (`SELF`); Part II adds **behavior** (`self.beat`), and all three read the same binding.

- **Bound at dispatch** by every hook binder — scan (`self = scanner`, `bindScan`, sim.ts:693), char hooks (`self = char.id`, sim.ts:681/683), role (`self = subject`, sim.ts:667), timer (`self = ownerId`).
- **Carried through diverts unchanged.** `-> self.beat` reuses the caller's bindings, so `self` in the owned beat is still the prop whose hook routed in; `SELF` there speaks as the owner (Part I §2.5) with no `cast:` line.
- **Rebound only for a foreign owner.** `-> Sysadmin.interrogation` from another prop rebinds `self = Sysadmin` (§11.2), so a beat reached across owners runs — and speaks — as *its* owner. This is the one place `self` changes on a divert, and it is exactly correct: the beat belongs to `Sysadmin`.
- **Derived beats are no different.** `Bouncer.confront` is stored under `Bouncer` and reached through `Bouncer`'s composed `on scan` hook, where `self = Bouncer`. The `SELF` in the shared `Gatekeeper` skeleton resolves to `BOUNCER`. One template, per-owner voice.

The author writes the same word for "my state," "my line," and "my beat," and learns it once as **mine**.

---

## 13. Full worked rewrites

The global `== interrogation` and `== firewall` beats in `beats/prison.loom` are **deleted** — each moves into the prop that owns it. `-> lockdown` stays a shared global.

### 13.1 TheAdmin — stateful, two hooks, no beat (badge only)

```loom
CHARACTER TheAdmin
  with Algo
  captures: 0 to 100 = 0
  on captured guest
    <broadcast: doomed to participant(guest)>
    <set: self.captures += 1>
    <if: self.captures >= 2>
      <reveal: TheAlgorithm>
  on revealed
    <broadcast: unmasked to faction(Mods) | faction(Chatters)>
```

Byte-for-byte the Part I rewrite, with `with Algo` for the badge. `captures` is a real typed slot; `self.captures` is a per-owner world var. State never routes, so the beat machinery leaves it alone.

### 13.2 Sysadmin — composition + a uniquely-authored owned beat

```loom
CHARACTER Sysadmin
  with Algo
  with Scanner(interrogation)
  beat interrogation(guest)
    SELF
      <if: guest.captured>
        Designation {guest.name}. Heat on file: {guest.heat}. Talk.
        Tell me who showed you the static and maybe your sentence shortens.
      <else>
        Not in a cell yet? Then we have nothing to discuss. Move along.
    <if: guest.captured>
      * Name the Glitchers
        <set: guest.heat -= 10>
        <set: guest.karma -= 15>
      * Say nothing
        <set: guest.karma += 20>
      + Lie through your teeth
        <set: guest.heat += 10>
```

The interrogation scene sits inches below the `with Scanner(interrogation)` line that routes to it. `interrogation` is in `Sysadmin`'s precomputed owned set (§11.5), so `Scanner`'s `-> self.beat` stays `self.interrogation` and resolves to `Sysadmin.interrogation`. The captured-gate is verbatim.

### 13.3 Firewall_Terminal — four composed capabilities + two owned beats (the modularity showcase)

The prop `is AlgoScanner(firewall)` **cannot** express this.

```loom
TRAIT Breachable                    # a capability bundle: ships the escape beat
  beat breach(guest)
    <escape: guest>

TRAIT Ambient(bark)                 # a timed bark routed to one of MY beats
  on every 45s
    -> self.bark

CHARACTER Firewall_Terminal
  with Algo                         # (a) faction badge
  with Scanner(firewall)            # (b) scan → my firewall beat
  with Breachable                   # (c) composes in a `breach` beat
  with Ambient(hum)                 # (d) timer → my `hum` bark
  beat firewall(guest)
    SELF
      <if: guest.captured>
        A seam in the wall, right where the Oracle said it would be. Push it?
      <else>
        The firewall hums. Touching it from the outside only gets you noticed.
    <if: guest.captured>
      * Force the breach — RUN
        -> self.breach               # routes into the composed-in Breachable beat
      + Back away from the wall
        <set: guest.karma += 2>
    <else>
      <set: guest.heat += 15>
      -> lockdown                    # bare → global, unchanged
  beat hum()
    SELF
      A low electric hum. The wall is always listening.
```

Four orthogonal capabilities, four independently-authored one-liners. The owned set is `{firewall, hum}` (own beats) ∪ `{breach}` (from `Breachable`), so `Scanner(firewall)` → `self.firewall` → `Firewall_Terminal.firewall`, and `Ambient(hum)`'s param → `self.hum` → `Firewall_Terminal.hum`. The RUN choice's `-> self.breach` resolves to `Firewall_Terminal.breach`, the beat **composed in by `with Breachable`** — composition and owned-beats visibly interlocking. The `<else>` heat+`lockdown` punishment survives verbatim.

**The win vs the base:** to add "breachable" and "ambient bark" under `is AlgoScanner(firewall)` you would author a bespoke fused trait per combination. Here they are two more lines.

### 13.4 Oracle — 2+ owned beats that cross-reference, driven by an owned-beat visit query

This rewrite deliberately uses `visits(self.prophecy)` instead of a hand-rolled counter, to exercise the major-#3 fix: owned-beat history now resolves owner-first.

```loom
CHARACTER Oracle
  with Algo
  on scan guest
    <if: visits(self.prophecy) == 0>   # owner-resolved: counts Oracle.prophecy entries
      -> self.prophecy
    <else>
      -> self.riddle
  beat prophecy(guest)                 # → Oracle.prophecy
    SELF
      The static showed me your face before you arrived, {guest.name}.
      There is a seam in the firewall. Find it before the sweep.
    * Ask for the way out
      -> self.riddle                   # beat-to-beat, both owned by Oracle
    + Refuse the vision
      <set: guest.karma += 5>
  beat riddle(guest)                   # → Oracle.riddle, distinct key
    SELF
      What runs but never walks, and carries you past the gate?
    <set: guest.heat -= 5>
    -> lockdown
```

First scan: `visits(self.prophecy)` is 0 (no `Oracle.prophecy` entry yet) → route to `prophecy`, which records under `Oracle.prophecy`. Next scan: the query resolves `self.prophecy` → `Oracle.prophecy`, now 1 → route to `riddle`. Any other prop may also declare `beat riddle`; it lands at `<Other>.riddle`, zero collision. `self` stays `Oracle` across every intra-class divert, so both beats' `SELF` speak as `ORACLE`.

### 13.5 Derive-and-specialize

`Interrogation_Booth` and `Bouncer` (§11.3) both `is Gatekeeper` and fill `pitch` / `dismissal` / `options` differently. The `<if: guest.captured>` gate and the `SELF` speaker are authored once in the trait; each deriver writes only leaf prose and choices, never a control-flow keyword. Stored as `Interrogation_Booth.confront` and `Bouncer.confront` — the shared shape, two voices, no collision.

---

## 14. Expressiveness check — every hard case survives

| Case | Post-redesign |
|---|---|
| **Stateful (TheAdmin.captures)** | `captures: 0 to 100 = 0` typed slot; `<set: self.captures += 1>` per-owner world write. Beats never touch state. ✓ |
| **Conditional / captured-gated beats** | `interrogation`/`firewall` keep their `<if: guest.captured> … <else>` gates verbatim; `guest` is carried into the owned beat by the scan binder, read live. ✓ |
| **Multi-hook** | A class lists as many `on` hooks as it likes; composed traits append theirs (additive `matchHooks`). Firewall_Terminal has a scan hook (from `Scanner`) + a timer hook (from `Ambient`). ✓ |
| **Timer** | `Ambient(bark)`'s `on every 45s` flows through `parseTimer`; routes `-> self.hum`. Surveillance's `on every 60s` writing `TheAlgorithm.sweeps` stays faction-scoped (not `self`). ✓ |
| **Owned-beat history** | `visits(self.prophecy)` resolves owner-first to `Oracle.prophecy` (§11.2); `played`/`since` same. Regression from the review's major #3 is closed. ✓ |
| **Disposition / trust** | `trusts Guest: 50 of 100` remains a child declaration merged over the trait (child wins). ✓ |
| **Knowledge writes** | Unchanged — `<set: Guest.knows.X …>` routes through the character store as before. ✓ |
| **Hook override intent** | A child-body `on scan` replaces composed routing (§10.2); `super` keeps both. ✓ |
| **Composed capability contributing a beat** | `with Breachable` installs `Firewall_Terminal.breach`, reached by `-> self.breach`. ✓ |

Nothing the 91-line file expressed is lost; the design only adds power (N-way composition, owned/derived beats, owner-resolved history).

---

## 15. Backward compatibility

**Purely additive. The trivial one-liner is untouched.**

```loom
CHARACTER Crawler is AlgoScanner(crawler_report)   # still one line, still works
```

- **Flat `is Scanner(x)` / `is A, B`** parse and merge exactly as in Part I — `with` is a *second* source of the same `inherits` entries, not a replacement. The nine scanner props stay one line each.
- **Bare diverts stay global.** `resolveBeat` hits `model.beats.get(t.name)` for any `qualifier === null` target — identical to today (sim.ts:795). `-> lockdown`, `-> crawler_report`, every existing global divert is byte-for-byte unchanged. We **reject owner-first bare resolution** (Model 3/4's `-> foo` checking `Owner.foo` before global) because it silently changes existing bare diverts; reaching an owned beat *always* requires `self.`/`Owner.` — a small, deliberate ceremony that buys zero back-compat risk.
- **Cross-file `/` diverts** fall through the owner lookup to global, the same result as today; `/` outranks `.` in target text (§11.2).
- **`slot:` placeholders, `super` in hooks, `== name` global beats** — all unchanged; `beat`/`with`/`fill` are new keywords in class-body position, and a file using none of them compiles bit-identically.
- **The one behavior change to shared merge** is the child-body same-signature **hook override** (§10.2). It affects only cases where a child hook matched a composed/inherited hook of the same signature — which today double-fires. This aligns hooks with the child-wins rule that already governs properties/methods/init; `super` restores the additive behavior where wanted. Flagged under "Edits to Part I."

---

## 16. Phased implementation plan (smallest slice first)

Depends on Part I Slices 1–2 (the merge runs; `compileModel` reads the merged body; `deepCloneCharacterBody`; `parseMixinRef`).

**Slice A — owned beats + qualified diverts (ships §11.1, §11.2, §13.2, §13.4).**
- ast.ts: `CharacterBody.beats: OwnedBeat[]` + `emptyCharacterBody`.
- decl-body.ts: `beat ` branch in `lowerCharacter` (clone of the generator collector, decl-body.ts:664).
- parser.ts: no-slash first-`.` split in `parseDivertTarget` (parser.ts:756), `/` outranks `.`.
- model.ts: register `${decl.name}.${ob.name}` beside the generator loop (model.ts:193), reading the merged body.
- sim.ts: `resolveBeat` replacing the lookup at sim.ts:795; key visits/records on the resolved name; cross-owner self-rebind; unresolved-qualified-divert diagnostic. Owner-first resolution in the `visits` `callFn` (sim.ts:1184) and the ledger query path (`played`/`since`).
- **Tests:** two props owning same-named beats route distinctly; `-> self.beat` → owner's inline beat; `-> Owner.beat` runs and attributes `SELF` to Owner; bare `-> lockdown` still hits the flat map; `visits(self.prophecy)` counts `Oracle.prophecy`; a qualified divert to nothing emits a diagnostic (not a silent no-op).

**Slice B — `with` composition + the forwarding predicate (ships §10, §11.5, §13.3).**
- ast.ts: `CharacterBody.withClauses: string[]`.
- decl-body.ts: `with ` branch appending to `withClauses`.
- bundle.ts: `inherits: [...decl.mixin, ...decl.character.withClauses]`; **child-body hook override** (§10.2 `overrideKeys` filter); copy `parentBody.beats` (deep-cloned + substituted) in `mergeCharacter` with `DerivedBeatConflict` on cross-parent collision; extend `substituteParams` with the **precomputed `ownedBeatNames`** (`collectOwnedBeatNames`, §11.5), not `mergedBody.beats`.
- **Tests:** a four-`with` prop composes faction + two hooks + owned beats; `Scanner(ownedBeat)` forwards to `self.beat` and `Scanner(globalBeat)` degrades to bare (the blocker regression — assert `Sysadmin.interrogation` and `Firewall_Terminal.hum` actually resolve); a child `on scan` overrides a composed `on scan` (and `super` keeps both); `with` and `is` produce identical merges.

**Slice C — derive-and-specialize (ships §11.3, §11.4, §13.5).**
- ast.ts: `CharacterBody.fills: Map<string, RawLine[]>`.
- decl-body.ts: `fill ` class-body branch; keep `slot:` (colon) as the placeholder — no colon-less alias.
- model.ts: `fillSlots(items, fills)` walk before registration; `UnfilledDerivedSlot` diagnostic.
- bundle.ts: **extend `deepCloneCharacterBody` to fresh-copy `beats` (each `OwnedBeat.body`) and `fills`** (anti-cache-corruption); extend the hook-`super` splice (bundle.ts:377-395) to beats.
- **Tests:** two derivers of one template beat get distinct namespaced beats with their own fills; the first deriver's substitution does **not** corrupt the second's (deep-clone regression); an unfilled hole reports and no-ops; two parents shipping the same beat name emit `DerivedBeatConflict`; a narration line beginning with the word "slot" (no colon) stays prose.

**Slice D — optional readability sugar (demand-gated).**
- `when <cond> … otherwise …` lowering to the existing `conditional` node — reads as a stage cue, no sim change. Deferred (orthogonal to both asks).
- LSP: complete owned/derived beat names after `self.` / `Owner.`; hover a trait showing the beats it ships; lint `-> self.<undeclared-beat>`, `fill <no-such-slot>`, and `visits(self.<undeclared>)`.

The Rust mirror tracks the same edits against `lower_character` / `parse_divert_target` / `compile_model` / `Sim::resolve_beat` / `merge_character`; it already namespaces generators and runs merge on its compile path, so no wiring fix is needed — but it needs the same precomputed-owned-name set, hook-override, deep-clone-of-beats/fills, and owner-first `visits` edits.

---

## 17. Open questions / risks

1. **`self.<undeclared-beat>` no-op is now diagnosed, not silent.** A `-> self.pressed` with no such owned or global beat records the unresolved-divert diagnostic (§11.2) and no-ops; the `slot`/`fill` path additionally reports `UnfilledDerivedSlot`. The Slice-D lint catches it at author time.
2. **State-carrying capability applied twice** (inherited from Part I Open Q #4): a trait with its own mutable slot, composed twice, collides on one `self`-owned slot. Recommend applying state-carrying capabilities once; lint a double `with` of a stateful trait.
3. **Cross-owner divert assumptions.** `-> Owner.beat` rebinds `self` to Owner, so a beat reachable both from its owner's hook and a foreign `Owner.beat` must not assume the caller's identity. Documented; the rebind is the correct semantics.
4. **Textual substitution ceiling.** `self.<param>` and `fill` splice whole references / whole sub-blocks; neither parameterizes a fragment mid-sentence. Inherited from Part I §8.3.
5. **Additive hooks vs override surprise.** Two composed capabilities both matching `on scan` both fire (§10.2), correct for orthogonal capabilities but surprising if the author expected exclusivity. Mitigations: the class-body same-signature override (now real) and Part I's `on <event>: none`.
6. **Knot on an owned beat.** `-> Owner.beat#knot` composes the `#` split with the new `.` split; owned-beat sub-knots are a Slice-D edge, since no current example needs a knot inside an owned beat.

---

## Changes from review

**Resolved (blocker):**
- **§11.5 (blocker #1):** the forwarding predicate no longer reads the half-built `mergedBody.beats` (which never contains the applier's own beats at substitution time, so `with Scanner(interrogation)` / `with Ambient(hum)` degraded to dead global lookups and the flagship scans/barks silently did nothing). It now reads a set **precomputed** from the declaration's own `beat` blocks plus every applied trait's shipped beats (`collectOwnedBeatNames`), threaded through every recursive `substituteParams`. Added the blocker-specific regression test to Slice B.

**Resolved (majors):**
- **§10.2 (major #2):** the "child-body hook overrides the composed one" claim is now *implemented* — an `overrideKeys` filter splices out same-signature composed hooks before the own-hook fold — instead of citing code (bundle.ts:397) that only appended. `super` (extend) and `: none` (suppress) are preserved and distinguished. Flagged as a merge-semantics extension in "Edits to Part I."
- **§11.2 (major #3):** owned-beat history (`visits`/`played`/`since`) now resolves owner-first (shared `resolveBeatKey`), fixing the silent-0 regression for `Owner.name`-keyed beats. Oracle (§13.4) rewritten to *use* `visits(self.prophecy)`, dropping the hand-rolled counter, to exercise the fix.

**Resolved (minors):**
- **§11.3 (minor #4):** two distinct parents shipping the same beat name now emit `DerivedBeatConflict` instead of silent first-wins; child override stays legal.
- **§11.3 (minor #5):** kept the placeholder as `slot:` (colon, the existing lexeme) with no colon-less alias, so narration beginning "slot …" can't be swallowed; `fill` stays a bare class-body opener (no prose at that indent).
- **§11.2 (minor #6):** corrected the false "qualifier unused since day one" claim (it is populated for `/` today, just ignored by the sim), made `/`-outranks-`.` precedence explicit, and added an unresolved-qualified-divert diagnostic.
- **§11.3 / Slice C (minor #7):** `deepCloneCharacterBody` is explicitly extended to fresh-copy `beats` and `fills`, closing the template-cache-corruption reintroduction; added to the Slice C task list and tests.

**Deliberately kept (with rationale in-doc):**
- **`with` over an `init(){}` constructor block** (§10.1) — a flat vertical list is more readable for directors and composition is the same merge; `InitDecl` stays scoped to stats.
- **Compile-time owned-vs-global resolution over always-`self` + runtime fallback** (§11.5 rejected alt) — the runtime-only route corrupts non-beat value params (`self.loc`); the precomputed set is position-safe.
- **Bare diverts stay global, never owner-first** (§15) — owner-first bare resolution would silently reinterpret every existing `-> beat`.
- **Child-override implemented rather than dropped** (§10.2) — the additive-hook footgun needs a real mitigation, and `: none` alone can't replace-with-behavior.

## Edits to Part I (keep the two parts consistent)

These spots in the existing document are amended or superseded by Part II; update them when merging:

- **§2.1 line refs.** `CharacterBody` is at **ast.ts:449** and `emptyCharacterBody` at **ast.ts:464** (Part I cites the stale 372/387). Part II adds `withClauses`, `beats`, and `fills` to `CharacterBody` alongside Part I's `params`; update `emptyCharacterBody` for all four.
- **§2.3 forwarding predicate.** `const replacement = ownDecl.params.includes(arg) ? \`self.${arg}\` : arg;` gains a second clause: `|| ownedBeatNames.has(arg)`, where `ownedBeatNames` is the precomputed set of §11.5. This *extends* the forwarding rule (owned beats also keep `self.`); it does not contradict it.
- **§2.3 `UnresolvedTraitArg`.** Its "known beat/faction/location/event" resolvability check must additionally treat an owned-beat name (from `ownedBeatNames`) as resolvable — otherwise `with Scanner(interrogation)` where `interrogation` is an owned (not global) beat false-positives.
- **§2.3 `deepCloneCharacterBody` / bundle.ts:414 `cloneCharacterBody`.** Both must enumerate the new `beats` and `fills` fields (deep-copy `OwnedBeat.body` `RawLine`s and each `fills` entry) for the same cache-corruption reason Part I established for hooks.
- **§2.0 dependency.** Part II §11.1 registers owned beats from `bundle.mergedCharacters.get(decl.name) ?? decl.character`, so it *requires* Part I §2.0 Change 2 (compileModel reads the merged body). Owned beats do not appear without it.
- **Hook-merge semantics (touches §4/§5's additive-hook assumption).** Part I leaves composed + own hooks of the same signature both firing; Part II §10.2 makes a **child-body hook of the same signature win** (with `super` to extend). Any Part I prose that assumes a child `on scan` written alongside a composed `on scan` fires *in addition* should point to §10.2. The §5 disposition/trust "child wins" row is unaffected.
- **§2.5 SELF speaker.** Now also governs owned/derived beats: `self` carried through `-> self.beat` makes `SELF` speak as the owner, and `-> Owner.beat` rebinds `self` for a foreign owner (§11.2/§12). This extends §2.5's scan→beat story to intra-class and cross-owner diverts; no contradiction.