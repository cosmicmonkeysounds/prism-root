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

### Phase 2 — DSL + parser + codegen
- New crate `packages/prism-ui-build` (compile-time codegen).
- Extend `prism-core::language` with `prism_ui` grammar +
  `SyntaxProvider`.
- `prism-cli codegen` learns the `prism-ui.d.luau` and
  `prism-ui-types.rs` emitters.
- Round-trip test: `.prism-ui` → AST → Rust → render commands → HTML →
  parse HTML → assert structural equivalence.

### Phase 3 — Component model unification
- Collapse `Component::render_slint` + `render_html` into
  `Component::layout`. Migrate the 16 `register_builtins` components
  (heading, text, link, image, container, form, input, button, card,
  code, divider, spacer, columns, list, table, tabs).
- Drop `HtmlRegistry`; `prism-relay` calls
  `prism_ui_runtime::html::lower_document` against the unified tree.
- Source-first machinery (ADR-006) re-pointed at `.prism-ui` markers.

### Phase 4 — Shell port
- Translate `ui/app.slint` (~2500 lines, 7 components) into
  `ui/app.prism-ui`. Behind a `prism-ui` cargo feature on `prism-shell`
  for a parallel-build period; native bin and wasm bin both build both
  variants until parity is reached.
- Re-target visual harness + `BuiltinScene` + `TestScript` /
  `E2eDriver` onto the new runtime.

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
