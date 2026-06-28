# Loom as a social-ecosystem simulator — minimal runtime

Loom is a language for **social ecosystem simulation** — LARPs, interactive
installations, multiplayer games. The unit of play is not a linear story but a
**living world of entities that have relationships and react to events.**

This `sim/` runtime is the minimal engine built from that first principle. It is
validated against a real installation: **"Escape the Internet"** — a Meow-Wolf
style event where party-goers (with a phone app + a personal QR code) join one of
two public factions (**Mods** / **Chatters**), can defect/betray, and may be
**captured** by a hidden third faction (**The Algorithm**) — physically moved into
"the Internet" (a tarp black-box prison) until they **escape**. Named actors have
individual relationships with each party-goer and with the factions.

## The five primitives

| Primitive | `.loom` form | Runtime meaning |
|-----------|-------------|-----------------|
| **Person** | created at runtime (account at the door) | a real human; identity = QR id; cast into a **Role** |
| **Role / Character** | `ROLE Guest` / `CHARACTER Moderator_Prime` | a part. CHARACTER = fixed NPC/actor; ROLE = a slot many Persons fill. Both carry relationships + hooks |
| **Faction** | `FACTION Mods` | a group with membership + ethos + rivalries; membership is dynamic (join / defect) |
| **Location** | `LOCATION Internet` | a physical place; Persons have a current location; `prison: true` traps until release |
| **Hook** | `on scan guest` | a reactive rule: *trigger* → optional `<if:>` guard → *effects* |

## State = the expression `World`

All state lives as **dotted paths** in the ported `expr.World` — no new store:

- entity state: `g42.faction`, `g42.location`, `g42.captured`, `g42.score`
- relationships (directed, subject→object): `Moderator_Prime.trusts.g42`
- collections (for `count()`, comprehensions, broadcast scopes): `Mods`, `Internet`, `Persons`

Every entity id also resolves to **its own name as a string** (`Chatters` → `"Chatters"`),
so `guest.faction == Chatters` reads naturally. Expressions gain a `bindings` map
that substitutes hook-scoped variables (`guest`, `self`) into any path segment, so
`Moderator_Prime.trusts.guest` resolves to `Moderator_Prime.trusts.g42`.

## The loop: events → hooks → effects → fixpoint

The world advances only when something **happens**. External input (the app / props)
calls the **input API**; each call appends a `SimEvent` to the **ledger** and then
**drains the rule engine to a fixpoint**:

```
ingest(event):
  log.push(event)
  for each hook whose trigger matches event:
      bind self / params, eval <if:> guard, run effect body
      → effects mutate World + emit more events → re-drain until quiescent
  return the new ledger envelopes  (app shows notifications, prop shows a response)
```

Hooks fire with a **binding scope**: `self` (the owning entity) plus trigger params
(`on scan guest` binds `guest`). A hook body is executed by the **same executor that
runs beats** — hook bodies are re-parsed into structured `BodyItem`s, so `<if:>`,
dialogue, `<set:>`, and `-> beat` diverts all work identically in hooks and beats.

## Input API (what the app / props / actors call)

- `createPerson(id, name)` → casts into the default Role, emits `accountCreated`
- `join(person, faction)` / `defect(person, to)` / `betray(person, secret)`
- `scan(scanner, person)` → fires the *scanner's* `on scan` hooks (QR / MagicBand
  model). The scanner may be a CHARACTER/prop **or** a Person (peer scan).
- `arrive(person, location)` ; `escape(person)` ; `signal(name, subject?)`
- `choose(person, index)` → resolves a live participant's pending choice
- Read views: `publicFactionOf` (hidden factions read null until revealed),
  `trueFactionOf`, `factionRevealed`, `pendingChoiceFor`

## Effect (directive) vocabulary

`<set: path OP rhs>` (bareword RHS = enum string) · `<join: p to F>` ·
`<defect: p from A to B>` · `<betray: p to F>` · `<capture: p into Loc>` ·
`<release: p from Loc>` · `<escape: p>` · `<reveal: F>` ·
`<broadcast: cue to participant(p)|faction(F)|location(L)>` (scope args are
expressions, so `faction(guest.faction)` works) · `<respond: text>` (addressed
back to the scanning device) · `<cast: p as Role>` · `<promote: p to Role>` ·
`<cue:>`/`<sfx:>` (logged) · `-> beat`

## Trigger forms

`on scan guest` (scanner-owned, binds subject) · `on captured` / `on escape`
(role-owned, `self` = the affected person) · `on captured guest`
(character-owned reaction to *any* subject) · `on lockdown` (character-owned
**global cue**, `self`-only, no subject) · `on exits LOCATION` · `on betray` ·
`on <signal> [subject]` (generic).

## Hardening (verification-driven, 22 scenario tests, two adversarial rounds)

State transitions are **idempotent**: re-capturing an imprisoned guest is a
no-op (no double score-dock), and escape only rewards a genuine
imprisoned→free transition (no point-farming). The hidden villain faction is
**observer-relative** — membership is engine-internal and the app-facing
`publicFactionOf` reads null until a `<reveal:>` (e.g. after N captures) exposes
it.

**The executor is an explicit stack-based trampoline, not the JS call
stack.** This makes a choice a *first-class suspension*: the work stack at the
prompt point IS the continuation, so a choice reached through nested
`-> beat` diverts snapshots everything still pending and `choose(person, index)`
resumes the selected option **and** the full continuation in order — effects
after the interaction never run early. Pending choices are queued per person
(fan-out doesn't clobber); an out-of-range index leaves the prompt answerable.
Movement fires symmetric `exits`/`enters` triggers on every path
(arrive/capture/escape).

The reactive core (per-person isolation, faction-membership consistency,
capture-cascade ordering, identity equality) was adversarially verified as
leak-free across both rounds.

## Why this is the right minimal core

A linear playhead is just one **head** walking a beat; the coroutine scheduler is an
optimization for ambient generators. The irreducible core of *social ecosystem
simulation* is **entities + relationships + a reactive event loop**. Everything else
(multi-head play, tiers, improv windows) layers on top of this without changing it.
