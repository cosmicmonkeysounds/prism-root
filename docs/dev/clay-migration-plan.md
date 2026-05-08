# Clay Migration Plan

> Migrating Prism's UI layer off [Slint](https://github.com/slint-ui/slint)
> onto [Clay](https://github.com/nicbarker/clay) (with Rust bindings),
> wrapped in a Prism-native declarative DSL — `prism-ui` — so every
> concept the framework already speaks (signals, properties, facets,
> connections, design tokens, manifests) is a first-class citizen of
> the UI layer, with no impedance mismatch and no second render path
> for SSR.

## ⚠ Pivot 2026-05-04 — Taffy, not Clay

After Phase 1 stood up the vendored `clay-layout` fork end-to-end
(real `cc`-built `clay.h`, FFI live, sanity test green), we hit
clippy hostility on the upstream Rust binding's tests and were
reminded that the binding is *young* (~2 KLOC, 0.4.0, single
maintainer) while we already pull in **Taffy 0.7** in the workspace
(used by `prism-builder` for the CSS Grid / Flex / Block layout pass
that powers the editor today).

**Decision:** swap Clay for Taffy. Everything else in this plan —
the `prism-ui` DSL, the retained `Surface`, the render-command
contract, the HTML lowering, the Luau authoring surface, the
phasing, the Slint tear-out list — stays exactly as written. The
only thing that changes is *which engine sits behind
[`compute()`](#51-layout--clay-vendored-fork)*.

### Why Taffy beats Clay for us

1. **Already a workspace dep.** Builder uses `taffy::TaffyTree` to
   resolve the page grid + per-node `LayoutMode`. Adopting it for the
   runtime collapses two layout engines into one — `prism-builder`
   and `prism-ui-runtime` agree on sizing semantics, percentages,
   min/max, grid placement, and flex distribution by construction.
2. **Pure Rust.** No `cc` build, no `clay.h` vendoring, no FFI
   lifetimes, no clippy-on-upstream friction, no Windows C++ branch
   in `build.rs`. WASM is a `--target wasm32-unknown-unknown` away;
   no JS-side memory management.
3. **CSS Grid is real.** Clay does flex + scroll containers. Taffy
   does flex + grid + block, all CSS-spec'd. The builder's existing
   page-grid story (ADR-003) carries straight through; Clay would
   have made us hand-roll a grid layer on top.
4. **MIT-licensed already.** Same licence destination as the
   migration plan locks for Phase 5 (`MIT OR Apache-2.0`). No vendor
   step, no fork, no upstream-velocity risk — Taffy is shipped by
   the same folks who maintain large Rust UI ecosystems and is
   actively developed.
5. **Mature text-measure callback.** Taffy's measure-function shape
   (`Fn(known_dimensions, available_space) -> Size<f32>`) drops
   `cosmic-text` straight in. Clay's measure callback works too —
   but Taffy's is the more obviously documented one.
6. **Render commands stay ours.** Clay's value-add was the
   ready-made `Vec<RenderCommand>` stream. Taffy outputs
   *rectangles*, not draw calls — but our `Vec<RenderCommand>` was
   always going to be ours anyway (we own the HTML lowering, the
   z-order, the hint pass-throughs). Generating commands from
   Taffy's per-node `Layout` is a small visitor pass.

### Trade-offs we accept

- **No "render commands for free".** We walk the resolved Taffy
  tree and emit `RenderCommand` entries ourselves. Cost: ~one
  small visitor file. Already paid in part: the Phase-1 hand-rolled
  `compute()` already does this against our own intermediate tree.
- **No native scroll/clip primitives.** Taffy doesn't track scroll
  offsets — that's a Prism concern anyway (we want them in the
  `Surface` for invalidation), so this is barely a regression.
- **No built-in hit testing.** We add a small AABB walk over the
  cached layout — same shape Clay's `pointer_over` would have
  needed wrapping for our event model.

### What gets ripped out

- `vendor/clay-layout/` — gone. The fork served its purpose proving
  the FFI shape; now redundant.
- `clay-layout` workspace member + `prism-ui-runtime` dep — gone.
- `prism-ui-runtime::tests::vendored_clay_binding_is_live` — gone;
  replaced by a `taffy_layout_pass_runs` sanity test.
- Workspace `Cargo.toml` `vendor/clay-layout` entry — gone.
- Build-side `cc` dependency — gone.

### What stays exactly the same

- `Node` / `ContainerProps` / `TextProps` / `Sizing` / `Padding` /
  `Color` / `CornerRadius` / `Viewport` — the typed value-tree shape
  is layout-engine-agnostic. (This was the explicit Phase 1 design
  goal.)
- `RenderCommand` — backend-neutral, stays.
- `Surface` retained-mode contract — stays. Dirty bit, invalidation
  triggers, `commands()` chokepoint — all unchanged.
- `prism-builder::ui_runtime` translator — stays. It produces the
  same `Node` tree.
- The `prism_ui_runtime::luau` module — stays. Luau scripts don't
  see the layout engine.
- The HTML lowering — stays.
- The phasing (Phase 0–6) — stays.
- §11 Luau authoring contract — stays.
- The `prism-ui` DSL design — stays.

### Naming

The plan filename and the doc title say "Clay". We keep them: the
*plan* is the migration off Slint and onto a CSS-derived layout
engine wrapped in a Prism-native DSL. Whether the engine is called
Clay or Taffy is a leaf detail compared to the rest of the
architecture being described. A future cleanup pass renames
`docs/dev/clay-migration-plan.md` → `docs/dev/ui-migration-plan.md`
and search-and-replaces "Clay" → "Taffy" throughout (or just "the
layout engine"). Until then: read every "Clay" below as "Taffy",
read every "Phase 1 ships Clay" as "Phase 1 ships Taffy", and the
contract is unchanged.

---

**Status:** Phase 0 accepted 2026-05-04. Supersedes
`docs/dev/slint-migration-plan.md` at Phase 5 cutover.
**Owner:** TBD
**Created:** 2026-05-04
**Last updated:** 2026-05-04

## Decisions locked at Phase 0 (2026-05-04)

1. **Licence:** workspace relicences to **`MIT OR Apache-2.0`** at
   Phase 5 cutover (when the last Slint dep is removed). New crates
   (`prism-ui-runtime`, `prism-ui-build`) ship under that dual licence
   from day one to mark the boundary; existing crates stay on
   `GPL-3.0-or-later` until cutover.
2. **Clay binding:** **fork `clay-layout`** into
   `vendor/clay-layout/` and pin from the workspace `Cargo.toml`. The
   upstream binding is small enough that drift is cheap; forking lets
   us evolve the FFI surface alongside `prism-ui-runtime` without
   waiting on upstream review, and means the build doesn't break if
   upstream stalls.
3. **DSL flavour:** **HTMX-inspired** (see §4). Tag-element syntax
   instead of curly-brace blocks, behaviour declared as attributes
   (`on:click`, `bind:value`, `if`, `for`), facets and signals are
   first-class attribute namespaces. Goal: a non-programmer who knows
   HTML can read and edit `.prism-ui` files; an engineer loses no
   expressiveness vs. the strawman in the prior draft.
4. **Retained-mode layout (added 2026-05-04).** The runtime does **not**
   recompute the Clay layout every frame. Layout is cached in a
   [`Surface`](#52-retained-mode-surface) and recomputed only when
   something invalidates it — tree mutation, viewport resize, scroll
   offset change, an animation tick, a theme/token swap, or an explicit
   `Surface::invalidate()`. Backends (`femtovg`, `web`, `html`) pull a
   `&[RenderCommand]` slice every frame and only pay the layout cost on a
   dirty cycle. This is the explicit retained-mode contract; immediate-
   mode wrappers are a non-goal.
5. **Luau-authorable components (added 2026-05-04).** Clay-backed
   components (the `Node` tree, the `Surface`, signal hooks) are a
   first-class Luau surface. Luau code must be able to **author** new
   components programmatically, **edit** an existing component's props
   / children / styles, **generate** components from data
   (template-style emission), and **hook into** them by attaching
   signal handlers and observing lifecycle. This preserves today's
   `LuauComponent` capability into the post-Slint world. See §11 for
   the API contract.

---

## 0. Why Clay (this time, for real)

The original Phase 0 of the Slint plan rejected Clay on three grounds:
two renderers to maintain, no declarative surface, and Puck-replacement
greenfield risk. Six months in, Slint has solved all three — but at a
cost we didn't price in:

1. **Slint is not Prism.** Every Prism concept (signals, facets,
   connections, properties with type-aware editors, manifests, design
   tokens, the cascade) has to be lowered into Slint's vocabulary
   (callbacks, properties, `ComponentFactory`) and then *raised back
   out* on the codegen / inspector / property-panel side. The
   `BuilderDocument` ↔ `.slint source` ↔ `slint-interpreter` triangle
   is the load-bearing seam, and every feature touches all three legs.
2. **Two render paths still exist.** `Component::render_slint` and
   `Component::render_html` are independent walkers — adding a block
   means writing it twice, keeping them in sync, and accepting that
   the SSR output and the live UI can drift. The relay's HTML walker
   is a parallel universe to the shell's Slint walker.
3. **The interpreter is fragile.** ADR-007 already disabled live-preview
   on the full `app.slint` because of an `slint-interpreter` `VRc`
   panic in `Flickable`'s `ChangeTracker`. `ComponentContainer` /
   `component-factory` don't work in interpreted mode at all. Every
   "live edit" feature has to route around interpreter limitations.
4. **GPL-3.0-or-later.** The workspace is GPL because Slint's
   royalty-free terms required it. Dropping Slint frees the licence
   choice (Apache-2.0 / MIT / dual is back on the table — Clay is
   zlib).
5. **No declarative surface that *is Prism's*.** Slint's `.slint` DSL
   is excellent — but it's a foreign language we have to map onto
   Prism Syntax, generate stubs for, and parse with a separate
   `SlintSyntaxProvider`. We already own a DSL pipeline
   (`prism-core::language::syntax` + `language::codegen`) — we should
   use it.

[Clay](https://github.com/nicbarker/clay) is a single-header,
immediate-mode, retained-layout C library with a tiny Rust binding
([`clay-layout`](https://crates.io/crates/clay-layout)). It does
**only** layout + render-command emission — no windowing, no renderer,
no input. That sounds like a regression vs. Slint's full stack, but it
is exactly the property we want: **Clay computes a layout tree per
frame and hands us a `Vec<RenderCommand>` we can lower to anything** —
femtovg/wgpu native, canvas2d/WebGL web, native iOS/Android textures,
**or HTML strings**. One layout engine, N renderers, including a
deterministic HTML/CSS lowering that obsoletes the parallel SSR walker.

The trade-off: **we have to build the declarative layer ourselves**.
That is the entire point. Prism already has half of it (Prism Syntax
scanner, codegen `SourceBuilder`, the `Component` trait, signals,
facets, field factories) — we are not starting from zero, we are
unifying what's already there into a single DSL whose lowering is the
new UI runtime.

## 1. Non-goals

- Not changing the data layer. Loro CRDT remains source of truth.
- Not rewriting `prism-daemon`. Its emscripten Luau build is independent.
- Not replacing Luau. Luau stays the user-script extension language;
  the new DSL is for *UI structure*, Luau remains for *behaviour*. The
  two compose through the same `PrismContext` already in flight.
- Not removing the `BuilderDocument` tree. It becomes the *runtime
  representation* of the new DSL, not a parallel construct.
- Not shipping an in-house font shaper or SVG library. Clay's renderer
  contract lets us reuse `cosmic-text` (or femtovg's text path) on
  native and the browser's text engine on web.
- Not abandoning visual / e2e harnesses. They re-target the new
  runtime; the `BuiltinScene` enum and `TestScript` API survive.

## 2. Scope inventory (what has to move)

| Surface | Today (Slint) | Tomorrow (Clay + `prism-ui`) |
|---|---|---|
| `prism-shell/ui/app.slint` (~2500 LOC, 7 components) | hand-written `.slint` | hand-written `.prism-ui` (new DSL), compiled by `prism-ui-build` codegen |
| `prism-shell/build.rs` `slint_build::compile` | build-time Slint compile | build-time `prism_ui_build::compile` → generated Rust module |
| `slint::include_modules!()` | inlines generated types | `prism_ui::include_modules!()` (or direct `include!`) |
| `prism-shell` runtime | Slint event loop + femtovg | `prism-ui-runtime` ⇒ winit + femtovg native, winit-web + WebGL2 web |
| `prism-builder::slint_source::SlintEmitter` | emits `.slint` text | `prism_ui_emit::Emitter` emits `.prism-ui` text on the same `SourceBuilder` |
| `prism-builder::render::compile_slint_source` (interpreter) | runtime `slint-interpreter` | runtime `prism_ui::interpret` — parses DSL → `BuilderDocument` → render-command stream (no separate compile step) |
| `Component::render_slint` + `render_html` | two walkers | one `Component::layout(props, children) -> Vec<UiNode>` — Clay layout pass + HTML lowering both consume the same node stream |
| `prism-relay` SSR | `render_document_html` | `prism_ui::lower_html(BuilderDocument)` — same render-command stream lowered to HTML/CSS instead of a draw list |
| `prism-core::language::slint_lang` | `.slint` LanguageContribution | `prism-core::language::prism_ui` — built on the existing `Scanner` / `SyntaxProvider` |
| `prism-cli codegen` | emits `core.d.luau` + `builder.d.luau` + `signals.d.luau` | adds `prism-ui.d.luau` (component schemas) + `prism-ui-types.rs` (Rust-side) — same pipeline, more emitters |
| Live-reload | `SLINT_LIVE_PREVIEW=1` + interpreter | hot-swap by re-parsing DSL on file change → diff `BuilderDocument` → re-layout. No special compile path because runtime parsing *is* the path. |
| Visual harness `BuiltinScene` | renders Slint scenes | renders DSL scenes; same enum, new runtime |
| E2E `TestScript` | callback-level into Slint | callback-level into runtime's input dispatcher |
| Licence | GPL-3.0-or-later | revisit; Clay is zlib, no copyleft pressure |

Everything in `prism-core` (foundation, identity, language, kernel,
manifest, vault, design tokens) is unaffected — those subtrees were
never Slint-coupled. `prism-daemon`, `prism-cli`, the luau integration
(`luau_component.rs`, `script_loader.rs`), the relay's auth /
federation / mailbox modules: all unchanged.

## 3. The core idea — one tree, two lowerings, no walkers

Today:

```
BuilderDocument ──► render_slint ──► .slint text ──► slint-interpreter ──► live UI
                └─► render_html  ──► HTML string ──► relay
```

Two walkers, two output formats, two parsers, two registries
(`ComponentRegistry` + `HtmlRegistry`).

Proposed:

```
.prism-ui source ─┬─► parse (Prism Syntax Scanner) ──► BuilderDocument (typed tree)
                  │
BuilderDocument ──► layout pass (Clay)               ──► Vec<RenderCommand>
                                                            │
                                              ┌─────────────┼─────────────┐
                                              ▼             ▼             ▼
                                         femtovg       canvas2d/WebGL    HTML/CSS
                                         (native)        (web)          (relay SSR)
```

One source format. One typed tree. One layout pass. N renderers — one
of which is "render commands → HTML string". The `Component` trait
collapses to a single method that returns layout primitives; the
shell, the web build, and the relay are three lowerings of the same
output.

This is only viable if the layout primitives are expressive enough to
round-trip to HTML/CSS without information loss. Clay's primitives
(rectangles with padding/margin/gap/flex/sizing/border/corner-radius/
text/scroll/image) **do** map cleanly to HTML+CSS because Clay's
layout model is itself CSS-derived (flexbox + box-model). This is the
load-bearing claim of the whole plan. §7 stress-tests it.

## 4. The DSL — `prism-ui`

A small, declarative, Prism-native surface. Goals: every Prism concept
is a keyword or attribute namespace; every element in the DSL is a
`Component`; signals, connections, and facets are syntax, not
annotations. **HTMX-inspired:** the markup looks like HTML, behaviour
lives in attributes, control flow lives in attributes, data binding
lives in attributes — so a non-programmer who knows HTML can read,
copy, and edit `.prism-ui` files; an engineer can drop into Luau via
the same attribute surface without leaving the file.

### 4.1 Design principles

1. **HTML-shaped.** Tags, closing tags, attributes — exactly what
   anyone who's edited a webpage already knows.
2. **One concept per attribute namespace.** `on:` for signals, `bind:`
   for two-way binding, `if`/`for`/`else` for control flow, `fct:` for
   facet bindings, `sig:` for declared signals, `style:` for tokens,
   `aria:`/`data:` pass through to the HTML lowering verbatim.
3. **No hidden magic.** Every attribute is a direct lowering to a
   first-class type the runtime already has (`Connection`, `FacetDef`,
   `FieldSpec`, `ActionKind`). Reading the DSL is reading the runtime.
4. **Inline expressions are Prism Syntax.** `{...}` interpolations are
   parsed by `prism-core::language::syntax::Scanner` — the same
   expression dialect that powers facets, signal payloads, and Luau
   bridge calls. No new expression language to learn.
5. **No closing-tag drift.** Self-closing where natural (`<input/>`,
   `<spacer/>`); explicit `</tag>` where children matter. We accept the
   verbosity for the readability win.

### 4.2 Strawman syntax (HTMX-flavoured)

```prism-ui
<component name="Card" props="title: text, badge: text?">
  <signals names="clicked, hovered"/>

  <container
    layout="flow"
    direction="column"
    gap="{tokens.spacing.md}"
    style:background="{tokens.colors.surface}"
    style:radius="{tokens.radius.md}"
    on:click="emit clicked"
    on:hover="emit hovered with { x: event.x, y: event.y }"
  >
    <heading level="3">{title}</heading>

    <pill tone="accent" if="{badge}">{badge}</pill>

    <facet name="items" from="resource:posts" limit="5">
      <link href="{item.url}">{item.title}</link>
    </facet>
  </container>
</component>
```

Compact "single-tag" form for trivial cases:

```prism-ui
<button label="Save" on:click="emit save" style:tone="primary"/>
```

### 4.3 Attribute namespaces

| Namespace | Purpose | Lowers to |
|---|---|---|
| *(bare attr)* | Component property — `title="Hello"`, `level="3"` | `props: serde_json::Value` entry |
| `{expr}` | Inline expression — `title="{user.name}"` | Prism Syntax expression bound at render |
| `on:<event>` | Signal connection — `on:click="emit save"` | `Connection { signal, action: ActionKind::* }` |
| `bind:<prop>` | Two-way binding — `bind:value="form.email"` | sugared signal pair (read + write-back) |
| `if`, `else-if`, `else`, `for` | Control flow — `if="{badge}"`, `for="post in posts"` | conditional / repeated subtree in `BuilderDocument` |
| `style:<token>` | Token-resolved style — `style:background="{tokens.colors.surface}"` | resolved at parse time, baked into render commands; falls back to runtime lookup for dynamic values |
| `fct:<name>` | Facet binding on a component — `fct:items="resource:posts"` | `FacetDef` attached to node |
| `sig:<name>` | Declared signal on a component (shorthand for a `<signals>` entry) | `SignalDef` registered on the component |
| `aria:*`, `data:*` | Pass-through to HTML lowering | dropped on native, emitted verbatim in HTML |
| `class`, `id` | CSS-style addressing for the inspector + HTML | both |

### 4.4 Action grammar (right-hand side of `on:*`)

The value of an `on:event` attribute is a tiny single-line expression
language, parsed by the same Scanner:

```
emit <signal>                                   → ActionKind::EmitSignal
emit <signal> with { k: v, ... }                → EmitSignal { payload }
set <target>.<prop> = <expr>                    → SetProperty
toggle <target>.visible                         → ToggleVisibility
navigate to <page>                              → NavigateTo
play <animation> on <target>                    → PlayAnimation
luau { ... }                                    → ActionKind::Custom { handler }
```

Multiple actions chain with `;`. Anything more complex drops into
`luau { ... }` and is handed to the existing `exec_custom_handlers`
queue verbatim.

### 4.5 Why this beats the curly-brace strawman

- **Lower the floor.** "It's HTML with extra attributes" is a true
  one-sentence explanation. The curly-brace version asked the reader
  to learn a custom block syntax first.
- **No ceiling penalty.** Every piece of expressiveness from the
  strawman survives — facets, signals, control flow, design tokens,
  Luau handlers. The lowerings are 1:1.
- **HTML lowering is nearly free.** `<container layout="flow">` with
  `style:*` attributes lowers to `<div class="prism-container">` with
  `style="display:flex; ..."` — same shape, with prefixed CSS custom
  properties for tokens. `prism-relay`'s SSR becomes "stringify the
  attribute namespaces with the right rules" rather than a parallel
  walker.
- **Editor tooling for free.** Existing HTML-aware tools (syntax
  highlighters, formatters, LSP clients) work out of the box on the
  raw markup; we layer Prism-aware diagnostics + completions on top
  via `SyntaxProvider`, the same way `slint_lang` does today.
- **Familiar to LLMs and humans.** Both populations have seen
  thousands of HTML examples. Onboarding cost ≈ 0.

### 4.6 Compile-time vs. runtime

- `<component name="...">` declarations in
  `packages/prism-shell/ui/*.prism-ui` are compiled at build time by
  `prism-ui-build` into a Rust module — same role
  `slint::include_modules!()` plays today.
- `<component>` declarations authored at *runtime* (the builder panel
  emitting a `BuilderDocument` and serialising back to `.prism-ui`)
  go through the same parser, no second compile path. Live edits
  re-parse the file → diff against current `BuilderDocument` →
  re-layout. ADR-007's interpreter-fragility class of bugs simply
  doesn't exist because there is no interpreter — there is only a
  parser whose output is a typed tree.

### 4.7 Parser: built on Prism Syntax

The DSL is parsed by extending `prism-core::language::syntax::Scanner`
with a small grammar layer. **No regex, no hand-rolled string indexing**
— per the project's standing rule that all parsers must go through
Prism Syntax. The grammar lives in
`prism-core::language::prism_ui::grammar`, exposes a
`SyntaxProvider` for the editor (diagnostics, completions, hover) the
same way `slint_lang` does today, and produces a `prism_ui::Ast`
consumed by:

- the `prism-ui-build` build-script codegen (compile-time Rust modules),
- the runtime `interpret(source) -> BuilderDocument` path (live editing),
- the `BuilderSyntaxProvider` already wired into the CodeEditor panel.

### 4.8 Codegen — one pipeline, multiple emitters

`prism-cli codegen` already walks the `ComponentRegistry` to produce
`signals.d.luau`. The new pipeline reuses
`prism_core::language::codegen::SourceBuilder` and adds emitters:

- `core.d.luau` *(unchanged)*
- `builder.d.luau` *(unchanged)*
- `signals.d.luau` *(unchanged)*
- **`prism-ui.d.luau`** — per-component prop/signal/facet types so
  Luau handlers see strongly-typed component instances.
- **`prism-ui-types.rs`** — Rust types for hand-written components
  (props/signals/facets as concrete structs). Replaces today's
  `slint::include_modules!()` role.

Compile-time `.prism-ui` files in `packages/prism-shell/ui/` are read
by `prism-ui-build` (a build-script crate sibling to
`prism-core::language::codegen`), parsed, and lowered to a generated
`mod ui { ... }` Rust module — same ergonomics as
`slint::include_modules!()` but without the foreign DSL.

## 5. The runtime — `prism-ui-runtime`

A new crate, `packages/prism-ui-runtime`, replaces Slint's role inside
`prism-shell`. Three layers:

### 5.1 Layout — Clay (vendored fork)

The Clay binding ships **vendored at `vendor/clay-layout/`** as a
workspace member, forked from the upstream
[`clay-layout`](https://crates.io/crates/clay-layout) Rust binding at
a pinned commit. Rationale: the binding is small (~2 KLOC of FFI +
helpers), the FFI surface needs to evolve in lockstep with
`prism-ui-runtime`'s layout-tree builder, and forking removes
upstream-velocity risk. Upstream-fix backports go through normal
`git` cherry-picks; out-bound contributions go upstream as PRs.

Each frame:

1. Build the Clay `BuilderDocument` → `clay::Element` tree from the
   typed `BuilderDocument` (`prism-builder` already owns this struct;
   it's the existing source-of-truth).
2. Run `clay::layout(viewport)` → `Vec<RenderCommand>`.
3. Hand the command stream to the active backend (native / web /
   HTML).

The layout pass is pure, deterministic, and trivially testable — no
event loop, no GPU, no IO. This unlocks property-test-driven layout
verification we don't have today.

### 5.2 Retained-mode `Surface`

Layout is cached. The runtime exposes a `Surface` value that owns:

- the typed `Node` tree (the `UiTree` root),
- the current `Viewport`,
- the most recently computed `Vec<RenderCommand>`,
- a single `dirty: bool` flag.

The public API is small and deliberately retained-mode:

```rust
let mut surface = Surface::new(tree, viewport);

surface.set_tree(new_tree);          // marks dirty
surface.with_tree_mut(|root| ...);   // marks dirty
surface.set_viewport(new_viewport);  // marks dirty iff value changed
surface.invalidate();                // explicit hook (scroll / animation)

let cmds: &[RenderCommand] = surface.commands();
//   ^ recomputes layout iff dirty, otherwise returns the cache
```

**Why retained:** for an editor-class UI, the vast majority of frames
are visually identical to the previous frame — re-running Clay's
layout pass on every vsync is wasted CPU. A dirty bit, flipped only
by the small set of events that *can* change layout, lets the shell
stay at 60+ fps with the layout pass running well under 60 Hz.

**Invalidation triggers (canonical set):**

- tree mutation (`set_tree`, `with_tree_mut`, builder applies a CRDT op)
- viewport / window resize
- scroll offset change inside any scroll container
- animation tick (interpolated layout values advanced)
- theme / design-token change (cascade output diverges)
- explicit `invalidate()` for cases not covered above (e.g. font load,
  image decoded, locale change)

Backends do **not** call `compute()` directly — they always go through
`Surface::commands()`, which is the single chokepoint where dirtiness
is checked.

### 5.3 Backends

- **`backends/femtovg`** *(native)* — winit window + femtovg renderer.
  We pull both directly (Slint's bundled versions go away with Slint).
  Text via `cosmic-text` shaped into femtovg paths.
- **`backends/web`** *(wasm)* — winit's web target into a `<canvas>`,
  rendered by either femtovg's WebGL2 path *or* a thin canvas2d
  adapter; we'll prototype both during Phase 1 and pick on the basis of
  text quality + binary size.
- **`backends/html`** *(SSR)* — pure function `lower(Vec<RenderCommand>)
  -> String`. No event loop, no async. Lives in
  `prism-ui-runtime::html` and is the only thing `prism-relay` needs to
  call. The relay drops `Component::render_html` and the parallel
  `HtmlRegistry`; SSR is a side-effect-free render of the same tree the
  shell renders.

### 5.4 Input + event loop

Winit on every target; input events become `prism_ui::Event` values
fed into the existing signal dispatch (`dispatch_signal`,
`exec_custom_handlers`). The shell's `Store<AppState>` is unchanged —
only the binding from "store update" to "redraw" is rewritten (today
it's a Slint property; tomorrow it's a `request_redraw()` call).

## 6. Concept-by-concept mapping

| Prism concept | Slint today | `prism-ui` tomorrow |
|---|---|---|
| **Component** | `impl Component { render_slint, render_html }` | `impl Component { layout(ctx, props, children) -> UiTree }`; one method, two lowerings (Clay + HTML) |
| **Property** | Slint property + `FieldSpec` separately | Single declaration in DSL header; `FieldSpec` derived |
| **Signal** | `SignalDef` + Slint callback wired manually | `signals { ... }` section in DSL; codegen wires both |
| **Connection** | `Connection { source, signal, target, action }` | identical struct; DSL sugar `on click -> ...` lowers to it |
| **Facet** | `FacetDef` + manual template wiring | `facet items: list(...)` keyword; lowers to `FacetDef` |
| **Design token** | `tokens.*` referenced via Slint globals | `tokens.*` resolved at parse time, baked into render commands |
| **Layout** | Slint flow / GridLayout / HorizontalLayout | Clay flex / grid / scroll containers |
| **Style cascade** | Re-implemented in `panels::properties` | Same logic, now *also* consumed by HTML lowering — one cascade, two outputs |
| **Live edit** | `slint-interpreter` re-compile per change | re-parse `.prism-ui` file → derive `BuilderDocument` → re-layout. No second compiler. |
| **HTML SSR** | `Component::render_html` walker | `lower_html(commands)` lowering of the same tree the shell renders |
| **Source-first (ADR-006)** | `.slint` source canonical, `BuilderDocument` derived | `.prism-ui` source canonical, `BuilderDocument` derived (same shape, same `SourceMap` markers, same edit operations) |
| **Luau scripting** | `PrismContext` + `LuauComponent` impls `Block` for both render targets | Same `PrismContext`; `LuauComponent` returns a `UiTree` (single render contract) |
| **Live preview** | disabled for `app.slint` (ADR-007) | first-class — runtime *is* the parser, no second engine |

## 7. Risks & open questions

1. **HTML/CSS round-trip fidelity.** Clay's flexbox model maps to CSS
   flexbox cleanly, but text shaping, sub-pixel positioning, and shadow
   geometry have to match between the native renderer and the browser
   renderer for visual-regression tests to pass against both. Mitigation:
   accept *semantic* equivalence in HTML (same DOM structure, CSS-
   equivalent layout) rather than pixel-equivalence with native; visual
   tests run only on native targets.
2. **Text rendering.** Slint's femtovg+rustybuzz pipeline is one of the
   reasons Slint feels good. We'll reuse `cosmic-text` directly (Slint
   itself uses it underneath) so this is a re-export rather than a
   rewrite — but the integration is non-trivial and is its own Phase.
3. **Clay maturity.** Clay is young; its Rust binding is younger.
   Mitigation: **vendored fork at `vendor/clay-layout/`** (decision
   locked at Phase 0), pinned to a known-good upstream commit;
   surface area we depend on stays small (layout in, commands out);
   fixes flow back upstream via PRs without blocking us.
4. **DSL design space.** A new DSL is a permanent design decision. We
   will lock the v0 surface to the smallest grammar that covers
   `app.slint` today (≈30 keywords) and grow it conservatively.
5. **Migration cliff.** Slint and `prism-ui` cannot coexist in a single
   `Shell`. Phase 2 builds `prism-ui` to feature-parity *behind a
   feature flag* on a side branch; Phase 3 is the cutover.
6. **Visual / e2e harnesses.** Both depend on the runtime's input and
   redraw model. Phase 4 ports the harnesses to the new runtime;
   Phase 3 cannot land without this.
7. **Mobile.** Slint had iOS/Android targets nearly for free via
   winit. Clay has no mobile renderer. We get winit-on-mobile but have
   to bring our own GL/Metal context. Punt to Phase 6.

## 8. Phase roadmap

Each phase ends with `cargo test --workspace` green and the `prism`
CLI surface unchanged.

### Phase 0 — Decision & ADRs (this doc + 3 ADRs)
- ADR-008: replace Slint with Clay + custom DSL (this plan, condensed)
- ADR-009: `prism-ui` DSL grammar v0
- ADR-010: render-command → HTML lowering contract
- Licence revisit: drop GPL requirement; pick Apache-2.0 or dual.

### Phase 1 — Runtime spike
- New crate `packages/prism-ui-runtime` with `clay-layout` dep.
- Three backends behind cargo features: `femtovg`, `web`, `html`.
- Hand-build a 5-element scene in Rust (no DSL yet) and render it to
  all three backends. Snapshot tests on the HTML backend.
- **In progress (2026-05-04):** vendored fork of `clay-layout` 0.4.0
  imported under `vendor/clay-layout/` (renderer integrations
  stripped; `cc`-built `clay.h` linked into the crate). Sanity test
  in `prism-ui-runtime` exercises `Clay::new` / `begin` / `end` to
  prove the FFI is live. `prism-builder::ui_runtime` translates
  `BuilderDocument` → `prism_ui_runtime::layout::Node` (containers,
  text, spacers; `FlowProps` → flex direction / gap / padding; the
  `StyleProperties` cascade resolves background / radius / color /
  font_size). The `luau` feature on `prism-ui-runtime` ships the
  Phase-1 starter for §11 — `ui.container` / `ui.text` / `ui.spacer`
  / `ui.surface` constructors plus a `LuaSurface:edit(id, fn)`
  hook for the §11 "edit" capability.
- **Update 2026-05-07:** vendor/clay-layout deleted; `prism-ui-runtime`
  now drives `taffy::TaffyTree` end-to-end (per the 2026-05-04 pivot).
  Added the Phase-1 snapshot acceptance test
  (`tests::five_element_scene_html_snapshot`) — the canonical
  five-element scene runs through `compute` → `backends::html::lower`
  and is captured by `insta`. Re-run with `cargo insta review` after
  intentional layout / lowering changes. Native (`femtovg`) and web
  backends remain stubs until the renderer phase.
- **Update 2026-05-08 (input + rAF):** Phase-4 input dispatch hook
  + rAF loop wired. `event::EventHandler` is a
  `Box<dyn FnMut(&Event, &mut Surface) + 'static>` the host installs
  on `backends::femtovg::run` / `backends::web::mount`. Input
  translation lives in `backends::input::translate` (private), shared
  by both backends — pointer / wheel / mouse-button / modifier state /
  keyboard / IME-commit / resize / focus all map onto the existing
  `event::Event` vocabulary. The shell's hook into
  `prism_builder::signal::dispatch_signal` is now a one-liner closure
  the host writes; `prism-ui-runtime` itself stays signal-agnostic.
  The web backend gained a real frame loop: it now runs winit's wasm
  event loop via `EventLoopExtWebSys::spawn_app`, attaching to an
  existing `<canvas>` through `WindowAttributesExtWebSys::with_canvas`
  and rendering on every `RedrawRequested` (winit-on-wasm schedules
  these via `requestAnimationFrame`, so the rAF cadence is automatic
  — no hand-rolled rAF closure dance). Both backends honour the
  retained-mode contract: a frame only requests a repaint when
  `Surface::is_dirty()`.
- **Update 2026-05-08 (initial backends):** native + web backends landed.
  `paint::draw<R: Renderer>` is the shared render-command → femtovg
  paint-list translator (rectangles with rounded corners, borders,
  scissors, hint pass-through, text via cosmic-text). `text::TextSystem`
  owns a `cosmic_text::FontSystem` + `SwashCache` plus a
  `(CacheKey, RGBA)`-keyed glyph image cache; mask glyphs are baked
  with the fill colour pre-multiplied (no femtovg "tint alpha image"
  paint, so different colours can't share a texture), color glyphs
  (emoji) blit straight through. `backends::femtovg::run(surface)`
  drives a winit `ApplicationHandler` over a glutin GL context and a
  `Canvas<OpenGl>` (resize / scale-factor / pointer / focus events
  flow into the surface — `prism_builder` signal dispatch wires up
  in Phase 4). `backends::web::mount(canvas_id, &mut surface)` grabs
  an existing `<canvas>` and renders one frame through the same
  `paint::draw` over `OpenGl::new_from_html_canvas`; the rAF/event
  loop on the wasm side will be driven by the shell's
  `#[wasm_bindgen(start)]` once Phase 4 retargets it. Workspace pulls
  `winit 0.30 / femtovg 0.23 / glutin 0.32 / glutin-winit 0.5 /
  raw-window-handle 0.6 / cosmic-text 0.12` directly (matched to
  Slint's transitive versions to keep the toolchain stable through
  cutover). Both `cargo build -p prism-ui-runtime --features
  femtovg,html,web` (host) and `cargo build -p prism-ui-runtime
  --target wasm32-unknown-unknown --no-default-features --features
  web` are green; clippy is `-D warnings`-clean on both.

### Phase 2 — DSL + parser + codegen
- New crate `packages/prism-ui-build` (compile-time codegen).
- Extend `prism-core::language` with `prism_ui` grammar +
  `SyntaxProvider`.
- `prism-cli codegen` learns the `prism-ui.d.luau` and
  `prism-ui-types.rs` emitters.
- Round-trip test: `.prism-ui` → AST → Rust → render commands → HTML →
  parse HTML → assert structural equivalence.
- **Update 2026-05-08:** `prism-ui-build` now wires through
  `prism_core::language::prism_ui::parse`. `compile_source` and
  `compile(path)` produce a generated Rust module exposing `SOURCE:
  &str` (round-trip preserved for the runtime interpret path) and
  `COMPONENT_NAMES: &[&str]` (every `<component name="...">`
  declaration in document order). Recoverable parse errors abort the
  build via `CompileError::Parse { count, first }` so build-script
  failures point at the offending line/column.
- **Update 2026-05-08 (AST → Node lowering):** `prism-ui-runtime`
  gains an `interpret` module — `interpret(source) -> Result<Vec<Node>,
  Vec<ParseError>>` plus `lower_document(&Document) -> Vec<Node>`.
  Handles the v0 surface end-to-end: `<container>` with `direction` /
  `gap` / `padding[-{side}]` / `width` / `height` / `style:background`
  / `style:radius`, `<text>` / `<heading level="N">` with `font-size`
  / `style:color`, `<spacer width height/>`, plus hex `#rgb` / `#rrggbb`
  / `#rrggbbaa` colours and `grow` / `fit` / `<px>` sizings. The
  generated module from `prism-ui-build` now also emits a `pub fn
  nodes() -> Vec<prism_ui_runtime::layout::Node>` runtime helper that
  re-parses `SOURCE` through `interpret` (build-time validation
  guarantees the parse succeeds). Round-trip test
  `interpret::tests::five_element_source_round_trips_to_layout` walks
  source → AST → `Node` → render commands and asserts the same five
  commands the hand-built scene produces. Phase 2's "AST → Rust →
  render commands" leg is closed; the HTML half of the round trip is
  already covered by the Phase-1 snapshot test.
- **Update 2026-05-08 (codegen fan-out):** `prism codegen luau-types`
  learned the `prism-ui.d.luau` emitter (plan §4.8). New module
  `prism_ui_runtime::luau_types` ships hand-rolled `export type`
  stubs for every value type the runtime exposes — `Color`,
  `CornerRadius`, `Padding`, `Sizing`, `Direction`, `Viewport`,
  `TextProps`, `ContainerProps`, `Node`, `Rect`, `RenderCommand` —
  curated leaf-first to match the rest of the workspace's stub
  files. `prism-cli` writes it next to `core.d.luau` /
  `builder.d.luau` / `signals.d.luau`, so Luau handlers authoring
  `ui.container({...})` get strongly-typed completions and hover.
  Three runtime tests + one CLI integration test cover the registry
  shape + filesystem write.

### Phase 3 — Component model unification
- Collapse `Component::render_slint` + `render_html` into
  `Component::layout`. Migrate the 16 `register_builtins` components
  (heading, text, link, image, container, form, input, button, card,
  code, divider, spacer, columns, list, table, tabs).
- Drop `HtmlRegistry`; `prism-relay` calls
  `prism_ui_runtime::html::lower_document` against the unified tree.
- Source-first machinery (ADR-006) re-pointed at `.prism-ui` markers.
- **Update 2026-05-08 (Block::lower_ui — first three blocks migrated):**
  built-in lowering moved from a string-dispatch branch in
  `ui_runtime::translate_node` to a `Component::lower_ui` method every
  block implements. New module `prism-builder/src/ui_lower.rs` owns
  the shared primitives every block reuses — `LowerCtx` (registry +
  cascade carrier with `lower` / `lower_children` / `default_container`),
  `container_props_from`, `parse_color`, `text_node`, `spacer_node`,
  `sizing_from_dimension`. `Component::lower_ui` (default) and
  `Block::lower_ui` (forwarded by the blanket impl) both default to
  the generic container fallback, so structural blocks (containers,
  columns, lists, …) inherit correct behaviour without writing any
  code. `TextBlock` and `SpacerBlock` override `lower_ui` to produce
  `UiNode::Text` / `UiNode::Spacer` directly — `TextBlock` honours the
  `level` prop (`paragraph` / `h1`–`h6`) for default font size via
  the existing `level_font_size` helper that `render_slint` /
  `render_html` already share, and reads body from the schema's `body`
  field with a fallback to legacy `text`/`content` props for old
  fixtures. `ui_runtime` exposes parallel registry-aware (`*_with_registry`)
  and registry-less APIs during the parallel-build period; the
  registry-less path falls through to the container default for every
  node, the registry-aware path is what Phase 5 promotes to canonical.
  No duplication: cascade resolution, colour parsing, sizing, and
  container-prop construction live exactly once each in `ui_lower`,
  and every block lowering is a 5–15 line override that calls the
  helpers. Remaining 12 builtins (image, container[explicit], form,
  input, button, code, divider, columns, list, table, tabs, accordion)
  fall through the default and migrate one-at-a-time as their bespoke
  layout vocabulary is needed.
- **Update 2026-05-08 (declarative container lowering + 6 more
  builtins):** the `Block::lower_ui` migration moves forward without
  duplicating cascade/sizing/colour logic per block. New helpers in
  `crate::ui_lower`:
  - `LowerCtx::container_with(node, style, customize)` — builds the
    same `UiNode::Container` `default_container` would, then hands the
    `ContainerProps` to a closure so the block tweaks only the fields
    it actually owns. Cascade resolution, flow-props lowering, child
    recursion live exactly once.
  - `LowerCtx::synthetic_container(node, style, children, customize)`
    — same shape for blocks that synthesise their own children
    (button-with-label, code-with-text) instead of walking
    `node.children`.
  Six builtins migrated through these helpers — all 5–15 line impls,
  no per-block boilerplate: `ContainerBlock` (schema spacing/padding
  → flow gap/padding), `ColumnsBlock` (direction=Row + gap),
  `ListBlock` (gap from item_spacing), `DividerBlock` (1px stroke
  with cascade-aware bg), `CodeBlock` (synthetic text child + bg /
  radius / padding defaults matching `render_slint`), `ButtonBlock`
  (centred label + fixed 36px height). Eight Phase-3 acceptance
  tests in `ui_runtime::tests` cover the new lowerings end-to-end.
  Remaining 6 builtins (image, form, input, table, tabs, accordion)
  fall through to the registry-aware default-container path; image
  lands when the runtime grows an `Image` primitive (plan §5.3).
  *(Resolved 2026-05-08 — see "all 13 builtins migrated" update below.)*
- **Update 2026-05-08 (5 more builtins → all non-image migrated):**
  `FormBlock`, `InputBlock`, `TableBlock`, `TabsBlock`, `AccordionBlock`
  picked up `Block::lower_ui` overrides. Two new helpers in
  `ui_lower.rs` keep the composite blocks duplication-free:
  - `bare_container(id, children, customize)` — builds a
    `UiNode::Container` *without* a builder `Node` driving it.
    Composite blocks (table headers, tab strips, accordion bars,
    input field rows) synthesise nested layout using this single
    constructor; `customize: FnOnce(&mut ContainerProps)` is the only
    way fields move off `ContainerProps::default()`.
  - `uniform_radius(r)` — equal-on-all-corners `CornerRadius`.
    `CodeBlock` / `ButtonBlock` / `TableBlock` / `AccordionBlock` /
    `InputBlock` all route through it, replacing the prior 4-field
    struct-literal duplication.
  Test registry collapsed from per-block `register_block` calls into
  a `register_all! { "id" => Type, ... }` macro — adding a new block
  to the test stack is one line. Six new acceptance tests cover the
  new lowerings end-to-end (form/input × 2/table/tabs/accordion);
  `lower_single` + `expect_container` test helpers eliminate the
  per-test fixture / pattern-match boilerplate, so each test reads as
  pure assertions on layout shape. **Phase-3 scoreboard:** 12 of 13
  non-image builtins now have dedicated `lower_ui` impls (text,
  spacer, container, columns, list, divider, code, button, form,
  input, table, tabs, accordion). Image is the lone holdout, blocked
  on a runtime `Image` primitive (plan §5.3). 25 `ui_runtime` tests +
  403 total `prism-builder` tests green; clippy `-D warnings` clean.
- **Update 2026-05-08 (all 13 builtins migrated — `Image` primitive
  lands):** `prism-ui-runtime` grows a `Node::Image` variant alongside
  the existing `Container` / `Text` / `Spacer` — same retained-mode
  shape, same Taffy-leaf integration, same `RenderCommand::Image`
  pass-through. The `Image` variant carries a stable id, a `source`
  string the host renderer resolves (URL / `/asset/<hash>` / file
  path), `Sizing` width/height (so `grow` / `fit` / fixed-pixel
  images flow through Taffy identically to containers), and a
  `CornerRadius` that round-trips into the render command (the html
  backend now emits `<img>` with `border-radius`; the femtovg backend
  remains a stub until the asset decoder phase). `RenderCommand::Image`
  picks up a matching `radius` field — same shape `Rectangle` already
  has, no new vocabulary. New `ui_lower::image_node(id, source, style,
  width, height)` helper sits next to `text_node` / `spacer_node` /
  `bare_container` so any block that needs an image is one call.
  `ImageBlock::lower_ui` is 12 lines: resolve `src` through
  `AssetSource::from_prop().to_html_src()` (the *same* path the SSR
  walker uses — single source of truth for asset URL resolution, no
  parallel logic), then call `image_node` with `Sizing::Grow` to match
  `render_slint`'s `width: parent.width; height: parent.height`. The
  `full_registry` test stack adds `"image" => ImageBlock` — one line.
  Two new acceptance tests (`image_block_lowers_to_image_node_with_url_source`,
  `image_block_propagates_cascade_radius`) cover URL pass-through and
  cascade-radius propagation. **Phase-3 scoreboard:** 13/13 non-prefab
  builtins now have dedicated `lower_ui` impls. 27 `ui_runtime` tests
  green; full workspace `cargo test` green; clippy `-D warnings`
  clean.
- **Update 2026-05-08 (semantic HTML lowering; relay cuts over to
  the unified pipeline):** Phase 5's "drop `Component::render_html`
  + `HtmlRegistry`" now has its replacement landed and live in the
  relay.
  - **`Semantic` value type** in `prism-ui-runtime/src/layout/mod.rs`
    — `{ tag, class, aria_label, role, attrs }`, all skip-if-empty
    so existing JSON round-trips unchanged. Builder methods
    (`Semantic::tag(t).with_class(c).with_attr(k, v)`) keep the
    block-side declaration to one or two lines per migration.
    Drops `Copy` from `ContainerProps`/`TextProps` (they now own
    `Semantic` strings); construction sites updated mechanically.
  - **`prism-ui-runtime::backends::semantic_html`** — pure
    tree-walking emitter, no layout pass, no render commands.
    Dispatches by `Semantic::tag` first, then per-variant default
    (`<div>` / `<span>` / `<p>` / `<img>`). Default text tag
    buckets by font-size so blocks that haven't migrated yet still
    produce something sensible. When an explicit semantic tag is
    set the walker omits inline layout styles — semantic HTML
    defers chrome to stylesheets, the pixel-faithful
    `backends::html` lowering remains the place for inline-style
    layout reproduction. 7 dedicated tests cover the walker.
  - **Builder helpers** in `ui_lower.rs`:
    - `with_semantic(node, hint)` attaches a `Semantic` to any
      Container / Text / Image variant in one call. `Spacer`
      ignores it (no semantic anchor — layout-only).
    - Existing `text_node` / `bare_container` / `image_node`
      compose with `with_semantic` so each block's `lower_ui`
      override stays a 5-15 line declaration of layout +
      semantic, no duplication of cascade / sizing / colour /
      child-recursion logic.
  - **`prism-builder::ui_runtime::lower_semantic_html` /
    `lower_semantic_html_with_registry`** — `BuilderDocument` →
    semantic HTML in one call. Registry-aware version is what the
    relay calls.
  - **Block migrations (5 of 13):** TextBlock declares
    `<h1>`-`<h6>` / `<p>` based on the `level` prop, and wraps the
    content in `<a href="…">` when `href` is set (single source of
    truth for the level→tag table is `level_to_html_tag` in
    `starter.rs`). ImageBlock pulls `alt` onto the hint. FormBlock
    declares `<form method="…" action="…">`. ContainerBlock
    declares `<section>`. ListBlock declares `<ul>` / `<ol>`.
    Remaining 8 blocks (table, tabs, accordion, code, divider,
    button, input, columns) keep the per-variant default `<div>`
    until they declare their own hint — the relay output is
    correct for them, just less expressive.
  - **Relay cutover** in `prism-relay/src/ssr_routes.rs` — the
    portal-detail handler now calls
    `lower_semantic_html_with_registry(doc, &state.registry)`
    instead of `render_document_html(doc, &state.html_registry,
    &state.tokens)`. Single SSR pipeline, single chokepoint.
    Integration test (`portal_detail_renders_welcome_page`)
    asserts the new output's structural shape — `<section>` from
    container, `<h1>Welcome…</h1>` from heading text,
    `<p><a href="/portals">…</a></p>` from anchored text — all
    green. `render_error_response` removed (semantic walker is
    infallible).
  - **What still has to happen for the full Phase 5 cutover:**
    migrate the remaining 8 block `lower_ui` impls to declare
    their semantic shape; then delete `Component::render_html`,
    the `HtmlBlock` trait, `HtmlRegistry`, `register_html_widgets`,
    `register_html_builtins`, `html_starter.rs`, and the SSR-only
    half of `core_widget.rs` / `render.rs`. The relay no longer
    calls any of that code, so deletion is mechanical from here.

- **Update 2026-05-08 (Image lands in §11 Luau surface; declarative
  variant dispatch):** the new `Node::Image` variant is now first-class
  in the `prism_ui_runtime::luau` bindings — `ui.image({ id, source,
  width, height, radius })` constructor takes the same value-shape
  `Node::Image` carries on the Rust side, so authoring an image from
  Luau is one call. The `LuaNode:kind()` method now delegates to a
  single `Node::kind() -> &'static str` helper on the runtime side
  instead of an inline match — adding a Node variant updates one
  arm, not three (Luau, debug, future hint dispatch). One new test
  (`ui_image_constructs_an_image_node`) covers the constructor +
  `kind()` round-trip. 25 luau tests + 405 builder tests + clippy
  `-D warnings` clean.

- **Update 2026-05-08 (semantic catalogue closed; void-tag walker;
  declarative semantic via `props.semantic`):** the remaining 8 block
  `lower_ui` impls now declare their own SSR semantic shape, finishing
  the per-block migration started by the 2026-05-08 "5 of 13" update
  above. **No new dispatcher**, **no per-block walker**, and **no
  duplicate `with_semantic` wrapping** — every block sets
  `props.semantic = Semantic::tag("…")` directly inside its
  `customize` closure on `synthetic_container` / `bare_container`,
  reusing the *same* container constructor that already owns sizing,
  cascade resolution, and child recursion. Adding a semantic shape is
  one line.
  - **Void-tag handling** in `prism-ui-runtime::backends::semantic_html`
    — a single `VOID_TAGS` constant lists the spec's full set; the
    walker checks it once. Containers tagged `<hr>` / `<input>` etc.
    emit self-closing markup with their `Semantic` attrs and *drop*
    their children (which were layout-only anyway). Single source of
    truth: a new void-tag block doesn't have to teach the walker
    about itself, it just sets `props.semantic = Semantic::tag(…)`.
  - **Block migrations (8 of 13):**
    - `ButtonBlock` — `<button type=…>` paired, or `<a href… role="button">`
      when the `href` prop is set; `disabled="disabled"` propagates.
    - `DividerBlock` — `<hr>` (void; no children, no inline styles).
    - `CodeBlock` — outer `<pre>` wrapping inner `<code>` with a
      `class="language-{lang}"` matching `render_html`. The inner
      `<code>` semantic is set via `with_semantic` on the `text_node`
      since it's a child rather than the outer container.
    - `InputBlock` — outer `<label>` (when a label prop is set),
      inner `<input>` (void) carrying `type` / `name` / `placeholder`
      / `value` / `required` attrs declared once on
      `Semantic::tag("input").with_attr(…)`.
    - `TableBlock` — outer `<table>`, caption text → `<caption>`,
      header strip → `<thead>` containing a `<tr>` of `<th>` cells.
      Each layer adds exactly one `props.semantic` line.
    - `TabsBlock` — strip → `role="tablist"`, each pill →
      `<button role="tab" aria-selected=…>`, panel host →
      `role="tabpanel"`. The first pill / panel get `aria-selected="true"`.
    - `AccordionBlock` — `<details open="open"?>` with `<summary>`
      header; content host stays a default `<div>` so children's
      own semantics flow through unchanged.
    - `ColumnsBlock` — left as default `<div>` flex container; no
      semantic anchor in the HTML spec for "side-by-side columns",
      and adding `role="group"` here is noisier than letting the
      `<div>` defer to the parent's stylesheet.
  - **One acceptance test** — `lower_semantic_html_covers_the_phase3_block_catalog`
    in `prism-builder/src/ui_runtime.rs` walks a single document
    that touches every migrated block and asserts the SSR markup
    `render_html` would have produced (`<button>`, `<a href>`,
    `<hr>`, `<pre><code class="language-…">`, `<label><input>`,
    `role="tablist"`/`tab`/`tabpanel`, `<details open><summary>`,
    `<table><caption><thead><tr><th>`). Two new dedicated tests in
    `backends::semantic_html::tests` cover the void-tag walker
    (`void_tag_container_emits_self_closing_and_drops_children`,
    `input_void_tag_carries_attrs`).
  - **Phase-5 deletion punch list (now mechanically empty):**
    every relay-facing block has a semantic-HTML shape declared in
    its `lower_ui` impl, so the parallel `Component::render_html`,
    `HtmlBlock`, `HtmlRegistry`, `register_html_widgets`,
    `register_html_builtins`, `html_starter.rs`, and the SSR-only
    half of `core_widget.rs` / `render.rs` are no longer load-bearing
    on any caller. Phase 5 deletes them in one mechanical pass
    (next session); the relay already calls
    `lower_semantic_html_with_registry` and nothing else.

- **Update 2026-05-08 (unified pipeline entry point):** added
  `prism_builder::ui_runtime::render_commands(doc, viewport) ->
  Vec<RenderCommand>` and `lower_html(doc, viewport) -> String` —
  the single chokepoint Phase 5 will rewire the relay through.
  Composition: `document_to_ui_tree` → `prism_ui_runtime::layout::compute`
  → `backends::html::lower`. `prism-builder` now opts into
  `prism-ui-runtime`'s `html` feature so the SSR lowering is callable
  without dragging femtovg/cosmic-text into the relay's dep graph.
  Three round-trip tests cover empty, non-empty, and color-bearing
  documents end-to-end. `HtmlRegistry` and `Component::render_html`
  stay in place during the parallel-build period; the next steps
  migrate built-in blocks one at a time so their layout vocabulary
  flows through `ui_runtime` instead of two parallel walkers.

- **Update 2026-05-08 (Phase-5 punch list — parallel HTML pipeline
  deleted):** the SSR-only half of `prism-builder` is gone in one
  mechanical pass, since every relay-facing block already declared
  its semantic shape via `lower_ui` and the relay had already cut
  over to `lower_semantic_html_with_registry`. Net code removed:
  - **Files:** `html_block.rs`, `html_starter.rs` deleted outright.
  - **Trait surface:** `HtmlBlock`, `HtmlRegistry`, `HtmlRenderContext`,
    `Block::render_html`, the `Block`→`HtmlBlock` blanket impl,
    `PrefabHtmlBlock`, `FacetHtmlBlock`, `LuauComponent::render_html`,
    `Modifier::wrap_html`, `CoreWidgetBlock::render_html`,
    `render_template_html`, `register_html_builtins`,
    `register_core_html_widgets`, `render_document_html` /
    `render_document_html_with_data`. The `prism-luau-derive`
    `#[derive(PrismBlock)]` macro no longer emits a `render_html`
    arm — one render method per derive, end-to-end.
  - **API simplification (smart-pattern reduction):**
    `register_block(reg, html_reg, block)` collapsed to
    `register_block(reg, block)`; `register_builtins(reg, html_reg)`
    collapsed to `register_builtins(reg)`. The `starter::register_builtins`
    body is now a `reg!("id", BlockType)` macro table — adding a
    new builtin is one row, no per-call boilerplate. ~30 callsites
    across `prism-shell`, `prism-relay`, `prism-cli`, and
    `prism-builder` lost their second argument; `prism-relay::AppState`
    lost its `html_registry` field entirely.
  - **What stays:** `html.rs` (the `Html` buffer + `escape_attr` /
    `escape_text`) is preserved — `prism-relay` and the
    `prism-luau-derive` macro use these helpers for chrome
    composition that has nothing to do with the deleted walker.
  - **Verification:** `cargo test --workspace` (default features)
    and `cargo check --all-targets --features prism-builder/luau`
    both green; clippy `-D warnings` clean. Phase-5 punch list is
    now empty for the SSR half; the Slint half (`render_slint`,
    `SlintEmitter`, `slint_source.rs`, the `interpreter` feature
    chain) stays load-bearing until the shell port lands.

### Phase 4 — Shell port
- Translate `ui/app.slint` (now ~4400 lines, 13 components) into
  `ui/app.prism-ui`. Behind a `prism-ui` cargo feature on `prism-shell`
  for a parallel-build period; native bin and wasm bin both build both
  variants until parity is reached.
- Re-target visual harness + `BuiltinScene` + `TestScript` /
  `E2eDriver` onto the new runtime.
- **Update 2026-05-08 (sibling registry + first shell primitive):**
  the shell's bespoke chrome components (`IconButton`,
  `ToolbarSeparator`, `MenuBarRow`, `NavButton`, `SectionHeader`,
  `DragNumberField`, `TransformEditor`, `FieldEditor`, `InspectorRow`,
  `Toast`, `DocsContent`, `AppCard`, `AppWindow`) flow through the
  *same* `Block::lower_ui` path the 13 content builtins use. New
  module `prism-shell/src/components/` holds:
  - `ShellComponentRegistry` — newtype around
    `prism_builder::ComponentRegistry` so shell primitives stay out
    of the user's document component palette while reusing the
    cascade machinery, the `Block`/`Component` blanket impl, the
    `LowerCtx` helpers (`synthetic_container`, `image_node`,
    `uniform_radius`, `with_semantic`), and the
    `lower_semantic_html_with_registry` SSR walker. Adding a shell
    primitive is one row in `register_shell_builtins`'s `reg!` table
    — the same shape as `prism_builder::starter::register_builtins`.
    `as_component_registry()` exposes the inner `&ComponentRegistry`
    so existing relay/lowering signatures plug in unchanged.
  - `IconButton` — first primitive landed. `shell.icon-button`,
    schema = `icon`/`enabled`/`tooltip-text`/`help-id`, signals add
    `hover-start { help_id, x, y }` + `hover-end` to the 12 universal
    common signals. `lower_ui` produces a 28×28 `Container` with
    6px radius and a centred 16×16 `Image` glyph; SSR semantic is
    `<button>` with `aria-label` derived from `tooltip-text` and
    `disabled` propagated from `enabled=false`. Resting visual
    state only — hover/pressed visual transitions land alongside
    the runtime-level state vocabulary (see runtime gaps below).
    8 unit tests cover lowering shape, ARIA propagation, disabled
    attrs, schema, and registry registration.
- **Update 2026-05-08 (two more chrome primitives — gap-free):**
  `ToolbarSeparator` and `SectionHeader` landed via the same
  `Block::lower_ui` recipe. Neither needed a runtime extension:
  - **`shell.toolbar-separator`** — 1×20 fixed-size container with a
    translucent foreground background. Empty schema, no signals.
    SSR semantic is `role="separator"` + `aria-orientation="vertical"`
    (the WAI-ARIA pattern for toolbar dividers — no native HTML
    element exists for this shape). 1 unit test.
  - **`shell.section-header`** — 36px collapsible-section header
    with a *prop-conditional* chevron glyph (`chevron-left` when
    collapsed, `chevron-down` when expanded). The conditional
    resolves at lower-time, so no runtime state vocabulary is
    needed; this is the canonical pattern for prop-driven visuals
    until the runtime grows hover/pressed states. Synthesises a
    column with two children — a row (chevron + label, plus a
    "(default)" badge text node when collapsed) and a 1px hairline.
    SSR semantic is `<header>` with `data-section` + (when
    collapsed) `data-collapsed="true"` attrs so CSS / scripted
    hosts can target sections without the runtime knowing about
    them. Signals: `section-toggled { section_id }` + 12 universals.
    5 unit tests.
  - **Pattern reinforced — zero duplication:** both primitives reuse
    `synthetic_container` / `bare_container` / `image_node` /
    `text_node` / `parse_color` from `ui_lower`. Neither hand-rolls
    a `UiNode::Container { … }` literal. Adding a primitive that
    doesn't need a runtime extension is mechanically a 30-90 line
    file plus one row in the `reg!` table.
  - **Phase-4 chrome scoreboard:** 3 of 13 primitives migrated
    (`shell.icon-button`, `shell.toolbar-separator`,
    `shell.section-header`). Remaining 10 group by their blocker:
    - **Hover-state-blocked:** `NavButton`, IconButton's hover bg,
      MenuBarRow items, Tab pills.
    - **Slot-blocked:** `AppWindow`, `MenuBarRow` (parent injects
      children).
    - **Control-flow-blocked:** any primitive with a `<for>` loop —
      MenuBarRow, TabBar.
    - **TextInput-blocked:** `DragNumberField`, `FieldEditor`,
      `InspectorRow` editable fields.
    - **Overlay-blocked:** `Toast`, command-palette, help tooltip.
    Hover-state vocabulary unblocks the largest cluster, so it's
    the next runtime extension to land.
- **Update 2026-05-08 (hover-state vocabulary lands; NavButton + IconButton
  hover wired):** the runtime grew a sparse, declarative hover-overrides
  vocabulary — *no* state machine, *no* parallel render path, and the
  retained-mode dirty-bit contract is preserved.
  - **`HoverOverrides` struct** in `prism-ui-runtime/src/layout/mod.rs`:
    `{ background: Option<Color>, radius: Option<CornerRadius> }`. Sparse
    so unset fields fall through to the resting paint state. `is_empty`
    helper for the dirty-bit short-circuit.
  - **`ContainerProps::hover: Option<HoverOverrides>`** — declarative
    field, serde-skipped when empty so existing JSON round-trips
    unchanged. SSR backends ignore it (hover is paint-only). Text
    hover deferred until a primitive demands it.
  - **`compute_with_hover(tree, viewport, hovered_id: Option<&str>)`**
    — sibling of `compute`, threads the hovered id through
    `build_taffy_subtree`. The Container arm folds overrides into the
    `NodeContext` only when `id == hovered_id`. Layout box model is
    untouched — hover affects paint only, by design.
  - **`Surface::set_hovered(Option<String>)`** — host-driven hover
    state. Marks dirty *only* when the transition crosses a node that
    declares non-empty `hover` overrides (helper `node_has_hover`
    walks the tree once). Idempotent on same-id calls; transitioning
    between two non-affecting nodes never recomputes. Preserves the
    retained-mode contract: `commands()` re-runs layout iff
    `dirty == true`.
  - **Hit-testing intentionally NOT wired in this turn.** The runtime
    exposes `set_hovered(Option<String>)`; the host (input dispatcher)
    owns the hit-test for now. A `Surface::hit_test(x, y) -> Option<&str>`
    helper lands when the first concrete native input pipeline needs
    it — currently the IconButton hover state is fully expressible
    in storage and ready for the host wire-up.
  - **4 new layout tests:** `hover_overrides_swap_in_when_id_matches`,
    `hover_overrides_ignored_when_id_does_not_match`,
    `surface_set_hovered_dirty_only_when_paint_actually_changes` (covers
    enter / leave / drift / idempotent), `surface_hover_swap_round_trip`.
  - **Block migrations using the new vocabulary:**
    - `IconButton` — enabled buttons declare a translucent foreground
      hover bg (`#1f000000`); disabled buttons leave `props.hover =
      None` so they stay static under the pointer. Two new tests.
    - **`shell.nav-button` (5th primitive, hover-state-blocked → unblocked):**
      48×48 activity-bar button with a 3px accent rail on the left
      edge that paints when `selected=true`. Resting buttons declare
      a hover-bg override; selected buttons keep their accent bg
      under the pointer (no double-state). SSR semantic is
      `<button type="button">` with `aria-pressed="true"` when
      selected. Reuses `bare_container` / `image_node` / `parse_color`
      — the rail is a 3px-wide grow-height bare container, the body
      is a centred glyph in a padding-resolved bare container, the
      outer is the standard `synthetic_container`. Zero hand-rolled
      `UiNode::Container { … }` literals. 3 unit tests.
  - **Phase-4 chrome scoreboard:** **4 of 13** primitives migrated
    (`shell.icon-button`, `shell.toolbar-separator`,
    `shell.section-header`, `shell.nav-button`). Hover-state half of
    the cluster is now unblocked — `MenuBarRow` items, Tab pills, and
    every other "highlight on hover" primitive can declare their
    hover shape immediately. Remaining blockers are `<slot/>`,
    control-flow lowering, `TextInput`, and overlay z-layer.
- **Update 2026-05-08 (interactive-chrome helper extraction):** with
  IconButton + NavButton both shipping, the duplicated boilerplate at
  the bottom of every button-shaped `lower_ui` impl had clear
  shape — and the user explicitly called out smart-pattern factoring.
  Two thin helpers landed, each on the layer that already owns the
  primitive:
  - **`Semantic::button()`** (in `prism-ui-runtime/src/layout/mod.rs`)
    — pre-shapes a `<button type="button">` Semantic; chainable from
    the existing fluent builder.
  - **`Semantic::with_attr_if(bool, k, v)`** — folds the
    `if cond { semantic = semantic.with_attr(k, v); }` two-liner into
    one chainable call. Used for `aria-pressed` / `disabled` /
    future per-state attrs.
  - **`Semantic::with_aria_label_opt(Option<&str>)`** — same shape
    for `Option`-shaped tooltip / help-id sources.
  - **`hover_bg(&str) -> Option<HoverOverrides>`** (in
    `prism-builder/src/ui_lower.rs`) — one-line constructor for the
    "hover swaps the background only" pattern. Returns `None` when
    the colour string fails to parse, so callers assign
    `props.hover = hover_bg(...)` unconditionally and the parse-error
    path stays paint-free.
  - **Both refactored callers shrank by ~10 lines each** with no
    behaviour change beyond IconButton picking up `type="button"` on
    its `<button>` (correct/desired — matches NavButton). All 808
    tests stay green; 6 new helper tests cover the conditional
    branches and parse failures.
  - **Why this is the right level of abstraction:** the helpers
    *compose* with the existing fluent builder rather than wrapping
    it in a new struct. No new "ButtonShape"/"ChromeBuilder" type,
    no DI registration, no parallel lowering path — just two more
    nodes on the existing `Semantic` builder graph and one ergonomic
    constructor on the existing `ui_lower` namespace. Future
    button-shaped primitives (Tab pill, MenuBarRow item, future
    StatusBar buttons) get the same density without picking up new
    vocabulary.
- **Update 2026-05-08 (overlay z-layer lands; Toast migrated):** the
  third runtime gap from the punch list filled. The smart-pattern
  shape mirrors `HoverOverrides` exactly — sparse vocabulary, retained-
  mode dirty-bit contract preserved, *zero* per-primitive z-order
  knowledge in the runtime, and Block lowering signature unchanged.
  - **`Overlay { id, anchor, node }`** in
    `prism-ui-runtime/src/layout/mod.rs`. An overlay is **just an
    existing `Node` plus an anchor**. No new layout vocabulary; the
    overlay's subtree lays out via the same `build_taffy_subtree` /
    `compute_layout` / `emit_commands` path as the main tree. Hover
    overrides, semantic SSR hints, and child recursion all flow
    through unchanged.
  - **`OverlayAnchor` enum** with three variants closed at design
    time: `Corner { corner, inset }` (toasts, fixed badges),
    `Point { x, y }` (help tooltip pinned to pointer / context menu),
    `Center { offset_y }` (command palette, modal dialogs). Anchors
    requiring a query against the *main* tree's resolved layout
    (e.g. "anchored to the rect of node `nav-button-3`") are
    deliberately deferred — that case lands behind a new variant
    only when the first concrete primitive demands it.
  - **`Surface::push_overlay` / `remove_overlay` / `clear_overlays` /
    `set_overlays`** — same dirty-bit shape as `set_tree`. `push_overlay`
    replaces in place when the id collides, so a host can call it on
    every state tick (toast list updated, palette query changed)
    without juggling z-order. Hover hit-testing extends naturally:
    `set_hovered` walks both the main tree *and* every overlay subtree
    when deciding whether to flip the dirty bit.
  - **`compute_full(tree, overlays, viewport, hovered_id)`** — the
    main tree paints first, then each overlay in declaration order
    with its anchor-resolved origin folded in. Z-order = stack order;
    last `push_overlay` wins. Existing `compute` / `compute_with_hover`
    delegate, so every existing call site is unchanged.
  - **Block authoring contract unchanged.** A Toast's `lower_ui`
    produces a `Node` exactly like every other primitive. The host
    (`prism-shell`) decides whether to mount it inside the main
    tree or on the overlay stack. This separation is what makes the
    same Toast lowering reusable inside a hypothetical "notification
    list" panel — no overlay knowledge baked into the Block.
  - **`shell.toast` (6th primitive, overlay-blocked → unblocked):**
    320px-wide notification card. Kind-tinted left rail (`info` /
    `success` / `warning` / `error`) + title-and-body column.
    Reuses `bare_container` / `parse_color` / `text_node` /
    `uniform_radius` from `ui_lower` — zero hand-rolled
    `UiNode::Container { … }` literals. SSR semantic is `<aside>`
    with kind-driven `role` (`status` / `alert`) and `data-kind`
    attribute so screen readers announce errors as alerts. The
    one-line `kind_chrome` lookup table replaces a four-way branch
    inside the lowering body. 5 unit tests covering rail/column
    shape, body-omission when the prop is empty, kind→role/data
    table, schema, and the merged `dismissed` + universal-signals
    list.
  - **5 new layout tests** (`overlay_paints_after_main_tree_at_resolved_corner`,
    `overlay_anchor_center_resolves_to_viewport_centre_with_offset`,
    `overlay_anchor_point_translates_verbatim`,
    `surface_overlay_lifecycle_marks_dirty_only_when_stack_changes`,
    `surface_overlay_hover_swap_dirties_through_overlay_subtree`)
    cover anchor math, paint order, the in-place replace path, the
    no-op-on-empty `clear_overlays` short-circuit, and that the
    hover dirty-bit walks overlay subtrees too.
  - **Phase-4 chrome scoreboard:** **5 of 13** primitives migrated
    (`shell.icon-button`, `shell.toolbar-separator`,
    `shell.section-header`, `shell.nav-button`, `shell.toast`). The
    overlay z-layer cluster — Toast, command-palette, help-tooltip —
    is unblocked: the remaining two will lower with the same recipe
    (Block produces a card-shaped Node; host pushes onto the surface
    with the appropriate `OverlayAnchor::Center` or `Point`).
    Remaining blockers narrow to `<slot/>`, control-flow lowering,
    and `TextInput`.
- **Phase-4 runtime gaps to fill** (each one lands just-in-time as
  the next shell primitive demands it; the IconButton lowering above
  flagged the first):
  1. **Hover/pressed state vocabulary** on `Node` — needed for the
     *visual* half of every interactive primitive (IconButton bg
     change on hover, NavButton accent on selection, Tab pill on
     active). Touch input already flows through
     `event::EventHandler` → `prism_builder::signal::dispatch_signal`,
     so the *behavioural* half is solved; the gap is purely a
     declarative surface for "this Container's background switches
     on the `hovered` signal". Smart-pattern target: a `States`
     vocabulary on `ContainerProps` / `TextProps` whose lowering
     declares the swap, instead of authors writing imperative
     handlers.
  2. **`<slot/>` semantics** for component composition — required
     by every wrapper-shaped primitive (MenuBarRow with embedded
     tabs, SectionHeader with body, AppCard with content). Reuse
     `prism_builder::prefab::ExposedSlot` if shapes match, else
     extend the DSL grammar with one new element `<slot/>` whose
     children come from the parent invocation.
  3. **`Image::colorize`** — icon tinting (palette foreground +
     transparency variants). Add `tint: Option<Color>` to
     `Node::Image`; the femtovg backend already pre-multiplies mask
     glyphs with a colour, the same code resolves an image's tint.
  4. **`<if>` / `<else-if>` / `<else>` / `<for>` lowering** — the
     namespace exists in the AST (`AttributeNamespace::ControlFlow`),
     `interpret::lower_document` doesn't act on it yet. Land alongside
     the first primitive that needs it (likely TabBar's pill loop
     or MenuBarRow's menu list).
  5. **`TextInput`** primitive on `Node` — required by
     `DragNumberField`, `FieldEditor`, search box. cosmic-text
     already owns text editing; the runtime gap is exposing it as a
     layout-leaf with focus + IME flow.
  6. **Overlay / popup z-layer** — *landed 2026-05-08* via
     `Surface::overlays: Vec<Overlay>` + `OverlayAnchor` (Corner /
     Point / Center). `shell.toast` migrated as the first consumer.
     Command-palette and help-tooltip lower with the same recipe
     (Block → Node, host pushes onto the overlay stack with the
     appropriate anchor).

### Phase 5 — Cutover
- Delete `slint`, `slint-build`, `slint-interpreter`, `.slint` files,
  `slint_lang` module, `SlintEmitter`, `compile_slint_source`,
  `instantiate_document`, `LiveReloadingComponent`,
  `BuilderSyntaxProvider`'s Slint half, `Component::render_slint`,
  `Component::render_html`, `HtmlRegistry`.
- Update `CLAUDE.md` files. Update root licence.
- Archive `docs/dev/slint-migration-plan.md` under
  `docs/dev/archive/`.

### Phase 6 — Mobile & packaging
- iOS / Android via winit + a chosen GL/Metal context.
- `cargo-packager` packaging unchanged in shape; binary surface is
  Clay + femtovg instead of Slint.

## 9. Decision log

| Date | Decision | Rationale |
|---|---|---|
| 2026-05-04 | Draft Clay-migration plan | Slint friction (§0); SSR/UI-walker duplication; interpreter fragility (ADR-007); licence pressure |
| 2026-05-04 | Phase 0 accepted; three decisions locked | Licence → `MIT OR Apache-2.0` at cutover; Clay binding **vendored fork** at `vendor/clay-layout/`; DSL flavour **HTMX-inspired** (tag-element + attribute-namespace) — see ADR-008 |
| 2026-05-04 | Retained-mode runtime contract added (decision #4) | Editor-class UIs idle most frames; recomputing layout per vsync is wasted CPU. `Surface` caches `Vec<RenderCommand>`, dirty bit flips only on tree mutation / resize / scroll / animation tick / theme change / explicit `invalidate()`. Backends always go through `Surface::commands()`. |
| 2026-05-04 | Luau-authorable Clay components (decision #5) | Preserve today's `LuauComponent` capability into the post-Slint world: Luau must author / edit / generate / hook into Clay-backed components. Imposes value-type design on `Node` / props / `Surface` so `prism-luau-derive` can wrap them mechanically. See §11. |
| 2026-05-08 | Hover-state vocabulary lands (`HoverOverrides` + `Surface::set_hovered`) | Sparse paint-only override bundle on `ContainerProps`; dirty bit only flips when the hover transition crosses an affecting node. Unblocks every "highlight on hover" chrome primitive without a state machine or shadow render path. |
| 2026-05-08 | Interactive-chrome helper extraction (`Semantic::button` / `with_attr_if` / `with_aria_label_opt` / `hover_bg`) | IconButton + NavButton revealed the same "button-shaped Semantic + conditional ARIA + hover swap" boilerplate. Helpers compose with the existing fluent builder — no new abstraction layer, no DI/registration, just two nodes on `Semantic` and one constructor in `ui_lower`. Future button-shaped chrome primitives inherit the density automatically. |
| 2026-05-08 | Overlay z-layer lands (`Overlay` + `OverlayAnchor` + `Surface::push_overlay`); `shell.toast` migrated | Sparse anchor enum (Corner / Point / Center) covers Toast + command-palette + help-tooltip without forking the layout vocabulary — overlays are just `(Node, anchor)` pairs that flow through the same `build_taffy_subtree` / `emit_commands` pipeline as the main tree. Block lowering is unchanged: a Toast produces a Node; the host decides whether to mount it inline or on the overlay stack. Hover hit-testing extends naturally to overlay subtrees. Unblocks the third of the four chrome clusters identified in the punch list. |

## 10. Appendix — file-by-file Slint footprint to retire

(Compiled during the codebase survey on 2026-05-04 — these are the
exact files Phase 5 deletes or rewrites.)

- `packages/prism-shell/ui/app.slint` *(rewrite as `ui/app.prism-ui`)*
- `packages/prism-shell/build.rs` *(swap `slint_build` for `prism_ui_build`)*
- `packages/prism-shell/src/lib.rs` *(`slint::include_modules!()` → DSL include)*
- `packages/prism-builder/src/slint_source.rs` *(replace with `prism_ui_emit`)*
- `packages/prism-builder/src/render.rs`
  (`render_document_slint_source`, `compile_slint_source`,
  `instantiate_document`) — replaced by direct
  `BuilderDocument` → render-command stream
- `packages/prism-builder/src/live.rs` *(retain shape, retarget at
  `.prism-ui` source + new `SourceMap` markers)*
- `packages/prism-builder/src/syntax_provider.rs` *(rewire to the new
  grammar's `SyntaxProvider`)*
- `packages/prism-core/src/language/slint_lang/*` *(replaced by
  `language/prism_ui/*`)*
- `packages/prism-relay/src/*` *(stop walking
  `Component::render_html`; call `prism_ui_runtime::html::lower_*`
  instead)*
- Workspace `Cargo.toml` — drop `slint`, `slint-build`,
  `slint-interpreter`; add `vendor/clay-layout` path dep,
  `cosmic-text`, `winit`, `femtovg` direct deps.
- Root `LICENSE` + per-crate `license.workspace = true` — flip
  workspace default to `MIT OR Apache-2.0`. Add `LICENSE-MIT` +
  `LICENSE-APACHE` files at the workspace root.

## 11. Luau authoring of Clay components

**Requirement (locked 2026-05-04):** every Clay-backed component
must be a first-class Luau surface, on par with today's
`LuauComponent` impls of `prism_builder::Component`. Luau scripts
must be able to:

| Capability | What it means | Lowering |
|---|---|---|
| **Author** | Build a `Node` tree from scratch in Luau code — `ui.container { direction = "row", children = { ui.text("hi") } }` returning a tree handle | mlua-bound constructors over `Node` value types |
| **Edit** | Mutate an existing component's props, children, or styles after creation | `Surface::with_tree_mut` exposed as Luau methods on a tree handle |
| **Generate** | Emit components from data — loops over a list, template substitution from a facet binding | Luau closures invoked during the parse → `BuilderDocument` lowering, identical surface to `<for>` in the DSL |
| **Hook** | Attach signal handlers and observe lifecycle (`on_mount`, `on_signal`) without owning the renderer | `on:*` actions whose `luau { ... }` body is queued through `exec_custom_handlers` (the same path the DSL takes) |

**Design rules this imposes on the runtime:**

1. `Node`, `ContainerProps`, `TextProps`, `Sizing`, `Padding`, `Color`,
   `CornerRadius`, `Viewport`, `RenderCommand` are all `serde`-friendly
   value types — no lifetimes, no trait objects, no `Rc`/`RefCell` in
   the public API. This is what lets `prism-luau-derive` wrap them as
   `mlua::UserData` mechanically.
2. Stable string `id`s on every node. Luau handles index by id, not
   by tree-walk path, so a CRDT op or a re-render doesn't dangle a
   handle.
3. Props are typed structs, not opaque `HashMap<String, Value>`. The
   Luau bindings translate to/from Lua tables at the boundary, not
   all the way down — Rust callers stay strongly typed.
4. The DSL and Luau authoring produce the **same** typed `Node` tree.
   Luau is an alternative front-end to the same `BuilderDocument` →
   `UiTree` pipeline, never a parallel one.
5. `prism-cli codegen` emits `prism-ui.d.luau` (per §4.8) so Luau
   handlers see strongly-typed component instances — same pipeline as
   `signals.d.luau` today.

**Phasing.**

- Phase 1: Rust-side `Node` value-type design (lands with the runtime
  spike; constraints 1–3 enforced).
- Phase 2: Luau bindings (`prism_ui_runtime::luau` module +
  `prism-luau-derive` annotations on the value types) ship alongside
  the DSL parser, so the DSL and Luau front-ends arrive together.
- Phase 3: Existing `LuauComponent` impls in `prism-builder` migrate
  to the new surface during component-model unification.

## 12. Shell components via the `Block` registry

**Strategy locked 2026-05-08.** Every bespoke shell-chrome component
(IconButton, ToolbarSeparator, MenuBarRow, NavButton, …) is a
`prism_builder::Block` impl registered into a sibling
`ShellComponentRegistry`. The registry is a newtype around
`ComponentRegistry`, so the *trait surface, lowering helpers, cascade,
and SSR walker stay singular*; the only thing the newtype buys is a
separate namespace so document palettes do not surface chrome.

**Why this beats a parallel walker / parallel trait:**

- **One `lower_ui` per primitive.** Native rendering and SSR both
  consume the same lowering — a shell component declared this way
  is a first-class citizen of the unified Taffy/SSR pipeline, not a
  shadow vocabulary the renderer only half understands.
- **Helpers live exactly once.** Cascade resolution, `synthetic_container`,
  `image_node`, `uniform_radius`, `with_semantic`, colour parsing —
  every shell primitive uses the same `LowerCtx` helpers the 13
  content builtins do. Adding a primitive is 5–15 lines.
- **Authoring stays declarative.** A shell component is referenced
  from `.prism-ui` source by tag name (`<shell.icon-button …/>`),
  resolved through `lower_semantic_html_with_registry` /
  `render_commands_with_registry` exactly like a content block. No
  per-primitive walker arm, no string-dispatch branch.
- **Smart pattern, not a new abstraction.** The newtype delegates to
  `ComponentRegistry`, exposes `as_component_registry()`, and
  registers via the same `register_block` flow. Zero new
  infrastructure; the type distinction alone is what we wanted.

**Recipe** for adding a shell primitive (mirrors the 4-step recipe
in `prism-builder/CLAUDE.md`):

1. Implement `Block` with a `lower_ui` override (and `render_slint`
   during the parallel-build period). Reuse `LowerCtx` helpers; do
   *not* hand-roll `UiNode::Container { … }` literals.
2. Add a row to `register_shell_builtins`'s `reg!(…)` macro table
   in `prism-shell/src/components/registry.rs`.
3. Reference the component from `.prism-ui` source by its registered
   id (`shell.<name>`). Resolution flows through the standard
   lowering pipeline — no shell-specific dispatch.
4. Cover lowering shape, ARIA / semantic propagation, schema, and
   registry insertion in unit tests next to the impl.

**Status.** `IconButton` landed 2026-05-08 as the template. The
remaining 12 chrome components migrate one-at-a-time as their
dependent runtime primitives (hover state, slot, image tint,
control-flow lowering, text input, popup overlay) land — see the
Phase-4 runtime-gap punch list under §8.
