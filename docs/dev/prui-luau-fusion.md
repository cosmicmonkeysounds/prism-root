# PRUI / PRSS / Luau Fusion — three projections, one source

> Goal: today PRUI, PRSS, and Luau are three languages with three
> parsers, cooperating at runtime but sealed off at authoring
> time. This document proposes folding them into a single
> authoring surface — three *projections* of one source — without
> giving up the bounded declarative tree-render contract (§16 of
> the PRUI reference) that makes the runtime cheap and predictable.

## TL;DR

- **PRUI** stays the tree, **PRSS** stays the theme, **Luau**
  stays the behaviour — non-overlapping roles.
- A widget can be **one file** (`.prui` with inline `<style>` +
  `<script>` blocks) or **three** (`.prui` + sibling `.prss` +
  sibling `.luau`); the loader treats them identically.
- Six cross-language reference edges, one canonical syntax each;
  no overloading a language outside its lane.
- **Luau's gradual type system threads through every seam** —
  PRUI expression slots, PRSS computed values, signal payloads,
  macro attrs, scope bindings. `prism lint --types` fails CI on
  any cross-projection mismatch.
- Twelve new authoring mechanisms (§7) — colocated scripts,
  function literals, quasi-quotes, lifecycle attrs, pattern
  matching, suspense, macros, sub-dialects, computed PRSS,
  typed scopes, live probes, keyframe animations.
- **Zero LOC migration cost** — every existing `.prui` /
  `.prss` / `.luau` file continues to parse and render unchanged.

**Status:** design draft (2026-05-15). **Waves A–G landed +
H/I partial, 2026-05-15.** Runtime-complete: (A) colocated
`<script lang="luau">`; (B) closures + `|`/`|>` pipes; (C)
`prui[[…]]` + `prism.macro` (hygienic); (D) `<match>`/`<case>` +
`<suspense>`/`<fallback>`; (E) `prism.dialect` + `<language>` /
`~name{…}` (+ `prui_ast.*` constructors); (F) computed PRSS
`{ lua = "…" }` + native colour helpers; (G) `probe:` namespace
+ `prism.probes:on` + `at:` keyframe namespace. Partial: (H)
inline `<style>` + `<import>`/`ImportResolver` landed,
FS sibling-pairing / fingerprint-cache / `prism new` are host /
build-tool follow-ups; (I) `prism.scope` value bridge landed,
the LSP / `luau-analyze` / `prism lint --types` / Inspector
typing is external-tooling and intentionally not faked. See §9
Waves A–I for as-built notes. Cross-wave deferrals:
type-driven `<match>` exhaustiveness, `<suspense>` coroutine
scheduling + host probe-firing (event-router family), reactive
`prism.state` / per-class PRSS invalidation (reactive-substrate
family), the Effect-driven `at:`/`transition:`/`animate:`
animator, and bundled `prism-builder` dialect files.

**Related docs:** `prui-reference.md` (the surface this extends),
`prss-reference.md` (the stylesheet half), `luau-integration-plan.md`
(the host-side Luau plumbing this rides on), `dioxus-inspiration.md`
(fingerprint cache + reactive substrate).

---

## 1. Motivation

PRUI today is a tight declarative HTML-shaped DSL with thirteen
attribute namespaces, a Pratt-parser expression slot, and a closed
set of seven primitive tags. PRSS is a TOML-shaped stylesheet
language with bounded, validated tokens and classes. Luau is the
scripting layer the daemon and shell already speak. All three
cooperate at runtime — but at *authoring* time they're three
languages with three editors, three diagnostics streams, three
ways of expressing "the value of this thing depends on that
thing." The seams between them are narrow:

- PRUI → PRSS: `class="…"` opts into a class table.
- PRUI → Luau: `luau { … }` action body; `use:` modifier
  attachment; `FacetKind::Script` data facet.
- PRSS → anything: nothing — values are static TOML literals.
- Luau → PRUI: `prism.widget { render = … }` and a per-shell
  registration dance.

That's it. An author who wants a one-line filter, a one-line
on-mount effect, a token-derived hover state, or a tiny custom
helper has to either:

1. Add a Rust block (200+ LOC of boilerplate per behaviour).
2. Author a Luau modifier in a separate file and route through `use:`.
3. Pre-compute the value host-side and bind it as a prop.
4. Hand-author a parallel CSS class with the colour hard-coded.

None of those are wrong, but they're disproportionate. The
gradient from "literal attribute" to "trivial closure" to
"behaviour module" is a cliff, not a slope. And the type story
splinters: PRUI's `ExprValue` is one universe, PRSS values are
another, Luau is a third — even when the underlying data is the
same `DesignTokens` struct.

The proposal: **fold all three into a single authoring surface,
preserve their non-overlapping roles, and thread Luau's gradual
type system through every seam.** Every fragment runs in a
bounded, scope-typed, capability-scoped context. Every mechanism
below either flows into the existing `exec_with_setup` channel
or compiles to closures captured at parse time. The bounded
declarative tree-render contract stays intact — Luau is not a
new evaluator inside the render walk, it's a parse-time and
attribute-time helper that *produces* the values the walker
consumes.

The result:

- An expression slot can hold a Pratt expression *or* a Luau
  function literal.
- A `.prui` file can ship colocated `<script lang="luau">` and
  `<style lang="prss">` blocks; their locals / classes become
  document-scope bindings.
- Sibling files (`widget.prss`, `widget.luau`) are picked up
  automatically; no `<import>` ceremony required.
- A Luau file can embed PRUI literals (`prui [[ … ]]`) and load
  PRSS data programmatically (`prss "./theme.prss"`).
- A PRSS value can call Luau (`{ lua = "darken(...)" }`) and
  subscribe to the same signal graph the rest of the document
  uses.
- New tags, sub-dialects, lifecycle hooks, suspense boundaries,
  computed styles, and probes all share one substrate.
- Every cross-language seam typechecks end-to-end through
  LuaLS / `luau-analyze`.

---

## 2. Design principles

These constrain the proposals below. If a mechanism breaks one,
either kill the mechanism or fix the principle.

1. **PRUI's tree-render contract is sacred.** No new mechanism
   may turn the render walk into an unbounded computation. Luau
   fragments inside the DSL are either: (a) evaluated at *attribute*
   time to produce a value, (b) attached as *handlers* fired by
   the event router, or (c) compiled to *render-time pure
   functions* that take scope-bound args and return a value. None
   of them recurse, schedule, or yield during the walk.

2. **One parser, one lowering.** Every embedded Luau form lowers
   to data structures the existing `interpret::lower_document_with_scope`
   already understands. Add new variants where required; don't
   fork the walker.

3. **Authorable, editable, generatable, hookable.** Carrying the
   `feedback_clay_luau_authoring` rule forward — every new
   mechanism must be reachable from each of those four angles.
   If a feature can only be written by hand, it's wrong.

4. **Capability-scoped.** Luau fragments inherit the surrounding
   document's `PrismContext` capability set. A facet-resolver
   fragment is read-only; a signal-handler fragment is read/write;
   an admin-mode fragment sees everything. Same matrix as
   `luau-integration-plan.md` §2.

5. **No big-bang.** Each mechanism ships as a wave; the existing
   `.prui` corpus continues to parse and render unchanged at every
   step.

6. **Readable first.** Inspirations get raided for syntactic
   ideas but each landing is judged on *whether a Prism author
   recognises it without reading the spec*. Cleverness that
   demands lookup loses.

---

## 3. Inspirations (and what we take from each)

Brief — one line each — so the design has named ancestry.

| Source | What we take |
|---|---|
| **Svelte 5** | `<script>` colocated state, `$:` reactive declarations, `{#each items as item}` block + `{:else}` fallback |
| **Vue SFC** | Single-file component shape, `<script setup>`, named slots |
| **Elixir / Phoenix LiveView** | `~H[[ … ]]` quasi-quote, server-driven hydration model |
| **Imba** | Tags as first-class values, terse memo (`<div [memo]>`), CSS-in-tags |
| **Roc / Gleam / Elm / F#** | Pipeline operator `\|>`, exhaustive `case` |
| **Rebol / Red** | Dialected sub-languages — string sigils that swap parser |
| **Racket / Scheme** | Quasi-quote / unquote (`'expr` / `,expr`) for AST staging |
| **Hyperscript / _hyperscript** | Imperative one-liners on element attributes |
| **HTMX** | Declarative behaviour via attribute namespaces |
| **SwiftUI** | View builders, `@State`, computed views, modifier chains |
| **Cue / Dhall / KCL** | Structural validation + computation in one config language |
| **Glimmer / Handlebars** | `(helper arg arg)` helpers as values |
| **Rust `match`** | Exhaustive pattern matching on tagged unions |
| **React 18 Suspense** | Async UI boundaries that render fallbacks while waiting |
| **OCaml PPX / Racket macros** | Hygienic macros that expand to AST, not strings |
| **LSP-driven languages (Roc, Grain)** | Diagnostics-first authoring |

The cross-pollination Prism is uniquely positioned for: most of
these languages had to invent their own evaluation runtime. We
already have one — Luau — sitting *next to* the DSL. The fusion
is mostly plumbing.

---

## 4. The core insight: three projections of one source

PRUI, PRSS, and Luau today are three files, three parsers, three
mental models. They cooperate at runtime — `class="…"` on a PRUI
element resolves through PRSS; `luau { … }` on an `on:` action
runs in Luau — but at *authoring* time they're separate worlds.

The proposal makes all three **projections of one source**. Each
can host inline fragments of the others. PRSS and Luau are
*optional* — a widget that needs neither styles nor logic can be
pure PRUI — but PRUI itself is required because it's the tree.
The same widget can be authored as one file (`.prui` with inline
`<style>` + `<script>`) or three (`.prui` + sibling `.prss` +
sibling `.luau`); both lower to byte-identical output.

HTML's relationship to CSS and JS is the existence proof — every
browser accepts `<style>` / `<script>` inline, sidecar `.css` /
`.js`, *and* arbitrary mixes — and that flexibility is why the
authoring surface scales from one-liner pen demos to enterprise
component libraries. We mirror the shape, with two additions
HTML doesn't have: non-overlapping roles enforced at parse time,
and a single gradual type system spanning all three projections.

```
       ┌─────────────────────────────────────────────┐
       │              widget.prui                    │
       │                                             │
       │  <style lang="prss">           ← inline     │
       │    [class.card]                  PRSS       │
       │    background = "…"                         │
       │  </style>                                   │
       │                                             │
       │  <script lang="luau">          ← inline     │
       │    local state = prism.state{…}  Luau       │
       │    local function color(p) … end            │
       │  </script>                                  │
       │                                             │
       │  <container class="card"       ← PRUI       │
       │     style:color="{color(p)}"     body       │
       │     on:click="luau { … }">                  │
       │    …                                        │
       │  </container>                               │
       └─────────────────────────────────────────────┘
                          ↕   ↕   ↕
       ┌─────────────┬───────────┬─────────────────┐
       │ widget.prui │widget.prss│  widget.luau    │
       │             │           │                 │
       │ <container  │[class.card│ local function  │
       │   class=    │background │   color(p) …   │
       │   "card"…>  │  = "…"    │ end             │
       │             │           │                 │
       │             │           │ prui [[ … ]]    │
       └─────────────┴───────────┴─────────────────┘
                  multi-file (sidecar)
```

The three files are basename-paired by convention: `widget.prui`
implicitly imports `widget.prss` and `widget.luau` from the same
directory if they exist. The same content authored as
`<style lang="prss">` and `<script lang="luau">` blocks inside
the single-file PRUI yields **byte-identical** lowered output.

Either projection can host inline fragments of the others.
PRSS values can call into Luau (`{ lua = "…" }`). PRUI can quote
Luau inline (`@click="luau { … }"`) and reference PRSS classes
(`class="card"`). Luau can quote PRUI inline (`prui [[ … ]]`)
and load PRSS programmatically (`local theme = prss "./theme.prss"`).
The bridge is symmetric.

The rest of this doc fleshes out (a) the file-resolution and
inline-block model that makes the projection equivalence work,
and (b) the per-mechanism authoring surface that lives on top.

---

## 5. Three languages, one source — the file model

This section is the architectural backbone. Every mechanism in §6
assumes this resolution model.

### 5.1 The three roles

| Language | File ext | Role | What it owns |
|---|---|---|---|
| **PRUI** | `.prui` | Markup — the *tree* | Element vocabulary, attribute namespaces, control flow, slots, host-children. HTML in this analogy. |
| **PRSS** | `.prss` | Stylesheet — *theming* | Design tokens, named classes, state variants, computed values. CSS in this analogy. |
| **Luau** | `.luau` | Logic — *behaviour* | Helpers, reactive state, lifecycle, signal handlers, macros, dialects, derived values. JS in this analogy. |

The roles are *non-overlapping*: PRUI cannot define a token,
PRSS cannot define an element, Luau cannot define a CSS-shaped
selector. Cross-cutting concerns flow through the explicit
references documented below — never by overloading a language
into a role outside its lane.

### 5.2 Multi-file: convention-based pairing

The default and recommended shape. Three sibling files named
identically apart from extension:

```
widgets/
  task-card.prui       ← markup (always required)
  task-card.prss       ← styles (optional)
  task-card.luau       ← logic  (optional)
```

When `task-card.prui` loads, the document loader checks for
the sibling `.prss` and `.luau` *automatically*. No `<import>`,
no `src=` — just file-system convention. Found siblings are
attached to the document's scope before `interpret` runs.

This is the same convention Vue Single-File Components export to
when they "eject" (`Component.vue` ↔ `Component.css` ↔
`Component.ts`), and the same convention SwiftUI / React projects
adopt by hand. We make it implicit so no boilerplate is needed
for the common case.

**Cardinality:**

- **0..1 PRSS sibling.** A `.prui` file pairs with at most one
  same-name `.prss`. Need more styles? Either import additional
  PRSS files via `<import>` (§5.4) or move shared theme tokens to
  a project-root `theme.prss`.
- **0..1 Luau sibling.** Same constraint. Multi-file Luau
  decomposes through Luau's own `require` rather than ad-hoc
  PRUI imports.

**Override:** if a directory contains both `task-card.prui` with
an inline `<script lang="luau">` *and* a sibling `task-card.luau`
file, the loader concatenates: sibling first, inline second, so
the inline block's locals can reference the sibling's exports.
Same for PRSS, but `class` definitions in the inline block win
on collision.

### 5.3 Single-file: inline `<script>` and `<style>` blocks

The HTML-in-one-file shape. Any `.prui` file may host top-level
`<script lang="luau">` and `<style lang="prss">` blocks:

```prui
<style lang="prss">
  [tokens.colors]
  accent = "#7c3aed"

  [class.card]
  background = "{tokens.colors.surface}"
  radius = 8
  padding = "12 16"
</style>

<script lang="luau">
  local state = prism.state { expanded = false }

  local function priority_color(p)
    if p == "high" then return tokens.colors.danger end
    return tokens.colors.text_secondary
  end
</script>

<container class="card">
  <text style:color="{priority_color(task.priority)}">{task.title}</text>
  <button on:click="state.expanded = !state.expanded">…</button>
</container>
```

**Why this is allowed (reversing the earlier draft's
rejection):** the file-organization argument is symmetric. If
single-file authoring is good enough for `<script>`, it's good
enough for `<style>`. The objection that ended up motivating
"styles always go in `.prss`" was about *runtime layering* — the
fingerprint cache, the class-merge order, the inspector
addressing path. None of those depend on whether the PRSS text
lives in a sibling file or an inline block; both feed the same
`PrssDocument` parser. So the layered argument doesn't pay rent.

**Cardinality and order:**

- **One `<script>` block per file.** Multiple scripts encourage
  IIFE-style scope juggling we don't want; a single script with
  Luau `require` for additional modules is the path.
- **Multiple `<style>` blocks allowed.** Multiple sheets compose
  predictably (later sheets layer over earlier ones, matching PRSS
  multi-file load order). The pragmatic case: a base stylesheet
  for the component plus a per-variant `<style>` block gated by
  `if=`.
- **Order on the page is irrelevant.** `<style>` and `<script>`
  blocks are pulled to the top of the document before lowering,
  in source order, regardless of where they appear among element
  siblings. Authors typically put them at the top for
  legibility; the parser doesn't care.
- **No nested blocks.** `<style>` inside a `<container>` is a
  parse error. The blocks are document-level only — same rule as
  HTML's `<head>` content.

### 5.4 Explicit imports — `<import>`

For cross-component reuse (a theme stylesheet shared across many
widgets; a Luau helper module used in three places), explicit
imports beat convention. The `<import>` element is a self-closing
top-level tag:

```prui
<import stylesheet="./theme.prss"/>            <!-- merge PRSS into document scope -->
<import stylesheet="./elevations.prss" as="elev"/>  <!-- namespaced — referenced as class="elev.card" -->
<import script="./behaviour.luau"/>            <!-- merge Luau locals into scope -->
<import script="./fmt.luau" as="fmt"/>         <!-- namespaced — call via fmt.currency(…) -->
<import widget="./button.prui"/>               <!-- register sibling-named tag — <button/> -->
<import widget="./fancy-card.prui" as="card"/> <!-- register as <card/> -->
<import dialect="./markdown.luau"/>            <!-- register a dialect (§7.8) -->

<container>
  <card title="…"/>
  <text>{fmt.currency(price)}</text>
</container>
```

Six import shapes, one element, all consistent: pick the
projection (`stylesheet` / `script` / `widget` / `dialect`),
optionally name it (`as=`), and the loader handles the rest.

**Resolution rules:**

- Relative paths resolve from the importing `.prui` file's
  directory.
- Absolute paths starting with `prism://` reach into the
  workspace's `scripts.*` glob roots declared in `.prism.json`
  (so `prism://lib/fmt.luau` resolves to the workspace's
  shared library).
- Imports are hoisted before script blocks and styles, so a
  `<script>` body can reference an imported helper:
  `<import script="./fmt.luau"/>` + `<script>local x = fmt.currency(5)</script>`.

**No circular imports** — a `.prui` cannot import a `.prui` that
(transitively) imports it back. The widget-as-tag dispatch works
through the resolver, not through structural import, so a card
that recursively renders nested cards uses `<card/>` directly
without an import cycle.

### 5.5 Cross-language references — the six edges

Six directions, each with one canonical syntax. None of the
edges are bidirectional in a single expression — every reference
flows one way, the target language sees a value the source
projected for it.

| From → To | Mechanism | Example |
|---|---|---|
| PRUI → PRSS | `class="…"` opts into PRSS-defined classes | `<container class="card primary"/>` |
| PRUI → Luau | `{expr}` expression slots resolve Luau scope identifiers; `luau { … }` action bodies; `<script>` block scope | `style:color="{priority_color(p)}"` |
| PRSS → Luau | `{ lua = "…" }` values evaluate in document Luau scope | `background = { lua = "darken(tokens.colors.accent, 0.1)" }` |
| PRSS → PRUI | (none — stylesheets don't construct elements) | — by design |
| Luau → PRUI | `prui [[ … ]]` quasi-quote; `prism.widget { render = … }` | `local node = prui [[ <container/> ]]` |
| Luau → PRSS | `local style = prss "./theme.prss"` loads a stylesheet as data; `prism.tokens` reads the merged token table | `local accent = prism.tokens.colors.accent` |

PRSS → PRUI deliberately doesn't exist. CSS doesn't construct
DOM nodes; PRSS doesn't construct PRUI elements. A stylesheet
that *generated* tree shape would muddy the inspector path and
the fingerprint cache.

### 5.6 Scope and resolution order

When a PRUI expression slot resolves an identifier, the lookup
walks a stack of frames. Adding inline `<script>` blocks and
imports extends this stack:

```
expression slot {expr}
  ↓
1. let bindings (sibling-scoped, §6.3 of prui-reference)
2. for-loop iteration variables (item, idx, key)
3. <script> block locals (this doc §7.1)
4. imported Luau modules (named via `as=` from <import script>)
5. host_children scope / slots
6. tokens (always present — design tokens via PRSS + runtime defaults)
7. host bindings (props from the resolver)
8. document globals (prism.*, ui.*, error)
```

Inner frames shadow outer ones — a `for="task in tasks"` iteration
variable shadows a `<script>` block's `task` local. This matches
Lua's own lexical scoping intuition and the PRUI reference's
`<let>` shadowing rule.

PRSS resolution is simpler: a `class="a b c"` attribute walks
the classes left-to-right, each class's `extends` chain bottom-up,
applying `apply_style_override` in order. Imports merge classes
into the document's class table; namespaced imports
(`as="elev"`) keep them in a sub-table addressed via
`class="elev.card"`.

Luau scope is plain Luau — `<script>` block locals plus any
`<import script>` modules plus the `prism.*` global. The
`<script>`'s `_ENV` is per-document, so two documents loading the
same widget get independent Luau states (Resolved Decision 4 of
the integration plan).

### 5.7 Hot-reload boundaries

The fingerprint cache (`prism_ui_build::FingerprintCache`) treats
each file independently:

| File changed | What invalidates |
|---|---|
| `widget.prui` | Same as today — `LiteralOnly` patch or structural respawn |
| `widget.prss` | The class table for the document; nodes that reference changed classes mark dirty; rest of the tree reuses cache |
| `widget.luau` | The script's compiled bytecode; `prism.state` values *survive* if the source-position keying matches (Svelte-style HMR — see §9 open question 2); derived values re-evaluate |
| Inline `<style>` or `<script>` block changes | Same handler as the equivalent sibling file — the FingerprintCache treats the inline block as a virtual file keyed by `{prui-path}#style:0` / `#script:0` |

This means a developer working on a card widget can edit the
markup, the styles, or the behaviour and get exactly the
narrowest-scoped reload that's safe.

### 5.8 One-file vs. multi-file: how to choose

Both shapes lower to the same `Node` tree. The choice is
authoring ergonomics:

| Use one file (`.prui` with inline blocks) when | Use three files when |
|---|---|
| Building a prototype or pen-style demo | Building a reusable component for a library |
| The styles fit in <10 PRSS lines | The styles want their own diff history |
| The Luau is <20 lines and component-local | The Luau is shared across multiple widgets |
| You want to send a single file in a code review | Your team's tooling treats `.luau` / `.prss` separately for linters / coverage / tests |
| Teaching / docs | Production component packages |

**Refactoring is non-destructive.** A single-file widget's
`<style lang="prss">` block can be moved to a sibling
`widget.prss` (delete the block, save the contents next to the
file, reload) with zero changes to the PRUI body. Same for
`<script lang="luau">`. The parser produces the same intermediate
representation either way.

---

## 6. Use Luau's type system *fully*

Luau is **gradually typed** — annotations are optional, but
when present LuaLS / luau-analyze enforce them at edit time and
the runtime can opt into strict checks. Prism has invested
heavily in this story already: `prism codegen luau-types` emits
`core.d.luau` + `builder.d.luau` + `signals.d.luau` from the
authoritative Rust sources (`luau-integration-plan.md` Phase 5).
We have first-class typed access to `prism.objects` /
`prism.edges` / `prism.tokens` / `prism.signals` / every widget
schema today, and the macro pipeline keeps it accurate by
deriving the stubs.

**The proposal: every authoring surface this doc introduces is
typed, and authors are *expected* to lean on it.** Untyped Luau
is permitted (gradual typing means migration is painless) but
the linter, the templates, and the docs all push toward typed.
The same discipline that made Roc and Grain feel safer than
their dynamic ancestors — diagnostics first, completion-driven
authoring — is the experience we want for PRUI fusion.

### 6.1 What Luau gives us out of the box

| Feature | What it does | Where it pays off |
|---|---|---|
| **Type annotations** | `local x: number = 5` ; `function f(a: string): boolean ... end` | Every `<script>` block local; every macro `attrs`/`children`; every signal payload |
| **Type aliases** | `type Task = { id: string, title: string, ... }` | Shared domain types in `prism://lib/types.luau` |
| **Generic functions** | `function map<A, B>(xs: {A}, f: (A) -> B): {B} ... end` | Pipeline helpers, dialect parsers |
| **Union & intersection** | `type Status = "open" \| "done" \| "blocked"` | `<match>` exhaustiveness, prop validation |
| **Singleton string types** | `"open"` as a type, not just a value | Tagged-union dispatch, signal names |
| **Refinement** | Narrowing on `type(x) == "string"` / `if x.tag == "Foo"` | `<match>` case body inference |
| **`typeof`** | `typeof(prism.tokens.colors.accent)` | Token-type-derived values stay in sync |
| **`--!strict` / `--!nonstrict`** | Per-file enforcement level | Production widgets `--!strict`; pen demos `--!nonstrict` |
| **Module type exports** | `export type Card = ...` | Cross-file shared shapes |

LuaLS already implements all of this; we just have to wire the
authoring surface to use it consistently.

### 6.2 PRUI expression slots are typed too

The biggest leverage point. Today PRUI bindings are loosely
typed (`ExprValue` is a tagged sum carried at runtime). The fusion
flips this: **every binding that crosses the PRUI / Luau seam
has a declared type, and the LSP enforces it.**

```prui
<script lang="luau">
  --!strict
  type Task = {
    id: string,
    title: string,
    priority: "low" | "medium" | "high",
    status: "open" | "in_progress" | "done",
    notes: string?,
  }

  ---@param task Task
  ---@return string
  local function priority_color(task: Task): string
    if task.priority == "high"   then return tokens.colors.danger end
    if task.priority == "medium" then return tokens.colors.accent end
    return tokens.colors.text_secondary
  end

  --- The host binds this prop; we declare what we expect.
  ---@type Task
  local task = prism.scope.task

  --- Reactive state with a declared shape.
  local state: prism.State<{ expanded: boolean, filter: Task.status | "all" }> =
    prism.state { expanded = false, filter = "all" }
</script>

<container>
  <text style:color="{priority_color(task)}">{task.title}</text>
  <text>{task.foo}</text>                  <!-- LSP error: Task has no field 'foo' -->
  <button on:click="state.fliter = 'open'"> <!-- LSP error: typo, did you mean 'filter'? -->
    Filter open
  </button>
</container>
```

**How the LSP sees this:**

1. The PRUI LSP layer (`PrismUiSyntaxProvider`) walks the
   `<script>` block first, hands its source to LuaLS, and gets
   back a typed symbol table.
2. Every `{expr}` slot in the file is rewritten internally to
   *equivalent Luau code* and passed back through LuaLS for
   typechecking. Diagnostics from LuaLS are translated back to
   PRUI source ranges and surfaced inline.
3. `prism.scope.<name>` is the typed handle for "a binding the
   host (resolver) provides". The `---@type` annotation declares
   the contract; the host's `BlockSpec` schema is checked
   against it at boot.

The translation rewrite is mechanical: `{task.title}` becomes
`(task.title)` in a synthetic Luau context; `{ priority_color(t) }`
stays as-is. The PRUI expression grammar is a *subset* of Luau
expressions, so the rewrite is identity for most slots; the
slot's expected return type (number for `width`, string for
`style:color`, etc.) becomes the type-check target.

### 6.3 PRSS values are typed too

`{ lua = "…" }` PRSS values go through the same type pipeline.
Each PRSS field has a declared type:

| PRSS key | Type |
|---|---|
| `background`, `color`, `tint`, `border-color` | `Color` (hex string `#rrggbb[aa]` or token reference) |
| `radius`, `padding`, `padding-*`, `gap` | `number` (px) or `string` (CSS-shorthand) |
| `width`, `height` | `Sizing` (`"grow"` \| `"fit"` \| `number` \| `"<n>%"`) |
| `font-size` | `number` |

Annotated upstream as Luau types in `core.d.luau`. A
`{ lua = "darken('not-a-color', 0.1)" }` call gets a type error
at PRSS-load time, not a runtime miscompile.

### 6.4 Signal payloads are typed end-to-end

`signals.d.luau` already generates per-component signal-payload
types. The fusion extends this all the way to the handler body:

```prui
<script lang="luau">
  --!strict

  -- Inferred from signals.d.luau — no hand annotation needed.
  -- type SaveEvent = { source: string, timestamp: number }

  prism.on_signal("save", function(event)
    -- event is SaveEvent — typechecked
    prism.objects:update(event.source, { savedAt = event.timestamp })
    --                       ^^^^^^^^^ LSP knows this is a string id
  end)
</script>

<button on:click="emit save">Save</button>
```

The `signals.d.luau` codegen pass walks the document's component
tree, infers which signal handlers can fire in this file, and
synthesises the union type. `prism.on_signal("save", fn)` is
overloaded by string-literal type — the function arg's type is
narrowed to `SaveEvent` because `"save"` is a singleton string
in the signal-name union.

### 6.5 Macros and dialects ship their own types

A `prism.macro` declaration includes a typed schema; the macro's
call site at the PRUI seam typechecks attribute values:

```luau
-- macros/empty-state.luau
--!strict

export type EmptyStateAttrs = {
  title: string,
  subtitle: string?,
  icon: string?,
}

return prism.macro("empty-state", function(attrs: EmptyStateAttrs, children)
  return prui [[
    <container direction="column" gap="8" padding="32">
      <image src="{attrs.icon or 'icon:inbox'}"/>
      <text>{attrs.title}</text>
      <text if="{attrs.subtitle}">{attrs.subtitle}</text>
      <fragment>{children}</fragment>
    </container>
  ]]
end)
```

When a PRUI file uses `<empty-state>`:

```prui
<empty-state title="No tasks" subtitleX="Add one"/>
<!--                          ^^^^^^^^^^ LSP error: unknown attribute, did you mean 'subtitle'? -->
<empty-state title="{state.count}"/>
<!--                ^^^^^^^^^^^^^ LSP error: title expects string, got number -->
```

Same for dialects: `prism.dialect { name, parse }` exports a
type that names the dialect; LSP completes dialect names from
the document-scope dialect registry.

### 6.6 Strict mode by default for production widgets

Project-level convention: any file under
`scripts.widgets` / `scripts.automations` / `scripts.build_steps`
in `.prism.json` is loaded with `--!strict` automatically unless
the file opens with `--!nonstrict`. This is a one-line addition
to `prism_builder::load_widgets` and friends; it makes typed the
default and untyped the explicit opt-out.

Pen-style single-file `.prui` files default to `--!nonstrict`
because they're prototypes, but the inspector / editor will
suggest "annotate state shape" / "annotate prop type" linter
hints to nudge upgrade.

### 6.7 Authoritative Rust → Luau pipeline

The whole stack rides one principle: **Rust is the source of
truth, Luau is the projection.** Reinforced by:

- **`prism codegen luau-types`** (already shipped) regenerates
  `.d.luau` from `#[luau_expose]` annotations. Run on every CI
  build; type drift surfaces as a CI failure.
- **`prism.scope.<name>`** is the typed binding-resolver bridge
  (§6.2). The host's `BlockSpec` schema is the source of truth;
  the `.d.luau` declares the matching Luau view.
- **Macro / dialect / widget registration** ships its type
  exports through the same codegen — the registry is the source,
  the `.d.luau` is the view.
- **PRSS values** type-check against the same `Color` / `Sizing`
  / `Dimension` types `prism-ui-runtime` consumes.

Authors should never hand-write a parallel type that Rust
already knows about. The `core.d.luau` / `builder.d.luau` /
`signals.d.luau` triad is the single import path.

### 6.8 Enforcing this in the authoring loop

Tooling pushes typed authoring:

1. **`prism lint --types`** runs `luau-analyze` over every
   `.luau` and every `<script>` block in every `.prui`, plus
   the rewritten expression-slot context, and fails the lint on
   any error.
2. **`prism dev`** surfaces type errors in the same inline
   diagnostics gutter as parse errors. A red squiggle under a
   `{task.foo}` slot is as visible as one under malformed PRUI.
3. **Templates** (`prism new widget`) emit `--!strict` and a
   stub `type Props = { … }` declaration as the first lines of
   `widget.luau`. Strict mode is the path of least resistance.
4. **The Luau REPL panel** (`luau.eval`, integration plan
   §4.7) runs typed by default — expressions are checked before
   evaluation; ambiguous types surface as warnings.
5. **The Inspector** displays each binding's *declared* type
   next to its current value. Mismatches show as a yellow flag
   on the property row.

### 6.9 What full type discipline buys us

| Without types | With types |
|---|---|
| `{task.foo}` silently renders `nil`, debugged at runtime | LSP error at edit time |
| Renaming a prop breaks every consumer silently | Renaming surfaces every reference, refactor-safe |
| `<match>` over a tagged union may miss a case | Exhaustiveness checked at parse time |
| PRSS `darken('blue', 0.1)` returns `nil`, page renders broken | PRSS load fails with "expected Color, got string" |
| Signal handler reads `event.userId` where Rust emits `user_id` | Mismatch flagged from `signals.d.luau` |
| Cross-component drift as schemas evolve | One source (`#[luau_expose]`); codegen prevents drift |

The cost: writing `type Task = { … }` once. The benefit: every
consumer of `Task` in every PRUI / PRSS / Luau context across the
codebase typechecks. Refactor-friendly, completion-rich,
diagnostics-first — the experience Roc and Grain promised for
ML-derived languages, applied to a UI DSL.

---

## 7. Mechanisms

Twelve authoring constructs that ride the file model (§5) and
type system (§6). Each is independently useful and lands as its
own implementation wave (§9). Listed roughly in order of leverage
— §7.1 is the foundation; the rest compose on top.

### 7.1 `<script lang="luau">` — colocated module

The single biggest leverage point. A `.prui` file can host one
top-level `<script lang="luau">` block; its body runs once per
document load with the **document scope** as its return surface.
Three kinds of declarations get hoisted:

```prui
<script lang="luau">
  -- (1) Helper functions — visible to every expression slot
  local function priority_color(p)
    if p == "high"   then return tokens.colors.danger end
    if p == "medium" then return tokens.colors.accent end
    return tokens.colors.text_secondary
  end

  -- (2) Reactive state — surfaces as a bound atom-like value
  local state = prism.state {
    expanded = false,
    filter   = "all",
  }

  -- (3) Derived values — recomputed when deps change
  local visible_tasks = prism.derive(function()
    return tasks
      :filter(function(t) return state.filter == "all" or t.status == state.filter end)
      :sort_by("priority", "desc")
  end)

  -- (4) Lifecycle
  prism.on_mount(function()
    print("card mounted")
  end)

  return { priority_color, state, visible_tasks }
</script>

<container direction="column" gap="8">
  <text style:color="{priority_color(task.priority)}">
    {task.title}
  </text>
  <button on:click="state.expanded = !state.expanded">
    {state.expanded ? "Collapse" : "Expand"}
  </button>
  <container if="{state.expanded}" for="t in visible_tasks">
    <text>{t.title}</text>
  </container>
</container>
```

**What's happening:**

- `local` declarations in the `<script>` body become **scope
  bindings** accessible from every `{expr}` slot in the same file.
  No `return` block needed — the parser walks the script's AST
  (via `full-moon`) and harvests every top-level `local <name>`.
- `prism.state({ … })` returns a *reactive table* whose fields are
  signals. Reads inside expressions auto-subscribe the surrounding
  `BlockInvalidator` (Phase 3b reactive context already exists
  in `RenderScope`). Writes from a `luau { … }` action body
  trigger the block to re-lower.
- `prism.derive(fn)` returns a memoised signal — reruns when any
  signal it read changes.
- `prism.on_mount` / `on_update` / `on_cleanup` register
  lifecycle callbacks against the **block's** lifecycle, not the
  shell's, so unmounting the block tears them down.

**Why one script per file:** keeping it singular makes the
mechanism predictable — bindings shadow document-scope names
the same way `<let>` does (§6.4 of prui-reference). Multi-script
files are a slippery slope to "Lua REPL with HTML attached"; we
want "HTML with helpers."

**Implementation:** parse-time pre-pass before the control-flow
expander. The script block's source is handed to a *bounded*
`mlua::Lua` instance scoped to the document; the script's locals
populate a `LuauScopeFrame` that sits between the
`tokens`/`host_children` frame and the user-binding frame in
`LowerScope`. Expression slots that resolve a bare identifier
fall through the new frame transparently — `priority_color(task.priority)`
is **the existing call-resolver path** (§5.4.1 of the PRUI ref,
`try_call_owned`), just with the function lookup augmented to
hit the Luau scope.

**Cost guard:** each `prism.state` field is one signal; each
`prism.derive` is one effect. Per-document Lua state per
`luau-integration-plan.md` "Resolved Decision 4" caps blast
radius.

### 7.2 Luau in expression slots — function literals & pipelines

Every `{…}` slot today runs through the Pratt expression parser.
The proposal: when the body starts with `\fn` or `|args|` (a
function literal sigil), the slot switches to **Luau**.

```prui
<!-- Function literal — for ad-hoc closures. -->
<container for="task in tasks | filter(|t| t.priority == 'high')">
  <text>{task.title}</text>
</container>

<!-- Same idea, more readable with `\fn` for multi-line. -->
<container for="grp in entries(group_by(tasks, \fn(t) return t.assignee end))">
  <heading>{grp.key}</heading>
</container>
```

**The pipe form `a | f`** is sugar for `f(a)` — borrowed from F#
and Elm. Compose pipelines without nesting:

```prui
<container for="t in tasks
    | filter(|t| t.status == 'open')
    | sort_by(\fn(t) return -t.priority end)
    | take(5)">
  …
</container>
```

The pipe operator is a parser-level rewrite; the right-hand-side
of `|` must be a call expression and the left-hand-side becomes
the first argument. No special evaluation order — `|>` (Elixir)
is equivalent and accepted as an alias.

**Why this works:** PRUI's existing
`try_call_owned` already accepts string-named field references
as the "closure" surface. The function-literal form is a
*generalisation* of that — a string name becomes one of several
ways to identify a callable. The runtime function table grows to
accept `Closure(LuauRef)` alongside `BuiltinHelper`.

**What it doesn't do:** the closure body still can't side-effect
through the render walk. The Luau function is called with the
iteration's bound values, returns a value, and the walker
continues. No `print`, no global mutation. We install the
function in a sandboxed sub-`Lua` whose `os` / `io` / `debug`
libraries are stripped.

### 7.3 PRUI quasi-quote in Luau — `prui[[ … ]]`

The reverse direction. Inside a Luau file (or `<script>` block):

```luau
-- widgets/badge.luau
return prism.widget {
  id = "badge",
  schema = {
    prism.field.text("label"),
    prism.field.select("tone", { options = {"info","warn","danger"} }),
  },
  render = function(props)
    return prui [[
      <container
          padding="4 8"
          style:radius="999"
          style:background="{tones[props.tone]}">
        <text font-size="12" style:color="#fff">{props.label}</text>
      </container>
    ]]
  end,
}
```

`prui[[ … ]]` is a Lua long-bracket string whose contents go
through the **same `prism_core::language::prism_ui::parse`** the
`.prui` files use, with one twist: bare identifiers inside `{…}`
slots resolve against the enclosing Luau scope (so `props.tone`
above lifts from the `render` argument).

Why long-brackets specifically: Lua treats `[[ … ]]` as a raw
string, so the PRUI body needs no escaping. The host wires this
up by detecting the `prui` *function call* + string argument at
parse time and rerouting to the PRUI parser. Luau's `string`
hooks let us register `prui` as a Lua function whose call form
returns a `VirtualNode` directly.

**This is the inverse of `<script lang="luau">`:** instead of
Luau inside PRUI, PRUI inside Luau. They compose. A widget
defined in `.luau` *can* host a `<script lang="luau">` block
inside its `prui [[ … ]]` body for a co-located helper — it
nests cleanly because each side parses straightforwardly into
the other.

### 7.4 Lifecycle namespace — `effect:`

PRUI gets a new attribute namespace (`effect:`) that attaches
lifecycle callbacks to a specific element:

| Attribute | Fires when | Body |
|---|---|---|
| `effect:on-mount` | Element is first lowered | Luau closure: `fn() -> ()` |
| `effect:on-cleanup` | Element is removed from the tree | Same |
| `effect:on-update` | Any signal the closure reads changes | Same |
| `effect:on-prop-change` | A specific prop changes | `fn(old, new) -> ()` |

```prui
<container
    effect:on-mount="\fn() prism.signals:fire(self_id, 'opened', {}) end"
    effect:on-cleanup="\fn() prism.signals:fire(self_id, 'closed', {}) end">
  …
</container>
```

A pseudo-binding `self_id` is auto-injected into the closure
scope so the handler can address the element it's attached to.

**Two interfaces, one channel.** `effect:on-mount=` (attribute,
per-element) and `prism.on_mount(fn)` (script-block, document-level
from §7.1) are dual surfaces on the same `BlockInvalidator`
lifecycle pipeline. Use the attribute when the hook is tied to a
specific element; use the script-block form when the hook is
document-level. They compose cleanly — both fire on the same tick.

**Why this is a namespace, not a special tag:** lifecycle hooks
belong *on* an element, not *between* elements. Putting them on
attributes keeps the tree shape unchanged.

**Implementation:** lowering threads the closures into the
runtime's `BlockInvalidator` lifecycle channel. The `Animator`
substrate already has `observe` / `apply` / `tick` lifecycle —
the lifecycle namespace adds a parallel `mount` / `cleanup`
channel on the same node-keyed table.

### 7.5 Pattern match — `<match>` element

PRUI today expresses discriminated-union dispatch through chained
`if` / `else-if` / `else`. Verbose. The proposal: a `<match>`
element with `<case>` children:

```prui
<match on="{event.kind}">
  <case is="click">
    <text>You clicked at ({event.x}, {event.y})</text>
  </case>
  <case is="hover">
    <text>Hovering since {event.since}ms</text>
  </case>
  <case is="key" if="{event.key == 'Escape'}">
    <text>Escape pressed</text>
  </case>
  <case default>
    <text>Unknown event {event.kind}</text>
  </case>
</match>
```

Two `case` forms: `is="literal"` matches structural equality; an
extra `if=` clause narrows further. `<case default>` is the
catch-all (must be last). Falls through to the existing
`if` / `else-if` lowering — `<match>` is parser sugar.

**Why this beats `if`/`else-if`:** the matched expression is
written once, cases are visually scannable, and the parser
warns on unreachable / non-exhaustive matches when the matched
value is a tagged union from a Luau-typed scope binding (§7.10).

### 7.6 Async + suspense — `<suspense>` and `<fallback>`

Reactive computations sometimes need IO — fetching from
`prism.objects`, calling a federated relay, hitting a CRDT
backfill. The Luau side already has the coroutine async model
(`luau-integration-plan.md` Phase 3). The PRUI side gains a
`<suspense>` boundary:

```prui
<suspense>
  <fallback>
    <text style:color="{tokens.colors.text_secondary}">Loading tasks…</text>
  </fallback>
  <container
      for="task in async_tasks"
      effect:on-mount="\fn() async_tasks = prism.objects:query_async({...}) end">
    <shell.task-row props="{task}"/>
  </container>
</suspense>
```

Inside `<suspense>`, any read of a binding whose value is a
**pending coroutine** swaps the subtree for the `<fallback>`
body until the coroutine resolves. The pending state is a tagged
value (`{ tag = "Pending", co = <coroutine> }`) — once resolved
the binding writes through, the suspense boundary re-evaluates,
and the real subtree renders.

**Boundary scope:** suspense is opt-in per subtree. Outside a
`<suspense>` the existing behaviour holds (reading a pending
value evaluates to its default).

### 7.7 Macro tags — define-your-own elements in Luau

Today a new tag is one row in a `BlockSpec` (Rust) or one
`PrismUiSpec` (a `.prui` component declared in
`SHELL_PRISM_UI_COMPONENTS`). The proposal: a third path —
**Luau macros** that take attributes and children and return a
PRUI AST node.

```luau
-- macros/empty-state.luau
return prism.macro("empty-state", function(attrs, children)
  return prui [[
    <container direction="column" gap="8" padding="32"
               style:background="{tokens.colors.surface}">
      <image src="{attrs.icon or 'icon:inbox'}" width="48" height="48"/>
      <text font-size="16">{attrs.title}</text>
      <text font-size="14" style:color="{tokens.colors.text_secondary}">
        {attrs.subtitle}
      </text>
      <fragment>{children}</fragment>
    </container>
  ]]
end)
```

Used in any `.prui`:

```prui
<empty-state title="No tasks yet" subtitle="Create one to get started"
             icon="icon:plus">
  <button on:click="cmd task.new">Create task</button>
</empty-state>
```

**Distinction vs. `prism.widget`:** widgets ship with schemas,
signals, variants, data-queries — they're heavyweight. A macro
is *just* a transformation: attrs+children → PRUI AST. Macros are
zero-cost at render time because the expansion happens at parse
time (or on first encounter, then memoised by tag name).

**Hygiene:** macro bodies see only `attrs`, `children`, and the
Luau scope from their declaring file. They can't read the caller's
PRUI scope unless an attribute passes it explicitly. This keeps
macro reasoning local — Racket-style hygiene rather than C
preprocessor copy-paste.

### 7.8 Sub-dialects — `<language>` blocks and PRUI sigils

Dialects: register a parser, get a tag. Markdown, SQL, Mermaid,
Mathematica-style notation — any text body that parses to a PRUI
node tree.

```luau
-- dialects/markdown.luau
prism.dialect {
  name = "markdown",
  parse = function(source)
    -- Author's job: return a PRUI AST. We expose `prui_ast.*`
    -- constructors so the dialect doesn't have to serialise+reparse.
    return prui_ast.container({
      direction = "column",
      gap = 8,
      children = parse_markdown_to_ast(source),
    })
  end,
}
```

Then any `.prui` file uses it:

```prui
<markdown>
  # Welcome

  This is **rendered** as PRUI nodes — not HTML.
  - bullets
  - become `<text>` nodes with leading bullets
</markdown>
```

For inline use, Rebol-style sigils:

```prui
<container>
  ~md{**bold** and _italic_ inline}
  ~sql{select * from tasks where status = 'open'}
</container>
```

A sigil `~name{ … }` is parser sugar for
`<language name="name">…</language>`. The body is a raw run with
balanced braces (escapable with `\{`).

**Why this is huge:** every dialect Prism wants to support
becomes a *Luau file*, not a parser fork. ADR-shaped: keep one
core PRUI parser, let the ecosystem add languages.

### 7.9 PRSS × Luau — computed stylesheets

PRSS today is pure TOML — bounded, validated, no computation.
The proposal: an opt-in TOML extension where any value may be a
`{lua = "…"}` table whose body is a Luau expression evaluated
once at stylesheet load.

```toml
[class.btn-primary]
background = { lua = "tokens.colors.accent" }
radius     = { lua = "tokens.radius.md" }
padding    = { lua = "tokens.spacing.sm * 2" }

[class.btn-primary:hovered]
background = { lua = "darken(tokens.colors.accent, 0.1)" }
```

Why this beats inline `style:`: PRSS keeps the *single seam* for
theme variation. Inline lua in PRSS still parses through TOML
(no new file format), still resolves through the Luau host (no
new evaluator), and still caches through the existing
fingerprint pipeline (Luau body hashed alongside the rest of the
rule).

**Reactivity:** a `{lua = …}` body's reads of `tokens` /
`prism.state` subscribe the stylesheet to those signals. A token
override at runtime invalidates only the rules that depend on
it — fingerprint cache stays warm for the rest.

### 7.10 Schema-driven typed scopes

Today PRUI bindings are loosely typed (`ExprValue`). LuaLS
already understands the generated `.d.luau` stubs
(`luau-integration-plan.md` Phase 5). The proposal: `.prui`
files can *declare their scope's type* in the `<script>` block,
and the rest of the file gets typed completions:

```prui
<script lang="luau">
  ---@type Task
  local task = prism.scope.task

  ---@type Task[]
  local children = prism.objects:query({
    filters = {{ field = "parent", op = "eq", value = task.id }},
  })
</script>

<container>
  <text>{task.title}</text>       <!-- LSP knows .title is string -->
  <text>{task.foo}</text>          <!-- error: Task has no field 'foo' -->
  <container for="c in children">
    <text>{c.title}</text>         <!-- c is Task, inferred -->
  </container>
</container>
```

The PRUI LSP layer reads the script block's Luau annotations and
threads them into the expression-completion context. The actual
Luau is type-checked by LuaLS in-place; PRUI inherits the same
diagnostics for the slots it borrows from.

This unifies the two type stories — no parallel inference
engine. Whatever LuaLS knows about a Luau symbol, PRUI sees too.

### 7.11 Live probes — `probe:` namespace

Inline observability without printf-debugging. The `probe:`
attribute namespace attaches a named tap that streams to the
Inspector / DevTools panel:

```prui
<container
    probe:layout="card_layout"
    probe:render-count="card_render_count">
  …
</container>
```

The runtime fires probe events on the matching lifecycle hooks
(`layout` → after Taffy; `render-count` → each lower invocation).
A Luau handler in `<script>` can listen:

```luau
prism.probes:on("card_layout", function(probe)
  if probe.computed_height > 800 then
    print("card overflow at", probe.computed_height)
  end
end)
```

The probe stream is also addressable by the dev-tools panel
(filter, replay, snapshot). Removes the
"add a `data-foo="{...}"` to see what the value is" workflow
that always leaves debug attrs in production.

### 7.12 Animations as data — Luau-driven `at:` namespace

Today `transition:opacity="200ms"` interpolates one prop linearly.
The proposal: an `at:<duration>` namespace that declares keyframe
*states*, with Luau as the easing/interpolation backplane:

```prui
<container
    at:0s="{ opacity = 0, scale = 0.9 }"
    at:200ms="{ opacity = 1, scale = 1.0 }"
    at:exit="{ opacity = 0, scale = 0.95 }"
    transition:easing="\fn(t) return 1 - (1 - t)^3 end">
  …
</container>
```

The `at:` namespace evaluates each value at the matching
timeline point; the animator interpolates between them using the
optional Luau easing closure (or the default cubic).
`at:exit="…"` fires when the element unmounts and delays the
unmount until the transition completes (Vue's `v-leave` shape).

**Why this is fusion-shaped:** the interpolator *is* a Luau
function. Want a spring? Pull in a Luau spring helper. Want
elastic, back, bounce, custom? Same.

---

## 8. Composition example — a complete card

Showing all the pieces in one file. This is the readability
litmus test: can a Prism author skim this and follow what's
happening on first sight?

```prui
<!-- widgets/task-card.prui -->
<script lang="luau">
  ---@type Task
  local task = prism.scope.task

  local state = prism.state {
    expanded = false,
  }

  local children = prism.derive(function()
    return prism.objects:query({
      filters = {{ field = "parent", op = "eq", value = task.id }},
    })
  end)

  local function priority_class(p)
    if p == "high"   then return "tone-danger"  end
    if p == "medium" then return "tone-warn"    end
    return "tone-neutral"
  end

  prism.on_mount(function()
    prism.probes:emit("task_card_mounted", { id = task.id })
  end)
</script>

<container class="card {priority_class(task.priority)}" gap="8"
           effect:on-update="\fn() prism.audit:log('viewed', task.id) end">

  <container direction="row" gap="8">
    <text font-size="16">{task.title}</text>
    <fragment if="{task.priority == 'high'}">
      <badge label="HIGH" tone="danger"/>
    </fragment>
  </container>

  <markdown if="{task.notes}">{task.notes}</markdown>

  <button on:click="state.expanded = !state.expanded">
    {state.expanded ? "Hide" : "Show"} {children.length} subtask(s)
  </button>

  <suspense if="{state.expanded}">
    <fallback><text>Loading…</text></fallback>
    <container for="c in children
        | filter(|c| c.status != 'archived')
        | sort_by(|c| -c.priority)">
      <task-card task="{c}"/>     <!-- self-recursive via the macro form -->
    </container>
  </suspense>

  <match on="{task.status}">
    <case is="open"><text class="badge-open">Open</text></case>
    <case is="done"><text class="badge-done">Done</text></case>
    <case default><text>{task.status}</text></case>
  </match>

</container>
```

That single file: colocated reactive state, typed scope, Luau
function literals in expression slots, sub-dialect markdown
inline, suspense for async children, pattern matching on status,
inline lifecycle effects, derived collections, recursive
component reference, named CSS classes from PRSS — every
mechanism cooperating.

Twenty-five lines of body, no lost readability.

---

## 9. Implementation plan

Phased to ship value early, with each wave independently useful.

### Wave A — Colocated `<script>` (foundation) — ✅ landed 2026-05-15
- A.1 ✅ Parser: `<script>` / `<style>` are raw-text elements
  (`is_raw_text_tag` in `prism_core::language::prism_ui::grammar`)
  — body scanned verbatim until the matching close, stashed as a
  single `Node::Text` child so Luau `{`, `<`, `&` don't get
  PRUI-interpreted.
- A.2 ✅ Per-document `mlua::Lua` behind
  `LowerScope::with_luau_scope` / `luau_scope()`
  (`prism-ui-runtime`, `luau` feature). One Lua per
  `lower_document_with_scope` call. Default (HTML/SSR) builds stay
  mlua-free — the field + pre-pass are `#[cfg(feature = "luau")]`.
- A.3 ✅ `prism_core::language::luau::top_level_locals` (full-moon
  walk) reports each top-level `local`'s names + the `local`
  keyword offset; `luau_scope::strip_top_level_locals` splices the
  keyword out so the binding lands in the chunk env, then
  `LuauScopeFrame` harvests names → functions (RegistryKey) +
  JSON snapshot. Reachable from `lookup_path_owned`.
- A.4 ✅ Baseline `prism.state` (identity), `prism.derive`
  (eager once), `prism.on_mount`/`on_update`/`on_cleanup`
  (no-op stubs — block-lifecycle wiring is a later wave).
- A.5 ✅ `try_call_owned` gained a Luau-function dispatch arm
  after the array-builtin gate: a harvested helper resolves via
  `frame.has_function` / `frame.call`, args through the existing
  `parse_call_args` path, trailing dotted chain preserved.

Integration: `prism-shell`'s `native` feature now enables
`prism-ui-runtime/luau` (mlua was already in that graph via
`prism-core/luau`), so `<script>` blocks are live in the real
shell render path (`render_tree` → `lower_document_with_scope`).
The `web` matrix stays luau-free (vendored Luau doesn't build for
`wasm32-unknown-unknown`) — scripts are inert there, matching the
relay SSR path.

Capability baseline: the per-document Lua state runs under Luau's
`lua.sandbox(true)` — stdlib + `prism`/`tokens` globals are frozen
read-only, the script's own top-level assignments still land in
the harvestable sandbox layer (design principle 4).

Deviations from the sketch above: harvesting goes through a
`prism-core` helper rather than a direct full-moon dep in
`prism-ui-runtime`; locals are surfaced by stripping the `local`
keyword (chunk-env capture) rather than a synthetic return block;
the richer `ShellHandles::install` host-handle matrix
(`prism.objects` / `prism.edges` / signals) is still deferred to a
later wave — Wave A scripts see only pure helpers + tokens.

**Unlocks:** §7.1 (script blocks), §7.4 (lifecycle namespace,
since lifecycle hooks are just `prism.on_mount` calls referenced
by name), §7.10 (typed scope — annotations in the script body
flow into LSP).

### Wave B — Function literals + pipelines — ✅ landed 2026-05-15
- B.1 ✅ `desugar_closure` (`prism-ui-runtime::luau_scope`): `|args|
  expr` → `function(args) return (expr) end`; `\fn(args) … end` →
  `function(args) … end`. Recognised in the owned-value call path,
  not the Pratt `AnyExprNode` evaluator — closures only ever appear
  as functional-builtin args here, so that's the seam that needs
  them (no `AnyExprNode::LuauClosure` variant required).
- B.2 ✅ `LuauScopeFrame::call_closure` compiles the desugared
  source lazily and memoises it by source-hash in a per-frame
  `RefCell<HashMap<u64, RegistryKey>>` — one compile per distinct
  closure regardless of array length.
- B.3 ✅ `rewrite_pipes` / `find_last_top_level_pipe`
  (`interpret.rs`, pure — no Lua): left-associative `a | f(x)` /
  `a |> f(x)` → `f(a, x)`, depth- + quote-aware, `||` excluded,
  closure bars excluded (pipe is `|>` or whitespace-flanked `|`).
  Runs at the top of `lookup_path_owned` so for-sources, attribute
  interpolations, and text bodies all get it.
- B.4 ✅ `eval_closure_builtin` adds the closure call-form to
  `filter` / `reject` / `map` / `find` / `any` / `all` / `sort_by`
  / `group_by` / `count_by`, dispatched in `try_call_owned` ahead
  of `parse_call_args` (so the literal isn't mangled). The
  field-name form (`filter(arr, "f", v)`) is untouched.

Beyond the sketch: a script-less document that uses a closure /
pipe sigil auto-provisions an empty (helpers + tokens) Luau frame
(`document_uses_luau_expr` AST scan, `||` stripped first) so
`for="t in tasks | filter(|t| …)"` works without a `<script>`
block. Closure builtins are `#[cfg(feature = "luau")]`; on the
HTML/SSR path a closure-form call resolves to nothing rather than
mis-parsing.

**Unlocks:** §7.2 (Luau expression slots).

### Wave C — Quasi-quote + macros — ✅ landed 2026-05-15
- C.1 ✅ `prui` is a Luau global installed in the base env
  (frozen by the sandbox). It's **identity over the
  long-bracket string** — `prui[[ … ]]` hands the verbatim PRUI
  source back to the host, which re-parses it through the same
  `prism_core::language::prism_ui::parse` and lowers it with
  `attrs` / `children` bound. No `VirtualNode` round-trip
  through Lua is needed for the macro path, so identity is the
  whole implementation (a Lua→Rust AST handoff would only matter
  for the deferred `prism.widget` path).
- C.2 ✅ `prism.macro(name, fn)` retains `fn` in the registry and
  records `(name, key)` via a script-time collector drained into
  `LuauScopeFrame.macros`. `lower_element`'s unknown-tag arm
  checks `frame.has_macro(tag)` **before** the host
  `TagResolver`, so a document can define/shadow element
  vocabulary locally. `expand_macro_element` resolves the call
  site's attributes to a JSON object, lowers its children in the
  caller scope, calls the macro, and re-parses + lowers the
  returned source.
- C.3 ✅ Hygiene: the macro body lowers in a fresh `LowerScope`
  carrying only document context (resolver, `tokens`, the Luau
  frame) plus the injected `attrs` + `children` — the caller's
  PRUI bindings are dropped, so `{leak}` from a caller `for=`
  cannot reach into a macro body (regression-tested). `{children}`
  / `<host-children/>` splices the caller's already-lowered
  nodes; nested macro tags expand because the Luau frame rides
  the hygienic scope.

A macro body's `class="…"` **does** resolve the call site's PRSS
sheet — `LowerScope::stylesheet_arc` re-threads the sheet `Arc`
into the hygienic scope (after the `tokens` binding so `[tokens.*]`
overrides still cascade), regression-tested.

Deviations: macros are document-scoped (registered by the
document's own `<script>` — matches the doc's "per-document
table"); cross-document macro registration is open question 6,
deferred. On the HTML/SSR path (no `luau` feature) a macro tag
falls through to the host resolver / drop-wrapper default,
exactly as an unknown tag does today.

**Unlocks:** §7.3 (quasi-quote), §7.7 (macros).

### Wave D — Pattern match + suspense — ✅ landed 2026-05-15
- D.1 ✅ `expand_match` (`interpret.rs`): a `"match"` tag arm
  rewrites `<match on="{X}">` + `<case>` children to a synthetic
  `<let name="__match_<offset>" value="{X}"/>` plus a chained
  `if`/`else-if`/`else` over `<fragment>` wrappers, lowered
  through the existing control-flow expander. `is="lit"` →
  `__match == 'lit'`; `is="{e}"` → `__match == (e)`; a case
  `if="{c}"` narrows to `(eq) and (c)`; `<case default>` →
  `else`. First-match-wins; cases after `default` are dropped as
  unreachable. The matched expr evaluates exactly once.
- D.2 ⚠️ Structural semantics only — first-match-wins +
  default-terminates are enforced; **type-driven exhaustiveness
  / unreachable warnings are deferred to Wave I** (they need the
  Luau-typed scope binding from §7.10, not yet landed). Noted as
  the one Wave D deferral.
- D.3 ✅ `expand_suspense` / `subtree_has_pending`: the first
  `<fallback>` child is the placeholder, the rest the primary
  subtree. If any expression the primary reads (interpolation,
  attribute, `for=`/`if=`, or the *root* binding of a dotted
  path) resolves to a `{ tag = "Pending" }` marker, the fallback
  renders; otherwise the primary renders (fallback stripped).
  Full coroutine scheduling + `BlockInvalidator` resume
  notification is open question 3, deferred — this is the
  lowering-time swap that makes the boundary observable today.

**Unlocks:** §7.5, §7.6.

### Wave E — Sub-dialects + sigils — ✅ landed 2026-05-15
- E.1 ✅ `prism.dialect { name = …, parse = fn }` registers a
  dialect (same script-time collector → drained
  `LuauScopeFrame.dialects` pattern as `prism.macro`).
- E.2 ✅ Grammar: `language` joins `script`/`style` as a
  raw-text tag, and `try_parse_sigil` parses the Rebol-style
  `~name{ … }` sigil (lookahead-validated so bare `~` in prose
  is untouched; balanced braces, `\{`/`\}` escapes) into a
  `<language name="name">body</language>` element. The
  `"language"` lowering arm routes the raw body through
  `frame.expand_dialect(name, body)` → re-parses + lowers the
  returned `prui[[…]]` source in the call-site scope (dialects
  are inline, so non-hygienic by design — `{tokens.*}` resolves).
  Unknown dialect / no Luau scope → renders nothing.
- E.3 ⚠️ Partially shipped: a markdown-style dialect is
  demonstrated as a regression test proving the "every dialect
  is a Luau file, returns a node tree" path. Packaging
  `markdown` / `mermaid` / `sql-view` as bundled `.luau` files
  **in `prism-builder`** (plus a richer `prui_ast.*` constructor
  table as an alternative to the `prui[[…]]` string return) is a
  separate prism-builder follow-up — the runtime mechanism (E.1
  + E.2) is complete and dialects return `prui[[…]]` source
  today.

**Unlocks:** §7.8.

### Wave F — PRSS × Luau — ✅ landed 2026-05-15
- F.1 ✅ `prism-core` PRSS parser recognises `key = { lua = "…" }`
  single-key tables (in class properties, state sub-tables, and
  token buckets) and encodes them with the
  `LUA_VALUE_SENTINEL` prefix — the IR stays a flat
  `IndexMap<String,String>`. Checked before the state-name guard
  so `background = { lua = … }` isn't read as a state.
- F.2 ✅ Runtime `prss_value_resolved` strips the sentinel and
  evaluates the expression through the same owned-value pipeline
  class bindings use (so `tokens.*` + Luau-frame helpers
  resolve), applied in `apply_prss_class` + the descendant-selector
  path, base **and** state values. Native `darken` / `lighten` /
  `alpha` / `mix` colour helpers ship so the doc's
  `darken(tokens.colors.accent, 0.1)` works without a script;
  a `<script>`-defined helper is also reachable (the PRSS→Luau
  edge), regression-tested.
- F.3 ⚠️ Deferred: per-class signal-dependency tracking /
  selective invalidation needs the reactive substrate wiring
  (same family as the A.4 reactive-`prism.state` deferral).
  Computed values evaluate correctly at apply time today, just
  not incrementally.

**Unlocks:** §7.9.

### Wave G — Probes + `at:` animations — ✅ landed 2026-05-15
- G.1 ✅ `probe:<name>="event-key"` is a new `AttributeNamespace`
  (`prism-core`), lowered to a `data-probe-<name>` semantic attr
  (same round-trip discipline as `route:` / `use:`).
- G.2 ✅ `prism.probes:on(name, fn)` subscribe API
  (collector→drain into `LuauScopeFrame.probes`);
  `frame.fire_probe(name, payload)` invokes the handler.
  Subscribe→fire→state-mutation round-trip is regression-tested
  end-to-end. Wiring the host event router to fire probes off a
  `data-probe-*` hit is the documented follow-up (same
  event-router family as `<suspense>` resume).
- G.3 ⚠️ `at:<time>="{…}"` is a new namespace lowered to
  `data-at-<time>` — author intent + keyframe data round-trip
  exactly as `transition:` / `animate:` already do today; the
  Effect-driven animator that *interpolates* multi-stop
  timelines (and Luau easing closures) is the shared animator
  follow-up the existing `transition:`/`animate:` namespaces
  also wait on.

**Unlocks:** §7.11, §7.12.

### Wave H — File model & multi-projection authoring — ◑ partial 2026-05-15
- H.1 ⚠️ Sibling-pairing (`widget.prui` ↔ `widget.prss` ↔
  `widget.luau`) needs a filesystem + the document's path. The
  string-only `interpret()` has neither; landed instead as the
  `ImportResolver` host hook (below) which the shell/relay
  drives — true convention-based sibling probing is a
  prism-shell loader follow-up on top of that hook.
- H.2 ✅ Inline `<style lang="prss">` blocks (grammar already
  raw-texts `<style>`): `collect_inline_stylesheets` →
  `prism_core::language::prss::parse` → `StyleSheet::merged_with`
  layered over any host sidecar sheet, **before** the Luau frame
  is built so a Wave-F `{lua=…}` value in an inline sheet still
  resolves. Multiple blocks layer in order (later wins).
- H.3 ✅ `<import stylesheet|script|widget|dialect [as=]/>`
  parsed; resolution delegated to the new
  `ImportResolver` trait (`LowerScope::with_import_resolver`).
  `stylesheet` merges into the document sheet; `script` /
  `dialect` feed the Luau frame. `widget` import + `as=`
  namespacing are parsed but not yet applied (documented
  follow-ups). FS / `prism://` resolution is the host's job by
  design (the runtime has no filesystem).
- H.4 ⚠️ FingerprintCache virtual-file keys — a
  `prism-ui-build` / hot-reload concern, deferred.
- H.5 ⚠️ `prism new widget` template — a `prism-cli` concern,
  deferred.

**Unlocks:** §5.3 + §5.4 (inline blocks + imports); §5.2
sibling-pairing rides the H.3 hook.

### Wave I — Type system end-to-end — ◑ partial 2026-05-15
- I.2 ✅ `prism.scope.<name>` runtime bridge:
  `LowerScope::bindings_json()` snapshots host/document bindings,
  seeded onto the sandbox-frozen `prism` table as `prism.scope`
  so a `<script>` reads host props read-only
  (`local task = prism.scope.task`); absent bindings are
  nil-safe. Regression-tested.
- I.1 / I.3 / I.4 / I.6 / I.7 ⚠️ **External-tooling, not runtime
  code.** The LSP rewrite-and-typecheck pass, `{lua=…}`
  type-checking, signal-payload narrowing, `prism lint --types`
  (`luau-analyze` integration), and the Inspector type
  annotations all require the external `luau-analyze` binary +
  the LSP host wired together. They are deliberately *not*
  faked in the runtime; the value bridges they annotate
  (`prism.scope`, the `{lua=…}` evaluator, the signal
  registrations) are landed and ready to be type-checked once
  that toolchain pass is built. `--!strict` defaulting (I.5) is
  a one-line `prism_builder::load_*` flag flip gated on I.6.

**Unlocks:** §6.2 value bridge (the typed view is external
tooling).

### Cross-cutting work (touches every wave)
- LSP: completions and diagnostics for every new construct
  (`<script>` body completion, function-literal arg
  completion, `<match>` exhaustiveness warnings, dialect-name
  completion, `effect:` attribute autocomplete, sibling-file
  detection).
- Hot-reload: each new construct must survive a fingerprint-cache
  literal-only patch. Script bodies, macro definitions, dialect
  registrations, and `<style>` blocks are all keyed
  independently so the narrowest-scoped patch wins.
- Codegen: `prism codegen luau-types` adds stubs for `prism.state`,
  `prism.derive`, `prism.macro`, `prism.dialect`, `prism.scope`,
  `prism.probes`, `prism.on_signal`, `prism.tokens`, plus
  PRSS value-type targets.

---

## 10. Open questions

1. **Where do `<script>` bindings live in the binding stack?**
   The current frame order is host_children → tokens →
   user_bindings. Where does `LuauScopeFrame` go? Probably
   between tokens and user_bindings, so a Luau helper can
   reference `tokens.*` but a route prop can still shadow a Luau
   local. Needs validation against existing files.

2. **Hot-reload semantics for `prism.state`.** Svelte preserves
   component state across HMR; React loses it. Which do we
   want? Leaning Svelte — but the implementation cost (key
   each `state` block by source position and migrate values
   across a structural change) is non-trivial.

3. **Coroutine scheduling under `<suspense>`.** The Lua state is
   per-document, but the coroutine pool is shared. A subtree
   awaiting a `prism.objects:query_async` shouldn't starve
   unrelated subtrees. Likely: per-suspense-boundary coroutine
   queue, drained on the document's render tick.

4. **Sub-dialect security.** A `<markdown>` body that contains
   `~md{<script>...</script>}` could re-invoke the PRUI parser
   with arbitrary content. Each dialect declares its own
   allowed/forbidden element set? Or sandbox at the AST level
   (whitelist tags returned)?

5. **`prui_ast.*` ergonomics.** Should the constructor table be
   the *only* way to build PRUI AST from Luau, or does the
   `prui [[ … ]]` long-bracket form fully subsume it? Probably
   the latter for ergonomics, the former for dialects that need
   programmatic emission.

6. **Cross-document macro / dialect registration.** Today a
   `.prui` file's macros are document-scoped. Do we want
   workspace-global macros (registered through `.prism.json
   scripts.macros`)? Tradeoff: portability of files vs. global
   namespace pollution. Leaning workspace-scoped with explicit
   imports.

7. **Pipeline operator vs. `|` in expressions.** `|` is used
   today only inside `for` clauses as a separator. The proposed
   pipe form conflicts iff an expression body uses bitwise OR.
   PRUI's existing expression evaluator has no bitwise operators,
   so the conflict is theoretical — but if we ever want bitwise,
   `|>` becomes mandatory and `|` falls.

---

## 11. Explicitly rejected ideas

Each of these came up while drafting. Listing them so the rationale
isn't relitigated.

- **`<script lang="javascript">`.** No. One scripting language.
  Adding JS doubles the runtime, splits the type stub story, and
  Luau is already faster and safer.

- **JSX-style children-as-function (`{children(props)}`)**. The
  named-slot mechanism (§7.2 of prui-ref) already covers the
  same use cases declaratively; adding "child as function"
  introduces a second composition path with subtly different
  semantics. Stays out.

- **PRSS that *generates* PRUI elements.** A stylesheet that
  produced tree shape (CSS `content: "…"` taken further) would
  muddy the inspector path and the fingerprint cache. PRSS
  themes a tree the PRUI author wrote; it doesn't author the
  tree itself. (This is the PRSS → PRUI edge we deliberately
  leave empty in §5.5.)

- **Macros that emit Slint or HTML directly.** Phase 6 of the
  Luau integration plan already resolved this: macros return
  *virtual nodes*, the host walks them through the existing
  `Component` registry, both `render_slint`-equivalent (femtovg
  walker) and `render_html` (semantic HTML SSR) emit from the
  same intermediate. No escape hatch to raw target source.

- **A separate `.plang` file for Luau widgets distinct from
  `.luau`.** Phase 6 of the integration plan already lands
  widget definition in plain `.luau`; introducing a separate
  extension would fragment the ecosystem.

- **First-class Luau coroutines exposed in PRUI grammar
  (`<coroutine>` element).** The async story is handled by
  `<suspense>` + `prism.objects:query_async`; exposing the
  underlying coroutine primitive at the markup layer is too
  low-level for the audience.

- **Imperative `<while>` / `<break>` (the request from §16).** No
  — and adding Luau doesn't change this. The DSL stays bounded
  declarative; imperative bodies live in `<script>` or `luau {…}`.

- **Auto-`for` over Luau iterators.** `for="i in iterator"`
  where `iterator` is a Lua function with `__call` semantics.
  Slippery slope to non-terminating render walks. The author
  must materialise the iterable to a value first (`local rows =
  collect(iter)`), so the runtime can size the iteration up
  front.

---

## 12. What this unlocks (compared to today)

A by-numbers comparison of authoring effort, for the kinds of
things Prism users actually want to build.

| Task today | LOC | LOC with this fusion |
|---|---|---|
| Inline filter over a list (host pre-computes filtered array, binds as prop) | ~10 lines split across Rust + DSL | 1 line: `for="t in tasks \| filter(\|t\| t.open)"` |
| Conditional ARIA / data attr (already idiomatic via ternary) | 1 line | 1 line (unchanged) |
| Lifecycle hook to fire a signal on mount | 1 modifier file (~20 LOC) | 1 attr: `effect:on-mount="\fn() … end"` |
| Discriminated-union dispatch with 4 cases | 8 lines of `if`/`else-if` | 6 lines of `<match><case>` (and exhaustiveness checked) |
| Async query with loading state | Hand-write a coroutine wrapper + bind result | 4 lines: `<suspense>` + `<fallback>` + `prism.objects:query_async` |
| New "empty state" tag used across the codebase | Author a Rust `Block` or a `.prui` component spec | 8 lines of `prism.macro` |
| Markdown rendered inline | Pre-render to HTML or hand-build a parser | `<markdown>…</markdown>` after one dialect registration |
| Themed class with token-derived hover state | PRSS class + manual hover variant | 4 lines of PRSS using `{ lua = … }` derive |
| Live "what's this value during render" check | Add a `data-foo` attr, read in dev-tools | `probe:value="key"` + inspector panel |
| Spring-animated mount | None — currently linear-only | `at:0s="…" at:200ms="…" transition:easing="\fn(t) spring(t) end"` |

The pattern: every cell where "1 modifier file" / "1 Rust block"
/ "host pre-compute" was the answer collapses to inline DSL.
Authoring stays in one file, the type story stays unified, and
the runtime stays bounded.

---

## 13. Closing thought

PRUI is a good language. PRSS is a good language. Luau is a good
language. They're already in the same codebase, with the same
type stubs, the same trust model, and the same hot-reload
pipeline. The interface between them today is *narrow* — `luau
{ … }` action bodies, `class="…"` lookups, `use:` modifiers,
`FacetKind::Script`. A handful of syntaxes for a handful of
boundaries, with the three languages otherwise sealed off from
each other.

Widening those interfaces — sibling-paired files, inline
`<script>` and `<style>` blocks, `<import>` for explicit reuse,
function literals, `prui [[ … ]]` quasi-quotes, `{ lua = "…" }`
PRSS values, lifecycle attrs, macro tags, dialects, and Luau
types threading **through every seam** — turns "three languages
bolted together" into "one language with three projections." HTML
already proved the shape works at planetary scale: inline-style
demos, sidecar component libraries, and everything in between,
all parsed by the same engine and addressable by the same
inspector. We mirror it.

Authors don't pick between PRUI, PRSS, and Luau; they pick which
projection reads cleanest for the moment. The parser threads all
three into one AST, the type system threads them into one
inference context, the hot-reload threads them into one
fingerprint cache, and the inspector threads them into one
debugging surface. Three projections, one source, fully typed
end-to-end.

That's the super-power.
