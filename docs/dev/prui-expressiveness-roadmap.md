# PRUI/PRSS Expressiveness Roadmap

**Status:** draft RFC, 2026-05-20. Sequel to
`prui-luau-fusion.md` (Waves A–H, runtime-complete 2026-05-18).
This doc proposes **Wave J**: the component-level language features
that the fusion plan deliberately left for later — inheritance,
contracts, defaults, richer composition, and a substantially more
expressive PRSS surface.

The through-line is the same as the fusion doc: **one file, one
type story, one bounded runtime**, and the canonical 2026-05-15
surface syntax (`§5.10` of the fusion doc) is non-negotiable. Every
example here uses `,`-separated attributes, `$` for actions, `{…}`
for value exprs, bare idents, postfix `as`, brace-expr PRSS.

---

## 0. TL;DR

What we have today (verified against the code, not the docs):

- **PRUI grammar** (`prism-core/src/language/prism_ui/grammar.rs`)
  accepts HTML-shaped tags with 17 attribute namespaces. Seven
  layout primitives (`<container>`, `<text>`, `<heading>`,
  `<spacer>`, `<image>`, `<input>`, plus three meta-tags `<slot>`,
  `<host-children/>`, `<component>`). No notion of inheritance,
  contracts, defaults, or required props at the markup layer.
- **PRSS** (`prism-core/src/language/prss/stylesheet.rs:111`) has
  `extends = "<parent>"` for **class** inheritance, brace-expr
  computed values (Wave F, §7.9), four colour helpers
  (`darken`/`lighten`/`alpha`/`mix` at
  `prism-ui-runtime/src/interpret.rs:4722-4748`) — all sRGB-naive.
  State variants `:hovered`/`:selected`/`:focused` are *parsed*
  but only `:hovered` *applies at runtime* on `background`/`radius`
  (`interpret.rs:5797`, `:5805`); the other two round-trip as
  `data-style-<key>-<state>` attrs and the runtime swap is
  unwritten.
- **No pseudo-states** for `:pressed`, `:disabled`, `:focus-within`,
  `:checked`, `:empty`. No variant prefixes (`dark:`, `compact:`).
  No PRSS mixins. No theme axes beyond the token table. Component
  props have no DSL-level default values, no `required` marker,
  no typed slot declarations.

What this RFC adds:

| § | Feature | Headline |
|---|---|---|
| 4.1 | Component `extends` | `<component name=Button, extends=BaseButton>` shallow inheritance with override semantics |
| 4.2 | Component contracts | `<contract name=Focusable>` typed property+callback shape, structural matching |
| 4.3 | Property declarations | `<property name=count, type=int, default=0>`; presence-of-default == optional |
| 4.4 | Typed named slots | `<slot name=header, takes={title: string}>` + caller `<header>{…}</header>` |
| 4.5 | PRSS mixins | `@mixin elevation(level) { … }` / `@include elevation(2)` |
| 4.6 | Pseudo-state expansion | `:pressed`, `:disabled`, `:focus-within`, `:empty`, `:checked` |
| 4.7 | Colour helpers v2 | OKLCH `mix`/`adjust`, channel `with(c, l=+0.1, a=0.5)`, slash-alpha `accent/50` |
| 4.8 | Variant prefixes | `class=[card, hover:elevated, dark:muted]` and `@variant` declarations in PRSS |
| 4.9 | Rename `widget` projection → `component`, then land the missing wiring + Luau parity | Two-part. **(a) Rename:** `widget` is dubious — it conflates the future `prism.widget{…}` Luau builder with the fusion `<import widget=…>` projection, and "widget" reads as a UI subclass everywhere else (toolkits) where it should mean "anything that registers as a tag". **`component`** matches `<component name=Button>` (§4.1/§4.3), the industry-standard term, and the projected `prism.component{…}` builder — one term, three sites. **(b) Wire:** `widget` is parsed (`interpret.rs:1210`) but the runtime drops it (`:1017` only handles `script`/`dialect`). Land the missing runtime so `<import component="./button.prui"/>` registers a tag *and* `<import component="./button.luau"/>` registers a Luau-defined `prism.component{…}` through the **same** projection — file extension is convention, projection is role. |

Phasing (§5) lands Wave J in five tight slices.

> **Companion Wave K (§9 below, added 2026-05-20).** Wave J is
> *additive* — richer expressiveness through inheritance,
> contracts, mixins, OKLCH. Wave K is the *subtractive* mirror:
> the codebase audit at the bottom of this doc found three
> "ideological duplications" (Facet means three things; Component
> / Widget / Prefab / Block name the same concept across four
> registration paths; `<host-children/>` and `<slot/>` overlap
> in role) and several attribute namespaces that earn nothing.
> Wave J and Wave K share the same ratchet — fewer ways to say the
> same thing — and should land together. If a Wave J feature is
> superseded by a Wave K deletion, the Wave K deletion wins
> (§9.3 on the `<component>` element is the active fork). Read
> §9 before committing to Wave J Phase 2.

> **One-way-to-do-it principle (Python).** Every file-level
> composition feature below rides the four existing `<import>`
> projections from fusion §5.4. New projections are added only
> when the role is genuinely distinct. After this Wave the surface is:
>
> | Projection | Role | Files | Alias |
> |---|---|---|---|
> | `stylesheet` | apply styles | `.prss` (or `.luau` returning a stylesheet table — future) | `as ns` (namespaced class table) |
> | `script` | reuse helpers | `.luau` | `as ns` (named module) / bare (flat-merge) |
> | `component` | register a tag | `.prui` *or* `.luau` (calling `prism.component{…}`) | `as Tag` (rename target) |
> | `contract` | parse-time shape check | `.prui` (`<contract>` decl), `.luau` (`prism.contract{…}`) | `as Name` (new in §4.2) |
> | `dialect` | extend the language | `.luau` (calling `prism.dialect{…}`) | (no alias) |
>
> **The projection is the role. The file extension is convention.**
> Same `.luau` file can be imported as `script`, `component`,
> `contract`, or `dialect` depending on which side of its API the
> consumer wants. There is one `<import>` element, one
> `ImportResolver`, one `as`-namespace mechanism, one document-level
> module cache. PRSS does **not** get its own `@import` — using
> `<import stylesheet=…>` at the host PRUI document (or inline
> `<style>`-block scope) is the one obvious way. The `widget`
> projection name is deprecated on landing; the parser accepts both
> spellings during the migration window and emits a diagnostic
> pointing at `component`. The cheapest, most
contained slice (Phase 1, §5.1) lands first and is the proof of
concept for the bigger ones.

---

## 1. Why this, why now

The fusion doc closed the *runtime* gap: scripting, macros,
dialects, lifecycle, computed PRSS, probes, animations,
multi-projection authoring. It deliberately did not touch the
*component-shape* layer because Waves A–H needed to land first
without arguing about inheritance semantics.

After three weeks of using the resulting DSL, three patterns of
code duplication account for almost every "this feels gross"
moment in `apps/*/shell.prui` and `packages/prism-shell/ui/`:

1. **Two near-identical `<container>` shells** that differ only
   in one style override. Today the only escape is a shared PRSS
   class, but classes don't compose hierarchically at the markup
   layer — you can't say "Button, but with a louder hover".
2. **Hand-rolled state variants** — `style:background:hovered=…`
   on every interactive element, repeated structurally. No
   `:pressed`, so click-feedback is a script-toggled class.
3. **Inline ternaries for derived colours** —
   `style:background={priority == 'high' ? '#…' : darken('#…', 0.1)}`
   patterns leak the same expression all over the codebase.

The features below address each of those, with the bonus that
they're the same features the rest of the declarative-UI
ecosystem (Slint, SwiftUI, Compose, Tailwind v4, Sass) settled on.

---

## 2. Design principles (carried from the fusion doc)

Reiterated, lightly. If a proposal here violates one, drop the
proposal not the principle.

- **Expressiveness over magic.** A new feature must make a real
  authoring win — collapse N lines to 1, eliminate a duplication
  class. If it only adds a synonym, drop it.
- **Keep the surface narrow.** New syntax must not overlap an
  existing form. Where two forms could do the same job
  (e.g. PRSS state variant vs PRUI `class:foo`), pick one and
  ban the other.
- **Composition over inheritance, where possible.** Inheritance
  enters only where composition can't reach (default-value
  override, layout-role preservation).
- **Structural, not nominal.** Conformance to a contract is
  property-shape, not a tagged-on `implements` keyword (mirrors
  Luau's structural types — fusion doc §6.1).
- **Static where we can, runtime where we must.** Defaults
  resolve at parse; inheritance flattens at parse;
  pseudo-state flags fire at hit-test. The render walk stays
  bounded.

---

## 3. Snapshot of current state (verified, not assumed)

Cite-grounded — each row is a real `file:line` lookup.

| Area | Current state | Cite |
|---|---|---|
| PRUI parser entry | `parse(source) -> (Document, Vec<ParseError>)` | `prism-core/src/language/prism_ui/grammar.rs:46` |
| PRUI primitives | `container`, `text`, `heading`, `spacer`, `image`, `input`, `slot`, `host-children`, `component`, `fragment`, `let`, `facet`, `teleport`, `script`, `style`, `import`, `match`, `case`, `suspense`, `fallback`, `language`, `dispatch` | grammar `ast.rs` + `interpret.rs` element tables |
| `<component>` element semantics | **alias for `<container>`** — `"container" \| "component" => { … }` in one match arm; no declaration semantics today (this is the seam Wave J §4.1/§4.3 would change, §9.3 questions whether it should) | `prism-ui-runtime/src/interpret.rs:1739` |
| `<facet>` element runtime | repeats children once per item in resolved `from=` source — sugar for `<container for="x in items">…</container>` | `prism-ui-runtime/src/interpret.rs:1894-1908` |
| `<facet>` production usage | **zero** uses across `packages/prism-shell/ui/**` and `apps/*` (grep) | — |
| `FacetComponent` builder block | `Component::lower_ui` impl reading `items` + `max_items` from `node.props`; clones template per item | `packages/prism-builder/src/facet/render.rs:24-101` |
| `BuilderDocument.facets` field | **does not exist** — the prism-builder CLAUDE.md row at `:190-194` mentioning `FacetDef` / `FacetKind` / `FacetDataSource` / `FacetTemplate` / `FacetOutput` / `FacetBinding` / `FacetLayout` is stale; grep confirms zero matches for any of those types | `prism-builder/src/document.rs:21-37` |
| `fct:` namespace runtime | lowers to `data-fct-<key>` semantic attr; no other runtime behaviour | `interpret.rs:3109`, `:3595` |
| `widget=` import projection | parsed in `collect_imports` but the only handler skips it (matches `"script" \| "dialect"` only) — **dead path** | parser at `interpret.rs:1210`; dispatch at `:1018` |
| Attribute namespaces (17) | `Bare`, `On`, `Bind`, `ControlFlow`, `Style`, `Facet`, `Signal`, `Aria`, `Data`, `Route`, `Transition`, `Use`, `Class`, `Animate`, `Probe`, `At`, `Identifier` | `prism_ui/ast.rs::AttributeNamespace` |
| PRSS class inheritance | `extends = "<parent>"` flattens with cycle detection | `prism-core/src/language/prss/stylesheet.rs:111`, `:411`, `:664` |
| PRSS state variants (parsed) | `:hovered`, `:selected`, `:focused` (suffix list) | `STATE_SUFFIXES` in `interpret.rs:5439` |
| State variants (applied at runtime) | only `:hovered` on `background`/`radius` | `interpret.rs:5797` (bg), `:5805` (radius) |
| `:selected` / `:focused` runtime | round-trip as `data-style-*` attrs, no swap | `interpret.rs:3372-3377` |
| Colour helpers | `darken`/`lighten`/`alpha`/`mix`, sRGB-linear | `interpret.rs:4710-4751` |
| PRSS computed values | `key = { luau-expr }` brace exprs | fusion `§5.10 rule 4`, `prss/stylesheet.rs:208` |
| `<import>` namespacing | postfix `as`: `<import script="./x.luau"/> as x` | fusion `§5.10 rule 5` |
| `<import>` projections (parsed) | `stylesheet` ✅ wired, `script`/`dialect` ✅ wired, **`widget` ❌ parsed but runtime drops it** | `interpret.rs:976` (stylesheet), `:1017-1030` (script/dialect), `:1210` (parser accepts widget) |
| Closures | `\|args\| expr` or `\fn(args) … end` | fusion `§7.2` |
| Property defaults | **MISSING** at DSL surface | (no parse path) |
| Component inheritance | **MISSING** | (no parse path) |
| Component contracts/interfaces | **MISSING** | — |
| Typed slots | **MISSING** — only positional `<slot name="…"/>` lookup | `interpret.rs` slot dispatch |
| PRSS mixins | **MISSING** | — |
| Variant prefixes | **MISSING** (class lists are flat) | `interpret.rs` class resolver |
| `:pressed`/`:disabled`/`:focus-within`/`:empty` | **MISSING** | — |
| OKLCH-quality colour math | **MISSING** (sRGB lerp at `:4720`) | — |
| Slash-alpha shorthand | **MISSING** | — |

The rest of this doc is structured around closing the **MISSING**
rows.

---

## 4. Feature designs

### 4.1 Component inheritance — `<component … extends=…>`

**Problem.** Two buttons identical except for one style override.

**Proposal.** Shallow, single-parent inheritance on the
`<component>` declaration, mirroring Slint's `inherits` and
PRSS's existing `extends`:

```prui
<component name=BaseButton>
  <property name=label, type=string>
  <property name=tone, type=enum<default|primary|danger>, default=default>
  <container tag=button, class=[btn, tone:{props.tone}]>
    <text>{props.label}</text>
  </container>
</component>

<component name=DangerButton, extends=BaseButton>
  <property name=tone, default=danger>
</component>
```

**Semantics.**

1. **Children of the parent become the rendered body.** The child's
   `<component>` body is a *patch*, not a replacement — only
   `<property>` declarations (§4.3), `<slot>` declarations
   (§4.4), and a single optional `<style>` block are allowed
   inside an `extends`-ing component. If the child wants a
   structurally different body, it should compose with `<BaseButton>`
   as a child element instead — *that's the composition path*,
   and we keep both available for the cases where each wins.
2. **Property defaults override.** Same-named `<property>` in the
   child changes only the default; the type is fixed by the
   parent declaration.
3. **No diamond.** Single parent. Multi-axis variation goes
   through props (`tone`, `size`) or contracts (§4.2), never
   multiple inheritance.
4. **Parse-time flattening.** The interpreter resolves the parent
   chain at parse and walks the flattened form at render —
   identical perf to today.

**Rejected.** Multi-inheritance, mixin-in-class-position, deep
chains. The fusion-doc anti-pattern review (§10 of the survey)
identified fragile-base-class as the single biggest UI inheritance
footgun; shallow + same-tag covers 90% of the win.

---

### 4.2 Component contracts — `<contract name=…>`

**Problem.** "This slot accepts anything `Focusable`" has no
language-level expression today.

**Proposal.** Declare contracts as named property+callback shapes,
matched structurally:

```prui
<contract name=Focusable>
  <property name=focused, type=bool>
  <callback name=focus>
  <callback name=blur>
</contract>

<component name=FormField>
  <slot name=control, accepts=Focusable>
</component>
```

**Semantics.**

1. **Structural conformance.** A component conforms to `Focusable`
   if its declared `<property>`s and `<callback>`s are a superset
   of the contract's. No `implements=` annotation — the contract
   is matched, not declared.
2. **Parse-time check, no runtime cost.** A `<slot accepts=X>`
   that receives a non-conforming child fails at lower-time
   with a contribution error. The runtime walk doesn't change.
3. **Contracts live in their own files.** `<import contract="./focusable.prui"/> as Focusable`
   uses the existing import projection. (`script`/`stylesheet`
   are projection keywords today — `contract` joins them as a
   new projection in the `<import>` table.)

**Rejected.** Declared `implements=Focusable` annotation. Mirroring
Slint's structural-on-properties approach is enough; the
annotation would only restate what the props already say.

---

### 4.3 Property declarations — `<property>` + defaults + required

**Problem.** Today's component prop story is "anything goes;
the host binding decides". This means defaults live in the host,
required-ness is unenforced, and type-checks rely on the external
`luau-analyze` pipeline (Wave I).

**Proposal.** First-class declarations inside `<component>`:

```prui
<component name=Card>
  <property name=title,    type=string, required>
  <property name=subtitle, type=string, default="">
  <property name=padding,  type=int,    default=16>
  <property name=onClick,  type=action>
  <container padding={props.padding}, on:click=$props.onClick()>
    <heading level=3>{props.title}</heading>
    <text if={props.subtitle != ""}>{props.subtitle}</text>
  </container>
</component>
```

**Semantics.**

1. **`type=`** is one of: `string`, `int`, `number`, `bool`,
   `color`, `length`, `action`, `enum<a|b|c>`, `array<T>`,
   `object<{…}>`, or a `<contract>` name (§4.2). The set is
   closed so the type-check path stays static. Luau-side, the
   types map to the existing `prism-core::language::luau` stub
   set.
2. **`default=` is a literal**, not a `{…}` expression. Defaults
   that depend on other props are computed in the body, not at
   the declaration. (Avoids the recursive-default class of bugs
   from Vue's `withDefaults`.) The literal form mirrors the
   canonical bare-token / quoted-string value forms (fusion
   §5.10 rule 3).
3. **`required`** is a boolean presence flag (fusion §5.10 rule 3
   "boolean: name, no `=`"). Required props with no incoming
   binding raise a contribution error at lower-time.
4. **Optional props == default-bearing.** No need for a separate
   `optional` keyword; the presence/absence of `default=` does
   the work.

**Why a literal default and not an expression.** A `default={expr}`
opens a recursion: the expression scope must include the other
props, which haven't resolved yet. Slint's `property <int> count: 0;`
literal-only form is the proven shape. If a derived default is
needed, the component body uses a ternary on the incoming value:
`<container padding={props.padding ?? row.depth * 4}>`.

---

### 4.4 Typed named slots — `<slot name=…, takes={…}>`

**Problem.** Today slots are positional or named string keys with
no contract on what props are passed *into* the slot body (the
"render-prop" / Svelte 5 `Snippet<[Args]>` pattern). A
`<list>` component that wants to let the caller render each row
has no language-level surface; the caller has to know the prop
names by convention.

**Proposal.** Slots declare an `accepts=<Contract>` (§4.2)
*or* a `takes={…}` schema (when the slot body needs scope-pass
rather than contract-match):

```prui
<component name=List>
  <property name=items, type=array<object>, required>
  <slot name=row, takes={item: object, index: int}>

  <container direction=column>
    <fragment for={item, i in props.items}>
      <slot use=row, with={item, index=i}/>
    </fragment>
  </container>
</component>

<!-- caller -->
<List items={tasks}>
  <row {item, index}>
    <text>{index + 1}. {item.title}</text>
  </row>
</List>
```

**Semantics.**

1. **Declaration site.** `<slot name=…, takes={…}>` inside a
   `<component>` declares the slot's name and the typed scope
   it passes to its body.
2. **Usage site (provider).** `<slot use=…, with={…}/>`
   *invokes* the slot at the render point, supplying the scope.
3. **Caller site (consumer).** A child element matching the slot
   name (e.g. `<row {item, index}>…</row>`) provides the body.
   The `{item, index}` destructure-list is a *binding form*, not
   a value expression — it names the locals available inside
   the body.
4. **Default slot.** Unnamed children stream into the implicit
   `children` slot (today's behaviour). Backwards-compatible.

**Why `<row …>` instead of `<template slot="row">`.** The caller
form should read as if it were declaring a sub-element; the
Svelte 5 `{#snippet row(args)}` form is the closest precedent.
Vue's `<template #row>` and the older `<template v-slot:row>`
are both heavier than they need to be in a markup language
where `<row>` itself is unambiguous (any user tag in the
calling-site scope is either a registered component or a
slot-name match — slot-name resolution wins, and a
diagnostic fires on collision).

---

### 4.5 PRSS mixins — `@mixin` and `@include`

**Problem.** Multi-property reusable style fragments (an
"elevation 2 card" = padding + radius + shadow + bg) have no
language surface today. Either you compose classes
(`class=[card, padded, rounded, elevated-2]`) — Tailwind class-soup
— or you copy properties. PRSS already has class `extends`, but
that's a 1:1 chain; mixins are N:M.

**Proposal.** SCSS-flavoured `@mixin` + `@include`, with
brace-expr params (consistent with the existing PRSS computed-value
syntax):

```prss
@mixin elevation(level) {
  radius = 8
  background = { tokens.colors.surface }
  padding = 16
  # shadows — once <shadow> primitive lands; today omitted
}

[class.card]
@include elevation(2)
background = { tokens.colors.surface-bright }   # overrides
```

**Semantics.**

1. **Mixins are not classes** — they don't appear in selector
   space, can't be `extend`ed, and don't carry state variants.
   They're textual reuse (the same insight as SCSS @mixin vs
   @extend; the survey called this out).
2. **Parameters are PRSS values.** Bare tokens, quoted strings,
   or brace-exprs evaluated in the mixin invocation scope. Same
   six value forms (fusion §5.10 rule 3).
3. **Mixins can `@include` other mixins.** Cycle detection mirrors
   the existing `validate_extends_chains` (`stylesheet.rs:664`).
4. **Override semantics.** Properties declared after `@include`
   in the host class override the mixin's. (Same direction as
   CSS later-wins; mixins are applied first, host class second.)
5. **`@include` inside state blocks.** Yes —
   `[class.btn:hovered] @include elevation(3)` is valid and
   applies only when the state matches.
6. **Mixin libraries cross files via `<import stylesheet=…>`.**
   An imported PRSS sheet's `@mixin` declarations merge into the
   document's mixin table by name; namespaced imports
   (`<import stylesheet="./elev.prss"/> as elev`) keep them under
   `@include elev.elevation(2)`. **There is no PRSS `@import`** —
   the host `<import>` is the one obvious way; mixins are merely
   the *contents* the projection carries.

---

### 4.6 Pseudo-state expansion

**Problem.** Today's runtime tracks `:hovered` and applies it
on `background`/`radius` only. `:pressed`, `:disabled`,
`:focus-within`, `:empty`, `:checked` are useful and absent.
`:selected` and `:focused` are parsed but not applied.

**Proposal.**

1. **`STATE_SUFFIXES` grows** to `["hovered", "pressed",
   "focused", "focus-within", "selected", "disabled", "empty",
   "checked"]` (`interpret.rs:5439`).
2. **Every state targets the same property whitelist:**
   `background`, `radius`, `color`, `padding`, `gap`, `width`,
   `height`, `tint`. (Today only `background`/`radius`; expanding
   to colour and spacing is the next ergonomic cliff.)
3. **State-flag wiring.** The shell event router writes per-node
   bool flags (`is_pressed`, `is_disabled`, `is_focused`) into
   the existing `Surface::hovered_id`-style state table. The
   `apply_container_attributes` pass already has the override
   merging primitive (`HoverOverrides`); adding `PressedOverrides`,
   `FocusedOverrides`, `DisabledOverrides` is mechanical.
4. **State precedence.** When multiple states match, last-write
   wins in the declared order: `hovered < focused < pressed <
   selected < disabled`. (Disabled always wins so a greyed-out
   button doesn't visually press.)
5. **Surface API.**
   ```prui
   <container
     style:background=accent,
     style:background:hovered={lighten(accent, 0.1)},
     style:background:pressed={darken(accent, 0.1)},
     style:background:disabled=mute>
   ```
   and the PRSS equivalent:
   ```prss
   [class.btn] background = { accent }
   [class.btn:hovered] background = { lighten(accent, 0.1) }
   [class.btn:pressed] background = { darken(accent, 0.1) }
   [class.btn:disabled] background = mute
   ```

---

### 4.7 Colour helpers v2 — OKLCH + slash-alpha

**Problem.** Current helpers (`interpret.rs:4720`) lerp in sRGB,
which means `darken('#3b82f6', 0.3)` produces a desaturated grey-blue
rather than a darker-but-still-vivid blue. The survey's headline
recommendation (CSS Color Level 5, OKLCH `color-mix`) is the
2026 state of the art.

**Proposal.**

1. **Keep `darken`/`lighten`/`alpha`/`mix` names; switch their
   implementation to OKLCH** under the hood. sRGB hex in, sRGB
   hex out, but the lerp/adjust happens in OKLCH. No surface
   change; visual output gets dramatically better. (`interpret.rs:4710-4751`
   is ~40 lines; the rewrite is contained.)
2. **Add `with(c, l=…, c=…, h=…, a=…)`** for channel adjustments:
   ```prui
   style:background={with(tokens.colors.accent, l=+0.05, c=-0.02)}
   ```
   Positive numbers prefixed with `+` *add*; bare numbers *set
   absolute*. Same `as_f` evaluator as the existing helpers.
3. **Add `saturate`/`desaturate`** as shortcuts (`with(c, c=±t)`).
4. **Slash-alpha shorthand in attribute position.** A bare colour
   token followed by `/N` (where `N` is 0–100) sets alpha:
   ```prui
   <container style:background=accent/50>
   ```
   is sugar for `style:background={alpha(tokens.colors.accent, 0.5)}`.
   Parses at the attribute-value scanner — identical-shape with
   how `padding=12 16` already accepts multi-token bare values.
5. **Hex-with-alpha unchanged.** `#3b82f680` continues to work.

**Why OKLCH.** OKLCH is perceptually uniform, lerping in it
preserves saturation and hue under `darken`/`lighten`, and CSS
itself adopted it. Switching the *implementation* is invisible
to callers but removes the "darken makes my blue go grey"
trap. The implementation reuses an existing `palette` Rust crate
(it ships an OKLCH module); no math from scratch.

**Rejected.** A zoo of named functions (`shade`, `tint`,
`tone`, `complement`, …). One `with` + one `mix` covers every
ergonomic case the survey identified, and the CSS Level 5
working group walked away from the SCSS `darken` family for the
same reason.

---

### 4.8 Variant prefixes in class lists — `hover:`, `dark:`, custom

**Problem.** Tailwind's `class="bg-blue-500 hover:bg-blue-700 dark:bg-blue-800"`
collapses three state-aware overrides into one attribute. Today
PRUI splits this across PRSS state variants (`:hovered` in PRSS)
and the explicit `style:background:hovered=…` form. The first is
verbose for one-off overrides; the second balloons attribute count.

**Proposal.**

1. **Class lists accept prefixed entries:**
   ```prui
   <container class=[card, hover:elevated, dark:muted, pressed:sunken]>
   ```
   `hover:elevated` means "apply `.elevated`'s declarations only
   when `:hovered`". Implementation: lower to the same
   per-state override merging today's PRSS `[class.elevated:hovered]`
   form uses.
2. **Built-in variant prefixes:** `hover`, `focus`, `focus-within`,
   `pressed`, `disabled`, `selected`, `empty`, `checked`, `dark`,
   `light`, `compact`. (The pseudo-state set from §4.6 plus the
   theme axes from below.)
3. **User-defined variants in PRSS:**
   ```prss
   @variant compact = data-density="compact"
   @variant reduced-motion = prefers-reduced-motion
   ```
   The right-hand side is a *match expression* against
   document/environment state. `data-density="compact"` resolves
   via the existing `data:` attribute scope; `prefers-reduced-motion`
   reads from `@environment` (new builtin scope, mirrors SwiftUI
   `@Environment(\.…)`). This lets app authors define their own
   variant axes without engine changes.
4. **Composition.** `dark:hover:elevated` chains prefixes. Order
   is conjunctive (all must match).

**Why not in PRSS class definitions only.** Variant prefixes are
most valuable when they're one-off — "this specific button gets
an extra hover effect not worth a new class". The PRSS form
(`[class.btn:hovered]`) remains the right answer for systemic
hover styles. Both forms target the same runtime override pass.

---

### 4.9 The `component` projection — file-level composition, unified

**Problem.** The fusion doc (§5.4) already designed file-level
component composition: `<import widget="./button.prui"/>` registers
a sibling-named tag. The parser collects it (`interpret.rs:1210`)
but **the runtime drops it on the floor** (`:1017` only fans out
`script` and `dialect`). On top of that, the projection name
`widget` conflates two things and reads wrong against the rest of
the codebase, where `<component name=Button>` (§4.1) is already
the term for "a reusable tag".

**Proposal — two atoms, landed together.**

**(a) Rename the projection.** `widget` → `component`. The parser
accepts both spellings for one release window and emits a
diagnostic on `widget=` pointing at `component=`. After the
window, only `component` parses. The doc-only `prism.widget{…}`
Luau builder mentioned in fusion §5.5 / §7.7 — never implemented
— lands as `prism.component{…}` directly.

**(b) Wire the runtime + Luau parity.** `<import component=…>`
resolves through the same `ImportResolver` as the three already-wired
projections, with file-extension-driven dispatch:

```prui
<import component="./button.prui"/>           <!-- sibling-named tag <button/>     -->
<import component="./fancy.prui"/> as card    <!-- alias as <card/>                -->
<import component="./icon.luau"/>             <!-- Luau-defined component          -->
<import component="prism://lib/avatar.prui"/> <!-- workspace-shared, anchored root -->
```

- `.prui` source → parse + lower against the host scope, register
  the document's outer element as a tag-resolver entry.
- `.luau` source → evaluate; the file's `return prism.component{…}`
  expression yields the registration record. The Luau-frame
  module cache (fusion §5.9 tier 3) keys on the resolved absolute
  path so a component imported from N call sites evaluates once.
- The resolved tag goes through the **existing `TagResolver`**
  (`prism-shell` `ShellComponentRegistry`'s seam) — no parallel
  registration path. A `.prui`-defined card and a `.luau`-defined
  card are indistinguishable at use-site.

**(c) Luau-side surface.** A `.luau` file that wants to author a
component returns:

```luau
-- icon.luau
return prism.component {
  -- typed property declarations (mirror PRUI §4.3)
  props = {
    glyph = { type = "string", required = true },
    size  = { type = "int",    default = 16 },
    tint  = { type = "color",  default = nil },
  },
  -- the body. Build with prui [[ … ]] or the prui_ast.* constructors.
  render = function(props)
    return prui [[
      <image src={"icon:" .. props.glyph}, width={props.size}, height={props.size},
             style:tint={props.tint}/>
    ]]
  end,
}
```

The shape mirrors PRUI's `<component name=… ><property …>…</component>`
1:1 — the only difference is the authoring surface (markup vs
function). Both compile to the same `Block`-like record the
`TagResolver` walks at lower-time. A `.prui` file with sibling
`.luau` (fusion §5.2 convention pairing) still works: the
sibling's flat-merged locals are in scope inside the `.prui`
body, just as today. The `component` projection covers the
*explicit* multi-file case where the convention pairing isn't
enough (cross-directory reuse, library distribution).

**Semantics that need to land for this to be honest:**

1. **Module identity.** Same as fusion §5.9 cross-tier invariant:
   resolved absolute path is the key. `<import component="./button.prui"/>`
   from two call sites in a document does **not** parse the file
   twice.
2. **Tag-name binding.** Without `as`, the registered tag name is
   the file stem (`button.prui` → `<button/>`). With `as`, the
   given alias wins. Tag-name collisions between two imports
   raise a contribution error pointing at both call sites.
3. **No circular components.** A `.prui` cannot import a `.prui`
   that transitively imports it back. Tag-level recursion
   (a tree that nests `<card/>` inside `<card/>` based on data)
   works because the tag dispatch goes through the resolver, not
   through the structural import — exactly the case the fusion
   doc called out at §5.4.
4. **Hot-reload boundary.** The fingerprint cache treats an
   imported component the same way it treats a sibling: a change
   to `button.prui` marks every node that registers it as dirty.
5. **No new attributes.** The `<import>` element keeps the exact
   syntax fusion §5.10 rule 5 specified — a self-closing tag,
   one projection attribute, optional postfix `as`. Adding
   `component=…` is a new value in the existing slot, not a new
   shape.

**What this *deletes*.** The old PRSS `@import` proposal earlier
in this doc — duplicate of `<import stylesheet=…>`. The proposed
`<import component=…>` parallel path — duplicate of (renamed)
`<import widget=…>`. Both retired in favour of one projection
per role.

---

## 5. Phased rollout

Five slices. Each slice is independently shippable, tested before
the next starts. Each one's "cheapest, most-contained win" framing
keeps scope honest.

### Phase 1 — Colour helpers v2 + pseudo-state expansion (smallest, ships first)

- **Scope:** §4.7 (OKLCH math + `with()` + `saturate`/`desaturate`
  + slash-alpha) and §4.6 (expand `STATE_SUFFIXES` to include
  `pressed` + `disabled`, wire runtime flags, expand applied
  property whitelist to `color`/`padding`/`gap`).
- **Why first:** zero grammar changes; entirely
  `prism-ui-runtime` + a couple of state-tracker rows in
  `prism-shell`. The user-visible win is enormous (transparent /
  lighter / darker / "this but X" colour modifiers, plus working
  `:pressed`) for ~300 LOC of pure-Rust work.
- **Files touched:** `prism-ui-runtime/src/interpret.rs`
  (`eval_color_call`, `STATE_SUFFIXES`, override merge sites),
  `prism-shell/src/events.rs` (pressed-state writeback in the
  pointer router), `prism-core/src/language/prss/stylesheet.rs`
  (state-suffix validator).
- **Tests:** new colour-equivalence tests against known OKLCH
  reference values; pseudo-state tests in `interpret.rs`
  `#[cfg(test)]` mirrors of the existing hover tests at `:7342`.

### Phase 2 — `<property>` declarations + component `extends`

- **Scope:** §4.1 + §4.3.
- **Why second:** these change the grammar but in an
  additive way (`<property>` is a new tag; `extends=` is a new
  attribute on `<component>`). The lowering pass gains a
  flattening step before walking the tree.
- **Files touched:** `prism-core/src/language/prism_ui/ast.rs`
  (new `Node::Property`), `…/grammar.rs` (parse the new tag +
  attribute), `prism-ui-runtime/src/interpret.rs` (flatten the
  chain at parse, validate `required`, apply defaults).

### Phase 3 — Typed slots + contracts + `component` projection rename & wiring

- **Scope:** §4.2 + §4.4 + §4.9.
- **Why third:** typed slots want a way to declare slot contracts
  (§4.2), and the `component` projection lands together with
  contracts because both are file-level shape mechanisms and they
  share the `as`-namespace and resolver paths. Pulling §4.9 in
  here (vs. its own phase) keeps the import family changes in one
  reviewable diff.
- **Rename + wire ordering inside this phase:**
  1. Accept both `widget=` and `component=` in the parser, emit a
     diagnostic on `widget=`. (Source compatibility for in-flight
     code.)
  2. Implement the runtime wiring on `component=` (the slot that
     was always meant to exist, never written).
  3. Migrate the in-tree docs + examples from `widget=` to
     `component=`. (Today there are no in-tree `<import widget=…>`
     call sites; the rename is essentially free.)
  4. Drop `widget=` parser support after one release window.
- **New projection in `<import>`:** `contract="./focusable.prui"` /
  `contract="./focusable.luau"` (parallels `component`).

### Phase 4 — PRSS mixins + variant prefixes

- **Scope:** §4.5 + §4.8.
- **Why fourth:** both are PRSS-side and ship together. They
  share parser surface (`@mixin`/`@include`/`@variant` are all
  `@`-prefixed directives — `@` is currently unused at the PRSS
  top level). **There is no PRSS `@import`** — `<import stylesheet=…>`
  is the one obvious way (see §4.5 rule 6 + §4.9).

### Phase 5 — Polish + docs

- Update `prss-reference.md` and `prui-reference.md` to cover
  Wave J.
- Migration sweep over `apps/*/shell.prui` and
  `packages/prism-shell/ui/` to fold duplicated style overrides
  into mixins + variant-prefixed class lists.
- `prism-cli` lint check: warn on inline `style:` overrides that
  appear three or more times across the workspace (suggest
  promoting to a class + variant).

---

## 6. Open questions

1. **Defaults that depend on other props.** §4.3 disallows
   expression defaults to dodge recursion. Should there be a
   second tier (`<property … computed={…}>`) for derived props,
   topologically resolved? Leaning yes, in a later wave; the
   first cut keeps it simple.
2. **`<contract>` self-import.** Can a contract reference itself
   (a `Composite` slot type)? Today no. If demand surfaces,
   gate behind explicit `recursive` keyword.
3. **Variant precedence with chained prefixes.** `dark:hover:elevated`
   when `hover:muted` is also declared on the same class list —
   which wins? Proposed: latest wins, same as Tailwind. Needs
   an explicit unit test.
4. **OKLCH gamut clipping.** Out-of-gamut OKLCH values must be
   projected back to sRGB. The `palette` crate gives chroma
   reduction; we pick that over hue rotation. Document the
   choice in `prss-reference.md`.
5. **Disabled-state pointer behaviour.** Should `:disabled`
   suppress `on:click` at the dispatcher? Or only style? Lean
   suppress — but it changes existing semantics for any author
   who manually styled a disabled-looking button without expecting
   click suppression. Audit first.
6. **`component` projection — what does a `.prui` file with
   multiple top-level `<component>` declarations import as?**
   Proposed: the *file* exports its top-level component(s); a
   single top-level component is bound to the file stem (or `as`
   alias); multiple top-level components require `as Ns` and bind
   under `<Ns.foo/>` / `<Ns.bar/>` (mirrors the namespaced
   `<import script>` semantics). Open: do we ever want unnamespaced
   multi-export, or is single-component-per-file the cultural rule?
7. **`.luau` component vs `.luau` script in the same file.** Can a
   `.luau` file `return prism.component{…}` *and* also expose
   module helpers? Today's tier-2 module convention is one return
   value. Proposal: returning a `component` table that *also*
   carries `helpers = {…}` is forward-compatible; a consumer
   importing as `script` reads `helpers`, importing as `component`
   reads the component record. Same file, two valid projections —
   the projection name picks the lens. Needs prototype.

---

## 7. Explicitly rejected ideas

- **A new `<import component=…>` projection alongside `widget=`.**
  Rejected as a parallel path — `widget` is already in the fusion
  design; the right fix is to rename it (§4.9), not duplicate it.
- **PRSS `@import "elevations.prss"`.** Rejected as a parallel
  path to `<import stylesheet=…>`. PRSS sheets compose via the
  host PRUI document's import family; mixins ride in via the
  `stylesheet` projection (§4.5 rule 6).
- **Multi-inheritance / mixin-in-extends.** §4.1. Single parent.
- **Imperative `style.set(…)` API in `<script>` blocks.** PRSS +
  variant prefixes cover the use case declaratively; an imperative
  escape hatch fragments the styling story.
- **CSS-style descendant selectors `:not(:first-child)`.** PRSS
  already has multi-segment descendant keys; pseudo-selector
  *negation* doesn't compose with the brace-expr value form and
  would force a second parsing mode. The same effect lands as
  a `for` loop with `if={i > 0}` on the first child.
- **A `theme {}` block at the document root.** Token tables
  already do this; adding a second mechanism splits the design-token
  story.
- **Auto-import of `prism://core/mixins`.** Explicit `<import>`
  beats implicit globals — mirrors the fusion doc's rejection
  of `scripts=["…"]` manifest lists.
- **Distinct `prism.widget{…}` vs `prism.component{…}` Luau
  builders.** One projection, one builder. Renamed in §4.9.

---

## 8. What this unlocks

Same shape of table as fusion §12. Numbers are honest reads of
existing `.prui` files (`apps/lattice/shell.prui`,
`packages/prism-shell/ui/components/*.prui`).

| Task today | LOC | LOC after Wave J |
|---|---|---|
| "Button-but-louder-hover" component | new PRSS class + new PRUI component (~25 lines) | 4-line `<component name=Loud, extends=Button>` |
| Lighter / darker / transparent variant of a token colour | hand-tuned hex per call site | 1 expr: `accent/50`, `with(accent, l=+0.05)` |
| Pressed-state visual feedback | script-toggled class + PRSS variant (~12 lines) | 1 attr: `style:background:pressed=…` |
| Reusable elevation system across the workspace | 6 near-identical classes copy-pasted | 1 `@mixin elevation(level)` + `@import` |
| Dark-mode override on one element | duplicate class with `dark-` prefix | 1 prefix in class list: `dark:muted` |
| Form field that requires a focusable control | manual prop binding + runtime assertion | 1 attr: `<slot accepts=Focusable>` |
| Required prop with a sensible default | host binding + nullable check in body | 1 line: `<property name=label, type=string, default="">` |
| Import a `.prui` widget from a sibling directory | impossible (no runtime wiring) | 1 line: `<import component="../widgets/card.prui"/> as card` |
| Author a component imperatively from Luau | impossible (`prism.widget{…}` was never written) | `return prism.component{props=…, render=…}` in a `.luau` file, then `<import component="./card.luau"/>` |

Same pattern as Wave F→H: every cell where "two files + a runtime
check" was the answer collapses to one declarative line.

---

## 9. Wave K — Ideological consolidation (the Prefab thesis)

**Status:** sketch, added 2026-05-20 after a deep audit of the
`prism-ui-runtime` element table, the `prism-builder`
`ComponentRegistry`, and the production `.prui` corpus. Wave J
adds *new* expressive features (inheritance, contracts, defaults,
mixins, OKLCH). Wave K is the *subtractive* mirror: it deletes
mechanisms that exist today but say the same thing in three
different ways. The two waves are complementary — Wave J makes
the language *richer*, Wave K makes it *narrower*. Both must land
for the surface to feel coherent. The user's framing — "facets
and widgets are fuzzy constructs, really small parts of Prism's
old Prefab system, which is just a user-facing construct for
aggregating / composing components" — is the through-line.

### 9.1 The Prefab thesis

At the authoring layer, every named UI artifact is a
**Component** — the trait that exposes `lower_ui` to the render
walk and registers a tag with a `TagResolver`. The codebase
currently registers Components through **five** distinct entry
points; the user sees one concept, the runtime exposes five:

| Path | Source of truth | Cite |
|---|---|---|
| `BUILTINS` table | declarative `BlockSpec` rows (`SpecBlock` interprets them) | `prism-builder/src/starter.rs` — 17 built-ins (`text`/`image`/`container`/`form`/`input`/`button`/`card`/`code`/`divider`/`spacer`/`columns`/`list`/`table`/`tabs`/`accordion`/`facet`/`graph-view`) |
| `register_core_widgets` | engine-supplied `WidgetContribution` (a `TemplateNode` IR) wrapped in `CoreWidgetBlock` | `prism-builder/src/core_widget.rs` — 45+ Flux / Fitness / CRM / Calendar / etc. domain widgets |
| `PrefabComponent` | user-authored `PrefabDef` (a node subtree + `ExposedSlot` pins + variants) | `prism-builder/src/prefab.rs` |
| `LuauComponent` (`luau` feat) | `#[derive(PrismBlock)]` proc-macro that walks a `template()` function through `lower_template` | `prism-builder/src/luau_component.rs` |
| `<import widget="…">` | parsed by `collect_imports` (`interpret.rs:1210`) — **dead**, the only handler (`:1018`) matches `"script" \| "dialect"` and skips `widget` | `prism-ui-runtime/src/interpret.rs:1018,1210` |

Three live paths exist because each authoring surface
(declarative spec, IR-from-engine, document-tree-with-slots,
Luau-from-derive) has structural needs the others don't. But from
the **user's** side they are all "a thing I write once, register
under a name, and invoke with a tag." That's one concept dressed
in five costumes; one of the five is purely vestigial.

**Wave K's claim:** *Prefab*, *Widget*, *Component*, and *Block*
are not distinct categories — they are the same concept at
different authoring surfaces. The `Component` trait survives. The
five registration paths collapse into one logical authoring
*concept* (a registered tag with a body + props + slots + signals)
expressed through three *authoring surfaces* (Rust spec, `.prui`
file, Luau function). The internal "Block trait", "BlockSpec",
"CoreWidgetBlock", "PrefabComponent", "LuauComponent" become
implementation details of one path each. The user only writes
`prism.component{…}` (Luau) or a `<component>` tag (`.prui`) or a
`BlockSpec` (Rust); the registry only sees a `Component`.

This is the moment the `widget` projection rename in Wave J §4.9
becomes load-bearing: it isn't just a name cleanup, it is the
**naming surface** of the consolidation. Wave K is what makes
Wave J §4.9 honest.

### 9.2 The Facet trinity

`facet` exists in three places that conceptually share one job —
"repeat children once per item in a data source":

1. **PRUI element** `<facet name="x" from="…">…</facet>` —
   `interpret.rs:1894-1908`. The comment at `:1884-1886`
   explicitly admits the redundancy: "Sugars `<container
   for="post in state.posts">…</container>` into a dedicated tag
   that reads at the call site as data iteration rather than
   control-flow plumbing."
2. **Builder block** `FacetComponent` —
   `prism-builder/src/facet/render.rs:24-101`. A `Component`
   reading `items` + `max_items` from `node.props`, cloning the
   child template per item. Registered as a `BUILTINS` row in
   `starter.rs`.
3. **Attribute namespace** `Facet` (`fct:`) — `ast.rs:89`.
   Classifies any attribute prefixed `fct:` and lowers it to a
   `data-fct-<key>` semantic attr (`interpret.rs:3109`, `:3595`).
   Pure pass-through — no runtime behaviour beyond emitting the
   attribute string.

**Production usage**, verified by grep across
`packages/prism-shell/ui/**` and `apps/*`:

- `<facet>` element: **zero** uses
- `fct:` namespace: **zero** uses
- `FacetComponent` block: only appears in its own unit tests
  + the `BUILTINS` registration row

**`BuilderDocument` has no `facets` field.** The
`prism-builder/CLAUDE.md` row at `:190-194` (`FacetDef`,
`FacetKind`, `FacetDataSource`, `FacetTemplate`, `FacetOutput`,
`FacetBinding`, `FacetLayout`, `AggregateOp`, `ScriptLanguage`,
`FacetVariantRule`, `ResolvedFacetData`, `FacetSchema`,
`SchemaField`, `SchemaFieldKind`, `FacetRecord`,
`ValidationError`, `FACET_KIND_TAGS`, `AGGREGATE_OP_TAGS`, plus
the `apply_scalar_bindings` / `evaluate_calculations` /
`promote_inline_to_component` / `parse_filter_expr` /
`apply_aggregate` helpers) is **stale** — workspace-wide grep
returns zero matches for *any* of those types. They were deleted
in the migration noted at `prism-builder/src/facet/mod.rs:8`:
"This replaced the `FacetDef` data-model subsystem". The current
`facet/` directory holds three files (`mod.rs`, `render.rs`,
`resolve.rs`) and exports two symbols (`FacetComponent`,
`resolve_template_expressions`).

**Recommendation (Wave K.1).** Delete the `<facet>` element from
the runtime (line 1894-1908). Delete `FacetComponent` from the
builder. Delete the `Facet` attribute namespace (or fold `fct:`
into a generic `data:` pass-through). One repeater primitive:
`<container for="x in items">…</container>`. Update
`prism-builder/CLAUDE.md` to remove the dead-type catalogue.

Net change: ~150 LOC deleted, zero features lost, three concepts
collapsed to one. The same data-iteration ability is available
through `<container for="x in y">` *and* through composing a user
component named `<facet>` if the call-site phrasing matters.

### 9.3 The `<component>` element question — is it a tag or a synonym?

`interpret.rs:1739`:

```rust
"container" | "component" => { … }
```

`<component>` adds **no** semantics over `<container>` at the
runtime today. It is an *alias*, not a declaration site. Wave J
§4.1 / §4.3 propose making `<component name=…>` a real
declaration site, with `<property>` / `<slot>` children and
`extends=`. That is **Option A** below. Wave K opens two
alternatives:

- **(A) Markup component declarations.** Keep Wave J §4.1/§4.3 as
  drafted. `<component name=…, extends=…>` is the declaration
  site; the file can hold multiple. This is the Vue SFC route.
- **(B) File-as-component.** Every `.prui` file's outer element
  *is* the component body. Properties / slots / inheritance go
  into a small `<component>` head element at the top, or into a
  sibling `.luau` (convention pairing). The `<component>`
  element retires as a tag — its only legal position is the
  document head. This is the React / Svelte route — the file is
  the unit.
- **(C) Luau-as-declaration.** `prism.component{…}` (Wave J §4.9
  Luau builder) is the *sole* declaration site. `.prui` files
  are just markup imported by a Luau wrapper that returns the
  component table. The `<component>` element retires entirely.

The strongest argument for **(A)** is incrementalism — markup
inheritance is the smallest change from today, and `.prui`-first
authors don't need to learn Luau. The strongest argument for
**(B)** is that the file path already names the component, so a
top-level `<component name=Button>` restates information the
import already carries — and one tag can be deleted from the
grammar. The strongest argument for **(C)** is the "one bounded
runtime" principle from the fusion doc — every component is a
Luau table the runtime calls, period; markup is just one way to
*build* that table.

**This is a structural fork.** Wave J Phase 2 (§5.2) lands
`<component extends=…>` markup; it cannot ship before Wave K
picks A / B / C. Lean: **(B)** — it kills the worst current
duplication (the `<component>`-alias-for-`<container>` overload)
without forcing every author through Luau. But the call needs
explicit user buy-in; flagged in §10 Open questions of this doc
and in `docs/dev/clay-migration-plan.md` decision log.

### 9.4 Slot vs. host-children — pick one spelling

`<slot name="X"/>` (interpret.rs:1848-1858) and
`<host-children name="X"/>` (`:1909-1919`) cover the same use
case with slightly different precedence:

- `<slot/>` first checks `LowerScope::slots` (template-time AST
  bindings), then `host_children_by_slot`, then falls back to its
  own children.
- `<host-children/>` first checks `host_children_for_slot`, then
  the broader pre-lowered `host_children_ui`, then its own
  children.

The runtime distinguishes them because Wave 11.2 added
`<host-children/>` first and Wave 13.1 retrofitted the named-slot
map onto both. The comment at `:1865-1867` admits the equivalence:
"same semantics `<slot/>` carries."

**Production usage** (grep across `packages/prism-shell/ui/**`):

- `<slot name="X">…fallback…</slot>` — **1 file**
  (`app-window.prui`, three named slots: `menu` / `nav` / `status`)
- `<host-children/>` — **9 files** (`schema-designer`,
  `properties-panel`, `app-window` *also*, `launchpad`,
  `inspector-tree`, `dock-panel`, `builder-canvas`,
  `signals-panel`, `toast-stack`)

The current convention is *de facto* "use `<host-children/>` for
the default content slot; use `<slot name=…>` when you want
named slots with fallbacks." `<slot/>` is the more general form —
a bare `<slot/>` *is* a `<host-children/>`, and named
`<slot name="X">…fallback…</slot>` is what `<host-children>`
can't express today (host-children with named=… exists but
without inline-fallback semantics in the same readable shape).

**Recommendation (Wave K.4).** Standardise on `<slot/>` —
specifically:

- `<slot/>` (no name) ≡ today's `<host-children/>` — splice the
  caller's default children here, or own-children as fallback.
- `<slot name="X">…fallback…</slot>` — named projection with
  inline fallback (today's `<slot>` pattern, exactly).

Sunset `<host-children/>` with a one-release deprecation
diagnostic. Rewrite the 9 production files (mechanical: replace
`<host-children/>` with `<slot/>`, `<host-children name="X"/>`
with `<slot name="X"/>`). Drop the runtime arm at `:1909-1919`.
Delete `host_children_for_slot` / `host_children_ui` from the
`LowerScope` surface. Net change: one fewer special tag, zero
features lost.

### 9.5 Attribute namespace audit — load-bearing vs. sugar

The 17 namespaces (`ast.rs:74-152`) split into three tiers based
on (a) whether the runtime actually dispatches on the namespace
and (b) whether production code uses the namespace:

**Tier 1 — Load-bearing semantics (keep):**

| NS | What earns its keep | Production .prui files |
|---|---|---|
| `Bare` | direct prop assignment | every file |
| `On` (`@e`) | event handler, lowers to `Connection` | 6 files |
| `Bind` (`:p`) | two-way signal binding sugar | 0 today, but the design lands with Wave J Phase 2 |
| `ControlFlow` | `if`/`else-if`/`else`/`for` expansion | every file |
| `Style` (`style:`) | token-resolved style attribute | most files |
| `Signal` (`sig:`) | declared signal r/w | 0 today (signal authoring still flows through `<script>` blocks) |
| `Class` (`class:p`) | reactive class toggle | 0 today |
| `Identifier` (`class`, `id`) | CSS / inspector addressing | every file |

**Tier 2 — Pure sugar (collapse candidates):**

| NS | What it lowers to | Production uses | Verdict |
|---|---|---|---|
| `Aria` (`aria:`) | `aria-*` HTML pass-through | 5 files | **keep** — load-bearing for SSR / a11y |
| `Data` (`data:`) | `data-*` HTML pass-through | many files | **keep** — bare names would collide with prop assignment |
| `Route` (`route:`) | `data-<key>` — **`ast.rs:96-106` explicitly states the equivalence**: "the namespace is sugar, not a new runtime concept" | **0** | **delete**, use `data:` |
| `Facet` (`fct:`) | `data-fct-*` semantic attr | **0** | **delete** (see §9.2) |
| `Probe` (`probe:`) | `data-probe-*` + Luau `prism.probes:on` hook | 0 in .prui, but the Luau side is wired | **keep** — the runtime subscribes |

**Tier 3 — Incomplete deferred (finish or delete):**

| NS | Status | Production uses | Action |
|---|---|---|---|
| `Transition` (`transition:`) | parsed; runtime install deferred (Wave 9.4) | 0 | ship in Wave J Phase 1 *or* delete |
| `Animate` (`animate:`) | parsed; install deferred (Wave 14.6) | 0 | same — ship or delete |
| `At` (`at:`) | parsed; multi-stop animation deferred | 0 | same |
| `Use` (`use:`) | parsed; lowers to `data-use-<id>`; modifier integration pending | 0 (the lone grep hit is a doc-comment in `dock-panel.prui:4`, not a directive) | finish or delete |

**Recommendation (Wave K.2 + K.3).** Two deletions
(`Route`, `Facet`) drop the namespace count from 17 to 15 with
zero production impact. The Tier 3 audit then forces a
ship-or-cut decision per namespace: anything with zero production
uses and no scheduled implementation in Wave J Phase 1 is up for
deletion. The "ship eventually" rationale is the same drift that
left `widget=` parsed-but-dead — let it set the bar.

### 9.6 The `widget=` import projection — already on the books

Wave J §4.9 covered the rename + wire. Wave K's amendment is the
**deeper question**: given §9.3, does the projection need to
exist at all?

If the §9.3 fork lands on:

- **Option (A) (markup declarations).** Yes —
  `<import component="./button.prui"/>` is the obvious shape,
  matching the rename. Wave J §4.9 stands.
- **Option (B) (file-as-component).** Yes — same shape; the
  imported file's outer element *is* the component, the import
  registers it under the file stem or `as=` alias. Wave J §4.9
  stands.
- **Option (C) (Luau-as-declaration).** Mostly no — component
  imports flow through `<import script="./button.luau"/>` (already
  wired in §5.4) plus the `prism.component{…}` Luau builder. The
  `component` projection becomes a narrow synonym for `script`,
  and may not be worth the slot.

Either way, the dead-code wiring at
`interpret.rs:1018` must be *fixed* (Wave J §4.9 (b)) or the
parser entry at `:1210` must be *deleted*. The current state —
parsed but skipped — is the worst of both worlds: source code
that promises a behaviour and ships nothing.

### 9.7 Phasing — Wave K slices

Wave K is mostly **subtraction**, so each slice is small and
independently testable. Lowest-risk-first ordering:

| Slice | Scope | LOC delta | Reversibility |
|---|---|---|---|
| K.1 | Delete `<facet>` element + `FacetComponent` + `Facet` namespace + `BUILTINS` row; verify zero production breakage; update `prism-builder/CLAUDE.md` (drop the stale `FacetDef`/`FacetKind`/… catalogue) | ~150 deleted | reversible (git revert) |
| K.2 | Delete `Route` namespace (`route:` → diagnostics: "use `data:`"); reroute through `Data` | ~20 deleted | reversible |
| K.3 | Audit Tier 3 namespaces (`Transition` / `Animate` / `At` / `Use`) against Wave J Phase 1 ship list; delete the ones we're not shipping | ~30-100 deleted per namespace dropped | semi-reversible (would need to re-add parser + dispatch) |
| K.4 | Sunset `<host-children/>` in favour of `<slot/>` with named-slot + fallback semantics; rewrite the 9 production files; delete the runtime arm | ~80 deleted, 9 files touched | reversible during deprecation window |
| K.5 | Decide §9.3's A/B/C fork *before* Wave J Phase 2 lands `<component extends=…>`; document the decision in `clay-migration-plan.md` | n/a (decision) | one-way once Phase 2 ships |
| K.6 | Unify the five registration paths in `ComponentRegistry` behind one authoring concept with three skins (Rust `BlockSpec`, `.prui` file, Luau `prism.component{…}`); fold `CoreWidgetBlock` / `PrefabComponent` / `LuauComponent` into implementation details of one path each | ~300 reorg | requires migration of every block + every prefab |

K.1-K.4 are safe near-term wins (a week or two of careful
deletion + test pass). K.5 is the structural fork Wave J §4.1 /
§4.3 hinge on — must close before Wave J Phase 2 starts. K.6 is
the long arc; it ratchets in over several phases and is the
ultimate destination of the rename in Wave J §4.9.

### 9.8 What Wave K gives us (the subtraction table, mirror of §8)

| Today | After Wave K |
|---|---|
| Three ways to repeat children (`<facet>`, `FacetComponent`, `<container for=>`) | One: `<container for="x in items">` |
| Two ways to splice caller content (`<slot/>`, `<host-children/>`) | One: `<slot name="X">…fallback…</slot>` |
| Five paths to register a component (`BUILTINS`, `register_core_widgets`, `PrefabComponent`, `LuauComponent`, dead `<import widget>`) | One concept, three authoring skins |
| 17-entry `AttributeNamespace` enum with 4 pure-sugar variants | 12-14 entries after Tier 2 + Tier 3 cuts |
| `<component>` element silently aliases `<container>` | one of: real declaration site (A), file head only (B), retired (C) — picked, not drifted |
| `prism-builder/CLAUDE.md` lies about `FacetDef` etc. | accurate, post-deletion |
| `widget=` projection parsed-but-dead | wired and renamed (J §4.9) *or* deleted (K.6) |

The ratchet is the same as Wave J's: every cell where "two or
three mechanisms exist for the same job" collapses to one. Wave J
adds the *new* features that one mechanism needs to be enough;
Wave K performs the *deletions* that prove it.

### 9.9 Risks and open questions

1. **The `<facet>` element has zero production uses today but is
   a documented language feature.** Deleting it is a breaking
   change for downstream `.prui` authors outside this monorepo
   (if any). Mitigation: emit a parser diagnostic for one release
   pointing at the `for=` form before removing.
2. **The Tier 3 namespace audit (K.3) requires committing to or
   abandoning the deferred animation pieces.** That's a real
   product call — does Prism ship the CSS-transition / keyframe
   surface in Wave J Phase 1, or move it to a later wave? If
   "later wave", the namespaces should be ripped now and re-added
   on the day they ship — vestigial grammar attracts drift.
3. **K.6 (registration unification) interacts with the `Block`
   trait / `Component` trait split in `prism-builder`.** `Block`
   was added as "single-trait sugar" over `Component`; if every
   registered tag becomes a `Component` via one path, the `Block`
   trait either becomes the canonical name or disappears entirely.
   Either choice is a workspace-wide rename.
4. **The `<host-children/>` sunset (K.4) ripples through
   `packages/prism-shell/ui/components/*.prui`.** Nine files of
   mechanical replacement, but they're all in the same workspace,
   so the deletion + rewrite + test pass is one PR.
5. **The §9.3 fork is genuinely open.** A skim of similar systems
   (Vue SFC, Svelte, React, SwiftUI, Slint) shows every viable
   option works in the wild. The right call depends on whether
   Prism's primary author surface is markup-first (favours A or
   B) or scripting-first (favours C). The fusion doc's "one
   bounded runtime" principle leans C; the user's "everything is
   a Prefab" framing leans B; the path of least disruption is A.

---

## 10. Closing thought

Waves A–H made PRUI *runnable* — script, macro, dialect, lifecycle,
suspense, animation, multi-projection. Wave J makes it *reusable*:
inheritance for the cases composition can't reach, contracts for
the cases nominal-shape would help, defaults so authoring doesn't
reach into the host, and a PRSS surface as expressive as Tailwind
v4 + Sass + CSS Color 5 combined — all while staying in one file
format with one type story. Wave K (§9) makes it *coherent*: the
audit found five paths to register a component, three ways to
repeat children, two spellings for slot injection, and four
attribute namespaces that earn nothing. Each duplication is a
small papercut on its own; together they're the reason the DSL
feels larger than it is. The fusion doc's ratchet — every new
feature must collapse N lines to 1 — applies in both directions:
**§8 is the additive receipts (Wave J), §9.8 is the subtractive
receipts (Wave K).** The end state is a surface that's more
expressive *and* smaller than today's, which is the only kind of
language change that ages well.
