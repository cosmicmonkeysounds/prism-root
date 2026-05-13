# PRUI Language Reference

> Full reference for **PRUI** — the Prism UI DSL. PRUI is the
> tag-element, attribute-namespaced source language for every visual
> surface Prism renders. Files use the **`.prui`** extension.

**Status:** living reference (2026-05-13). Authoritative grammar lives
in `packages/prism-core/src/language/prism_ui/`; lowering in
`packages/prism-ui-runtime/src/interpret.rs`; build pipeline in
`packages/prism-ui-build/`; resolver-side composition in
`packages/prism-builder/src/ui_resolver.rs`. Today's files still carry
the legacy `.prism-ui` extension; the `.prui` rename pass is a sibling
follow-up. Everything else here is current.

**Recently landed (2026-05-13):** `for-step` (`for="i in 0..100 step 10"`),
`for-reverse` (`for="item in items reverse"`), `key="…"` reconciliation
hint round-trip as `data-key`, §16 imperative-control discipline,
numeric units (`14px` / `1rem` / `0.5em` / `50%`), CSS-shorthand
padding (`padding="8 16"`, TRBL), `<fragment>` grouping element,
`@event` ≡ `on:event` and `:prop` ≡ `bind:prop` shorthands, `class="…"`
PRSS integration (see [PRSS reference](prss-reference.md)).

**Related docs:** ADR-008 (decision to replace Slint with the DSL),
`clay-migration-plan.md` (phase plan), `composable-builder-plan.md`
(Waves 9 – 15 of grammar), `dioxus-inspiration.md` (reactive
substrate, hot-reload pipeline, fingerprint cache).

---

## 1. Overview

PRUI is HTML-shaped markup with attribute-namespace behaviour. The
canonical surface ADR-008 set:

```prui
<container layout="flow" gap="{tokens.spacing.md}" on:click="emit save">
  <heading level="3">{title}</heading>
  <pill if="{badge}" tone="accent">{badge}</pill>
  <facet name="items" from="resource:posts" limit="5">
    <link href="{item.url}">{item.title}</link>
  </facet>
</container>
```

**Design rules:**

- **HTML-shaped.** `<tag attr="value">...</tag>` and `<tag/>`. The
  surface reads as HTML so highlighters, formatters, and humans
  recognise it on first sight.
- **Attribute-namespaced behaviour.** Every dynamic feature (events,
  bindings, styles, control flow, modifiers, slot routing) is a
  namespaced attribute, never a new tag. `on:click`, `style:radius`,
  `route:role`, `bind:value`, `if`, `for`, `use:hover`.
- **No regex, no string indexing.** The parser is driven by
  `prism_core::language::syntax::Scanner` (project-wide rule for
  every language parser).
- **One render path.** Compile-time codegen and runtime live-edit go
  through the same parser and the same `interpret::lower_document_with_scope`
  pipeline. There is no separate interpreter.
- **One language, three runtimes.** A single `.prui` file lowers to
  the native femtovg backend, the wasm32 + WebGL browser backend,
  and the semantic-HTML SSR backend without per-target code.

---

## 2. File format

- **Extension:** `.prui` (canonical). Legacy `.prism-ui` is still
  accepted by `prism_core::language::prism_ui::create_prism_ui_contribution()`'s
  `PRISM_UI_EXTENSIONS`.
- **Encoding:** UTF-8.
- **Whitespace:** preserved in text runs, ignored between tags. The
  control-flow chain rule treats inter-tag whitespace as
  non-breaking, so an `if=` / `else=` pair on adjacent lines still
  pairs up.
- **Comments:** `<!-- ... -->`. Preserved by the parser so the
  formatter can round-trip; ignored by the lowering pass.
- **Top-level shape:** zero or more sibling nodes. There is no
  required root; `app.prui` carries the studio shell + sibling
  overlay tags as parallel roots.

```prui
<!-- this is a comment -->
<container>
  <text>Hello.</text>
</container>
<shell.toast-stack/>
```

---

## 3. Built-in element vocabulary

The runtime owns a **closed set** of seven primitive tags. Every
other tag is dispatched through a host-registered `TagResolver`
(§9). The closed set:

| Tag | Lowers to | Purpose |
|---|---|---|
| `container` | `Node::Container` | The universal flex/grid box. Direction, gap, padding, sizing, hover, background, semantic role/tag. |
| `component` | `Node::Container` | Alias for `container` reserved for declaration sites (`<component name="…">`). The compile-time codegen extracts these by name. |
| `text` | `Node::Text` | Inline text run. Reads font-size, colour. Interpolates `{expr}` bodies in children. |
| `heading` | `Node::Text` | Same shape as `text`. The `level` attribute (1 – 6) maps to an HTML-style default font size (28, 24, 20, 18, 16, 14 px). |
| `spacer` | `Node::Spacer` | Fixed-size gap. `width` / `height` attrs only. |
| `input` | `Node::TextInput` | Single-line text input. Keystroke handling lives in the runtime, not the DSL. |
| `image` | `Node::Image` | Bitmap or SVG. `src`, `width`, `height`, `style:radius`, `style:tint`. |
| `slot` | (passthrough) | Named-slot insertion point (§7.2). Falls back to the slot's own children when the caller didn't override. |
| `host-children` | (passthrough) | Inject the caller's pre-lowered children at this seam (§7.1). |
| `let` | (no-op) | Sibling-scoped binding declaration (§6.4). |
| `fragment` | (passthrough) | Emit children verbatim with no wrapping container. Useful for multi-element `if=` / `for=` bodies; React `<>…</>` / Vue `<template>` / Svelte `<svelte:fragment>` equivalent. |

**Everything else** — `<shell.icon-button>`, `<prism.text-input>`,
`<my.card>` — is a *registered tag* routed through the host's
`TagResolver`. The runtime treats an unknown tag as "drop the
wrapper, keep its children" if no resolver claims it.

### 3.1 `<container>` attributes

| Bare attribute | Type | Default |
|---|---|---|
| `id` (or `class`/`id` namespace) | string | `""` |
| `direction` | `row` \| `column` | `column` |
| `gap` | number (px) | `0` |
| `padding` | number (px) — uniform | `0` |
| `padding-left`, `padding-right`, `padding-top`, `padding-bottom` | number (px) | `0` |
| `width`, `height` | `grow` \| `fit` \| `<number>` (px) | `fit` |
| `tag` | string — HTML tag name on the `Semantic` carrier | `None` |
| `role` | string — ARIA role | `None` |
| `aria-label` | string — accessibility label | `None` |
| `key` | string — reconciliation hint; lowers to `data-key` semantic attr (§6.2.1) | `None` |

Anything else lowered through the `style:`, `on:`, `route:`, `data:`,
`aria:`, `bind:`, `transition:`, `use:` namespaces; see §4.

### 3.2 `<text>` and `<heading>` attributes

| Bare attribute | Type | Default |
|---|---|---|
| `id` | string | `""` |
| `font-size` | number (px) | runtime default |
| `level` (heading only) | 1 – 6 | 1 |
| `style:color` | hex | runtime default |

Text content is a single text run formed from concatenated children
(whitespace-trimmed text + interpolations). Interpolations that
resolve to `""` are dropped (no leading separator).

### 3.3 `<image>` attributes

| Attribute | Type |
|---|---|
| `src` (or `source`) | string — asset path |
| `width`, `height` | `grow` \| `fit` \| number |
| `style:radius` | number (px, uniform corner radius) |
| `style:tint` (or `style:color`) | hex — tint colour |
| `tag`, `role`, `aria-label` | semantic carriers |
| `aria:*`, `data:*`, `route:*` | pass-through |

### 3.4 `<input>` attributes

| Attribute | Type |
|---|---|
| `id` | string |
| `value` | string |
| `placeholder` | string |
| `font-size` | number |
| `style:color` | hex |
| `width`, `height` | sizing |

Note: `<input>` is the runtime's *primitive* text input. Real
keystroke handling, focus management, IME, and multi-line buffers
live in `prism.text-input` / `prism.text-buffer` (Tier-3
primitives) — `<input>` itself is the bare retained widget.

### 3.5 `<spacer>` attributes

`id`, `width`, `height`. Nothing else.

### 3.6 Numeric units

Length-valued attributes (`gap`, `padding`, `radius`, `font-size`,
`width`, `height`, `padding-left/right/top/bottom`, …) accept CSS
unit suffixes. Single seam: every length-valued attribute parses
through `parse_f32` / `parse_sizing` so author intent is uniform
across the DSL.

| Suffix | Meaning | Example |
|---|---|---|
| (none) | px (matches CSS bare-number rule) | `padding="16"` |
| `px` | px | `padding="16px"` |
| `rem` | `n × REM_PX` (16) | `font-size="0.875rem"` → 14 |
| `em` | same as `rem` today | `padding="1em"` |
| `%` | sizing only; lowers to `Sizing::Percent(n/100)` | `width="50%"` |
| `grow` | sizing only; `Sizing::Grow` | `height="grow"` |
| `fit` / `auto` | sizing only; `Sizing::Fit` | `width="fit"` |

`em` resolves to the same constant as `rem` today — proper
parent-font-size scope threading is a follow-up. The `tokens`
binding (§8) is the idiomatic path for theme-driven sizes.

---

## 4. Attribute namespaces

Every attribute is classified into a namespace by its prefix (or by
being a known keyword). The classifier is
`AttributeNamespace::classify(raw)`. Twelve namespaces in total:

| Namespace | Prefix / keyword | Lowers to | Example |
|---|---|---|---|
| **Bare** | (no prefix) | Container/text/image bare props (§3.1) | `gap="8"` |
| **On** | `on:` *or* `@` shorthand | `data-on-<event>` semantic attr; routed by the shell event router | `on:click="cmd save"` / `@click="cmd save"` |
| **Bind** | `bind:` *or* `:` shorthand | `data-bind-<prop>` semantic attr; `DocumentBindings::install_for` installs the reactive `Effect` at document load | `bind:value="form.email"` / `:value="form.email"` |
| **ControlFlow** | `if` / `else-if` / `else` / `for` | Sibling expansion pass (§6) | `if="{count > 0}"` |
| **Style** | `style:` | Style override into `ContainerProps` / `TextProps` (§4.3) | `style:background="#0060c0"` |
| **Facet** | `fct:` | Reserved for `FacetDef` lowering; pass-through today | `fct:items="resource:posts"` |
| **Signal** | `sig:` | Reserved for `SignalDef` shorthand; pass-through today | `sig:clicked="..."` |
| **Aria** | `aria:` | `aria-<key>` HTML attribute on `Semantic.attrs` | `aria:selected="true"` |
| **Data** | `data:` | `data-<key>` semantic attr; participates in hit-test cache | `data:role="palette-item"` |
| **Route** | `route:` | Sugar for `data:` — same lowering, dedicated namespace for hit-routing attrs | `route:role="resize-handle"` |
| **Transition** | `transition:` | `data-transition-<prop>` semantic attr; future `Effect`-driven animator hook | `transition:opacity="200ms"` |
| **Use** | `use:` | Attach a registered `ModifierBehaviour` by id (Wave 13.3); the attribute value becomes the modifier's first prop | `use:tooltip="Click to save"` |
| **Identifier** | `class` / `id` | `id` sets the node id; `class` reserved for inspector / HTML | `id="palette::row"` |

Two cross-cutting rules:

- **Empty-string filter.** `data:foo=""`, `aria:bar=""`, and
  `on:event=""` are *dropped* before lowering. This lets ternary
  branches that resolve to `""` omit attributes cleanly:
  `aria:level="{depth > 0 ? depth + 1 : ''}"` emits no attribute
  when `depth == 0`.
- **Pseudo-state suffix.** A `style:<key>:<state>` attribute splits
  off a trailing `:hovered` / `:selected` / `:focused` suffix. See
  §4.3.

### 4.1 `on:` — events

`on:<event>="<action>"` lowers to `data-on-<event>` on the
`Semantic.attrs` carrier. The shell event router reads it at
pointer-down time and dispatches through
`prism_builder::signal::parse_action`. Action grammar:

| Action | Shape | Notes |
|---|---|---|
| `cmd <id>` | `cmd file.save` | Dispatch a registered command by id. |
| `emit <signal>` | `emit changed` | Fire a `SignalDef` on the node. |
| `set <key> = <expr>` | `set selected = true` | `ActionKind::SetProperty`. |
| `toggle <prop>` | `toggle visibility` | `ActionKind::ToggleVisibility`. |
| `navigate to <page>` | `navigate to home` | `ActionKind::NavigateTo`. |
| `play <animation>` | `play fade-in` | `ActionKind::PlayAnimation`. |
| `bind <key> from <source>` | `bind value from form.email` | Two-way binding wiring. |
| `luau { … }` | `luau { props:write('x', 1) }` | Inline Luau handler — the escape hatch. |

**Event modifiers** (Wave 14.3 — fully wired). `on:click.once`,
`on:click.stop`, `on:click.prevent` lower to `data-on-click-once`,
`data-on-click-stop`, `data-on-click-prevent` respectively (dotted
suffix joined with dashes). At dispatch time:

- **`.once`** — the shell records the `(hit-id, attr-key)` pair in
  `AppState::once_fired` after the first dispatch; subsequent
  clicks on the same handler are no-ops until the document
  reloads.
- **`.stop` / `.prevent`** — the router returns the consume bit
  regardless of whether the action did observable work, so the
  rest of the pointer-down chain (canvas selection, palette drag,
  gizmo capture) is suppressed. In a retained-mode tree there is
  no parent-bubbling to halt; "propagation" here means the
  pointer-down fallback chain.

Modifiers compose: `on:click.once.stop="emit save"` fires once and
consumes the press. Order is irrelevant — the suffix parser splits
on `-` and matches against the recognised set.

### 4.2 `bind:` — reactive bindings

`bind:value="form.email"` lowers to `data-bind-value="form.email"`.
At document load time `DocumentBindings::install_for(&node)` reads
the attr and installs an `Effect` that subscribes to the right
signal and writes back to `props.value`. Same wiring as
`ActionKind::Bind` (Phase 4a of the dioxus reactive plan); the
attribute is the authoring sugar.

### 4.3 `style:` — visual styling

Lowers into `ContainerProps` / `TextProps` / `Image` fields directly,
not as data-attrs. Six recognised bare keys for containers
(`apply_style_override` in `interpret.rs`):

| Key | Lowers to | Type |
|---|---|---|
| `background` | `ContainerProps.background` | hex colour |
| `radius` | `ContainerProps.radius` (uniform) | number (px) |
| `padding` | `ContainerProps.padding` | CSS-shorthand: `"8"` (uniform), `"8 16"` (V/H), `"8 16 24"` (top/H/bottom), `"8 16 24 32"` (TRBL) |
| `padding-left/right/top/bottom` | per-side padding | number (px) |
| `gap` | `ContainerProps.gap` | number (px) |
| `width`, `height` | `ContainerProps.width/height` | `grow` \| `fit` \| number |

Plus two pseudo-state recognised on the containers:

| Key + state | Lowers to |
|---|---|
| `style:background:hovered` | `ContainerProps.hover.background` |
| `style:radius:hovered` | `ContainerProps.hover.radius` |

Other state suffixes (`:selected`, `:focused`) round-trip as
`data-style-<key>-<state>` semantic attrs — author intent
survives but no runtime swap fires today.

`<text>` and `<image>` recognise their own narrow set
(`style:color`, `style:tint` / `style:color`, `style:radius` on
images). Unknown `style:` keys on any tag drop silently — they are
not synthesised into `data-style-*`. Authors who want arbitrary
data payloads use `data:` directly.

### 4.4 `route:` and `data:` — routing attrs

Both lower to `data-<key>` on the `Semantic` carrier. They are
identical at the wire level; `route:` exists as a dedicated
namespace for hit-test routing attrs so authoring stops feeling
like manual `data-*` ladders. Use `route:` for "this surface is
clickable / has a target id / has a direction"; use `data:` for
generic key-value payloads consumed by JS or SSR (or the same
hit-test cache).

```prui
<container
    route:role="resize-handle"
    route:direction="br"
    data:target-id="{node-id}"/>
```

### 4.5 `aria:` — accessibility

`aria:<role>="<value>"` lowers to `aria-<role>` on the semantic
attrs. Empty-string filter applies: `aria:selected=""` is dropped.
Used by SSR (real HTML attribute) and the runtime hit-test path
(treated as semantic metadata).

### 4.6 `use:` — modifier attachment

Wave 13.3. `use:<modifier-id>[="<value>"]` attaches a registered
`ModifierBehaviour` by id from the DSL. Today it round-trips as a
`data-use-<id>` semantic attr; full integration with the
`ModifierBehaviour::wrap` render-fold pipeline is a follow-up.
When wired:

```prui
<container use:hover use:tooltip="Click to save"/>
```

is equivalent to authoring two `Modifier` entries on the node's
`modifiers: Vec<Modifier>` table directly.

### 4.7 `transition:` — animations

`transition:<prop>="<duration>"` lowers to
`data-transition-<prop>`. The
[`prism_ui_runtime::animator::Animator`] substrate consumes the
attribute through a three-call lifecycle: `observe` pre-render to
detect a moved declared value, `apply` mid-render to rewrite the
prop to its interpolated sample, and `tick` post-render to prune
finished transitions. Hosts that want animated transitions
construct one `Animator`, share it across frames, and merge its
`needs_redraw()` bit into the per-frame dirty signal.

Recognised props today (all numeric on `ContainerProps`): `gap`,
`padding` (uniform), `padding-left`/`-right`/`-top`/`-bottom`,
`radius` (uniform), and `width`/`height` when sized `Fixed(n)`.
Mixed-corner radii, percentage sizing, and color interpolation are
follow-up extensions of the same substrate. Duration parses as
`"200ms"` / `"1.5s"` / bare-integer-milliseconds; easing defaults
to linear (`Easing::Linear`) with `EaseIn`, `EaseOut`, `EaseInOut`
available through the imperative `start_with_easing` API.

---

## 5. Expression language

Embedded between `{` and `}`. Two contexts:

1. **Attribute interpolation.** `width="{1 + offset * 2}"`,
   `style:background="{tokens.colors.surface}"`,
   `for="item in items"`, `if="{count > 0}"`.
2. **Text content.** Element children of `<text>` / `<heading>`
   bodies: `<text>{count} items selected</text>`.

The grammar is `prism_core::language::expression`. Pratt parser
producing `AnyExprNode`; evaluator returns `ExprValue` which is
loosely typed (number / boolean / string / null / list / object).

### 5.1 Literals

- **Numbers:** `1`, `0.5`, `-3`. Decimals supported. Underscores not.
- **Strings:** `'single'` or `"double"`. Both forms allowed inside
  `{...}` bodies. Outside the body, attribute values are written
  in HTML-style double-quoted strings, so single quotes inside the
  body avoid the escape ladder: `style:color="{is-row ? '#aaa' : '#000'}"`.
- **Booleans:** `true`, `false`.
- **Null:** `null`.

### 5.2 Operators

Precedence high → low (matches Rust / JavaScript intuition):

| Tier | Operators | Notes |
|---|---|---|
| Unary | `!`, `-` | Logical not, arithmetic negation |
| Multiplicative | `*`, `/`, `%` | Multiply / divide / modulo |
| Additive | `+`, `-` | Add / subtract; `+` is also string concat for `string + string` |
| Comparison | `<`, `<=`, `>`, `>=`, `==`, `!=` | Loose equality (number↔string coerce); never strict-equal-only |
| Logical | `&&` (also `and`), `\|\|` (also `or`) | Short-circuit |
| Ternary | `cond ? then : else` | Right-associative; chains as `a ? b : c ? d : e` |
| Power | `**` | Right-associative; `2 ** 3` = 8 |

Boolean truthiness for `if=` / `&&` / `?:` / `!`: empty string,
zero, false, null, and empty arrays/objects evaluate false;
everything else is truthy.

### 5.3 Identifiers and dotted paths

A bare identifier reads from the current `LowerScope`'s binding
table. Dotted paths walk JSON object fields:

```prui
{user.name}
{tabs.0.label}                     <!-- 0-based array index -->
{tokens.colors.surface}
```

`for` iterators bind in the immediate scope, so within a loop body
`item` and `item.label` resolve through scope.

### 5.4 Built-in functions

Provided by `language::expression::evaluator::builtin_functions`.
A representative subset:

| Function | Shape | Notes |
|---|---|---|
| `if(cond, then, else)` | ternary alias | Useful in template contexts |
| `len(x)` | `string \| array \| object` → number | |
| `min(a, b, …)`, `max(a, b, …)` | varargs | |
| `round(n)`, `floor(n)`, `ceil(n)`, `abs(n)` | number → number | |
| `concat(a, b, …)` | varargs → string | |
| `upper(s)`, `lower(s)` | string → string | |
| `format(fmt, …)` | `"{:.2}"` style | Driven by `format_number` |
| ISO-date helpers | `today()`, `now()`, `add_days(d, n)`, … | Powered by `chrono` |

The full list lives in `language::syntax::syntax_engine::builtin_functions`
— the same registry the LSP completions read from.

### 5.5 Mixed templates

An attribute value can interleave literal text with interpolations:

```prui
<container id="palette::row::{idx}"
           padding-left="{12 + depth * 16}">
```

Template parts are evaluated independently and stringified;
typed-only `<let value="{n + 1}"/>` (where the entire value is a
pure `{expr}`) preserves the underlying number / bool / object
type. Mixed templates always collapse to a string.

---

## 6. Control flow

Sibling-level only. The control-flow pre-pass (`expand_control_flow`)
walks every sibling list once before lowering, evaluating chains
and seeding scopes per-branch. Nested control flow always lives in
a child element.

### 6.1 `if` / `else-if` / `else`

```prui
<container if="{count > 0}">…</container>
<container else-if="{loading}">…</container>
<container else>Empty state.</container>
```

- Chains track a "taken" flag across siblings. A non-element
  sibling with content (text or interpolation) breaks the chain;
  whitespace-only siblings don't.
- An `else` after a `for=` that produced zero items renders as the
  empty-state fallback (Svelte's `{:each}{:else}` shape, Wave 15.1).

```prui
<shell.row for="row in rows"/>
<text else>No rows.</text>
```

### 6.2 `for` — iteration

Three iteration shapes, dispatched by the right-hand-side value type:

| Source type | Form | Variable binding |
|---|---|---|
| Array | `for="item in items"` | `item` ← element |
|  | `for="item, idx in items"` | `item` + `idx` (integer) |
| Object | `for="value, key in obj"` | `value` ← entry value, `key` ← string key |
| Numeric range | `for="i in 0..10"` | `i` ← integer (0 .. 9 exclusive) |
|  | `for="i in 0..=10"` | `i` ← integer (0 .. 10 inclusive) |

Range endpoints are full expressions: `for="i in 0..items.length"`
works. Reverse ranges (start > end) yield zero items, matching
Rust's `Range` semantics.

**Modifiers** apply after the source on a `for=` clause:

| Modifier | Shape | Applies to | Effect |
|---|---|---|---|
| `step N` | `for="i in 0..100 step 10"` | Range only | Step by `N` between endpoints. `N >= 1`; zero or negative drops the element. Matches Python `range(0, 100, 10)`. |
| `reverse` | `for="item in items reverse"` | Range, array, object | Flip the emitted order. |

Both modifiers compose and accept either order:
`for="i in 0..100 step 10 reverse"` and
`for="i in 0..100 reverse step 10"` are equivalent. A repeated
modifier (`reverse reverse`, `step 1 step 2`) rejects the whole
clause so the missing output surfaces the typo.

The loop body is the element itself, applied once per item with
`item` (and optionally `idx` / `key`) bound in a fork of the
parent scope.

### 6.2.1 `key="…"` — reconciliation hint

Round-trips to a `data-key` semantic attr. The runtime tree-diff
that would consume the hint for stable cross-render identity is a
future unblock; today the hint is preserved so SSR and any future
incremental-diff substrate inherit author intent verbatim.

```prui
<shell.signal-connection-row
    for="row in connections"
    key="{row.id}"
    props="{row}"/>
```

Empty-string values drop the attribute, matching the `data:` /
`aria:` empty-string filter convention so a ternary that resolves
to `""` omits the hint.

### 6.3 `let` — sibling-level constants

Wave 14.2 / Svelte `{@const}` lookalike. Lifts an expression into
a binding visible to subsequent siblings only.

```prui
<let name="is-row" value="{kind == 'row'}"/>
<let name="is-empty" value="{kind == 'empty'}"/>
<container style:background="{is-row ? '#0a000000' : (is-empty ? '#fff' : '#26000000')}"/>
```

Typed: a pure `{expr}` value preserves the underlying JSON type
(number / bool / object). Templated values
(`value="row-{idx}"`) flatten to string.

`<let>` renders nothing. Place it before the elements that read it
in the same sibling list.

### 6.4 Combining

`if` / `else-if` / `else` and `for` are mutually exclusive on a
single element. Wrap with a `<container>` if you need both:

```prui
<container for="row in rows">
  <text if="{row.error}">Failed: {row.error}</text>
  <text else>{row.label}</text>
</container>
```

---

## 7. Composition

Four composition mechanisms cooperate inside a single component:
host-children, named slots, props spread, and style spread.

### 7.1 `<host-children/>` — pre-lowered children injection

Inside a component body, `<host-children/>` emits the caller's
pre-lowered children at that point. The shell's `.prui` loader
stuffs the resolver-provided `host_children` into
`LowerScope::host_children_ui` before invoking
`lower_document_with_scope`, and the element handler emits them
verbatim.

```prui
<!-- shell.signals-panel body -->
<container tag="section" direction="column" gap="4">
  <text if="{title}">{title}</text>
  <host-children/>
  <shell.signal-connection-row for="item in connections" props="{item}"/>
</container>
```

A caller writes the wrapper as:

```prui
<shell.signals-panel title="Connections" connections="{rows}">
  <text>Hint: drag rows to reorder.</text>
</shell.signals-panel>
```

The `<text>` ends up exactly where `<host-children/>` is in the
component body.

`<host-children/>` falls back to its own AST children when nothing
is injected (acts as a fallback slot). It also accepts an opt-in
`name="X"` attribute that pulls from the named-slot map (next
section).

### 7.2 `<slot name="…"/>` — named slots

Wave 13.1, Vue / Web Components shape. The resolver buckets a
dispatched element's children by their `slot="X"` attribute, then
threads the resulting map through `LowerScope::host_children_by_slot`.
`<slot name="X"/>` reads from the map first, then falls back to
its own children.

```prui
<!-- shell.app-window body -->
<container direction="column">
  <slot name="menu">
    <shell.menu-bar-row id="menu"/>
  </slot>
  <container direction="row">
    <slot name="nav">
      <shell.nav-button for="b in buttons" props="{b}"/>
    </slot>
    <container>
      <host-children/>
    </container>
  </container>
  <slot name="status">
    <shell.status-bar text="{status}"/>
  </slot>
</container>
```

A caller overrides a slot with `slot="…"` on an AST child:

```prui
<shell.app-window>
  <text slot="status">Custom status</text>
  <shell.builder-canvas/>  <!-- falls into the default `host-children` body -->
</shell.app-window>
```

Resolution order inside a `<slot name="X"/>`:

1. AST-level slot bindings (`LowerScope::slots`) — used by
   in-DSL template expansion.
2. Pre-lowered named-slot map (`host_children_by_slot`).
3. The slot element's own children (the fallback content).

### 7.3 `props="{obj}"` — props spread

Resolver-side (Wave 11.2). Unpacks an object's entries onto the
dispatched node's `props` without enumerating each key.

```prui
<shell.nav-page-row for="item in items" props="{item}"/>
```

Equivalent to:

```prui
<shell.nav-page-row for="item in items"
    id="{item.id}"
    label="{item.label}"
    selected="{item.selected}"/>
```

Schema defaults (declared on the receiving block) merge in before
the spread, so missing-but-defaulted keys inherit the Rust-side
default — `show-add=true` on `signals-panel` doesn't need
restating in DSL.

### 7.4 `style="{obj}"` — style spread

Wave 12. Each key of the spread object is fed through
`apply_style_override` exactly like an authored `style:` attribute.
Composes with explicit `style:` attrs on the same element — the
order matters but normally the spread is the *theme* and the
explicit attrs are the *overrides*.

```prui
<shell.icon-button style="{theme.button}" style:background="#ff0000"/>
```

The spread's `style` key is invisible to the receiving block (it
never appears in `node.props`).

### 7.5 `use:<modifier-id>` — declarative modifier attachment

See §4.6.

---

## 8. Design tokens

Wave 14.1. Every `.prui` lowering scope is seeded with a `tokens`
binding shaped exactly like `prism_core::design_tokens::DesignTokens`:

```prui
<container
    padding="{tokens.spacing.md}"
    style:background="{tokens.colors.surface}"
    style:radius="{tokens.radius.md}">
  <text font-size="{tokens.typography.font-size-md}"
        style:color="{tokens.colors.text-primary}">
    {title}
  </text>
</container>
```

Token shape:

| Path | Type | Example |
|---|---|---|
| `tokens.colors.<name>` | hex string `#rrggbbaa` | `background`, `surface`, `surface-elevated`, `border`, `text-primary`, `text-secondary`, `accent`, `accent-muted`, `danger`, `success` |
| `tokens.spacing.<size>` | number (px) | `xs`, `sm`, `md`, `lg`, `xl` |
| `tokens.radius.<size>` | number (px) | `sm`, `md`, `lg`, `pill` |
| `tokens.typography.<key>` | number | `font-size-sm/md/lg/xl`, `line-height-md` |

Colours emit as strings the same `parse_color` consumes; spacing /
radius / type emit as raw integers `parse_f32` consumes.

---

## 9. Host extension — custom components

The runtime's tag vocabulary is closed (§3). Everything else is a
*registered tag* dispatched through `TagResolver`:

```rust
pub trait TagResolver: Send + Sync {
    fn resolve(&self, element: &Element, scope: &LowerScope) -> Option<Vec<Node>>;
}
```

In practice, two resolvers cover the workspace:

- `prism_builder::ui_resolver::RegistryTagResolver` — dispatches
  every registered `BlockSpec` (shell chrome, builder builtins,
  primitives, user prefabs, Luau-registered components).
- A future plugin resolver chains under it for plugin-provided
  vocabularies.

A new tag is one row in a `BlockSpec` table or one `.prui`
declaration in `SHELL_PRISM_UI_COMPONENTS` (the DSL self-hosting
seam — §11.2 of the composable-builder plan).

When the resolver returns `Some(vec![...])`, the runtime emits
those nodes in place of the tag. Returning `None` falls through to
"drop the wrapper, keep its children" — useful for namespace
groupings.

### 9.1 Authoring a `.prui` component

```prui
<!-- packages/prism-shell/ui/components/icon-button.prui -->
<container
    tag="button"
    direction="row"
    width="32" height="32"
    style:radius="4"
    style:background:hovered="#1a000000"
    role="button"
    aria-label="{tooltip-text}"
    data:role="icon-button"
    data:on-click="cmd {command}">
  <image src="{icon}" width="16" height="16"/>
</container>
```

Then register it in the shell's `SHELL_PRISM_UI_COMPONENTS` table
with a `PrismUiSpec` carrying the schema (`FieldSpec` rows for
`icon`, `tooltip-text`, `command`). The loader parses the file at
boot, exposes the tag as `shell.icon-button` through the registry,
and `RegistryTagResolver` dispatches.

### 9.2 Component schemas + the Inspector

Every block's `Component::schema() -> Vec<FieldSpec>` flows into
the Inspector property panel. A `.prui` component declares its
schema in the `PrismUiSpec` row. A field's `kind`
(`text`, `number`, `select`, `color`, `file`, `textarea`, …)
drives the inspector editor:

| Kind | Inspector UI |
|---|---|
| `text` | one-line text input |
| `textarea` | multi-line text input (Shift-Enter inserts newline) |
| `number` / `integer` | drag-scrub field; arrow keys nudge ±1 / ±10 |
| `select` | click-to-cycle (popover with full list lands with `prism.select`) |
| `color` | text-edit session for hex (HSL picker lands with `prism.color-picker`) |
| `file` | text-edit session for path (native dialog lands with `prism.file-button`) |
| `boolean` | checkbox |

---

## 10. Reactivity

PRUI is a *render layer* over the reactive substrate. Every prop
value is read through `LowerCtx::prop_*` which routes through
`DocumentBindings::props_for(node_id).signal(key)` — Phase 4b of
the dioxus reactive plan. Writes go through
`NodeMutator::with_bindings(...)`. The DSL author rarely sees the
signal layer directly; they see:

- **`bind:value="path"`** — installs an `Effect` that watches `path`
  and writes `value` whenever it fires.
- **`on:click="emit save"`** — fires the named signal.
- **`<container if="{state.loaded}"/>`** — the `if=` predicate's
  expression auto-subscribes the surrounding block's
  `BlockInvalidator` reactive context, so a write to `state.loaded`
  re-walks the block.

Luau modifiers can read / write any prop through `ReactiveProps`:

```lua
function mod.install_effects(node_id, bindings, owner)
    local label = bindings:props_for(node_id):signal("label")
    effect(function()
        local v = label:read()
        -- … fires whenever any DSL writer commits to `label`
    end, owner)
end
```

Cross-process / cross-relay reactivity is handled by the
`RemoteSignal` / `RemoteState` carriers — a prop bound to a
daemon-published value transparently routes through IPC; a prop
bound to a federated topic routes through `FederatedSignal`. No
DSL change.

### 10.1 `class="…"` and PRSS

A `class="..."` attribute (whitespace-separated class names) on a
container opts that container into one or more **PRSS classes** —
the stylesheet vocabulary documented in
[`prss-reference.md`](prss-reference.md). When a stylesheet is
installed on the active `LowerScope`, each class resolves through
its `extends` chain, applies base properties, then layers state
overrides — all through the same `apply_style_override` vocabulary
inline `style:` uses.

```prui
<container class="btn">Cancel</container>
<container class="btn-primary">Save</container>
<container class="card row" gap="12"><!-- multiple classes --></container>
<container class="btn-primary" style:background="#ff0000"><!-- inline wins --></container>
```

Application order: block defaults → classes (left to right) →
inline `style:` → `style="{obj}"` spread. Without a stylesheet
loaded, `class="…"` round-trips harmlessly (no styling change),
so the same DSL works in headless / SSR / first-boot contexts.

---

## 11. Build pipeline

### 11.1 Compile time

A consumer crate's `build.rs` calls `prism_ui_build::compile("ui/app.prui")`
once per source file. The generated Rust module is written to
`OUT_DIR` and brought into scope via
`include!(concat!(env!("OUT_DIR"), "/app.rs"))`.

The generated module exposes:

| Item | Purpose |
|---|---|
| `pub const SOURCE: &str` | Original `.prui` text (re-parsed at runtime) |
| `pub const STRUCTURAL_HASH: u64` | Tree-shape fingerprint (Phase 10 of dioxus plan) |
| `pub const FULL_HASH: u64` | Tree + literals fingerprint |
| `pub const COMPONENT_NAMES: &[&str]` | Names of every `<component name="…">` declaration |
| `pub fn nodes() -> Vec<runtime::Node>` | Re-parses `SOURCE` and lowers via `interpret::interpret` |

The re-parse is intentional: `.prui` is the source of truth, the
AST is derived. Build-time validation guarantees `nodes()` never
panics on parse error in a shipped binary.

### 11.2 Runtime live edit

The shell parses `ui/app.prui` at boot through the same
`interpret::interpret(source)` the codegen uses. `prism dev shell`
watches `packages/prism-shell/ui/` for `.prui` edits and
respawns the child (or, with `--hot=subsecond`, patches the
`lower_template` body in place).

### 11.3 Fingerprint cache (template hot-reload)

`prism_ui_build::FingerprintCache::observe(path) -> TemplateChange`
returns one of:

| Variant | Meaning | Dev-loop action |
|---|---|---|
| `FirstSighting { fingerprint }` | First time we've seen this path | Seed the cache |
| `NoChange` | Identical bytes since last observe | Skip respawn |
| `LiteralOnly { patches: Vec<LiteralPatch> }` | Same tree shape, only literal values changed | Apply in-place patches (no re-walk) |
| `Structural` | Tree shape changed | Full respawn / `subsecond::call` re-entry |
| `ParseError { message }` | Diagnostic | Surface to editor; keep last good tree |
| `ReadError { message }` | I/O | Surface; keep last good tree |

This is the substrate for "literal-only tweaks update without
losing scroll position, focus, or animation state." Pairs with
`subsecond` for `.rs` body edits.

---

## 12. Diagnostics

`parse(source) -> (Document, Vec<ParseError>)`. The parser
recovers and continues so the editor can show every error in one
pass. Each `ParseError` carries:

| Field | Notes |
|---|---|
| `message` | Human-readable |
| `range: SourceRange` | Span in the source |
| `code: &'static str` | Stable diagnostic id |

Stable codes (representative):

| Code | Triggered by |
|---|---|
| `stray-close-tag` | `</tag>` with no matching open |
| `unterminated-comment` | `<!--` without closing `-->` |
| `unterminated-element` | `<tag` reaches EOF before `>` |
| `unexpected-end-tag` | `<a><b></a>` |
| `invalid-attribute-name` | Non-identifier characters in attr name |

The build script (`prism_ui_build::compile`) returns
`CompileError::Parse { count, first }` if any diagnostic is
present, aborting the build — `.prui` files in `src/` always
have to parse clean before linking.

The `PrismUiSyntaxProvider` (`prism_ui::provider`) layers
LSP-style diagnostics, completions (tag names, attribute
namespaces), and hover on top — used by the editor / LSP.

---

## 13. Idioms

### 13.1 Empty-state fallback after `for`

```prui
<shell.signal-connection-row for="item in connections" props="{item}"/>
<text else style:color="#80000000">No connections yet.</text>
```

### 13.2 `<let>` to DRY ternary cascades

When the same predicate appears 3+ times, lift it once:

```prui
<let name="active" value="{kind == 'node' && selected}"/>
<container
    style:background="{active ? '#26000000' : ''}"
    aria:selected="{active ? 'true' : ''}">
```

### 13.3 Typed-attr passthrough via `props="{item}"`

For loops over `Vec<Object>`, spread the row object onto the row
block:

```prui
<shell.nav-page-row for="item in pages" props="{item}"/>
```

### 13.4 Conditional ARIA / data

Use the empty-string filter, not a wrapping `if`:

```prui
<container aria:level="{depth > 0 ? depth + 1 : ''}"
           data:state="{is-pending ? 'pending' : ''}"/>
```

### 13.5 Hover tint via the modifier substrate

Don't hand-roll a hover container; attach the modifier:

```prui
<container use:hover>…</container>
```

### 13.6 Style spread for theming

```prui
<shell.icon-button style="{tokens.controls.icon-button}"/>
```

### 13.7 Overlay-gate via `if`/`else`

The picker-overlay pattern (Wave 11.2 batch 5):

```prui
<container if="{open}" data:role="picker">
  …full open body…
</container>
<container else width="0" height="0" data:visible="false"/>
```

Avoids a separate `chrome::hidden_overlay` helper.

---

## 14. Anti-patterns

- **Don't hand-emit `data-on-<event>` attrs.** Use the `on:`
  namespace; the empty-string filter and event-modifier suffix
  shape only kick in there.
- **Don't write conditional attributes with `if=` on the parent
  element when one ternary will do.** Conditional ARIA / data
  attrs are idiomatic with the empty-string filter (§13.4); a
  wrapping `<container if="">` doubles the tree depth.
- **Don't author classes — `class="…"` exists for inspector
  addressing and HTML SSR.** Visual styling is `style:` attrs.
- **Don't hardcode hex colours that have a token.** Use
  `{tokens.colors.<name>}`. The token table is the single source
  of truth for the design system.
- **Don't fork a block to change one colour.** Use `style:` /
  `style="{}"` spread to override from the parent.
- **Don't author `<host-children/>` inside a `<slot/>`.** The two
  are orthogonal seams; `host-children` is the *whole* caller
  body, `slot` is one named bucket.
- **Don't reach for Rust when a Luau modifier will do.** A
  one-prop behaviour with `wrap` + `install_effects` is a single
  Luau file (§3.7 of the composable-builder plan).

---

## 15. What the DSL can't yet express

Three rough categories of authoring gap. Each is a known
follow-up; tracked across the composable-builder plan's Wave
tables. (Imperative control — `while`, `break`, `continue`,
`return` — is a fourth category, but a *deliberate* omission
rather than a gap; see §16.)

**Render-time imperative state.** Drag gestures with mid-drag
state machines, hit-test-aware document hosts, retained-mode
canvas paint, syntax-highlight passes with per-line spans. Ships
as runtime *primitives* (`prism.builder-host`, `prism.text-buffer`,
`prism.canvas-paint`) rather than DSL grammar. The DSL composes
against the primitive; the primitive owns the imperative body.

**Runtime tag dispatch.** `<{panel.tag} .../>` is not directly
expressible, but the equivalent shape lives at the resolver
level: **`<dispatch component="{expr}" props="{obj}"/>`** resolves
its `component` attribute through scope and dispatches against
the live registry at render time. `properties-panel` already uses
this to materialise per-row inspector editors whose component id
lives in data.

```prui
<dispatch for="row, idx in rows"
          id="props::row::{idx}"
          component="{row.component}"
          props="{row.props}"/>
```

The remaining limit: `<dispatch>` resolves through the host's
`TagResolver` (i.e. only registered tags), so it cannot target a
closed-set runtime primitive like `<container>` or `<text>` from
a data-driven name. That's intentional — the closed set is the
runtime's contract, not a registry.

**Per-frame imperative bodies.** Anything that needs to compute
during the render walk (selection bbox in viewport coordinates,
gizmo handle positions from a transform matrix, palette-drag
ghost positioned at cursor) lives in a Rust block today. Those
are the Tier-3 primitives the next section enumerates.

Three smaller gaps tracked as deferred Wave rows:

| Gap | Status |
|---|---|
| `<teleport to="…"/>` (overlay routing from a deep child) | ✅ Landed (Wave 14.3) — payload AST is collected by a document-level pre-scan and appended at the target id during lowering. Targets see the payload after their own children; binding resolution flows from the destination scope. |
| `v-memo` / memoised expressions | ✅ Landed (Wave 14.3) — `memo="[dep1, dep2]"` on an element with a resolvable `id` caches the lowered subtree in a host-supplied [`MemoCache`]. Unchanged dep tuples skip re-lowering verbatim. The shell threads its own `ShellInner::memo_cache` into every `render_tree_with` call, so authored memos survive across frames. |
| Two-way `bind:value` on `<input>` | ✅ Landed (Wave 14.3) — `bind:value="<node-id>.<key>"` lowers to a `data-bind-value` semantic attr; the shell click router consumes it to open a field-focus session, and the existing `FieldFocusService` routes subsequent `Event::Text` / `Event::Key` back to the source prop via `set_node_prop`. |
| Event modifier behaviour (`.once`, `.stop`, `.prevent`) | ✅ Landed (Wave 14.3) — the shell click router parses the modifier suffix off `data-on-click[-mod]` attr keys, gates dispatch through an `AppState::once_fired` set for `.once`, and returns the consume bit unconditionally for `.stop` / `.prevent`. |
| `transition:` animator | ✅ Landed (Wave 14.3) — `prism_ui_runtime::animator::Animator` substrate with `observe` / `apply` / `tick` lifecycle. Supports `gap`, `padding[-side]`, `radius`, fixed `width` / `height` interpolation with linear / cubic easing. Wired through `ShellInner::animator`: every `Shell::render` observes the live tree, applies in-flight transitions, and folds `animator.needs_redraw()` into the femtovg loop's dirty bit so the next frame ticks automatically. |

(Reconciliation `key="…"` itself round-trips today as `data-key`;
the *consumer* is the deferred half — a live tree-diff that uses
the hint to preserve cross-render identity.)

---

## 16. Imperative control — deliberately not in the grammar

PRUI does **not** support `while`, `do-while`, `break`, `continue`,
or `return`. These were skipped on purpose in Wave 15 of the
composable-builder plan. The rationale:

PRUI is a *bounded declarative tree-render DSL*. Each parse of the
source produces a finite tree of nodes; the runtime walks the
tree once per frame and exits. The classic imperative-control
vocabulary doesn't map onto that shape:

| Construct | Why it isn't in PRUI |
|---|---|
| `while (cond)` / `do-while` | A render walk has no halt condition — once the tree is walked, the frame is done. Use a bounded `for="i in 0..n"` whose `n` is a computed expression. |
| `break` | PRUI's `for=` is a *comprehension over an iterable*, not an imperative loop with mid-flight escape. Filter the data before iterating. |
| `continue` | Same shape as `break` — there is no per-iteration control flow to short-circuit. Wrap the body in `<container if="{predicate}">` to skip individual iterations. |
| `return` | PRUI elements don't *return* values; they *emit* nodes. To drop an element conditionally, gate it with `if="…"` on the element itself. |

**Modelling the same use cases declaratively:**

| Imperative idiom | PRUI shape |
|---|---|
| "Stop iterating when we hit a separator." | `for="row in rows-before-separator"` — filter the array host-side, render against the filtered shape. |
| "Skip rows where `row.hidden`." | `<container for="row in rows" if="{!row.hidden}">…</container>` — per-iteration `if=` filters the body. |
| "Render at most N rows." | `for="row in rows.slice(0, N)"` — bound the source. (Today done host-side; slicing in the expression layer is a future addition.) |
| "Bail out of a whole branch when `state.errored`." | `<container if="{!state.errored}">…</container>` around the branch. |
| "Loop until some computed condition." | Restate the upper bound: `for="i in 0..(max-iterations)"` with the bound computed in scope or supplied by the host. |

If a genuinely imperative body is required (early exit from a
long computation, recursive descent with mutation, a state
machine), the answer is a **Luau modifier** or a **Rust block**,
not new DSL grammar. Both have full imperative vocabularies and
compose into the DSL at the right seam:

- **Luau modifier** — author `wrap` / `install_effects` bodies in
  Luau (`use:my-modifier="…"` from the DSL). Full while / break /
  return / recursive call vocabulary inside the script.
- **Rust block** — implement `lower_ui` directly. The composable
  inspector + DSL self-host path migrates everything that *can*
  go to the DSL; what remains is exactly the set that needs
  imperative bodies (§17 below).

**Anti-pattern.** Don't try to fake imperative control with
recursive `<dispatch component="{this-component}"/>` calls and an
early-out `if=`. The render walk has no stack-frame model — every
dispatch is a fresh node; "recursion depth" doesn't exist as a
concept. If you find yourself reaching for it, the imperative body
belongs in Luau or Rust.

---

## 17. Tier-1 / Tier-2 Rust files that genuinely need imperative bodies

Per Wave 11.2 of `composable-builder-plan.md`, every visual-only
chrome component has been migrated to `.prui` source. What remains
in `packages/prism-shell/src/components/*.rs` (excluding
`mod.rs` / `registry.rs` / `chrome.rs` / `prism_ui_loader.rs` /
`field_editor.rs`) is the set the DSL can't yet express. Each is
either a Tier-3 primitive seam (the imperative body ships as a
runtime primitive and the wrapper shrinks to thin `.prui`) or a
true imperative leaf.

The seven Rust components, why each one needs a body the DSL
can't yet express, and what unblocks migration:

### 1. `builder_canvas.rs` (~864 lines) — *Tier-3, primitive seam*

The WYSIWYG canvas: forwards a live `BuilderDocument` tree as
`host_children`, paints grid-cell overlays, paints the
palette-drag ghost rectangle at the live cursor position, paints
the selection bbox outline + 8-handle resize ring whenever
`selection-rect` is present, and dispatches per-tool gizmo paint
(move / rotate / scale).

**What the DSL can't express:** real-time geometry painted at
sub-frame coordinates. The selection bbox is a `(x, y, w, h)`
quadruple updated every pointer-move; the palette-drag ghost is
positioned at the cursor minus a hot-spot offset. The DSL has no
way to read the live cursor or live geometry inside a render
walk.

**What unblocks it:** the `prism.builder-host` primitive (Wave 11.4,
registered) materialises the imperative body. Once its handler
forwards pointer events + the host can paint the
selection-bbox / palette-ghost overlays from its own retained
state, `shell.builder-canvas` becomes a thin `.prui` wrapper.

### 2. `command_palette.rs` (~315 lines) — *Tier-2*

The Ctrl+Shift+P fuzzy-find palette. Renders a 480px modal card
with a query input row (via `text_input_node` →
`UiNode::TextInput`) and a result list. Selection state lives on
the host (`AppState::command_palette_*`); the palette captures
modal focus when open.

**What the DSL can't express:** the `prism.text-input` primitive's
keystroke handling. The DSL `<input>` element today is just the
retained widget — typing, IME, selection, paste, Enter/Esc
commit, arrow keys — all live in the primitive's Rust body. Until
that body lands, `text_input_node(...)` is the only way to wire
real input.

**What unblocks it:** the full `prism.text-input` body (the
"keystroke handling, focus management, IME, multi-line buffer"
bundle the plan calls out at §11.4 / §10.3). The palette's modal
focus capture also wants `prism.focus-trap` from the same primitive
batch.

### 3. `code_editor.rs` (~315 lines) — *Tier-3, primitive seam*

Renders a monospaced column of lines with a left gutter for line
numbers and a status strip. Reads a `lines` JSON array
(`[{ number, text, fold-state? }, …]`), `language`, and
`cursor-{line,column}`.

**What the DSL can't express:** the per-line span emission that a
real code editor wants — token-coloured spans within a single
text run, cursor positioned at the right glyph offset, soft-wrap
state, fold ranges. The DSL `<text>` emits one
`Node::Text` per element; emitting one text node per token would
balloon the tree.

**What unblocks it:** the `prism.text-buffer` primitive (Wave
11.4, registered). When its body lands — retained buffer +
highlight spans + cursor — the wrapper shrinks to `.prui` that
composes `prism.text-buffer` with a gutter container and a
status strip.

### 4. `nav_graph.rs` (~266 lines) — *Tier-3, primitive seam*

The script / code-as-graph visualisation. Paints nodes and edges
on a 2D plane; supports pan, zoom, and per-node drag — all
computed during the render walk against viewport coordinates.

**What the DSL can't express:** custom 2D paint. The DSL emits
retained-mode `Node::Container` / `Node::Text` trees laid out by
Taffy. A node-graph wants direct draw-command emission (lines
between two computed points, parabola edges, hit-test against
those edges).

**What unblocks it:** the `prism.canvas-paint` primitive (Wave
11.4, registered). Its body owns the raw `RenderCommand` stream
for the bounded canvas region; the `.prui` wrapper carries the
viewport sizing + toolbar.

### 5. `dock_workspace.rs` (~392 lines) — *Tier-2, recursion gap*

Decodes the active `DockNode` JSON tree and recursively emits
nested split containers (with `Sizing::Percent` from the ratio)
and tab-group leaves (lowering through `shell.dock-panel`). Pure
recursion over a serialisable tree.

**What the DSL can't express:** typed recursion. `<for>` is
flat — there's no `<self/>` element to recurse into the
component's own body with a different sub-tree. The runtime
processes the tree's variants (`Split` vs. `TabGroup`) by
matching in Rust; the DSL would need either a recursive
`<self/>` tag with bound props, or a worklist iteration shape,
or a `<match>` element over a discriminated union.

**What unblocks it:** a `<self/>` or recursive-component sugar
that materialises the current component for a sub-prop. Open
design question — listed under "future grammar additions" in
the plan. A thinner unblock: lifting the recursion into a
host-side flatten pass that emits a typed array of
`{ kind, depth, pane-id, ratio }` rows the DSL can render with a
flat `for=`.

### 6. `dock_panel.rs` (~217 lines) — *Tier-2, ready when host pre-resolves*

Hosts one dock panel: optional tab bar (dispatched via
`ctx.lower_as("shell.dock-tab-bar", …)` if `tabs` is non-empty),
then a body that adopts the inner subtree as `host_children` or
— when no body is authored — *routes* to a content tag computed
from `panel-id` via the Rust lookup `PanelKind::tag_for(id) ->
&'static str`.

**What the DSL can't express today:** the `panel-id → content-tag`
mapping itself. `PanelKind::tag_for` is a Rust function; PRUI has
no way to call it. `<dispatch component="{expr}"/>` already exists
at the resolver, so the dispatch *mechanic* is in place — the
gap is plumbing the resolved tag into scope as a binding.

**What unblocks it:** the host pre-resolves the content tag and
binds it as a prop on the block. `dock-panel.prui` then becomes:

```prui
<container direction="column" data:role="dock-panel">
  <shell.dock-tab-bar if="{tabs.length > 0}" tabs="{tabs}"/>
  <container if="{has-host-children}"><host-children/></container>
  <dispatch else-if="{content-component}"
            component="{content-component}"
            id="{id}::content"/>
</container>
```

— roughly 15 lines of `.prui` replacing 217 lines of Rust. The
host computes `content-component` from `panel-id` once and passes
it through `props`.

### 7. `transform_editor.rs` (~354 lines) — *Tier-2, ready when drag-scrub lands*

Position / Rotation / Scale / Anchor row stack. Driven by a
declarative `ROW_SPECS` table (rows × fields × axis colour); each
field flows through `super::chrome::drag_number_field_node` for
live drag-scrub gestures.

**What the DSL can't express:** the drag-scrub gesture itself —
press-and-drag the value pill to nudge the bound prop, with
shift-modifier acceleration, min/max clamp, and live commit
through `set_node_prop`. Mid-drag state lives in
`AppState::field_focus`; the DSL `<input>` widget knows nothing
about it.

**What unblocks it:** the `prism.drag-scrub` primitive body
(Wave 11.4, registered; Wave 2.2 wired arrow-key nudges to
the existing helper but the drag side still lives in `chrome`).
When the primitive owns the press/move/release routing, the DSL
authors:

```prui
<container for="row in row-specs">
  <text>{row.label}</text>
  <prism.drag-scrub for="field in row.fields"
      value="{field.value}" min="{field.min}" max="{field.max}"
      label="{field.axis-label}" label-color="{field.axis-color}"
      on:commit="set {field.value-prop} = {value}"/>
</container>
```

— roughly a 30-line `.prui` file replacing 354 lines of Rust.

---

## Summary table

| File | Lines | Tier | Imperative blocker |
|---|---|---|---|
| `builder_canvas.rs` | 864 | 3 | Real-time selection/ghost geometry painted at sub-frame coordinates |
| `transform_editor.rs` | 354 | 2 | Drag-scrub gesture state (waits on `prism.drag-scrub` body) |
| `dock_workspace.rs` | 392 | 2 | Typed recursion over `DockNode`'s discriminated union |
| `code_editor.rs` | 315 | 3 | Per-line/token highlight spans + cursor geometry |
| `command_palette.rs` | 315 | 2 | `text_input_node` keystroke handling (waits on `prism.text-input` body) |
| `nav_graph.rs` | 266 | 3 | Custom 2D paint of nodes + edges |
| `dock_panel.rs` | 217 | 2 | Host needs to pre-resolve `panel-id → content-component` and bind it as a prop (mechanic via `<dispatch component="{expr}"/>` exists) |

Tier-3 (3 files) is *structurally* imperative — the primitives that
own the imperative body are already registered; the `.prui`
wrappers will be one-screen files once each primitive's body
lands. Tier-2 (4 files) is *waiting on grammar* — drag-scrub
needs the primitive body, two depend on recursion / dispatch
sugar that has a designed but not implemented shape.

No remaining Rust component is *intrinsically* outside the DSL's
expressive ceiling. Every blocker is either a primitive whose
imperative half is in flight, or a small grammar addition with a
known shape (`<self/>`, `<dispatch tag="{expr}"/>`,
`prism.drag-scrub` body).
