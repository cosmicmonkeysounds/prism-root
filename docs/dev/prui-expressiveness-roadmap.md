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
| Inheritance only via PRSS `extends=`; no markup-level composition | Inline header attrs on the `<component>` wrapper: `extends=Parent`, `impls=[…]`, `derive=[…]`, plus runtime `with=[…]` on use sites |
| Style-modifying behaviours wired by editing the runtime | User-defined attribute macros (`prism.macro{…}`) and traits (`prism.trait{…}`) |
| Host services (clipboard / network / fs) reached via magic globals | Typed `capabilities=[name: Type, …]` inline attr the host provides at lower-time |
| `darken`/`lighten`/`mix` lerp in sRGB → desaturated colours | OKLCH-backed (callers unchanged) + `with(c, l=±, a=…)` channel adjust + slash-alpha (`accent/50`) |
| Component props: anything goes, host decides | Inline typed declarations on the `<component>` wrapper: `name: type [= default \| required]`, including discriminated `union<…>` |
| Six separate declaration tags (`<property>` / `<slot>` / `<extends>` / `<impls>` / `<derive>` / `<capability>` / `<contract>`) for component schema | **Four** declaration wrappers total: `<component>` / `<trait>` / `<mixin>` / `<macro>`. All inline-typed header attrs, no sub-tags, no `name=`. PascalCase first-positional token = the declaration name. |
| One component per `.prui` file (file-as-component flat) | Multi-component files first-class — any number of top-level `<component>` / `<trait>` / `<mixin>` / `<macro>` wrappers per file; `.luau` files multi-export via returned table mixing components, traits, mixins, macros, helpers |
| Two closure forms (`\|args\| expr` and `\fn(args)…end`) | One: `\|args\| expr` only (see §2, §9) |
| Each authoring surface (`.prui` / Luau / Rust / `prism-luau-derive`) emits *its own* downstream artifacts — Luau stubs only auto-flow from PRUI (Wave I), Rust typed handles never auto-flow at all, PRSS isn't callable from Luau or Rust typed-ly | **Schema-first unification** — one canonical `ComponentSchema` / `TraitSchema` per artifact, all surfaces auto-generated from it. Declare in PRUI → Rust gets typed handles + Luau gets stubs for free; declare in Luau → Rust + PRUI get them; declare in Rust via `prism-luau-derive` → Luau + PRUI get them. (§6.14) |
| Component invocation: `<component>` markup wrapper (alias for `<container>`); shell tags `<shell.icon-button/>` are mixed-case dotted; "is this a primitive or a user component" requires looking at the docs | **PascalCase rule (React / Vue convention)** — `<Card/>` / `<CustomForm/>` / `<AppWindow/>` are components by their PascalCase name; `<container/>` / `<text/>` / `<slot/>` are primitives by their lowercase name. The `<component>` markup tag retires entirely. (§3, §6.1 Part 1) |

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
`BlockSpec` row declares in Rust. **Same artifact, different
authoring surfaces.** Prism Studio shows them as "components"
in its UI; this doc keeps that convention.

### Authoring surfaces — three input forms, one noun, one call site

| You write | You call it | Tag at the call site |
|---|---|---|
| A `.prui` file with one or more `<component Name …>` wrappers | a component (markup decl) | `<Name/>` — file-stem-PascalCased if single-component, or `<ns.Name/>` if imported `as ns` for multi-component files |
| `prism.component{name="Name", …}` in a `.luau` file (single or via a returned table) | a component (Luau decl) | `<Name/>` (with optional `as` namespace) |
| `BlockSpec::new("Name", …)` row in Rust | a component (Rust decl) | the spec id: `<Name/>` (PascalCase) or `<button/>` (lowercase, when overriding a primitive name) |

All three produce the *same noun*. The `ComponentRegistry`
sees a `Component` (the trait); the resolver looks up the
PascalCase tag and invokes `Component::lower_ui`. Differences
are *where the author types* and *what shape the source has* —
not what the runtime registers.

The internal Rust-side variants (`WidgetContribution` +
`CoreWidgetBlock` for engine-shipped components; `PrefabDef` +
`PrefabComponent` for user compounds; `LuauComponent` for
derive-emitted) are *bridges from a specific authoring shape to
the universal schema*, not separate authoring surfaces at the
language layer. §6.14 (schema unification) makes the bridge
shape explicit and folds `prism-luau-derive` into one
schema-codegen pipeline that produces all consumer artifacts.

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

### Components are invoked by name — the PascalCase rule (React / Vue)

End state: **the component's name *is* the tag**. There is no
`<component>` markup tag. The PascalCase / lowercase split
disambiguates:

- **PascalCase tag** (`<Card/>`, `<CustomForm>…</CustomForm>`,
  `<AppWindow>…</AppWindow>`) → component invocation, looked up
  by name in the `ComponentRegistry`.
- **lowercase tag** (`<container>`, `<text>`, `<heading>`,
  `<input>`, `<image>`, `<slot>`, etc.) → built-in primitive or
  HTML pass-through.

This is the React / Vue convention. Mature ecosystems converged
on it for a reason — the visual cue (capital first letter)
makes "this is a user-defined thing" obvious at the call site,
and the parser dispatch becomes one branch: "first character
uppercase? → registry lookup; else → primitive table."

Today's `<component name=Foo>` markup wrapper retires entirely
(see §6.13). The declaration site moves to one of the three
authoring surfaces in §6.1; all three register `Foo` so that
`<Foo/>` works as the call.

### The unified declaration syntax — four wrappers, no `name=`

Every reusable artifact in the language is declared with one of
**four lowercase wrapper tags**. The wrapper's first positional
token is the PascalCase name. The attribute list carries
typed props, slots, capabilities, extends, impls, derive — all
inline, no `<property>` / `<slot>` / `<extends>` / etc.
sub-tags needed:

```prui
<component Card                              <!-- declaration wrapper -->
  title:    string  required,                <!-- inline typed prop -->
  tone:     Tone    = default,               <!-- prop with default -->
  on-click: action,
  children: slot,                            <!-- slot is a typed prop -->
  
  extends      = BaseCard,                   <!-- single-parent inheritance -->
  impls        = [Pointable, Focusable],     <!-- trait conformance -->
  derive       = [Draggable],                <!-- parse-time mixin expansion -->
  capabilities = [clipboard: Clipboard,      <!-- typed host services -->
                  network: Network optional]>

  <container with=[Hoverable], on:click=$on-click()>
    <heading level=3>{title}</heading>
    <slot/>                                  <!-- default slot invocation -->
  </container>
</component>
```

The four wrappers and their roles:

| Wrapper | Role | Body | First positional |
|---|---|---|---|
| `<component Name attrs>…body…</component>` | UI component | render template | PascalCase component name |
| `<trait Name attrs/>` (or with state body) | typed shape (also serves as contract for slot matching) | optional state/method list | PascalCase trait name |
| `<mixin Name>…body…</mixin>` | composable behaviour (state + hooks + styles) | `<state>` / `<on>` / `<style>` decls | PascalCase mixin name |
| `<macro Name>…body…</macro>` | parse-time markup expansion | `<match>` / `<expand>` pair | PascalCase macro name |

That's the entire declaration surface. **No `name=`
attribute** appears in any declaration — the PascalCase first
token IS the name, mirroring the call-site rule above. No
separate `<property>`, `<extends>`, `<impls>`, `<derive>`,
`<capability>`, `<contract>` tags exist — those concepts are
*inline attribute syntax*, not nested elements.

### Inline attribute syntax — props, slots, capabilities

The header of a declaration wrapper uses **typed attribute
syntax**: `key: type` for declarations (with optional default
or `required` marker), and `key = value` for plain attributes
(extends, impls, derive). The two forms read distinctly:

```prui
<component Toast
  title:    string  required,        <!-- typed prop, required -->
  duration: int     = 2000,          <!-- typed prop, with default -->
  on-dismiss: action,                <!-- typed callback prop -->
  
  tone: union<                       <!-- discriminated-union prop -->
    info,
    success { count: int = 1 },
    error   { retry: action? }
  >,
  
  header: slot,                                                   <!-- untyped slot prop -->
  body:   slot<(item: Task, index: int) -> ui>,                   <!-- typed slot prop -->
  
  extends      = BaseToast,                                        <!-- plain attr -->
  impls        = [Dismissable, Animated],
  derive       = [AutoTimeout],
  capabilities = [clock: Clock, sound: Sound optional]>
  …body…
</component>
```

The rule: `key: type [= default | required]` declares a typed
prop (or slot or capability — distinguished by the type
position); `key = value` is a plain attribute the wrapper
interprets specially. This is the same shape Rust's struct
field syntax uses, and the same shape function-parameter lists
take in most languages.

### How declarations look in a `.prui` file

A `.prui` file may contain **one or more** top-level
declaration wrappers. Single-component files are the common
case; multi-component files bundle a related set (a form-field
family, a chart system) into one file:

```prui
<!-- ./forms.prui — registers <forms.TextField/> and <forms.DropdownField/>
     (when imported as `as forms`; see §6.11 for namespace rules) -->

<component TextField
  label: string required,
  value: string = "",
  on-change: action<string>>

  <container direction=column gap=4>
    <text class=label>{label}</text>
    <input value={value} on:change=$on-change(_)/>
  </container>
</component>

<component DropdownField
  label:    string         required,
  options:  array<string>  required,
  selected: string         = "",
  on-select: action<string>>

  <container direction=column gap=4>
    <text class=label>{label}</text>
    <select value={selected} on:change=$on-select(_)>
      <fragment for={opt in options}>
        <option value={opt}>{opt}</option>
      </fragment>
    </select>
  </container>
</component>
```

When the file holds only one component, the file stem
PascalCase-normalises to the tag (`card.prui` → `<Card/>`).
When it holds multiple, the `<import component=…/> as Ns`
namespace prefixes them (`<forms.TextField/>`,
`<forms.DropdownField/>`). See §6.11 for resolution rules.

### Names in the codebase that aren't the same thing

- **`Component` trait** (`prism-builder/src/component.rs`) —
  the Rust trait every registered component impls. The
  `Component::lower_ui` method is the render contract.
- **`ComponentRegistry`** (`prism-builder/src/registry.rs`) —
  the Rust struct that holds registered components and
  dispatches tag lookups through a `TagResolver`.

### Reading rule

- Lowercase "component" without backticks → the noun.
- `<PascalCase/>` in backticks → an invocation of the named
  component (`<Card/>`, `<CustomForm/>`).
- `<lowercase/>` in backticks → a primitive element
  (`<container/>`, `<slot/>`).
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
| `<component>` semantics | **alias for `<container>`** — one match arm: `"container" \| "component" => { … }`; no declaration semantics today. **Retires entirely in Phase 6** (§6.13); components are invoked by PascalCase tag (§3, §6.1). | `interpret.rs:1739` |
| Tag dispatch — PascalCase rule | Not enforced today; unknown tags fall through to a `TagResolver` regardless of case. Mixed-case shell tags (`<shell.icon-button/>`) use dot-namespacing. | `interpret.rs:1929-1948` |
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

### 5.1 Authoring and invoking a component

Three authoring surfaces (`.prui` / Luau / Rust). One
call-site convention: **PascalCase tag = component invocation;
lowercase tag = primitive** (React / Vue rule). One unified
declaration shape across all surfaces.

```prui
<!-- ./button.prui — registers <Button/> -->
<component Button
  label: string  required,
  tone:  union<default, primary, danger> = default,
  on-click: action,
  impls = [Pointable, Focusable]>

  <container with=[Pointable, Focusable] on:click=$on-click()>
    <text>{label}</text>
  </container>
</component>
```

```luau
-- ./icon-system.luau — multi-export: components, traits, helpers
return {
  Icon = prism.component {
    name = "Icon",
    props = {
      glyph = { type = "string", required = true },
      size  = { type = "int",    default  = 16 },
    },
    render = |props| prui [[
      <image src={"icon:" .. props.glyph} width={props.size}/>
    ]],
  },
  Avatar = prism.component { name = "Avatar", … },
  helpers = {
    icon-url = |glyph| "https://cdn.icons/" .. glyph .. ".svg",
  },
}
```

```rust
// In starter.rs — Rust-authored component; registers <Icon/>
const ICON_SPEC: BlockSpec = BlockSpec::new("Icon", icon_schema)
    .lower(icon_lower)
    .help("builder.components.icon", "Icon", "…");
```

Use sites — same call shape, regardless of which surface
authored the component:

```prui
<import component="./icon-system.luau"/> as icons

<container direction=column gap=8>
  <Button label="Save" tone=primary on-click=$save()/>
  <icons.Icon glyph="check" size=24/>
  <text>{icons.helpers.icon-url("check")}</text>
</container>
```

There is no `<component>` markup wrapper at the call site —
declaration lives in the file head (via `<component Name …>`),
Luau builder, or Rust spec. Invocation is always the PascalCase
name. All three surfaces register into the same
`ComponentRegistry` and resolve through one `TagResolver` —
`<Button/>` reads identically regardless of which surface
authored its body.

**See §6.1** for declaration syntax + the multi-component-file
question. **§6.2** for property declarations. **§6.3** for
inheritance, contracts, traits, mixins, derives. **§6.11** for
the import family. **§6.14** for the schema-first cross-
language unification that makes typed handles flow
automatically between PRUI, Luau, and Rust.

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
<!-- ./list.prui — declares <List/> -->
<component List
  items:  array<Task> required,
  row:    slot<(item: Task, index: int) -> ui>,
  empty:  slot<() -> ui> = { <text>No tasks yet</text> }>

  <container if={#items > 0}>
    <fragment for={t, i in items}>
      <slot row item={t} index={i}/>
    </fragment>
  </container>
  <slot empty if={#items == 0}/>
</component>
```

```prui
<!-- caller -->
<List items={tasks}>
  <slot row args={item, index}>
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

The fusion `<import>` family stays; multi-component files and
mixed-kind Luau tables make one projection cover many
extension kinds.

```prui
<import stylesheet="./theme.prss"/>
<import script="./helpers.luau"/> as h
<import component="./card.prui"/>              <!-- single component file -->
<import component="./forms.prui"/> as forms    <!-- multi-component .prui — namespaced -->
<import component="./icon-system.luau"/> as i  <!-- Luau returning a table of components + helpers -->
<import script="./traits.luau"/> as t          <!-- Luau returning traits / mixins / macros etc. -->
```

Same `<import>` element, same `ImportResolver`, same
`as`-namespacing, same module cache. The Luau side has one
builder family covering five extension kinds — return a single
value or a table mixing any of:

```luau
prism.trait     {…}   -- typed shape (also serves as contract)
prism.mixin     {…}   -- composable behaviour (state + hooks)
prism.macro     {…}   -- parse-time markup expansion
prism.component {…}   -- a component definition
prism.dialect   {…}   -- embedded sub-language (Wave E)
-- plus bare functions / values as helpers — anything Lua-side
```

**See §6.11** for the full import-family design (multi-component
files, projection-as-lens, multi-export tables); **§6.4** for
trait registration; **§6.8** for macros.

### 5.6 Mixins, macros, derive — two declaration tags, two use modes

Two declaration wrappers cover behaviour composition + grammar
extension:

- **`<mixin>`** — composable behaviour (state + hooks + styles).
  Applied at one of two call sites: `with=[…]` at any element
  (runtime composition, linearised chain, `super()`-overridable)
  or `derive=[…]` on a `<component>` header (parse-time
  expansion, inlined, cheaper at runtime).
- **`<macro>`** — parse-time markup expansion. Pattern-match
  tags or attributes; expand to other tags/attributes.

```prui
<mixin Hoverable>
  <state is-hovered = false>
  <on pointerenter>{ is-hovered = true }</on>
  <on pointerleave>{ is-hovered = false }</on>
</mixin>

<mixin Draggable>
  <state is-dragging = false>
  <state drag-offset = (0, 0)>
  <on pointerdown>{ is-dragging = true; drag-offset = (e.x - self.x, e.y - self.y) }</on>
  <on pointermove if=is-dragging>{ self.x = e.x - drag-offset.x; self.y = e.y - drag-offset.y }</on>
  <on pointerup>{ is-dragging = false }</on>
</mixin>

<macro Field>
  <match><Field label={lbl} value={val}/></match>
  <expand>
    <container direction=column gap=4>
      <text class=field-label>{lbl}</text>
      <input value={val}/>
    </container>
  </expand>
</macro>

<!-- Runtime composition on a use site -->
<container with=[Hoverable, Draggable]>…</container>

<!-- Parse-time expansion in a component declaration -->
<component Card derive=[Draggable], title: string>
  <container>
    <heading>{title}</heading>
  </container>
</component>

<!-- Macro expansion at the call site -->
<Field label="Title" value={state.title}/>
```

**See §6.3** for mixin / derive semantics; **§6.8** for macros.

### 5.7 Capabilities — typed host services

Components declare what host services they need; the host
provides them at lower-time; missing capabilities fail at
parse, not at runtime. No magic globals.

```prui
<!-- ./share-button.prui — declares <ShareButton/> -->
<component ShareButton
  text: string required,
  capabilities = [
    clipboard: Clipboard,
    network:   Network optional,
  ]>

  <button on:click=$clipboard.write(text)>Copy</button>
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
<!-- ./toast.prui — declares <Toast/> -->
<component Toast
  tone: union<
    info,
    success { duration: int = 2000 },
    error   { dismissable: bool = true, retry: action? }
  > required>

  <match on={tone}>
    <case info>           …                                            </case>
    <case success(d)>     <progress duration={d}/> …                   </case>
    <case error(dis, r)>  … <button if={r != nil} on:click={r}>Retry</button> </case>
  </match>
</component>
```

```prui
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

### 6.1 Components — the unified declaration syntax

**Problem.** Five distinct registration paths for what is
conceptually one noun (see §3 Terminology, §4 snapshot). The
PRUI `<component>` markup tag is a silent alias for
`<container>` today and has no declaration semantics
(`interpret.rs:1739`). Authors don't know which surface to
write to, and the call-site syntax (`<shell.icon-button/>`,
`<facet/>`, mixed-case ad-hoc) has no consistent rule. The
earlier `<property name=…>` / `<extends>` / `<impls>` /
`<derive>` / `<capability>` / `<contract>` sub-tag scheme
ballooned six declaration tags where one declaration *header*
would do.

**Design — three parts.**

#### Part 1: Call-site invocation — the PascalCase rule

**Components are invoked by their name.** First character
disambiguates:

- **PascalCase** (`<Card/>`, `<AppWindow/>`,
  `<CustomForm>…</CustomForm>`) → component invocation;
  resolves via the `ComponentRegistry`'s `TagResolver`.
- **lowercase** (`<container/>`, `<text/>`, `<slot/>`,
  `<input/>`) → built-in primitive or HTML pass-through.

Same convention as React / Vue / Svelte / SolidJS. The
`<component>`-as-`<container>`-alias retires. Existing shell
tags rename from `<shell.icon-button/>` to `<ShellIconButton/>`
or `<IconButton/>` under a namespace; mechanical migration.

#### Part 2: Declaration — the four-wrapper syntax with inline attributes

A declaration is a **lowercase wrapper tag** (`<component>`,
`<trait>`, `<mixin>`, `<macro>`) whose **first positional
token is the PascalCase name** and whose **attribute list
inlines props / slots / capabilities / extends / impls /
derive** using typed syntax. No `name=` attribute. No
`<property>` / `<slot>` / `<extends>` / `<impls>` /
`<derive>` / `<capability>` / `<contract>` sub-tags.

```prui
<!-- ./card.prui — registers <Card/> -->
<component Card
  title:    string  required,
  subtitle: string  = "",
  tone:     union<default, primary, danger> = default,
  padding:  int     = 16,
  on-click: action,
  children: slot,

  extends      = BaseCard,
  impls        = [Pointable, Focusable],
  derive       = [Draggable],
  capabilities = [clipboard: Clipboard]>

  <container with=[Hoverable], padding={padding}, on:click=$on-click()>
    <heading level=3>{title}</heading>
    <text if={subtitle != ""}>{subtitle}</text>
    <slot/>
  </container>
</component>
```

The header reads as a **typed function signature** — what the
component takes, what it conforms to, what behaviour it
composes. The body is the render template.

**Attribute syntax — two forms, distinct at a glance:**

- **`key: type`** (or `key: type = default`, `key: type required`)
  → declares a typed prop / slot / capability. The type
  position disambiguates which kind: `string` / `int` / etc.
  is a value prop; `slot` or `slot<sig>` is a slot prop; a
  capability type (`Clipboard`, `Network`) inside the
  `capabilities=[…]` list is a capability.
- **`key = value`** → plain attribute the wrapper interprets
  specially (`extends=`, `impls=`, `derive=`, `capabilities=`).

The same rule applies to all four declaration wrappers
(§6.3 covers `<trait>` / `<mixin>` / `<macro>`).

#### Part 3: Authoring surface — file / Luau / Rust

Three surfaces produce the declaration. The wrapper above is
the **`.prui` form**; the other two mirror it 1:1.

**`.prui` file (`<component>` wrapper):** As above. One or
more wrappers per file (multi-component files first-class —
§6.11 for namespace rules).

**Luau (`prism.component{…}`):** Returned from a `.luau` file
imported as `component` or `script`. The Luau table mirrors
the PRUI header structure:

```luau
return prism.component {
  name = "Card",
  props = {
    title    = { type = "string",  required = true },
    subtitle = { type = "string",  default  = "" },
    tone     = { type = "union<default, primary, danger>", default = "default" },
    padding  = { type = "int",     default  = 16 },
    on-click = { type = "action" },
    children = { type = "slot" },
  },
  extends      = "BaseCard",
  impls        = { "Pointable", "Focusable" },
  derive       = { "Draggable" },
  capabilities = { clipboard = "Clipboard" },
  render       = |props| prui [[
    <container with=[Hoverable], padding={props.padding}, on:click=$props.on-click()>
      <heading level=3>{props.title}</heading>
      <slot/>
    </container>
  ]],
}
```

Multi-export from a `.luau` file = return a table of named
entries (§6.11):

```luau
return {
  Card  = prism.component { name = "Card",  … },
  Field = prism.macro     { name = "Field", … },
  Pointable = prism.trait { name = "Pointable", … },
  helpers   = {                            -- bare functions are fine too
    format-date = |t| os.date("%Y-%m-%d", t),
  },
}
```

**Rust `BlockSpec`:** A const spec row registers the tag.
PascalCase id = component tag; lowercase id = primitive
override (used for the 17 starter built-ins).

```rust
const CARD_SPEC: BlockSpec = BlockSpec::new("Card", card_schema)
    .lower(card_lower)
    .help("builder.components.card", "Card", "…");
```

All three flow into one `Component` trait impl in the
`ComponentRegistry`. The `Block` / `BlockSpec` /
`CoreWidgetBlock` / `PrefabComponent` / `LuauComponent`
Rust-side types stay as source-level clarity but are
*invisible* at the authoring layer (§3, §6.14 schema
unification).

#### Multi-component files

A `.prui` file may contain **any number of top-level
`<component>` wrappers** (also `<trait>` / `<mixin>` /
`<macro>`). The user is no longer constrained to one
component per file:

```prui
<!-- ./forms.prui — three components in one file -->
<component TextField …>…</component>
<component DropdownField …>…</component>
<component DateField …>…</component>

<!-- and supporting traits / mixins, also in the same file -->
<trait FieldLike, label: string, value: any/>
<mixin FieldValidation>…</mixin>
```

Import + namespace rules (§6.11):

- `<import component="./card.prui"/>` — single component,
  registers under file stem PascalCased (`<Card/>`).
- `<import component="./forms.prui"/> as forms` — multi-component,
  namespaced (`<forms.TextField/>`, `<forms.DropdownField/>`).
- `<import component="./forms.prui"/>` (no `as=`) — bare
  imports each component into scope; collision = parse error.

#### Component invocation — passing props, hooks, slots

The call site uses **plain attribute syntax** (no `:` types
— types belong to the declaration). Slots are passed inline
as nested `<slot>` providers; default-slot children stream into
the unnamed slot:

```prui
<Card
  title="Welcome"
  subtitle="Read the intro below"
  tone=primary
  padding=24
  on-click=$open-intro()>

  <!-- streams into the default `children: slot` -->
  <text>Some body content here.</text>

  <!-- if Card declared additional named slots, e.g. `footer: slot`: -->
  <slot footer>
    <text>— signed, the team</text>
  </slot>
</Card>
```

Slot providers can destructure args from typed slots:

```prui
<List items={tasks}>
  <slot row args={item, index}>
    <text>{index + 1}. {item.title}</text>
  </slot>
  <slot empty>
    <text>No tasks yet.</text>
  </slot>
</List>
```

§6.5 covers the slot rules in detail.

**Wave / Phase.** Phase 6 (the unified declaration syntax +
inline prop / extends / impls / derive / capabilities).

**What it deletes / supersedes.** The `<component>`-as-`<container>`
alias (silently overlapping). The `<property>`, `<extends>`,
`<impls>`, `<derive>`, `<capability>`, `<contract>` sub-tag
proposals (collapsed into header attributes). The
`<shell.icon-button/>` dot-namespacing convention (PascalCase
rename). The `name=` attribute on every declaration kind
(replaced by PascalCase first-positional token).

**Open questions.** Q1 (collision handling for bare
multi-component imports); Q6 (PascalCase-normalisation edge
cases for filenames); Q7 (`.luau` returning mixed
components + helpers + macros — handled, §6.11).

---

### 6.2 Properties — the type set, defaults, discriminated unions

**Problem.** Today's component prop story is "anything goes;
the host binding decides." Defaults live in the host;
`required` is unenforced; type-checks rely on the external
`luau-analyze` pipeline (Wave I) instead of the parser.
Variant-rich props (`tone = {info | success | error}`)
degrade to boolean ladders.

**Design.** Properties are declared inline in the `<component>`
wrapper header (§6.1), using the typed-attribute syntax
`name: type [= default | required]`. This section covers the
**type set** and the **discriminated-union** case in detail.

#### The type set (closed)

| Type | Example | Notes |
|---|---|---|
| `string` | `label: string required` | Lua-side typed `string` |
| `int` / `number` | `padding: int = 16` | typed integer / float |
| `bool` | `disabled: bool = false` | |
| `color` | `tint: color` | hex / token / OKLCH-helper value |
| `length` | `radius: length = 8` | px / em / token unit |
| `action` | `on-click: action` | callback type, `$expr` at call site |
| `action<T>` | `on-change: action<string>` | callback with typed arg |
| `enum<a\|b\|c>` | `tone: enum<default\|primary\|danger>` | flat enum |
| `union<…>` | (see below) | discriminated union with variant fields |
| `array<T>` | `items: array<Task>` | typed array |
| `object<{…}>` | `style: object<{bg: color}>` | typed object |
| `slot` / `slot<sig>` | `children: slot`, `row: slot<(item: Task) -> ui>` | slot prop (§6.5) |
| `<TraitName>` | `field: Focusable` | structural match against a trait |

The set is closed so the parser-side type-check stays static.

#### Defaults and `required`

- `key: type = literal` — default value. Must be a **literal**
  (string / number / bool / token / enum case / `nil`), not a
  `{…}` expression. Avoids the recursive-default class of bugs
  from Vue's `withDefaults`. Derived defaults compute in the
  body: `padding={props.padding ?? row.depth * 4}`.
- `key: type required` — required, no default. Missing at call
  site raises a contribution error at lower-time.
- `key: type` — optional, default is the type's zero
  (`""`, `0`, `false`, `nil`).

#### Discriminated unions — props that carry variant fields

The `union<…>` type names a set of variants, each optionally
carrying fields. The body pattern-matches via the existing
`<match>` primitive extended with destructure-binding:

```prui
<component Toast
  tone: union<
    info,
    success { duration: int = 2000 },
    error   { dismissable: bool = true, retry: action? }
  > required>

  <match on={tone}>
    <case info>           …                                              </case>
    <case success(d)>     <progress duration={d}/> …                     </case>
    <case error(dis, r)>  … <button if={r != nil} on:click={r}>Retry</button> </case>
  </match>
</component>

<!-- caller -->
<Toast tone={error(dismissable=true, retry=$retry-upload)}/>
<Toast tone={info}/>
<Toast tone={success}/>            <!-- duration uses the variant default -->
```

The property panel auto-generates a variant picker + per-variant
sub-form. The Luau type-stub generator emits a tagged union the
analyzer narrows inside `if props.tone.kind == "error" then …`.
The "boolean prop ladder" anti-pattern (`is-success` +
`is-error` + `is-info`) becomes impossible to express.

**Wave / Phase.** Inline prop declarations + `required` +
literal defaults: Phase 6 (with the §6.1 unified syntax).
Discriminated unions + `<case Variant(fields)>` destructure:
Phase 12 (depends on the trait registry being live for the
inspector + Luau-narrowing integration).

**What it deletes / supersedes.** The `<property name=…>`
sub-tag idea (collapsed into header attributes, §6.1); the
boolean-prop-ladder anti-pattern; the "host knows the schema"
implicit coupling.

**Open questions.** Q2 (computed defaults — deferred to §11.2).

---

### 6.3 Composition — extends, traits, mixins (three primitives, two declaration tags)

**Problem.** "Two components identical except for one
override" has no markup-level expression today; the only
escape is a shared PRSS class. "This slot accepts anything
`Focusable`" has no language-level expression. Stacking
behaviours (drag + hover + select + tooltip-host) onto a
container requires hand-rolling state + handlers + styles in
every component that wants the bundle.

**Design.** Three composition primitives over **two
declaration tags** (`<trait>` and `<mixin>`) plus one
component-header attribute (`extends=`). The earlier separate
`<contract>` and `<derive>` declaration kinds collapse — a
contract is a trait with no implementation, and a derive is a
mixin applied at parse time via the `derive=[…]` attribute
instead of `with=[…]` at runtime.

#### Inheritance — `extends=Parent` attribute

Shallow, single-parent. The child component's body is a
*patch* of the parent's; the child header may add or override
typed props, slots, and add an optional inline `<style>` block.
For structurally different bodies, *compose* with `<Parent>` as
a child element instead.

```prui
<!-- ./base-button.prui -->
<component BaseButton
  label: string,
  tone:  enum<default|primary|danger> = default>

  <container tag=button class=[btn, tone:{tone}]>
    <text>{label}</text>
  </container>
</component>

<!-- ./danger-button.prui -->
<component DangerButton
  extends = BaseButton,
  tone:     enum<default|primary|danger> = danger>
</component>
```

No multi-inheritance, no diamond. Multi-axis variation goes
through props (`tone`, `size`), mixins, or derives (below) —
never multiple `extends=`. Parse-time flattening.

#### Traits — typed shape (also serves as contract)

A trait is a **typed shape**: a name + a set of typed methods
+ optional typed state. With **no body**, it serves the
"contract" role (slot matching, structural conformance). With
a body holding `<state>` declarations, it also tracks per-impl
state — and that state appears in the inspector and LSP
completions when a component `impls` the trait.

```prui
<trait Pointable
  on-click:   action,
  on-hover:   action,
  is-hovered: bool = false/>           <!-- self-closing — pure shape -->

<trait Focusable
  focused: bool,
  focus:   action,
  blur:    action/>                    <!-- contract-like -->
```

Use as **contract** (slot accepts a shape):

```prui
<component FormField
  control: slot<() -> Focusable>>      <!-- slot prop's return type IS the contract -->
  <slot control/>
</component>
```

Use as **attribute vocabulary** (component impls the trait):

```prui
<component Button impls = [Pointable, Focusable]>
  <container>
    …body reads pointer.is-hovered, focusable.focused freely…
  </container>
</component>
```

Coherence is Rust-style: two traits with the same method name
on the same component is a parse-time error unless the author
disambiguates with `<trait-alias from=A.on-click, as=primary-click>`.

#### Mixins — composable behaviour (state + hooks + styles)

A mixin is a **trait that supplies implementation**.
Declaration body uses `<state>`, `<on>`, and `<style>`
primitives (lowercase body tags):

```prui
<mixin Hoverable>
  <state is-hovered = false>
  <on pointerenter>{ is-hovered = true }</on>
  <on pointerleave>{ is-hovered = false }</on>
  <style>&:hovered { background = lighten(currentBg, 0.05) }</style>
</mixin>

<mixin Draggable>
  <state is-dragging = false>
  <state drag-offset = (0, 0)>
  <on pointerdown>{ is-dragging = true; drag-offset = (e.x - self.x, e.y - self.y) }</on>
  <on pointermove if=is-dragging>{ self.x = e.x - drag-offset.x; self.y = e.y - drag-offset.y }</on>
  <on pointerup>{ is-dragging = false }</on>
</mixin>
```

A mixin is applied at one of **two call sites** — the
distinction is *when*, not *what*:

| Call site | Attribute | When applied | Overridable downstream? | Cost |
|---|---|---|---|---|
| Use site (any element) | `with = [Mixin, …]` | runtime — composed as a chain | yes, via `super()` | one indirect call per mixin per event |
| Component header | `derive = [Mixin, …]` | parse-time — inlined into the declaration | no — flattened away | zero (state and hooks become part of the host component) |

Same `<mixin>` declaration, two activation modes. There is no
separate `<derive>` declaration tag. The use site picks.

```prui
<!-- Runtime composition: chain Hoverable + Draggable on this container -->
<container with=[Hoverable, Draggable]>
  <text>I float and glow.</text>
</container>

<!-- Parse-time expansion: bake Draggable into Card's declaration -->
<component Card derive=[Draggable], title: string>
  <container>
    <heading>{title}</heading>
  </container>
</component>
```

`super()` in a runtime-mixin chain calls the next-in-
linearisation handler (Scala-style MRO).

#### Body-tag primitives used by mixin / component declarations

Three lowercase tags are valid only inside declaration bodies:

| Tag | Where | Shape |
|---|---|---|
| `<state name [: type] = default>` | inside `<mixin>` and `<component>` | declares per-impl reactive state |
| `<on event [if=cond]>{handler}</on>` | inside `<mixin>` and `<component>` | event hook; chains via `super()` in mixin context |
| `<style>{ … PRSS … }</style>` | inside `<mixin>` and `<component>` | scoped PRSS that applies only to this impl |

These three are the *only* sub-tags needed for composition —
everything else lives in header attributes (§6.1).

#### Choosing among the three primitives

| Need | Use | Cost | Overridable downstream? |
|---|---|---|---|
| Variant of a parent with one override | `extends=Parent` header attr | parse-time | no (single parent) |
| Slot must accept components of a certain shape | `<trait>` (no body) + `slot<() -> TraitName>` | parse-time | n/a |
| Open vocabulary of attributes on a component | `<trait>` + `impls=[Trait, …]` header attr | runtime dispatch | n/a |
| Composable behaviour, stackable across components | `<mixin>` + `with=[Mixin, …]` on use site | runtime chain + indirect call per event | yes (super()) |
| Composable behaviour, no runtime chain needed | `<mixin>` + `derive=[Mixin, …]` on component header | parse-time | no |

Two declaration tags (`<trait>`, `<mixin>`) cover five
composition needs.

**Wave / Phase.** `extends`: Phase 6 (lands with the unified
syntax). Traits + mixins (declaration + `impls=` + `with=` +
`derive=` attributes): Phase 8 / Phase 9 (depend on the trait
registry being live).

**What it deletes / supersedes.** The earlier separate
`<contract>` declaration tag (collapses into empty-body
`<trait>`). The earlier separate `<derive>` declaration tag
(collapses into `<mixin>` + use-site `derive=` attribute). The
`<extends file=…/>` head sub-tag (becomes `extends=` attr).
The `<impls traits=[…]/>` head sub-tag (becomes `impls=`
attr). Wave J §4.5 PRSS `@mixin` (subsumed by §6.8 macros + a
`<mixin>`'s embedded `<style>`). Wave J §4.8 variant prefixes
(`hover:elevated` becomes `with=[Hoverable, Elevated]`).

**Open questions.** Q5 (trait self-reference); Q9 (variant
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

### 6.5 Slots — declared as typed props, invoked in body, provided by caller

**Problem.** Two spellings for "splice caller content here":
`<slot/>` (`interpret.rs:1848`) and `<host-children/>`
(`:1909`). Comment at `:1865-1867` admits the equivalence. The
1 / 9 production split (slot / host-children) reflects history,
not intent. Typed slot scopes (the React render-prop / Svelte 5
snippet pattern) have no language surface today.

**Design.** Slots are **typed props** — declared in the
`<component>` header as `name: slot` or `name: slot<sig>`
(§6.1, §6.2). The `<slot>` body primitive plays three roles,
disambiguated by position:

| Position | Role | Shape |
|---|---|---|
| Inside a component body, no name | invoke the default slot (caller's unnamed children) | `<slot/>` |
| Inside a component body, named, no args | invoke a named slot | `<slot name/>` |
| Inside a component body, named, with args | invoke a typed slot with caller-provided scope | `<slot name arg1={…} arg2={…}/>` |
| Inside a component invocation (caller) | provide content for a named slot | `<slot name args={a, b}>…content…</slot>` |

One tag, position disambiguates role — no extra keyword
needed. There is no `<invoke>` tag; slot invocation is just
`<slot name args/>`.

#### End-to-end example

```prui
<!-- ./list.prui — declares <List/> with three slot props -->
<component List
  items:  array<Task> required,
  row:    slot<(item: Task, index: int) -> ui>,
  header: slot,                                            <!-- untyped, no args -->
  empty:  slot<() -> ui> = { <text>No tasks yet.</text> }> <!-- with default body -->

  <container direction=column>
    <slot header/>                                         <!-- invoke header -->

    <fragment for={t, i in items} if={#items > 0}>
      <slot row item={t} index={i}/>                       <!-- invoke row with args -->
    </fragment>

    <slot empty if={#items == 0}/>                         <!-- invoke empty (uses default if omitted) -->
  </container>
</component>
```

Caller:

```prui
<List items={tasks}>
  <slot header>
    <heading level=2>Today's tasks</heading>
  </slot>
  <slot row args={item, index}>
    <text>{index + 1}. {item.title}</text>
  </slot>
  <!-- empty slot omitted — List's default body fires when items is empty -->
</List>
```

#### Default slot — the caller's unnamed children

A component that wants to splice the caller's *unnamed* child
nodes declares `children: slot` and invokes `<slot/>` (no
name):

```prui
<component Card, title: string, children: slot>
  <container>
    <heading>{title}</heading>
    <slot/>                              <!-- caller's <Card>…here…</Card> children -->
  </container>
</component>

<Card title="Hello">
  <text>Streamed into the default slot.</text>
</Card>
```

This subsumes today's `<host-children/>` — `<slot/>` (no name)
IS what `<host-children/>` did, just spelled as part of the
unified `<slot>` primitive.

#### Slot defaults — body in the declaration

A slot prop carries a default body via the `= { …markup… }`
notation, mirroring scalar defaults (§6.2):

```prui
empty: slot<() -> ui> = { <text>No items</text> }
```

If the caller omits the slot, the default body runs at the
invocation site. Same shape as scalar defaults — `= literal`
for scalars, `= { …markup… }` for slots.

#### `<host-children/>` retires

Mechanical rewrite of 9 production files (one PR). Deprecation
diagnostic for one release; deletion after.

**Wave / Phase.** Phase 4 (unify `<slot>` / `<host-children/>`,
default-slot variant). Phase 13 (typed signatures + slot
default bodies; depends on the trait registry being live).

**What it deletes / supersedes.** `<host-children/>` element;
`LowerScope::host_children_for_slot` / `host_children_ui`
surface; Wave J §4.4's `takes={…}` and `<invoke>` (generalised
to typed `slot<sig>` props + plain `<slot name args/>`
invocation).

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

#### Syntactic shape — CSS-Nesting vs YAML (Q12)

PRSS today is TOML-ish: `[selector]` headers + `key = value`
assignments + brace-expr computed values. The Wave L shapes
above add CSS-Nesting-style `{…}` blocks with `&:state`
selectors. The 2024 CSS Nesting spec is the proximate
reference.

An open alternative: **YAML-based PRSS**. The same content
expressed as indented data:

```yaml
class.btn:
  background: '{ accent }'
  radius: 8
  '&:hovered':
    background: '{ lighten(0.1) }'
    radius: 12
  '&:pressed':
    background: '{ darken(0.1) }'
  '&:disabled':
    background: '{ mute }'
    opacity: 0.5
```

**Pros of YAML.** Universally recognised; less syntax noise;
indentation maps cleanly to nesting; strong existing tooling;
familiar to anyone who's touched Kubernetes / Ansible / GitHub
Actions.

**Cons of YAML.** Brace-expr values must be quoted strings
(YAML's inline-dict syntax `{a: 1}` clashes with PRSS's
`{ luau-expr }`); the pipeline form (`accent | :hovered →
lighten(0.1)`) doesn't fit YAML; `@color` / `@variant`
directives need a workaround for YAML's reserved `@`-handling
in some parsers; `&:hovered` keys need quoting; significant
whitespace breaks copy-paste from chat / docs / inspectors.

**The deeper issue.** YAML is great for *pure data*. PRSS in
Wave L is *data + expressions + pipelines + computed values +
nested selectors*. The expression-heavy parts are exactly what
YAML handles least gracefully. Adopting YAML would require
either escaping every expression as a quoted string (which
defeats the readability win) or building a YAML-superset
dialect (which loses the "you already know this"
portability).

**Doc lean:** keep CSS-Nesting. PRSS is a Prism-specific DSL
already; familiarity with CSS authors is a net positive that
YAML doesn't replicate, and the expression syntax is
first-class in CSS-Nesting-shape but quoted-string in YAML.

**Q12 in §8.** Open for discussion. If we adopt YAML, Phase 14
(state-variant nested records) is the natural landing point
since it's where the new syntax wraps the old runtime.
See §11.12 for the deeper "what if we did adopt YAML"
exploration.

**Wave / Phase.** Phase 14 (Shapes 1 + 2 + named-reference
plumbing); Phase 15 (named state-responsive value declarations
in the token table). Depends on the trait registry being live
(Phase 8) for the parent-context helper resolution. **Blocked
on Q12** — Phase 14 cannot ship until the syntactic-shape
choice is committed.

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
lowering. `macro_rules!` for the DSL. Declared with the
`<macro>` wrapper (PascalCase first-positional name, §3, §6.1):

```prui
<macro Field>
  <match><Field label={lbl} value={val}/></match>
  <expand>
    <container direction=column, gap=4>
      <text class=field-label>{lbl}</text>
      <input value={val}/>
    </container>
  </expand>
</macro>

<Field label="Title" value={state.title}/>
```

At parse time `<Field/>` is matched and replaced with the
expansion; `{lbl}` and `{val}` are substituted. Expansions can
recurse (a macro expanding to another macro), bounded by a
configurable depth limit.

**Hygiene.** Macro-introduced identifiers (e.g. a `let` inside
the expansion) are renamed to fresh names so they can't
shadow the call-site bindings. Same shape Rust 2018+ macros
use.

**Attribute macros** ride the same primitive — the `<macro>`
wrapper takes an `attribute` flag (boolean attr) plus typed
params using the same inline-typed-prop syntax (§6.2):

```prui
<macro Elevation, attribute, level: int>
  <expand to-attrs>
    style.radius      = 8,
    style.background  = {tokens.surface},
    style.shadow      = {elevations[level]}
  </expand>
</macro>

<container elevation=2>…</container>
```

**Wave J §4.5 PRSS `@mixin` is absorbed.** `@mixin
elevation(level) { … }` becomes one specific case of attribute
macro — the macro engine generalises across both PRUI and PRSS
surfaces, one engine instead of two parser dialects.

**Authored from Luau too:**

```luau
prism.macro {
  name    = "Field",
  pattern = prui_pattern [[ <Field label={lbl} value={val}/> ]],
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
<!-- ./share-button.prui — declares <ShareButton/> -->
<component ShareButton
  text: string required,
  capabilities = [
    clipboard: Clipboard,
    network:   Network optional,
  ]>

  <button on:click=$clipboard.write(text)>Copy</button>
</component>
```

No `<capability>` sub-tag — the inline list is the whole
surface. Same shape from Luau:

```luau
prism.component {
  name = "ShareButton",
  props = { text = { type = "string", required = true } },
  capabilities = {
    clipboard = "Clipboard",
    network   = { type = "Network", optional = true },
  },
  render = |props| prui [[ <button on:click=$clipboard.write(props.text)>Copy</button> ]],
}
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

**What it deletes / supersedes.** The earlier `<capability
name=…, type=…>` sub-tag idea (collapsed into the
`capabilities=[…]` header attribute). Module-global host
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
<component Toast
  tone: union<
    info,
    success { duration: int = 2000 },
    error   { dismissable: bool = true, retry: action? }
  > required>

  <match on={tone}>
    <case info>           …                              </case>
    <case success(d)>     <progress duration={d}/> …     </case>
    <case error(dis, r)>  … <button if={r != nil}>Retry</button> </case>
  </match>
</component>

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

**Design.** Four projections in the end state (the earlier
`contract` projection collapses — contracts are
empty-body `<trait>`s declared inside a `.prui` or `.luau`
file and imported through `component` or `script`):

| Projection | Role | Files | Alias |
|---|---|---|---|
| `stylesheet` | apply styles | `.prss` (or `.luau` returning a stylesheet table) | `as ns` |
| `script` | reuse helpers / register any extension kind | `.luau` | `as ns` (namespaced) / bare (flat-merge) |
| `component` | register one-or-more component tags | `.prui` *or* `.luau` returning `prism.component{…}` or a table of them | `as Ns` (namespaced) / bare (each component into scope) |
| `dialect` | extend the language | `.luau` calling `prism.dialect{…}` | (no alias) |

`component` replaces `widget` (renamed + wired).

#### File-extension dispatch

`<import component="./button.prui"/>` parses + lowers the
`.prui` body; `<import component="./icon.luau"/>` evaluates
the file's `return prism.component{…}` (or a table of them).
The resolved tag(s) go through one `TagResolver` — a
`.prui`-defined card and a `.luau`-defined card are
indistinguishable at the call site.

#### Multi-component files — namespace rules

Both `.prui` files (with multiple top-level `<component>` /
`<trait>` / `<mixin>` / `<macro>` wrappers) and `.luau` files
(returning a table of `prism.<kind>{…}` entries plus bare
helpers) may carry **more than one declaration**. The
`<import>` `as` attribute controls how they bind:

```prui
<!-- single-component file: file stem becomes the tag -->
<import component="./card.prui"/>
<!-- registers <Card/> -->

<!-- multi-component file with `as=` namespace: each name prefixed -->
<import component="./forms.prui"/> as forms
<!-- registers <forms.TextField/>, <forms.DropdownField/>, etc. -->

<!-- multi-component file without `as=`: each name bare-imported -->
<import component="./forms.prui"/>
<!-- registers <TextField/>, <DropdownField/> directly; collision = parse error -->

<!-- Luau-defined components, same rules: -->
<import component="./card-system.luau"/> as cards
<!-- registers <cards.Card/>, <cards.Field/>, etc. from the returned table -->
```

#### Luau side — multi-export including non-component artifacts

A `.luau` file's `return` value is one of:

1. A single `prism.<kind>{…}` table → registers as that one
   thing (component / trait / mixin / macro / dialect).
2. A **table of named entries** → each entry registers under
   its key. Entries can mix kinds, and can include bare Lua
   functions / values for helpers.

```luau
-- card-system.luau
return {
  -- Components — registered as tags
  Card     = prism.component { name = "Card",  props = …, render = … },
  Avatar   = prism.component { name = "Avatar", props = …, render = … },

  -- Other extension kinds — registered with the appropriate registry
  Pointable = prism.trait { name = "Pointable", attrs = … },
  Hoverable = prism.mixin { name = "Hoverable", … },
  Field     = prism.macro { name = "Field", pattern = …, expand = … },

  -- Bare helpers — available as expression values under the import's namespace
  helpers = {
    format-date = |t| os.date("%Y-%m-%d", t),
    accent-for  = |tone| if tone == "danger" then "#ef4444" else "#3b82f6" end,
  },
}
```

Imported as `<import script="./card-system.luau"/> as cs`:

- `<cs.Card title="Hi"/>` — component invocation (PascalCase)
- `<container with=[cs.Hoverable]>` — mixin reference
- `<component MyForm impls=[cs.Pointable]>` — trait reference
- `<cs.Field label="Title"/>` — macro expansion
- `{cs.helpers.format-date(now)}` — bare helper called in
  expression context

**Same `<import>` element, same resolver, same `as`
namespacing.** The user picks the *lens* via the projection:
`component` imports the components only; `script` imports
everything in the returned table (components, traits, mixins,
macros, dialects, AND helpers).

#### Projection vs returned shape

| Projection | What it binds from the `.luau` return |
|---|---|
| `component` | only the `prism.component{…}` entries (filtered) |
| `script` | every entry — components, traits, mixins, macros, dialects, plain functions / values |
| `stylesheet` | only the `prism.stylesheet{…}` entries (or a raw stylesheet table) |
| `dialect` | only the `prism.dialect{…}` entries |

This is the "projection is the lens" rule from the fusion
doc: the same `.luau` file viewed through different `<import>`
projections exposes different subsets of its return table.

#### Module identity

Resolved absolute path is the key — N call sites importing the
same file parse / evaluate it once.

**Wave / Phase.** `widget` → `component` rename + wiring +
multi-component file support: Phase 7. The full
`prism.<kind>{…}` Luau-builder family lands incrementally as
each kind ships (mixins Phase 9, macros Phase 10, etc.).

**What it deletes / supersedes.** The `widget` projection
keyword; the dead `interpret.rs:1018` handler skip; the
doc-only `prism.widget{…}` builder; the standalone `contract`
projection (collapsed — contracts are body-less `<trait>`s
imported as components or scripts).

**Open questions.** Q1 (bare multi-component import collision
handling — error vs first-wins vs warn); Q6 (PascalCase-
normalisation edge cases for file stems); Q7 (already
resolved — `.luau` can return mixed kinds via a table).

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
| Tier 3 incomplete animation namespaces (`Transition` / `Animate` / `At`) | `ast.rs:108-149` | ship in Phase 1  | 3 |
| `<component>` markup tag (currently aliases `<container>`) | `interpret.rs:1739` | **retired entirely** — components are invoked by PascalCase tag (`<Card/>`), declared via file-as-component / Luau / Rust spec (§3, §6.1) | 6 |
| Dead `widget=` import projection | `interpret.rs:1210` parses, `:1018` skips | renamed to `component=` and wired | 7 |
| 17-namespace `AttributeNamespace` enum | `ast.rs:74-152` | open trait registry | 8 |
| Wave J §4.5 PRSS `@mixin` (would have shipped in J Phase 4) | (would be) | absorbed by §6.8 macro engine | 10 |
| Wave J §4.8 variant prefixes (would have shipped in J Phase 4) | (would be) | `with=[Mixin, …]` (§6.3) | 9 |
| Long-form closure `\fn(args) … end` (fusion §7.2 alt) | fusion-doc grammar | `\|args\| expr` only | n/a (already removed from this doc's surface) |
| `<property>` sub-tag (Wave J §4.3 draft) | (would be) | inline header attr `name: type [= default \| required]` on `<component>` (§6.1, §6.2) | 6 |
| `<extends>` / `<impls>` / `<derive>` / `<capability>` sub-tags (earlier drafts of this doc) | (would be) | inline header attrs `extends=`, `impls=[…]`, `derive=[…]`, `capabilities=[name: Type, …]` (§6.1, §6.3, §6.9) | 6–11 |
| `<contract>` declaration tag (earlier drafts) | (would be) | body-less `<trait Name attrs/>` (§6.3 — contract = trait with no implementation) | 8 |
| `<derive>` declaration tag (earlier drafts) | (would be) | use-site `derive=[Mixin, …]` attribute on `<component>` (the same `<mixin>` declaration; parse-time vs runtime is a use-site choice — §6.3) | 9 |
| `<invoke>` tag (earlier drafts of §6.5) | (would be) | plain `<slot name args/>` invocation primitive (§6.5) | 13 |
| `name=` attribute on any declaration tag | (every kind, earlier drafts) | PascalCase first-positional token (§3, §6.1) | 6 |
| One-component-per-file constraint (earlier file-as-component lean) | (was a doc lean only) | multi-component `.prui` files first-class; `.luau` files multi-export via returned table (§6.1, §6.11) | 7 |

**Why so many deletions.** Most of these are not breaking
changes for the live codebase — the production `.prui` corpus
uses zero `<facet>`, zero `route:`, zero `fct:`, zero `use:`,
zero `transition:`, zero `animate:`, zero `at:`, zero
`widget=`. The cleanup is overwhelmingly about not shipping
mechanisms that promise behaviours and deliver nothing.

The two with real production impact:

- `<host-children/>` (9 files) — mechanical rewrite to
  `<slot/>` in one PR.
- `<component>` aliasing `<container>` — retires regardless;
  the PascalCase invocation rule (§3, §6.1) makes the markup
  tag unnecessary. Shell-side `<shell.icon-button/>` →
  `<ShellIconButton/>` (or namespaced) is a mechanical rename
  across 48 shell blocks + every callsite.

---

### 6.14 Schema-first cross-language unification

**Problem.** Today the authoring surfaces (`.prui` / Luau /
Rust `BlockSpec` / Rust struct + `prism-luau-derive`) each emit
*their own* downstream artifacts. Wave I generates Luau type
stubs from PRUI but not from Rust-derived components.
`prism-luau-derive` generates a `Block` impl from a Rust struct
but doesn't emit Luau type stubs (those come separately). PRUI
components don't get auto-generated Rust handles; Rust
components don't get auto-generated PRUI tag schemas. PRSS
classes don't expose themselves to Luau or Rust callers in any
typed way. The "declare once, use everywhere" promise is
partial in every direction.

**Design.** Treat the **schema** —
`{props, slots, signals, state, hooks, capabilities,
impls / derives, styles}` — as the universal currency. Every
authoring surface emits a schema; the schema auto-flows to
every consumer surface. Authoring surfaces are *input forms*;
consumer surfaces are *generated outputs*.

#### The five authoring surfaces, all produce a schema

| Surface | Input form | Schema produced |
|---|---|---|
| `.prui` file, `<component>` wrapper | inline-typed header + body | parsed `ComponentSchema` |
| `.prui` file, `<trait>` / `<mixin>` / `<macro>` wrapper | inline-typed header + body | parsed `TraitSchema` / `MixinSchema` / `MacroSchema` |
| Luau `.luau` file | `return prism.component {…}` (or table mixing `prism.<kind>{…}` entries plus helpers) | table-shaped schema record(s) |
| Rust `BlockSpec` row | `const FOO_SPEC: BlockSpec = …` | constant `ComponentSchema` |
| Rust struct + `prism-luau-derive` | `#[derive(PrismComponent)]` | derived `ComponentSchema` |

All five compile to one canonical `ComponentSchema` struct (or
`TraitSchema`, `MixinSchema`, … — one per `prism.<kind>` in
§6.11). The `prism-luau-derive` crate, today a Rust→Block-impl
shortcut, becomes the *Rust-side parser* for the schema —
same role the `.prui` parser plays for the markup side.

#### The four consumer surfaces, all auto-generated from the schema

| Consumer | What auto-flows from a `ComponentSchema` | Today | End state |
|---|---|---|---|
| Luau type stubs | Typed call interface (`Button.new(props: {label: string, tone: enum<…>}) -> UI`) | partial (Wave I, PRUI only) | full coverage, all authoring surfaces |
| Rust typed handles | `pub fn button(props: ButtonProps) -> ComponentRef` constructor + a `ButtonProps` struct | not wired | auto-generated via the schema codegen |
| Inspector + property panel | Field rows, types, default editor, variant picker | partial (Rust schemas) | full coverage |
| LSP completions + hover | Tag completion, attribute completion, hover docs | partial (Wave I) | full coverage |

The principle: **schema once, surface many**. A component
authored in Luau gets Rust handles for free; a component
authored as a Rust derive gets Luau type stubs for free; a
component authored in `.prui` gets both. Same for traits,
mixins, derives, contracts, and (via §6.6) PRSS classes and
named state-responsive values.

#### `prism-luau-derive` — folded into the unification

Today `prism-luau-derive` is a separate proc-macro crate doing
two jobs:

1. Parse a Rust struct's attributes into a Component shape.
2. Emit a `Block` impl that satisfies the `Component` trait.

In the end state, those two jobs split:

1. **Parse:** the macro is the Rust-side input form for the
   universal schema. Output is a `ComponentSchema` constant —
   the same shape the `.prui` parser produces.
2. **Emit:** the *generic schema codegen* (a new crate, or
   part of `prism-builder`) walks any `ComponentSchema` and
   emits:
   - the `Block` / `Component` trait impl (Rust runtime)
   - the Luau type stubs (`.d.luau` artifacts the analyzer reads)
   - the Rust typed handle constructor + `…Props` struct
   - the inspector field schema entry

The macro's emit logic shrinks to "call schema-codegen with my
parsed schema record." Every other input surface (`.prui`
parser, Luau `prism.component{…}` evaluator, Rust `BlockSpec`)
does the same thing.

#### Concrete: a card defined in Luau, used from Rust

```luau
-- card.luau
return prism.component {
  name  = "Card",
  props = {
    title    = { type = "string", required = true },
    tone     = { type = "enum<default|primary|danger>", default = "default" },
    onClick  = { type = "action" },
  },
  slots = {
    children = { signature = "() -> ui" },
  },
  render = |props| prui [[
    <container with=[Pointable], style={…}, on:click=$props.onClick()>
      <heading level=3>{props.title}</heading>
      <slot/>
    </container>
  ]],
}
```

Schema flow at build time:

- **PRUI consumer:**
  `<import component="./card.luau"/>` + `<Card title="Hi" tone=primary>…</Card>`
  — already works (§6.11). The schema makes the markup typed at
  parse: misspelled `tone=primery` or missing `title=` fails
  there.
- **Luau consumer:**
  `local card = require("./card.luau"); prism.use(card, {title="Hi", tone="primary"})`
  — the schema produces a typed Luau call signature the
  analyzer narrows.
- **Rust consumer (NEW, today not wired):**
  ```rust
  use card::{Card, CardProps, CardTone};

  let node = Card::new(CardProps {
      title: "Hi".into(),
      tone:  CardTone::Primary,
      on_click: Some(callback!(|| { … })),
  });
  ```
  The `Card` constructor, `CardProps` struct, and `CardTone`
  enum are auto-generated from the schema. The Rust side gets
  full type safety against the component's declared props
  without writing a single line of FFI.

#### Bidirectional: PRSS / traits / mixins

The same schema-first unification covers the §6.4 trait
registry and the §6.6 PRSS surface:

- A `<trait Pointable …/>` declared in PRUI auto-generates:
  - A Luau interface (`type Pointable = {on_click: action,
    is_hovered: bool, ...}`)
  - A Rust trait or marker (`pub trait Pointable { … }`)
  - LSP completions when an author types `pointer.<TAB>`
- A `prism.trait{…}` declared in Luau auto-exposes to PRUI as
  an `impls=Pointable` candidate, with the inspector and LSP
  showing it identically to a PRUI-declared trait.
- A `[class.card]` defined in PRSS auto-generates a Luau handle
  (`tokens.class.card` returning the StyleProperties record)
  so Luau code can dynamically apply the class without
  restringing the name.
- A `@color responsive-accent` in PRSS auto-generates a Luau
  reference (`tokens.colors.responsive_accent`) — a typed
  state-responsive value the consumer can pass to a
  `style.background=`.

The conceptual model: **every authored artifact in the
PRUI/PRSS/Luau stack carries a schema; the schema is the lingua
franca across the three surfaces**. Today's partial flows
(Wave I PRUI→Luau, `prism-luau-derive` Rust→Block-impl) are
two specific bridges. Wave L's end state is *all* bridges, all
generated from one schema codegen path.

#### What this unlocks

- **Rust apps consume Prism components type-safely.** A native
  shell consuming a builder-authored library calls into
  components with typed handles, not stringly-typed props.
- **Luau authors get IDE completion for Rust-defined
  components.** No more "what props does `<shell.app-window>`
  take?" friction.
- **PRSS classes become callable.** A Luau script that wants
  to dynamically pick a class gets typed enums, not magic
  strings.
- **`prism-luau-derive` becomes thinner**, not thicker — the
  bidirectional codegen lives in one place, and the derive
  delegates to it.

#### Wave / Phase

Phase 8 (trait registry) is the prerequisite — once the
universal `ComponentSchema` / `TraitSchema` / etc. types exist
in `prism-core`, the schema-codegen pipeline can start being
wired into each surface. Concretely:

- Phase 8 — define the canonical schema types in `prism-core`.
- Phase 8.5 — make every input surface (`.prui` parser, Luau
  evaluator, Rust `BlockSpec`, `prism-luau-derive`) produce
  the canonical schema instead of its own ad-hoc shape.
- Phase 9–12 — as mixins / derives / traits / unions land,
  each gets its own schema kind in the same codegen pipeline.
- Phase 16 — codegen for the Rust typed handles + the PRSS
  Luau handles (the "new" consumer surfaces). Could ship
  earlier if priority dictates.

#### What it deletes / supersedes

- The today-partial flows that each generate a slice of
  what's needed (Wave I-only Luau stubs from PRUI;
  derive-only Block impl from Rust struct).
- Ad-hoc, hand-written FFI between Rust hosts and
  Prism-defined components.
- The implicit "Rust authors stay Rust-side, Luau authors
  stay Luau-side, PRUI authors stay markup-side" silos.

#### Open questions

- Q13 (new — see §8): which `ComponentSchema` types are
  *closed* (e.g., the `type` set in property declarations) vs
  *open* (e.g., capability names, mixin names)? The closed
  parts get proper Rust enums in codegen; the open parts get
  typed strings + runtime registry lookup.
- Whether the schema codegen runs at compile time (build.rs +
  proc-macros) or at runtime (when the registry first loads).
  Lean: compile time, so the typed handles ship as part of
  the consumer's binary with no init cost. Runtime fallback
  for hot-reloaded components.
- Where `prism-luau-derive` lives after the split. Probably
  becomes a thin shim in `prism-builder` (no separate crate)
  once the universal codegen lives there.

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

**Q1. Bare (unnamespaced) multi-component import — collision
handling.** Resolved that multi-component `.prui` / `.luau`
files are first-class (§6.1, §6.11). When imported with
`as Ns`, components bind under that namespace
(`<Ns.TextField/>`). When imported without `as`, components
bind into the bare scope. Three options for collision:
(a) parse error on first collision (strictest, easiest to
reason about — doc lean); (b) first-import-wins with a
diagnostic; (c) last-import-wins (CSS-like). Decision affects
Phase 7 scope.

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

**Q5. Trait self-reference.** Can a `<trait>` (body-less,
serving as contract) reference itself (a `Composite` slot
type)? Today no. If demand surfaces, gate behind an explicit
`recursive` keyword.

**Q6. PascalCase-normalisation rules for filenames.** A
`.prui` file's stem becomes its registered tag name (§6.1).
`card.prui` → `<Card/>`. `app-window.prui` → `<AppWindow/>`
(hyphen uppercases next letter). Edge cases needing a pick-once
ruling: filenames with dots (`foo.bar.prui` → `<FooBar/>`?
`<Foo.Bar/>`? error?), numerics (`h1.prui`,
`2fa-prompt.prui`), leading underscore (`_private.prui`),
all-caps (`CSS-helper.prui`), non-ASCII, and tags conflicting
with reserved primitive names (`container.prui` →
`<Container/>` shadows nothing, but `<container/>` lower wins
on the call side anyway). Decision should land with Phase 6.

**Q7. `.luau` component vs `.luau` script in the same file.**
Resolved (§6.1, §6.11): a `.luau` file returns a *table* of
named entries that can mix `prism.<kind>{…}` records of any
type plus bare helpers. The `<import>` projection acts as the
*lens*: `component` binds only the components; `script` binds
every entry (components + traits + mixins + macros + dialects
+ helpers). Same file viewed through different projections
exposes different subsets.

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

**Q12. PRSS syntactic shape — CSS-Nesting vs YAML.** §6.6
covers the choice and recommends CSS-Nesting (`&:state` +
brace-expr values + pipeline forms). YAML is an alternative
that trades expression ergonomics for familiarity. Decision
should land before Phase 14 ships. See also §11.13 for the
deeper exploration.

**Q13. Closed vs open schema fields in cross-language codegen.**
§6.14 raises this. Which `ComponentSchema` fields are *closed*
(get proper Rust enums in the auto-generated typed handles —
e.g. the property `type` set, the state-suffix set) vs *open*
(get typed strings + runtime registry lookup — e.g. capability
names, mixin names, user-registered trait names)? The dividing
line affects both compile-time safety and forward compatibility
when new entries land in the open registries.

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
| Button-but-louder-hover variant | new PRSS class + new component (~25 lines) | new file with `<component LoudButton extends=Button …>` + 1 prop default (~5 lines) |
| Lighter / darker / transparent variant of a token colour | hand-tuned hex per call site | `accent/50`, `with(accent, l=+0.05)` (1 expr) |
| Pressed-state visual feedback | script-toggled class + PRSS variant (~12 lines) | `style.background={accent \| :pressed → darken(0.1)}` (1 line) |
| 3 props × 4 states on one component | 12 lines (cross-product) | 7 lines (nested record) |
| 5 buttons sharing one hover/press/disabled curve | 20 lines | 6 lines (`@color` + 5 refs) |
| Reusable elevation system across the workspace | 6 near-identical classes copy-pasted | 1 attribute macro |
| Dark-mode override on one element | duplicate class with `dark-` prefix | `with=[Card, dark ? Muted : nil]` |
| Form field that requires a focusable control | manual prop binding + runtime assertion | `<slot accepts=Focusable>` (1 attr) |
| Required prop with a sensible default | host binding + nullable check in body | `label: string = ""` inline in the `<component>` header |
| Import a `.prui` component from a sibling directory | impossible (no runtime wiring) | `<import component="../widgets/card.prui"/> as card` |
| Author a component imperatively from Luau | impossible (`prism.widget{…}` never written) | `return prism.component{…}` in a `.luau` file |
| Stack draggable + hoverable + selectable on a container | hand-roll state + handlers + styles (~40 lines) | `with=[Draggable, Hoverable, Selectable]` (1 attr) |
| Define a new attribute kind (e.g. `elevation=2`) | impossible without engine release | `<macro Elevation, attribute, level: int>…</macro>` or `prism.trait{…}` |
| Component needs clipboard access | thread global through props | `capabilities=[clipboard: Clipboard]` inline in the `<component>` header |
| Toast with three variants carrying different fields | boolean prop ladder | `tone: union<…>` inline declaration + `<match>` body |
| Multi-component file (form-field family, chart system, …) | one component per file → many files, many imports | multiple `<component>` wrappers in one `.prui` file; `<import component="./forms.prui"/> as forms` namespaces all of them |
| Luau library exporting components AND helpers AND traits in one file | impossible — `.luau` returns one value | return a table of mixed entries; `<import component=…>` filters to components, `<import script=…>` binds everything |

### Structural wins

| Today | End state |
|---|---|
| 17-namespace `AttributeNamespace` enum, hard-coded | Open trait registry, document-scoped, Luau-extensible |
| 5 registration paths for a component | 1 noun, 3 authoring surfaces |
| 3 implementations of "repeat children once per item" | 1 (`<container for=…>`) |
| 2 spellings for "splice caller content" | 1 (`<slot/>`) |
| PRUI `style:key=` vs PRSS `[selector] key =` (two syntaxes) | 1 nested-record syntax shared |
| 4 sugar-only namespaces in the enum | 0 (deleted) |
| `<component>` markup tag silently aliases `<container>`; call sites use mixed-case dotted (`<shell.icon-button/>`) | PascalCase invocation rule (React / Vue): `<Card/>` is a component, `<container/>` is a primitive. `<component>` tag retires entirely; declaration moves to file head / Luau builder / Rust spec. |
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
<trait Flex
  impls = Container,
  requires = { Container.display = flex|grid },
  gap: int/>
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

A `padding: int <= {row.depth * 4}` extension to the inline
typed-prop syntax — `<=` signals "computed default from the
following expression," distinct from the `=` literal-default
form. Topologically resolved across the property graph; cycle
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
<!-- ./chat.prui -->
<component Chat
  effects = [user-message: string]>           <!-- declared as inline header attr, like capabilities -->
  <button on:click=$yield user-message(input.value)>Send</button>
</component>
```

```prui
<!-- ./chat-room.prui -->
<component ChatRoom>
  <handle user-message as msg>
    {append-to-log(msg); broadcast(msg)}
  </handle>

  <container>
    <Chat/>
  </container>
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
A trait-based take: `<trait Responsive …/>` could carry
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

### 11.12 PRSS as YAML — the deeper exploration

§6.6 surfaces the YAML-vs-CSS-Nesting fork (Q12 in §8) and
recommends keeping CSS-Nesting. The musing below is the "what
if we did adopt YAML" thought experiment carried further.

A YAML-based PRSS could lean into YAML's strengths and work
around its weaknesses:

- **Inline expressions stay parenthesised.** Borrow Jinja's
  `{{ expr }}` shape so brace-expr values don't clash with
  YAML's inline-dict:
  ```yaml
  class.btn:
    background: {{ lighten(accent, 0.1) }}
  ```
- **Pipeline forms become a YAML sequence with a pipeline
  marker:**
  ```yaml
  class.btn:
    background:
      - {{ accent }}
      - { state: hovered,  apply: lighten(0.1) }
      - { state: pressed,  apply: darken(0.1) }
      - { state: disabled, value: mute }
  ```
  Verbose, but YAML-native. Less terse than `|` pipeline,
  more navigable for tooling that walks YAML AST.
- **@-directives become top-level YAML keys:**
  ```yaml
  '@color':
    responsive-accent: …
  '@variant':
    compact: 'data-density="compact"'
  ```

This shape would lose terseness but gain:

- **Cross-tool portability.** YAML AST consumers (linters,
  formatters, schema validators) work without bespoke PRSS
  tooling.
- **Schema-first PRSS.** A JSON Schema (or YAML schema) for
  the PRSS document could be checked by any YAML-aware tool,
  not just the PRSS parser.
- **Easier non-Prism consumers.** A relay-side analytics
  pipeline that reads `.prss` for theming metadata doesn't
  need to parse PRSS-shape; YAML libraries exist everywhere.

The cost is real: every example in this doc grows by ~50% in
line count, the pipeline form becomes ~3× more verbose, and
PRSS stops looking like CSS (the closest analogue most authors
know). Unless one of the YAML-portability benefits becomes
load-bearing, the CSS-Nesting shape wins on authoring
ergonomics. Listed here for the day the portability
calculation changes (e.g., a major external tool consuming
`.prss` files).

### 11.13 Things we deliberately won't do

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
