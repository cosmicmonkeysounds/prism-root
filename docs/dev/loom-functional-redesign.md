# Loom redesign: traits with `self`-parameters, faction badges, and the `SELF` speaker

> Status: proposal, hardened after hostile review. Lowers entirely into the
> existing `packages/loom/core/` model; the Rust mirror tracks it 1:1.
> Target file: `docs/dev/loom-functional-redesign.md`.

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
