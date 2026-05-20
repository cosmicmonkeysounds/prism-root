# PRUI / PRSS Expressiveness Roadmap

**Status:** living design doc + roadmap, restructured 2026-05-20.
Sequel to `prui-luau-fusion.md` (Waves A–H, runtime-complete
2026-05-18). Earlier drafts of this doc were organized
chronologically by wave (J → +K → +L); this revision is
**concept-organized** so the same material reads as both a design
doc *and* a roadmap. Wave names survive as tags on individual
features and phases.

The destination spans three named waves:

- **Wave J — additive.** Inheritance, contracts, defaults, OKLCH
  colour math, pseudo-state expansion. Lands in the current
  HTML-shaped grammar.
- **Wave K — subtractive.** Delete mechanisms that say the same
  thing twice: facet trinity, `<host-children/>` vs `<slot/>`,
  dead `widget` projection, sugar-only namespaces.
- **Wave L — structural.** Replace the 17-namespace
  `AttributeNamespace` enum with an open *trait registry*; add
  mixins, derives, macros, capabilities, algebraic property
  types, and nested-record state variants. The structural
  rethink that makes "more expressive AND smaller" honest.

All three are necessary. §5 is the end-state vision; §6 is the
per-concept design detail; §7 is the unified phasing across all
three waves; §11 is "tier 2" musings that are interesting but
not on the immediate path.

---

## 0. TL;DR — the trajectory

| Today | End state |
|---|---|
| 17-namespace `AttributeNamespace` enum, hard-coded in the parser | Open trait registry, document-scoped, Luau-extensible |
| Five paths to register a component (`BUILTINS` / `register_core_widgets` / `PrefabComponent` / `LuauComponent` / dead `<import widget>`) | One noun ("component"), three authoring surfaces (`.prui` / Luau / Rust `BlockSpec`) |
| Three ways to repeat children (`<facet>` / `FacetComponent` / `<container for=>`) | One way (`<container for=>`) |
| Two ways to splice caller content (`<slot/>` / `<host-children/>`) | One way (`<slot/>` with optional `name=` + fallback) |
| State variants restate property × selector at each cell (N × M lines) | Nested records + pipelines + named values (N + M lines) |
| Inheritance only via PRSS `extends=`; no markup-level composition | `<component extends=…>` + mixins (`with=[…]`) + derives (`derive=[…]`) |
| Style-modifying behaviours wired by editing the runtime | User-defined attribute macros (`prism.macro{…}`) and traits (`prism.trait{…}`) |
| Host services (clipboard / network / fs) reached via magic globals | Typed `<capability>` declarations the host provides at lower-time |
| `darken`/`lighten`/`mix` lerp in sRGB → desaturated colours | OKLCH-backed (callers unchanged) + `with(c, l=±, a=…)` channel adjust + slash-alpha (`accent/50`) |
| Component props: anything goes, host decides | Declared `<property>` with types, defaults, `required`, discriminated unions |
| Two closure forms (`\|args\| expr` and `\fn(args)…end`) | One: `\|args\| expr` only (see §2, §9) |

The trajectory in three sentences:

1. **Today**'s authoring surface is the HTML-namespace-prefix
   shape with seventeen ad-hoc behaviours bolted on, five
   incomparable ways to register a component, and N × M
   restatement for state-aware styles.
2. **The end state** is the same XML-shaped grammar with a
   single open trait registry behind it, mixins + macros +
   capabilities doing the work seventeen namespaces tried to,
   one noun for components with three authoring surfaces, and
   nested records collapsing the N × M state grid to N + M.
3. **Wave J** adds the features the current grammar needs
   (additive). **Wave K** deletes the mechanisms the rethink
   obsoletes (subtractive, safe). **Wave L** performs the
   rethink. Phasing in §7 orders these so every step ships
   independently.

---

## 1. Why this — three patterns of daily pain

After three weeks of writing `.prui` files across
`apps/*/shell.prui` and `packages/prism-shell/ui/`, three
patterns account for almost every "this feels gross" moment:

1. **Near-identical components that differ in one override.**
   Today the only escape is a shared PRSS class, but classes
   don't compose hierarchically at the markup layer — there's
   no way to say "Button, but with a louder hover."
2. **Hand-rolled state variants.** `style:background:hovered=…`
   on every interactive element, repeated structurally. The
   PRSS-side `[class.btn:hovered] background = …` ladder is
   just as verbose. No `:pressed`, so click-feedback is a
   script-toggled class.
3. **Inline ternaries for derived colours.**
   `style:background={priority == 'high' ? '#…' : darken('#…', 0.1)}`
   patterns leak the same expression across the codebase.

Layered on these three are three structural problems the
codebase audit (§4) found:

4. **Five registration paths for what is conceptually one
   noun.** "Component", "Block", "Widget", "Prefab" — same
   concept, different surfaces, different file paths, scattered
   docs.
5. **Three implementations of "repeat children once per item"**
   — `<facet>` element + `FacetComponent` block + `fct:`
   attribute namespace.
6. **Two spellings of "splice caller content here"** —
   `<slot/>` and `<host-children/>`, with production code using
   mostly the latter.

Wave J fixes problems 1-3 with new features. Wave K fixes
problems 4-6 by deleting redundant mechanisms. Wave L fixes the
deeper issue underneath all six: the 17 attribute namespaces
hard-coded into the parser, which is what makes adding "a new
attribute kind" require an engine release.

---

## 2. Design principles

Carried from the fusion doc. If a proposal violates one, drop
the proposal not the principle.

- **Expressiveness over magic.** Every new feature must
  collapse N lines to 1 or eliminate a duplication class. A
  synonym is not a feature.
- **Keep the surface narrow.** New syntax must not overlap an
  existing form. Where two forms could do the same job, pick
  one and ban the other. (This is the principle Wave K enforces
  retroactively.)
- **Composition over inheritance, where possible.** Inheritance
  enters only where composition can't reach (default override,
  layout-role preservation). Mixins (§6.3) carry the rest.
- **Structural, not nominal.** Conformance to a contract or
  trait is property-shape, not a tagged-on `implements` keyword
  — mirrors Luau's structural types.
- **Static where we can, runtime where we must.** Defaults
  resolve at parse; inheritance flattens at parse; macros
  expand at parse; pseudo-state flags fire at hit-test. The
  render walk stays bounded.
- **One closure form.** `|args| expr` (Lua arrow). The fusion
  doc originally allowed a long form (`\fn(args) … end`) as an
  alternative; that's rejected — single form across the
  expressiveness stack (see §9).
- **Pay for what you use.** A `.prui` author writing a static
  page never sees the trait registry, the macro engine, the
  capability system, or the algebraic-prop matcher. Those
  surfaces are opt-in; their cost is zero when unused.

---

## 3. Terminology — what is a *component*?

This doc uses **component** (lowercase, no backticks) as the
*noun* for any reusable UI definition — what a `.prui` file
holds, what `prism.component{…}` returns from Luau, what a
`BlockSpec` row declares in Rust, what `<component name=…>`
declares inside a parent `.prui` file. **Same artifact,
different authoring surfaces.** Prism Studio shows them as
"components" in its UI; this doc keeps that convention.

### Authoring surfaces — five spellings, one noun

| You write | You call it | Where it lives at runtime |
|---|---|---|
| A `.prui` file whose outer element *is* the body | a component (file-as-component) | one `ComponentRegistry` entry; tag = file stem (or `as` alias) |
| `<component name=Foo>…</component>` inside a `.prui` file | a component (markup decl) | one `ComponentRegistry` entry; tag = `Foo` |
| `prism.component{props=…, render=…}` in `.luau` | a component (Luau decl) | one `ComponentRegistry` entry; tag = `as` alias |
| `BlockSpec` row in `starter.rs` / `prism-shell/.../registry.rs` | a component (Rust decl) | one `ComponentRegistry` entry; tag = spec id |
| `WidgetContribution` + `CoreWidgetBlock` + `register_core_widgets` | a component (engine-provided) | one `ComponentRegistry` entry |
| `PrefabDef` + `PrefabComponent` | a component (user compound) | one `ComponentRegistry` entry |

All six rows produce the *same noun*. The `ComponentRegistry`
sees a `Component` (the trait); the resolver looks up a tag and
invokes `Component::lower_ui`. Differences are *where the
author types* and *what shape the source has* — not what the
runtime registers.

### Codebase names that aren't separate concepts

| Codebase name | What it actually is |
|---|---|
| `Block` (`prism-builder/src/block.rs`) | Single-trait sugar over `Component`; blanket-impls `Component`. Ergonomics, not semantics. |
| `Widget` (`WidgetContribution`, `CoreWidgetBlock`) | The engine-supplied subset of components. Source label, not a kind. |
| `Prefab` (`prism-builder/src/prefab.rs`) | The user-authored compound subset. Source label, not a kind. |
| `BlockSpec` + `SpecBlock` | Declarative-record way to register a component in Rust without writing a trait impl. |

A `.prui` author never types "Block" or "Prefab" or "Widget" —
they write a component. The codebase names survive as
internal-source-level clarity; the authoring surface knows only
"component".

### The `<component>` PRUI tag is a *separate* construct

- **component** (lowercase, no backticks) — the *noun*. Always
  "a reusable UI definition."
- **`<component>`** (backticks + angle brackets) — the specific
  PRUI markup tag. Today it is a silent alias for `<container>`
  (`interpret.rs:1739`). §6.1 proposes giving it real
  declaration semantics; the §6.1 A/B/C fork (Q1 in §8) decides
  whether the tag survives at all.
- **`Component` trait** (`prism-builder/src/component.rs`) —
  the Rust trait every registered component impls. The
  `Component::lower_ui` method is the render contract.
- **`ComponentRegistry`** (`prism-builder/src/registry.rs`) —
  the Rust struct that holds registered components and
  dispatches tag lookups through a `TagResolver`.

### Reading rule

- Lowercase "component" without backticks → the noun.
- `<component>` in backticks → the PRUI markup tag.
- `Component` capitalised, no backticks → the Rust trait.
- "components" (plural) in narrative → the noun, plural.

---

## 4. Current state — verified snapshot

Each row is a real `file:line` lookup, not a doc claim.

### PRUI runtime

| Area | Current state | Cite |
|---|---|---|
| Parser entry | `parse(source) -> (Document, Vec<ParseError>)` | `prism-core/src/language/prism_ui/grammar.rs:46` |
| Element table | `container`, `text`, `heading`, `spacer`, `image`, `input`, `slot`, `host-children`, `component`, `fragment`, `let`, `facet`, `teleport`, `script`, `style`, `import`, `match`, `case`, `suspense`, `fallback`, `language`, `dispatch` | `interpret.rs:1738-1948` |
| `<component>` semantics | **alias for `<container>`** — one match arm: `"container" \| "component" => { … }`; no declaration semantics today | `interpret.rs:1739` |
| `<facet>` runtime | Repeats children once per item in resolved `from=` source — sugar for `<container for="x in items">` | `interpret.rs:1894-1908` |
| `<facet>` production usage | **zero** uses across `packages/prism-shell/ui/**` and `apps/*` (grep) | — |
| `<slot/>` runtime | Resolves `LowerScope::slots` → `host_children_by_slot` → own children | `interpret.rs:1848-1858` |
| `<host-children/>` runtime | Resolves `host_children_for_slot` → `host_children_ui` → own children | `interpret.rs:1909-1919` |
| `<slot>` vs `<host-children/>` production usage | `<slot name="X">` — 1 file (`app-window.prui`); `<host-children/>` — 9 files | — |
| Attribute namespaces (17) | `Bare`, `On`, `Bind`, `ControlFlow`, `Style`, `Facet`, `Signal`, `Aria`, `Data`, `Route`, `Transition`, `Use`, `Class`, `Animate`, `Probe`, `At`, `Identifier` | `prism_ui/ast.rs::AttributeNamespace:74-152` |
| `widget=` import projection | Parsed by `collect_imports` but the only handler skips it (`"script" \| "dialect"` only) — **dead path** | parser `interpret.rs:1210`; dispatch `:1018` |
| Closure form | `\|args\| expr` (Lua arrow) — fusion §7.2. **Long form `\fn(args)…end` removed in this doc** (§9). | fusion §7.2 |

### PRSS runtime

| Area | Current state | Cite |
|---|---|---|
| Class inheritance | `extends = "<parent>"` flattens with cycle detection | `prss/stylesheet.rs:111`, `:411`, `:664` |
| State variants (parsed) | `:hovered`, `:selected`, `:focused` (suffix list) | `STATE_SUFFIXES` in `interpret.rs:5439` |
| State variants (applied at runtime) | Only `:hovered` on `background`/`radius` | `interpret.rs:5797`, `:5805` |
| `:selected` / `:focused` runtime | Round-trip as `data-style-*` attrs, no swap | `interpret.rs:3372-3377` |
| Colour helpers | `darken`/`lighten`/`alpha`/`mix`, sRGB-linear (perceptually broken) | `interpret.rs:4710-4751` |
| PRSS computed values | `key = { luau-expr }` brace exprs (Wave F) | `prss/stylesheet.rs:208` |

### `<import>` projections

| Projection | Status | Cite |
|---|---|---|
| `stylesheet` | ✅ Wired | `interpret.rs:976` |
| `script` | ✅ Wired | `interpret.rs:1017-1030` |
| `dialect` | ✅ Wired | `interpret.rs:1017-1030` |
| `widget` | ❌ Parsed; runtime drops it | `interpret.rs:1210` parses, `:1018` skips |

### prism-builder ComponentRegistry — five authoring surfaces

| Path | Source | Cite |
|---|---|---|
| `BUILTINS` table | 17 declarative `BlockSpec` rows | `prism-builder/src/starter.rs` |
| `register_core_widgets` | 45+ engine `WidgetContribution` wrapped in `CoreWidgetBlock` | `prism-builder/src/core_widget.rs` |
| `PrefabComponent` | User `PrefabDef` template + slots | `prism-builder/src/prefab.rs` |
| `LuauComponent` (`luau` feat) | `#[derive(PrismBlock)]` macro | `prism-builder/src/luau_component.rs` |
| `<import widget="…">` | Parsed but dead | `interpret.rs:1018, 1210` |

### Stale documentation

`prism-builder/CLAUDE.md:190-194` lists `FacetDef`, `FacetKind`,
`FacetDataSource`, `FacetTemplate`, `FacetOutput`,
`FacetBinding`, `FacetLayout`, `AggregateOp`, `ScriptLanguage`,
`FacetVariantRule`, `ResolvedFacetData`, `FacetSchema`,
`SchemaField`, `SchemaFieldKind`, `FacetRecord`,
`ValidationError`, `FACET_KIND_TAGS`, `AGGREGATE_OP_TAGS` —
**none of those types exist** (grep returns zero matches). They
were deleted in the migration noted at
`prism-builder/src/facet/mod.rs:8`. `BuilderDocument` has **no**
`facets` field. Cleanup is part of Phase 2 (Facet trinity
deletion).

### What's missing today

| Area | Status |
|---|---|
| Component inheritance | Missing — no parse path |
| Component contracts / interfaces | Missing |
| Property declarations + defaults + `required` | Missing at DSL surface |
| Typed slots | Missing — only positional `<slot name="…"/>` lookup |
| PRSS mixins | Missing |
| Pseudo-states `:pressed` / `:disabled` / `:focus-within` / `:empty` / `:checked` | Missing |
| OKLCH-quality colour math | Missing (sRGB lerp at `:4720`) |
| Slash-alpha shorthand | Missing |
| Open trait registry | Missing — fixed 17-namespace enum |
| Mixins / derives | Missing |
| Markup macros | Missing — only whole-tree dialects |
| Capabilities | Missing — host services are magic globals |
| Algebraic property types (discriminated unions) | Missing |
| Nested state-variant records | Missing — N × M restatement is the only form |

---

## 5. End-state vision — the surface we're aiming at

What the language looks like after Waves J + K + L all land.
Each subsection shows the end-state syntax with a code block,
a brief comparison to today, and a forward link to the detailed
design in §6. Read this for the destination; read §6 for the
rationale and §7 for the order.

### 5.1 Authoring a component

Three authoring surfaces (file / markup / Luau), one noun.

```prui
<!-- ./button.prui — a file IS a component -->
<component>
  <property name=label, type=string, required>
  <property name=tone,  type=union<default, primary, danger>, default=default>

  <container with=[Pointable, Focusable], style={…}>
    <text>{props.label}</text>
  </container>
</component>
```

```luau
-- ./icon.luau — Luau-authored component
return prism.component {
  props = {
    glyph = { type = "string", required = true },
    size  = { type = "int",    default  = 16 },
  },
  render = |props| prui [[
    <image src={"icon:" .. props.glyph} width={props.size}/>
  ]],
}
```

```rust
// In starter.rs — Rust-authored component
const ICON_SPEC: BlockSpec = BlockSpec::new("icon", icon_schema)
    .lower(icon_lower)
    .help("builder.components.icon", "Icon", "…");
```

All three register as components in the same `ComponentRegistry`
under the same tag-resolution path. The `<button/>` callsite
doesn't care which surface authored the body.

**See §6.1** for declaration syntax + the A/B/C fork.
**§6.2** for property declarations. **§6.3** for inheritance,
contracts, traits, mixins, derives. **§6.11** for the import
family.

### 5.2 The attribute system — open trait registry

Today: 17 hard-coded namespaces (`bare`, `on:`, `style:`,
`aria:`, `fct:`, `route:`, …) each with bespoke runtime
dispatch. End state: every attribute is a *trait method call*
against an open registry; new traits ship as Luau or Rust
libraries without grammar surgery.

```prui
<container
  layout={direction=column, gap=12, padding=16}
  style={background=accent, radius=12}
  with=[Card, Hoverable, is-active ? Elevated : nil]
  drag={handle=".header"}
  a11y.label="Save changes"
  data.role=palette-item
  @click=$save()>
  …
</container>
```

The four attribute surface forms:

- **`bare=value`** — direct property on the element's schema.
- **`trait.method=value`** — explicit trait dispatch
  (`layout.gap=12`).
- **`group={k=v, …}`** — record-shaped trait call; groups N
  attributes per concept into one syntactic unit.
- **`with=[Mixin, …]` / `derive=[Trait, …]`** — behaviour
  composition.

Plus two sugars for the most-common cases:

- **`@event=…`** ≡ `pointer.on-event=…`
- **`:prop={…}`** ≡ `bind.prop={…}`

The grammar shrinks (one rule, not seventeen). The language
opens (any Luau-registered trait works exactly like a built-in).

**See §6.4** for the trait registry; **§6.3** for mixins and
derives.

### 5.3 State variants — nested records and pipelines

Today's N × M cross-product (one selector per (property,
state) cell) collapses to N + M nested deltas. Three layered
forms, authors pick by shape of the duplication.

**Nested records — for many properties at once:**

```prui
<container style={
  background = accent,
  radius     = 8,
  :hovered   = { background = lighten(0.1), radius = 12 },
  :pressed   = { background = darken(0.1) },
  :disabled  = { background = mute, opacity = 0.5 },
}>
```

```prss
[class.btn] {
  background = accent
  radius     = 8
  &:hovered  { background = lighten(0.1), radius = 12 }
  &:pressed  { background = darken(0.1) }
  &:disabled { background = mute, opacity = 0.5 }
}
```

PRSS uses CSS-Nesting (`&` for parent context) and PRUI uses
nested-record `style={…}` — same shape, different surfaces.
`lighten(0.1)` inside a state record defaults its base argument
to the parent record's same-named property.

**Pipeline form — for one property, several state deltas:**

```prui
<container style.background={
  accent
  | :hovered  → lighten(0.1)
  | :pressed  → darken(0.1)
  | :disabled → mute
}>
```

**Named state-responsive value — for reuse across the workspace:**

```prss
@color responsive-accent = accent
  | :hovered  → lighten(0.1)
  | :pressed  → darken(0.1)
  | :disabled → mute

[class.btn]     background = responsive-accent
[class.alt-btn] background = responsive-accent
```

**See §6.6** for the full design and the N × M / N + M
comparison.

### 5.4 Slots — one unified primitive

Today's `<slot>` and `<host-children/>` collapse to one element
with optional `name=`, optional fallback, optional typed
signature.

```prui
<component>
  <!-- typed slot with caller-provided scope -->
  <slot name=row,   signature=(item: Task, index: int) -> ui>
  <slot name=empty, signature=() -> ui, optional>
    <text>No tasks yet</text>
  </slot>

  <container if={#items > 0}>
    <fragment for={t, i in props.items}>
      <invoke slot=row, args={item=t, index=i}/>
    </fragment>
  </container>
  <invoke slot=empty if={#items == 0}/>
</component>

<!-- caller -->
<List items={tasks}>
  <slot name=row args={item, index}>
    <text>{index + 1}. {item.title}</text>
  </slot>
</List>
```

The default slot (no `name=`) splices the caller's unnamed
children — that's what today's `<host-children/>` does. Named
slots can carry fallbacks. Slot signatures make the
caller-provided scope a typed function.

**See §6.5** for the unification + typed signatures.

### 5.5 Imports & extension — one mechanism, many kinds

The fusion `<import>` family stays; a single Luau projection
covers six extension kinds.

```prui
<import stylesheet="./theme.prss"/>
<import script="./helpers.luau"/> as h
<import component="./card.prui"/>              <!-- file is a component -->
<import component="./icon.luau"/>              <!-- Luau prism.component{…} -->
<import contract="./focusable.prui"/> as Focusable
<import script="./traits.luau"/> as t          <!-- Luau-registered traits -->
```

Same `<import>` element, same `ImportResolver`, same
`as`-namespacing, same module cache. The Luau side has one
builder family covering six extension types:

```luau
prism.trait     {…}   -- attribute trait
prism.mixin     {…}   -- composable behaviour (state + hooks)
prism.macro     {…}   -- parse-time markup expansion
prism.derive    {…}   -- parse-time decl expansion
prism.component {…}   -- a component definition
prism.dialect   {…}   -- embedded sub-language (Wave E)
```

**See §6.11** for the import-family design; **§6.4** for trait
registration; **§6.8** for macros.

### 5.6 Mixins, derives, macros — three flavours of composition

Authoring a behaviour that attaches to many components, in
order of runtime cost:

- **Mixin** — runtime composition with linearisation (Scala
  MRO). Mixin state lives separately; hooks chain via
  `super()`. Overridable downstream.
- **Derive** — parse-time expansion. The mixin's state + hooks
  are inlined into the component declaration as if hand-written.
  No runtime chain. Not overridable downstream; cheaper at
  runtime.
- **Macro** — parse-time markup expansion. Pattern-match on
  tags or attributes; expand to other tags/attributes. The most
  general; closest to user-extending the grammar.

```prui
<mixin name=Hoverable>
  <state name=is-hovered, type=bool, default=false>
  <on event=pointerenter>{ is-hovered = true }</on>
  <on event=pointerleave>{ is-hovered = false }</on>
</mixin>

<derive name=Draggable>
  <state name=is-dragging, type=bool, default=false>
  <state name=drag-offset, type=point,  default=(0,0)>
  <on event=pointerdown>{ is-dragging = true; … }</on>
  <on event=pointermove if=is-dragging>{ … }</on>
  <on event=pointerup>{ is-dragging = false }</on>
</derive>

<macro name=field>
  <match><field label={lbl} value={val}/></match>
  <expand>
    <container direction=column gap=4>
      <text class=field-label>{lbl}</text>
      <input value={val}/>
    </container>
  </expand>
</macro>

<container with=[Hoverable]>…</container>
<component name=Card, derive=[Draggable]>…</component>
<field label="Title" value={state.title}/>
```

**See §6.3** for mixins/derives; **§6.8** for macros.

### 5.7 Capabilities — typed host services

Components declare what host services they need; the host
provides them at lower-time; missing capabilities fail at
parse, not at runtime. No magic globals.

```prui
<component name=ShareButton>
  <capability name=clipboard, type=Clipboard>
  <capability name=network,   type=Network, optional>

  <button on:click=$clipboard.write(props.text)>Copy</button>
</component>
```

The relay refuses to provide `FileSystem` to a public-facing
component → the component fails parse, the relay never renders
an exploit. Tests inject `MockClipboard`. The IDE completes
`$clipboard.<TAB>`.

**See §6.9** for the capability declaration + provision design.

### 5.8 Algebraic property types — discriminated unions

Props that are "one of these shapes" carry their variant
fields declaratively; the body pattern-matches.

```prui
<component name=Toast>
  <property name=tone, type=union<
    info,
    success { duration: int = 2000 },
    error   { dismissable: bool = true, retry: action? }
  >>

  <match on=props.tone>
    <case info>           …                                            </case>
    <case success(d)>     <progress duration={d}/> …                   </case>
    <case error(dis, r)>  … <button if={r != nil} on:click={r}>Retry</button> </case>
  </match>
</component>

<Toast tone={error(dismissable=true, retry=$retry-upload)}/>
```

The "boolean prop ladder" anti-pattern (`is-success` +
`is-error` + `is-info`) becomes impossible to express.

**See §6.10.**

### 5.9 Colour helpers — OKLCH + slash-alpha

Same function names, different math underneath; the
`darken('#3b82f6', 0.3)` no longer goes grey. New `with(c, l=±, c=±, h=±, a=…)`
for channel adjustments and `accent/50` slash-alpha sugar.

```prui
<container
  style.background=accent/30                             <!-- alpha = 0.30      -->
  style.tint={with(tokens.accent, l=+0.05, c=-0.02)}>    <!-- channel adjust    -->
```

**See §6.12.**

---

## 6. Feature designs

Each subsection follows the same shape: Problem → Design →
Rationale → Wave / Phase → What it deletes → Open questions.
Forward references to §7 phasing and §8 open questions.

### 6.1 Components — declaration, registration, the three surfaces

**Problem.** Five distinct registration paths for what is
conceptually one noun (see §3 Terminology, §4 snapshot). The
PRUI `<component>` markup tag is a silent alias for
`<container>` today and has no declaration semantics
(`interpret.rs:1739`). Authors don't know which surface to
write to.

**Design.** Three authoring surfaces, one noun, one
`ComponentRegistry`:

1. **File-as-component (`.prui`).** Every `.prui` file's outer
   element is the component body. Properties / slots /
   inheritance live in a `<component>` head block at the top of
   the file (or — depending on the A/B/C fork below — in a
   sibling `.luau`). Imported as
   `<import component="./button.prui"/>`, tag name = file stem.
2. **Markup declaration (`<component name=Foo>…</component>`).**
   For multiple components in one file; rarely used once
   file-as-component lands but kept for the in-file case (a
   `<form>` with three private sub-components).
3. **Luau (`prism.component{…}`).** Returned from a `.luau`
   file imported as `component` or `script`. The
   `prism.component{…}` builder is part of the
   `prism.<kind>{…}` family (§6.11).

All three convert to one `Component` trait impl that the
`ComponentRegistry` holds. The `Block` / `BlockSpec` /
`CoreWidgetBlock` / `PrefabComponent` / `LuauComponent`
Rust-side types are kept as source-level clarity but are
*invisible* at the authoring layer.

**The A/B/C fork (Q1 in §8).** Where does the *declaration
site* live?

- **(A) Markup decl is canonical.** `<component name=…,
  extends=…>` declarations inside `.prui` files are the
  declaration site; the file can hold multiple. Vue SFC route.
- **(B) File-as-component.** Every `.prui` file *is* a
  component body; declarations live in a head block. The
  `<component>` element retires as a tag — its only legal
  position is the document head. React / Svelte route.
  **Doc lean.**
- **(C) Luau-as-decl.** `prism.component{…}` is the sole
  declaration site. `.prui` files are markup imported by a
  Luau wrapper. The `<component>` element retires entirely.

Lean (B) because the file path already names the component; a
top-level `<component name=Button>` restates information the
import carries. (B) kills the worst current duplication
(`<component>`-as-alias-for-`<container>`) without forcing
every author through Luau.

**Wave / Phase.** Phase 6 (component declarations + properties
+ extends) unblocks once the A/B/C decision (Phase 5) closes.
The decision is one-way; once Phase 6 ships, retreating
requires a migration.

**What it deletes / supersedes.** The `<component>`-as-alias
behaviour (silently overlapping with `<container>`) goes away
regardless of which option wins; only the spelling of "where
do I declare a component" shifts.

**Open questions.** Q1 (A/B/C fork); Q6 (multi-component
files); Q7 (single `.luau` as both component and script).

---

### 6.2 Properties — declarations, defaults, required, discriminated unions

**Problem.** Today's component prop story is "anything goes;
the host binding decides." Defaults live in the host;
`required` is unenforced; type-checks rely on the external
`luau-analyze` pipeline (Wave I) instead of the parser.
Variant-rich props (`tone = {info | success | error}`)
degrade to boolean ladders.

**Design.** First-class declarations inside the component
body, including discriminated unions:

```prui
<component>
  <property name=title,    type=string, required>
  <property name=subtitle, type=string, default="">
  <property name=padding,  type=int,    default=16>
  <property name=onClick,  type=action>

  <!-- discriminated union — variant fields carried inline -->
  <property name=tone, type=union<
    info,
    success { duration: int = 2000 },
    error   { dismissable: bool = true, retry: action? }
  >>

  <container padding={props.padding} on:click=$props.onClick()>
    <heading level=3>{props.title}</heading>
    <text if={props.subtitle != ""}>{props.subtitle}</text>

    <match on=props.tone>
      <case info>           …                                  </case>
      <case success(d)>     <progress duration={d}/> …         </case>
      <case error(dis, r)>  … <button if={r != nil}>Retry</button> </case>
    </match>
  </container>
</component>
```

**Type set (closed).** `string`, `int`, `number`, `bool`,
`color`, `length`, `action`, `enum<a|b|c>`, `union<…>`,
`array<T>`, `object<{…}>`, or a `<contract>` / trait name. The
closed set keeps the parser-side type-check static.

**Default is a literal**, not a `{…}` expression — avoids the
recursive-default class of bugs from Vue's `withDefaults`.
Derived defaults compute in the body:
`padding={props.padding ?? row.depth * 4}`.

**`required` is a boolean presence flag** (no `=`). Missing
required props raise a contribution error at lower-time.

**Optional == default-bearing.** No separate `optional`
keyword.

**Wave / Phase.** Property declarations + `required` + literal
defaults: Phase 6. Discriminated unions + `<case Variant(fields)>`
destructure: Phase 12 (depends on the trait registry being
live for the inspector + Luau-narrowing integration).

**What it deletes / supersedes.** The "host knows the schema"
implicit-coupling that pollutes binding code today.

**Open questions.** Q2 (computed defaults via
`<property … computed={…}>`; deferred to §11).

---

### 6.3 Composition — inheritance, contracts, traits, mixins, derives

**Problem.** "Two components identical except for one
override" has no markup-level expression today; the only
escape is a shared PRSS class. "This slot accepts anything
`Focusable`" has no language-level expression. Stacking
behaviours (drag + hover + select + tooltip-host) onto a
container requires hand-rolling state + handlers + styles in
every component that wants the bundle.

**Design.** Five layered composition primitives, each with a
distinct cost / overridability tradeoff.

#### Inheritance — `<component extends=Parent>`

Shallow, single-parent. The child's body is a *patch* of the
parent's, allowed only to add `<property>` overrides,
`<slot>` declarations, and a single optional `<style>` block.
For structurally different bodies, *compose* with `<Parent>`
as a child element instead.

```prui
<!-- ./base-button.prui -->
<component>
  <property name=label, type=string>
  <property name=tone,  type=enum<default|primary|danger>, default=default>
  <container tag=button class=[btn, tone:{props.tone}]>
    <text>{props.label}</text>
  </container>
</component>

<!-- ./danger-button.prui -->
<component extends="./base-button.prui">
  <property name=tone, default=danger>
</component>
```

No multi-inheritance, no diamond. Multi-axis variation goes
through props (`tone`, `size`) or trait composition (below),
never multiple `extends=`. Parse-time flattening.

#### Contracts — declared shape, no body

A contract is a named property + callback shape (no body, no
state). Slots accept `accepts=ContractName`; components
conform *structurally* (no `implements=` keyword).

```prui
<contract name=Focusable>
  <property name=focused, type=bool>
  <callback name=focus>
  <callback name=blur>
</contract>

<component>
  <slot name=control, accepts=Focusable>
</component>
```

A component conforms to `Focusable` if its declared
`<property>`s and `<callback>`s are a superset of the
contract's. Mismatched callers fail at lower-time.

#### Traits — declared shape PLUS attribute / inspector wiring

A trait is a contract that *also* carries (state, attribute
methods, hooks, optional styles). It is the unit of the §6.4
open attribute registry — a trait names a set of attributes
the runtime knows how to dispatch.

```prui
<trait name=Pointable>
  <method name=on-click,   signature=action>
  <method name=on-hover,   signature=action>
  <state  name=is-hovered, type=bool, default=false>
</trait>

<component impls=[Pointable]>
  …body reads pointer.is-hovered freely…
</component>
```

Once a component impls a trait, the trait's attributes appear
in inspector + LSP completions + lowering. Coherence is
Rust-style: two traits with the same method name on the same
component is a parse-time error unless the author
disambiguates with `<trait-alias from=A.on-click, as=primary-click>`.

#### Mixins — trait + implementation, runtime composition with linearisation

A mixin is a trait that supplies *implementation* alongside
shape. Stacking mixins composes state + hooks. When mixins
collide on the same hook, Scala's linearisation gives a
deterministic order; chains call `super()` for the next hook.

```prui
<mixin name=Hoverable>
  <state name=is-hovered, type=bool, default=false>
  <on event=pointerenter>{ is-hovered = true }</on>
  <on event=pointerleave>{ is-hovered = false }</on>
  <style>&:hovered { background = lighten(currentBg, 0.05) }</style>
</mixin>

<container with=[Hoverable, Draggable]>…</container>
```

Linearisation is right-to-left "next" chain. `super()` calls
the next-in-chain handler. Render-time cost: one indirect call
per mixin per event.

#### Derives — trait expansion at parse time

A derive is a trait whose state + hooks are *inlined* into the
component declaration at parse time, as if hand-written. No
runtime dispatch chain; not overridable downstream; cheaper at
runtime than a mixin. Authored from Luau:

```luau
prism.derive("Draggable", |decl| {
  decl:state("is-dragging", false)
  decl:state("drag-offset", point(0, 0))
  decl:on("pointerdown",                              |e| { is-dragging = true; … })
  decl:on("pointermove", { if = "is-dragging" },      |e| { … })
  decl:on("pointerup",                                |e| { is-dragging = false })
})
```

```prui
<import script="./draggable.luau"/>
<component derive=[Draggable]>…</component>
```

Same shape Rust `#[derive(Clone)]` uses.

#### Choosing among the five

| Need | Use | Cost | Overridable downstream? |
|---|---|---|---|
| Variant of a parent with one override | `extends=Parent` | parse-time | no (single parent) |
| Slot must accept components of a certain shape | `<contract>` + `accepts=` | parse-time | n/a |
| Open vocabulary of attributes for a component | `<trait>` + `impls=` | runtime dispatch | n/a |
| Composable behaviour bundle (state + hooks + styles) | `<mixin>` + `with=[…]` | runtime chain + indirect call per event | yes (super()) |
| Behaviour bundle, no runtime chain needed | `<derive>` + `derive=[…]` | parse-time | no |

**Wave / Phase.** `extends`: Phase 6. Contracts: Phase 7.
Traits: Phase 8 (the trait registry itself). Mixins: Phase 9.
Derives: Phase 9.

**What it deletes / supersedes.** Wave J §4.5 PRSS `@mixin`
(subsumed by §6.8 macros + traits with styles). Wave J §4.8
variant prefixes (`hover:elevated` becomes
`with=[Hoverable, Elevated]`).

**Open questions.** Q5 (contract self-reference); Q9 (variant
precedence with chained prefixes — same answer as mixin
linearisation: latest wins).

---

### 6.4 The attribute system — open trait registry

**Problem.** 17 hard-coded namespaces (`AttributeNamespace`
enum in `ast.rs:74-152`). Each has bespoke runtime dispatch.
Adding a new attribute kind requires editing the enum, the
parser, and the lowering pass — three sites and a release.
Four of the seventeen are pure sugar (lower to `data-*`
pass-through) with zero production users.

**Design.** Replace the namespace enum with an **open trait
registry**. Every attribute is a trait method call, resolved
at parse time against a document-scoped registry. New
attribute kinds = new traits, registered from Luau or Rust
without grammar edits.

#### The four attribute surface forms

1. **`bare=value`** — direct property on the element's
   type-derived schema.
2. **`trait.method=value`** — explicit trait dispatch
   (`layout.gap=12`, `style.background=accent`).
3. **`group={k=v, …}`** — record-shaped trait call; groups N
   attributes per concept into one syntactic unit
   (`layout={direction=column, gap=12, padding=16}`).
4. **`with=[Mixin, …]` / `derive=[Trait, …]`** — behaviour
   composition (§6.3).

Plus two sugars for the most-common cases:
- **`@event=…`** ≡ `pointer.on-event=…`
- **`:prop={…}`** ≡ `bind.prop={…}`

The parser classify pass collapses from a seventeen-arm match
to two cases: bare name, or `trait.method` (split at first
`.`). Trait resolution moves to the registry; the parser stops
caring about namespaces.

#### Today's 17 namespaces — fate

| Namespace | Production uses | End-state fate |
|---|---|---|
| `Bare` | every file | survives as form 1 |
| `On` (`@e`) | 6 files | becomes `pointer.on-event`; `@` sugar stays |
| `Bind` (`:p`) | (J Phase 2) | becomes `bind.prop`; `:` sugar stays |
| `ControlFlow` (`if`, `for`) | every file | survives as element attribute (not trait) |
| `Style` (`style:`) | most files | becomes `style.<prop>` or `style={…}` record (§6.6) |
| `Signal` (`sig:`) | 0 today (via `<script>`) | becomes `signal.<name>` trait |
| `Class` (`class:p`) | 0 | becomes `style.class.foo=cond` or mixin |
| `Identifier` (`class`, `id`) | every file | survives as form 1 |
| `Aria` (`aria:`) | 5 files | becomes `a11y.<attr>` trait |
| `Data` (`data:`) | many files | becomes `data.<attr>` trait |
| `Route` (`route:`) | **0** | **deleted** (sugar for `data.<k>`; explicitly equivalent per `ast.rs:96-106`) |
| `Facet` (`fct:`) | **0** | **deleted** (see §6.13) |
| `Probe` (`probe:`) | 0 in .prui (Luau side wired) | becomes `probe.<name>` trait — load-bearing, runtime subscribes |
| `Transition` (`transition:`) | 0; install deferred | **ship or delete** in Phase 3 |
| `Animate` (`animate:`) | 0; install deferred | **ship or delete** in Phase 3 |
| `At` (`at:`) | 0; deferred | **ship or delete** in Phase 3 |
| `Use` (`use:`) | 0 | **delete** — subsumed by `derive=` + mixin attrs |

Net: 17 → ~12 namespaces as *labels*, but the *registry*
opens to user extension — adding `drag.handle=…` is one Luau
declaration, not an engine edit.

**Wave / Phase.** Phase 8 (trait registry + four built-in
traits: `layout`, `style`, `pointer`, `a11y`). Phases 2-3
carry the deletions (`route`, `facet`, Tier 3 audit).

**What it deletes / supersedes.** The `AttributeNamespace`
enum itself, the per-namespace dispatch in `interpret.rs`,
the four sugar-only namespaces.

**Open questions.** Q3 (Tier 3 audit decision); Q8 (pipeline
`|` vs logical-or — see §6.6); Q11 (cross-library trait
coherence).

---

### 6.5 Slots — one unified primitive with optional typed signature

**Problem.** Two spellings for "splice caller content here":
`<slot/>` (`interpret.rs:1848`) and `<host-children/>`
(`:1909`). Comment at `:1865-1867` admits the equivalence. The
1 / 9 production split (slot / host-children) reflects
history, not intent. Typed slot scopes (the React render-prop
/ Svelte 5 snippet pattern) have no language surface today.

**Design.** One element: `<slot>`. Four orthogonal axes:

- **Named or default.** `<slot/>` is the default slot;
  `<slot name="X"/>` is named.
- **With or without fallback.** `<slot>…children render if no
  override…</slot>`.
- **With or without typed signature.**
  `<slot signature=(item: T, index: int) -> ui>` declares the
  slot is invoked with a typed scope.
- **Invoked or sliced.** A typed slot is *invoked* with
  `<invoke slot=row args={…}/>`; an untyped slot is sliced
  (the current default behaviour).

```prui
<component>
  <slot name=row,   signature=(item: Task, index: int) -> ui>
  <slot name=empty, signature=() -> ui, optional>
    <text>No tasks yet</text>
  </slot>

  <container if={#items > 0}>
    <fragment for={t, i in props.items}>
      <invoke slot=row, args={item=t, index=i}/>
    </fragment>
  </container>
  <invoke slot=empty if={#items == 0}/>
</component>

<List items={tasks}>
  <slot name=row args={item, index}>
    <text>{index + 1}. {item.title}</text>
  </slot>
</List>
```

The default slot (no `name=`) splices the caller's unnamed
children — exactly what `<host-children/>` does today. Named
slots with no fallback fall through to nothing. Typed slot
signatures fail at parse if the caller provides a mismatched
scope.

**`<host-children/>` retires.** Mechanical rewrite of 9
production files (one PR). Deprecation diagnostic for one
release; deletion after.

**Wave / Phase.** Phase 4 (unify `<slot>` / `<host-children/>`,
no-signature variant). Phase 13 (typed signatures + `<invoke>`,
depends on trait registry being live).

**What it deletes / supersedes.** `<host-children/>` element;
`LowerScope::host_children_for_slot` / `host_children_ui`
surface; Wave J §4.4's `takes={…}` (generalised to
`signature=`).

**Open questions.** None blocking.

---

### 6.6 Style and state variants — nested records, pipelines, named values

**Problem.** Today, state-aware style declarations restate the
property × selector grid:

```prui
<container
  style:background=accent,
  style:background:hovered={lighten(accent, 0.1)},
  style:background:pressed={darken(accent, 0.1)},
  style:background:disabled=mute>
```

```prss
[class.btn] background = { accent }
[class.btn:hovered] background = { lighten(accent, 0.1) }
[class.btn:pressed] background = { darken(accent, 0.1) }
[class.btn:disabled] background = mute
```

The property name (`background`) and (in PRSS) the selector
head (`[class.btn`) appear four times each. The N × M
cross-product explodes linearly in authoring effort for
content that conceptually scales with N + M.

**Design.** Four layered shapes, authors pick by the shape of
the duplication.

#### Shape 1 — Nested state records (for many properties at once)

A `style={…}` record (PRUI) or PRSS class body accepts state
keys as nested records that override declared properties:

```prui
<container style={
  background = accent,
  radius     = 8,
  :hovered   = { background = lighten(0.1), radius = 12 },
  :pressed   = { background = darken(0.1) },
  :disabled  = { background = mute, opacity = 0.5 },
}>
```

```prss
[class.btn] {
  background = accent
  radius     = 8
  &:hovered  { background = lighten(0.1), radius = 12 }
  &:pressed  { background = darken(0.1) }
  &:disabled { background = mute, opacity = 0.5 }
}
```

PRSS uses CSS-Nesting (`&` for parent context — the 2024 CSS
Nesting spec). PRUI uses nested-record `style={…}`. **Both
surfaces share one nesting pattern**; the PRUI `style={…}`
value *is* a PRSS class body literal.

Inside a state record, colour helpers (`lighten`, `darken`,
OKLCH `with`) **default their first argument to the parent
record's same-named property**. `lighten(0.1)` inside
`:hovered.background` is sugar for
`lighten(parent.background, 0.1)`. Same "implicit `self`"
rule SwiftUI's modifier chain and CSS's `currentColor` use.

#### Shape 2 — Pipeline form (terse, for one property)

One property, several state deltas, in pipeline shape:

```prui
<container style.background={
  accent
  | :hovered  → lighten(0.1)
  | :pressed  → darken(0.1)
  | :disabled → mute
}>
```

```prss
[class.btn] background = accent
  | :hovered  → lighten(0.1)
  | :pressed  → darken(0.1)
  | :disabled → mute
```

Reads as a sentence: "background is accent — when hovered,
lighten by 0.1; when pressed, darken; when disabled, mute."

#### Shape 3 — Named state-responsive values (for workspace reuse)

```prss
@color responsive-accent = accent
  | :hovered  → lighten(0.1)
  | :pressed  → darken(0.1)
  | :disabled → mute

[class.btn]        background = responsive-accent
[class.alt-btn]    background = responsive-accent
[class.danger-btn] background = danger
  | :hovered  → lighten(0.1)
  | :pressed  → darken(0.1)
  | :disabled → mute
```

`@color`, `@spacing`, `@radius` live in the token table. Every
button on the workspace shares one hover/press/disabled curve.

#### Comparison — line counts

| Cells | Today | Shape 1 (nested) | Shape 2 (pipeline) | Shape 3 (named) |
|---|---|---|---|---|
| 1 prop × 4 states | 4 lines | 5 lines | **4 lines** | **1 line** + 1 defn |
| 3 props × 4 states | 12 lines | **7 lines** | 12 lines | 3 lines + 1 defn per prop |
| 5 buttons × 1 prop × 4 states | 20 lines | 25 lines | 20 lines | **6 lines** (1 `@color` + 5 refs) |

**Symmetry between PRUI and PRSS.** A snippet copy/pasted from
PRSS into a PRUI `style={…}` works without rewriting — same
nesting, same delta syntax, same named-value references. This
collapses the historical PRUI `style:key=` vs. PRSS
`[selector] key =` duplication into one syntax shared by both
surfaces.

#### Pipeline `|` and logical-OR (Q8)

The pipeline form is parsed only when the value position is
typed `Stateful<T>` or `Animated<T>`. Bare `|` in any other
expression position parses as logical-or, same as today.

**Wave / Phase.** Phase 14 (Shapes 1 + 2 + named-reference
plumbing); Phase 15 (named state-responsive value declarations
in the token table). Depends on the trait registry being live
(Phase 8) for the parent-context helper resolution.

**What it deletes / supersedes.** The four-line per-state
restatement (retained as a fallback parse path during the
transition).

**Open questions.** Q8 (pipeline `|` syntax — resolved via
type-position disambiguation).

---

### 6.7 Pseudo-state runtime expansion

**Problem.** Today the runtime tracks `:hovered` and applies
it on `background` / `radius` only (`interpret.rs:5797`,
`:5805`). `:pressed`, `:disabled`, `:focus-within`, `:empty`,
`:checked` are useful and absent. `:selected` and `:focused`
are parsed (`STATE_SUFFIXES` in `interpret.rs:5439`) but never
swap at runtime.

**Design.**

1. **`STATE_SUFFIXES` grows** to `["hovered", "pressed",
   "focused", "focus-within", "selected", "disabled", "empty",
   "checked"]`.
2. **Property whitelist expands** beyond `background` /
   `radius` to `color`, `padding`, `gap`, `width`, `height`,
   `tint`.
3. **State-flag wiring** in the shell event router writes
   per-node bool flags (`is_pressed`, `is_disabled`,
   `is_focused`) into the existing `Surface::hovered_id` state
   table. `apply_container_attributes` gains
   `PressedOverrides` / `FocusedOverrides` /
   `DisabledOverrides` alongside `HoverOverrides`.
4. **State precedence.** Multiple matching states: last-write
   wins in the declared order `hovered < focused < pressed <
   selected < disabled`. Disabled always wins so a greyed-out
   button doesn't visually press.

**Wave / Phase.** Phase 1 — ships first, alongside the colour
helpers (§6.12). Zero grammar changes; entirely
`prism-ui-runtime` + a couple of state-tracker rows in
`prism-shell`. The authoring surface for these states
(verbose today, collapsed in §6.6) is independent of the
runtime work.

**What it deletes / supersedes.** Nothing — additive only.

**Open questions.** Q10 (does `:disabled` suppress `on:click`
at the dispatcher, or only style?).

---

### 6.8 Macros over markup

**Problem.** Today PRUI has dialects (Wave E) — embedded
sub-languages for whole subtrees. Useful, but coarse-grained.
There's no way to extend the *attribute* surface or define a
component-like shape that *expands at parse time* into a tree.
Wave J §4.5's PRSS `@mixin` solved one specific case
(PRSS-side multi-property reuse); the macro engine generalises
it.

**Design.** Pattern-match on markup, expand to markup, before
lowering. `macro_rules!` for the DSL.

```prui
<macro name=field>
  <match>
    <field label={lbl} value={val}/>
  </match>
  <expand>
    <container direction=column, gap=4>
      <text class=field-label>{lbl}</text>
      <input value={val}/>
    </container>
  </expand>
</macro>

<field label="Title" value={state.title}/>
```

At parse time `<field>` is matched and replaced with the
expansion; `{lbl}` and `{val}` are substituted. Expansions can
recurse (a macro expanding to another macro), bounded by a
configurable depth limit.

**Hygiene.** Macro-introduced identifiers (e.g. a `let` inside
the expansion) are renamed to fresh names so they can't
shadow the call-site bindings. Same shape Rust 2018+ macros
use.

**Attribute macros** ride the same primitive:

```prui
<attribute-macro name=elevation, params=[level: int]>
  <expand to-attrs>
    style.radius=8,
    style.background={tokens.surface},
    style.shadow={elevations[level]}
  </expand>
</attribute-macro>

<container elevation=2>…</container>
```

**Wave J §4.5 PRSS `@mixin` is absorbed.** `@mixin
elevation(level) { … }` becomes one specific case of
attribute-macro — the macro engine generalises across both
PRUI and PRSS surfaces, one engine instead of two parser
dialects.

**Authored from Luau too:**

```luau
prism.macro {
  name    = "field",
  pattern = prui_pattern [[ <field label={lbl} value={val}/> ]],
  expand  = |args| prui [[
    <container direction=column, gap=4>
      <text class=field-label>{args.lbl}</text>
      <input value={args.val}/>
    </container>
  ]],
}
```

**Wave / Phase.** Phase 10. Requires trait registry (Phase 8)
for the Luau-side registration shape and the
attribute-substitution machinery.

**What it deletes / supersedes.** Wave J §4.5 PRSS `@mixin`
(one absorbed case); the implicit "engine ships every
attribute kind" assumption.

**Open questions.** None blocking. Depth limit + size limit
default values are a `prism-cli` lint flag.

---

### 6.9 Capabilities — typed host services

**Problem.** Components today reach host services (clipboard,
network, filesystem, VFS) via magic globals or by threading
them through props. No SSR sandbox enforcement; no type-safe
IDE completion; no test injection.

**Design.** A component declares the capabilities it needs by
type; the host provides them at lower-time; missing required
capabilities fail at parse, not at render.

```prui
<component name=ShareButton>
  <capability name=clipboard, type=Clipboard>
  <capability name=network,   type=Network, optional>

  <button on:click=$clipboard.write(props.text)>Copy</button>
</component>
```

Host provision:

```rust
ctx.with_capability(Clipboard::system())
   .with_capability(Network::reqwest_client())
   .render(component);
```

Three wins:

- **DI without globals.** Components don't import
  `prism.clipboard` from a magic root.
- **SSR sandboxing.** The relay refuses to provide
  `FileSystem` to a public-facing component → component fails
  parse, relay never renders an exploit.
- **Test injection.** Tests provide `MockClipboard`, no module
  state.

Capabilities are *types*, not strings. Misspellings fail at
parse; the IDE completes `$clipboard.<TAB>`.

**Wave / Phase.** Phase 11. Requires trait registry (Phase 8)
for the host-provision machinery, but no Luau-side
prerequisite.

**What it deletes / supersedes.** Module-global host
accessors; ad-hoc prop threading.

**Open questions.** None blocking. Host-capability
allow-lists per environment (Shell / SSR / mobile / web) are
policy decisions.

---

### 6.10 Algebraic property types and pattern destructure

**Problem.** Variant-rich props (`tone = {info | success |
error}`, each carrying different fields) degrade to flat
enums plus boolean ladders today. The "is-success +
is-error + is-info" anti-pattern is one wrong-typing away
from a runtime check.

**Design.** Discriminated unions as a property type, plus
`<case Variant(fields)>` destructure in the existing
`<match>` primitive.

```prui
<property name=tone, type=union<
  info,
  success { duration: int = 2000 },
  error   { dismissable: bool = true, retry: action? }
>>

<match on=props.tone>
  <case info>           …                              </case>
  <case success(d)>     <progress duration={d}/> …     </case>
  <case error(dis, r)>  … <button if={r != nil}>Retry</button> </case>
</match>

<Toast tone={error(dismissable=true, retry=$retry-upload)}/>
```

The property panel auto-generates a variant picker plus a
per-variant sub-form. The Luau type stub generator emits a
tagged union the analyzer narrows inside
`if props.tone.kind == "error" then …`.

**Wave / Phase.** Phase 12 (depends on the trait registry +
the parser-side type set extension).

**What it deletes / supersedes.** The boolean-prop-ladder
anti-pattern; the runtime-check duplication.

**Open questions.** None blocking.

---

### 6.11 Imports & projections — one mechanism, many extension kinds

**Problem.** The fusion `<import>` family has four
projections (`stylesheet`, `script`, `dialect`, `widget`).
Three are wired; `widget` is parsed but the runtime drops it
(`interpret.rs:1210` parses, `:1018` skips). The projection
name `widget` overloads with the codebase term and reads
wrong against the §3 "component is the noun" rule.

**Design.** Five projections in the end state:

| Projection | Role | Files | Alias |
|---|---|---|---|
| `stylesheet` | apply styles | `.prss` (or `.luau` returning a stylesheet table) | `as ns` |
| `script` | reuse helpers / register extensions | `.luau` | `as ns` (named module) / bare (flat-merge) |
| `component` | register a component tag | `.prui` *or* `.luau` (calling `prism.component{…}`) | `as Tag` |
| `contract` | parse-time shape check | `.prui` (`<contract>` decl), `.luau` (`prism.contract{…}`) | `as Name` |
| `dialect` | extend the language | `.luau` (calling `prism.dialect{…}`) | (no alias) |

The fifth (`dialect`) lands with fusion Wave E. `component`
replaces `widget` (renamed). `contract` is new.

**File-extension dispatch.**
`<import component="./button.prui"/>` parses + lowers the
`.prui` body; `<import component="./icon.luau"/>` evaluates
the file's `return prism.component{…}` expression. The
resolved tag goes through one `TagResolver` — a `.prui`-
defined card and a `.luau`-defined card are
indistinguishable at the call site.

**One Luau projection covers six extension kinds.** A `.luau`
file imported as `script` can return any of:

```luau
prism.trait     {…}   -- §6.4 attribute trait
prism.mixin     {…}   -- §6.3 composable behaviour
prism.macro     {…}   -- §6.8 markup expansion
prism.derive    {…}   -- §6.3 parse-time decl expansion
prism.component {…}   -- §6.1 component definition
prism.dialect   {…}   -- Wave E embedded language
```

The file's `return` value (or a table of them) is bound under
the import's `as` namespace. Same `<import>` element, same
`ImportResolver`, same `as`-namespacing, same module cache.

**Module identity.** Resolved absolute path is the key — N
call sites importing the same file parse it once.

**Wave / Phase.** `widget` → `component` rename + wiring:
Phase 7. New `contract` projection: Phase 7. The full
`prism.<kind>{…}` Luau-builder family lands incrementally as
each kind ships (mixins/derives Phase 9, macros Phase 10,
etc.).

**What it deletes / supersedes.** The `widget` projection
keyword; the dead `interpret.rs:1018` handler skip; the
doc-only `prism.widget{…}` builder.

**Open questions.** Q6 (multi-component `.prui` files); Q7
(`.luau` as both component and script in one file).

---

### 6.12 Colour helpers v2 — OKLCH + slash-alpha

**Problem.** Current helpers (`interpret.rs:4710-4751`) lerp
in sRGB → `darken('#3b82f6', 0.3)` produces a desaturated
grey-blue instead of a darker vivid blue. CSS Color Level 5 /
OKLCH `color-mix` is the 2026 state of the art.

**Design.**

1. **Keep `darken` / `lighten` / `alpha` / `mix` names; switch
   implementation to OKLCH.** sRGB hex in, sRGB hex out, lerp
   / adjust in OKLCH. No surface change; output dramatically
   better. ~40 lines rewrite.
2. **Add `with(c, l=…, c=…, h=…, a=…)`** for channel
   adjustments. `+0.05` adds; `0.5` sets absolute.
3. **Add `saturate` / `desaturate`** as shortcuts (`with(c, c=±t)`).
4. **Slash-alpha shorthand.** `accent/50` is sugar for
   `alpha(tokens.colors.accent, 0.5)`.
5. **Hex-with-alpha unchanged.** `#3b82f680` continues to work.

```prui
<container
  style.background=accent/30
  style.tint={with(tokens.accent, l=+0.05, c=-0.02)}>
```

**Wave / Phase.** Phase 1 — ships first alongside pseudo-state
runtime. Zero grammar changes; contained rewrite. Reuses the
`palette` Rust crate's OKLCH module.

**What it deletes / supersedes.** sRGB-lerp helpers (replaced
in place); the implicit "transparent value via separate alpha
function" idiom.

**Open questions.** Q4 (OKLCH gamut clipping — chroma
reduction over hue rotation, documented choice).

**Rejected.** A zoo of named functions (`shade`, `tint`,
`tone`, `complement`). One `with` + one `mix` cover every
case; the CSS Level 5 working group walked away from the SCSS
`darken` family for the same reason.

---

### 6.13 Deletions — what goes away

A summary table of mechanisms that exist today but won't
survive the J + K + L trajectory. Each row maps to a Phase
in §7.

| Mechanism | Where it lives | What replaces it | Phase |
|---|---|---|---|
| `<facet>` element | `interpret.rs:1894-1908` | `<container for="x in items">` | 2 |
| `FacetComponent` builder block | `prism-builder/src/facet/render.rs` | user can author a `<facet>` component if call-site phrasing matters | 2 |
| `Facet` attribute namespace (`fct:`) | `ast.rs:89` | fold into `data:` | 2 |
| Stale `FacetDef` / `FacetKind` / … catalogue in `prism-builder/CLAUDE.md` | docs | accurate post-deletion documentation | 2 |
| `Route` namespace (`route:`) | `ast.rs:97-106` | `data:` (explicitly equivalent per existing doc comment) | 2 |
| `<host-children/>` element | `interpret.rs:1909-1919` | `<slot/>` (default) with optional `name=` | 4 |
| `Use` namespace (`use:`) | `ast.rs:115-121` | `derive=` + mixin attrs | 8 |
| Tier 3 incomplete animation namespaces (`Transition` / `Animate` / `At`) | `ast.rs:108-149` | ship in Phase 1 or delete; same drift trap that left `widget=` dead | 3 |
| `<component>`-as-`<container>` silent alias | `interpret.rs:1739` | one of: real declaration site (A), file head only (B), retired (C) | 5 (decision), 6 (impl) |
| Dead `widget=` import projection | `interpret.rs:1210` parses, `:1018` skips | renamed to `component=` and wired | 7 |
| 17-namespace `AttributeNamespace` enum | `ast.rs:74-152` | open trait registry | 8 |
| Wave J §4.5 PRSS `@mixin` (would have shipped in J Phase 4) | (would be) | absorbed by §6.8 macro engine | 10 |
| Wave J §4.8 variant prefixes (would have shipped in J Phase 4) | (would be) | `with=[Mixin, …]` (§6.3) | 9 |
| Long-form closure `\fn(args) … end` (fusion §7.2 alt) | fusion-doc grammar | `\|args\| expr` only | n/a (already removed from this doc's surface) |

**Why so many deletions.** Most of these are not breaking
changes for the live codebase — the production `.prui` corpus
uses zero `<facet>`, zero `route:`, zero `fct:`, zero `use:`,
zero `transition:`, zero `animate:`, zero `at:`, zero
`widget=`. The cleanup is overwhelmingly about not shipping
mechanisms that promise behaviours and deliver nothing.

The two with real production impact:

- `<host-children/>` (9 files) — mechanical rewrite to
  `<slot/>` in one PR.
- `<component>` aliasing `<container>` — only matters once
  the fork (A/B/C) decides whether the tag survives at all.

---

## 7. Phased rollout

One unified plan across Waves J + K + L, ordered by readiness.
Each phase is independently shippable: tests pass, no other
phase depends on the next-in-line. Decision-only phases (Phase
5) carry no code but block downstream work.

| # | Phase | Wave tag | Scope (§ refs) | Prereqs | Reversibility | LOC delta |
|---|---|---|---|---|---|---|
| 1 | Colour v2 + pseudo-state runtime | J Phase 1 | §6.12 + §6.7 | none | reversible | +300, 0 deleted |
| 2 | Facet trinity + Route deletion | K.1 + K.2 | §6.13 rows 1–5 | none | reversible (git revert) | −170 |
| 3 | Tier 3 namespace audit | K.3 | §6.13 row 8 (decision + impl) | none | semi-reversible | −30 to −100 per namespace dropped |
| 4 | Slot / host-children unification | K.4 | §6.5 (no-signature variant) | none | reversible during deprecation | −80, 9 files edited |
| 5 | Component A/B/C fork decision | K.5 | §6.1, Q1 | none (decision) | one-way once Phase 6 ships | n/a |
| 6 | Component declarations + properties + `extends` | J Phase 2 | §6.1, §6.2, §6.3 (extends only) | Phase 5 | reversible until widely adopted | +400 |
| 7 | Contracts + `component`/`contract` projection wiring | J Phase 3 | §6.3 (contracts), §6.11 | Phase 6 | reversible | +300 |
| 8 | Trait registry + four built-in traits + attribute surface | L.1 + L.2 | §6.4, §6.6 (parent-context helper) | Phase 4 | one-way; the big one | +600, −200 |
| 9 | Mixins + derives + `with=` / `derive=` | L.3 + L.4 | §6.3 (mixins, derives), §6.13 row 11 | Phase 8 | reversible until widely adopted | +400 |
| 10 | Macros over markup + hygiene | L.7 | §6.8 | Phase 9 | reversible | +300 |
| 11 | Capabilities + host injection pipeline | L.5 | §6.9 | Phase 8 | reversible | +250 |
| 12 | Algebraic property types + `<case Variant(fields)>` | L.6 | §6.2 (union types), §6.10 | Phase 8 | reversible | +200 |
| 13 | Typed slot signatures + `<invoke>` | L.8 | §6.5 (typed variant) | Phases 4, 8 | reversible | +150 |
| 14 | State-variant nested records + pipeline form | L.9 | §6.6 (Shape 1, Shape 2) | Phases 8, 9 | reversible until widely adopted | +200, −30 |
| 15 | Named state-responsive `@color` / `@spacing` / `@radius` | L.10 | §6.6 (Shape 3) | Phase 14 | reversible | +100 |
| 16 | Polish — migration sweep + docs + `prism-cli` lint | J Phase 5 | all | Phase 15 | n/a | mostly doc / lint |

### Pacing

Phases 1–4 are safe near-term wins (a week or two each).
Phase 5 is a decision; once made, Phase 6 follows promptly.
Phases 6–8 form the structural middle — Phase 8 is the
largest single slice and unlocks Phases 9–15. Phase 16 is
ongoing.

The big risk is that Phase 6 ships before Phase 5 closes,
locking in option (A) by accident. Mitigation: Phase 5 is on
the critical path and explicitly blocking.

Total LOC delta across all 16 phases: approximately +3000
added, −600 deleted (counting only surfaces in this doc;
test code is excluded). The net is +2400 across roughly a
year of part-time work — a significant but not unreasonable
expansion for the value the table in §10 returns.

---

## 8. Open questions

Numbered for cross-reference. **Q1 is the highest priority** —
it blocks Phase 6.

**Q1. The §6.1 A/B/C fork.** Where does the component
declaration site live? (A) markup `<component name=…>` decls
inside `.prui` files; (B) file-as-component; (C)
Luau-as-decl. Doc lean: B. Decision phase: 5. Blocks Phase 6.

**Q2. Computed property defaults.** §6.2 disallows expression
defaults to dodge recursion. Should there be a second tier
(`<property … computed={…}>`) for derived props, topologically
resolved? Leaning yes, in a later wave; first cut keeps it
simple. See §11.2.

**Q3. Tier 3 namespace audit (Phase 3).** Does Prism ship the
deferred animation primitives (`transition:`, `animate:`,
`at:`) in Phase 1, or move them to a later wave? If "later
wave", the namespaces should be ripped now and re-added on
the day they ship — vestigial grammar attracts drift (same
shape as the dead `widget=`).

**Q4. OKLCH gamut clipping.** Out-of-gamut OKLCH values must
project back to sRGB. The `palette` crate gives chroma
reduction; we pick that over hue rotation. Document the
choice in `prss-reference.md`.

**Q5. Contract self-reference.** Can a `<contract>` reference
itself (a `Composite` slot type)? Today no. If demand
surfaces, gate behind an explicit `recursive` keyword.

**Q6. Multi-component `.prui` files.** Proposed:
single-component-per-file is the cultural rule; multiple
top-level components require `as Ns` and bind under
`<Ns.foo/>`. Open: do we ever want unnamespaced multi-export?

**Q7. `.luau` component vs `.luau` script in the same file.**
Can a `.luau` file return both a component table and helper
exports? Proposed: yes, via `{ component = …, helpers = … }`
shape; consumer picks the lens via `<import component=…>` vs
`<import script=…>`. Needs prototype.

**Q8. Pipeline `|` vs logical-or.** Resolved: the pipeline
form is parsed only in value positions typed `Stateful<T>` or
`Animated<T>`; bare `|` elsewhere is logical-or.

**Q9. Variant precedence with chained prefixes.** Resolved:
latest wins, same as Tailwind. Mixin linearisation (§6.3) is
the same rule applied at the trait registry layer.

**Q10. Disabled-state pointer behaviour.** Should `:disabled`
suppress `on:click` at the dispatcher, or only style? Lean
suppress, but it changes existing semantics — audit first.

**Q11. Trait coherence in cross-library use.** Two unrelated
libraries register `Draggable` differently → use namespaced
imports (`<import script="./libA.luau"/> as libA` →
`with=libA.Draggable`). Bare `with=Draggable` is a parse
error when two unprefixed imports collide.

---

## 9. Rejected alternatives

A consolidated list of ideas considered and dropped. Listed
so future debates can find the prior reasoning instead of
relitigating.

- **The long-form closure `\fn(args) … end`** (originally an
  alternative to `|args| expr` in fusion §7.2). Single closure
  form going forward: `|args| expr`. Rationale: two spellings
  for one concept is a Wave K-style violation of the
  "keep the surface narrow" principle (§2). The short form
  covers every case the long form did. Already enforced in
  this doc — every code example uses `|args| expr` only.
- **A new `<import component=…>` projection alongside
  `widget=`.** The fix is to rename `widget=`, not duplicate
  it (§6.11).
- **PRSS `@import "elevations.prss"`.** Duplicate of
  `<import stylesheet=…>`. PRSS sheets compose via the host
  PRUI document's import family.
- **Multi-inheritance / mixin-in-extends.** §6.3
  single-parent rule; multi-axis goes through mixins.
- **Imperative `style.set(…)` API in `<script>` blocks.** PRSS
  + state variants cover the use case declaratively; an
  imperative escape hatch fragments the styling story.
- **CSS-style descendant negation selectors
  (`:not(:first-child)`).** PRSS multi-segment descendant keys
  already cover the use case; pseudo-selector negation doesn't
  compose with brace-expr values and would force a second
  parsing mode. The same effect lands as a `for` loop with
  `if={i > 0}`.
- **A `theme {}` block at the document root.** Token tables
  already do this; adding a second mechanism splits the
  design-token story.
- **Auto-import of `prism://core/mixins`.** Explicit
  `<import>` beats implicit globals — mirrors the fusion
  doc's rejection of `scripts=["…"]` manifest lists.
- **Distinct `prism.widget{…}` vs `prism.component{…}` Luau
  builders.** One builder; renamed in §6.11.
- **A zoo of named colour functions** (`shade` / `tint` /
  `tone` / `complement`). One `with` + one `mix` cover every
  case (§6.12).
- **`implements=Focusable` annotation on a component.**
  Structural-on-properties (§6.3 contracts) is enough; the
  annotation would only restate what the props already say.
- **A second markup syntax (JSX-style, Pug, anything)** that
  produces the same AST. The XML-shape grammar is the
  surface; multiple surfaces splinter the type story.

---

## 10. Receipts — what this unlocks

LOC measurements are honest reads of existing `.prui` files
(`apps/lattice/shell.prui`,
`packages/prism-shell/ui/components/*.prui`). Comparing today
to the end state across all three waves.

### Authoring wins

| Task today | LOC | End-state LOC |
|---|---|---|
| Button-but-louder-hover variant | new PRSS class + new component (~25 lines) | `<component extends=Button>` + 1 prop default (4 lines) |
| Lighter / darker / transparent variant of a token colour | hand-tuned hex per call site | `accent/50`, `with(accent, l=+0.05)` (1 expr) |
| Pressed-state visual feedback | script-toggled class + PRSS variant (~12 lines) | `style.background={accent \| :pressed → darken(0.1)}` (1 line) |
| 3 props × 4 states on one component | 12 lines (cross-product) | 7 lines (nested record) |
| 5 buttons sharing one hover/press/disabled curve | 20 lines | 6 lines (`@color` + 5 refs) |
| Reusable elevation system across the workspace | 6 near-identical classes copy-pasted | 1 attribute macro |
| Dark-mode override on one element | duplicate class with `dark-` prefix | `with=[Card, dark ? Muted : nil]` |
| Form field that requires a focusable control | manual prop binding + runtime assertion | `<slot accepts=Focusable>` (1 attr) |
| Required prop with a sensible default | host binding + nullable check in body | `<property name=label, type=string, default="">` |
| Import a `.prui` component from a sibling directory | impossible (no runtime wiring) | `<import component="../widgets/card.prui"/> as card` |
| Author a component imperatively from Luau | impossible (`prism.widget{…}` never written) | `return prism.component{…}` in a `.luau` file |
| Stack draggable + hoverable + selectable on a container | hand-roll state + handlers + styles (~40 lines) | `with=[Draggable, Hoverable, Selectable]` (1 attr) |
| Define a new attribute kind (e.g. `elevation=2`) | impossible without engine release | `<attribute-macro name=elevation>` or `prism.trait{…}` |
| Component needs clipboard access | thread global through props | `<capability name=clipboard, type=Clipboard>` (1 line) |
| Toast with three variants carrying different fields | boolean prop ladder | `<property name=tone, type=union<…>>` + `<match>` |

### Structural wins

| Today | End state |
|---|---|
| 17-namespace `AttributeNamespace` enum, hard-coded | Open trait registry, document-scoped, Luau-extensible |
| 5 registration paths for a component | 1 noun, 3 authoring surfaces |
| 3 implementations of "repeat children once per item" | 1 (`<container for=…>`) |
| 2 spellings for "splice caller content" | 1 (`<slot/>`) |
| PRUI `style:key=` vs PRSS `[selector] key =` (two syntaxes) | 1 nested-record syntax shared |
| 4 sugar-only namespaces in the enum | 0 (deleted) |
| `<component>` silently aliases `<container>` | Real declaration site (per fork option) |
| `widget=` import parsed but dropped | Either wired (as `component=`) or deleted |
| Two closure spellings (`\|args\| expr` and `\fn(args)…end`) | One (`\|args\| expr`) |
| Stale `prism-builder/CLAUDE.md` Facet catalogue | Accurate documentation |

### Failure-mode wins

| Today | End state |
|---|---|
| Hidden globals leak host state to public-facing SSR | Capability declarations fail parse at the boundary |
| Two libraries' "Draggable" silently conflict | Namespaced imports force disambiguation |
| Wrong prop type fails at runtime | Discriminated unions + IDE narrowing catch it at parse |
| New attribute kind requires an engine release | Luau-registered traits ship as user libraries |

---

## 11. Future musings — tier 2 ideas

Items interesting but not on the immediate path. Each is
sketched well enough that a future RFC author can pick it up
without restarting the design. Listed in rough order of
authoring-impact-per-implementation-cost; nothing here is
committed, and several entries conflict with each other.

### 11.1 Typestate-encoded invariants

`<container display=block gap=8>` silently ignores `gap`
today. Typestate would encode the legality:

```prui
<trait name=Flex, impls=Container>
  <require Container.display=flex|grid>
  <method name=gap, signature=int>
</trait>
```

`<container display=flex gap=8>` legal; `<container
display=block gap=8>` fails at parse with a pointer at `gap`.

Once the trait registry (§6.4) lands, this is mechanical —
maybe a hundred lines of resolver code. **Deferred** because
the right surface for typestate violations is probably the
inspector hint, not a parser error for every author. Listed
for the day someone wants to ship "Prism's grammar is provably
correct."

### 11.2 Computed property defaults (Q2)

`<property name=padding, type=int, computed={row.depth * 4}>`.
Topologically resolved across the property graph; cycle
detection identical to the PRSS extends-chain validator.
Mostly deferred because the recursive-default class of bugs
(Vue's `withDefaults`) makes it easy to get wrong. The
minimal version would require the computed expression to
reference only *declared* properties (no nested computeds),
keeping the topology trivial.

### 11.3 Algebraic effect handlers — beyond capabilities

Capabilities (§6.9) handle host-supplied services
declaratively. The next step is *effect handlers* in the Koka
/ Eff sense — a component can declare it produces an effect
(`yield user-message`) and a parent in the tree provides the
handler:

```prui
<component name=Chat>
  <effect name=user-message, type=string>
  <button on:click=$yield user-message(input.value)>Send</button>
</component>

<component name=ChatRoom>
  <handle effect=user-message as msg>
    {append-to-log(msg); broadcast(msg)}
  </handle>
  <Chat/>
</component>
```

This generalises today's signal dispatch into a typed,
statically-scoped effect system. Performance is a question
(closure allocation per `yield`) but the model is cleaner
than event bubbling. Lean: prototype after Phase 11
(capabilities) lands and we have host-injection plumbing to
extend.

### 11.4 First-class modules — OCaml-style

`prism.component{…}` is a table; the module system could be
too. A `.luau` file returning a `prism.module{…}` could expose
re-exportable, namespace-scoped, parametrically polymorphic
unions of traits / mixins / components. Strongest use case:
library distribution — "import this card system and get the
trait, the mixin, three components, the colour palette, and
the inspector entries in one line." Today's
`<import script>` plus a returned table covers ~70% of this;
first-class modules cover the remaining ergonomics.

### 11.5 Container queries / responsive at the trait layer

CSS container queries (`@container (width > 600px)`) ship in
every browser. PRSS today only has token-table breakpoints.
A trait-based take: `<trait name=Responsive>` could carry
breakpoint-aware methods
(`responsive.gap@desktop=24`, `responsive.gap@mobile=8`).
The pipeline form (§6.6) extends naturally:
`gap = 16 | @desktop → 24 | @mobile → 8`.

### 11.6 Prototype delegation as a mixin alternative

The Self / Io / JS object model: instead of mixin
linearisation, a component delegates to a prototype chain at
lookup time. Cleaner than linearisation for some cases
(override-by-shadowing beats override-by-super-chain when the
shadowing is sparse). Mostly a "did we pick the right
composition primitive" question; mixins are the Scala choice
and have 20 years of battle-testing. Listed because the
prototype model is genuinely simpler to explain and might be
the right move if linearisation surfaces get fiddly.

### 11.7 Hot-swap of mixins / traits at runtime

Today's hot-reload reloads whole component bodies. A future
version could hot-swap a mixin's implementation while keeping
the component instances live — the mixin's state stays put,
the hooks rewire to the new bodies, the render walk reflects
the new behaviour. Useful for live theming, A/B testing of
interaction behaviours, dev-time experimentation. Probably
~Phase 17 in scope.

### 11.8 Effect-typed reactive scope

Today every reactive scope is implicitly "anything signed up
to this signal." An effect-typed extension would type the
*subscriptions* — a reactive cell declares it produces values
of type `T` *and* depends on a set of named effects (`net`,
`time`, `storage`). The render walk could refuse to subscribe
a scope without the matching effect handlers. This is the SSR
side of §11.3 — same shape, applied to the reactive graph
instead of the render tree.

### 11.9 Plug-in lints for the trait surface

`prism-cli` already lints on inline `style:` overrides that
appear N+ times. The trait registry opens richer rules:

- "this component uses the `Draggable` mixin but never reads
  `is-dragging`" → maybe a derive would be cheaper
- "three components implement `Focusable` by hand without
  declaring it" → suggest adding `impls=Focusable`
- "this PRSS class restates a state variant available as a
  named `@color`"
- "trait `Foo` is registered but never used in the workspace"

Each is a 50-line lint rule once Phase 16 (polish) is past.

### 11.10 Tier 3 namespaces — finish or delete

The `Transition` / `Animate` / `At` / `Use` namespaces are
parsed today but their runtime is deferred. Phase 3 forces a
ship-or-cut decision. If "ship eventually," the design these
namespaces *would* expose:

- A unified `Animator` trait carrying transition curves,
  keyframe sets, and entry/exit animations as trait methods.
- Pipeline-shape over time on the value side (`opacity = 0 →
  1 over 200ms`), mirroring the §6.6 state pipeline.
- `at:50%` / `at:200ms` keyframes as nested records inside
  the `Animator` trait, not as a separate namespace.

If "delete," these get re-added on the day they ship —
vestigial grammar attracts drift (same shape as the dead
`widget=`).

### 11.11 Mid-decision forks not yet locked in

These are explicit branches the doc currently presents both
options for. They become hard decisions once their gating
phase arrives:

- The §6.1 A/B/C fork (file-as-component vs markup-decl vs
  Luau-as-decl). Q1 in §8.
- Whether the `component` projection (§6.11) survives once the
  universal Luau-builder family is in place (it might reduce
  to ergonomic sugar for `script` with a file-stem alias).
- Whether `Block` and `Component` (Rust traits) collapse into
  one. Today `Block` is "single-trait sugar" over
  `Component`; if the registry's surface narrows to one trait,
  the name to keep is `Component` (matches the noun).
- Whether `Aria` and `Data` namespaces survive as labels once
  the trait registry can express them generically
  (`a11y.role`, `data.role`). Lean: keep — they read better
  in markup than the generic form.

### 11.12 Things we deliberately won't do

A short list that should NOT happen, even if tempting:

- **Templating languages embedded inside attribute values**
  (Handlebars in `style:background={{lookup color "accent"}}`).
  The brace-expr form is the only expression language;
  unifying around one parser is non-negotiable.
- **A second markup syntax (JSX-style, Pug, anything)** that
  produces the same AST. The XML-shape grammar is the
  surface; multiple surfaces splinter the type story.
- **Class hierarchies with virtual methods.** Single-parent
  `extends` is the inheritance ceiling. Composition (§6.3
  traits, mixins, derives) carries the rest.
- **Macros that introduce new keyword tokens at the lexer
  level.** Hygiene + macro depth limits + readable expansion
  are non-negotiable; a macro that rewires the lexer breaks
  all three.
- **A second closure form** (alongside `|args| expr`).
  Single-form rule (§2, §9). The fusion doc's `\fn(args) … end`
  alternative is rejected.

---

## 12. Closing thought

Waves A–H made PRUI *runnable* — script, macro, dialect,
lifecycle, suspense, animation, multi-projection. **Wave J**
makes it *reusable*: inheritance for the cases composition
can't reach, contracts for the cases nominal-shape would
help, defaults so authoring doesn't reach into the host, and
a PRSS surface as expressive as Tailwind v4 + Sass + CSS
Color 5 combined. **Wave K** makes it *coherent*: the
codebase audit found five paths to register a component,
three ways to repeat children, two spellings for slot
injection, and four attribute namespaces that earn nothing.
**Wave L** makes it *principled*: traits replace the
namespace enum, mixins replace ad-hoc class lists, macros
replace per-attribute parser edits, capabilities replace
magic globals, and nested state records replace the N × M
property-state cross product.

Each duplication is a small papercut on its own; together
they're the reason the DSL feels larger than it is. The
fusion doc's ratchet — every new feature must collapse N
lines to 1 — applies in all three directions. §10 is the
receipts.

The end state is a surface that's more expressive, smaller,
and more open than today's. The 17-namespace enum is gone.
The five registration paths are one. The three facet
implementations are one. The two slot spellings are one. The
two closure forms are one. The four-line state-variant
repetition is one. **Every grammatical concept the language
exposes is justified by user-facing semantics — there are no
mechanism-residues left over from earlier eras.** That's the
destination Wave J + K + L points at. §7 phases the trip in
16 slices; the first three ship in weeks, the last few in
months, and every one is independently shippable. §11 is the
list of things we *could* do after, kept honest so they
don't accrete into the load-bearing design.
