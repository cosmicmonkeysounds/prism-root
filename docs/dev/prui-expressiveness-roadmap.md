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
| Inheritance only via PRSS `extends=`; no markup-level composition | `use Parent` inside a body, OR `Comp2 = Parent with { overrides }` as record-update on a component value (§12.5) |
| Style-modifying behaviours wired by editing the runtime | User-defined attribute macros (`prism.macro{…}`) and traits (`prism.trait{…}`) |
| Host services (clipboard / network / fs) reached via magic globals | `requires cap: Cap` as a body statement (parse-time validated) plus `?.` optional-chain on the body side (§12.13) |
| `darken`/`lighten`/`mix` lerp in sRGB → desaturated colours | OKLCH-backed (callers unchanged) + `with(c, l=±, a=…)` channel adjust + slash-alpha (`accent/50`) |
| Component props: anything goes, host decides | Function-signature parameter list: `name: type [= default \| required]`, including discriminated unions `type Tone = a \| b(…) \| c` (§12.11) |
| Six separate declaration tags (`<property>` / `<slot>` / `<extends>` / `<impls>` / `<derive>` / `<capability>` / `<contract>`) for component schema | **One gesture, six heads** — every declaration is `Name [params] = expression` (or `Name [params] { body expression }`). Heads: `component` / `trait` / `mixin` / `macro` / `type` / `class` (§12.2) |
| One component per `.prui` file (file-as-component flat) | Multi-component files first-class — any number of top-level declarations per file; `.luau` files multi-export via returned table mixing components, traits, mixins, macros, helpers |
| State is a tag (`<state name=…>`) or a host-attached binding | State is a **value**: `let count = state(0)` creates a reactive cell; `count <- v` writes; bare `count` reads-and-tracks (§12.4) |
| Slots have their own ceremony (`<slot/>` declaration, `<slot name args>…</slot>` provider) | Slots are **typed function parameters** carrying `ui` (a tree) or `\|args\| → ui` (a render-prop). No `<slot>` machinery; passed and called like any other value (§12.6) |
| Two closure forms (`\|args\| expr` and `\fn(args)…end`) | One parameter syntax, two body shapes: `\|args\| expr` for single expression, `\|args\| { … }` for blocks. The `\fn` long-form stays rejected (§2, §9, §12.16). |
| `<import …/>` XML wrapper with explicit projection attribute | `import "path" [projection] [as alias]` bare directive — projection inferred from file extension (`.prss` → stylesheet, `.prui` → component, `.luau` → script), trailing keyword for the override (§12.17) |
| XML declaration wrappers + body sub-tags (`<component>`, `<state>`, `<on>`, `<style>`, `<import>`) | Function-shape + body statements: `component Name(params) { let cell = state(…); on event(e) { … }; style { … }; <tree/> }`. The render *tree* stays tagged; everything else is one block of statements (§12) |
| Each authoring surface (`.prui` / Luau / Rust / `prism-luau-derive`) emits *its own* downstream artifacts — Luau stubs only auto-flow from PRUI (Wave I), Rust typed handles never auto-flow at all, PRSS isn't callable from Luau or Rust typed-ly | **Schema-first unification** — one canonical `ComponentSchema` / `TraitSchema` per artifact, all surfaces auto-generated from it. Declare in PRUI → Rust gets typed handles + Luau gets stubs for free; declare in Luau → Rust + PRUI get them; declare in Rust via `prism-luau-derive` → Luau + PRUI get them. (§6.14) |
| Component invocation: `<component>` markup wrapper (alias for `<container>`); shell tags `<shell.icon-button/>` are mixed-case dotted; "is this a primitive or a user component" requires looking at the docs | **PascalCase rule (React / Vue convention)** — `<Card/>` / `<CustomForm/>` / `<AppWindow/>` are components by their PascalCase name; `<container/>` / `<text/>` / `<slot/>` are primitives by their lowercase name. The `<component>` markup tag retires entirely. (§3, §6.1 Part 1) |

The trajectory in three sentences:

1. **Today**'s authoring surface is the HTML-namespace-prefix
   shape with seventeen ad-hoc behaviours bolted on, five
   incomparable ways to register a component, and N × M
   restatement for state-aware styles.
2. **The end state** is the *canonical surface* of §12 — one
   gesture, **`Name [params] = expression`**, applied to
   every declaration kind. Components are typed functions
   from props to trees; state is `state(initial)` + `<-`;
   composition is `use Mixin` inside the body or `with`
   record-update outside; slots are typed function
   parameters; capabilities are `requires`-statements; types
   are algebraic; trees stay tagged. All of it backed by an
   open trait registry, mixins + macros + capabilities doing
   the work seventeen namespaces tried to, and nested
   records collapsing the N × M state grid to N + M.
3. **Wave J** adds the features the current grammar needs
   (additive). **Wave K** deletes the mechanisms the rethink
   obsoletes (subtractive, safe). **Wave L** performs the
   rethink. **§12** is the surface that carries it. Phasing
   in §7 orders the J/K/L features so every step ships
   independently; §12.21 phases the canonical surface
   alongside.

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
- **One closure form, two body shapes.** `|args| expr` for a
  single expression, `|args| { … }` for a multi-statement
  block (last expression is the value; `return` allowed for
  early exit). Same parameter syntax across both modes — the
  only difference is body shape. The fusion-doc-rejected
  long-form `\fn(args) … end` stays rejected; the brace
  block covers every multi-line case it would have (see
  §12.6 for the design and §9 for the rejected-form
  rationale).
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
| A `.prui` file with one or more `<component Name …>` wrappers | a component (markup decl) | `<Name/>` if the file is bare, `<Ns.Name/>` if the file declares `<namespace=Ns/>` at its top (§6.1 Part 3); `as=` on the importer overrides |
| `prism.component{name="Name", …}` in a `.luau` file (single or via a returned table) | a component (Luau decl) | `<Name/>` if bare, `<Ns.Name/>` if the `prism.module{namespace="Ns", …}` builder names it; `as=` overrides |
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
<!-- ./forms.prui — declares the Forms namespace -->
<namespace=Forms/>

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

When a file declares `<namespace=Ns/>` at its top, every
top-level declaration in it binds under that namespace
(`<Forms.TextField/>`, `<Forms.DropdownField/>`). Files
without a `<namespace=…/>` directive bind their declarations
bare (`<TextField/>`). `as=` on `<import>` overrides whatever
the file declared. The earlier "file-stem-PascalCased = tag"
rule is gone (too many edge cases — see Q6 in §8). See §6.11
for full resolution rules.

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

**Path note.** The bare `interpret.rs:NNNN` citations below live
at `packages/prism-ui-runtime/src/interpret.rs` (the 10,882-line
lowering pass — Phase 0 splits this file). `ast.rs` and
`grammar.rs` live at `packages/prism-core/src/language/prism_ui/`;
`stylesheet.rs` at `packages/prism-core/src/language/prss/`.

### PRUI runtime

| Area | Current state | Cite |
|---|---|---|
| Parser entry | `parse(source) -> (Document, Vec<ParseError>)` | `prism-core/src/language/prism_ui/grammar.rs:46` |
| Element table | `container`, `text`, `heading`, `spacer`, `image`, `input`, `slot`, `host-children`, `component`, `fragment`, `let`, `facet`, `teleport`, `script`, `style`, `import`, `match`, `case`, `suspense`, `fallback`, `language`, `dispatch` | `prism-ui-runtime/src/interpret.rs:1738-1948` |
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
| `BUILTINS` table | 16 declarative `BlockSpec` rows + 1 one-off `FacetComponent`; sibling table `primitives.rs` adds 14 `prism.*` shell-primitive specs (same shape, registered separately) | `prism-builder/src/starter.rs:823` + `prism-builder/src/primitives.rs` |
| `register_core_widgets` | Domain `WidgetContribution`s from `prism-core` (calendar, timekeeping, ledger, spreadsheet, comments, dashboard, habits, goals, fitness, reminders, crm, projects, focus_planner + views) wrapped in `CoreWidgetBlock` | `prism-builder/src/core_widget.rs` |
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
a brief comparison to today, and a forward link to the
detailed design in §6.

**A note on syntax.** Examples in §5 and §6 use the
*canonical surface* from §12 — function-shape declarations,
`{ … }` blocks, bare `import` directives. The XML-shape
appears in §6 design tables and the §12.20 cheatsheet as the
stepping stone, but the destination is the §12 shape.

### 5.1 Authoring and invoking a component

Three authoring surfaces (`.prui` / Luau / Rust). One
call-site convention: **PascalCase tag = component invocation;
lowercase tag = primitive** (React / Vue rule). One unified
declaration shape across all surfaces.

```prui
-- ./button.prui — registers <Button/>
component Button(
  label:    string required,
  tone:     default | primary | danger = default,
  on-click: action,
) {
  use Pointable, Focusable                            -- trait conformance

  <container with=[Pointable, Focusable] @click=$on-click>
    <text>{label}</text>
  </container>
}
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
import "./icon-system.luau" as icons

<container direction=column gap=8>
  <Button label="Save" tone=primary on-click=$save()/>
  <icons.Icon glyph="check" size=24/>
  <text>{icons.helpers.icon-url("check")}</text>
</container>
```

There is no `component` keyword at the call site — declaration
lives in the file head (via `component Name(…) { … }`), Luau
builder, or Rust spec. Invocation is always the PascalCase
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

### 5.4 Slots — typed function parameters, period

Today's `<slot>` and `<host-children/>` dissolve. Slots
become *typed parameters carrying either a `ui` tree or a
`|args| → ui` render-function*. No special slot syntax,
no `<slot>` declaration tags.

```prui
-- ./list.prui — declares <List/>
component List(
  items: array<Task> required,
  row:   |Task, int| → ui,
  empty: ui = <text>No tasks yet.</text>,
) {
  <container>
    {if items.length == 0 then empty
     else <fragment for={t, i in items}>{row(t, i)}</fragment>}
  </container>
}
```

```prui
-- caller — slot props pass like any other prop
<List
  items={tasks}
  row={|item, i| <text>{i + 1}. {item.title}</text>}/>
```

A magic-named `children: ui` parameter receives the
caller's unnamed JSX-style children automatically — that's
what today's `<host-children/>` does. Named slots are just
named parameters with `ui` or `|args| → ui` types. Defaults
are tree literals (`= <text>…</text>`) or lambdas. No new
mechanism; just function calls.

**See §6.5** for the unification design and **§12.6** for
the typed-parameter shape.

### 5.5 Imports & extension — one mechanism, many kinds

The bare `import` directive (§12.3) replaces `<import>`;
multi-component files and mixed-kind Luau tables make one
projection cover many extension kinds, with the projection
*inferred from the file extension*.

```prui
import "./theme.prss"                    -- → stylesheet
import "./helpers.luau"     as h         -- → script
import "./card.prui"                     -- → single component
import "./forms.prui"       as forms     -- → multi-component .prui, namespaced
import "./icon-system.luau" as icons     -- → Luau returning table of components + helpers
import "./traits.luau"      as t         -- → Luau returning traits / mixins / macros
import "./markdown.luau"    dialect      -- → explicit projection override
```

Same `ImportResolver`, same `as`-namespacing, same module
cache. The Luau side has one builder family covering five
extension kinds — return a single value or a table mixing
any of:

```luau
prism.trait     {…}   -- typed shape (also serves as contract)
prism.mixin     {…}   -- composable behaviour (state + hooks)
prism.macro     {…}   -- parse-time markup expansion
prism.component {…}   -- a component definition
prism.dialect   {…}   -- embedded sub-language (Wave E)
-- plus bare functions / values as helpers — anything Lua-side
```

**See §6.11** for the full import-family design
(multi-component files, projection-as-lens, multi-export
tables); **§6.4** for trait registration; **§6.8** for
macros; **§12.3** for the bare-keyword import shape.

### 5.6 Mixins, macros, derive — `use` inside the body

Two declaration heads cover behaviour composition + grammar
extension. Both compose into a component via the `use`
statement inside its body (parse-time) or as a function call
at the use site (runtime).

- **`mixin Name { … }`** — a body of `let` / `on` / `style`
  statements that splice into whatever component `use`s
  them.
- **`macro Name(…) { … }`** — parse-time markup expansion.
  Pattern-match tags or attributes; expand to other
  tags/attributes.

```prui
mixin Hoverable {
  let hovered = state(false)
  on pointerenter { hovered <- true }
  on pointerleave { hovered <- false }
}

mixin Draggable {
  let dragging = state(false)
  let offset   = state((0, 0))

  on pointerdown(e) {
    dragging <- true
    offset   <- (e.x - self.x, e.y - self.y)
  }
  on pointermove(e) if dragging {
    self.x <- e.x - offset.x
    self.y <- e.y - offset.y
  }
  on pointerup { dragging <- false }
}

macro Field(lbl: string, val: string) {
  match  { <Field label={lbl} value={val}/> }
  expand {
    <container direction=column gap=4>
      <text class=field-label>{lbl}</text>
      <input value={val}/>
    </container>
  }
}

-- Parse-time: `use Mixin` inside the body, mixin's bindings come into scope
component Card(title: string) {
  use Hoverable, Draggable
  <container class={if hovered then "card hover" else "card"}>
    <heading>{title}</heading>
  </container>
}

-- Runtime: function-style composition OR use-site attribute
<container with=[Hoverable, Draggable]>…</container>
{Hoverable(Draggable(<container>…</container>))}

-- Macro expansion at the call site:
<Field label="Title" value={state.title}/>
```

**See §6.3** for mixin / derive semantics; **§6.8** for macros;
**§12.5 / §12.14** for the `use`-statement composition.

### 5.7 Capabilities — typed host services

Components declare what host services they need; the host
provides them at lower-time; missing capabilities fail at
parse, not at runtime. No magic globals.

```prui
-- ./share-button.prui — declares <ShareButton/>
component ShareButton(text: string required) {
  requires clipboard: Clipboard
  requires network:   Network?

  on click {
    clipboard.write(text)
    network?.post("/share", { text })       -- optional capability call
  }

  <button @click=$click>Copy & post</button>
}
```

`requires` is a body statement, not a header attribute —
the requested capability appears as a local binding for the
rest of the body, and the IDE completes `clipboard.<TAB>`
after the `requires` line. The relay refuses to provide
`FileSystem` to a public-facing component → the component
fails parse, the relay never renders an exploit. Tests
inject `MockClipboard`. Optional capabilities (`Network?`)
must be guarded with `?.` or an `if cap != nil` check.

**See §6.9** for the capability declaration + provision
design; **§12.13** for the body-statement shape.

### 5.8 Algebraic property types — discriminated unions

Props that are "one of these shapes" carry their variant
fields declaratively; the body pattern-matches.

```prui
-- ./toast.prui — declares <Toast/>
type Tone =
    info
  | success(duration: int = 2000)
  | error(dismissable: bool = true, retry: action?)

component Toast(tone: Tone required) {
  <match on={tone}>
    <case info>           …                                            </case>
    <case success(d)>     <progress duration={d}/> …                   </case>
    <case error(_, r)>    … <button if={r != nil} @click={r}>Retry</button> </case>
  </match>
}
```

```prui
<Toast tone={error(dismissable=true, retry=$retry-upload)}/>
```

Lifting `Tone` to a named `type` is the canonical move when
the union is non-trivial; inline `tone: info | success(…) |
…` is allowed but extracting reads better and surfaces in
the LSP. The "boolean prop ladder" anti-pattern
(`is-success` + `is-error` + `is-info`) becomes impossible to
express.

**See §6.10.**

### 5.9 Colour helpers — OKLCH + slash-alpha

Same function names, different math underneath; the
`darken('#3b82f6', 0.3)` no longer goes grey. New `with(c,
l=±, c=±, h=±, a=…)` for channel adjustments and `accent/50`
slash-alpha sugar.

```prui
<container
  style.background=accent/30                              -- alpha = 0.30
  style.tint={with(tokens.accent, l=+0.05, c=-0.02)}/>    -- channel adjust
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

#### Multi-component files + file-declared namespaces (Q6 / Q11)

A `.prui` file may contain **any number of top-level
`<component>` wrappers** (also `<trait>` / `<mixin>` /
`<macro>`). The user is no longer constrained to one
component per file.

**File-declared namespaces (the C# rule).** A file may
declare its own namespace via a `<namespace=Ns/>` directive
at the top — every top-level declaration in the file binds
under that namespace. Files without a namespace declaration
bind their declarations bare. This replaces the earlier
file-stem-PascalCased rule, which had too many edge cases
(dots, numerics, leading underscores, all-caps, non-ASCII,
primitive collisions — see Q6 in §8 for the catalogue).

```prui
<!-- ./forms.prui — declares the Forms namespace -->
<namespace=Forms/>

<component TextField …>…</component>
<component DropdownField …>…</component>
<component DateField …>…</component>

<!-- supporting traits / mixins, also bound under Forms -->
<trait FieldLike, label: string, value: any/>
<mixin FieldValidation>…</mixin>
```

```prui
<!-- ./card.prui — no namespace declaration: bare bind -->
<component Card …>…</component>
```

**Import + namespace rules (§6.11):**

- `<import "./card.prui"/>` — the file has no `<namespace=…/>`,
  so `Card` binds bare → `<Card/>` is the tag.
- `<import "./forms.prui"/>` — the file declares
  `<namespace=Forms/>`, so the file's declarations bind under
  it → `<Forms.TextField/>`, `<Forms.DropdownField/>`, etc.
  The consumer doesn't need to spell `as=Forms` because the
  file already says so.
- `<import "./forms.prui"/> as fields` — `as=` **overrides**
  the file-declared namespace → `<fields.TextField/>`.
- Two bare-bound declarations colliding in the importing
  document = parse error (Q1).

**Luau side.** A `.luau` file's namespace is set via the
`prism.module{namespace="Ns", entries={…}}` builder (or by
returning a table with a `namespace` key). The same Q11
override rule applies — `<import script="./helpers.luau"/>
as h` overrides whatever namespace the file declared.

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

**Open questions.** All resolved as of 2026-05-20 — Q1
(parse error on first collision), Q6 (file-declared
`<namespace=Ns/>` directive replaces filename mangling), Q7
(mixed-kind `.luau` returns via a table). See §8.

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

#### The type set (default-shipping, but the registry is open)

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

These are the **default-shipping** entries the trait registry
ships with. Q13 (§8) resolved against a hard-closed type set —
user-registered types (e.g., `measurement: Length<m | px | pt>`
or `nominal: Tagged<UserId, string>`) extend the registry from
Luau or Rust without grammar edits, and flow into auto-generated
typed handles via the §6.14 schema codegen.

#### Defaults and `required`

- `key: type = literal` — default value. Must be a **literal**
  (string / number / bool / token / enum case / `nil`), not a
  `{…}` expression. Keeps the parser-side resolution trivial.
- `key: type <= {expr}` — **computed default** (Q2 — §8).
  Topologically resolved across the property graph at call
  time, cycle-detected by the same logic the PRSS extends-chain
  validator uses. The expression may reference *other declared
  properties on the same component* (no nested computeds in
  the first cut — keeps the topology trivial). Example:
  ```prui
  <component TreeRow
    depth:   int = 0,
    padding: int <= {depth * 4 + 8}>          <!-- computed from depth -->
  ```
  The `<=` arrow distinguishes computed from literal default;
  `=` is "pin this value" and `<=` is "derive this value." Sites
  that need the host to override pass the explicit value;
  otherwise the computed default fires.
- `key: type required` — required, no default. Missing at call
  site raises a contribution error at lower-time.
- `key: type` — optional, default is the type's zero
  (`""`, `0`, `false`, `nil`).

The Vue-`withDefaults`-style recursion footguns are avoided
because (1) computed expressions may only reference other
*declared* (not computed) properties in the first cut, and (2)
the topology check runs at parse time, failing loudly before
runtime.

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
inspector + Luau-narrowing integration). Computed defaults
(`<= {expr}`): Phase 17 (depends on Phase 6 declarations and
shares the PRSS extends-chain topology checker).

**What it deletes / supersedes.** The `<property name=…>`
sub-tag idea (collapsed into header attributes, §6.1); the
boolean-prop-ladder anti-pattern; the "host knows the schema"
implicit coupling; the "computed defaults are too risky to
ship" earlier doc lean (Q2 resolved against deferral).

**Open questions.** None — Q2 (computed defaults) resolved
to ship, lands Phase 17.

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

**Self-reference — the `recursive` flag (Q5).** A trait that
needs to reference itself (a `Composite` slot type, a
tree-of-`Focusable` contract, a tagged-list trait) declares
itself `recursive` in the header. Mutually-recursive trait
clusters declare every participant `recursive`:

```prui
<trait Composite, recursive,
  children: slot<() -> Composite> = nil/>      <!-- self-reference allowed -->

<trait TreeNode, recursive,
  parent: TreeNode?,
  children: array<TreeNode> = []/>

<!-- Mutual recursion -->
<trait Folder, recursive, items: array<FileLike>/>
<trait File,   recursive, parent: Folder/>
```

Without `recursive`, a self-reference inside the trait header
fails at parse with a clear "this trait isn't marked
`recursive`" error. The flag is intentionally explicit so
authors *opt into* the cycle — accidentally recursive traits
get caught early.

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

**Open questions.** All resolved — Q5 (trait self-reference
via the `recursive` flag, sketched above); Q9 (variant
precedence: latest wins, mixin linearisation rule). See §8.

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
| `Transition` (`transition:`) | 0 in `.prui`; install deferred | **subsumed by unified `Animator` trait** in Phase 1 (§6.15); namespace label retires in Phase 3 |
| `Animate` (`animate:`) | **5 files** (toast, color / connection / modifier / select pickers) use `animate:opacity` / `animate:out-opacity` for overlay fade-ins; lowers to `data-animate-in-*` but the full animator install is deferred | **subsumed by `Animator`** in Phase 1 (§6.15); the 5 live files migrate to `style.opacity={ … \| :entry → from 0 over 200ms }` shape; namespace label retires in Phase 3 |
| `At` (`at:`) | 0; deferred | **subsumed by `Animator`** in Phase 1 (§6.15); keyframes become `animator.keyframes={…}`; namespace label retires in Phase 3 |
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

**Open questions.** All resolved — Q3 (Tier 3 ships via the
§6.15 unified `Animator` in Phase 1); Q8 (pipeline `|`
type-position disambiguation, §6.6); Q11 (cross-library trait
coherence via namespaced imports + file-declared namespaces,
§6.11). See §8.

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

#### Syntactic shape — CSS-Nesting (Q12 resolved)

PRSS today is TOML-ish: `[selector]` headers + `key = value`
assignments + brace-expr computed values. The Wave L shapes
above add CSS-Nesting-style `{…}` blocks with `&:state`
selectors. The 2024 CSS Nesting spec is the proximate
reference, and Q12 (§8) is resolved: **CSS-Nesting wins**.
YAML was considered and rejected — expression-heavy content
reads poorly in YAML, the pipeline shape doesn't fit, and
`&:hovered` keys need quoting. The portability benefit
doesn't outweigh the authoring-ergonomics loss. §11.12
(earlier "what if we did adopt YAML" musing) retires.

**Wave / Phase.** Phase 14 (Shapes 1 + 2 + named-reference
plumbing); Phase 15 (named state-responsive value declarations
in the token table). Depends on the trait registry being live
(Phase 8) for the parent-context helper resolution.

**What it deletes / supersedes.** The four-line per-state
restatement (retained as a fallback parse path during the
transition).

**Open questions.** None — Q8 (pipeline `|` syntax) and Q12
(CSS-Nesting over YAML) both resolved.

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
   "checked", "entry", "exit"]`. (`:entry` / `:exit` are
   transition-lifecycle states owned by the §6.15 `Animator`
   trait — same Phase 1 slice.)
2. **Property whitelist expands** beyond `background` /
   `radius` to `color`, `padding`, `gap`, `width`, `height`,
   `tint`, `opacity`, `transform`.
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
5. **`:disabled` suppresses `on:click` at the dispatcher**
   (Q10 — §8). Not only style: a disabled button does not
   fire its callback. The dispatcher checks `is_disabled` on
   the deepest hit-test node and short-circuits before
   delivering the pointer event. Audit of any code relying on
   the old style-only semantics is part of this phase's
   migration sweep.

**Wave / Phase.** Phase 1 — ships first, alongside colour
helpers (§6.12) and the unified `Animator` trait (§6.15). Zero
grammar changes; entirely `prism-ui-runtime` + a couple of
state-tracker rows in `prism-shell`. The authoring surface for
these states (verbose today, collapsed in §6.6) is independent
of the runtime work.

**What it deletes / supersedes.** Nothing — additive only,
except the implicit "disabled is purely visual" assumption
(Q10).

**Open questions.** None — Q10 resolved against dispatcher
suppression.

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

#### Multi-component files + file-declared namespaces (Q6 / Q11)

Both `.prui` files (with multiple top-level `<component>` /
`<trait>` / `<mixin>` / `<macro>` wrappers) and `.luau` files
(returning a table of `prism.<kind>{…}` entries plus bare
helpers) may carry **more than one declaration**.

**The C# rule.** The file decides its own namespace via a
`<namespace=Ns/>` directive at the top (`.prui`) or a
`namespace="Ns"` field in the Luau-side `prism.module{…}`
builder. Every top-level declaration in the file binds under
that namespace. Files without a namespace declaration bind
their declarations bare. `as=` on `<import>` overrides the
file's declared namespace. Filename → tag mangling retires
entirely.

```prui
<!-- single-component file, no namespace: bare bind -->
<!-- ./card.prui -->
<component Card …>…</component>
```

```prui
<!-- multi-component file, file-declared namespace -->
<!-- ./forms.prui -->
<namespace=Forms/>
<component TextField …>…</component>
<component DropdownField …>…</component>
```

```prui
<!-- consumer side -->
<import "./card.prui"/>           <!-- registers <Card/> -->
<import "./forms.prui"/>           <!-- registers <Forms.TextField/>, <Forms.DropdownField/> -->
<import "./forms.prui"/> as f      <!-- override → <f.TextField/> -->
```

Bare-bound declarations collide → parse error (Q1).

#### Luau side — file-declared namespaces + multi-export

A `.luau` file's `return` value is one of:

1. A `prism.module{namespace="Ns", entries={…}}` table → all
   entries bind under `Ns` automatically.
2. A single `prism.<kind>{…}` table → registers as that one
   thing (component / trait / mixin / macro / dialect), bare.
3. A **table of named entries** → each entry registers under
   its key. Entries can mix kinds, and can include bare Lua
   functions / values for helpers. Bare by default; `as=` on
   `<import>` lifts them under a namespace.

```luau
-- card-system.luau
return prism.module {
  namespace = "Cards",                        -- C#-style file namespace
  entries   = {
    -- Components — registered as tags
    Card     = prism.component { props = …, render = … },
    Avatar   = prism.component { props = …, render = … },

    -- Other extension kinds — registered with the appropriate registry
    Pointable = prism.trait { attrs = … },
    Hoverable = prism.mixin { … },
    Field     = prism.macro { pattern = …, expand = … },

    -- Bare helpers — available as expression values under the namespace
    helpers = {
      format-date = |t| os.date("%Y-%m-%d", t),
      accent-for  = |tone| if tone == "danger" then "#ef4444" else "#3b82f6" end,
    },
  },
}
```

Imported as `<import script="./card-system.luau"/>` (no `as=`,
file's own namespace wins):

- `<Cards.Card title="Hi"/>` — component invocation
- `<container with=[Cards.Hoverable]>` — mixin reference
- `<component MyForm impls=[Cards.Pointable]>` — trait reference
- `<Cards.Field label="Title"/>` — macro expansion
- `{Cards.helpers.format-date(now)}` — bare helper called in
  expression context

Or imported as `<import script="./card-system.luau"/> as cs`
to override — `<cs.Card/>`, `<cs.helpers.format-date(now)>`, etc.

**Same `<import>` element, same resolver, same override rule.**
The user picks the *lens* via the projection: `component`
imports the components only; `script` imports everything in
the returned table (components, traits, mixins, macros,
dialects, AND helpers).

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
multi-component file support + `<namespace=…/>` directive:
Phase 7. The full `prism.<kind>{…}` Luau-builder family lands
incrementally as each kind ships (mixins Phase 9, macros
Phase 10, etc.).

**What it deletes / supersedes.** The `widget` projection
keyword; the dead `interpret.rs:1018` handler skip; the
doc-only `prism.widget{…}` builder; the standalone `contract`
projection (collapsed — contracts are body-less `<trait>`s
imported as components or scripts); the earlier filename →
PascalCase tag mangling rule (replaced by file-declared
namespaces — see Q6).

**Open questions.** All resolved as of 2026-05-20. Q1 (parse
error on first collision), Q6 (file-declared namespaces, not
filename mangling), Q7 (mixed-kind `.luau` returns), Q11
(namespaced imports + file-declared namespaces). See §8.

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

**Open questions.** None — Q4 resolved in favour of chroma
reduction (preserves hue; sacrifices saturation when out of
gamut). Documented in `prss-reference.md` with the Phase 1
landing.

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
| Three deferred animation namespaces (`Transition` / `Animate` / `At`) | `ast.rs:108-149` | unified `Animator` trait (§6.15) — pipeline-shape value over time + entry/exit transitions + keyframe records. Phase 1 lands the trait + runtime; Phase 3 retires the now-deprecated namespace labels once the 5 live `animate:` callers have migrated to the pipeline syntax. | 1 (trait + runtime), 3 (label cleanup) |
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
zero `transition:`, zero `at:`, zero `widget=`. The single
partial exception is `animate:` (5 files, overlay fade-ins) —
that namespace gets *finished*, not deleted (§6.4 fate row).
Otherwise the cleanup is overwhelmingly about not shipping
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

#### Q13 resolution — open schema fields by default

Every `ComponentSchema` field gets a **typed string + runtime
registry lookup** in the generated artifacts, *including* the
fields that look closed today: the property `type` set, the
state-suffix set, the event-kind set, the capability names,
the mixin / trait / macro names. The few hard-coded primitives
(`string` / `int` / `bool` / `color` / `length` / `action` /
`enum` / `union` / `array` / `object` / `slot` — the §6.2 type
set) ship as registered entries in the default registry, not
as hand-rolled Rust enums in codegen.

```rust
// Auto-generated from a card schema — props carry typed
// strings backed by the registry, not hand-rolled enums.
pub struct CardProps {
    pub title: String,                  // primitive: registry["string"]
    pub tone:  EnumValue<"CardTone">,   // registry-resolved enum
    pub on_click: Option<Callback>,     // primitive: registry["action"]
}
```

**Why open by default.** Wave L's whole thesis is that the
attribute / trait / mixin / macro surfaces *open* to user
extension. Forcing parts of the schema to be closed in codegen
would split the codegen story across "generates Rust enums"
vs "generates registry lookups" — two flavours of consumer
code for the same artifact. Forward compatibility is also
preserved unconditionally: a Luau library that registers a
new property type (`measurement: Length<m | px | pt>`) shows
up in the typed handle without a Prism release. Compile-time
safety is paid for with one extra `.unwrap()` per registry
lookup at construction time — acceptable in exchange for the
"add a type, ship a Luau file" property.

#### Other open questions

- Whether the schema codegen runs at compile time (build.rs +
  proc-macros) or at runtime (when the registry first loads).
  Lean: compile time, so the typed handles ship as part of
  the consumer's binary with no init cost. Runtime fallback
  for hot-reloaded components.
- Where `prism-luau-derive` lives after the split. Probably
  becomes a thin shim in `prism-builder` (no separate crate)
  once the universal codegen lives there.

---

### 6.15 Animations and transitions — unified `Animator` trait

**Problem.** Three deferred namespaces (`transition:`,
`animate:`, `at:`) each promise to do a slightly different
thing: `transition:opacity="200ms"` interpolates a mid-life
value change; `animate:opacity="0 200ms"` interpolates an
entry transition from a "from" value; `at:50%` declares a
keyframe stop in a multi-stop timeline. Today only `animate:`
has a partial lowering to `data-animate-in-*` (5 production
files use it for overlay fade-ins — see §4 and §6.13);
`transition:` and `at:` parse but have no runtime. The three
are conceptually one thing — *time + value interpolation* —
split across three namespaces with no shared design. Q3 (§8)
resolves to: ship now, unified.

**Design.** Merge the three into one `Animator` trait, in the
same shape the trait registry uses (§6.4). Three call surfaces
cover the three use cases, sharing one engine:

- **Mid-life value change** — pipeline-shape over time on the
  value side, mirroring the §6.6 state pipeline:
  ```prui
  <container style.opacity={
    1
    | :hover → 0.6 over 200ms                <!-- mid-life smoothing -->
  }>
  ```
- **Entry / exit transitions** — declared as `:entry` / `:exit`
  pseudo-states with time + from/to:
  ```prui
  <container style.opacity={
    1
    | :entry → from 0 over 200ms             <!-- fade-in on observe -->
    | :exit  → to   0 over 150ms             <!-- fade-out on un-observe -->
  }>
  ```
- **Keyframe stops** — nested record on the `animator` trait
  for multi-stop timelines:
  ```prui
  <container animator.keyframes={
    duration = 800ms,
    0%       = { opacity = 0 },
    50%      = { opacity = 1, transform = scale(1.05) },
    100%     = { opacity = 0.5 },
  }>
  ```

**Three namespaces → one trait.** Same value-pipeline shape
covers mid-life smoothing, entry/exit transitions, and
keyframe timelines. Authors stop reaching for three different
prefixes to express "this value changes over time."

```prui
<!-- Wave J state (today) -->
<container
  transition:opacity="200ms"               <!-- mid-life value change -->
  animate:opacity="0 200ms"                <!-- entry transition -->
  at:50%="{ opacity: 1 }"/>                <!-- keyframe stop -->

<!-- Phase 1 end state — one trait, three idiomatic shapes -->
<container style.opacity={
  1
  | :hover → 0.6 over 200ms
  | :entry → from 0 over 200ms
}>
```

**Runtime.** The `Surface` retained-mode renderer already has
hooks for per-frame ticking. The `Animator` trait dispatches
into those hooks; `:entry` / `:exit` fire on observe /
un-observe; keyframes interpolate along the trait-specified
curve (default: cubic-bezier ease, OKLCH lerp for colours so
fades pass through the perceptual gamut, not sRGB grey).

**Wave / Phase.** Phase 1 — ships alongside pseudo-state
runtime (§6.7) and colour v2 (§6.12). All three are
`prism-ui-runtime`-internal; no grammar surgery beyond
recognising `animator.<method>=` in the existing attribute
resolver, which Phase 0 makes easier by splitting
`interpret.rs`.

**What it deletes / supersedes.** The `transition:`,
`animate:`, `at:` attribute namespaces (subsumed into the
`animator` trait — the namespace labels themselves retire in
Phase 3 once callers have migrated). The deferred-runtime
story for animations. The "ship or delete in Phase 3"
framing — Q3 (§8) resolved against deletion.

**Open questions.** None blocking. Exact pipeline syntax for
keyframes (nested `animator.keyframes={…}` vs an inline
`| :keyframe(50%) → …` shape) lands with Phase 1
implementation; both are sketches above.

---

## 7. Phased rollout

One unified plan across Waves J + K + L, ordered by readiness.
Each phase is independently shippable: tests pass, no other
phase depends on the next-in-line. **Phase 0** is a pre-feature
code-cleanup pass (file splits + stale-doc fixes + dead-path
audit) that every later phase benefits from. Phase 5 is
retained as a skipped row for traceability — its blocking
decisions (Q1 + the §6.1 fork) all resolved on 2026-05-20.

| # | Phase | Wave tag | Scope (§ refs) | Prereqs | Reversibility | LOC delta |
|---|---|---|---|---|---|---|
| 0 | Code cleanup — file splits + stale-doc + dead-path audit | (cleanup) | §7.0 below | none | reversible (pure refactor) | ~0 net (code moves, doesn't go away); +200–400 from `ui_lower` test additions |
| 1 | Colour v2 + pseudo-state runtime + unified `Animator` trait | J Phase 1 | §6.12 + §6.7 + §6.15 | Phase 0 (recommended) | reversible | +600, ~0 deleted (Animator subsumes three deferred namespaces, runtime shipping) |
| 2 | Facet trinity + Route deletion | K.1 + K.2 | §6.13 rows 1–5 | none | reversible (git revert) | −170 |
| 3 | Retire deprecated namespace labels (`transition:` / `animate:` / `at:`) + `Use` namespace audit | K.3 | §6.13 row 7 + row 8 (now post-migration cleanup) | Phase 1 (Animator wired) + Phase 9 (`derive=` lands) | reversible | −80 to −120 |
| 4 | Slot / host-children unification | K.4 | §6.5 (no-signature variant) | none | reversible during deprecation | −80, 9 files edited |
| 5 | ~~Component A/B/C fork decision~~ — **skipped** (Q1 + §6.1 fork resolved 2026-05-20; row retained for traceability) | K.5 | §6.1, Q1 | n/a | n/a | n/a |
| 6 | Component declarations + properties + `extends` | J Phase 2 | §6.1, §6.2, §6.3 (extends only) | Phase 0 (recommended) | reversible until widely adopted | +400 |
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
| 17 | Computed property defaults (`<= {expr}` second tier) | (Q2) | §6.2 ("Defaults and `required`") | Phase 6 (declarations) + PRSS extends-chain topology checker (already shipped) | reversible | +150 |

### Phase 0 detail — code cleanup before the feature waves

A pre-feature pass that splits oversized files, adds the
missing tests on the riskiest module, and fixes stale internal
docs. No language features ship here; all of it removes
obstacles to landing Phases 1–16 cleanly.

**File splits.** Six files account for most of the friction
every later phase will hit:

| File | Lines | Split plan | Driving phase(s) |
|---|---|---|---|
| `prism-ui-runtime/src/interpret.rs` | 10,882 | The single biggest impediment. Split along the existing `// ───` section dividers into `document.rs` (top-level walk + imports), `elements.rs` (`lower_element` + body), `control_flow.rs` (`expand_match` / `expand_suspense` / `expand_language` / `expand_control_flow`), `expression.rs` (binding lookup + interpolation), `style.rs` (`STATE_SUFFIXES`, hover overrides, colour helpers). The 225 `#[test]` blocks travel with their parent functions. | 1, 6, 8 — every phase touches this file |
| `prism-shell/src/events.rs` | 5,185 | Extract `EventModifiers` state machine into a sibling module; keep `dispatch_event` as the router facade. | 1 — `:pressed` / `:disabled` / `:focused` / `:focus-within` wiring writes here |
| `prism-ui-runtime/src/layout/mod.rs` | 2,709 | Split grid / flex compute from the node → node transform propagation pass. | 14 — state-variant overrides thread through layout |
| `prism-core/src/language/prism_ui/grammar.rs` | 1,566 | Audit whether the lexer is cleanly separable from the recursive-descent parser; extract if so. | 6, 8 — declaration syntax + attribute surface both rewrite the grammar |
| `prism-builder/src/{starter,primitives}.rs` | 1,001 + 1,363 | Both are declarative `BlockSpec` tables (16 + 1 facet, and 14 shell-primitive specs). Consolidate into one module, or formalise the split via a shared manifest. | 6 — the §6.1 PascalCase rule folds these tables under one component-tag dispatch |
| `prism-builder/src/ui_lower/mod.rs` | 1,319 | Split style resolution from node factory. (Coverage gap closed during the Phase 0 audit: 19 unit tests now exist for `lower_as` / `BlockInvalidator` / `hover_bg` / `prop_*` helpers / `modifier_fold`, so the split lands behind a real safety net.) | 6, 8 — modify this module |

**Stale-doc + dead-path fixes.**

- `prism-builder/CLAUDE.md:190-195` — strike the stale `FacetDef`
  / `FacetKind` / `FacetDataSource` / `FacetTemplate` /
  `FacetOutput` / `FacetBinding` / `FacetLayout` / `AggregateOp`
  / `ScriptLanguage` / `FacetVariantRule` / `ResolvedFacetData`
  / `FacetSchema` / `SchemaField` / `SchemaFieldKind` /
  `FacetRecord` / `ValidationError` / `FACET_KIND_TAGS` /
  `AGGREGATE_OP_TAGS` catalogue (none compile — see §4 and
  `prism-builder/src/facet/mod.rs:8`). Replace with an accurate
  one-paragraph note of what `facet/` actually holds today
  (`FacetComponent` + `resolve_template_expressions`, ~25 lines).
- Dead `widget=` import projection (`interpret.rs:1018, 1210`) —
  decide: rename + wire as `component=` now (subsumes Phase 7's
  rename), or rip out the parse path and re-add in Phase 7.
  Vestigial grammar attracts drift; the same shape applies to the
  `Use` namespace (`ast.rs:115-121`, lowers to `data-use-*`, no
  live consumer; Phase 9's `derive=` supersedes).
- §4 inaccuracies surfaced during the audit, fixed in this phase:
  (1) bare `interpret.rs:NNN` cites live in `prism-ui-runtime`,
  not `prism-core` (cite column annotated above);
  (2) `BUILTINS` is 16 `BlockSpec` rows + 1 facet `Component`,
  not "17 `BlockSpec` rows" (cite column updated);
  (3) `animate:` has 5 production files / 10 instances of use
  with partial `data-animate-in-*` lowering, not zero (§6.4 and
  §6.13 rows updated).

**Why before Phase 1?** Every §6 feature beyond Phase 1
modifies `interpret.rs`, `grammar.rs`, and / or `events.rs`.
Touching 10k- and 5k-line files is the largest reviewer-friction
surface in the codebase. The grammar / runtime surgery in
Phases 6 + 8 is dramatically more reviewable on focused
1,500-line modules than on slices of a 10,000-line file. The
stale `prism-builder/CLAUDE.md` catalogue is a trap for any
author who reads docs to orient — fixing it makes Phase 2's
facet-trinity deletion mechanical instead of archaeological.

**Reversibility, LOC, ordering.** Pure-refactor PRs, no
behaviour change — every split is `git mv` plus pub re-exports
from a thin `mod.rs`. ~0 net LOC for the splits (code moves,
doesn't go away); +200 to +400 LOC for the `ui_lower` test gap.
Order within the phase: doc fixes first (cheapest, lowest
risk), then `ui_lower` tests, then file splits smallest-first
(grammar.rs → events.rs → layout/mod.rs → interpret.rs).
`interpret.rs` lands last so other phases' diff with HEAD stays
small for as long as possible.

**Out of scope.** Public-API renames (those land in their
feature phase, not here). Algorithm changes — Phase 0 is
structural rearrangement only. New tests beyond the `ui_lower`
gap; existing test surfaces travel with their parent functions
during splits.

### Pacing

Phase 0 ships in ~2–3 weeks of pure-refactor PRs (no behaviour
change). Phases 1–4 are safe near-term wins (a week or two
each). Phase 5 is a skipped row — once Q1 and the §6.1 fork
resolved, no decision-only phase blocks Phase 6. Phases 6–8
form the structural middle — Phase 8 is the largest single
slice and unlocks Phases 9–15. Phase 16 is ongoing. Phase 17
(computed defaults — Q2) can ship any time after Phase 6 but
is sequenced last to keep the type-set additions out of the
larger structural rewrites.

Total LOC delta across all 18 phases (Phase 0 cleanup +
Phases 1–17 features): approximately +3500 added, −700
deleted (counting only surfaces in this doc; test code is
excluded; Phase 0's pure-refactor moves don't count toward the
net). The net is +2800 across roughly a year of part-time
work — a significant but not unreasonable expansion for the
value the table in §10 returns.

---

## 8. Open questions

Numbered for cross-reference. All thirteen are **resolved** as
of 2026-05-20 — each row records the question, the decision,
and where the resolution lands in the design.

**Q1. Bare (unnamespaced) multi-component import — collision
handling.** ✅ **Resolved: (a) parse error on first
collision** — strictest, easiest to reason about, and matches
the namespace-aware design from Q6 / Q11. Two files declaring
the same namespace + same component name → parse error. Two
files without a namespace declaring the same bare component
name → parse error. CSS-like last-wins and first-wins were
both rejected for being implicit. Lands with Phase 7.

**Q2. Computed property defaults.** ✅ **Resolved: yes —
second tier ships.** A `padding: int <= {row.depth * 4}`
extension to the inline typed-prop syntax (or
`computed-default={…}` as a header attribute) is committed.
Topologically resolved across the property graph with the same
cycle-detection logic the PRSS extends-chain validator uses.
First cut requires the computed expression to reference only
*declared* properties on the same component (no nested
computeds) — keeps the topology trivial. See §6.2's
"Computed defaults" subsection; lands in Phase 17.

**Q3. Tier 3 namespace audit (Phase 3).** ✅ **Resolved:
ship Animations + Transitions in Phase 1 under a unified
`Animator` trait.** They're important enough to deserve real
design + implementation love now, not "later wave" deferral.
The unified design (§6.15) merges `transition:` (mid-life
value-change interpolation), `animate:` (entry / exit
transitions), and `at:` (keyframe stops) into one trait
carrying transition curves, keyframe sets, and entry/exit
animations as trait methods — three namespaces become one
trait. Phase 1's scope grows accordingly; Phase 3 shrinks to
the `Use` namespace decision only.

**Q4. OKLCH gamut clipping.** ✅ **Resolved: chroma
reduction.** Out-of-gamut OKLCH values project back to sRGB
via the `palette` crate's chroma-reduction path (preserves
hue, sacrifices saturation). Hue rotation was rejected — it
shifts the *perceived* colour, which is exactly the property
designers preserve by reaching for OKLCH in the first place.
Documented in `prss-reference.md` when Phase 1 lands.

**Q5. Trait self-reference.** ✅ **Resolved: yes —
`recursive` keyword enables it.** A body-less or full
`<trait>` may reference itself when its declaration carries
the `recursive` flag (`<trait Composite, recursive, …/>`).
Without `recursive`, self-references at type-check time fail
with a clear "this trait isn't marked `recursive`" error. The
keyword also extends to mutually-recursive trait clusters
(declare both with `recursive`). Lands with Phase 8 (trait
registry).

**Q6. Tag-name registration — how does a component bind to a
tag?** ✅ **Resolved: file-declared namespaces (C#-style),
not filename mangling.** The original "PascalCase-normalised
filename = tag name" rule had too many edge cases (dots,
numerics, leading underscores, all-caps, non-ASCII, primitive
collisions). Replaced by an explicit `<namespace=Ns/>`
directive at the top of a `.prui` file (and `namespace=` on
the Luau-side `prism.module{…}` builder), which binds every
top-level declaration under that namespace. Files without a
namespace declaration bind their declarations bare; `as=` on
an `<import>` overrides the file's namespace. Filename → tag
mangling retires entirely. See §6.1 (Part 3) and §6.11 for
the full design; lands with Phase 6.

**Q7. `.luau` component vs `.luau` script in the same file.**
✅ **Resolved (§6.1, §6.11):** a `.luau` file returns a
*table* of named entries that can mix `prism.<kind>{…}`
records of any type plus bare helpers. The `<import>`
projection acts as the *lens*: `component` binds only the
components; `script` binds every entry (components + traits +
mixins + macros + dialects + helpers). Same file viewed
through different projections exposes different subsets.

**Q8. Pipeline `|` vs logical-or.** ✅ **Resolved:** the
pipeline form is parsed only in value positions typed
`Stateful<T>` or `Animated<T>`; bare `|` elsewhere is
logical-or.

**Q9. Variant precedence with chained prefixes.** ✅
**Resolved:** latest wins, same as Tailwind. Mixin
linearisation (§6.3) is the same rule applied at the trait
registry layer.

**Q10. Disabled-state pointer behaviour.** ✅ **Resolved:
`:disabled` suppresses `on:click` at the dispatcher**, not
only at the style layer. Greyed-out buttons that still fire
their callback are a footgun the language shouldn't ship.
Audit + migration of any code relying on the old "style-only"
semantics is part of Phase 1. See §6.7.

**Q11. Trait coherence in cross-library use.** ✅
**Resolved: namespaced imports OR file-declared namespaces.**
Two unrelated libraries can both declare `Draggable` — the
consumer disambiguates either by importing with `as ns`
(`<import script="./libA.luau"/> as libA` →
`with=[libA.Draggable]`) or by relying on each library's own
`<namespace=…/>` declaration (Q6). Bare `with=[Draggable]` is
a parse error when two unprefixed `Draggable`s are in scope.
Same mechanism applies to traits, mixins, macros — anything
the trait registry holds.

**Q12. PRSS syntactic shape — CSS-Nesting vs YAML.** ✅
**Resolved: CSS-Nesting.** `&:state` selectors + brace-expr
values + pipeline forms (§6.6). YAML is rejected outright —
expression-heavy content reads poorly in YAML, the pipeline
shape doesn't map cleanly, and the `&:hovered` keys need
quoting. The portability benefits don't outweigh the
authoring-ergonomics loss. §11.12 ("PRSS as YAML — the deeper
exploration") retires.

**Q13. Closed vs open schema fields in cross-language
codegen.** ✅ **Resolved: open by default.** Every
`ComponentSchema` / `TraitSchema` / etc. field gets a typed
string + runtime registry lookup, *including* the entries
that look "closed" today (the property `type` set, the
state-suffix set, the event-kind set). The few cases that
remain closed (primitive types like `int` / `string` / `bool`
in property declarations) ship as registered entries in the
default registry, not as hand-rolled Rust enums. Mechanically
this means the codegen surface in §6.14 generates typed
strings everywhere; the trait registry holds the canonical
allow-lists; new entries land via Luau or Rust registration
calls, not via grammar edits. Forward compatibility is
preserved unconditionally. Lands with the §6.14 schema
codegen pipeline (Phase 8.5 + Phase 16).

---

## 9. Rejected alternatives

A consolidated list of ideas considered and dropped. Listed
so future debates can find the prior reasoning instead of
relitigating.

- **The long-form closure `\fn(args) … end`** (originally an
  alternative to `|args| expr` in fusion §7.2). Single
  parameter syntax going forward: `|args|` for both the
  single-expression form (`|args| expr`) and the multi-line
  block form (`|args| { … }`, §12.6). Rationale: two
  *parameter* spellings for one concept is a Wave K-style
  violation of the "keep the surface narrow" principle (§2);
  one parameter form with two body shapes (expression or
  block) covers every case the long form did with less
  surface. The block form is the multi-line escape hatch
  authors wanted from `\fn`; the parameter form stays uniform.
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
| Button-but-louder-hover variant | new PRSS class + new component (~25 lines) | `component LoudButton = Button with { tone = loud }` (1 line) |
| Lighter / darker / transparent variant of a token colour | hand-tuned hex per call site | `accent/50`, `with(accent, l=+0.05)` (1 expr) |
| Pressed-state visual feedback | script-toggled class + PRSS variant (~12 lines) | `style.background={accent \| :pressed → darken(0.1)}` (1 line) |
| 3 props × 4 states on one component | 12 lines (cross-product) | 7 lines (nested record) |
| 5 buttons sharing one hover/press/disabled curve | 20 lines | 6 lines (`@color` + 5 refs) |
| Reusable elevation system across the workspace | 6 near-identical classes copy-pasted | 1 attribute macro (`macro Elevation attribute(level: int) { … }`) |
| Dark-mode override on one element | duplicate class with `dark-` prefix | `with=[Card, dark ? Muted : nil]` |
| Form field that requires a focusable control | manual prop binding + runtime assertion | `control: \|\| → Focusable` (1 typed param) |
| Required prop with a sensible default | host binding + nullable check in body | `label: string = ""` in the parameter list |
| Import a `.prui` component from a sibling directory | impossible (no runtime wiring) | `import "../widgets/card.prui" as card` |
| Author a component imperatively from Luau | impossible (`prism.widget{…}` never written) | `return prism.component{…}` in a `.luau` file |
| Stack draggable + hoverable + selectable on a container | hand-roll state + handlers + styles (~40 lines) | `use Draggable, Hoverable, Selectable` inside the body (1 statement) |
| Define a new attribute kind (e.g. `elevation=2`) | impossible without engine release | `macro Elevation attribute(level: int) { … }` or `prism.trait{…}` |
| Component needs clipboard access | thread global through props | `requires clipboard: Clipboard` (body statement) plus `clipboard.write(…)` |
| Toast with three variants carrying different fields | boolean prop ladder | `type Tone = info \| success(d: int) \| error(…)` + `<match>` body |
| Multi-component file (form-field family, chart system, …) | one component per file → many files, many imports | multiple `component` declarations in one `.prui` file; `import "./forms.prui" as forms` namespaces all of them |
| Luau library exporting components AND helpers AND traits in one file | impossible — `.luau` returns one value | return a table of mixed entries; `import "x.luau" as ns` exposes the whole table under `ns` |
| Multi-line event handler | concat-strings into `<script>` block | `on click(e) { stmt; stmt; … }` (in-line block) |
| Optional capability or action prop | nested nil-checks | `on-click?(e)`, `network?.post(…)`, `state.title ?? "Untitled"` |
| Stateful counter in 4 lines | not possible without a custom block | `component Counter(initial: int = 0) { let n = state(initial); on click { n <- n + 1 }; <button @click=$click>{n}</button> }` |
| Pure tree-returning helper | not first-class | `fn priority-tag(p: Priority) → ui = match p { high → <fire/>, _ → <leaf/> }` |

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
| XML declaration wrappers (`<component>`, `<state>`, `<on>`, `<style>`, `<import>`) + header-attr clauses | Function-shape declarations with English clause keywords + `{ … }` blocks; `import "path"` directives (§12) |
| Two closure spellings (`\|args\| expr` and `\fn(args)…end`) | One parameter syntax (`\|args\|`), two body shapes (expression or `{ … }` block) |
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

### 11.2 Computed property defaults — *resolved, promoted to §6.2 / Phase 17*

Q2 (§8) committed to shipping the second tier. The `<= {expr}`
arrow on a property declaration signals a computed default;
topology resolved at parse time via the PRSS extends-chain
cycle detector. First cut requires the expression to reference
only *declared* properties (no nested computeds). See §6.2's
"Defaults and `required`" subsection for the design; Phase 17
in §7 for the rollout slot.

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

The basic `prism.module{namespace="Ns", entries={…}}` builder
ships with Phase 7 (§6.11, post Q6/Q11 resolution) — that
covers ~70% of the OCaml-style module ergonomics by giving
files an explicit namespace + a multi-export table. The
remaining tier-2 ambition is **parametrically polymorphic
modules**: a `prism.module{…}` that takes type / trait
parameters and yields a fresh namespace-scoped bundle on
instantiation (`local IconSet = make-icon-set("filled")`).
Strongest use case: library distribution where the same
component family wants to specialise on a style axis without
the consumer re-declaring every prop. Deferred until the
schema codegen (§6.14) stabilises and we know whether
runtime-time module instantiation breaks the typed-handle
story.

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

### 11.10 Tier 3 namespaces — *resolved, promoted to §6.15*

The `Transition` / `Animate` / `At` namespaces lifted out of
"future musings" once Q3 (§8) committed to shipping in Phase 1.
See §6.15 for the unified `Animator` trait design (pipeline
value + time, entry/exit transitions, keyframe records). The
`Use` namespace is replaced by Phase 9's `derive=` attribute
(§6.3); the namespace label retires in Phase 3.

### 11.11 Mid-decision forks not yet locked in

These are explicit branches the doc currently presents both
options for. They become hard decisions once their gating
phase arrives:

- Whether the `component` projection (§6.11) survives once the
  universal Luau-builder family is in place. With Q6 resolved
  (file-declared `<namespace=…/>` replaces filename mangling),
  `component` is closer to "filter the script-table return for
  `prism.component{…}` entries only" than to a true alternative
  projection. Possibly retires in favour of `script` plus
  per-component filtering at the call site.
- Whether `Block` and `Component` (Rust traits) collapse into
  one. Today `Block` is "single-trait sugar" over
  `Component`; if the registry's surface narrows to one trait,
  the name to keep is `Component` (matches the noun).
- Whether `Aria` and `Data` namespaces survive as labels once
  the trait registry can express them generically
  (`a11y.role`, `data.role`). Lean: keep — they read better
  in markup than the generic form.

### 11.12 PRSS as YAML — *rejected*

Q12 (§8) is resolved against YAML and in favour of
CSS-Nesting. Expression-heavy content reads poorly in YAML,
the pipeline shape doesn't fit, `&:hovered` keys need quoting,
and significant whitespace breaks copy-paste from
chat / docs / inspectors. The portability benefits don't
outweigh the authoring-ergonomics loss. Retained as a slot
here so future debates can find the prior reasoning instead
of relitigating; the original sketch lived in the git history
of this file (commit before 2026-05-20).

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
- **A second closure *parameter* form** (alongside `|args|`).
  The canonical surface (§12.6) extends the closure with a
  multi-line **body** shape (`|args| { … }`) — same `|args|`
  parameter syntax, just an expression body OR a brace block.
  What's still rejected is a *second parameter syntax*: the
  fusion doc's `\fn(args) … end` alternative would have given
  the same concept two spellings. One parameter form, two
  body shapes is the single-form rule (§2, §9).

---

## 12. The canonical surface — programs as expressions

The XML form spells everything as markup: declarations,
clauses, body sub-tags, even imports. The previous draft of
this section moved declarations to function-shape headers
but kept the same skeleton — clauses between the params and
the body, body sub-blocks (`state`, `on`, `style`) in a
fixed order, mixin/trait/capability composition wired
through declaration-time attributes. **That's still XML in
brace clothing.**

The real move is one gesture: **every declaration is `name
= expression` (or `name(params) { body-expression }`).** A
component is a typed function whose value is a tree. A type
is an algebraic expression. A trait is a structural shape.
A mixin is a behaviour bundle you call into a scope. Style,
state, handlers, capabilities — all become **statements
inside the body**, not clauses around it. The result is a
surface where:

- The favourite line — `component Avatar(src: string, size:
  px = 32) { <image src={src} width={size} height={size}/> }`
  — is the *common* case, not an edge case. Anything more
  complex adds *statements inside the body*, never clauses
  outside it.
- `trait Marker {}` is a real declaration. `type Result<T,
  E> = ok(value: T) | err(error: E)` is a real
  declaration. The same `Name = expression` gesture covers
  trivial and rich cases.
- Tags and code blend bidirectionally: trees are expression
  values, expressions are tree children. Author picks the
  shape that reads cleanest at each spot.

**The §6 feature set is unchanged.** Every discriminated
union still discriminates; every mixin still mixes in;
every capability still gets injected at lower-time. The
features stay; the surface collapses to one gesture.

### 12.1 The full surface in one file

Before any feature breakdown, here is a realistic
`apps/tasks/shell.prui` showing the whole language under
load. Reading top-to-bottom: imports → types → traits →
helpers → styles → mixins → presentational components →
the stateful main component → a derived variant → a macro.
Every §6 feature appears; reading this is the language's
real shape under typical work, not a happy-path snippet.

```prui
-- ./apps/tasks/shell.prui — task tracker, full feature exercise
namespace TasksApp

import "./theme.prss"
import "./db.luau"    as db
import "./icons.luau" as icons

-- ───── TYPES ────────────────────────────────────────────────

type Priority = low | medium | high | critical

type Task = {
  id:       string,
  title:    string,
  notes:    string?,
  priority: Priority,
  due:      timestamp?,
  done:     bool,
}

type Filter = {
  search:    string,
  show-done: bool,
  by:        all | priority(Priority) | overdue,
}

type Saving<T> = idle | saving | saved(T) | failed(message: string)

-- ───── TRAITS (structural shapes — used as type bounds and slot contracts) ──

trait Focusable {
  focus:   action
  blur:    action
  focused: bool = false
}

trait Editable<T> {
  value:     T
  on-change: |T| → unit
}

-- ───── PURE HELPERS — `name(args) = expr` for one-liners ───────────────────

fn priority-rank(p: Priority) → int = match p {
  critical → 0,  high → 1,  medium → 2,  low → 3
}

fn priority-color(p: Priority) → color = match p {
  critical → tokens.colors.red.bold
  high     → tokens.colors.orange.base
  medium   → tokens.colors.blue.base
  low      → tokens.colors.gray.muted
}

fn sort-by-priority(tasks: array<Task>) → array<Task> =
  tasks.sort(|a, b| priority-rank(a.priority) - priority-rank(b.priority))

fn matches(t: Task, f: Filter) → bool = {
  let by-text = f.search == "" or t.title.lower().contains(f.search.lower())
  let by-done = f.show-done or not t.done
  let by-cat  = match f.by {
    all          → true
    priority(p)  → t.priority == p
    overdue      → t.due != nil and t.due < now()
  }
  by-text and by-done and by-cat
}

-- ───── INLINE STYLES — `class Name { PRSS body }` ──────────────────────────

class app-shell {
  background = surface
  padding    = 24
  gap        = 16
  direction  = column
}

class task-row {
  padding = (10, 14)
  radius  = 6
  &:hovered { background = surface.alt }
  &.done    { opacity = 0.5, decoration = strikethrough }
  &.urgent  { border-left = (3, solid, accent) }
}

class chip {
  padding    = (2, 6)
  radius     = 4
  font-size  = 11
  background = surface.alt
}

-- ───── MIXINS — behaviour bundles you `use` inside a body ──────────────────

mixin Hoverable {
  let hovered = state(false)
  on pointerenter { hovered <- true }
  on pointerleave { hovered <- false }
}

mixin AutoFocus {
  on mount { self.focus() }
}

-- ───── PRESENTATIONAL COMPONENTS — one-line where possible ─────────────────

-- Pure prop → tree (the favourite gesture)
component Chip(text: string) = <text class=chip>{text}</text>

-- One stateless helper component, one match expression
component PriorityBadge(level: Priority) = {
  let symbol = match level {
    critical → "‼",  high → "!",  medium → "·",  low → " "
  }
  <text class=chip style.color={priority-color(level)}>{symbol}</text>
}

-- Slot-with-children pattern (children is a magic-named typed param)
component Card(title: string, children: ui) = {
  use Hoverable                                 -- mixin: brings `hovered` into scope

  style { padding = 16, radius = 12 }
  
  <container direction=column gap=8>
    <heading 3>{title}</heading>
    {children}                                  -- splice children into the tree
  </container>
}

-- A presentational row, with a trait annotation, no body state
component TaskRow(
  task:      Task,
  on-toggle: |Task| → unit,
  on-edit:   |Task| → unit = |_| unit,
) : Focusable = {
  let editing = state(false)

  on click(e) {
    if editing then unit else on-toggle(task)
  }

  on dblclick(e) {
    editing <- true
    on-edit(task)
  }

  <container
    class={"task-row"
      .. (if task.done                  then " done"   else "")
      .. (if task.priority == critical  then " urgent" else "")}
    @click=$click(_)
    @dblclick=$dblclick(_)>

    <PriorityBadge level={task.priority}/>
    <text style.flex-grow=1>{task.title}</text>

    {if task.due != nil  then <Chip text={format-due(task.due)}/>}
    {if editing          then <icon name=pencil tint={tokens.accent}/>}
  </container>
}

-- ───── THE BIG COMPONENT — every body statement, every gesture ─────────────

component TaskList(
  -- Required prop
  initial-tasks: array<Task>,

  -- Optional callbacks — slot-as-function defaults
  on-pick:  |Task| → unit    = |_| unit,
  on-edit:  |Task| → unit    = |_| unit,
  on-new:   |string| → unit  = |_| unit,

  -- Slot-as-tree defaults
  header:   ui               = <heading 2>Tasks</heading>,
  empty:    ui               = <text class=muted>No tasks yet.</text>,

  -- Slot-as-function the caller can override
  toolbar:  |Filter| → ui    = |f| <text>{summarize-filter(f)}</text>,
) = {
  -- Capabilities — `requires` is a body statement, not a clause
  requires storage:   Storage
  requires clipboard: Clipboard?

  -- Reactive state
  let tasks  = state(initial-tasks)
  let filter = state({ search = "", show-done = false, by = all })
  let sorted = state(true)
  let save   = state(idle: Saving<unit>)

  -- Computed values (auto-tracked, memoised)
  let visible = computed {
    let f = tasks.filter(|t| matches(t, filter))
    if sorted then sort-by-priority(f) else f
  }
  let count = computed { visible.length }

  -- Handlers — `on event(args) [if cond] { … }`
  on filter-change(text) {
    filter <- filter with { search = text }
  }

  on toggle-show-done {
    filter <- filter with { show-done = not filter.show-done }
  }

  on toggle-sort   { sorted <- not sorted }

  on task-pick(t: Task) {
    tasks <- tasks.map(|x|
      if x.id == t.id then x with { done = not x.done } else x)
    on-pick(t)

    save <- saving
    storage.write("tasks", tasks)
      .then(|_| save <- saved(unit))
      .catch(|e| save <- failed(e.message))
  }

  on copy-titles {
    let titles = visible.map(|t| t.title).join("\n")
    clipboard?.write(titles)                    -- optional capability
  }

  on new-task(text: string) {
    if text == "" then unit
    else {
      let t: Task = {
        id        = uuid(),
        title     = text,
        notes     = nil,
        priority  = medium,
        due       = nil,
        done      = false,
      }
      tasks <- [...tasks, t]
      on-new(text)
    }
  }

  -- Scoped host style
  style {
    background = surface
    padding    = 16
    radius     = 8
    min-height = 200
  }

  -- The component's value: the render tree
  <container class=app-shell>
    {header}

    <container direction=row gap=8 align=center>
      <input
        placeholder="Filter tasks…"
        :value={filter.search}
        @change=$filter-change(_)/>

      <button @click=$toggle-show-done>
        {if filter.show-done then "Hide done" else "Show all"}
      </button>

      <button @click=$toggle-sort>
        Sort: <text style.weight=bold>{if sorted then "priority" else "order"}</text>
      </button>

      <button if={clipboard != nil} @click=$copy-titles>
        <icon name=copy/> Copy
      </button>
    </container>

    -- Caller-customisable toolbar slot — invoked as a function
    {toolbar(filter)}

    -- Save indicator: pattern match in tree position via expression
    {match save {
       idle         → nil
       saving       → <text class=muted>Saving…</text>
       saved(_)     → <text class=muted>Saved.</text>
       failed(msg)  → <text style.color={tokens.colors.red.base}>Error: {msg}</text>
    }}

    -- The list itself
    {if count == 0 then empty else
       <container direction=column>
         <fragment for={t in visible}>
           <TaskRow task={t} on-toggle=$task-pick(_) on-edit={on-edit}/>
         </fragment>
       </container>}

    <NewTaskInput on-submit=$new-task(_)/>

    <text class=muted>{count} shown · {tasks.length} total</text>
  </container>
}

component NewTaskInput(on-submit: |string| → unit) = {
  use AutoFocus

  let text = state("")

  on submit {
    on-submit(text)
    text <- ""
  }

  <container direction=row gap=4>
    <input
      :value={text}
      placeholder="Add a task… (Enter to submit)"
      @keydown={|e| if e.key == "Enter" then submit()}/>
    <button @click=$submit>Add</button>
  </container>
}

-- ───── DERIVED VARIANTS — composition via `with` ───────────────────────────

-- Same TaskList, different defaults — no new body, no new file boilerplate
component CompactTaskList = TaskList with {
  empty   = <text>—</text>
  toolbar = |_| nil
}

-- Just call the parent — function composition
component DemoTaskList(seed: string) =
  TaskList(initial-tasks = db.demo-tasks(seed))

-- ───── MACROS — pattern match + expansion ──────────────────────────────────

macro Field(label: string, value: string) {
  match  { <Field label={label} value={value}/> }
  expand {
    <container direction=row gap=8>
      <text style.width=120 class=muted>{label}</text>
      <text>{value}</text>
    </container>
  }
}
```

That is the whole language under realistic load. Every
subsection below drills into one gesture and explains why
it works the way it does. The features mapped to §6 don't
change; only the shape they wear.

### 12.2 The one gesture — `name = expression`

Every top-level declaration in PRUI follows one of two
spellings:

| Shape | When |
|---|---|
| `<keyword> Name [params] = expression` | The declaration's value is a single expression — terse, common for trivial cases. |
| `<keyword> Name [params] { body }` | The declaration's value is a *block expression* — statements followed by a final expression. Used when the body needs setup before the value. |

These are the same gesture: the brace-block form is just
`= { body }` with the `=` dropped (the brace is unambiguous).
Both forms produce *an expression value bound to a name*.

| Keyword | Value type | Trivial form | Block form |
|---|---|---|---|
| `component` | a function from props to ui tree | `component Avatar(src: string) = <image src={src}/>` | `component Card(t: string) { state x = …; <container>…</container> }` |
| `type` | a type (alias / union / record) | `type Tone = info \| primary \| danger` | (rarely needs a block; lift sub-types instead) |
| `trait` | a structural shape | `trait Marker = {}` (or `trait Marker {}`) | `trait Pointable { on-click: action, … }` |
| `mixin` | a behaviour bundle | (rare — mixins almost always have body) | `mixin Hoverable { let hovered = state(false); on pointerenter { … } }` |
| `class` | an inline PRSS class | (rare — classes almost always have body) | `class card { background = surface, padding = 16 }` |
| `macro` | a pattern + expansion | (rare) | `macro Field(lbl, val) { match { … } expand { … } }` |
| `fn` | a function | `fn double(x: int) → int = x * 2` | `fn process(items: array<T>) → array<T> { … ; final-expr }` |
| `let` | a value | `let MAX = 100` | `let cfg = { let p = load(); p with { … } }` |

Every other syntactic surface — clauses, sub-tags, header
attributes, declaration ceremonies — collapses into this.
What was `<component Foo extends=Bar impls=[X] derives=[M]
capabilities=[c: C]>…</component>` becomes statements
inside the body:

```prui
component Foo(args) = {
  use Bar           -- inheritance via partial application (see §12.5)
  use X             -- trait conformance
  use M             -- mixin (parse-time)
  requires c: C     -- capability

  …
}
```

The composition still happens; it just happens *as part of
the function body*, not as decorations around it.

### 12.3 The component body — statements, then the value

Inside a `{ body }` block, statements run top-to-bottom; the
final expression is the block's value. For components, that
final expression is the rendered tree.

The body admits these statement kinds (any order, all
optional):

| Statement | Shape | Role |
|---|---|---|
| `let name [: type] = value` | binding | introduce a value (reactive if RHS is a `state(…)` / `computed { … }` call) |
| `use Name [, Name …]` | mixin / trait / inheritance | bring a mixin's state+handlers into scope, OR bring a trait into the conformance set, OR inherit from a parent component (§12.5) |
| `requires name: Type [, …]` | capability request | request a host service; available as `name` for the rest of the body |
| `on event[(args)] [if cond] { body }` | event handler | register a handler on `self` |
| `style { … }` | scoped PRSS | apply PRSS to the host element |
| `<tree/>` | render expression | the body's final value, the rendered tree |

The render expression is just the last expression in the
block; no separate `render` keyword. If the body is *only*
the render expression, you can drop the braces:

```prui
-- Trivial — no body block:
component Avatar(src: string, size: px = 32) = 
  <image src={src} width={size} height={size}/>

-- One state cell + tree — body block needed:
component Counter(initial: int = 0) {
  let count = state(initial)
  
  on click { count <- count + 1 }
  
  <button @click=$click>{count}</button>
}
```

The `=` in the trivial form is the same `=` from `type X =
Y` — assignment to a value expression. The body-block form
drops it because `Name(...) { ... }` is unambiguous on its
own; both are equivalent.

### 12.4 Reactive state — three primitives, one operator

PRUI doesn't have a `state` *keyword*. State is a value you
create by calling the `state(initial)` primitive, just like
any other function call:

```prui
let count    = state(0)                -- reactive cell
let title    = state("Untitled")
let user     = state<User?>(nil)       -- explicit type annotation
let visible  = computed { count > 0 }  -- auto-tracked derivation
```

Three primitives, one operator:

| Primitive | What it gives you |
|---|---|
| `state(initial)` | A reactive cell. Bare reads (`count`) auto-track; writes use `<-`. |
| `computed { expr }` | A memoised derivation. Reads inside re-track on dep changes. |
| `effect { stmts }` | A side effect that re-runs on dep changes. No value. |
| `cell <- new-value` | The only write operator. Equivalent to `cell.set(new-value)`. |

```prui
component Counter(initial: int = 0) {
  let count   = state(initial)
  let doubled = computed { count * 2 }
  
  effect { log("count became", count) }
  
  on click { count <- count + 1 }
  
  <text>{count} (×2 = {doubled})</text>
}
```

A bare reactive cell name in any expression position
(attribute brace-expr, tree interpolation, RHS of `=`, body
of `computed`) reads the cell's current value AND
establishes a tracking dependency. No `.get()` ceremony, no
`.value` accessor. Writes use `<-` and only `<-`.

There is no `state x = 0` keyword shortcut; the function
call `state(0)` is the canonical surface. Adding a keyword
would split the syntax for one micro-saving — every other
binding shape (`let cfg = computed { … }`, `let e = effect
{ … }`) already follows the `let name = call(…)` shape.

### 12.5 Composition — `use`, `with`, and function calls

The `extends`/`impls`/`derives` trinity from §6.3 collapses
into one keyword (`use`) inside the body and one operator
(`with`) on the value level.

| Composition need | Shape | Where |
|---|---|---|
| Inherit from a parent component (Wave J `extends`) | `Comp2 = Comp1 with { overrides }` *or* `Comp2(args) = Comp1(args, override=value)` | value level |
| Implement a trait (Wave J `impls`) | `use Trait` inside the body, *or* `: Trait` annotation after the param list | body / signature |
| Mix in behaviour at parse time (Wave J `derives`) | `use Mixin` inside the body | body |
| Mix in behaviour at runtime | `<container with=[Mixin]>` attr *or* function call `Mixin(<tree/>)` | use site |

**`use Name` inside a body** is the workhorse. It does
whatever `Name` is registered as:

- `use Hoverable` — Hoverable is a mixin → its state, hooks,
  and styles inline into this body's scope. The mixin's `let`
  bindings (e.g. `hovered`) become visible names in the host.
- `use Focusable` — Focusable is a trait → this component
  conforms structurally; the trait's required members must
  appear in the body. The check happens at parse, not
  runtime.
- `use Cards.Card` — Card is a component → inherit. The
  body is appended to (and may override) the parent's body
  via the parse-time flattening §6.3 specifies.

The parser knows which is which by what `Name` resolved to
in the registry; the *call site* doesn't need to spell it
out. Adding a new behaviour kind is a registry registration,
not a new keyword.

**`with { overrides }` on the value level** is the
record-update operator applied to a component:

```prui
component DangerButton = BaseButton with {
  tone = danger
}

component CompactList = TaskList with {
  empty   = <text>—</text>
  toolbar = |_| nil
}
```

It's the same `record with { field = value }` that updates a
normal record; components are first-class values, so the same
operator works. No new syntax.

**Inheritance as a function call** is the other option,
useful when the override changes behaviour rather than just
defaults:

```prui
component DangerButton(label: string) = {
  on confirm-then-click(e) {
    if confirm("Sure?") then BaseButton.dispatch-click(self, e)
  }
  BaseButton(label = label, tone = danger, on-click = $confirm-then-click(_))
}
```

The whole component body is plain composition: register a
handler, then return a `BaseButton(…)` call as the value.

### 12.6 Slots — typed function parameters, period

The `<slot>` machinery from §6.5 dissolves. Slots are just
**typed function parameters** carrying either a `ui` value
(a tree) or a `|args| → ui` function (a render-prop).

```prui
component List(
  items:  array<Task> required,
  header: ui                       = <heading>Tasks</heading>,
  empty:  ui                       = <text>No tasks</text>,
  row:    |Task, int| → ui         = |t, i| <text>{i}: {t.title}</text>,
) {
  <container direction=column>
    {header}                                           -- splice a tree
    
    {if items.length == 0 then empty
     else <fragment for={t, i in items}>{row(t, i)}</fragment>}
  </container>
}
```

Caller side — slot props are passed like any other prop:

```prui
<List
  items={tasks}
  header={<heading 2>Today's tasks</heading>}        -- tree as a value
  row={|t, i| <text>{i + 1}. {t.title}</text>}        -- function as a value
  empty={<text class=muted>Nothing pending.</text>}/>
```

The `children` prop is magic-named: any unnamed children
passed inside `<Comp>…</Comp>` flow into a `children: ui`
parameter automatically (mirrors React / JSX). For named
slots, just name the parameter. No `<slot/>` tag, no
`<slot row args={…}>` provider syntax.

`<slot/>` survives only inside macro definitions where it
serves as a marker for "where the captured tree goes" (§12.10).

### 12.7 Pattern matching at every level

`match` is an expression. It works at every position a value
goes, including inside a tree. Two surface forms; the author
picks for clarity:

**Expression form** — value-producing:

```prui
let symbol = match priority {
  critical → "‼",  high → "!",  medium → "·",  low → " "
}

fn render-status(s: Saving<unit>) → ui = match s {
  idle        → nil
  saving      → <spinner/>
  saved(_)    → <text>Saved.</text>
  failed(msg) → <text>Error: {msg}</text>
}
```

`match X { pattern → value, … }` — no `case` keyword. The
arrow `→` separates pattern from value. Patterns can be:
- Bare names (variant cases): `info`, `success`
- Destructure patterns: `custom(c)`, `error(_, retry)`
- Wildcards: `_`
- Record patterns: `{title, done = true}` (matches by shape)
- Literals: `0`, `"text"`, `true`

**Tagged form** — for trees of trees:

```prui
<container>
  <match on={view-mode}>
    <case list>     <TaskList .../>      </case>
    <case board>    <KanbanBoard .../>   </case>
    <case calendar> <CalendarView .../>  </case>
  </match>
</container>
```

The tagged form is sugar for the expression form spliced into
the tree — same AST, different shape. Use the tagged form
when the arms produce non-trivial trees and the visual
nesting matches the document structure; use the expression
form when the arms produce values or short fragments.

### 12.8 Control flow — attribute or expression

The element-level `if=` / `for=` attributes from Wave H stay
as sugar for the expression form:

| Attribute form | Expression form |
|---|---|
| `<container if={cond}>…</container>` | `{if cond then <container>…</container>}` |
| `<fragment for={t, i in xs}>…</fragment>` | `{xs.map(\|t, i\| …)}` |
| `<container if={a} for={b in c}>…</container>` | `{b.map(\|b\| if a then … else nil)}` |

Same AST, two surfaces. Inside expression mode (brace exprs,
function bodies), the canonical shape is the expression form:

```prui
if cond then expr1 else expr2          -- expression
if cond then { stmts; last } else { … } -- block branches
match X { pattern → value, … }          -- expression-form match
for (item in iter) { stmts }            -- iteration block (no value)
while cond { stmts }                    -- conditional loop
```

`if` / `match` are expressions; they produce a value. `for` /
`while` are statements; they don't.

### 12.9 The tag-expression blend — both ways, freely

Tags and code interleave at every boundary. The rules:

**Code → tree:** any `{expression}` inside a tree position
splices the expression's value into the tree.

```prui
<container>
  <text>Hello, {user.name}</text>             -- string interpolation
  {if loading then <spinner/>}                -- conditional fragment
  {visible.map(|t| <Row task={t}/>)}          -- list mapping
  {match status { ok → <ok/>, _ → <warn/> }}  -- match returning a tree
</container>
```

**Tree → code:** trees are first-class values. Bind them with
`let`, return them from functions, pass them as props:

```prui
let badge   = <warning-icon class=urgent/>
let display = if level == high then <fire/> else <leaf/>

fn priority-tag(p: Priority) → ui = match p {
  critical → <tag class=urgent>‼</tag>
  high     → <tag class=warn>!</tag>
  _        → <tag/>
}

<container>{priority-tag(task.priority)}</container>
```

**Components are functions** — three call forms produce the
same value:

```prui
let card1 = <Card title="Hi"/>                  -- tag form
let card2 = Card(title = "Hi")                  -- function-call form
let card3 = Card with { title = "Hi" }          -- partial-app form (no body)
```

The tag form is sugar for the function call; the parser
desugars `<Name a=b c=d>k</Name>` into `Name(a=b, c=d,
children=k)`. Author picks: tag form is natural in a tree
context; function-call form is natural in an expression
context.

**Slot providers as expressions** — pass any value into a
slot parameter:

```prui
<List
  items={tasks}
  header={<heading 2>Today</heading>}            -- tree literal
  row={|t, i| <text>{i}: {t.title}</text>}        -- lambda
  empty={"No tasks yet"}                          -- string (auto-wrapped)
  toolbar={priority-toolbar}                      -- function-bound name
/>
```

**Mixins as functions OR as use-site attributes:**

```prui
<container with=[Hoverable, Pulsing]>…</container>  -- attribute form
{Hoverable(Pulsing(<container>…</container>))}      -- function-composition form
```

Both work. The attribute form reads better when applying
mixins to a container at the call site; the function-call
form reads better when composing several mixins in a
helper-function.

This bidirectional flow is the heart of the design.
Wherever a tree is more visual, write a tag. Wherever code is
more compact, write an expression. The two never collide
because the brace `{…}` is always the bridge from one mode
to the other.

### 12.10 Macros — pattern + expansion, the only sub-tag-style block

Macros are the one place body-tag-style sections survive,
because the macro's job is to match markup and rewrite to
markup:

```prui
macro Field(label: string, value: string) {
  match  { <Field label={label} value={value}/> }
  expand {
    <container direction=row gap=8>
      <text style.width=120 class=muted>{label}</text>
      <text>{value}</text>
    </container>
  }
}
```

`match { ... }` is the input pattern; `expand { ... }` is
the output. Both are tree-shaped because that's what they
match and emit. Captured names (`label`, `value`) carry the
typed-param shape and flow into the expansion.

Attribute macros take an `attribute` modifier and emit
attribute key/value pairs via `expand-attrs`:

```prui
macro Elevation attribute(level: int) {
  expand-attrs {
    style.radius     = 8
    style.background = tokens.surface
    style.shadow     = elevations[level]
  }
}

<container elevation=2>…</container>
```

The `<slot/>` tag inside a macro `expand` block marks where
the captured caller-content goes (the only surviving use
of the literal `<slot/>` token, see §12.6):

```prui
macro Disclosure(title: string) {
  match  { <Disclosure title={title}><slot/></Disclosure> }
  expand {
    <details>
      <summary>{title}</summary>
      <slot/>            -- splice the original children
    </details>
  }
}

<Disclosure title="More">
  <text>Here's the rest.</text>
</Disclosure>
```

### 12.11 Algebraic types — `type X = a | b(…) | c`

```prui
type Priority = low | medium | high | critical
type Result<T, E> = ok(value: T) | err(error: E)
type View = list | board | calendar | timeline(zoom: float)
type Saving<T> = idle | saving | saved(T) | failed(message: string)
```

Pipe-separated variants on one line. Each variant is either:
- A bare PascalCase name (`info`)
- A name with a parenthesised typed field list (`custom(c:
  color)`, `error(message: string, retry: action?)`)

Record types use the brace shape:

```prui
type Task = {
  id:       string,
  title:    string,
  priority: Priority,
  due:      timestamp?,
  done:     bool,
}
```

`?` after a type marks it nullable. Inline unions are
allowed in component signatures but extract to a named
`type` when reused:

```prui
-- inline (one-off):
component Toast(tone: info | success | danger required) { … }

-- named (reused):
type Tone = info | success | danger
component Toast(tone: Tone required) { … }
component Banner(tone: Tone = info) { … }
```

Constructing a variant uses positional or named args:

```prui
let r1 = error(message = "bad", retry = $retry)    -- named
let r2 = custom(c = #3b82f6)                        -- named
let r3 = ok(42)                                     -- positional
```

Pattern-matching destructures the same way:

```prui
match r {
  ok(v)            → render(v)
  err({message})   → <text>Error: {message}</text>  -- record-pattern destructure
  loading          → <spinner/>
}
```

### 12.12 Traits — structural shapes, opt-in conformance

```prui
trait Focusable {
  focus:   action
  blur:    action
  focused: bool = false
}

trait Editable<T> {
  value:     T
  on-change: |T| → unit
}

trait Marker {}                                     -- pure contract, no members

trait Composite recursive {
  children: || → Composite = || nil
}
```

A trait is a structural shape — a record type with optional
members, used as a *contract* for slot types and as a *bound*
for conformance.

**Two ways to conform:**

1. **Implicit / structural** — the component's signature
   plus body bindings satisfy the trait's shape. Inferred
   at parse-time; no annotation needed.

2. **Explicit** — annotate with `: TraitName, …` after the
   param list, or `use TraitName` inside the body:
   
   ```prui
   component TaskRow(task: Task) : Focusable = { … }
   
   -- or:
   component TaskRow(task: Task) = {
     use Focusable                  -- explicit conformance
     …
   }
   ```
   
   Explicit conformance lets the parser tell you when the
   body doesn't match the trait yet ("missing `focus`
   method"). Implicit conformance is for the case where the
   shape is obvious from the body.

Traits used as slot type bounds:

```prui
component FormField(
  control: || → Focusable,           -- slot must return a Focusable
) {
  <container>
    {control()}
  </container>
}
```

The slot's return type *is* the contract. No special
"contract" declaration needed.

### 12.13 Capabilities — `requires` as a body statement

```prui
component ShareButton(text: string required) {
  requires clipboard: Clipboard
  requires network:   Network?
  
  on click {
    clipboard.write(text)
    network?.post("/share", { text })       -- optional capability call
  }
  
  <button @click=$click>Copy & post</button>
}
```

`requires` is a body statement that requests a capability
from the host. The bound name is in scope for the rest of
the body. A `?` after the type marks the capability as
optional — the body must guard with `cap?.method(…)` or
`if cap != nil`.

Missing required capability = parse-time failure at the
host's `lower-time` call. No magic globals; the IDE
completes `clipboard.<TAB>` after the `requires` line.

### 12.14 Mixins — `use Foo` inside, or `Foo(<tree/>)` outside

A mixin is a body of `let` / `on` / `style` statements that
get spliced into whatever component `use`s it:

```prui
mixin Hoverable {
  let hovered = state(false)
  on pointerenter { hovered <- true }
  on pointerleave { hovered <- false }
}

mixin Draggable {
  let dragging = state(false)
  let offset   = state((0, 0))
  
  on pointerdown(e) {
    dragging <- true
    offset   <- (e.x - self.x, e.y - self.y)
  }
  on pointermove(e) if dragging {
    self.x <- e.x - offset.x
    self.y <- e.y - offset.y
  }
  on pointerup { dragging <- false }
}

component Card(title: string) {
  use Hoverable, Draggable        -- multiple mixins in one statement
  
  <container 
    class={if hovered then "card hovered" else "card"}
    style.cursor={if dragging then "grabbing" else "grab"}>
    <heading>{title}</heading>
  </container>
}
```

The mixin's `let` bindings (`hovered`, `dragging`) become
visible names in the host. The handlers (`on`) chain via
`super()`. Conflicts at parse time → explicit
disambiguation required.

For runtime composition, mixins are also first-class
functions from trees to trees, exposed via the use-site
`with=[…]` attribute or direct function call:

```prui
<container with=[Hoverable, Pulsing]>…</container>
{Pulsing(Hoverable(<container>…</container>))}      -- equivalent
```

### 12.15 Inline PRSS — `class Name { body }`

A `class Name { … }` at file top level defines a PRSS class
scoped to the file (or the file's namespace):

```prui
namespace Cards

class card-base {
  background = surface
  padding    = 16
  radius     = 12
  &:hovered { background = lighten(0.05) }
  &:pressed { background = darken(0.05) }
}

component Card(title: string) = {
  <container class=card-base>
    <heading 3>{title}</heading>
  </container>
}
```

Inside a component or mixin body, `style { … }` applies a
scoped style to the host element:

```prui
style {
  background = accent
    | :hovered  → lighten(0.1)
    | :pressed  → darken(0.1)
    | :disabled → mute
  radius  = 8
  padding = (8, 16)
}
```

Both use the same PRSS-Nesting body — `&:state` selectors,
brace-expr values, the pipeline `|` operator for state-
responsive values (Q8 — pipeline `|` only parses in
value positions typed `Stateful<T>` / `Animated<T>`).

### 12.16 Functions, lambdas, blocks

The closure rules from the previous draft stand:

```prui
let double = |x| x * 2                          -- single expression
let format = |amt, cur| {                       -- block body
  let prefix = if cur == "USD" then "$" else "€"
  prefix .. tostring(amt)
}
```

The block-vs-record disambiguation rule: in expression
position, `{…}` is a record literal IF its first non-whitespace
content is `key =` / `key:`. Otherwise it's a block. After
`|args|`, after a declaration head, after `style`, and inside
a `class` body, it's always a block (or PRSS body).

Named `fn` declarations use the same gesture:

```prui
fn double(x: int) → int = x * 2                 -- assignment form
fn format(amt: int, cur: string) → string = {   -- assignment + block expression
  let prefix = if cur == "USD" then "$" else "€"
  prefix .. tostring(amt)
}

fn process(items: array<Task>) → array<Task> {  -- definition form (no `=`)
  let active = items.filter(|t| not t.done)
  sort-by-priority(active)
}
```

`fn name(args) → ret = expr` and `fn name(args) → ret {
body }` are identical in meaning; pick by shape. Function
types use `|args| → ret` in type position (same shape as
closure values, just no body).

### 12.17 Imports — bare directive, projection inferred

```prui
import "./theme.prss"                    -- → stylesheet (extension)
import "./helpers.luau"     as h         -- → script (extension default)
import "./card.prui"                     -- → component(s)
import "./forms.prui"       as forms     -- → multi-component, namespaced
import "./icons.luau"       as icons     -- → script, exposes `icons.*`
import "./markdown.luau"    dialect      -- → explicit projection
```

Shape: `import "<path>" [<projection>] [as <alias>]`. The
projection is inferred from extension; the trailing keyword
overrides for ambiguous cases. `as` overrides the imported
file's own `namespace` declaration.

### 12.18 What stays XML — the render tree wins as tags

Tagged blocks remain canonical for **tree structure**:

- Render trees in component bodies (`<container>`, `<text>`,
  `<heading>`, `<image>`, `<input>`, etc.)
- Component invocation as a tree value (`<Card title="Hi"/>`,
  `<Forms.TextField/>`) — sugar for the function call
- Tagged `<match>` / `<case>` (sugar for expression match)
- Tagged `<fragment for=…>` (sugar for `.map(|x| …)`)
- Macro `match` and `expand` body sections (§12.10)

The render tree, slot defaults that are trees, and the macro
shape are where XML earns its keep. Everywhere else, code
shapes win.

### 12.19 Translation cheatsheet — XML → canonical

| XML form (the §6 stepping stone) | Canonical form (§12) |
|---|---|
| `<component Name attrs>body</component>` | `component Name(params) { body-statements; <tree/> }` |
| `<trait Name attrs/>` | `trait Name { members }` (or `trait Marker {}`) |
| `<mixin Name>body</mixin>` | `mixin Name { body }` |
| `<macro Name>match/expand</macro>` | `macro Name(captures) { match {…} expand {…} }` |
| `<state name=v>` (body sub-tag) | `let name = state(v)` (body statement) |
| `<on event>{handler}</on>` | `on event(e) { handler }` |
| `<style>…</style>` | `style { … }` |
| `<import stylesheet="x"/>` | `import "x"` |
| `<import script="x"/> as h` | `import "x" as h` |
| `<namespace=Foo/>` | `namespace Foo` |
| `extends=Parent` (header attr) | `use Parent` inside body OR `Comp2 = Parent with { … }` |
| `impls=[A, B]` (header attr) | `: A, B` after params OR `use A, B` inside body |
| `derives=[M]` (header attr) | `use M` inside body |
| `capabilities=[c: C]` (header attr) | `requires c: C` inside body |
| `prop: type = default` (header attr) | `prop: type = default` (param list) |
| `tone: union<info, success{d: int}, …>` | `tone: info \| success(d: int) \| …` or named `type Tone = …` |
| `\|args\| expr` | `\|args\| expr` (unchanged) |
| (no multi-line lambda surface today) | `\|args\| { stmts; last }` (block body) |
| `<match><case P>arm</case></match>` | `<match><case P>arm</case></match>` (kept) OR `match X { P → V }` (expression) |
| `<slot name="x">` declaration | typed param `x: ui` or `x: \|args\| → ui` |
| `<slot name args/>` invocation in body | bare `{x}` or `{x(args)}` |
| `<slot name>...</slot>` provider on caller | named prop `<Comp x={<tree/>}/>` or `<Comp x={\|args\| …}/>` |

A `prism rewrite-canonical path/…` migration command
(Phase P2) automates ~90% of the translation.

### 12.20 Where the language might go wrong — open questions

A short, honest list of places the surface might bite back.
Each has a tentative answer; P1 confirms or revises.

1. **Reactivity granularity** — what makes a `let` binding
   reactive? Tentative rule: a `let` is reactive iff its RHS
   calls `state(…)` / `computed { … }` / `effect { … }` or
   reads a reactive cell. `let snap = cell` *captures the
   current value* (snapshot, not tracking) — to track, use
   `let live = computed { cell }`. Open: do we want
   implicit tracking on bare expressions too, like Solid?
2. **Block-vs-record disambiguation under `=`** — `let x =
   { … }` could be a block or a record literal. Rule: first
   non-whitespace content. `{ key = v }` is a record;
   `{ let z = 1; z }` is a block; `{ <tree/> }` is a block
   whose value is a tree. Risk: `{ name }` (one bare
   identifier) is ambiguous — does the author want a record
   with a punned field, or a block returning the identifier?
   Tentative: parse as a block; punned-field shorthand
   requires `{ name = name }` explicitly.
3. **`use Name` overload between mixin / trait / inheritance**
   — `use Cards.Card` could inherit from a component, conform
   to a trait, or splice in a mixin. The registry tells the
   parser which, and the parser dispatches. Open: is this
   ambiguity readable to authors? Mitigation: an LSP hover
   on `use Name` shows "(mixin)", "(trait)", or
   "(component-extends)" inline.
4. **`with { … }` on a component vs. on a record** — both
   spell the same way. Components are records of props +
   defaults; the operator is the same. Open: what about
   `with` on a *running instance* (mutation)? Tentative:
   forbidden; instances are immutable; mutate state via the
   `<-` operator on cells.
5. **Trait conformance with overlapping mixins** — if two
   mixins both expose `hovered`, who wins? Tentative: parse
   error, author disambiguates by renaming one
   (`use Hoverable as h1, Hoverable as h2`).
6. **`children` magic-name vs. explicit slot** — `children:
   ui` is special-cased to receive unnamed JSX-style child
   trees. Risk: authors who want a different name for the
   default slot. Tentative: only `children` is magic-named;
   to call it something else, take a normal prop and pass it
   explicitly at the call site.
7. **`?` as nullable vs. `?` as optional method-call** —
   `Network?` is a nullable type; `network?.post(…)` is an
   optional call. Both use `?`. Disambiguation by position
   (type vs. expression). Mirrors Swift / TypeScript / Kotlin.
8. **Component as a tree literal vs. a function call** —
   `Card(title="A")` and `<Card title="A"/>` both produce
   the same `ui` value. Risk: which to write? Convention:
   tag form inside a tree; function form everywhere else.

### 12.21 Phasing — canonical alongside XML

The canonical surface lands as a **second parser** alongside
the existing XML-shape declaration parser. Body-mode (the
render tree) is the same in both — only the declaration
shape switches.

| Phase | Scope | Reversibility |
|---|---|---|
| **P1** — canonical grammar lands | Both parsers run; first non-whitespace token of a declaration picks (`<` → XML, lowercase keyword → canonical). Both produce the same `ComponentSchema`. | reversible — XML parser stays |
| **P2** — migration tool | `prism rewrite-canonical path/…` mechanically rewrites XML to canonical; ambiguous cases get a `TODO` comment. | reversible — git revert |
| **P3** — XML-shape declarations deprecated | Parser warns on each XML-shape declaration; canonical is canonical in docs and examples. One release later, XML-shape declaration parsing removed (tagged-tree bodies stay). | reversible during the deprecation window |
| **P4** — doc rewrite | All §5 and §6 examples in this doc rewritten in canonical shape. Receipts (§10) recomputed against the new LOC. | one-way (a doc commit) |

LOC estimate for the canonical-surface work itself:
- Canonical parser: ~1200 LOC (recursive-descent; no new
  AST types — same `ComponentSchema`).
- Migration tool: ~700 LOC.
- XML-shape declaration parser deletion (P4): −400 LOC.
- Net: ~+1500 LOC added, ~−400 LOC removed.

P1 can land any time after §7 Phase 6. Suggested slot: P1
ships between §7 Phase 6 and §7 Phase 8 so early adopters
pick canonical from day one.

---

## 13. Closing thought

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
implementations are gone. The two slot spellings are one. The
two closure forms are one (one parameter syntax, two body
shapes). The four-line state-variant repetition is one.
**Every grammatical concept the language exposes is justified
by user-facing semantics — there are no mechanism-residues
left over from earlier eras.** That's the destination Wave J
+ K + L points at. §7 phases the trip in 18 slices (Phase 0
cleanup + Phases 1–17 features); the first three feature
phases ship in weeks, the last few in months, and every one
is independently shippable. §11 is the list of things we
*could* do after, kept honest so they don't accrete into the
load-bearing design.

**§12 is the syntax that carries it.** Wave J + K + L
designed the feature set; the canonical surface collapses
the *shape* down to one gesture — **every declaration is
`name = expression`** (or `name(params) { body
expression }`). The component is a typed function whose
value is a tree; state is a value you create with
`state(…)` and write with `<-`; mixins, traits, and
inheritance all happen *inside the body* as `use` and
`requires` statements, not as decorations *around* it. The
favourite line — `component Avatar(src: string, size: px =
32) { <image src={src} width={size} height={size}/> }` — is
the common case; richer cases just add statements before
the final tree. Tags and code blend bidirectionally: trees
are expression values, expressions are tree children, and
`{…}` is the one bridge between them. **Trees stay tagged
where the visual hierarchy is the structure; everything
else is one expression.**
