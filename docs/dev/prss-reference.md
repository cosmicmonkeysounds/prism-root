# PRSS Stylesheet Language Reference

> **PRSS** is the stylesheet language for Prism. Where **PRUI**
> (`.prui`) is the HTML — declarative tree of UI elements — PRSS
> (`.prss`) is the CSS — themes, design tokens, and named classes
> that PRUI elements opt into via `class="…"`. Where **Luau** is
> the JS — handlers, modifiers, scripted behaviour. Three
> languages, one stack.

**Status:** initial design (2026-05-13). Parser in
`packages/prism-core/src/language/prss/`; runtime integration in
`packages/prism-ui-runtime/src/interpret.rs` (the `LowerScope`
threading + `class` attribute application).

**Related docs:** `prui-reference.md` (the HTML half),
`composable-builder-plan.md` Wave 14.1 (the existing design-token
binding PRSS extends), `dioxus-inspiration.md` Phase 10 (the
fingerprint-cache substrate PRSS hot-reload rides on top of).

---

## 1. Why TOML, why now

CSS is great at *one* thing it does poorly in a Rust runtime:
it's a parallel grammar with parallel tooling, parallel parsers,
parallel diagnostics. PRSS replaces all of that with TOML — a
spec we already vendor for `Cargo.toml`, with `serde::Deserialize`
support, with editor highlighting in every IDE on Earth, and with
a grammar small enough to skim in five minutes.

The trade-off: PRSS is *not* CSS. No cascading specificity rules,
no media queries (yet), no `@keyframes`. Instead PRSS is a small
data-driven type system shaped exactly to what `apply_style_override`
in `prism-ui-runtime` actually consumes — `background`, `radius`,
`padding`, `gap`, `width`, `height`, `color`, and the
`:hovered` / `:selected` / `:focused` pseudo-states. The vocabulary
is bounded, typed, and validated at parse time.

What PRSS gives you:

1. **Theme tokens** — central place to override every color /
   spacing / radius / type value the runtime uses.
2. **User-defined classes** — composable bundles of styling that
   any PRUI element opts into via `class="name1 name2"`.
3. **`extends` composition** — Sass-flavoured class inheritance
   without learning a parallel syntax.
4. **State variants** — `:hovered` etc. branch off the base
   class in the same file.
5. **Hot reload** — fingerprint-cache pattern (same substrate as
   `.prui` literal-only updates); edits land without respawn.

---

## 2. File format

`.prss` files are **pure TOML 1.0**. No extensions, no
preprocessing, no `@import`. If `cargo`'s parser accepts your
`Cargo.toml`, ours accepts your `.prss`.

A `.prss` file is one of three top-level shapes (in order, all
optional):

```toml
prss-version = 1                  # ← optional schema version pin
[tokens.colors]                   # ← token-table sections
[class.<name>]                    # ← class-definition sections
```

That's the entire surface. Everything else is a TOML key or
sub-table.

---

## 3. Theme tokens

The `[tokens.*]` tables override the runtime's default
[`DesignTokens`](../../packages/prism-core/src/design_tokens.rs).
A loaded `.prss` *merges* over the defaults — keys you don't set
inherit the runtime's value. Available paths exactly mirror the
`tokens` binding PRUI already exposes:

| Path | Type | Notes |
|---|---|---|
| `tokens.colors.<name>` | hex `#rrggbb` or `#rrggbbaa` string | `background`, `surface`, `surface-elevated`, `border`, `text-primary`, `text-secondary`, `accent`, `accent-muted`, `danger`, `success` — plus any custom name |
| `tokens.spacing.<size>` | integer (px) | `xs`, `sm`, `md`, `lg`, `xl` — plus any custom name |
| `tokens.radius.<size>` | integer (px) | `sm`, `md`, `lg`, `pill` — plus any custom name |
| `tokens.typography.<key>` | integer | `font-size-sm`, `font-size-md`, `font-size-lg`, `font-size-xl`, `line-height-md` — plus any custom name |

Custom names are pure passthrough — `tokens.colors.brand-purple = "#7c3aed"`
becomes addressable as `{tokens.colors.brand-purple}` from PRUI.

Example:

```toml
[tokens.colors]
accent = "#7c3aed"
accent-muted = "#a78bfa"
brand-purple = "#5b21b6"  # custom

[tokens.spacing]
md = 16

[tokens.radius]
md = 8
```

Every PRUI file in the host gets the merged token table seeded
into scope as the `tokens` binding (Wave 14.1). Authoring
`style:background="{tokens.colors.accent}"` resolves through the
override.

---

## 4. Classes

Each `[class.<name>]` section defines one class. PRUI elements
opt in via the bare `class="..."` attribute (multiple classes
allowed, whitespace-separated).

```toml
[class.btn]
background = "#0060c0"
color = "#ffffff"
radius = 8
padding = 12
```

Then in PRUI:

```prui
<container class="btn">Save</container>
```

### 4.1 Property vocabulary

Each class table accepts the exact key set
`apply_style_override` already recognises (see PRUI reference §4.3):

| Key | Value type | Lowers to |
|---|---|---|
| `background` | hex color | `ContainerProps.background` |
| `radius` | number (px) | `ContainerProps.radius` (uniform) |
| `padding` | number (px) | `ContainerProps.padding` (uniform — see §4.4 for shorthand) |
| `padding-left` / `-right` / `-top` / `-bottom` | number (px) | per-side padding |
| `gap` | number (px) | `ContainerProps.gap` |
| `width`, `height` | number, `"grow"`, `"fit"` | `ContainerProps.width/height` |
| `color` | hex color | `TextProps.color` / `Image.tint` on child leaves |
| `font-size` | number | `TextProps.font_size` |
| `direction` | `"row"` / `"column"` | `ContainerProps.direction` |

Length-valued properties accept the same numeric units PRUI does
(see [PRUI reference §3.6](prui-reference.md#36-numeric-units)):

```toml
[class.card]
padding = "1rem"         # 16px
radius = "0.5rem"        # 8px
gap = "12px"             # 12px (explicit unit)
width = "50%"            # only meaningful for width/height
```

Unknown keys drop silently (forward compat for future grammar
extensions). Wrong-type values (e.g. `radius = "huge"`) drop
silently — the parser surfaces a diagnostic, but the runtime
never panics.

### 4.2 Composition with `extends`

Sass-style class inheritance:

```toml
[class.btn]
background = "#ffffff"
color = "#000000"
radius = 8
padding = 12

[class.btn-primary]
extends = "btn"
background = "#0060c0"
color = "#ffffff"
```

`btn-primary` inherits `radius = 8` and `padding = 12` from `btn`,
then overrides `background` and `color`. The `extends` chain may
go arbitrarily deep; cycles are detected at load time and produce
a diagnostic (the cyclic class drops).

### 4.3 State variants

Three states are recognised, matching the runtime's
`STATE_SUFFIXES`:

| State | Triggered by |
|---|---|
| `hovered` | `Surface::hovered_id` matches the container's id |
| `selected` | data-attr round-trip today (Wave 9.2); runtime swap is a follow-up |
| `focused` | same as `selected` |

State tables use TOML's nested-key syntax:

```toml
[class.btn]
background = "#ffffff"

[class.btn.hovered]
background = "#f0f0f0"

[class.btn.selected]
background = "#0060c0"
color = "#ffffff"
```

The base class properties apply unconditionally; state properties
layer on top when the state matches. For `hovered`, this lands on
`ContainerProps.hover` (the same path `style:background:hovered`
takes from inline PRUI). For `selected` / `focused`, the
properties round-trip as `data-style-<key>-<state>` semantic
attrs — author intent survives even though the runtime swap
hasn't landed.

### 4.4 CSS-shorthand padding

`padding` accepts shorthand:

```toml
[class.card]
padding = "12"           # uniform (number form is also fine: padding = 12)
padding = "12 16"        # vertical, horizontal
padding = "12 16 24"     # top, horizontal, bottom
padding = "12 16 24 32"  # top, right, bottom, left  (CSS TRBL order)
```

`padding-left` / `-right` / `-top` / `-bottom` override
individual sides after the shorthand applies — same precedence
order CSS uses.

### 4.5 Multiple classes

`class="btn-primary large"` applies `btn-primary` first, then
`large`. Later classes override earlier ones on conflicting
keys. Each class's full `extends` chain resolves before its own
properties apply.

### 4.6 Application order (specificity)

Lowest priority first; later steps override:

1. **Block defaults** — the `lower_ui` body's own styling.
2. **Stylesheet classes** — left to right of `class="…"`, base
   properties then state variants.
3. **Descendant selectors** (§4.7) — multi-segment matches layer
   on top of flat-class application; declaration-order resolves
   ties on conflicting keys.
4. **PRUI inline `style:` attributes** — `style:background="#…"`.
5. **PRUI inline `style="{obj}"` spread** — Wave 12.

Inline always wins. The mental model matches React's
`<Foo className="btn" style={{ background: 'red' }}/>`: the
class is the theme, the style is the override.

### 4.7 Descendant selectors

A class key may carry multiple whitespace-separated segments to
target an element only when its ancestor chain also matches.
TOML keys with whitespace must be quoted, so the spelling reads
either CSS-style (with leading dots) or bare:

```toml
[class.btn]
background = "{tokens.colors.surface}"

[class.icon]
color = "{tokens.colors.text-secondary}"

[class.".btn .icon"]
color = "{tokens.colors.accent}"
```

Used from PRUI:

```prui
<container class="btn">
  <container class="icon"/>      <!-- ancestor btn → matches .btn .icon -->
</container>

<container>
  <container class="icon"/>      <!-- no ancestor btn → only flat .icon applies -->
</container>
```

Matching follows CSS descendant rules:

* The **rightmost** segment must match a class on the current
  element.
* Each preceding segment must match an ancestor's class set, in
  order from innermost outward. Intermediate ancestors that don't
  match are skipped.
* Application order: flat classes first, then descendant
  selectors (declaration order). Later assignments win on key
  conflicts — same shape as `extends`.

### 4.8 Reactive class toggles (`class:foo`)

PRUI's `class:<name>="{cond}"` attribute toggles a PRSS class on
the container based on a boolean expression — Vue's
`<div :class="{ active: isActive }">` / Svelte's
`<div class:active={isActive}>` collapsed into one syntactic form.

```prui
<container class="btn" class:primary="{state.kind == 'primary'}">
  Save
</container>
```

When the expression is truthy, the named class participates in
the PRSS pre-pass exactly as if it were appended to the static
`class="…"` list. The boolean form `class:active` (no `=value`)
reads as `true`. Falsy values drop the class cleanly. Without a
stylesheet loaded, the toggle is a no-op — same shape `class="…"`
itself takes on a host without PRSS.

---

## 5. Example: a complete stylesheet

```toml
prss-version = 1

# ── Theme tokens ─────────────────────────────────────────────
[tokens.colors]
accent = "#7c3aed"
accent-muted = "#a78bfa"
surface = "#ffffff"
surface-elevated = "#fafafa"
text-primary = "#0a0a0a"
text-secondary = "#525252"

[tokens.spacing]
xs = 4
sm = 8
md = 16
lg = 24
xl = 32

[tokens.radius]
md = 8
lg = 12

# ── Classes ──────────────────────────────────────────────────
[class.row]
direction = "row"
gap = 8

[class.column]
direction = "column"
gap = 8

[class.card]
background = "{tokens.colors.surface}"
padding = "16"
radius = 12
gap = 12

[class.card.hovered]
background = "{tokens.colors.surface-elevated}"

[class.btn]
extends = "row"
background = "{tokens.colors.surface}"
color = "{tokens.colors.text-primary}"
radius = 8
padding = "8 16"

[class.btn.hovered]
background = "{tokens.colors.surface-elevated}"

[class.btn-primary]
extends = "btn"
background = "{tokens.colors.accent}"
color = "#ffffff"

[class.btn-primary.hovered]
background = "{tokens.colors.accent-muted}"
```

Used from PRUI:

```prui
<container class="card">
  <text font-size="{tokens.typography.font-size-lg}">{title}</text>
  <text style:color="{tokens.colors.text-secondary}">{description}</text>
  <container class="row" gap="8">
    <container class="btn">Cancel</container>
    <container class="btn-primary">Save</container>
  </container>
</container>
```

---

## 6. Token references inside class values

Class property values are typed strings. Two value shapes:

1. **Literal**: `background = "#7c3aed"`, `padding = 16`, `gap = "md"`.
2. **Interpolation**: `background = "{tokens.colors.accent}"` — a
   `{expr}` body parsed by the same expression engine PRUI uses,
   resolved against a scope where `tokens` is bound to the merged
   token table.

The literal form is simpler. The interpolation form lets a class
reference any token by full path — useful when the same color
needs to flow into both a class and a PRUI inline `style:`.

### Short-name token lookup

A bare identifier on a token-backed property resolves through the
matching token bucket without the explicit `{tokens.…}` body.
The bucket is derived from the property key:

| Key | Bucket | Lookup shape |
|---|---|---|
| `background` / `color` / `border` | `tokens.colors` | `tokens.colors.<value>` |
| `radius` | `tokens.radius` | `tokens.radius.<value>` |
| `gap` / `padding` / `padding-*` / `margin*` | `tokens.spacing` | `tokens.spacing.<value>` |
| `font-size` | `tokens.typography` | `tokens.typography.font-size-<value>` |
| `line-height` | `tokens.typography` | `tokens.typography.line-height-<value>` |

A short name must look like a token spelling (lowercase ASCII
identifier characters, no leading digit). Hex colors (`#…`),
numeric values (`8`, `1rem`, `50%`), and unrecognised names fall
through unchanged so the parser handles them. Hosts that don't
seed a token table see every short-name attempt drop silently —
same as today's "unknown drops cleanly" rule.

```toml
[class.btn]
radius = "md"            # → tokens.radius.md
padding = "md"           # → tokens.spacing.md
background = "accent"    # → tokens.colors.accent
font-size = "lg"         # → tokens.typography.font-size-lg
```

The same resolution applies to PRUI inline `style:` and bare
length-typed attrs, so `<container padding="md"/>` and
`style:background="accent"` read the table identically.

---

## 7. Runtime integration

The host loads `.prss` at boot and threads it into the lowering
scope:

```rust
use prism_core::language::prss;
use prism_ui_runtime::interpret::LowerScope;

let source = std::fs::read_to_string("ui/theme.prss")?;
let stylesheet = prss::parse(&source)?;
let scope = LowerScope::default()
    .with_design_tokens(&stylesheet.merged_tokens(&DEFAULT_TOKENS))
    .with_stylesheet(Arc::new(stylesheet));
```

From there every `<container class="…">` in any `.prui` file
under the scope resolves automatically. No per-component plumbing.

### Loading multiple files

A host that wants to compose multiple stylesheets (a base theme
plus a per-app override) calls `StyleSheet::merge(other)` —
later files win on conflicting keys, matching the canonical
later-overrides-earlier rule classes themselves follow.

### Headless / SSR

The same `LowerScope::with_stylesheet` works for SSR; the
`lower_semantic_html` backend emits `class="…"` as an HTML
attribute on the rendered element so the same stylesheet can
be inlined into a `<style>` block (or shipped as the canonical
CSS, with PRSS as the source of truth).

---

## 8. Hot reload

PRSS rides on the same fingerprint-cache substrate `.prui` uses
(Phase 10 of `dioxus-inspiration.md`). The `.prss` extension is
part of `prism_cli::dev_loop::DEFAULT_EXTENSIONS`, so any save
under a watched path triggers the reload pipeline. Each save:

1. Reparses the file.
2. Builds a [`PrssFingerprint`](../../packages/prism-ui-build/src/prss_hash.rs)
   from the new sheet (structural hash + full hash + literal slot
   table).
3. The host's [`PrssFingerprintCache`](../../packages/prism-ui-build/src/prss_watch.rs)
   compares against the previous fingerprint and emits a
   `PrssChange`:
   * `LiteralOnly { patches }` — same class shape, same key set;
     only literal property / token values differ. The host
     re-evaluates every reactive context that read those slots
     without touching the AST.
   * `Structural` — class added/removed, key set changed,
     `extends` chain changed. Falls back to a full re-walk
     (`subsecond::call` swap if running with `--hot=subsecond`,
     otherwise child respawn).
   * `NoChange` / `ParseError` / `ReadError` — no-op / surface
     diagnostic / retry on next batch.

Same `DevLoop` from `prism-cli`, no new infrastructure — the
`.prss` extension joins `.rs` and `.prism-ui` in the default
respawn filter.

---

## 9. Diagnostics

`prss::parse(source) -> Result<StyleSheet, ParseError>` returns
typed errors:

| Code | Triggered by |
|---|---|
| `toml-syntax` | The TOML parser rejected the bytes |
| `unknown-token-group` | `[tokens.<unknown>]` table |
| `cyclic-extends` | A class's `extends` chain forms a cycle |
| `missing-parent` | A class extends a name that doesn't exist |
| `invalid-property-value` | A property's value can't be coerced to its type |

Diagnostics are recovery-friendly — one error per class doesn't
abort the whole file. The build script (`prss::compile`)
aborts on any error; the dev-loop reload surfaces them to the
editor and keeps the last good stylesheet active.

---

## 10. What PRSS is not (yet)

| Feature | Status |
|---|---|
| Media queries (`@media`) | Defer — viewport branching handles this today |
| Container queries | Defer — same |
| `@keyframes` / animations | Wait on the `transition:` namespace's runtime animator |
| Pseudo-elements (`::before`, `::after`) | Skip — no consumer; the same shape lands as `<container/>` siblings |
| `!important` | Skip — the application-order table (§4.6) is the precedence rule |
| `@import` / `@use` | Defer — host loads + merges multiple files explicitly |
| CSS variables (`var(--foo)`) | Skip — `{tokens.<path>}` interpolation covers this |

---

## 11. Style discipline

PRSS is the right tool for:
- **Visual themes** — the colors, spacing, radii, and type sizes
  every component reads.
- **Reusable visual recipes** — `card`, `btn`, `btn-primary`,
  `row`, `column`. Anything authored more than once.
- **State variants of recipes** — hover tints, selected
  highlights.

PRSS is the wrong tool for:
- **Per-instance overrides** — use PRUI inline `style:` /
  `style="{obj}"` (Wave 12). One-off colors don't earn a class.
- **Data-driven styling** — `style:background="{is-warn ? '#…' : '#…'}"`
  reads cleaner inline. Don't shoehorn ternaries into class names.
- **Component internals** — the block's own `lower_ui` body owns
  default styling. PRSS reshapes from the outside.

The rule of thumb: if you'd reach for a `<let>` to DRY a value
across one component body, that's an inline concern. If three
components need the same recipe, that's a PRSS class.
