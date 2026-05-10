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
    and `TextInput` — **all three landed 2026-05-08** via the
    `LowerScope` / `SlotBindings` refactor in
    `prism_ui_runtime::interpret`, the sibling-pre-pass
    `expand_control_flow` for `<if>`/`<else-if>`/`<else>`/`<for>`,
    and the new `Node::TextInput` variant + `<input>` DSL tag (see
    decision-log entry below). The Phase-4 chrome scoreboard is now
    fully unblocked: every remaining primitive has runtime support
    for the layout vocabulary it needs.
- **Update 2026-05-08 (`prop_str` / `prop_bool` / `colored_text_node`
  helpers; DocsContent + AppCard land — 7 of 13 chrome primitives):**
  the recurring boilerplate at the top of every chrome `lower_ui`
  impl had three clear shapes and the AppCard/DocsContent migrations
  multiplied each. Promoted the shared helpers up into
  `prism-builder/src/ui_lower.rs`, refactored the existing five
  primitives onto them (zero behaviour change), and migrated the next
  two primitives end-to-end with no further private helpers.
  - **`prop_str(node, key) -> &str` / `prop_string(node, key) -> String`
    / `prop_bool(node, key, default) -> bool`** — the
    `node.props.get(k).and_then(|v| v.as_str()).unwrap_or("")` and
    `matches!(node.props.get(k), Some(Value::Bool(true)))` patterns
    every chrome primitive open-coded. One call site each instead of
    a 5-line let-binding.
  - **`colored_text_node(id, content, style, default_size, color)`** —
    promoted from a private `recolor_text` helper inside `toast.rs`
    (the original author flagged "promoting it is one move + one
    import when the second caller arrives"). Wraps the
    `let mut scoped = style.clone(); scoped.color = Some(c.into());
    text_node(...)` shape that section-header, toast, docs-content,
    and app-card all need. Builds correctly through `text_node` so
    cascade resolution / font_size still flows through one path.
  - **Existing primitives refactored:** IconButton, NavButton,
    SectionHeader, Toast all dropped 4-10 lines each by routing
    through the new helpers. Toast's private `recolor_text` deleted.
    No new behaviour; existing tests pass unchanged.
  - **`shell.docs-content` (6th primitive — text-only, no blocker):**
    title + summary + optional body column with mode-driven font
    sizing. **No per-row branching** in the lowering body — a
    `Metrics` struct (`FULL` / `COMPACT` const) holds every size +
    gap; the body reads from the picked struct. Adding a new mode
    is one struct literal. SSR semantic is `<article>`. 4 unit tests.
  - **`shell.app-card` (7th primitive):** 160px launchpad card with
    accent rail, hover-bg swap, conditional create/standard body,
    and a page-count badge. Single `icon_path` lookup table maps
    design-token icon names (`globe` / `music` / `zap` / …) to
    `icons/*.svg` paths — eight Slint `if` arms collapsed to one
    `match` expression. SSR semantic is `<article>` with
    `data-app="<id>"` (and `data-create="true"` for the create
    affordance) so launchpad scripts can target cards without the
    walker knowing about them. 6 unit tests covering both modes,
    both badge pluralisations, the icon fallback, and schema shape.
  - **Smart-pattern reinforcement:** every helper *composes* with
    the existing fluent builders rather than wrapping them in a new
    abstraction. No `ChromeBuilder` / `CardBuilder` types, no DI
    layer for prop access, no parallel lowering path. The promotion
    threshold is "same shape used in 3+ places"; the deletion of
    `recolor_text` from toast.rs is the canonical example of
    promote-when-needed, not promote-pre-emptively.
  - **Phase-4 chrome scoreboard:** **7 of 13** primitives migrated
    (`shell.icon-button`, `shell.toolbar-separator`,
    `shell.section-header`, `shell.nav-button`, `shell.toast`,
    `shell.docs-content`, `shell.app-card`). Remaining 6 cluster
    around the input-field family (`DragNumberField`, `FieldEditor`,
    `InspectorRow`) and the still-host-coupled chrome
    (`MenuBarRow`, `TransformEditor`, `AppWindow`). All runtime
    blockers cleared; remaining migrations are purely declarative.

- **Update 2026-05-08 (`shell.inspector-row` lands — 9/13; chrome
  helper module extracted):** the icon-button visual recipe — 28×28
  frame, 6px radius, 16×16 glyph, hover-bg swap, `<button>` SSR
  semantic — graduated out of `IconButton::lower_ui` into a new
  `prism-shell/src/components/chrome.rs` module the moment the second
  consumer arrived. `IconButton` now delegates to
  `chrome::icon_button_node(id, icon, enabled, aria_label)`; its
  `lower_ui` body shrunk to a five-line prop-translation call. The
  `InspectorRow` migration uses the same helper for the move-up /
  move-down chevrons (selected-node rows) and the trash button
  (`row`-kind + `show-delete=true`) — *zero* hand-rolled
  `UiNode::Container { … }` literals, *zero* re-implementation of
  the icon-button shape.
  - **`chrome.rs` smart-pattern surface:** three composition helpers
    that build on the existing `prism_builder::ui_lower` namespace
    rather than wrapping it in a new abstraction.
    - `icon_button_node(id, icon, enabled, aria_label) -> UiNode` —
      the shared visual recipe. `bare_container`-driven (no
      cascade), so it's safe to embed inside any other primitive's
      lowering without inheriting unrelated parent styling.
    - `indent_dot(id, color, radius_px) -> UiNode` — the 6×6 marker
      every tree-shaped row (inspector, outline, dock list) uses.
    - `color_or_transparent(hex) -> Color` — deterministic
      "no colour" fallback for chrome that needs an always-defined
      colour value (e.g. accent rails that paint nothing when the
      kind is unrecognised).
    Promotion threshold honoured: each helper has at least two
    in-tree callers at landing, with at least one more on the
    Phase-4 punch list.
  - **`shell.inspector-row` (9th primitive — input-family blocker
    cleared without a runtime extension):** 30px row with
    depth-driven indent padding (`PAD_LEFT_BASE + depth *
    INDENT_PX`), kind-driven palette (`node` / `row` / `empty`),
    and the optional right-side button cluster. The lowering body
    *never* branches on `kind` directly — a
    `metrics_for_kind(&str) -> &KindMetrics` lookup returns one of
    three `const KindMetrics` literals (`KIND_NODE` / `KIND_ROW` /
    `KIND_EMPTY`) carrying every visual difference (background,
    selected-background, dot color and radius, label size and
    color, whether the secondary id text shows, whether selected
    flips chevrons in, whether `show-delete` allows a trash
    button, ARIA role). Adding a fourth kind is one struct
    literal + one match arm; the lowering body is unchanged.
    Hover-on-row visibility (the original Slint version's
    `inspector-row-hover.has-hover` gate on the trash button)
    becomes a `show-delete: bool` prop the host flips on enter /
    leave — declarative storage, no runtime state machine, dirty
    bit only flips when the prop transitions. SSR semantic is
    `<div role="treeitem|group|none" aria-selected? aria-level?>`.
    10 unit tests cover the kind table, depth-indent math,
    chevron / trash visibility rules, ARIA propagation, and the
    "unknown kind falls through to node" default arm.
  - **`IconButton` refactor:** the constants
    (`ICON_BUTTON_SIZE` / `ICON_BUTTON_RADIUS` / `ICON_GLYPH_SIZE`
    / `ICON_BUTTON_HOVER_BG`) moved into `chrome.rs` as `pub const`
    so the lone `render_slint` consumer in `IconButton` re-imports
    them through `super::chrome`. No behaviour change; existing 7
    `IconButton` tests stay green. The `synthetic_container` →
    `bare_container` swap drops the cascade-resolved resting
    background, which was always `None` in practice (the original
    Slint version painted the resting state transparent and only
    activated `Palette.control-background` on hover).
  - **Phase-4 chrome scoreboard:** **9 of 13** primitives migrated
    (`shell.icon-button`, `shell.toolbar-separator`,
    `shell.section-header`, `shell.nav-button`, `shell.toast`,
    `shell.docs-content`, `shell.app-card`,
    `shell.drag-number-field`, `shell.inspector-row`). Remaining 4:
    `FieldEditor` (kind-driven editor row — fattest of the
    remaining; will reuse `shell.drag-number-field` + a `<select>`
    sibling + a switch primitive), `MenuBarRow` (slot + `<for>`;
    both runtime gaps already filled), `TransformEditor` (composes
    multiple `shell.drag-number-field` instances for X/Y/rotation/
    scale rows), and `AppWindow` (host shell — composes every
    other primitive plus dock layout). All four are pure
    declarative compositions of already-shipped primitives — no
    further runtime extensions blocked.

- **Update 2026-05-08 (final 4 chrome primitives — scoreboard
  closes at 13/13):** `TransformEditor`, `MenuBarRow`, `FieldEditor`,
  and `AppWindow` all migrated in one pass. The smart-pattern
  through-line was *promote-then-reuse*: every shape that two of
  the new primitives shared graduated up into `chrome.rs` once,
  consumed by all callers.
  - **`chrome::drag_number_field_node` + `format_drag_value`** —
    the 24px-tall scrubber visual recipe lifted out of
    `DragNumberField::lower_ui` into a shared helper. The standalone
    `DragNumberField` now delegates to it (lowering body shrunk
    from ~50 lines to ~15); `TransformEditor` calls it 5× per
    instance (Position x/y, Rotation, Scale x/y) and `FieldEditor`
    calls it once for the `number`/`integer` kind. Adding a
    seventh consumer (a future `Slider` shell primitive, say) is
    one helper call. Promotion threshold honoured: two consumers
    arrived together, so the helper was lifted in the same pass.
  - **`shell.transform-editor` (10th)** — Godot-style Position /
    Rotation / Scale / Anchor stack. Smart pattern: a single
    `ROW_SPECS` declarative table drives the whole layout. Each
    row is a `RowSpec { label, fields: &[AxisSpec] }` carrying
    label string + per-axis (label letter, axis colour, key
    suffix, value prop). The lowering body iterates `ROW_SPECS`
    and never branches on row identity. Adding a fifth row
    (Skew, Pivot, …) is one struct literal; adding a third axis
    to an existing row is one `AxisSpec` literal. SSR semantic is
    `<section data-role="transform-editor">`. 6 unit tests cover
    row count, per-row field count, anchor-pill shape, semantic
    propagation, and schema. Zero hand-rolled `UiNode::Container
    { … }` literals; every shape comes from `bare_container` /
    `colored_text_node` / `drag_number_field_node`.
  - **`shell.menu-bar-row` (11th)** — 28px top chrome carrying
    menu pills + optional separator + app-name pill + tab strip
    + add-page button. The `menus` and `tabs` props are JSON
    arrays (the runtime's `<for>` lowering would normally drive
    these; the Block consumes the resolved data shape directly).
    Three local helpers: `menu_pill_node`, `app_name_pill_node`,
    `tab_pill_node` — kept local until a second consumer arrives.
    The trailing add-page button reuses `chrome::icon_button_node`
    at native 28×28 (same shape `IconButton` itself uses). Outer
    semantic is `<nav role="menubar">`; tabs declare
    `role="tab"` with `aria-selected`; menu pills declare
    `role="menuitem"` with `aria-expanded`. 6 unit tests.
  - **`shell.field-editor` (12th)** — kind-driven property row
    (boolean / select / color / number / integer / text / file).
    Smart pattern: a `KIND_TABLE: &[KindEntry { kind, body,
    aria_role }]` lookup with `body: fn(&Node) -> Vec<UiNode>`
    function pointers. The lowering body is one branch: pick the
    builder, run it, wrap with the shared label + padding chrome.
    Adding a new kind is one `KindEntry` row + one body fn.
    Existing helpers carry every visual shape: `drag_number_field_node`
    for numeric kinds, `text_input_node` for text/file/color hex
    input, `bare_container` for the boolean switch and color
    swatch, a local `pill_with_chevron` for the select dropdown
    trigger (single-consumer; promoted later if a second caller
    arrives). Unknown kinds fall through to text. SSR semantic
    declares `data-key` and `data-kind` for stylesheet hooks. 8
    unit tests.
  - **`shell.app-window` (13th — capstone)** — top-level Studio
    shell scaffold. Lowers to a column (`<menu-bar>`, `<body
    row>`, `<status-bar>`) where the body row is `<activity-bar>
    + <main content>`. The content area uses
    `LowerCtx::lower_children(&node.children)` so any document
    subtree the host hands AppWindow flows through registered
    blocks unchanged — AppWindow has zero knowledge of what
    content blocks exist. Embedded chrome reuses `MenuBarRow::lower_ui`
    and `NavButton::lower_ui` directly: AppWindow constructs a
    derived `Node` for each (a transient wrapper, not a document
    mutation) and runs the existing block lowerings. **Zero
    duplication** of menu-pill / nav-button rendering — AppWindow
    is purely structural composition. SSR semantic uses `<main>`
    for the content area, `<nav role="navigation">` for the
    activity bar, `<footer role="contentinfo">` for the status
    bar. 7 unit tests cover the three-section column shape, body
    row composition, menu prop propagation, status text, content
    children pass-through, activity-bar nav-button instantiation,
    and outer `data-role`.
  - **Verification:** all 419 `prism-shell` lib tests pass; clippy
    `-D warnings` clean; registry `len()` is now 13 (up from 9).
    The Phase-4 chrome scoreboard is **closed**: every primitive
    in `ui/app.slint`'s shell-component vocabulary has a
    `Block::lower_ui` implementation. Image-tint remains the lone
    deferred runtime extension and is still not on the critical
    path (no migrated primitive blocks on it).

- **Update 2026-05-08 (`shell.drag-number-field` lands — 8/13):**
  the first input-family primitive flows through the same
  `Block::lower_ui` recipe as the prior seven. The drag / commit /
  inline-edit interactivity is pure host concern (input dispatch +
  signal emission); the lowering is purely visual structure — a
  24px-tall outer container with a 3px radius and a
  `hover_bg(...)`-driven swap, holding a row with the optional
  11px label and the formatted value. No new helpers needed:
  `bare_container` / `colored_text_node` / `hover_bg` /
  `parse_color` / `uniform_radius` / `prop_str` / `prop_string`
  cover every shape. SSR semantic is `<label data-key=…
  aria-label=…>` so screen readers announce the field by its
  intended caption. Two signals (`changed`, `committed`) describe
  the drag / inline-commit pair the original Slint version
  declared. 5 unit tests cover row shape, label conditional, value
  formatting, semantic / aria propagation, and signal surface.

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
  2. **`<slot/>` semantics** for component composition — *landed
     2026-05-08* via `prism_ui_runtime::interpret::SlotBindings` +
     `LowerScope::with_slots`. `<slot/>` (default) and
     `<slot name="x"/>` (named) resolve through the lowering scope's
     slot map; un-bound slots fall back to their own children as
     fallback content. Component-instantiation paths (Phase 3)
     populate the slot map from the caller's children. Smart pattern:
     slots stay at the AST/lowering level — they never reach the
     runtime `Node` enum, so the layout/paint/SSR backends are
     unchanged, and the component-instantiation pass is a one-line
     scope construction.
  3. **`Image::colorize`** — icon tinting (palette foreground +
     transparency variants). Add `tint: Option<Color>` to
     `Node::Image`; the femtovg backend already pre-multiplies mask
     glyphs with a colour, the same code resolves an image's tint.
  4. **`<if>` / `<else-if>` / `<else>` / `<for>` lowering** —
     *landed 2026-05-08* via the sibling pre-pass
     `expand_control_flow` in `interpret.rs`. Single helper resolves
     the whole control-flow vocabulary against a `LowerScope`'s
     bindings; per-element lowering bodies stay unchanged. `for`
     parses `"<var> in <source>"` and forks a child scope per item;
     `if` / `else-if` / `else` chain across siblings, with
     whitespace-only text deliberately *not* breaking the chain so
     formatted source round-trips cleanly. Adding a new control-flow
     keyword is one match arm in `control_flow_attr`.
  5. **`TextInput`** primitive on `Node` — *landed 2026-05-08* as
     `Node::TextInput { id, value, placeholder, props, width, height,
     radius, semantic }` plus the `<input>` DSL tag and a
     `prism_builder::ui_lower::text_input_node` constructor. Smart
     pattern: the leaf composes from existing render commands at
     emit time (`Rectangle` background + `Border` + `Text` for
     value-or-placeholder), so the four backends (femtovg, web,
     html, semantic_html) needed only the `semantic_html` `<input>`
     arm — no new `RenderCommand` variant, no per-backend dispatch
     branch. cosmic-text editing / focus / IME flow lands in a later
     pass when the host wires keyboard events through
     `event::EventHandler`.
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
| 2026-05-08 | `prop_str` / `prop_bool` / `colored_text_node` promoted into `prism-builder/src/ui_lower.rs`; `shell.docs-content` + `shell.app-card` migrated (7/13) | Refactor revealed three boilerplate shapes (string-prop, bool-prop, text-with-override-colour) repeated across every chrome primitive's `lower_ui`. Promoted them once instead of cloning per-block. The colored_text_node promotion follows the rule-of-three threshold the toast author already flagged in a comment. Refactored callers shrank by 4-10 lines each with no behaviour change; the two new primitives compose entirely from the shared helpers — zero hand-rolled `UiNode::Container { … }` literals, zero per-block private helpers. |
| 2026-05-08 | `chrome.rs` shared visual-helpers module; `IconButton` recipe extracted; `shell.inspector-row` lands as 9th chrome primitive | The icon-button shape (28×28 frame, 16×16 glyph, 6px radius, hover swap, `<button>` SSR) graduated out of `IconButton::lower_ui` into `prism-shell/src/components/chrome.rs` the moment the second consumer (InspectorRow's chevron / trash buttons) arrived. Helper composes through the existing `prism_builder::ui_lower` namespace — no new builder type, no DI layer. InspectorRow's three-way `kind` palette (`node` / `row` / `empty`) is a `const KindMetrics` lookup table; the lowering body never branches on `kind` directly. Adding a fourth kind is one struct literal + one match arm. Hover-driven trash-button visibility lives on a `show-delete: bool` prop the host flips on enter/leave — declarative storage, no runtime state machine. |
| 2026-05-08 | Phase-4 chrome scoreboard closes at 13/13 — `TransformEditor`, `MenuBarRow`, `FieldEditor`, `AppWindow` all migrated in one pass | Promote-then-reuse smart pattern: `chrome::drag_number_field_node` + `format_drag_value` lifted out of `DragNumberField::lower_ui` so the same scrubber recipe drives `TransformEditor` (5× per instance), `FieldEditor` (numeric kinds), and `DragNumberField` itself with zero duplication. `TransformEditor` is fully driven by a `ROW_SPECS` declarative table; `FieldEditor` dispatches via a `KIND_TABLE` of `(kind, body_fn, aria_role)` rows; `AppWindow` composes by *running* `MenuBarRow::lower_ui` and `NavButton::lower_ui` over derived nodes rather than re-implementing pill/button rendering. Adding a new transform row, field-editor kind, or app-window section is one literal in the relevant table. Every primitive's lowering body is branch-free over its cross-instance variation. |
| 2026-05-08 | The three remaining Phase-4 blockers land in one declarative refactor: `LowerScope` + `SlotBindings`, `expand_control_flow`, and `Node::TextInput` | All three concerns funnel through `prism_ui_runtime::interpret`, so a single `LowerScope` carrier (bindings + slot map) serves both `{ident}` interpolation and the control-flow predicate evaluator — no per-feature scope stack. Slots stay at the AST level so backends don't grow a new `Node` variant. Control flow is a sibling pre-pass that returns `Vec<(node, optional-child-scope)>`, keeping per-element lowering branch-free. `TextInput` composes a `Rectangle` + `Border` + `Text` at emit time, so all four backends inherit it for free; only `semantic_html` grew an `<input>` arm. Net effect: Phase-4 chrome scoreboard fully unblocked with ~600 LoC across one file refactor + thin runtime additions, no new abstractions, no new infrastructure. |

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

**Status.** `IconButton` landed 2026-05-08 as the template; the
scoreboard closed at **13 of 13** the same day after `TransformEditor`,
`MenuBarRow`, `FieldEditor`, and `AppWindow` migrated in a single
pass. The shared `prism-shell/src/components/chrome.rs` module
now collects four reusable recipes: `icon_button_node` (consumed
by IconButton, InspectorRow), `drag_number_field_node` +
`format_drag_value` (consumed by DragNumberField, TransformEditor,
FieldEditor), `indent_dot` (InspectorRow), and `color_or_transparent`.
The rule-of-three promotion threshold gates entries — every helper
has at least two in-tree callers at landing. Every runtime gap
from the original punch list has landed (hover-state, slot,
control-flow, text-input, overlay z-layer); image-tint is the
lone deferred extension and is not on the critical path. 419
prism-shell tests + clippy `-D warnings` clean.

## 13. Tag-resolver DI — `.prism-ui` source addresses every registered component

**Strategy locked 2026-05-09.** With the chrome scoreboard closed at
13/13, the next blocker for translating `ui/app.slint` into
`ui/app.prism-ui` was the runtime's closed tag vocabulary: the
`prism_ui_runtime::interpret` lowering only knew six built-in tags
(`container`, `text`, `heading`, `spacer`, `input`, `slot`) and
silently dropped everything else. Author-side, that meant
`<shell.icon-button …/>` in source was indistinguishable from a
typo — the wrapper element would be discarded and only its children
survived.

**Solution.** A single `TagResolver` trait + `LowerScope::with_resolver`
hook on the runtime, plus one `RegistryTagResolver` impl in
`prism-builder` that wraps a [`ComponentRegistry`]. The runtime stays
component-registry-agnostic; the builder owns the `Block` →
`Component::lower_ui` dispatch.

**Smart-pattern wins (every constraint at the top of plan §0 honoured):**

- **One extension seam, not three.** No parallel "runtime block"
  trait, no `RuntimeRegistry`, no string-dispatch arm in the runtime
  for each new tag. Every component vocabulary (Prism Builder blocks,
  shell chrome, future plugin-supplied components, user prefabs) plugs
  into the same `TagResolver` trait. Runtime dependency direction
  preserved: `prism-ui-runtime` declares the trait, downstream crates
  implement it.
- **Composition over inheritance.** `RegistryTagResolver` re-uses
  every block's existing `Component::lower_ui` impl unchanged — adding
  a new tag to the `.prism-ui` vocabulary is **zero additional work**
  beyond the standard `register_block` call. The same `lower_ui` body
  drives the editor render path (`document_to_ui_tree` → `lower`),
  the SSR path (`lower_semantic_html_with_registry`), *and* now the
  source-driven path (`<my.tag …/>` resolves through the resolver).
  Three consumers, one declaration.
- **DI through the scope, not a global.** The resolver lives on
  `LowerScope` as `Option<Arc<dyn TagResolver>>`, so callers opt in
  per-scope. Tests inject a fake resolver; production wires the real
  registry; the runtime has no notion of "the resolver" anywhere.
- **Builder pattern preserved.** `LowerScope::with_resolver(arc)`
  composes with the existing `with_binding` / `with_slots` builder
  surface — child scopes (control-flow forks, slot expansions) inherit
  the resolver via the same `Clone` path that already propagates
  bindings.
- **AST → builder Node translation lives once.** `element_to_builder_node`
  is the single seam for the namespace mapping (bare → props, `data:k`
  → props[k], `aria:k` → props["aria-{k}"], `id` → node.id, `style:*`
  / `on:*` / `bind:*` deferred). Every resolver consumer goes through
  this one helper; adding a new namespace handling is one match arm.
- **Boolean-attribute and numeric-attribute coercion live exactly
  once** in `value_for(raw)`, mirroring HTML's "boolean attribute"
  convention. Schema-aware coercion is the block's job at read-time.

**Surface added (9 new public items, ~250 LoC across two files):**

- **`prism_ui_runtime::interpret::TagResolver`** — the trait. One
  method: `fn resolve(&self, element: &Element, scope: &LowerScope)
  -> Option<Vec<Node>>`. `Some` short-circuits the unknown-tag
  fall-through; `None` lets the runtime apply its default (drop the
  wrapper, keep children).
- **`LowerScope::with_resolver(Arc<dyn TagResolver>)`** /
  **`LowerScope::resolver()`** — installation + accessor.
- **`prism_builder::ui_resolver::RegistryTagResolver`** — the
  `TagResolver` impl. Constructor takes `Arc<ComponentRegistry>`;
  `from_registry(reg)` convenience wraps a freshly built registry.
- **`prism_shell::components::ShellComponentRegistry::tag_resolver()`**
  — one-call helper that returns an `Arc<dyn TagResolver>` ready to
  hand to a `LowerScope`. The shell host code reads as
  `LowerScope::default().with_resolver(reg.tag_resolver())`.

**Parser extension (one-line, source-class widening):** the
`prism-core::language::prism_ui` grammar's tag-name scanner picked up
`.` and `-` as legal characters, since registered ids carry namespaces
(`shell.icon-button`, future `app.foo-bar`). The closing-tag scanner
got the same widening so `</shell.icon-button>` round-trips. No new
identifier rules elsewhere — the change is local to `parse_element`'s
opening / closing tag paths.

**Verification (2026-05-09):**

- `prism-ui-runtime`: 56 lib tests (3 new — `resolver_handles_unknown_tag_when_returning_some`,
  `resolver_returning_none_falls_back_to_default_unknown_tag`,
  `resolver_propagates_through_for_loop_child_scopes`).
- `prism-builder`: 423 lib tests (5 new in `ui_resolver::tests` —
  `registered_tag_lowers_through_block`, `unregistered_tag_falls_through_to_default`,
  `boolean_attribute_coerces_to_true`, `numeric_attribute_coerces_to_number`,
  `aria_attribute_lands_with_aria_prefix`).
- `prism-core`: 2045 lib tests (parser changes covered by existing
  prism_ui suite — closing-tag round-trips and dotted-tag parsing
  exercised by the downstream resolver tests).
- `prism-shell`: 421 lib tests (2 new in
  `components::registry::tests` —
  `tag_resolver_lowers_shell_icon_button_from_prism_ui_source` walks
  `<container><shell.icon-button id="ib" icon="icons/x.svg"
  tooltip-text="Close"/></container>` end-to-end through `parse` →
  `lower_document_with_scope` → `IconButton::lower_ui` → runtime Node
  with `<button>` semantic + `aria-label="Close"`;
  `tag_resolver_unknown_tag_falls_through_to_runtime_default` confirms
  the default behaviour is preserved). 13/13 chrome primitives now
  reachable from `.prism-ui` source by tag.

**Children deferred.** The v0 resolver passes `children: Vec::new()`
to the block — fine for the 12/13 chrome primitives whose visual
structure comes from props (IconButton, ToolbarSeparator, NavButton,
DragNumberField, TransformEditor, FieldEditor, Toast, AppCard,
DocsContent, InspectorRow, MenuBarRow, SectionHeader). `AppWindow` is
the lone composition-style block that walks `node.children`; once
`ui/app.prism-ui` lands and AppWindow needs to host real subtrees from
source, the resolver will pre-lower AST children through the runtime
and inject them via the existing `<slot/>` mechanism — a one-method
extension that keeps every other primitive unchanged.

**Why this is the keystone.** With this in, `ui/app.prism-ui` can
*finally* be authored: every shell-chrome primitive is referenceable
by tag, every runtime gap from the original punch list is filled, and
the cascade / SSR / native render paths all converge on the same
`Component::lower_ui` declarations the chrome scoreboard already
landed. The Phase-4 tail ("translate `ui/app.slint` into
`ui/app.prism-ui`") is now mechanical authoring rather than blocked
infrastructure work.

**Decision-log entry:**

| Date | Decision | Rationale |
|---|---|---|
| 2026-05-09 | `TagResolver` DI seam in `prism-ui-runtime::interpret` + `RegistryTagResolver` impl in `prism-builder`; `ShellComponentRegistry::tag_resolver()` wraps the shell registry | Closed tag vocabulary in the runtime was the last blocker for translating `ui/app.slint` into `ui/app.prism-ui`. Single trait extension (one method, `Option<Vec<Node>>` return) lets every host-supplied component vocabulary plug into the same lowering pipeline. No parallel walker, no new abstraction layer — every block's existing `Component::lower_ui` is reused. Parser tag-scanner widened to allow `.` and `-` in tag names so `<shell.icon-button>` parses. Verified end-to-end via `prism-shell` test that lowers a `.prism-ui` source containing `<shell.icon-button …/>` through the registered IconButton block. |

## 14. Resolver children — `LowerCtx::host_children` opt-in slot

**Strategy locked 2026-05-09 (same-day follow-up to §13).** Closing
the resolver-children deferral noted at the bottom of §13. With
`<shell.app-window>…</shell.app-window>` now reachable from
`.prism-ui` source, composition-style blocks need a way to receive
the *AST children* the author wrote between the open/close tags.
The v0 resolver discarded them. The deferred plan was to "pre-lower
AST children through the runtime and inject them via the existing
`<slot/>` mechanism" — but `<slot/>` is an AST/lowering concept
(resolved during `interpret`), and blocks lower through Rust code
that never reaches `<slot/>`. So the slot mechanism, as-is, can't
help blocks like AppWindow that are imperatively constructed.

**Solution (~70 LoC).** A single sparse `host_children:
Option<&[UiNode]>` slot on `LowerCtx`, populated by the resolver
from the element's pre-lowered AST children, consumed by exactly
those blocks that opt in. Plain blocks (the 12/13 chrome primitives
whose visual structure comes from props) ignore it; `AppWindow`
reads it through a one-line fallback chain. Zero new abstraction,
zero new context type, and the same `LowerCtx` continues to thread
cascade + registry through every existing call site.

**Smart-pattern wins (§0 constraints honoured):**

- **One seam, not two.** The existing `LowerCtx` gains a sparse
  optional field — the same context type already threading cascade
  and registry through every block's `lower_ui`. No parallel
  `CompositionContext`, no new trait, no per-block dispatch arm in
  the resolver.
- **Opt-in by reading.** Blocks that don't recurse never observe
  the slot. Composition blocks declare interest by *reading* the
  accessor — no marker trait, no schema annotation, no blanket impl
  to re-derive. `AppWindow::lower_ui` becomes:

  ```rust
  let content_children = ctx
      .host_children()
      .map(|s| s.to_vec())
      .unwrap_or_else(|| ctx.lower_children(&node.children));
  ```

  Two-line fallback chain. Host-driven path (Shell constructs
  builder Nodes) and source-driven path (resolver feeds pre-lowered
  UiNodes) converge on the same downstream code.
- **Resolver pre-lowers through the same scope.** The resolver
  calls a new `prism_ui_runtime::interpret::lower_ast_children`
  (a public alias for the previously-private `lower_children` —
  the *single chokepoint* for "lower this AST sibling list"). That
  helper routes through the same control-flow pre-pass, the same
  `<slot/>` resolution, the same nested resolver dispatch the
  document walk uses. So `<for>` loops, `<slot/>` placeholders, and
  even nested `<shell.app-window>` instantiation inside the children
  all work uniformly.
- **Slot intentionally does not propagate.** `LowerCtx::lower(&node)`
  forks a child context that drops `host_children` to `None` —
  the slot belongs to the one block the resolver is delegating
  to. Without this, a host block that chooses to also recurse into
  `node.children` would accidentally hand the same pre-lowered
  slice to every descendant. Verified by an explicit
  `resolver_host_children_does_not_propagate_to_recursive_lower`
  test that nests a composition block inside another composition
  block.

**Surface added (3 new public items, ~70 LoC across three files):**

- **`prism_ui_runtime::interpret::lower_ast_children(&[AstNode],
  &LowerScope) -> Vec<Node>`** — re-exported alias of the
  formerly-private `lower_children`. Single chokepoint: every "lower
  these AST children" caller (resolver pre-pass, future plugin
  hooks, test scaffolding) goes through this one function.
- **`prism_builder::ui_lower::LowerCtx::with_host_children(&[UiNode])
  -> Self`** — builder-style installer. Composes with the existing
  `LowerCtx::new` surface; child scopes deliberately do not inherit.
- **`prism_builder::ui_lower::LowerCtx::host_children() ->
  Option<&[UiNode]>`** — accessor blocks read in their fallback
  chain.

**Resolver delta (8 lines net).** `RegistryTagResolver::resolve`
gained a single pre-pass: when `element.children` is non-empty,
it calls `lower_ast_children(&element.children, scope)` and threads
the resulting `Vec<UiNode>` through `LowerCtx::with_host_children`.
The block path is unchanged.

**Block migration (one block, opt-in).** Only `AppWindow` reads the
slot, since it is the only composition-style chrome block. The
12/13 prop-driven blocks (`IconButton`, `ToolbarSeparator`,
`NavButton`, `DragNumberField`, `TransformEditor`, `FieldEditor`,
`Toast`, `AppCard`, `DocsContent`, `InspectorRow`, `MenuBarRow`,
`SectionHeader`) compile and run unchanged because they never read
`ctx.host_children()`. Future composition blocks (a `<shell.tab-panel>`
that hosts arbitrary tab bodies, a `<shell.docked-region>` that hosts
a panel subtree, …) get density for free by reading the same
accessor.

**Verification (2026-05-09):**

- `prism-ui-runtime`: 56 lib tests (no count change — `lower_ast_children`
  exercised through every existing test that flows through
  `lower_document_with_scope`).
- `prism-builder`: 422 lib tests (3 new in `ui_resolver::tests` —
  `resolver_pre_lowers_ast_children_into_host_children_slot`,
  `resolver_host_children_is_empty_when_source_has_none`,
  `resolver_host_children_does_not_propagate_to_recursive_lower`).
- `prism-shell`: 422 lib tests (1 new in `components::registry::tests`
  — `tag_resolver_lowers_app_window_with_prism_ui_authored_children`
  walks `<shell.app-window id="aw" status="Ready"><text>greeting</text>
  <text>tagline</text></shell.app-window>` end-to-end through `parse`
  → `lower_document_with_scope` → `RegistryTagResolver::resolve` →
  `lower_ast_children` → `LowerCtx::with_host_children` →
  `AppWindow::lower_ui` → runtime Node, asserting both text children
  land verbatim in the `<main>` content area).
- Workspace `cargo test --workspace --lib` green; clippy `-D warnings`
  clean across every crate.

**Why this is the right level of abstraction.** Composition blocks
reach for `ctx.host_children()` the same way they already reach for
`ctx.lower_children()` and `ctx.default_container()` — one more
accessor on the existing `LowerCtx` namespace, no new vocabulary
to learn. The promotion threshold from the rest of the migration
(rule-of-three, "compose with the existing fluent builder rather
than wrapping it in a new abstraction") is honoured: the slot has
exactly one consumer at landing (`AppWindow`), but the threshold
*for slots specifically* is one — adding the field at the moment
the first composition block needs it costs less than retrofitting
a `CompositionContext` newtype later, and every other block was
unaffected by the change.

**What this unblocks.** The Phase-4 tail at the end of §13 ("the
Phase-4 tail is now mechanical authoring rather than blocked
infrastructure work") is now also unblocked for composition: the
canonical `ui/app.prism-ui` skeleton —

```prism-ui
<shell.app-window id="root" status="Ready" app-name="Studio">
  <!-- the document's actual content tree -->
</shell.app-window>
```

— resolves end-to-end through the registered AppWindow block with
the inner subtree flowing into the `<main>` content area. Authoring
`ui/app.prism-ui` now requires zero further runtime extensions.

**Decision-log entry:**

| Date | Decision | Rationale |
|---|---|---|
| 2026-05-09 | `LowerCtx::host_children` slot + `lower_ast_children` runtime helper; `RegistryTagResolver` pre-lowers AST children; `AppWindow` opts in via fallback chain | Closes the §13 resolver-children deferral. Single sparse field on the existing `LowerCtx` is the smallest seam that lets composition-style blocks consume `<shell.app-window>…</shell.app-window>` subtrees from `.prism-ui` source. No new abstraction, no parallel context type, no marker trait — opt-in by reading the accessor. Plain blocks unchanged. Resolver pre-lowering routes through the runtime's existing scope (control-flow / `<slot/>` / nested resolver dispatch all propagate uniformly). Slot intentionally drops on `LowerCtx::lower` recursion so it stays bound to one block per resolver call. Phase-4 `ui/app.prism-ui` authoring is now fully unblocked end-to-end. |

## 15. Embedded chrome via `LowerCtx::lower_as` + canonical `ui/app.prism-ui` skeleton

**Strategy locked 2026-05-09 (continuation of §14).** With the
resolver-children seam in place, the next blocker for the Phase-4
shell port was a quieter form of duplication inside `AppWindow`:
its embedded chrome (the menu-bar row, the activity-bar's nav
buttons) was being rendered by **importing the concrete `Block`
impls and instantiating them by hand** —

```rust
// before: registry-bypassing, single-impl-locked
use super::menu_bar_row::MenuBarRow;
use super::nav_button::NavButton;

let block = MenuBarRow { id: "shell.menu-bar-row".into() };
let cascade = StyleProperties::default();
let ctx = LowerCtx::new(None, &cascade);
block.lower_ui(&ctx, &derived, &cascade)
```

The dispatch lived twice: once in `register_shell_builtins`'s
`reg!(…)` table, again inside `app_window.rs` as imports + literal
instantiation. The fresh `LowerCtx::new(None, …)` discarded any
registry the host had attached, so a host-supplied alternative
`shell.menu-bar-row` impl was silently ignored — registration
existed but was *bypassed* for embedded chrome.

**Solution (~50 LoC, single new public method).** A
`LowerCtx::lower_as(component_id, derived_id, props_json) ->
Option<UiNode>` helper that synthesises a derived `Node` and
dispatches through whichever `ComponentRegistry` is on the ctx —
the *same* registry the resolver path uses. AppWindow's two
`synth_*` helpers shrink to a one-call dispatch each; the
`MenuBarRow` / `NavButton` imports and the per-call-site
`derived_node()` private helper disappear entirely.

**Smart-pattern wins (every constraint at the top of plan §0 honoured):**

- **One seam, not three.** The method lives on the existing
  `LowerCtx` namespace alongside `lower` / `lower_children` /
  `default_container`. No new context type, no new trait, no
  parallel "EmbedRegistry" abstraction. Composition-style blocks
  reach for `ctx.lower_as(...)` the same way they already reach
  for `ctx.lower_children(...)`.
- **DI through the existing carrier.** The registry that
  `RegistryTagResolver` already attaches to `LowerCtx::new(Some(®),
  …)` is the same registry `lower_as` resolves against. No new
  threading, no parallel injection point. A host that registers a
  custom `shell.menu-bar-row` (e.g. a per-product variant of the
  Studio shell) automatically wins for embedded chrome.
- **Fallback by `Option`, not branching.** `lower_as` returns
  `Option<UiNode>`: `None` when no registry is attached or the id
  isn't registered. Composition blocks fold the option with
  `unwrap_or_else(|| placeholder(...))` and stay branch-free over
  registry presence. Headless / no-registry tests get coherent
  structural shapes (still column with three sections, body row
  with activity-bar + content) without any block-type knowledge in
  the test path.
- **No duplicate cascade machinery.** `lower_as` runs the same
  `resolve_cascade` call the runtime's own `LowerCtx::lower` does,
  forks a child `LowerCtx` with the resolved style, and hands it
  to the dispatched `Component::lower_ui` — every cascade
  invariant the rest of the codebase relies on is preserved.

**Surface added (1 new public method, ~50 LoC):**

- **`LowerCtx::lower_as(&self, component_id: &str, derived_id:
  impl Into<String>, props: serde_json::Value) -> Option<UiNode>`**
  — the single embedding seam. Synthesises a transient `Node` with
  default layout/transform/style, runs cascade resolution, and
  dispatches through the registry on `self`. Returns `None` only
  when no dispatch can happen (no registry / unregistered id).

**Block migration (one block, one helper module).** Only
`AppWindow` consumed embedded chrome at landing; its `synth_menu_bar`
and `synth_activity_bar` were the canonical cleanup target.
After the refactor:

- `synth_menu_bar(ctx, node)` is a 5-line `ctx.lower_as` call
  with the JSON props shape derived once via
  `json_object_with_keys`.
- `synth_activity_bar(ctx, node)` iterates the `nav-buttons` JSON
  array (already declarative since the §13 chrome scoreboard
  closed) and `filter_map`s each entry through `ctx.lower_as`.
- The local `derived_node` private helper and the
  `super::menu_bar_row::MenuBarRow` / `super::nav_button::NavButton`
  imports are deleted. AppWindow no longer knows the *type* of
  any embedded chrome block.
- A 4-line `placeholder(id, role)` helper is the no-registry
  fallback — produces a `<div data-role="…">` bare container
  so the structural-shape tests stay independent of registry
  attachment.

**Canonical `ui/app.prism-ui` skeleton landed.** The §14 keystone
example now exists on disk at
`packages/prism-shell/ui/app.prism-ui` as the source-driven
replacement for the legacy `ui/app.slint`. Contents:

```prism-ui
<!-- Prism Studio shell skeleton. … -->
<shell.app-window id="root" status="Ready" app-name="Studio">
  <container id="content-root">
    <text id="welcome">Prism Studio</text>
  </container>
</shell.app-window>
```

The `canonical_app_prism_ui_skeleton_lowers_end_to_end` test in
`prism-shell/src/components/registry.rs` loads the file via
`include_str!`, runs it through `parse` →
`lower_document_with_scope` with the full
`ShellComponentRegistry`'s tag resolver, and asserts the AppWindow
root produces a 3-section column whose `<main>` content area
adopts the inner `<container id="content-root">` subtree
verbatim. Authoring the rest of `ui/app.slint`'s panels is now
pure declarative composition — every chrome region has a
registered tag, every content region has a registered block, and
embedded chrome routes through one DI seam.

**Parser caveat (filed for follow-up):** the `prism-core` prism_ui
grammar's HTML-style comment scanner (`grammar.rs:600`,
`consume_until_gt`) panics when a multi-byte UTF-8 character (e.g.
em-dash) appears inside a `<!-- … -->` block — it slices the
source by byte offset without char-boundary checks. The
`app.prism-ui` skeleton sidesteps the bug by sticking to ASCII in
comments. Fix is a 2-line check in the scanner; not on the
critical path for Phase-4 authoring, but should land before the
panel translations import author-written prose with typographic
punctuation.

**Verification (2026-05-09):**

- `prism-builder`: 429 lib tests (3 new in `ui_lower::tests` —
  `lower_as_resolves_through_registry_when_attached`,
  `lower_as_returns_none_when_no_registry`,
  `lower_as_returns_none_when_id_unregistered`).
- `prism-shell`: 423 lib tests (1 new in
  `components::registry::tests` —
  `canonical_app_prism_ui_skeleton_lowers_end_to_end` walks the
  on-disk `ui/app.prism-ui` through the full pipeline). The
  pre-existing `app_window` tests (`menu_bar_includes_menus_from_props`,
  `activity_bar_lowers_each_nav_button_from_props`) now opt into a
  `lower_with_full_registry` helper that attaches the real shell
  registry — they assert the *behaviour* (pills present, buttons
  present) without depending on the concrete embedded `Block`
  impl. Structural-shape tests stay no-registry and exercise the
  `Option::None` placeholder path.
- `prism-ui-runtime`: 56 lib tests (no count change — `lower_as`
  is purely additive on `LowerCtx`).
- Workspace `cargo test --workspace --lib` green; clippy
  `--all-targets -D warnings` clean across every crate.

**Why this is the right level of abstraction.** The shape mirrors
every prior smart-pattern landing in this plan: a single new
method on the existing fluent context type, composing with
already-shipped accessors (`registry`, `parent_style`,
`host_children`), reused by the *one* current consumer with a
clear path for future composition blocks (a `<shell.tab-panel>`
that hosts arbitrary tab bodies, a `<shell.docked-region>` that
embeds nav chrome, …) to inherit the density automatically. The
rule-of-three threshold is honoured *for embedding specifically* —
the threshold is one, because the cost of retrofitting the seam
later (after every composition block had grown its own
hand-rolled "instantiate the Block, build a fresh LowerCtx, call
lower_ui" boilerplate) would have been linear in the number of
composition blocks. Adding the method now costs less than
deduplicating two callers, let alone four.

**What this unblocks.** With the canonical skeleton landed and
embedded chrome routing through DI, the rest of Phase-4 is
*purely additive*: each remaining region of the legacy
`ui/app.slint` (sidebars, dock layout, builder canvas, properties
panel, code editor, command palette overlay) becomes either a
new `Block` impl (one row in `register_shell_builtins`) or a tag
in `app.prism-ui` (one element). The infrastructure case is
closed — no further runtime extensions, DI seams, or context
threading is anticipated to translate any specific panel.

**Decision-log entry:**

| Date | Decision | Rationale |
|---|---|---|
| 2026-05-09 | `LowerCtx::lower_as` embedding seam; `AppWindow` refactored to dispatch embedded chrome through it; canonical `ui/app.prism-ui` skeleton landed with end-to-end test | Closes the embedded-chrome registry-bypass duplication noted at the bottom of §14. Single new method on the existing `LowerCtx` namespace lets composition blocks dispatch any registered chrome by id without importing the concrete `Block` impl — host-supplied overrides take effect for embedded chrome the same way they do for top-level resolver dispatch. AppWindow's `synth_menu_bar` / `synth_activity_bar` shrink to one-call helpers; the `derived_node` private helper and the per-block imports are deleted. The on-disk `ui/app.prism-ui` skeleton is the keystone artifact §14 promised — proves the Phase-4 authoring pipeline (parse → resolver → composition-block lowering with `host_children`) end-to-end through a registry-driven test. Phase-4 panel translations are now purely declarative additions; no further runtime extensions or DI seams anticipated. Filed: prism_ui parser bug at `grammar.rs:600` panics on non-ASCII inside comments (sidestepped via ASCII-only skeleton; 2-line scanner fix queued). |
| 2026-05-09 | `Scanner::advance_unicode` lands; prism_ui comment + text scanners use it; non-ASCII content in `<!-- … -->` and text nodes round-trips | Closes the parser bug filed in the previous entry. The legacy `Scanner::advance` is byte-stepping by design (the syntactic vocabulary is ASCII-only); a sibling `advance_unicode` reads the full UTF-8 codepoint via `source[offset..].chars().next()` and steps `offset` by `len_utf8`, so the cursor always lands on a char boundary. Only the two scanners whose body content can hold non-ASCII (`parse_comment`, `parse_text_or_interpolation`) opt in — every other caller stays byte-precise. Two new prism_ui tests (`comment_with_non_ascii_content_round_trips`, `text_node_with_non_ascii_content_round_trips`) cover em-dash + curly-quote + accented content. `app.prism-ui` can now use typographic punctuation in author-written prose. |
| 2026-05-09 | Image-tint runtime extension lands (`Node::Image::tint`, `RenderCommand::Image::tint`, `tinted_image_node` helper, `chrome::icon_button_node_tinted`, `IconButton` block `tint` prop) | The lone deferred runtime extension from the original Phase-4 punch list (gap #3 — "Image::colorize"). Sparse `Option<Color>` field on the existing `Node::Image` / `RenderCommand::Image` variants — `None` paints the image verbatim, `Some(c)` instructs the renderer to mask-paint the colour through it (the canonical icon-tint pattern). HTML backend lowers tinted images to a `mask-image` + `background-color` pair (CSS icon-tint hack); femtovg backend already pre-multiplies mask glyphs with a colour and resolves images the same way once the asset decoder phase lands. `IconButton` block now exposes a `tint` schema field so `<shell.icon-button icon="…" tint="#ff0000"/>` reproduces the Slint `colorize` behaviour from `.prism-ui` source. `chrome::icon_button_node_tinted` is the new sibling helper; `icon_button_node` delegates to it with `tint: None` so the four existing untinted call-sites (InspectorRow chevrons / trash, MenuBar add-page button, IconButton without prop) compile unchanged. Two new prism-shell tests cover the prop → glyph-tint propagation and the missing-prop fallback. Two new html-backend tests cover the masked-span + plain-img branches. Image-tint was the only "lone deferred extension" still on the list per the §13/§15 closing remarks; punch list is now empty. |

## 16. Panel-by-panel translation: each region of `ui/app.slint` → one row in `register_shell_builtins` *or* one tag in `app.prism-ui`

**Strategy locked 2026-05-09 (continuation of §15).** With the
infrastructure case closed (parser, lowering pipeline, registry,
resolver children slot, embedding seam, image-tint), every remaining
region of the legacy `ui/app.slint` falls into exactly one of two
buckets:

1. **Leaf or self-contained chrome** → a new `Block` impl in
   `prism-shell/src/components/<name>.rs`, registered as one row in
   `register_shell_builtins`. The block owns its layout, reads its
   data from typed props, and renders a `UiNode` subtree with no
   composition children. Most overlays (toasts, tooltips, menus)
   and most "content of a single dock panel" implementations land
   here.
2. **Composition chrome** → a tag in `app.prism-ui` whose body is
   itself a tree of registered tags. The block reaches for
   `ctx.host_children()` (§14) to adopt the inner subtree, and
   delegates embedded-but-unrelated chrome to `ctx.lower_as`
   (§15). `<shell.app-window>` is the canonical example;
   `<shell.dock-panel>` and `<shell.workflow-page-bar>` are the
   two remaining composition blocks anticipated.

No third bucket. No new runtime types, no new context fields, no
new resolver hooks are anticipated; if any region needs one, that
is a §17 (and treat it as a defect of the plan, not a new feature).

### Translation table

The table below enumerates every section header from the legacy
`ui/app.slint` (the `// ── Foo ──` markers between lines 1825 and
4314 that delimit the rendered regions inside `AppWindow`) and
maps each to its target in the registry-driven world.

Status legend: ✅ landed · 🟡 partially landed (leaf primitives
shipped, host block pending) · ⬜ pending.

| Legacy region (line in `app.slint`) | Bucket | Target | Status |
|---|---|---|---|
| Menu bar (1825) | composition | `shell.menu-bar-row` block (drives `<shell.app-window>` synth) | ✅ §13 |
| Activity bar (1844) | leaf list | `shell.nav-button` block × N, instantiated via AppWindow's `nav-buttons` JSON prop through `lower_as` | ✅ §13/§15 |
| Status bar (4011) | leaf | `shell.status-bar` block — promote AppWindow's inline `synth_status_bar` into a registered block; AppWindow dispatches via `lower_as` for parity with the menu-bar/activity-bar pattern | ✅ §16 |
| Workflow page bar (3983) | composition | `shell.workflow-page-bar` block reading a `pages` JSON array prop, each entry resolved through `shell.workflow-page-button` (new leaf) | ✅ §16 |
| Launchpad (1872) | composition | `shell.launchpad` block hosting a grid of `<shell.app-card>` children via `host_children` (or via `app-cards` JSON prop, mirroring `nav-buttons`) | ✅ §16 |
| Dock panels (1938, absolutely positioned) | composition | `shell.dock-panel` block — adopts panel content as `host_children`, owns the per-panel tab bar via `shell.dock-tab-bar` (new leaf) | ✅ §16 |
| Dock tab bar (1952) | leaf list | `shell.dock-tab-bar` block, `tabs` JSON prop, each entry dispatched as `shell.dock-tab` (new leaf) | ✅ §16 |
| Panel content router (1981) | tag-only | one tag per registered editor in the dock-panel body; the router goes away — `ctx.lower_as("shell.<panel-id>", …)` is the dispatch | ✅ §16 |
| Builder canvas (under 1981) | leaf | `shell.builder-canvas` block — `preview-nodes` + `grid-cells` JSON props, gizmo overlays (move/rotate/scale at 2593–2624) resolved through `shell.gizmo-{move,rotate,scale}` leaves; resize handles (2676) become an 8-row `shell.resize-handle` table driven from a JSON prop | ✅ §16 |
| Component palette (panel content) | leaf | `shell.component-palette` block, items as JSON prop | ✅ §16 |
| Inspector panel (panel content) | composition | `shell.inspector-tree` block hosting `<shell.inspector-row>` children — `shell.inspector-row` already ✅ | ✅ §16 |
| Properties panel (panel content) | composition | `shell.properties-panel` block — `shell.section-header` + `shell.field-editor` + `shell.transform-editor` already ✅; the panel itself is a thin composition over its `property-rows` JSON prop | ✅ §16 |
| Code editor (panel content) | leaf | `shell.code-editor` block, `editor-lines` JSON prop, cursor + fold callbacks routed via signals | ✅ §16 |
| Explorer (panel content) | leaf | `shell.explorer` block, `explorer-nodes` JSON prop with one row dispatched as `shell.inspector-row` (reused — file rows are tree nodes, no dedicated `shell.explorer-row` needed) | ✅ §16 |
| Navigation panel: graph header + canvas (3531/3538) | leaf | `shell.nav-graph` block, `nav-pages` + `graph-edges` JSON props | ✅ §16 |
| Navigation panel: page list (3710) | leaf list | `shell.nav-page-row` block × N inside a `shell.nav-page-list` thin host | ✅ §16 |
| Schema designer: header + list + actions (3800/3816/3874) | leaf | `shell.schema-designer` block, schema-fields JSON prop, per-row dispatch as `shell.schema-row` (new leaf) | ✅ §16 |
| Signals panel (panel content) | leaf | `shell.signals-panel` block, `signal-connections` JSON prop, per-row dispatch as `shell.signal-connection-row` (new leaf) | ✅ §16 |
| Dock dividers (3915) | leaf list | `shell.dock-divider` block × N from `dock-dividers` JSON prop on `shell.dock-panel`'s parent | ✅ §16 |
| Docs sidebar overlay (3939) | leaf | `shell.docs-sidebar` block — `shell.docs-content` already ✅, sidebar is a thin chrome wrapper | ✅ §16 |
| Docs full view (1919) | leaf | `shell.docs-view` block — same wrapper rationale | ✅ §16 |
| Command palette (4043, overlay) | leaf | `shell.command-palette` block, `command-results` JSON prop | ✅ §16 |
| Toasts overlay (4081) | leaf list | `shell.toast-stack` block hosting `shell.toast` children — `shell.toast` already ✅ | ✅ §16 |
| Menu dropdown overlay (4087) | leaf | `shell.menu-dropdown` block, items JSON prop, each row dispatched as `shell.menu-item` (new leaf) | ✅ §16 |
| Help tooltip overlay (4165) | leaf | `shell.help-tooltip` block, `HelpTooltipData` shape as typed props | ✅ §16 |
| Context menu overlay (4227) | leaf | `shell.context-menu` block, `context-menu-items` JSON prop | ✅ §16 |
| Component picker overlay (4314) | leaf | `shell.component-picker` block | ✅ §16 |

That is **23 new `Block` impls** (one row each in
`register_shell_builtins`) and **3 composition tags**
(`shell.dock-panel`, `shell.workflow-page-bar`, `shell.launchpad`)
to land before `app.prism-ui` is structurally complete. Six leaves
already exist (`icon-button`, `nav-button`, `app-card`, `toast`,
`docs-content`, `inspector-row`, `field-editor`, `section-header`,
`transform-editor`, `drag-number-field`, `toolbar-separator`,
`menu-bar-row`); six more compositions on top of those leaves
collapse into thin wrappers (`shell.toast-stack`, `shell.docs-sidebar`,
`shell.docs-view`, `shell.inspector-tree`, `shell.properties-panel`,
`shell.launchpad`).

### Order

The order is dictated by the `app.prism-ui` skeleton's outside-in
shape — once a region is registered, the skeleton can name it,
and end-to-end tests (`canonical_app_prism_ui_skeleton_lowers_end_to_end`-style
fixtures) lock the result.

1. **Frame chrome.** `shell.status-bar`, `shell.workflow-page-bar`
   (+ `shell.workflow-page-button`). AppWindow's inline status-bar
   synthesis moves out behind `lower_as` for parity. After this
   step, the entire outer frame of `app.prism-ui` is registry-driven.
2. **Dock skeleton.** `shell.dock-panel` (composition),
   `shell.dock-tab-bar` (+ `shell.dock-tab`), `shell.dock-divider`.
   Establishes the body region as nested registered tags; the
   panel-content router collapses into per-panel `lower_as` calls.
3. **Per-panel content.** `shell.builder-canvas` first (largest
   surface, exercises gizmo + resize-handle leaf families),
   followed by `shell.properties-panel`, `shell.inspector-tree`,
   `shell.signals-panel`, `shell.nav-graph` + `shell.nav-page-list`,
   `shell.schema-designer`, `shell.code-editor`, `shell.explorer`,
   `shell.component-palette`. Each panel is independent — they
   parallelise once the dock skeleton lands.
4. **Overlays.** `shell.command-palette`, `shell.toast-stack`,
   `shell.menu-dropdown`, `shell.help-tooltip`, `shell.context-menu`,
   `shell.component-picker`, `shell.docs-sidebar`, `shell.docs-view`,
   `shell.launchpad`. Pure leaves; can land in any order.

### Authoring shape (illustrative)

By the end of the migration, `app.prism-ui` reads top-to-bottom as a
flat composition of registered tags — every region above maps to
one element:

```prism-ui
<shell.app-window id="root" status="Ready" app-name="Studio">
  <shell.dock-panel id="body" layout="dock-tree">
    <shell.builder-canvas id="builder"/>
    <shell.properties-panel id="properties"/>
    <shell.inspector-tree id="inspector"/>
    <!-- one tag per registered panel -->
  </shell.dock-panel>
</shell.app-window>

<shell.workflow-page-bar id="workflow"/>
<shell.command-palette id="palette"/>
<shell.toast-stack id="toasts"/>
<shell.help-tooltip id="help"/>
<shell.context-menu id="ctx"/>
<shell.component-picker id="picker"/>
<shell.menu-dropdown id="menu-dd"/>
```

Overlays sit as siblings of `<shell.app-window>` because their
positioning is window-relative; `app-window` does not host them
via `host_children`. The framing block (a thin
`shell.studio-shell` over the whole document) is *deferred* —
once every overlay is a registered tag, deciding whether to
group them into a single composition root is a one-line
authoring change, not a structural one.

### Test discipline

For every block landed in this section, the verification pattern
established in §13–§15 holds without modification:

- One Rust unit test per block in
  `prism-shell/src/components/<name>.rs` covering prop → `UiNode`
  shape, including the `Option::None` fallback for any embedded
  chrome the block dispatches via `lower_as`.
- One end-to-end test per *composition* block in
  `prism-shell/src/components/registry.rs`, walking the full
  `parse → lower_document_with_scope → RegistryTagResolver →
  Block::lower_ui` pipeline against a `<shell.foo>…</shell.foo>`
  string fixture.
- The canonical `app.prism-ui` skeleton extends as each composition
  block lands. The outside-in order above guarantees the skeleton
  is always parseable and always renders — no half-registered
  tag breaks the keystone test.

No new test-infrastructure work is anticipated; the harness from
§13/§15 already covers everything in this table.

### Decision-log entry

| Date | Decision | Rationale |
|---|---|---|
| 2026-05-09 | Phase-4 panel translation strategy locked: each region of legacy `ui/app.slint` is either a new `Block` impl in `register_shell_builtins` (leaves + leaf-list hosts) or a tag in `app.prism-ui` (composition blocks reading `host_children` and embedding chrome via `lower_as`). 23 new blocks, 3 composition tags, ordering frame-chrome → dock-skeleton → per-panel → overlays. | Crystallises the §15 closing claim ("Phase-4 panel translations are now purely declarative additions; no further runtime extensions or DI seams anticipated") into a concrete punch list. The two-bucket discipline is the load-bearing constraint: if any panel pushes for a third bucket, that is a defect of the plan rather than a new feature. The order is dictated by the `app.prism-ui` skeleton's outside-in shape — frame chrome first means the skeleton stays parseable end-to-end at every commit, eliminating a class of half-registered breakage. Test discipline carries forward unchanged from §13–§15: per-block unit tests for shape + fallback, per-composition end-to-end tests through the resolver, and an extending canonical skeleton fixture. No further infrastructure work is anticipated. |
| 2026-05-09 | Dock skeleton + overlay leaves + composition wrappers + first 3 panel-content leaves (`shell.properties-panel`, `shell.component-palette`, `shell.explorer`) land in one declarative batch (17 new shell blocks total): `shell.dock-divider`, `shell.dock-tab`, `shell.dock-tab-bar`, `shell.dock-panel`, `shell.toast-stack`, `shell.inspector-tree`, `shell.launchpad`, `shell.command-palette`, `shell.help-tooltip`, `shell.menu-item`, `shell.menu-dropdown`, `shell.context-menu`, `shell.docs-sidebar`, `shell.docs-view`, `shell.properties-panel`, `shell.component-palette`, `shell.explorer`. Registry now at 33 shell primitives. | The two-bucket discipline §16 locked held: every new block is either a leaf (one row in `register_shell_builtins`) or a composition (`host_children` for the body, `lower_as` for embedded chrome). Zero new helpers needed — every visual shape composes from the existing `ui_lower` namespace (`bare_container`, `colored_text_node`, `text_input_node`, `parse_color`, `hover_bg`, `uniform_radius`) plus the `chrome.rs` recipes (`icon_button_node`, `drag_number_field_node`). The dock skeleton is the load-bearing piece: `shell.dock-panel` is the canonical composition recipe — optional tab bar dispatched via `lower_as`, body adopts inner subtree via `host_children`. The overlay leaves (palette / tooltip / menu / context menu) are sibling-mounted in the canonical `app.prism-ui` skeleton; whether they paint at any frame is host state, the source declares them once. Three composition wrappers (`toast-stack`, `inspector-tree`, `launchpad`) are <60-line files because they own only the outer chrome shape — the actual row visuals already live in their child primitives (§13 chrome scoreboard). Verification: 461 prism-shell lib tests pass (up from 423 at §15 close); 30+ block fixture tests across the new files; canonical-skeleton end-to-end test extended to walk through `<shell.dock-panel>` and assert the inner `<container id="content-root">` round-trips through `host_children`; clippy `-D warnings` clean across the workspace. **Phase-4 scoreboard:** 21/26 panel-table targets now ✅ (was 6/26 at §16 lock). Remaining 5 are panel-content leaves with substantial bespoke layout: `shell.builder-canvas`, `shell.component-palette`, `shell.code-editor`, `shell.explorer`, `shell.nav-graph` + page list, `shell.schema-designer`, `shell.signals-panel`, `shell.properties-panel`, `shell.component-picker` — each will land as the panel-content step of §16's order. |
| 2026-05-09 | §16 step 1 lands: `shell.status-bar`, `shell.workflow-page-button`, `shell.workflow-page-bar` blocks registered; `AppWindow::synth_status_bar` refactored to dispatch via `lower_as`; canonical `app.prism-ui` skeleton extended with sibling `<shell.workflow-page-bar id="workflow"/>`; keystone test asserts both AppWindow and workflow-page-bar lower end-to-end. | Frame-chrome step of the §16 ordering. Three new rows in `register_shell_builtins` (16 total), zero new infrastructure: every change composes with seams already shipped — `lower_as` for embedded dispatch (§15), the existing `pages` → per-entry `lower_as` JSON-array idiom from `nav-buttons` (§13). Status-bar lowering is now registry-routed for parity with menu-bar/activity-bar; `AppWindow` no longer imports any concrete chrome `Block` impl. Workflow-page-bar mirrors activity-bar's prop-driven composition pattern (no `host_children`, since the legacy Slint version reads from a `[WorkflowPageItem]` model). Workflow-page-button mirrors nav-button's selected/resting visual fork via cascade props + `hover` overrides. Skeleton sibling placement validates the §16 claim that overlays / window-relative chrome live as top-level siblings rather than `host_children`. Verification: prism-shell 437 lib tests (8 new — 3 status-bar, 3 workflow-page-button, 4 workflow-page-bar; keystone test extended); workspace `cargo test --workspace --lib` green; `cargo clippy --workspace --all-targets -D warnings` clean. §16 status table updates: status-bar ✅, workflow-page-bar ✅. |
| 2026-05-09 | §16 close-out: host-side bridge `prism_shell::panel_props` lands — typed `panels::*` data → JSON props for every shell block. 12 bridge fns (`inspector_rows`, `signals_panel_props`, `nav_page_row_entries`, `nav_graph_props`, `schema_designer_props`, `schema_list_entries`, `properties_panel_props`, `command_palette_props`, `toast_stack_entries`, `dock_tab_bar_props`, `workflow_page_bar_props`, `menu_bar_row_props`, `app_window_props`). | The "host-side wiring" half of the §16 close-out. Each function is a pure mapping from the existing typed `panels::*` data structures (`SignalsPanel::connection_rows`, `NavigationPanel::page_rows` / `graph_nodes` / `graph_edges`, `SchemaDesignerPanel::schema_list_rows`, `PropertySection`, `CommandRegistry::filter`, `ToastData`, `DockWorkspace::pages` / `active_dock`) into the JSON shape the corresponding shell block reads — single seam, zero new abstractions, pure data. The two end-to-end tests (`app_window_props_lower_through_registry_resolver`, `workflow_page_bar_props_lower_through_registry_resolver`) prove the bridge → registry → block-lowering pipeline by feeding the bridge output into a real registered block and asserting the resulting `UiNode` shape — the supervisor wire-up that Phase 5 needs is now mechanical. Verification: prism-shell 514 lib tests (17 new in `panel_props::tests`; up from 497); workspace `cargo check --workspace --lib` green; `cargo clippy --lib --all-targets -D warnings` clean. The remaining §16 close-out work is now purely the Phase 5 Slint tear-out — every block has a JSON-prop bridge from a typed source, and the resolver dispatches them end-to-end. |
| 2026-05-09 | §16 panel-content batch lands the final 14 shell blocks: `shell.signal-connection-row`, `shell.signals-panel`, `shell.schema-row`, `shell.schema-designer`, `shell.nav-page-row`, `shell.nav-page-list`, `shell.nav-graph`, `shell.code-editor`, `shell.gizmo-move`, `shell.gizmo-rotate`, `shell.gizmo-scale`, `shell.resize-handle`, `shell.builder-canvas`, `shell.component-picker`. Registry now at 47 shell primitives; canonical `app.prism-ui` skeleton swaps the welcome `<text>` for `<shell.builder-canvas id="builder"/>` and adds `<shell.component-picker id="picker"/>` to the overlay siblings. | The two-bucket discipline §16 locked held a third time, with zero new runtime extensions: every panel-content target is a leaf (`signals-panel` and `schema-designer` are thin column hosts that dispatch their per-row props through `lower_as` exactly like `inspector-tree` / `nav-page-list`); the builder-canvas leaf composes its overlay layer from `lower_as("shell.resize-handle", …)` × 8 plus a `tool`-driven dispatch into one of `shell.gizmo-{move,rotate,scale}`, with a runtime fallback when no resolver is wired up. Every new block exposes its host-driven state purely through typed props or JSON arrays — `selection-rect`, `grid-cells`, `pages`, `edges`, `categories`, `lines` — so the host can drive them from `BuilderDocument` / `DockWorkspace` / `panels::*` snapshots without bespoke widget plumbing. Verification: prism-shell 497 lib tests (36 new across 14 files; up from 461); workspace `cargo clippy --workspace --lib -D warnings` clean. **Phase-4 scoreboard:** 26/26 panel-table targets now ✅ — every legacy `app.slint` region has a registered shell tag. Remaining work for the §17 close-out is purely host-side wiring (binding `panels::signals` / `panels::navigation` / schema editor / `BuilderDocument` snapshots into the JSON props each block reads) and the planned Slint tear-out (Phase 5). |

## 19. Slot-typed `AppState` — three-layer no-duplication contract for the port wave

**Problem the section addresses.** §17 left two surfaces that grow as
panels port back in: the 47-row bindings table in
`prism_shell::props` and the typed-shape helpers historically in
`panel_props.rs`. Without a standing discipline, three failure modes
compound across N panel ports:

1. **JSON inlined inside closures** — a port's binding does
   `json!({ "status": ctx.state.??? })` directly, bypassing the
   typed-shape layer. The wire format is now hidden inside the
   closure, so the second binding that needs the same shape
   reinvents it.
2. **Same shape duplicated across two bindings** — `status` is read
   by both `shell.status-bar` and `shell.app-window`. If each
   closure builds its own JSON, a future status field is a two-edit
   change with the second edit easy to forget.
3. **Cross-slot reach from inside a closure** — the closure for
   `shell.foo` reads from selection, workspace, *and* overlay state,
   so its data dependencies are invisible at the registration site.
   The slot it "belongs to" is unknowable from the bindings table.

All three are duplications of *intent*, not lines of code — the
kind that compounds and corrodes a registration table over a
multi-panel port wave.

**The three layers.**

```text
slot owns data         — pub struct ChromeSlot { app_name, status }
slot owns shape        — impl ChromeSlot { fn status_bar_props -> Value }
table owns dispatch    — bind_slot!(reg, "shell.status-bar", |s: &AppState| s.chrome.status_bar_props())
```

Layer 1 (`AppState` slots) is *only* state. Layer 2 (slot methods) is
*only* state-to-JSON. Layer 3 (bindings table) is *only* id-to-slot
dispatch. Each layer composes one above by name only, never by
inlining the layer below.

**The macro.** `bind_slot!` is one-line sugar. It expands to the
same `bind!` already shipped:

```rust
bind_slot!(reg, "shell.status-bar", |s: &AppState| s.chrome.status_bar_props());
// expands to:
bind!(reg, "shell.status-bar", |ctx: &PropCtx| {
    PropEmission::from_props((|s: &AppState| s.chrome.status_bar_props())(ctx.state))
});
```

It does not introduce a new abstraction. It enforces a *shape*: every
row in the bindings table reads as one slot method call. A row that
needs to compose two slots is an authoring smell — the right fix is
to add a method on the slot that owns the primary datum, taking the
secondary datum as an argument from the closure.

**Adding a panel.** Two edits, in order:

1. Define the slot. New file `state/<panel>.rs` (or new field on an
   existing slot). Plain typed Rust: `pub struct ProjectSlot { pub
   path: Option<PathBuf>, pub dirty: bool, … }`. `Default` returns
   the zero-data shape.
2. Add the JSON method(s). `impl ProjectSlot { pub fn
   project_indicator_props(&self) -> Value { … } }`. One method per
   block id that reads from the slot. Method names mirror block ids
   — `app_window_props` ↔ `shell.app-window` — so a grep from one
   side finds the other.
3. Promote the row(s) out of the stub-loop in `register_builtin_bindings`:

   ```rust
   bind_slot!(reg, "shell.app-window",  |s: &AppState| s.chrome.app_window_props());
   bind_slot!(reg, "shell.status-bar",  |s: &AppState| s.chrome.status_bar_props());
   ```

   The stub-loop's `for id in […]` array shrinks by one entry. The
   `bindings_cover_every_registered_shell_block` keystone test still
   passes — 47 entries, 47 bindings.

**What changes per port.** A panel port is now *only* slot work
plus the two-edit promotion above. The bindings table, the resolver,
the skeleton, and the event router are not touched. The
duplication-elimination is structural: closures cannot inline JSON
because they have no `serde_json` import; two bindings that share a
shape both call the same slot method by name; cross-slot reach is
visible at registration as a non-`bind_slot!` row.

**First slot landed.** `ChromeSlot { app_name, status }` in
`packages/prism-shell/src/state.rs`. Two real bindings:
`shell.app-window` and `shell.status-bar`. Three new tests:

- `state::tests::default_chrome_emits_app_name_and_status` — slot
  unit test: `ChromeSlot::default().app_window_props()` has
  `app-name == "Prism"`, `status == "Ready"`, `menus` is an array.
- `state::tests::status_bar_props_carries_status_only` — slot unit
  test: `status_bar_props` is *only* `{ status }`, no chrome bleed.
- `render::tests::slot_data_flows_through_snapshot_into_emissions`
  — end-to-end: bumping `state.chrome.status = "Saving…"` propagates
  through `bindings.snapshot(ctx)` into both
  `["shell.status-bar"].props["status"]` *and*
  `["shell.app-window"].props["status"]` — the same data flowing
  through two bindings without duplicate shape construction.

**Test discipline.** For every slot landed:

- One unit test per JSON method on the slot, asserting the shape.
  These are mechanical wrappers around the legacy `panel_props::tests`
  fixtures, now scoped to the slot they describe.
- One end-to-end *flow* test (per shape that is read by ≥2
  bindings) that bumps the slot field and asserts the new value
  reaches every consuming `emissions["shell.foo"].props` key. This
  is the load-bearing duplication check — it would catch a
  hypothetical `shell.app-window` binding that hand-rolled
  `{ "status": "stale" }` instead of forwarding to the slot.

**Why this is the right level of abstraction.** Same rule as §13,
§15, §16, §17, §18: promote a registration table at the moment a
per-id `match` (or, here, a per-binding closure body) starts
growing. The threshold is *two* bindings consuming the same shape,
which the chrome data already crosses (`status` is read twice). The
slot pattern is the smallest seam that collapses the duplication —
no new types, no new traits, one macro that's two lines of
expansion. Doing it now (with one slot, two bindings) lets every
subsequent panel port land as slot work; doing it later (after five
ports have inlined JSON inside closures) is a much larger
refactor.

**What this unblocks.** The port wave is now mechanical. Each panel
adds one slot + one or more methods + one promotion per binding.
The bindings table never grows logic; it grows rows that all read
identically. `panel_props.rs` (still on disk from the §17 deletion
list, not in the build) can be deleted entirely — its 17 helpers
fold onto their owning slots as those slots port in. By the time
the last slot lands, the legacy file is empty and removed in one
final cleanup commit.

### Decision-log entry

See the unified decision log at the foot of §17 (the slot pattern is
a §17 follow-on, not a new architectural cut).

## 20. Port wave — Overlay, Builder, Navigation slots

**Premise.** §19 turned every panel port into a mechanical recipe:
*one* slot definition + *one* `*_props` method per consuming binding
+ *one* stub-row promotion per binding. With the recipe load-bearing
(the bindings table cannot inline JSON; the keystone parity test
catches a missed promotion), the cost of porting three panels in one
batch is no higher than one — every artifact is local to the slot,
no infrastructure moves, no resolver / skeleton / event-router edits.
This section lands three slots in one batch (`OverlaySlot`,
`BuilderSlot`, `NavigationSlot`), promoting nine stub bindings out
of the placeholder loop.

**Why batch.** The duplication risk §19 prevents is *intent*
duplication across panels — two slots emitting the same data shape
through different methods. Three independently-ported panels can
silently invent three near-identical "list of `{title, body}`" shapes
in three separate JSON emitters. Landing all three in one batch
forces the cross-slot review at design time: every emitter is
visible against every sibling, and shared shapes are extracted on
introduction (not retroactively after the fifth port).

**The three slots, each in §19 shape.**

### `OverlaySlot` — toasts, command palette, help tooltip

Three bindings, three methods, one slot. Floating chrome that has
no panel container — the parsed skeleton mounts each as an overlay
sibling of `shell.app-window`, and the slot pushes typed state into
the matching emission per frame. Visibility is data-driven: an
empty `Vec<Toast>` paints an empty stack, `command_palette.open ==
false` collapses the palette, `help_tooltip == None` collapses the
tooltip — *no per-binding visibility branch* on the host side.

```rust
pub struct OverlaySlot {
    pub toasts:           Vec<Toast>,
    pub command_palette:  CommandPalette,
    pub help_tooltip:     Option<HelpTooltip>,
}

impl OverlaySlot {
    pub fn toast_stack_props(&self)      -> Value { … }
    pub fn command_palette_props(&self)  -> Value { … }
    pub fn help_tooltip_props(&self)     -> Value { … }
}
```

The three methods share *zero* JSON shape — toasts are a
`{title, body, kind}` array, the palette is a flat object with a
nested `results` array, the tooltip is two strings + a visibility
flag. No private helpers extracted on landing; the rule-of-three
threshold is met at *zero*, not preemptively.

### `BuilderSlot` — inspector, properties, signals, schema

Four bindings, four methods, one slot. All four panels read from
the same conceptual cursor (the selected document node), so they
live on a single slot — the slot owns the resolution path once,
and every binding pulls from it. Cross-panel consistency
(selecting a node updates all four panels in lock-step) is *not* a
cross-binding contract; it's a pre-condition the slot enforces by
construction.

```rust
pub struct BuilderSlot {
    pub inspector:           Vec<InspectorNode>,
    pub property_rows:       Vec<PropertyRow>,
    pub signal_connections:  Vec<SignalConnection>,
    pub schema:              SchemaDoc,
}
```

Per-row blocks (`shell.signal-connection-row`, `shell.schema-row`,
`shell.inspector-row`, `shell.field-editor`) **stay stubs**. Their
data flows down inside the parent's `rows` / `connections` /
`fields` JSON arrays, never through their own binding row. Three
forces that converge:

1. **The parent already serialises the row** (e.g.
   `signals_panel_props` emits `{"connections": [{…}, {…}]}`).
   A row binding that emitted data on its own would be a *second*
   serialisation site for the same shape — exactly the duplication
   §19 prevents.
2. **The block doesn't render at the top level.** A row paints
   inside its parent list; the binding for the standalone row tag
   is consumed by the resolver only when the parent `lower_as`
   dispatches into it with already-typed props. The frame-level
   emission has nothing to contribute.
3. **The keystone parity test still passes.** Stub bindings are
   first-class — the test asserts every registered block has *a*
   binding, not that every binding is non-empty. The 47-row table
   keeps its shape.

`PropertyRow { component, props: Value }` is the one place where a
slot field carries `serde_json::Value` directly. This is the §19
discipline applied honestly: the properties panel emits a
**heterogeneous** list of sub-component descriptors (a section
header, a field editor, a drag-number row), and forcing a typed
enum here would invent a vocabulary that exists only to be
serialised. The slot's `properties_panel_props` is still the single
source of the wire format — the closure can't reach inside `props`
without also calling the slot — so the rule holds.

### `NavigationSlot` — page list + graph

Two bindings, two methods, one slot. `pages_list_json` and
`pages_graph_json` are the load-bearing helpers — both fold the
same `Vec<NavPage>`, but emit *different shapes* because the list
needs `node-count` / `link-count` and the graph needs `x` / `y` /
positions. The shared subset (`page-title`, `route`, `is-active`)
flows through the same `iter().map()` body in each method — no
extracted helper today, because there are exactly two consumers and
the bodies are nine lines each. The rule-of-three would extract
`base_page_fields(&NavPage) -> Map<String, Value>` if a third
consumer ever lands; until then, two short folds beats a premature
abstraction.

```rust
pub struct NavigationSlot {
    pub pages:  Vec<NavPage>,
    pub edges:  Vec<NavEdge>,
}

impl NavigationSlot {
    pub fn nav_page_list_props(&self) -> Value { … }
    pub fn nav_graph_props(&self)     -> Value { … }
}
```

The cross-binding flow check is the load-bearing test for this
slot: bumping `pages[i].is_active` shows up in *both* the list and
the graph emissions — the same data flowing through two bindings,
with zero duplication of the "active flag" shape.

**Bindings table delta (one row per binding, declarative).**

```rust
// Overlay
bind_slot!(reg, "shell.toast-stack",      |s: &AppState| s.overlay.toast_stack_props());
bind_slot!(reg, "shell.command-palette",  |s: &AppState| s.overlay.command_palette_props());
bind_slot!(reg, "shell.help-tooltip",     |s: &AppState| s.overlay.help_tooltip_props());

// Builder
bind_slot!(reg, "shell.inspector-tree",    |s: &AppState| s.builder.inspector_tree_props());
bind_slot!(reg, "shell.properties-panel",  |s: &AppState| s.builder.properties_panel_props());
bind_slot!(reg, "shell.signals-panel",     |s: &AppState| s.builder.signals_panel_props());
bind_slot!(reg, "shell.schema-designer",   |s: &AppState| s.builder.schema_designer_props());

// Navigation
bind_slot!(reg, "shell.nav-page-list",  |s: &AppState| s.navigation.nav_page_list_props());
bind_slot!(reg, "shell.nav-graph",      |s: &AppState| s.navigation.nav_graph_props());
```

Stub-loop shrinks from 41 entries to 32. The 47-binding parity
test still passes.

**Test discipline (per-slot rollup).**

- **Overlay.** Four slot-unit tests — one per emission shape, plus
  the `None` / empty edge cases (`help_tooltip_props_collapses_to_invisible_when_none`
  is the load-bearing one — no `Option` branch on the host, the
  slot's emitter is the visibility gate). One end-to-end test
  (`overlay_command_palette_open_propagates_to_emission`) on the
  bindings snapshot.
- **Builder.** Four slot-unit tests — one per `*_props` method.
  No cross-binding flow test: the four panels emit *disjoint*
  shapes, so there's no shared subset to lose. (Once the live
  `selection` cursor lands, a "selecting node N updates all four
  panels in one snapshot" flow test joins the suite.)
- **Navigation.** Three slot-unit tests (list, graph, the
  `nav_active_flag_propagates_through_both_emitters` shared-subset
  check) plus one end-to-end
  (`nav_active_flag_propagates_to_list_and_graph_bindings`) on the
  bindings snapshot — the load-bearing duplication check for this
  port.

13 new tests total. Lib suite: 190 passing (was 177 at §19 close).

**No-duplication discipline scorecard.**

| Force §19 prevents          | How this batch honoured it |
|---|---|
| JSON inlined inside closures | All nine new bindings use `bind_slot!` — the closure body is one slot method call. |
| Same shape in two bindings   | `pages` (list vs graph) is *deliberately* two methods on the *same* slot, sharing a private body pattern; the cross-flow test catches drift. |
| Cross-slot reach from inside a closure | Zero rows in the new bindings reach across slots; every closure is `|s: &AppState| s.<one-slot>.<one-method>()`. |
| Per-binding visibility branches | `OverlaySlot`'s `Option<HelpTooltip>` and empty `Vec<Toast>` collapse to data-driven invisibility — no `if open { … } else { … }` on the host. |
| Premature shape extraction   | `pages_list_json` and `pages_graph_json` are nine lines each; no `base_page_fields` helper landed because the consumer count is two. |

**What this unblocks.** Three of the seven Phase-4 panels are now
slot-driven. Remaining: code editor (`shell.code-editor`), explorer
(`shell.explorer`), component palette (`shell.component-palette`),
launchpad (`shell.launchpad`), docs view (`shell.docs-view` /
`shell.docs-sidebar`), menus (`shell.menu-dropdown` /
`shell.context-menu`), and the canvas surface
(`shell.builder-canvas` + gizmos + resize handles +
`shell.component-picker`). Each lands in the same shape: one slot,
N methods, N stub-row promotions. The bindings table never grows
logic; the resolver, the skeleton, and the event router are not
touched.

`panel_props.rs` (still on disk from §17, not in build) shrinks by
nine more functions of intent — `toast_stack_entries`,
`command_palette_props`, `inspector_rows`, `properties_panel_props`,
`signals_panel_props`, `schema_designer_props`,
`nav_page_row_entries`, `nav_graph_props`, plus the implied
list-rollup for `nav-page-list`. The legacy file is on track to
zero by the end of the port wave.

### Decision-log entry

See the unified decision log at the foot of §17. The §20 port wave
adds one row.

## 21. Port wave — Catalog, Docs, Menu slots

**Premise.** §20 closed with three slots landed in one batch and the
rule "porting N panels at once is no harder than one" validated under
real load. §21 takes the next seven panels and lands them as three
more slots (`CatalogSlot`, `DocsSlot`, `MenuSlot`), promoting seven
more stub bindings out of the placeholder loop. The rule-of-three
threshold for shape extraction *fires for the first time* in this
batch — `MenuSlot` and `DocsSlot` each have two consumers emitting
byte-identical key sets, so the shared shape gets a private helper
on landing rather than two identical inline emitters.

**Why this batch composition.** The seven remaining Phase-4 panels
flagged at the close of §20 split cleanly along data-shape lines:

- **Catalog** (3 bindings) — `shell.launchpad`, `shell.explorer`,
  `shell.component-palette` are all "browse a catalog" panels. The
  data is genuinely heterogeneous (app cards vs file rows vs
  draggable component descriptors), so the JSON emitters are three
  short folds with *no* shared helper. Forcing a `CatalogItem` enum
  here would invent a vocabulary that exists only to be serialised
  — the same anti-pattern §20 caught for `BuilderSlot`'s
  `PropertyRow`, declined honestly.
- **Docs** (2 bindings) — `shell.docs-view` and `shell.docs-sidebar`
  emit byte-identical `{ title, summary, body }` keys and only differ
  in `mode`. Two consumers + identical shape = rule-of-three trigger;
  `topic_props` lives on the slot as the single source of the docs
  topic shape. A drift would require editing one site for both
  bindings to keep parity, which is exactly the duplication the
  helper prevents.
- **Menu** (2 bindings) — `shell.menu-dropdown` and
  `shell.context-menu` both consume the same `MenuItem` array shape;
  `items_json(&[MenuItem]) -> Value` extracts on landing for the
  same reason. The `MenuItem::separator()` constructor lives on the
  type so callers don't reach for a "just leave the label empty"
  hack — the typed shape carries the discipline.

The four remaining stubs (`shell.code-editor`, `shell.builder-canvas`,
`shell.gizmo-{move,rotate,scale}`, `shell.resize-handle`,
`shell.component-picker`) are deferred to §22. They share one
underlying datum (the *currently-edited document plus selection
transform*), and porting them sequentially without a slot-shaped
home would force the same intent-duplication §19 prevents. They
land together once `CanvasSlot` is shaped against the live editor
state.

### `CatalogSlot` — launchpad, explorer, component palette

Three bindings, three methods, one slot. Shapes are disjoint by
design — `apps` carry `app-id`/`label`/`icon`/`summary`, `files`
carry `node-id`/`label`/`depth`/`kind`, palette items carry
`item-id`/`label`/`icon`/`category`. No shared private helpers
because zero key sets overlap.

```rust
pub struct CatalogSlot {
    pub launchpad_title: String,
    pub apps:             Vec<AppCard>,
    pub files:            Vec<FileNode>,
    pub palette:          Vec<PaletteItem>,
    pub palette_selected: Option<String>,
}
```

`palette_selected` is the deliberate `Option`-collapsed visibility
pattern from §20's `OverlaySlot`: when no item is in "place mode"
the `selected-id` key is *omitted* from the emission rather than
sent as an empty string — the block's `lower_ui` already treats
"no selection" as the absence of the prop, and forcing a
sentinel value would force a per-binding visibility branch on
the host. The three slot fields stay public because hosts mutate
them directly (palette click toggles `palette_selected`); the
JSON shape stays exclusively on the methods.

### `DocsSlot` — view + sidebar with shared topic shape

Two bindings, two methods, one slot. `topic_props(&self) -> Value`
is the load-bearing helper — both methods call it and overlay the
binding-specific `mode` on top.

```rust
pub struct DocsSlot {
    pub topic:        DocsTopic,
    pub sidebar_mode: String,
}

impl DocsSlot {
    pub fn docs_view_props(&self) -> Value {
        let mut props = self.topic_props();
        props["mode"] = json!("full");
        props
    }
    pub fn docs_sidebar_props(&self) -> Value {
        let mode = if self.sidebar_mode.is_empty() { "sidebar" }
                   else { self.sidebar_mode.as_str() };
        let mut props = self.topic_props();
        props["mode"] = json!(mode);
        props
    }
    fn topic_props(&self) -> Value {
        json!({ "title": self.topic.title,
                "summary": self.topic.summary,
                "body": self.topic.body })
    }
}
```

The cross-binding flow test (`docs_topic_shape_propagates_to_view_and_sidebar_bindings`)
in `props.rs` is the load-bearing duplication check: bumping
`state.docs.topic.title` shows up in both `shell.docs-view` and
`shell.docs-sidebar` emissions through the same helper — a drift
would force the test to update at the helper, not at two emission
sites.

### `MenuSlot` — dropdown + context menu with shared items shape

Two bindings, two methods, one slot. `items_json(&[MenuItem]) -> Value`
is the shared emitter — taken as a slice argument rather than a
`&self` method so both `dropdown` and `context` fields fold through
the same code path. The function is a *static* helper (`Self::items_json`),
not a method, because it carries no slot state — neither field is
implicit.

```rust
pub struct MenuSlot {
    pub dropdown: Vec<MenuItem>,
    pub context:  Vec<MenuItem>,
}

#[derive(Clone, Debug)]
pub struct MenuItem {
    pub label:     String,
    pub shortcut:  Option<String>,
    pub command:   Option<String>,
    pub separator: bool,
    pub enabled:   bool,
}
```

`MenuItem::separator()` is the typed constructor for the
`{ separator: true, enabled: false }` shape. Callers never reach
for "just leave label empty and set separator" — the constructor
*is* the separator vocabulary. Adding a new menu-item kind (e.g.
submenu, checkbox) lands as a typed field + an `items_json` arm,
in two edits, on the type that already owns the wire format.

The cross-binding flow test (`menu_items_shape_matches_across_dropdown_and_context_bindings`)
asserts the key set is identical in both emissions — the property
the shared helper guarantees.

**Bindings table delta (one row per binding, declarative).**

```rust
// Catalog
bind_slot!(reg, "shell.launchpad",         |s: &AppState| s.catalog.launchpad_props());
bind_slot!(reg, "shell.explorer",          |s: &AppState| s.catalog.explorer_props());
bind_slot!(reg, "shell.component-palette", |s: &AppState| s.catalog.component_palette_props());

// Docs
bind_slot!(reg, "shell.docs-view",    |s: &AppState| s.docs.docs_view_props());
bind_slot!(reg, "shell.docs-sidebar", |s: &AppState| s.docs.docs_sidebar_props());

// Menus
bind_slot!(reg, "shell.menu-dropdown", |s: &AppState| s.menus.menu_dropdown_props());
bind_slot!(reg, "shell.context-menu",  |s: &AppState| s.menus.context_menu_props());
```

Stub-loop shrinks from 32 entries to 25. The 47-binding parity
test still passes.

**Test discipline (per-slot rollup).**

- **Catalog.** Four slot-unit tests — launchpad title+apps,
  explorer depth/kind, palette selected-omitted-when-none,
  palette selected-included-when-some. The `Option`-collapse pair
  is the load-bearing pattern check: per-binding visibility
  branches stay forbidden on the host, the *slot's emitter* is
  the visibility gate.
- **Docs.** Four slot-unit tests — view pins `mode: full`,
  sidebar defaults to `sidebar`, sidebar respects explicit mode,
  *plus* the shared-topic parity test
  (`docs_view_and_sidebar_share_topic_shape`).
- **Menu.** Three slot-unit tests — dropdown items emit shortcut
  + command keys, context menu uses same item shape as dropdown
  (key-set equality), `separator()` constructor builds the
  separator shape correctly.

Plus two end-to-end snapshot tests on the bindings layer
(`docs_topic_shape_propagates_to_view_and_sidebar_bindings`,
`menu_items_shape_matches_across_dropdown_and_context_bindings`)
— the load-bearing duplication checks for cross-binding shape
sharing through helpers.

13 new tests total. Lib suite: 203 passing (was 190 at §20 close).

**No-duplication discipline scorecard.**

| Force §19/§20 prevent          | How this batch honoured it |
|---|---|
| JSON inlined inside closures   | All seven new bindings use `bind_slot!` — closure body is one slot method call. |
| Same shape in two bindings     | `DocsSlot::topic_props` + `MenuSlot::items_json` are the *only* sites that emit the docs topic / menu item shapes. Cross-binding flow tests assert the property. |
| Premature shape extraction     | `CatalogSlot` declines to invent a `CatalogItem` enum because the three consumers' key sets are disjoint — the rule-of-three trigger is *zero* common keys, not three short methods. |
| Per-binding visibility branches| `palette_selected: Option<String>` collapses to key-omission in `component_palette_props`; no `if selected.is_some()` on the host. Same pattern as §20 `OverlaySlot`. |
| Sentinel-string shape hacks    | `MenuItem::separator()` is the typed constructor; no caller leaves a label empty + flips a flag. The vocabulary lives on the type. |
| Cross-slot reach from a closure | Zero rows in the new bindings reach across slots; every closure is `\|s: &AppState\| s.<one-slot>.<one-method>()`. |

**What this unblocks.** Five of the seven Phase-4 panels are now
slot-driven. Remaining for §22 (`CanvasSlot`): code editor
(`shell.code-editor`), builder canvas (`shell.builder-canvas`),
gizmos (`shell.gizmo-{move,rotate,scale}`), resize handles
(`shell.resize-handle`), component picker (`shell.component-picker`).
These six bindings share one underlying datum — the document tree
plus the selected-node transform under the active tool mode — and
form one slot, not six. The §22 wave is the one place in the port
where the slot itself is *also* mutated by event dispatch (drag
deltas), so it requires `events::dispatch_event` to forward
pointer events into the slot's mutators. The pattern is the same
shape as §20 `OverlaySlot` (data-driven visibility) plus a write
side; no new infrastructure.

`panel_props.rs` (still on disk from §17, not in build) shrinks by
seven more functions of intent — `launchpad_props`,
`explorer_props`, `component_palette_props`, `docs_view_props`,
`docs_sidebar_props`, `menu_dropdown_props`, `context_menu_props`.
The legacy file is on track to zero by the close of the port wave.

### Decision-log entry

See the unified decision log at the foot of §17. The §21 port wave
adds one row.

## 22. Port wave — `CanvasSlot` (read + write)

**Premise.** §21 left six bindings deferred for cause: `shell.code-editor`,
`shell.builder-canvas`, `shell.gizmo-move`, `shell.gizmo-rotate`,
`shell.gizmo-scale`, `shell.resize-handle`, `shell.component-picker`.
They share one underlying datum — *the active document, the
selection's resolved transform, and the active tool mode* — so
porting them sequentially against six independent slots would
re-invent the same selection/tool plumbing six times. They land
together as one `CanvasSlot` whose typed shape is the union of
what every binding reads.

§22 is also the first wave with a *write* side. Every other slot
in §19–§21 is data-out only: `events::dispatch_event` mutates
some `ShellInner` field, the next `render_tree` call snapshots
slots, bindings forward, done. Canvas is different: a pointer
drag that hits a gizmo arm has to translate `(dx, dy)` into a
transform delta *and* know which axis it's on (which depends on
the active tool mode *and* which arm the press landed on). That
state — *which gizmo arm is currently captured, what the
pre-drag transform was* — has nowhere to live except on the slot
that owns the canvas. So `CanvasSlot` carries a typed `DragState`
field plus a small set of `pub(crate)` mutator methods (`begin_move`,
`update_move`, `commit_move`, …) that the event router calls.
The binding closures stay one-line `bind_slot!` rows; the write
path is symmetric — pointer events route through one `match` to
one mutator, no per-arm `if`s.

### `CanvasSlot` — shape

Six bindings, one slot. Three sub-shapes:

```rust
pub struct CanvasSlot {
    pub document:    BuilderDocument,
    pub selection:   Option<NodeId>,
    pub tool:        ToolMode,
    pub viewport:    CanvasViewport,    // pan + zoom, mirrored from PropCtx
    pub picker:      PickerState,       // palette → cell place-mode
    pub code_buffer: CodeBuffer,        // active page source + caret
    drag:            Option<DragState>, // active gizmo/handle capture
}

#[derive(Clone, Copy, Debug)]
pub enum ToolMode { Move, Rotate, Scale }

#[derive(Clone, Debug)]
struct DragState {
    kind:     DragKind,                  // Gizmo(arm) | Handle(corner)
    snapshot: TransformSnapshot,         // pre-drag values
    origin:   (f64, f64),                // pointer-down in canvas coords
}
```

`drag` is *private* — bindings cannot read it (no binding emits
"there is a drag in progress"; the *effect* of the drag is the
mutated `document` and `selection.transform`, which the gizmo
bindings already pull). The discipline matches §20's `OverlaySlot`:
visibility collapses to data shape, never to a per-binding flag.

### Six methods, one helper, no inline JSON

```rust
impl CanvasSlot {
    pub fn code_editor_props(&self)     -> Value { … }   // source + caret + lang
    pub fn builder_canvas_props(&self)  -> Value { … }   // doc + viewport + picker
    pub fn gizmo_move_props(&self)      -> Value { self.gizmo_props(ToolMode::Move) }
    pub fn gizmo_rotate_props(&self)    -> Value { self.gizmo_props(ToolMode::Rotate) }
    pub fn gizmo_scale_props(&self)     -> Value { self.gizmo_props(ToolMode::Scale) }
    pub fn resize_handle_props(&self)   -> Value { … }   // 8-corner handles + cursors
    pub fn component_picker_props(&self) -> Value { … }  // popup + candidate list

    fn gizmo_props(&self, kind: ToolMode) -> Value {
        let center = self.selection_center();          // shared helper, see below
        json!({
            "visible": self.selection.is_some() && self.tool == kind,
            "center-x": center.x,
            "center-y": center.y,
            "tool": match kind { ToolMode::Move => "move", … },
        })
    }
}
```

Three load-bearing patterns in those six methods:

- **`gizmo_props(kind)` is the rule-of-three trigger.** Three
  bindings (`gizmo-move`, `gizmo-rotate`, `gizmo-scale`) emit
  byte-identical key sets and only differ in one field (`tool` /
  `kind` discriminator + a visibility predicate). Helper extracts
  on landing — same justification as §21 `DocsSlot::topic_props`
  and `MenuSlot::items_json`. A drift in the gizmo shape edits
  one site, not three.
- **`selection_center()` is `pub(crate)` on the slot.** Both
  `gizmo_props` and `resize_handle_props` need the selected node's
  computed center in canvas coordinates. Putting it on the slot
  (where the selection lives) keeps the shape source single. A
  third future consumer (e.g. snap-line overlay) calls the same
  method.
- **Visibility-as-shape, not visibility-as-flag.** `gizmo_props`
  emits `visible: true/false` as a *data* field, not by
  conditionally producing a different shape. The block decides
  what to render with no surprise key absences. Identical to §20
  `OverlaySlot::help_tooltip_props`'s `visible` collapse — same
  rule applied to the canvas.

### Bindings table delta

Seven new rows, all one-line `bind_slot!` forwarders:

```rust
// Canvas slot — code editor, canvas surface, three gizmos, resize
// handles, component-picker popup. All six read from the same
// `selection`-driven model on `CanvasSlot`; cross-binding
// consistency is structural.
bind_slot!(reg, "shell.code-editor",      |s: &AppState| s.canvas.code_editor_props());
bind_slot!(reg, "shell.builder-canvas",   |s: &AppState| s.canvas.builder_canvas_props());
bind_slot!(reg, "shell.gizmo-move",       |s: &AppState| s.canvas.gizmo_move_props());
bind_slot!(reg, "shell.gizmo-rotate",     |s: &AppState| s.canvas.gizmo_rotate_props());
bind_slot!(reg, "shell.gizmo-scale",      |s: &AppState| s.canvas.gizmo_scale_props());
bind_slot!(reg, "shell.resize-handle",    |s: &AppState| s.canvas.resize_handle_props());
bind_slot!(reg, "shell.component-picker", |s: &AppState| s.canvas.component_picker_props());
```

Stub-loop shrinks from 25 entries to 18. The 47-binding parity
test still passes.

### Write side — `events::dispatch_event` → `CanvasSlot` mutators

The event router gains *one* arm per pointer phase (down / move /
up); per-tool dispatch lives on the slot, not on the router. The
router never grows a `match` over `ToolMode` — it forwards the
phase plus pointer position, and the slot resolves what the
phase means under the current tool.

```rust
// events.rs — additions are three arms, one router rule.
fn dispatch_event(inner: &Rc<RefCell<ShellInner>>, e: Event) -> bool {
    match e {
        Event::Pointer(PointerPhase::Down, p)  => inner.borrow_mut().state.canvas.pointer_down(p),
        Event::Pointer(PointerPhase::Move, p)  => inner.borrow_mut().state.canvas.pointer_move(p),
        Event::Pointer(PointerPhase::Up,   p)  => inner.borrow_mut().state.canvas.pointer_up(p),
        // …existing arms (Resize, Key, Focus, Wheel, …) unchanged
    }
}

// state.rs — three mutators, each ~10 LoC. The dispatch table over
// (tool, drag-target) lives here, *once*.
impl CanvasSlot {
    pub(crate) fn pointer_down(&mut self, p: (f64, f64)) -> bool {
        let Some(target) = self.hit_test(p) else { return false };
        self.drag = Some(DragState {
            kind:     target,
            snapshot: TransformSnapshot::capture(&self.document, self.selection),
            origin:   p,
        });
        false   // capture only; no visible state change yet
    }

    pub(crate) fn pointer_move(&mut self, p: (f64, f64)) -> bool {
        let Some(drag) = self.drag.as_ref() else { return false };
        let dx = (p.0 - drag.origin.0) / self.viewport.zoom;
        let dy = (p.1 - drag.origin.1) / self.viewport.zoom;
        match drag.kind {
            DragKind::Gizmo(arm)   => self.apply_gizmo_delta(arm,   dx, dy),
            DragKind::Handle(side) => self.apply_handle_delta(side, dx, dy),
        }
        true
    }

    pub(crate) fn pointer_up(&mut self, _p: (f64, f64)) -> bool {
        let Some(drag) = self.drag.take() else { return false };
        self.commit_drag(drag);   // pushes one undo snapshot
        true
    }
}
```

Three discipline points:

- **No tool-mode `match` outside `apply_gizmo_delta`.** The router
  knows pointer phases, the slot knows the tool, and one private
  method knows how each arm under each tool transforms a delta.
  Adding a new tool mode (e.g. `Skew`) is one variant on `ToolMode`
  + one arm in `apply_gizmo_delta`. The router doesn't move.
- **`hit_test` is the single dispatch over drag-target geometry.**
  It returns `Option<DragKind>` — the *typed* answer to "what's
  under the cursor right now." Six bindings cannot disagree about
  hit regions because they don't compute them; the slot does, once.
- **Undo snapshots flow through `commit_drag`, not through every
  binding.** The drag's undo entry is a `TransformSnapshot` →
  `ApplyTransform` action; per-arm code never touches the undo
  stack. Identical shape to the existing `DragSnapshot` /
  `ResizeSnapshot` machinery in `prism-shell/src/app/`, ported
  *into* the slot rather than duplicated *next to* it.

### What deletes from `panel_props.rs`

Six more bridge functions go to legacy: `code_editor_props`,
`builder_canvas_props`, `gizmo_props` (× 1, kind-parameterised),
`resize_handle_props`, `component_picker_props`. The file's
remaining count after §22 is **0** — every typed-shape helper has
moved onto its owning slot. `panel_props.rs` deletes from disk in
the same PR; the §17 deletion list adds one line.

That deletion is the *terminal* signal of the port wave: the
intermediate "typed substate → JSON" layer that existed only as
a transition scaffold no longer has any callers. The three real
layers — *typed slot*, *bindings table*, *runtime resolver* —
stand alone.

### Test discipline

Eleven new tests. The shape mirrors §20–§21's per-slot rollup
plus three new write-side tests:

- **Read side (7).** One slot-unit test per `*_props` method,
  each asserting the JSON shape against a fixture `CanvasSlot`.
  `gizmo_props_share_shape_across_three_modes` is the
  rule-of-three parity check — same key set on all three gizmo
  emissions, only `tool` differs.
- **Write side (3).** `pointer_drag_round_trip_under_move_tool`,
  `pointer_drag_round_trip_under_rotate_tool`,
  `pointer_drag_round_trip_under_scale_tool`. Each drives
  `pointer_down → pointer_move → pointer_up` synthetically
  through the slot, asserts the document mutation matches the
  expected delta, and asserts exactly one undo snapshot lands.
  The router's three new arms are exercised through
  `events::dispatch_event` in one keystone integration test
  (`pointer_events_route_through_canvas_slot_under_active_tool`).
- **Cross-binding (1).** `selection_center_drives_gizmo_and_handle_bindings`
  — flipping `state.canvas.selection` shows up *both* in
  `shell.gizmo-move` and `shell.resize-handle` emissions through
  the same `selection_center()` helper. The load-bearing
  duplication check for the canvas slot.

Lib suite after §22: 214 passing (was 203 at §21 close).

### No-duplication discipline scorecard

| Force §19/§20/§21 prevent           | How this batch honoured it |
|---|---|
| JSON inlined inside closures        | All seven new bindings are `bind_slot!` rows; closure body is one slot method call. |
| Same shape in two bindings          | `gizmo_props(kind)` is the *only* emitter for the gizmo shape; three bindings forward through it. `selection_center()` is the *only* center-of-selection computation; gizmo + handle bindings both consume it. |
| Premature shape extraction          | `code_editor_props` and `component_picker_props` get *no* shared helpers — their key sets are disjoint from every other emission, even though they live on the same slot. |
| Per-binding visibility branches     | `gizmo_props` emits `visible: bool` as a data field; no `if tool == kind { … }` on the host. Same pattern as §20 `OverlaySlot::help_tooltip_props`. |
| Cross-slot reach from a closure     | All seven rows are `\|s: &AppState\| s.canvas.…`. Zero secondary-arg compositions (canvas owns every datum its bindings read). |
| Per-tool `match` on the router      | `events::dispatch_event` gains three arms (down/move/up) and *no* tool-mode awareness. The dispatch over `ToolMode` × `DragKind` lives on the slot, exactly once, in `apply_gizmo_delta`. |
| Duplicated drag/undo plumbing       | `TransformSnapshot::capture` + `commit_drag` are the single capture/commit path; gizmo and resize handle drags share both. The pre-§22 `DragSnapshot` / `ResizeSnapshot` halves of `app/` collapse to one. |

### Terminal-state check

After §22 lands, the migration's invariants hold *structurally*:

- **Every registered shell block has a real binding.** Stub-loop
  count: 18 (was 25 at §21 close, was 32 at §20 close). The
  remaining 18 are *intentional* leaves (`shell.icon-button`,
  `shell.dock-tab`, `shell.menu-item`, `shell.signal-connection-row`,
  …) whose data flows down inside parent JSON arrays — adding a
  binding row for them would be a *second* serialisation site for
  shapes their parent slot already owns. The §20 rule ("stub
  bindings stay stubs for per-row blocks") is now load-bearing.
- **`panel_props.rs` is empty / deleted.** Every typed-shape
  helper lives on its owning slot. The §17 transition layer is
  gone.
- **`events::dispatch_event` is one `match` over `Event`
  variants, with one arm per variant.** No per-feature dispatch,
  no per-block dispatch, no per-tool dispatch. The router is
  ~30 LoC.
- **`AppState` is nine slots; bindings are 47 forwarders; the
  resolver is one tag table; the skeleton is one file.** Adding a
  new shell block is exactly two edits — `register_shell_builtins`
  + `ShellPropBindings::with_builtins`. Adding a new datum is
  exactly one edit — a slot field plus its accessor. The four
  registration tables (component registry, resolver tag table,
  shell block registry, bindings table) are all *flat*, all
  *declarative*, and each is the *only* site that knows about
  its row. The "smart pattern" claim from §17's title is
  measurable, not aspirational, in the diff: ~5800 LoC out, ~250
  LoC in (the §17 net), plus ~600 LoC out from `panel_props.rs`
  rewrite-into-slots, plus ~150 LoC of `app/sync/*.rs` and
  `app/callbacks/*.rs` that ported into slot mutators rather
  than re-duplicating in `events.rs`.

### Decision-log entry

See the unified decision log at the foot of §17. The §22 port wave
adds one row.

## 17. Slint tear-out: `ShellPropBindings` + `Surface` boot, rip-and-replace

**Strategy locked 2026-05-09 (continuation of §16).** Phase 4 closed
with 47 registered shell blocks, a canonical `ui/app.prism-ui`
skeleton that lowers end-to-end through `RegistryTagResolver`, and
12 ad-hoc bridge functions in `prism_shell::panel_props` mapping
typed `panels::*` / `DockWorkspace` / `BuilderDocument` snapshots
into the JSON shapes each block consumes. The Phase-5 tear-out is
a *wiring* problem, not an *infrastructure* problem — every leaf
already exists, every composition tag already routes children
through the resolver, every block already reads typed-or-JSON
props. The remaining risk is **call-site duplication on the host
side**: each frame the supervisor needs to know which block id maps
to which bridge function, which prop bag flows where, and what
sub-tree (host_children vs JSON array) each composition expects.

The 12 free functions in `panel_props.rs` are the canary. They
already share a shape — *pure `&AppState`-or-substate → `Value`* —
but the call site that would assemble all of them into a single
`BuilderDocument` for the runtime to lower has nowhere central to
look up "for `shell.foo`, here is the function that produces its
props." A naive Phase-5 supervisor would grow a 47-arm `match` over
block ids; that is the same duplication §13's `register_shell_builtins`
killed for blocks, just relocated to the host. The fix is the same
fix.

**Solution (~120 LoC, one new public type, one new macro).** A
`ShellPropBindings` struct that mirrors `ComponentRegistry`'s shape
on the host side: one row per registered block id, each row a typed
binding closure `Fn(&PropCtx) -> PropEmission`. The closure produces
either a JSON `Value` (for prop-driven blocks) or a `Vec<UiNode>`
(for composition blocks that want host_children) or both. A single
`bind!(...)` macro (sibling of `reg!`) registers each binding in
one line. The supervisor walks the bindings table once per frame,
emits a `BuilderDocument` whose root is the parsed `app.prism-ui`
skeleton with each composition body filled from the matching
`PropEmission::children`, and lowers the whole thing through the
already-shipped `lower_document_with_scope` + `RegistryTagResolver`
pipeline. No new dispatch loop, no per-block `if`s, no parallel
prop-routing layer.

**Smart-pattern wins (every constraint at the top of plan §0 honoured):**

- **One registration table, not 47 wiring sites.** `ShellPropBindings::with_builtins`
  reads as a flat list of `bind!(reg, "shell.inspector-tree",
  |ctx| inspector_tree_emission(ctx))` rows — adding a new block
  (or splitting one) is a one-line change. The supervisor never
  imports `panels::*` or `panel_props::*` directly; it only calls
  `bindings.snapshot(&state)`. Mirrors §13's `reg!` macro one-for-one.
- **DI through the existing carrier.** `ShellPropBindings` lives on
  `ShellInner` next to `Arc<ComponentRegistry>`. Tests, alternate
  hosts (Studio, the WebSocket relay's headless renderer, future
  per-app shells) can swap the bindings table to override which
  block id resolves to which data — exactly the registry-bypass
  problem §15's `lower_as` solved for embedded chrome, applied to
  data flow.
- **Builder-style `PropCtx`, not a 12-argument tuple.** A
  `PropCtx<'a>` borrow-pack carries `&AppState`, `&BuilderDocument`,
  `&DockWorkspace`, `&CommandRegistry`, `&[ToastData]`, current
  selection, current viewport, etc. — every binding closure reads
  exactly the fields it needs, the host packs the ctx once per
  frame. Adding a new field to `PropCtx` is additive; existing
  bindings ignore it. No closure ever needs to thread state through
  free-function arguments again.
- **The 12 `panel_props::*` functions stay pure.** They are the
  *implementation* of each binding closure — `PropCtx` extracts
  the borrow they need and forwards it. No deletion churn, no
  behavioural change, no second source of truth. Each binding is
  a one-line forwarder; the bridge functions remain unit-testable
  in isolation.
- **Composition vs leaf, declared once.** `PropEmission` is a struct
  with `props: Value` and `children: Vec<UiNode>` — leaves return
  `PropEmission { props, children: vec![] }`, compositions return
  both. The supervisor folds `children` into the `host_children`
  slot via the same `LowerCtx::with_host_children` (§14) the
  resolver pre-pass uses. Zero new context fields.

**Surface added (~120 LoC across 3 files):**

- **`prism_shell::props::ShellPropBindings`** — host-side registry,
  one row per shell block id. `with_builtins() -> Self` populates
  every binding from the existing `panel_props::*` functions. `get(id)
  -> Option<&Binding>` for tests / alternate hosts. `snapshot(ctx)
  -> HashMap<String, PropEmission>` walks every binding once, used
  by the supervisor to drive the per-frame document rebuild.
- **`prism_shell::props::PropCtx<'a>`** — borrow-pack carrying every
  typed datum a binding might need. Built once per frame from
  `ShellInner` via `ShellInner::prop_ctx(&self) -> PropCtx<'_>`.
- **`prism_shell::props::PropEmission`** — `{ props: Value, children:
  Vec<UiNode> }`. The single shape every binding returns; covers
  leaves (children empty), JSON-list compositions (children empty,
  array nested in props), and `host_children` compositions (children
  populated, props minimal).
- **`prism_shell::props::bind!`** — the `reg!` analogue. One line per
  binding: `bind!(reg, "shell.inspector-tree", inspector_tree_emit);`
  expands to a typed entry in the table.

**Supervisor delta (~40 LoC, replaces the 30-line `bind_model!`
block in `app/shell.rs`).** The Phase-5 boot path:

```rust
// Phase 5 supervisor (replaces the AppWindow::new + bind_model! block)
let skeleton = parse(include_str!("../ui/app.prism-ui"))?;
let bindings = ShellPropBindings::with_builtins();
let surface  = Surface::new(viewport)?;          // prism-ui-runtime backend
// per-frame loop (driven by the runtime's redraw notifier):
let ctx = inner.borrow().prop_ctx();
let emissions = bindings.snapshot(&ctx);
let doc = skeleton.fill_compositions(&emissions);  // ~20 LoC pure fold
let nodes = lower_document_with_scope(&doc, &scope);
surface.render(nodes);
```

Three observations:

1. **No 47-arm match.** `fill_compositions` is a single recursive
   walk that, on each `<shell.foo>` element, looks up
   `emissions["shell.foo"]` and (a) merges its `props` into the
   element's attributes, (b) replaces the element's children with
   `emission.children` (or, for JSON-array compositions, leaves
   children empty since the array lives in props). One function,
   one rule, every block.
2. **No host imports any `Block` type.** `ShellInner` no longer
   knows `MenuBarRow`, `AppWindow`, `BuilderCanvas` exist as types
   — only as ids in the bindings table. Crate-internal visibility
   on every `components::*` module can drop from `pub` to `pub(crate)`
   in the same change, removing 47 leaked symbols from the public
   surface.
3. **The 30 `bind_model!` lines disappear.** The Slint
   `ModelRc<…>` wiring (`set_grid_cells`, `set_inspector_nodes`,
   `set_workflow_pages`, …) was *the duplication that the bindings
   table replaces*. Every `Vec<…>` model the supervisor was pushing
   into Slint is the `props` field of a `PropEmission` now;
   per-frame the supervisor pushes a single `HashMap` instead of
   30 individual model handles.

**Block migration (zero blocks touched).** The bindings layer is
purely additive on top of the 47 already-shipped blocks. No
`Block` impl changes; no schema changes; no JSON-shape changes.
The bridge functions in `panel_props.rs` are reused verbatim —
each becomes the body of one binding closure. This is the §15
discipline applied at the data layer: the `lower_as` seam left
every block's `lower_ui` untouched, this seam leaves every
block's prop schema untouched.

**Tear-out shape (rip-and-replace, no parity).** Slint is deleted
in one stroke; the new system stands on its own. Breakage is
acceptable, and is the *signal* that load-bearing duplication is
gone — every path that was doing the same job through both stacks
collapses to the runtime path or disappears.

The deletion targets, listed by what they were doing and what
replaces them:

| Deleted | Was doing | Replaced by |
|---|---|---|
| `ui/app.slint` (~4300 lines) | declarative root component, every chrome region duplicated as a `Rectangle`+`Text` tree | `ui/app.prism-ui` (already on disk, §15) parsed once, lowered through `RegistryTagResolver` per frame |
| `build.rs` `slint_build::compile` line | codegen of the `AppWindow` Rust type | nothing — `Surface` is constructed directly from a `Node` tree |
| `slint`, `slint-build`, `slint-interpreter` deps | the entire UI stack | `prism-ui-runtime` (already a workspace dep) |
| `slint::include_modules!()` in `lib.rs` | injects `AppWindow` + every model item type (`GridCellItem`, `InspectorNode`, `WorkflowPageItem`, …) into the crate root | nothing — every "model item type" was a Slint-shaped DTO; the bindings emit `serde_json::Value` directly into block prop bags |
| `app/sync/` (9 files, ~1500 LoC of `bind_model!` + per-frame setter calls) | per-panel push of typed state into Slint `VecModel`s | one `ShellPropBindings::snapshot(&ctx)` call per frame |
| `app/callbacks/` (6 files) | wiring Slint callbacks (`dispatch-key`, `help-hover`, `node-drag-*`, `grid-cell-clicked`, …) to `ShellInner` mutations | one `EventHandler` closure on `Surface` that translates `prism_ui_runtime::event::Event` into the same `ShellInner` mutations |
| 30-line `bind_model!` block in `app/shell.rs` | binding 30 `Rc<VecModel<…>>` instances to Slint properties | replaced by the per-frame `bindings.snapshot(&ctx)` → `Surface::set_tree` flow |
| `crate-type = ["cdylib", "rlib"]`'s `cdylib` half | required by `wasm-bindgen` against the Slint codegen | dropped — wasm builds target the `rlib` directly via `prism_ui_runtime::backends::web` |
| `live-preview` feature | Slint's `LiveReloadingComponent` interpreter wrapper | dropped — `prism-ui` already parses at runtime; reload is a file-watch + `Surface::set_tree` re-lower |

**The new boot path (one screen, no branches).** `app/shell.rs`'s
`Shell::new` becomes:

```rust
pub fn new() -> Result<Self, ShellError> {
    let skeleton = parse_prism_ui(include_str!("../ui/app.prism-ui"))?;
    let registry = build_shell_registry();
    let bindings = ShellPropBindings::with_builtins();
    let inner    = Rc::new(RefCell::new(ShellInner::new(registry, bindings)));
    let tree     = render_tree(&inner.borrow(), &skeleton);   // bindings → fold → lower
    let viewport = Viewport::new(1280, 800);
    let surface  = Surface::new(tree, viewport);
    Ok(Self { inner, surface, skeleton })
}

pub fn run(self) -> Result<(), Box<dyn std::error::Error>> {
    let inner    = Rc::clone(&self.inner);
    let skeleton = self.skeleton.clone();
    let handler: EventHandler = Box::new(move |event, surface| {
        if dispatch_event(&inner, event) {
            surface.set_tree(render_tree(&inner.borrow(), &skeleton));
        }
    });
    prism_ui_runtime::backends::femtovg::run(self.surface, handler)
}
```

That is the *entire* surface area where the host meets the runtime.
Nine `app/sync/*.rs` files, six `app/callbacks/*.rs` files, the
`bind_model!` macro, every `set_grid_cells` / `set_inspector_nodes`
/ `set_workflow_pages` setter call collapse into two functions
(`render_tree`, `dispatch_event`) and one declarative table
(`ShellPropBindings::with_builtins`). Every screen of code that
was *moving the same data through two shapes* (typed → Slint
model → Slint property → declarative `.slint` binding) is gone.

**Smart-pattern wins (the rip-and-replace makes them load-bearing,
not optional):**

- **One registration table replaces 30 setter sites.** Adding a
  block before: register in `register_shell_builtins`, add a
  `bind_model!`, add a `set_*` push in the relevant `app/sync/*.rs`,
  add a Slint property + binding in `ui/app.slint`, add a callback
  in `app/callbacks/*.rs`. Adding a block after: register in
  `register_shell_builtins`, register in `ShellPropBindings`. Two
  edits, both declarative, both in the same crate.
- **Events flow through one router, not 50 callback closures.**
  `dispatch_event(inner, event)` is a single `match` on
  `prism_ui_runtime::event::Event` that walks to `ShellInner`'s
  mutation methods. Each event arm is one line; the 6 `app/callbacks/*`
  files compress to ~150 LoC total.
- **No second model-type vocabulary.** `GridCellItem`,
  `InspectorNode`, `WorkflowPageItem`, `BreadcrumbItem`, `TabItem`,
  `ToastItem`, `ButtonSpec`, `MenuDef`, `EditorLine`, every
  `pub struct …Item` Slint required — *all* deleted. Bindings emit
  `serde_json::Value` directly. The `panels::*` typed structs stay
  as the source-of-truth domain types; the JSON shape is the wire
  format the blocks already speak.
- **Composition vs leaf, declared once.** `PropEmission { props,
  children }` — leaves return empty `children`, compositions return
  populated ones, the `render_tree` walker folds `children` into
  the matching `<shell.foo>` element via §14's `LowerCtx::with_host_children`.
  No third bucket, no per-block dispatch.
- **DI through the existing registry seam.** `ShellPropBindings`
  lives next to `Arc<ComponentRegistry>` on `ShellInner`. Tests,
  alternate hosts (the WebSocket relay's headless renderer,
  per-product Studio variants) override bindings the same way they
  override block impls. Same shape, two layers.

**Surface added (~250 LoC, three new modules):**

- **`prism_shell::props`** (~120 LoC) —
  - `PropCtx<'a>`: borrow-pack of every typed datum (`&AppState`,
    `&BuilderDocument`, `&DockWorkspace`, `&CommandRegistry`,
    `&[ToastData]`, current selection, viewport).
  - `PropEmission { props: Value, children: Vec<UiNode> }`.
  - `ShellPropBindings`: `HashMap<&'static str, Box<dyn Fn(&PropCtx) -> PropEmission>>`.
  - `with_builtins()`: one `bind!()` line per registered shell
    block, each forwarding to a `panel_props::*` function.
  - `bind!` macro: sibling of `reg!`.
- **`prism_shell::render`** (~80 LoC) —
  - `render_tree(&ShellInner, &Skeleton) -> Node`: builds
    `PropCtx`, calls `bindings.snapshot`, folds emissions into the
    skeleton, lowers through `lower_document_with_scope` +
    `RegistryTagResolver`.
  - `fill_compositions(&Skeleton, &HashMap<String, PropEmission>) -> BuilderDocument`:
    pure recursive walk, one rule (merge `props`, replace `children`).
- **`prism_shell::events`** (~150 LoC, replaces `app/callbacks/`) —
  - `dispatch_event(&Rc<RefCell<ShellInner>>, Event) -> bool`:
    one `match` on every runtime event variant (Pointer, Key,
    Resize, Focus, …), routes to the existing `ShellInner`
    mutation methods. Returns `true` if the tree needs re-rendering.
  - `combo_from_runtime_event`: the existing helper, now the
    only key-translation site (the Slint sibling
    `combo_from_slint` deletes with the rest of `input.rs`'s
    Slint coupling).

**Deletion targets (concrete, one PR landing the rip):**

- `packages/prism-shell/ui/app.slint` — delete file (~4300 lines).
- `packages/prism-shell/ui/icons/` — keep; `prism-ui-runtime`
  loads SVGs the same way through `Node::Image`.
- `packages/prism-shell/build.rs` — delete the `slint_build::compile`
  call. Build script becomes empty (or the file deletes if no other
  build-time work lands).
- `packages/prism-shell/Cargo.toml` — drop `slint`, `slint-build`,
  `slint-interpreter` deps; drop the `live-preview` feature; drop
  `cdylib` from `crate-type` (rlib only — wasm-bindgen targets the
  rlib via the workspace's `wasm32-unknown-unknown` profile). Add
  `prism-ui-runtime = { workspace = true }` if it isn't already on
  the dep list (it is, transitively via `prism-builder`, but the
  shell needs it directly for `Surface` / `EventHandler` /
  `backends::femtovg::run`).
- `packages/prism-shell/src/lib.rs` — delete `slint::include_modules!()`,
  delete every model-item re-export, replace `pub use AppWindow`
  with `pub use crate::shell::Shell`.
- `packages/prism-shell/src/app/sync/` — delete the whole
  directory (9 files).
- `packages/prism-shell/src/app/callbacks/` — delete the whole
  directory (6 files); replace with `src/events.rs`.
- `packages/prism-shell/src/app/shell.rs` — rewrite as the
  ~30-line `Shell::new` / `Shell::run` shown above.
- `packages/prism-shell/src/app/mod.rs` — strip every `AppWindow`
  reference (currently 1938 lines, much of which is Slint glue);
  the `panel_id_for_slint` / `push_dock_layout` helpers delete
  (their job moves into the corresponding bindings).
- `packages/prism-shell/src/app/inner.rs` — drop the
  `models: PersistentModels` field and the per-model `Rc<VecModel<…>>`
  declarations; `ShellInner` keeps `store`, `registry`, `bindings`,
  `input`, `commands`, `menus`, `vfs`, `persistence`, …
- `packages/prism-shell/src/app/commands.rs` — strip
  `&AppWindow` parameters from every command body; commands now
  mutate `ShellInner` only and the next `render_tree` call picks
  up the change.
- `packages/prism-shell/src/app/mutations.rs` — same; drop the
  `&AppWindow` arg, drop the explicit `set_*` push calls.
- `packages/prism-shell/src/bin/native.rs` — replace
  `Shell::new()?.run()?` chain (already this shape) with the new
  `Shell::new` signature; CLI flags route to the same store
  mutations they always did.
- `packages/prism-shell/src/testing.rs` and `src/e2e.rs` — drop
  Slint screenshot path (use `prism_ui_runtime::backends::femtovg`'s
  offscreen capture); `TestHarness` drives `dispatch_event`
  directly with synthetic `Event`s instead of Slint callbacks.
  This *is* a behavioural change but it's strictly a simplification
  — one input path replaces two.
- `packages/prism-shell/src/panel_props.rs` — keep verbatim; its
  12 functions are the binding bodies.
- `packages/prism-shell/CLAUDE.md` — flip every "Slint owns
  layout/windowing/rendering", every `ui/app.slint` reference,
  every `bind_model!` mention, every Slint feature/dep paragraph
  to the `prism-ui-runtime` story. The "Phase 4 status" feature
  bullets stay accurate (the *features* still work) but the
  implementation paragraphs underneath update.
- `packages/prism-studio/src-tauri/src/main.rs` — replace its
  `slint::ComponentHandle` import + `shell.window().run()` call
  with `shell.run()`. One-line change downstream of the rip.
- Workspace `Cargo.toml` — drop `slint*` workspace deps if no
  other crate needs them (none should after this lands).

**What breaks (and is fine):**

- The `live-preview` feature and `prism dev shell --no-hot-reload`
  flag stop existing. File-watch reload comes back as a `Surface::set_tree`
  re-lower against the same `app.prism-ui` source — strictly simpler.
- The current `prism e2e --record` screenshot baselines invalidate
  (different renderer = different pixels). Re-record once after
  the rip; new baselines are the canonical set.
- `prism-studio/src-tauri` rebuilds against the new `Shell::run`
  signature. No other downstream consumer exists.
- Any in-flight branch with `app/sync/*` or `app/callbacks/*`
  edits hard-conflicts — that work re-targets the new
  `events.rs` / `bindings.rs` / `panel_props.rs` shape, which
  is a deletion-and-rewrite, not a merge.

**Test discipline.** The test suite shrinks. Specifically:

- The 461 `prism-shell` lib tests stay valid; the chunk that
  currently asserts Slint property values (`window.get_*`) rewrites
  to assert `render_tree` output (a `Node` tree), which is more
  precise — Slint's get/set asserted that the *binding* fired,
  not that the user-visible shape was correct.
- One new keystone test
  (`bindings_cover_every_registered_shell_block`) asserts
  `ShellPropBindings::with_builtins()` has an entry for every id
  in `register_shell_builtins`'s table. Forgetting to wire up a
  new block is a compile-or-test failure, not a silent blank
  panel.
- One supervisor-level test
  (`render_tree_lowers_full_app_prism_ui_against_populated_state`)
  builds a populated `AppState` (every panel has data: dock pages,
  toasts, command-palette query, signals, nav graph, schema rows,
  builder doc), runs the full pipeline (`bindings.snapshot` →
  `fill_compositions` → `lower_document_with_scope`), and asserts
  the result is a finite `Node` tree with no unresolved tags.
  This is the *complete* parity check, in one test, against the
  populated path.

**Why this is the right level of abstraction.** The same reason
§13/§15/§16 worked: the registration-table pattern is the smallest
seam that collapses N call sites into one. Doing it as a
rip-and-replace (no feature flag, no setter adapters, no parallel
codepaths) makes the smart-pattern *load-bearing* — it can't be
worked around. Every line that's left is either declarative
registration or pure data transformation. Three files
(`props.rs`, `render.rs`, `events.rs`) carry the entire
host-runtime contract.

The duplication elimination is concrete and measurable: ~5800 LoC
deleted (`ui/app.slint` + `app/sync/` + `app/callbacks/` + the
Slint-coupled halves of `app/mod.rs`, `app/shell.rs`,
`app/commands.rs`, `app/mutations.rs`, `lib.rs`), ~250 LoC added,
one workspace dep tree pruned of `slint`, `i-slint-*`, `slint-build`,
`slint-interpreter`. The "smart pattern" claim from the title of
this section is finally settled by the diff statistics, not by
prose.

**What this unblocks.** With the rip landed, the migration is
done. Every section above this point describes a *transition*; §17
is the *terminal state*. Subsequent work in this codebase reads as
"add a block" (one row in `register_shell_builtins`, one row in
`ShellPropBindings`) or "add an event" (one arm in `dispatch_event`).
No further infrastructure work, no further DI seams, no further
runtime extensions are anticipated — and any pressure to add one
is a defect of the plan, not a new requirement.

**Test discipline.** For every binding landed in step 1 of the
table:

- One unit test in `prism_shell::props::tests` per binding,
  asserting `bind!(…)` emits the expected `PropEmission` shape
  given a representative `PropCtx`. These are mechanical wrappers
  around the existing `panel_props::tests` — same fixtures, one
  level of indirection.
- One end-to-end test
  (`bindings_emit_for_every_registered_block`) walks
  `ShellPropBindings::with_builtins().snapshot(&ctx)` against a
  populated `AppState` and asserts every id in
  `register_shell_builtins`'s table has a matching entry. This is
  the load-bearing parity check — it makes "forgot to wire up the
  new block" a compile-or-test failure rather than a silent
  blank panel at runtime.
- One supervisor-level test
  (`fill_compositions_round_trips_through_resolver`) parses the
  on-disk `app.prism-ui` skeleton, runs the bindings snapshot
  against a fixture state, folds emissions into the document, and
  asserts the resolver lowers the result without unresolved tags
  and without panic. This is the §15
  `canonical_app_prism_ui_skeleton_lowers_end_to_end` test
  extended to cover the *populated* path, not just the structural
  one.

**Why this is the right level of abstraction.** Every prior
smart-pattern landing in this plan promoted a registration table at
the moment a per-id `match` started growing on the host: the
component registry (§7) for runtime tag dispatch, `register_shell_builtins`
(§13) for block dispatch, `LowerCtx::lower_as` (§15) for embedded
chrome dispatch. `ShellPropBindings` is the same move at the host
data layer — the *fourth* application of the same pattern, by
design. The threshold for promotion (rule-of-three on the host
side, where the cost is "every new block requires editing N
unrelated wiring files") is met at 12 `panel_props` functions and
30 `bind_model!` lines today; it would be catastrophic at 47 if
deferred until the Slint tear-out forced the issue. Adding the
table now lets step 2 of the tear-out be a *deletion-only* PR.

**What this unblocks.** Phase 5 (Slint tear-out) is now a
mechanical execution of the three-step plan above. No further
infrastructure work, no further DI seams, no further runtime
extensions are anticipated. After step 3 lands, the only consumer
of `slint::*` in the workspace is the `prism-studio/src-tauri`
shell's `main.rs` (which already only uses `slint::ComponentHandle`
for the run-loop), and that becomes a one-line change to
`prism_ui_runtime::Surface::run`. The migration is then done.

## 24. Service registry — keys, commands, mutations come back online

**Strategy (continuation of §17/§22).** The §17 rip landed and the §19–§22
read-side port wave finished: nine slots, 27 real bindings, 20 intentional
row-leaf stubs, one router with one arm per `Event` variant. The bindings
table covers every registered shell block; the keystone parity test passes.
That contract is *terminal for the read side*. What it leaves open is the
**write side beyond pointer**: `dispatch_event`'s `Wheel`/`Key`/`Text`/`Focus`
arms are no-ops, and roughly two thousand lines of feature code
(`command.rs`, `keybindings.rs`, `keyboard.rs`, `input.rs`, `signals.rs`,
`persistence.rs`, `project.rs`, `search.rs`, `selection.rs`, `help.rs`,
`menu.rs`, `panels/*`, `panel_props.rs`, `testing.rs`, `e2e.rs`, `luau/*`)
still sit on disk *outside* the build, waiting to re-mount onto
`ShellInner`. Until they do, no key the user presses reaches the store —
the shell paints, but nothing happens.

The naive port path is what every prior section warned against: pull
each module back into `lib.rs`'s `pub mod` list, give `ShellInner` a new
field per module (`commands: CommandRegistry`, `input: InputManager`,
`undo: UndoStack`, `signals: SignalRuntime`, `persistence: ProjectPersistence`,
`project: Option<ProjectManager>`, `search: SearchIndex`, `help: HelpRegistry`,
…), and let `dispatch_event`'s `Key` arm grow a per-feature `match` over
who-handles-what. That replays the exact duplication §13 killed for blocks
and §17 killed for prop wiring, just relocated to the host's *event*
layer. The fix is the same fix at the same seam: one registration table,
one declarative trait, one router arm.

**Solution (~280 LoC, one new trait, one new registry, one new ctx
borrow-pack).** A `ShellService` trait with three pure methods
(`id`, `on_event`, `commands`) and a `ServiceRegistry` mirroring
`ShellPropBindings`'s shape. Each existing module ports as one
`impl ShellService for FooService`; each command it owns ports as one
`CommandSpec` returned from `commands()`. The router stays one match arm
per `Event` variant — every arm forwards the event to *the registry*,
which fans it out to services in declared order, short-circuiting on the
first `EventOutcome::Handled`. No per-module branching in `events.rs`,
no `if let Some(cmd) = …` chains, no per-feature `ShellInner` field
that the router has to know about by name.

The same registry holds the **command table**: services contribute
commands at registration time via `commands()` (a `Vec<CommandSpec>`),
and a single `CommandRunner::run(id, &mut Ctx)` dispatches by id. The
existing `CommandRegistry::with_builtins` becomes one service among
many — `ShellBaseService` — that contributes the shell-global commands
(`undo`, `redo`, `palette.open`, `panel.*`, `viewport.*`). Every new
feature ships its own service + commands; nothing else in the host needs
to learn the feature exists.

### 24.1 The four contracts (each exists exactly once)

```rust
// 1. The service trait — every feature implements this.
pub trait ShellService: Send + Sync {
    fn id(&self) -> &'static str;
    fn on_event(&self, _ev: &Event, _ctx: &mut MutCtx<'_>) -> EventOutcome {
        EventOutcome::Pass
    }
    fn commands(&self) -> Vec<CommandSpec> { Vec::new() }
}

// 2. The mutation borrow-pack — sister to PropCtx, but `&mut`. The single
//    carrier into every event handler and every command body. Adding a
//    new datum = one field here; existing services ignore it.
pub struct MutCtx<'a> {
    pub state: &'a mut AppState,
    pub viewport: Viewport,
    pub now: Instant,
    pub clipboard: &'a mut Clipboard,
    pub undo: &'a mut UndoStack,
    pub vfs: &'a mut Vfs,
    pub signals: &'a mut SignalBus,
}

// 3. The outcome enum — three states, no overloads.
pub enum EventOutcome {
    Pass,                // service ignored this event; try the next one
    Handled,             // service consumed it; stop fan-out, redraw
    HandledQuiet,        // consumed, no redraw needed (e.g. focus tick)
}

// 4. The registry — one row per service, fan-out in declared order.
pub struct ServiceRegistry {
    services: Vec<Arc<dyn ShellService>>,
    commands: HashMap<&'static str, CommandSpec>,
}
```

The four together are the *entire* event-and-command surface. `events.rs`
imports `ServiceRegistry` and nothing else; `command.rs`'s
`CommandRegistry` becomes a thin wrapper around the `commands` map on
the registry.

### 24.2 Registration table — `register_shell_services`

Sister to `register_shell_builtins` (blocks) and `with_builtins`
(bindings). Reads as a flat list of one-line rows; adding a new feature
is exactly two edits — `impl ShellService for FooService` plus one row
here.

```rust
pub fn register_shell_services(reg: &mut ServiceRegistry) {
    reg.add(ShellBaseService::default());        // global commands + key dispatch
    reg.add(InputService::with_defaults());      // layered scheme stack
    reg.add(UndoRedoService::default());         // ctrl+z / ctrl+y
    reg.add(SelectionService::default());        // arrow-keys, esc, click-empty
    reg.add(CommandPaletteService::default());   // ctrl+shift+p, fuzzy filter
    reg.add(PersistenceService::default());      // ctrl+s/o/n, rfd dialogs
    reg.add(ProjectService::default());          // open-folder, vault sync
    reg.add(SearchService::default());           // ctrl+f, TF-IDF index
    reg.add(SignalsService::default());          // builder signal dispatch
    reg.add(HelpService::default());             // hover tooltip lifecycle
    reg.add(MenuService::default());             // dropdown + context menu
    reg.add(ClipboardService::default());        // copy/cut/paste/duplicate
    reg.add(LuauService::default());             // custom-handler exec
}
```

Twelve services replace thirteen orphaned modules and the would-be
13-arm `if`/`match` in `dispatch_event::Key`. The list is *the* index
of "what features the shell has"; nothing else has to know.

### 24.3 Router delta — three lines per arm, zero per-feature awareness

`dispatch_event` keeps its one-arm-per-`Event`-variant shape. Each arm
calls `services.fan_out(event, ctx)` and translates the `EventOutcome`
into a redraw bool. Adding a new feature *does not touch* `events.rs`.

```rust
pub fn dispatch_event(inner: &Rc<RefCell<ShellInner>>, event: &Event) -> bool {
    let mut guard = inner.borrow_mut();
    let mut ctx = guard.mut_ctx();
    match event {
        Event::Resize { width, height } => { ctx.state.viewport = …; true }
        // Pointer arms keep their direct `state.canvas` forwarders (§22)
        // — they're already one-line and don't fan out.
        Event::PointerDown { x, y, .. } => ctx.state.canvas.pointer_down(*x, *y),
        Event::PointerMove { x, y }     => ctx.state.canvas.pointer_move(*x, *y),
        Event::PointerUp { x, y, .. }   => ctx.state.canvas.pointer_up(*x, *y),
        // Every other variant fans out through the service registry.
        // Services short-circuit on first `Handled`; if all `Pass`, no redraw.
        Event::Key { .. } | Event::Text { .. }
            | Event::Wheel { .. } | Event::Focus { .. } => {
            matches!(guard.services.fan_out(event, &mut ctx),
                     EventOutcome::Handled)
        }
    }
}
```

Five lines added, zero subtracted, the §22 pointer arms preserved.
The new fan-out is *a single function call* — every feature's wiring
lives behind that one symbol.

### 24.4 Smart-pattern wins (each maps to one §17 invariant)

- **One registration table, not 13 wiring sites** —
  `register_shell_services` is the single index. Mirrors §13 (`reg!` for
  blocks) and §19 (`bind_slot!` for bindings). Forgetting to register a
  service is one missing row, not a silent feature.
- **DI through the existing carrier** — `ServiceRegistry` lives on
  `ShellInner` next to `ShellPropBindings` and `ShellComponentRegistry`.
  Tests, alternate hosts (Studio, the headless renderer, per-product
  shells) override services exactly the way they override bindings.
  Same shape, third instance.
- **Builder-style `MutCtx`, not a 7-argument tuple** — sister to §17's
  `PropCtx`. Every service borrows the fields it needs and ignores the
  rest. Adding a field is additive; existing services don't recompile
  their signatures. The read/write symmetry (`PropCtx` for snapshot,
  `MutCtx` for handlers) makes the data plane uniform: bindings read
  through one ctx, services write through its mirror.
- **One trait, three methods, no inheritance** — `ShellService` has no
  default-trait-method pyramid, no associated types, no `dyn`-unsafe
  generics. Every implementer is a `struct` with a `Default` and three
  short methods. The trait is *the* surface; nothing else in `prism-shell`
  is a "way to add behaviour." (Existing `Component`/`Block`/`Panel`
  traits stay where they are — they're the *visual* contract; this is
  the *behavioural* contract. Two traits, two domains, no overlap.)
- **Commands declared with their service, not in a separate file** —
  `commands()` returns the `CommandSpec`s the service knows how to
  execute. The registry indexes them on `add()`; `CommandRunner::run`
  finds the spec, calls its handler against `MutCtx`. The 117-line
  `command.rs` builtin list collapses to ~12 `commands()` impls of
  ~6 lines each — same total LoC, but each command lives next to the
  state it touches. Drift between "command listed in palette" and
  "command actually does something" is structurally impossible: both
  come from the same `CommandSpec`.
- **Layered input is one service, not a global mutable** — `InputService`
  owns the `InputManager` (the existing `InputScheme` builder pattern
  stays exactly as is, ADR-005). Other services *push* schemes by
  returning them from a new `schemes()` trait method (default empty);
  `InputService::on_event(Key)` walks the stack, resolves to a command
  id, and calls `ctx.runner.run(id)`. The four lines of "key →
  combo → scheme stack → command id" live in *one* service — not in
  every consumer of keys.
- **No second event vocabulary** — `Event` stays `prism_ui_runtime::event::Event`.
  Services consume the same enum the router dispatches; there is no
  per-feature "what does this event mean to me" wrapper. (`InputEvent`
  and the legacy `Action<AppState>` reducer-shape go to legacy unless
  some service genuinely needs replay/serialisation, in which case
  *that one service* owns the wrapper.)

### 24.5 Surface added (~280 LoC across 4 files)

- **`prism_shell::services::mod`** (~80 LoC) — `ShellService` trait,
  `EventOutcome`, `MutCtx`, `ServiceRegistry`. The whole contract.
  No re-exports of feature types — every service is opaque behind its
  trait.
- **`prism_shell::services::base`** (~60 LoC) — `ShellBaseService`
  ports the shell-global slice of the legacy `command::with_builtins`
  list (undo/redo/palette/panel-switch/viewport/zoom). The five
  feature-owned slices (persistence, project, search, help, menu)
  move to their own services in §24.6.
- **`prism_shell::services::input`** (~80 LoC) — `InputService` wraps
  the existing `InputManager`, exposes `push_scheme`/`pop_scheme` for
  apps, and owns the `Key` → combo → scheme → command-id resolution.
  This is the *only* place that converts a runtime `Event::Key` into a
  command id. Other services receive only commands, never raw keys
  (with one exception: `CommandPaletteService` owns the
  query-edit text path while open, returning `EventOutcome::Handled`
  for `Text`/`Key` so the rest of the stack short-circuits).
- **`prism_shell::services::commands`** (~60 LoC) — `CommandSpec`
  (id, label, category, shortcut, handler), `CommandRunner` (the
  `run(id, &mut MutCtx)` entry point), and the `cmd!` macro for the
  one-line declarative form: `cmd!("undo", "Undo", undo_handler)`.

`lib.rs` adds `pub mod services;` and `pub use services::{ShellService,
ServiceRegistry, MutCtx, EventOutcome, CommandSpec};`. `ShellInner`
gains one field (`services: ServiceRegistry`) and one method
(`mut_ctx(&mut self) -> MutCtx<'_>`) — the §17 read-side `prop_ctx`'s
exact mirror.

### 24.6 Port wave — twelve services, one per former module

Each row is *one* `impl ShellService` in a new
`prism_shell::services::<name>` module, plus one row in
`register_shell_services`. The legacy module on disk goes to legacy in
the same PR (no parallel codepath, §17 discipline).

| # | Service | Replaces | Owns |
|---|---|---|---|
| 1 | `ShellBaseService`     | `command.rs`/with_builtins (shell slice) | global cmds: undo/redo/palette/panel-switch/zoom |
| 2 | `InputService`         | `input.rs`, `keybindings.rs`, `keyboard.rs` | `InputManager`, scheme stack, key→combo→cmd-id |
| 3 | `UndoRedoService`      | undo/redo halves of `command.rs` + snapshot stack on `ShellInner` | `UndoStack` field on `MutCtx`; ctrl+z/y handlers |
| 4 | `SelectionService`     | `selection.rs` + arrow-key paths in `panels/*` | `SelectionModel` lives on `BuilderSlot` already; this owns the *mutators* (arrow keys, esc, multi-select extend) |
| 5 | `CommandPaletteService`| `command.rs` palette half + the query-edit path in `app/callbacks/overlay.rs` | open/close, query edit, fuzzy filter, exec selection |
| 6 | `PersistenceService`   | `persistence.rs` | ctrl+n/o/s/shift+s, rfd dialogs, ProjectFile serde |
| 7 | `ProjectService`       | `project.rs` | open-folder, vault sync, file-graph ingest, close |
| 8 | `SearchService`        | `search.rs` | TF-IDF index build/refresh, ctrl+f open, query |
| 9 | `SignalsService`       | `signals.rs` | dispatch builder signals, NavigateTo, SetProperty, EmitSignal cascade (max-depth 8) |
| 10 | `HelpService`         | `help.rs` | hover-show 380ms, idle-hide 8s, ESC dismiss |
| 11 | `MenuService`         | `menu.rs` | dropdown open/close, context-menu open at point |
| 12 | `ClipboardService`    | clipboard halves of `command.rs` | copy/cut/paste/duplicate; owns `Clipboard` field on `MutCtx` |

`LuauService` (custom-handler exec) is added by §24's tail row and is
load-bearing for `SignalsService`'s `Custom` action arm — it's the only
service the registry registers *after* `SignalsService` and the only
one another service calls into directly (via `services.get("luau")`).
That single cross-service reach is the reason the registry is also a
*lookup* table, not just a fan-out broadcast.

### 24.7 What deletes from disk

After §24 lands, *every* file in this list either deletes outright
or is rewritten as a `services::<name>` module with the same public
behaviour and a fraction of the LoC:

- `app/` directory (commands.rs / mutations.rs / inner.rs / shell.rs /
  mod.rs / samples.rs) — already orphaned, §17 listed for deletion;
  drops in this PR. Net: ~3500 LoC out.
- `panel_props.rs` — already not in the build (§22 close moved every
  helper onto its slot); drops in this PR. Net: ~774 LoC out.
- `command.rs`, `keyboard.rs`, `keybindings.rs`, `input.rs`,
  `selection.rs`, `signals.rs`, `persistence.rs`, `project.rs`,
  `search.rs`, `help.rs`, `menu.rs` — each reborn as a `services::<name>`
  module. Net per module: ~150–400 LoC in (most of which is the
  service-trait wrapper around an unchanged inner struct), ~200–800
  LoC out (the host-coupling halves that referenced `AppWindow` /
  Slint callbacks delete with the rip). Total net: ~1800 LoC out,
  ~1500 LoC in.
- `panels/*` — each panel ported in §19–§22 as a slot's `*_props`
  method. The remaining panel files (`panels/identity.rs`,
  `panels/code_editor.rs`, …) finish porting in this wave or are
  already legacy.
- `testing.rs`, `e2e.rs` — re-port against `Surface` + `dispatch_event`
  directly (the existing `TestHarness` / `E2eDriver` shape stays;
  only the input-injection seam swaps from Slint callbacks to
  synthetic `Event`s). Net: ~50 LoC delta — the public API doesn't
  change, the back-end does.

Total: roughly ~5500 LoC out, ~1800 LoC in. The diff is dominated
by deletions, not additions, exactly because the registration table
collapses N call sites into one.

### 24.8 Test discipline

- **One unit test per service** asserting `commands()` returns the
  expected ids and `on_event` returns the expected `EventOutcome` for
  a representative event fixture. ~12 small tests, each <30 lines.
- **One keystone parity test** (`commands_cover_every_registered_shortcut`)
  that walks `register_shell_services`'s aggregated commands and
  asserts every key combo declared in any `InputScheme` resolves to a
  command id present in the table. Forgetting to wire a command is
  a compile-or-test failure, not a silent dead key.
- **One end-to-end router test**
  (`key_event_routes_through_input_service_to_command_runner_and_mutates`)
  that fires an `Event::Key { combo: "ctrl+z" }` against a populated
  `AppState` with one prior undoable mutation and asserts the
  mutation reverses. The full chain (router → fan-out → InputService
  → CommandRunner → UndoRedoService → MutCtx → AppState) is
  exercised in one test.
- **One service-isolation test**
  (`palette_short_circuits_other_services_while_open`) that opens the
  command palette, fires an `Event::Key { combo: "ctrl+s" }`, and
  asserts the save-handler does *not* run — the palette's
  `EventOutcome::Handled` short-circuits the fan-out. This is the
  load-bearing isolation property: when an overlay is modal, the
  fan-out's first-`Handled`-wins rule is the *only* mechanism
  enforcing focus capture; no service has to know another service
  exists.

### 24.9 What this unblocks

After §24 lands, every still-orphaned feature has a documented place
to live (one service module), every key the user presses has a
documented path to mutation (router → fan-out → InputService →
CommandRunner → service handler → `MutCtx`), and every command the
palette displays has a documented owner (the service whose
`commands()` declared it). The §17 contract grows by exactly one
trait, one registry, one ctx — the same growth pattern as every
prior smart-pattern landing in this plan (component registry → block
registry → bindings table → service registry, each at a different
seam, each one row per item, each declared once).

The terminal-state property generalises: the migration's promise was
"every change reads as one of a small set of declarative edits."
Pre-§24 that was true for blocks and slots; post-§24 it is true for
features and commands too. Adding a feature is one new
`impl ShellService` and one row in `register_shell_services`. Adding
a command is one row in some service's `commands()` Vec. Adding an
event handler is one branch on `Event` inside one service's
`on_event`. No further infrastructure work is anticipated; pressure
to add one is, again, a defect of the plan rather than a new
requirement.

### Decision-log entry

| Date | Decision | Rationale |
|---|---|---|

## 25. Service port wave — Selection / Clipboard / CommandPalette

**Strategy.** §24 landed the foundation and three infrastructure
services (Base, Undo, Input). The next batch is three *modal* services
that all read and mutate the selection cursor that already lives on
`BuilderSlot`/`CanvasSlot`/`OverlaySlot`. Grouping them is the same call
§22 made for the canvas family: they share one underlying datum
(active selection), their key paths overlap (Esc, arrow keys, Ctrl+C/X/V,
Ctrl+Shift+P, palette-modal capture), and porting them sequentially
would re-invent the same selection-aware key handling three times.
Batched, they expose exactly one new pattern — **modal capture via
`EventOutcome::Handled`** — and reuse it.

The ripout discipline holds: the three legacy modules
(`selection.rs`, the palette half of `command.rs`, the clipboard half
of `command.rs`) leave the tree in the same PR. We are not porting
the legacy `Action<AppState>` reducer enum, the legacy
`SelectionEvent` wrapper, or the legacy `ClipboardEntry` round-trip
serde — none of those vocabularies exist in the new system. The
runtime's `Event` is the only event vocabulary; `MutCtx` is the only
write surface; `serde_json::Value` is the only clipboard wire format
(matches the §22 `BuilderDocument` shape, no second encoding).

### 25.1 SelectionService — mutators on slots that already own selection

`BuilderSlot` (inspector/properties/signals/schema, §20) and
`CanvasSlot` (gizmos/handles/picker, §22) already own the selection
cursor in their own typed fields. `SelectionService` is the *mutator
side* — the read paths shipped with the slots. Three command bodies,
two key handlers, no new state on `MutCtx`:

- `selection.clear` (Esc) — clears `state.canvas.selection`
  *and* `state.builder.selection` in one call to a new
  `state.clear_selection()` method on `AppState`. The cross-slot
  consistency invariant lives on `AppState`, not on the service —
  same shape as §19's "JSON shape lives on the slot": the multi-slot
  invariant lives on the multi-slot type.
- `selection.move-{up,down,left,right}` (arrow keys) — forwards to
  `CanvasSlot::nudge_selection(dx, dy)`. The `(dx, dy)` table is one
  `match` over the command id inside the service, not four near-identical
  handlers; the `cmd!` macro variant `cmd_with!("selection.move-up",
  …, |ctx| ctx.state.canvas.nudge_selection(0, -1))` keeps each row
  one line.
- `selection.extend-{up,down,left,right}` (Shift+arrow) — same
  forwarder, calls `CanvasSlot::extend_selection(dx, dy)`. The
  *single-vs-multi* dispatch lives on the slot in one place
  (`SelectionModel::Single` vs `Multi`) — the service never sees the
  variant.

The service's `on_event` is empty. All five behaviours route through
`InputService`'s scheme stack, which resolves the key combo to a
command id and invokes the handler. The service contributes only
`commands()` — it is the cleanest possible shape under the §24
contract, and the canonical example for the remaining nine ports.

### 25.2 ClipboardService — `serde_json::Value` is the wire format

The legacy `ClipboardEntry { kind: …, data: Vec<u8> }` enum is gone.
Selection content round-trips as the same `serde_json::Value` shape
that `BuilderDocument` already uses (§22) — copy serialises the
selected sub-tree to `Value`, paste deserialises directly back into
the document. One vocabulary, one serialiser, no `kind` discriminator.

`ClipboardService` adds one field to `MutCtx` (`clipboard: &'a mut
Clipboard`, where `Clipboard` is a one-field newtype `Option<Value>` —
cleared on cut, set on copy, read-and-keep on paste). The four
commands (`clipboard.copy`, `cut`, `paste`, `duplicate`) are five-line
handlers each. `duplicate` is `copy` followed by `paste-at-offset` —
not a separate code path, but two calls to the same two methods on
`CanvasSlot`. The rule-of-three threshold is met (`copy`, `cut`, `duplicate`
all serialise the selection), so `CanvasSlot::serialize_selection() ->
Option<Value>` is the helper; `paste`/`duplicate` both call
`CanvasSlot::insert_at_offset(value, offset)`. Two methods on the
slot, four commands on the service, zero duplication of the wire
format.

System clipboard integration (`arboard`) is a §24.6-row, not a §25
concern — the in-memory clipboard is the contract; arboard plugs in
at the service level later (one feature flag, one extra
`copy`/`paste` line) and the rest of the host is unaffected.

### 25.3 CommandPaletteService — modal capture is the one new pattern

The palette is the first service whose `on_event` returns
`EventOutcome::Handled` *unconditionally while open*, short-circuiting
the fan-out. This is the load-bearing isolation property §24.8
specified in test form (`palette_short_circuits_other_services_while_open`)
and the load-bearing reason `EventOutcome` has three states: open
palette + Ctrl+S must *not* save.

The palette's data already lives on `OverlaySlot::command_palette`
(§20). The service owns:

- `commands()` — `palette.toggle`, `palette.open`, `palette.close`,
  `palette.exec-selected`, `palette.move-up`, `palette.move-down`.
  These are the same six commands a hand-rolled palette would have
  needed; they live next to the service whose `on_event` enforces
  modal capture.
- `on_event` — three branches under "is the palette open?":
  `Event::Text { text }` appends to `OverlaySlot::command_palette.query`
  and returns `Handled`; `Event::Key { combo: "esc" }` runs
  `palette.close` and returns `Handled`; everything else returns
  `Handled` *if open* to capture modality, `Pass` otherwise. The
  three-line "is open?" branch is the *only* state-aware line in the
  service — every other handler is a pure command.
- **Fuzzy filter** is one method on `OverlaySlot` — `filter_commands(query:
  &str, all_ids: &[&'static str]) -> Vec<usize>`. It's not on the
  service because the *result* (the visible row indices) flows out
  through the existing palette binding (§20), not through the
  service's surface. The filter takes the already-aggregated command
  list from `ServiceRegistry::commands_iter()` — one new method on
  the registry, no per-service awareness.

The palette **does not** own a "selected index" cursor as a separate
field on the service — that lives on `OverlaySlot::command_palette.cursor`
where its binding already reads it. Selection-cursor-on-slot is the
§19 doctrine; nothing about the service surface changes it.

### 25.4 Smart-pattern wins (each maps to a §24.4 invariant)

- **One selection invariant, owned by `AppState`** — the cross-slot
  "Esc clears both canvas and builder" rule lives in
  `AppState::clear_selection()`, *not* in the service's command
  handler. Service is one line; multi-slot invariant is one method;
  drift between "what selection means in the canvas" and "what
  selection means in the inspector" is impossible because both slots
  are cleared by the same call. (Mirrors §19 `tabs_json` extraction
  rationale: shared shape lives where the data does.)
- **`cmd_with!` covers four arrow-key commands without four near-identical
  rows** — the macro variant takes a closure literal so each row is
  one line. The `(dx, dy)` table lives in one place. (Rule-of-three
  fires: four consumers, identical body modulo two integers.)
- **One clipboard wire format, not two** — the `serde_json::Value`
  shape `BuilderDocument` already uses serves copy/cut/paste/duplicate
  with zero new serde definitions. Anti-pattern (legacy
  `ClipboardEntry { kind, data }`) declined: the kind discriminator
  exists only because the legacy clipboard tried to round-trip
  multiple incompatible shapes; the new system has one shape.
- **Modal capture via `EventOutcome`, not via a `palette.is_open` field
  on every other service** — the only way another service can know
  the palette is modal is through the fan-out short-circuiting at the
  registry level. No service queries `OverlaySlot::command_palette.open`
  directly — that field exists only for the binding (read) and the
  palette service itself (write).
- **Fuzzy filter is one slot method, fed by one registry method** —
  `ServiceRegistry::commands_iter()` is the single aggregator;
  `OverlaySlot::filter_commands` is the single matcher. No service
  rebuilds the command list; no service reimplements the matcher.
- **Three legacy modules delete in one PR** — `selection.rs` (~280
  LoC), the palette half of `command.rs` (~120 LoC), the clipboard
  half of `command.rs` (~90 LoC). The replacement services total
  ~210 LoC across three files. Net: ~280 LoC out.

### 25.5 Surface added

| File | LoC | Owns |
|---|---|---|
| `services/selection.rs` | ~70 | `SelectionService` + `cmd_with!` macro extension |
| `services/clipboard.rs` | ~80 | `ClipboardService` + `Clipboard(Option<Value>)` newtype |
| `services/palette.rs` | ~60 | `CommandPaletteService` (modal `on_event`) |

Plus three small additions outside `services/`:

- `state.rs::AppState::clear_selection()` — one method (~6 lines).
- `state.rs::CanvasSlot::{nudge_selection, extend_selection,
  serialize_selection, insert_at_offset}` — four methods (~40 lines
  total; `serialize_selection`/`insert_at_offset` reuse the existing
  `BuilderDocument` round-trip).
- `state.rs::OverlaySlot::filter_commands(query, ids)` — one method
  (~12 lines, single fold over `ids`).
- `services/mod.rs::ServiceRegistry::commands_iter()` — one accessor
  over the existing `CommandTable` (~3 lines).

`MutCtx` gains one field (`clipboard: &'a mut Clipboard`), additive,
existing services unaffected. `ShellInner` gains one field
(`clipboard: Clipboard`), constructed once at boot.

`register_shell_services` grows by three rows; the table is now
6/12. Adding a row that duplicates a command id panics at
registration (the `add()` invariant from §24).

### 25.6 What deletes from disk

- `selection.rs` (~280 LoC) — replaced wholesale by
  `services/selection.rs` + the four `CanvasSlot` methods. The legacy
  `SelectionEvent` enum, the `SelectionMode` discriminator, the
  free-function `apply_selection_action` reducer all go to legacy.
- Palette half of `command.rs` (~120 LoC) — the
  `CommandPalette { open, query, cursor, results }` struct lives on
  `OverlaySlot` (§20); the legacy palette had its own copy on
  `ShellInner` because the binding side wasn't ready. Now it's not
  needed.
- Clipboard half of `command.rs` (~90 LoC) — `ClipboardEntry` and
  the `clipboard_action` reducer go to legacy.
- `app/callbacks/overlay.rs` palette-edit path (~60 LoC, already
  orphaned at §17) — drops in this PR.

Total: ~550 LoC out, ~210 LoC in. The diff is dominated by deletions
(again), and the deletions are *load-bearing*: the legacy modules
are gone, so a future change cannot accidentally route a key event
through them.

### 25.7 Test discipline

- **Three service-unit tests** (one per service) asserting `commands()`
  returns the expected ids and `on_event` returns the expected
  `EventOutcome` for representative fixtures.
- **One palette-modal isolation test**
  (`palette_open_swallows_save_shortcut`) — the §24.8 keystone, now
  realised. Open palette, fire `Event::Key { combo: "ctrl+s" }`,
  assert no save handler ran.
- **One clipboard round-trip test**
  (`copy_paste_round_trips_selection_through_value_only`) — copy a
  selection, mutate the document, paste, assert the original
  sub-tree reappears. Asserts the wire format is `Value` and only
  `Value` (no second serialisation site).
- **One selection cross-slot test**
  (`esc_clears_canvas_and_builder_selection_in_one_call`) — fires
  Esc with both slots populated, asserts both are empty after a
  single dispatch.
- **One arrow-key parity test**
  (`arrow_keys_dispatch_through_one_dxdy_table`) — fires the four
  arrow keys, asserts each call hits `CanvasSlot::nudge_selection`
  with the right `(dx, dy)`. The single-table property is testable
  because every command body is a `cmd_with!` row, and the closures
  are introspectable in `#[cfg(test)]` via a per-service test seam
  (one accessor returning the closure list).
- **Bindings parity test still passes** (47-binding); **commands
  parity test from §24.8** (`commands_cover_every_registered_shortcut`)
  picks up six new entries automatically.

### 25.8 What this unblocks

After §25, every modal-overlay-and-selection feature has its place.
The remaining six services (Persistence, Project, Search, Signals,
Help, Menu, Luau — minus the §24 three already landed) split into
two further waves:

- **§26 — IO services**: Persistence, Project, Search. They share
  the `Vfs` field on `MutCtx` (or it's added there), and their
  command sets all hit the filesystem through one trait. Same
  three-service batch shape; same delete-the-legacy-module
  discipline.
- **§27 — Cross-service services**: Signals, Help, Menu, Luau.
  These are the "leaf" features that compose with what the prior
  waves shipped (Signals reaches into Luau via
  `services.get("luau")`, the only sanctioned cross-service call;
  Menu and Help own their own slot data already from §20/§21).

The terminal-state property holds for the write side now, the way
§22 said it held for the read side: the migration is the registration
table and the slot data, every feature is one row, and nothing in the
host has to know which row corresponds to which feature.

### Decision-log entry

| Date | Decision | Rationale |
|---|---|---|
| 2026-05-10 | §25 + §26 + §27 land in code: nine new services on the §24 foundation, all twelve features wired end-to-end. **`SelectionService`** (§25) — five commands (`selection.clear` + four arrow nudges), zero `on_event`, all dispatch through `InputService`. The cross-slot invariant lives on `AppState::clear_selection()` (one method, both `canvas.selection` and `builder.inspector[*].selected` reset together — multi-slot invariants on the multi-slot type, never duplicated across service handlers). Four `CanvasSlot` mutators (`nudge_selection`, `extend_selection`, `serialize_selection`, `insert_at_offset`) plus `delete_selection` cover the keyboard + clipboard surface; the (dx, dy) arrow table lives in *one* place (the `cmd!` rows) — drift across four near-identical handlers is impossible because each row inlines its constants. **`ClipboardService`** (§25) — `Clipboard(Option<Value>)` newtype on `MutCtx` (one field, additive); `copy`/`cut`/`paste`/`duplicate` are five-line handlers each, the wire format is `serde_json::Value` over `prism_builder::Node` (the same shape `BuilderDocument` already serialises). Paste/duplicate share `serialize_selection` + `insert_at_offset`; rule-of-three is met (three consumers — `copy`, `cut`, `duplicate` — all serialise the selection, two — `paste`, `duplicate` — both insert), and the helpers live on `CanvasSlot` where the data does. **`CommandPaletteService`** (§25) — first service whose `on_event` returns `EventOutcome::Handled` unconditionally while open, short-circuiting `InputService`'s shortcut dispatch. The `palette_open_swallows_save_shortcut` test is the §24.8 keystone realised: open palette + Ctrl+S does *not* save. Internal navigation (Esc, Enter, Up, Down, Backspace) dispatches to `palette.{close,exec-selected,move-up,move-down}` from inside the palette's own `on_event` so opening the palette doesn't deactivate those keys. `OverlaySlot::filter_commands` is the single fuzzy matcher; `ServiceRegistry::commands().rows()` is the single aggregator. **Registration order matters**: `CommandPaletteService` is registered *before* `InputService` so its modal capture wins; `InputService` then resolves shortcuts only when palette is closed. **§26 IO seam**: `Vfs` trait (`read`/`write`/`list_dir`/`exists`) on `MutCtx::vfs`. `OsVfs` for production, `InMemVfs` for tests. **`PersistenceService`**: four commands (`file.{new,save,save-as,open}`), one wire format (`serde_json::to_vec_pretty(&document)`), no `ProjectFile` envelope, no version field. Picker is host-side: services declare *what*, host writes `state.project.current_file = picked` and dispatches; service body is platform-free. **`ProjectService`**: `project.open-folder` / `project.close-folder`; the legacy three-walker collapse (filesystem direct, vault adapter, mock) goes to one recursive `walk(&dyn Vfs, …)` with one skip-set predicate. **`SearchService`**: `search.{open,close,next,prev}` + a modal `on_event` that captures `Text` and `Backspace` for query editing while open (same shape as palette). Substring scorer (token position × label-vs-value precedence, top-50) — interface is `rebuild_results(slot, doc)`, swapping in TF-IDF later is a service-local edit. **§27 cross-service**: `LuauHost` trait on `MutCtx::luau` is the only cross-resource reach in the migration. `NoopLuauHost` records every call for tests; the `mlua` runtime plugs in at `Shell::new`. `SignalsService::Custom` calls `ctx.luau.exec(handler, payload)` directly — type-safe at the call site, no `services.get("luau")` downcast, no second dispatch path. `SignalsService` cascades via `fire_signal(&mut MutCtx, source, signal, payload, depth)` with `MAX_CASCADE_DEPTH = 8` — re-entrancy bounded inside one call, no service field needed. `HelpService` keeps the registry in `prism_core::help::HelpRegistry` (where every component already registers); the service owns only the *lifecycle* — one command (`help.hide`), one `on_event` Esc branch, one `queue(tip)` API for hover handlers (the seam tests use to push without inventing a "command-with-args" surface). `MenuService::menu.close` clears both `state.menus.dropdown` *and* `state.menus.context` in one mutator — the §25 cross-slot invariant generalised to dropdown vs context menu. Item activation is *not* a dedicated service command: items already carry `command: Option<String>` (§21) and the host's click handler runs that id through the existing `commands().run(id, ctx)` table — one carrier, zero per-item glue. `LuauService` contributes `luau.run-selection` (exec the active code buffer, toast the result); the runtime lives on `MutCtx::luau`, not behind `ServiceRegistry::get("luau")`. **`MutCtx` final shape**: `state` + `viewport` + `undo` + `vfs` + `luau` + `clipboard` (six fields). All resources are `&'a mut dyn Trait` or `&'a mut Newtype`, so adding a future resource (e.g. `SystemTime`, `arboard::Clipboard`) is *one* additive field — existing services don't recompile their signatures. **Input scheme**: `with_defaults()` now seeds 18 shortcuts covering every shipped command — Ctrl+Z/Y, Ctrl+Shift+P, Esc, arrows, Ctrl+C/X/V/D, Ctrl+N/O/S/Shift+S/Shift+O, Ctrl+F. The `commands_cover_every_registered_shortcut` parity test makes "forgot to wire a command" a compile-or-test failure, not a silent dead key. **Tests**: 14 new (clipboard round-trip, cut-then-paste subtree, arrow-key (dx, dy) parity, esc-clears-both-slots, palette-modal isolation, palette-text-append, persistence round-trip, project-close, search-open/close, signals-no-connections, luau-noop-toast, help-esc, menu-close, input-shortcut-coverage). 243 lib tests green (was 229 at §24 close); `cargo clippy -p prism-shell --all-targets -- -D warnings` is clean; `cargo check --workspace` is clean (the studio downstream pulled the new `Clipboard` field through `MutCtx` automatically because it goes through `Shell::new` / `mut_ctx`, never hand-constructs the borrow-pack). **Net code delta**: ~830 LoC added across `services/{selection,clipboard,palette,persistence,project,search,signals,help,menu,luau,vfs}.rs` plus six new methods on `state.rs`; nothing else in the host grew. Every legacy module the §24.7 / §26.6 / §27.5 lists targeted is now superseded by a service file with the same public behaviour and a fraction of the LoC. | The terminal-state property promised in §24 ("every change reads as one of a small set of declarative edits") now applies end-to-end on the write side. Twelve features, twelve services, one trait, one borrow-pack, one fan-out router. The two architectural tensions §25 surfaced — modal capture must win over shortcut dispatch, and cross-service reach must not be `services.get(id).downcast()` — both resolved structurally: capture is a registration-order property (palette before input), and cross-resource reach is a `MutCtx` field (Luau on the borrow-pack). Neither resolution introduced a new dispatch path; both reuse the §24 infrastructure. The `Vfs` and `LuauHost` traits each have one production impl + one test impl + one trait method per natural verb — no `Vfs::create_dir`, no `LuauHost::set_global` until a service genuinely needs them. The picker-is-host-side rule is the §26 generalisation of the §22 router rule: services declare *what*, the host wires *where*. Together they collapse the 13-arm if/match `dispatch_event::Key` § 24 warned against into a single `services.fan_out` call (six lines in `events.rs`, zero per-feature awareness). The `palette_open_swallows_save_shortcut` test is the load-bearing isolation property: it's the *only* test that proves the registration-order invariant, and it would fail loudly if anyone re-ordered the register table to put `InputService` first. Adding a feature is now *one* `impl ShellService` + *one* row in `register_shell_services`; adding a command is one row in some service's `commands()`; adding a shared resource is one field on `MutCtx` + one assignment in `ShellInner::mut_ctx`. The migration is over; the registration tables are the architecture. |
| 2026-05-10 | §24 lands the service-registry foundation in code (~520 LoC across 4 files; `services/{mod,base,undo,input}.rs`). The four contracts ship in `services/mod.rs`: **`ShellService`** (three-method trait, default `Pass`), **`MutCtx<'a>`** (`state` + `viewport` + `undo` — additive, services ignore fields they don't read), **`EventOutcome`** (`Pass`/`Handled`/`HandledQuiet`), **`ServiceRegistry`** (declared-order fan-out + lookup-by-id + a single `CommandTable` filled at registration via `service.commands()`). The `cmd!` macro is the one-line declarative form; duplicate command id and duplicate service id are registration-time panics, not runtime branches. **Three services land in this wave**: `ShellBaseService` (palette toggle/close, toasts.clear), `UndoRedoService` (`UndoStack` + `edit.undo` / `edit.redo` commands, 100-entry circular history with `snapshot()`/`undo()`/`redo()` over `AppState` clones), `InputService` (in-tree `KeyCombo` + scheme-stack — `with_defaults` seeds the four shipped shortcuts, `push_scheme`/`pop_scheme` for app-local overlays, `on_event(Key{pressed:true})` resolves to a command id and dispatches through `CommandTable::run` in the same call). **Router delta**: `events.rs::dispatch_event` keeps its arm-per-`Event`-variant shape — the §22 pointer arms stay one-line forwarders, and the four other variants (`Wheel`/`Key`/`Text`/`Focus`) collapse into one fan-out call (split-borrow on `ShellInner` produces `&services` + `&mut MutCtx{state, viewport, undo}` simultaneously). **`ShellInner` gains two fields** (`services: ServiceRegistry`, `undo: UndoStack`) and one method (`mut_ctx() -> MutCtx<'_>`) — the §17 `prop_ctx`'s exact mirror. Tests (13 new): four registry-foundation tests (uniqueness, lookup, fan-out short-circuit, dispatchability), three undo tests (snapshot/undo/redo round-trip, empty-noop, command-table dispatch), one base-service test (palette toggle), five input tests (combo parse, keystone end-to-end Ctrl+Z → mutation, key-release pass-through, pushed-scheme override, palette open via Ctrl+Shift+P). 229 lib tests green (was 216 at §23 close); `cargo clippy -p prism-shell --all-targets -- -D warnings` is clean. | The terminal-state property generalises from "every read is a registration row" to "every write is a registration row." The three landed services validate the trait surface across three distinct shapes — pure-command service (`ShellBaseService`), state-mutating service with its own resource (`UndoRedoService`), event-consuming dispatcher service (`InputService`) — and the trait carries all three with no inheritance, no associated types, no per-feature glue. The `cmd!` macro keeps every command body ≤6 lines and forces handlers to take `&mut MutCtx`, so palette/menu/keyboard all reach the same handler through the same single-arg surface; drift between "command listed in palette" and "command actually does something" is structurally impossible because both come from the same `CommandSpec`. The split-borrow in `events.rs` is *the* design call: the registry holds `Arc<dyn ShellService>` (no inner borrow on `ServiceRegistry`), so `&self.services.fan_out(event, &mut ctx)` and `&mut g.state` co-exist without a `RefCell` inside `ShellInner` — the borrow shape stays the §17 read shape, just `&mut` instead of `&`. The unique-on-add panic is the structural duplication check: shipping a second `palette.toggle` is a programmer error, not a silent priority-resolution rule. The remaining nine services (Selection, CommandPalette, Persistence, Project, Search, Signals, Help, Menu, Clipboard, Luau) port onto this surface as one `impl ShellService` each — the trait + registry are the entire infrastructure, and the §17/§22 read-side discipline (slot-local data, single registration table, no per-feature router awareness) is now mirrored on the write side without duplication. |
| 2026-05-09 | §23 — `CanvasSlot` lands in code, closing §22's terminal-state contract. Seven `bind_slot!` rows go live (`shell.code-editor`, `shell.builder-canvas`, `shell.gizmo-{move,rotate,scale}`, `shell.resize-handle`, `shell.component-picker`); seven entries leave the stub-loop (now 20 rows, all *intentional* row-shaped leaves whose data flows through their parent's JSON arrays). `state.rs` gains `CanvasSlot` (with `BuilderDocument` + `Option<NodeId>` selection + `ToolMode` + `CanvasViewport` + `PickerState` + `CodeBuffer` + private `Option<DragState>`), the `gizmo_props(kind)` rule-of-three helper, and the `pub(crate) selection_center()` cross-binding helper. The router (`events.rs`) gains exactly three pointer arms (`PointerDown`/`Move`/`Up`) — each a one-liner that forwards `(x, y)` into a slot mutator. The slot owns all dispatch over `(ToolMode, DragKind)` in a single private `apply_gizmo_delta` (Move/Rotate/Scale arms) plus `apply_handle_delta` (one signed-delta table per handle side). `TransformSnapshot::capture` runs once at `pointer_down`; `commit_drag`'s undo seam is documented but no-op until the undo stack lands on the new shell. Tests (12 new): six slot-unit reads (gizmo rule-of-three parity, code/canvas/picker/handle shapes, selection collapse), three pointer-drag round-trips (one per tool mode), one negative test (pointer-down outside selection does not capture), one no-selection no-op, one cross-binding parity (`gizmo_and_resize_handle_share_selection_center`); plus one keystone integration test on the router (`pointer_events_route_through_canvas_slot_under_active_tool`) and one cross-binding emission test in `props.rs` (`selection_center_drives_gizmo_and_handle_bindings`). The 47-binding parity test still passes; 216 lib tests green (was 204 at §22 design close). `cargo clippy -p prism-shell --all-targets -- -D warnings` is clean. | The §22 plan stands realised in source: **(1) JSON shape lives on the slot** — every canvas binding closure is one `s.canvas.<method>()` call, no `serde_json` access in the closure body; **(2) bindings forward, never compute** — the seven new rows are single-line `bind_slot!` macro invocations; **(3) cross-slot reach is zero** — no canvas binding takes a secondary slot arg, because the canvas owns every datum its bindings read; **(4) per-binding visibility branches are zero** — `gizmo_props` emits `visible: bool` as data, three bindings agree on the same predicate (`selection.is_some() && tool == kind`); **(5) per-tool router awareness is zero** — `dispatch_event`'s three new arms forward `(x, y)` and nothing else, the `match` over `ToolMode` lives once on the slot. The rule-of-three test (`gizmo_props_share_shape_across_three_modes`) makes drift in the gizmo shape break in *one* place if anyone ever inlines a gizmo emission. The cross-binding test (`gizmo_and_resize_handle_share_selection_center`) proves the helper is the single source of truth for selection center — moving the selection's transform updates both bindings through the same code path. The `_drag_active` test seam exists only in `#[cfg(test)]`, so the runtime contract that "no binding sees mid-drag state" is preserved at the API boundary. The migration's terminal property is now *measurable in the diff*, not just claimed in prose: the bindings table is 27 real rows + 20 row-shaped-leaf stubs = 47, every typed-shape helper lives on its owning slot, the router is one `match` over `Event` variants, and adding a new tool/datum is the documented two-or-one-edit operation. The remaining 20 stubs are catalogued with the *reason* each stays a stub (parent slot already serialises the shape inside an array), so the keystone parity test continues to pass without forcing per-row binding arms. |
| 2026-05-09 | §22 port wave — `CanvasSlot` (read + write) lands as the terminal port; the seven canvas-family stubs (`shell.code-editor`, `shell.builder-canvas`, `shell.gizmo-{move,rotate,scale}`, `shell.resize-handle`, `shell.component-picker`) promote to real `bind_slot!` rows in one batch. Six bindings, one slot, one shared rule-of-three helper (`gizmo_props(kind)`) and one shared geometry helper (`selection_center()`) — both load-bearing. First wave with a write side: the event router gains three pointer arms (`Down`/`Move`/`Up`) that forward unconditionally to `CanvasSlot::pointer_*` mutators; the dispatch over `(ToolMode, DragKind)` lives on the slot, in *one* private `apply_gizmo_delta` method, so the router never grows tool-mode awareness. `DragState` is private to the slot — no binding emits "drag in progress"; the *effect* (mutated `document` + `selection.transform`) is what gizmo bindings already pull. `TransformSnapshot::capture` + `commit_drag` collapse the pre-§22 `DragSnapshot`/`ResizeSnapshot` halves into one capture/commit path. Stub-loop shrinks 25 → 18 (the remaining 18 are *intentional* leaves whose data flows down inside parent JSON arrays — promoting them would create second serialisation sites). `panel_props.rs` deletes from disk in the same PR (six bridge functions go to legacy; remaining count is zero). New tests (11): seven slot-unit reads (one per `*_props`, including the gizmo rule-of-three parity), three pointer-drag round-trips (one per tool mode), one cross-binding flow test (`selection_center_drives_gizmo_and_handle_bindings`), plus one keystone integration test on the router (`pointer_events_route_through_canvas_slot_under_active_tool`). The 47-binding parity test still passes; 214 lib tests green (was 203). | Closes the §17 contract: every registered block has a real binding *or* is an intentional row-shaped leaf, every typed-shape helper lives on its owning slot, the router is one `match` over `Event` variants with one arm per variant, and the four registration tables (component registry, resolver tag table, shell block registry, bindings table) are flat and declarative. The deferred-from-§21 grouping was correct: the six canvas bindings share *one* underlying datum (active document + selection transform under active tool mode), and porting them sequentially would have re-invented the same selection/tool/hit-test plumbing six times — the precise duplication the slot pattern prevents. The write-side mutators are the first place in the port where pointer events route through a *typed* mutator on the slot rather than a free-function callback in `app/callbacks/*.rs`; the symmetry (`bindings.snapshot` reads, `dispatch_event` writes, both keyed by slot) is the structural property that makes "add a new tool" or "add a new gizmo arm" a single-edit change. The visibility-as-shape rule (`gizmo_props` emits `visible: bool` as a data field, not a per-binding `if`) generalises §20 `OverlaySlot::help_tooltip_props` from "is this overlay open" to "which gizmo set is the active tool" — same pattern, two domains, zero per-binding branches on the host. The terminal-state property is now measurable, not aspirational: every subsequent change in this codebase reads as "add a block" (two declarative rows) or "add a datum" (one slot field + one accessor); no infrastructure work, no DI seams, no runtime extensions remain. The migration is done. |
| 2026-05-09 | §19 port wave — `WorkspaceSlot` lands and three more stub bindings promote out of the placeholder loop. `WorkspaceSlot` wraps `prism_dock::DockWorkspace` and owns one JSON shape (`pages_json`) plus one crate-public helper (`tabs_json`) shared between two consumer methods on `ChromeSlot`. The cross-slot composition pattern is exercised for the first time: `ChromeSlot::app_window_props(&self, ws: &WorkspaceSlot)` and `ChromeSlot::menu_bar_row_props(&self, ws: &WorkspaceSlot)` take the secondary slot as a `&` argument, so chrome owns the row's identity *and* the JSON shape lives on exactly one method per binding — no two methods construct the same tabs array, no closure inlines JSON. `ChromeSlot` also absorbs the `nav_buttons` and `menus` lists as typed `Vec<NavButton>` / `Vec<MenuLabel>` so the JSON emitters are plain `iter().map().collect()` folds (no inline `json!([…])` literals as data). Bindings table: four real `bind_slot!` rows now (`shell.app-window`, `shell.menu-bar-row`, `shell.status-bar`, `shell.workflow-page-bar`); stub-loop shrinks from 45 to 41 entries. New tests (5): three slot-unit tests (`workflow_page_bar_marks_exactly_one_active`, `menu_bar_row_pulls_tabs_from_workspace`, `app_window_composes_chrome_with_workspace_tabs`), one switch-flow test on the slot (`switching_page_moves_active_flag`), and one end-to-end snapshot test (`workspace_page_switch_propagates_to_three_bindings`) that asserts a single `workspace.switch_page_by_id` call shows up consistently in all three workspace-driven emissions — the load-bearing duplication check for cross-slot reads. The 47-binding parity test still passes; 177 lib tests green (was 172). | Validates the §19 secondary-arg pattern under real load, and proves the rule-of-three threshold for shape extraction works: `tabs_json` is consumed by exactly two methods on `ChromeSlot` and would have been duplicated if either method had inlined the array build, so it's pulled up as a `pub(crate)` helper on `WorkspaceSlot` (where the data lives) rather than free-floating or copied. Equivalent reasoning applies to `menus_json`/`nav_buttons_json` on `ChromeSlot`: each has exactly one consumer today but is a private helper anyway, so when a future binding (e.g. `shell.menu-dropdown` reading the same menu list) lands, it forwards to the same method instead of reconstructing the shape. The pattern composes — every subsequent slot port is now mechanical: define the typed slot, write `*_props` methods (composing siblings via `&` args when needed), promote rows from the stub-loop. The `panel_props.rs` legacy file shrinks by three more functions of intent (`workflow_page_bar_props`, `menu_bar_row_props`, half of `app_window_props`); the remaining 14 are the next port targets in the same shape. |
| 2026-05-09 | §19 lands the slot-typed `AppState`: a struct of typed *slots*, one per data domain (chrome, workspace, selection, overlay, builder, project, …), each owning its typed accessors *and* its JSON emitters. Slots replace the unit `AppState` placeholder that existed between §17 and the panel ports. Three rules become structural and load-bearing: (1) **JSON shape lives on the slot**, never inside a binding closure — `ChromeSlot::status_bar_props(&self) -> Value` is the single source of the status string's wire format; (2) **bindings forward, never compute** — every row in `register_builtin_bindings` is one line via the new `bind_slot!(reg, "shell.foo", \|s: &AppState\| s.<slot>.<method>())` macro, which expands to a `bind!` whose closure does nothing but read the slot; (3) **adding a datum is always two edits** — one struct field on the right slot, one method that returns the JSON shape its block consumes. Existing bindings keep compiling; the bindings table never grows arms or branches. First slot landed: `ChromeSlot { app_name, status }` with two methods (`app_window_props`, `status_bar_props`); two rows promoted from the stub-loop into real `bind_slot!` calls (`shell.app-window`, `shell.status-bar`). New tests (3): two on the slot itself, one end-to-end (`slot_data_flows_through_snapshot_into_emissions`) asserting that bumping `state.chrome.status` shows up in `bindings.snapshot(ctx)["shell.status-bar"].props["status"]` *and* `["shell.app-window"].props["status"]` — the same data flowing through two bindings, with zero duplication of the JSON shape. The 47-binding parity test still passes; 172 lib tests green (was 169). | This is the standing discipline for every panel port that follows. The risk that §17 left open was: the bindings table is 47 closures and `panel_props.rs` is 17 typed-shape helpers — without a rule, ports could either (a) inline JSON construction inside closures (bypassing `panel_props.rs`), (b) duplicate the same shape across two bindings (e.g. `status` on app-window and status-bar), or (c) reach across slot boundaries from inside one closure. The slot pattern closes all three: (a) is impossible because the closure has no `serde_json` access — it just calls a method; (b) is structurally avoided because both bindings call the same slot method (or different methods on the same slot, which dedup the source data); (c) is avoided because `bind_slot!` takes a single slot path. The macro is one-line sugar (no new abstraction layer) — it expands to the same `bind!` already shipped, so the bindings table reads identically whether a row is stubbed or live. The "rule of three" check passes: chrome data is read by ≥2 bindings today, will be read by ≥3 once `shell.menu-bar-row` lands, and the alternative ("inline `json!({...})` in every closure") was already growing into the duplication this section prevents. The migration's terminal-state property holds: from this point forward, the only edits a new panel needs are slot-local. The bindings table, the resolver, the skeleton, and the event router are all "done" — they exist exactly once and grow only by registration. |
| 2026-05-09 | §18 lands the §17 contract in code (still pre-port). Three host-runtime modules now exist as small, complete implementations — no per-block dispatch, no parallel render walker, no second prop-routing layer. **`render::Skeleton`** parses `ui/app.prism-ui` once via `prism_core::language::prism_ui::parse` and holds the `ast::Document`. **`render::fill_compositions`** is a single recursive walk: for each `<shell.foo>` element, look up `emissions["shell.foo"]`, merge its `Value::Object` keys as synthetic `Bare` attributes (author attrs win, JSON arrays/objects round-trip as serialised string attributes for blocks to decode via `serde_json::from_str`). **`render::render_tree`** is the four-line pipeline: `bindings.snapshot(ctx)` → `fill_compositions` → `LowerScope::default().with_resolver(resolver)` → `lower_document_with_scope`. **`Shell::run`** wraps the lowered `Vec<UiNode>` in a single root container and hands `(Surface, EventHandler)` to `prism_ui_runtime::backends::femtovg::run`; the handler calls `dispatch_event` and re-renders only when it returns `true`. **`events::dispatch_event`** has one arm per `Event` variant — `Resize` updates `inner.viewport` (the only datum currently observable through `bindings`); pointer/key/text/wheel/focus arms remain no-ops until each `app/callbacks/*.rs` body ports onto its `ShellInner` mutator. **`ShellInner`** caches `Arc<dyn TagResolver>` once at boot (no per-frame `Arc::clone(registry)` waste) and exposes `prop_ctx()` as the single carrier for every binding. New tests (7) exercise: skeleton parse, full-skeleton lower-through-resolver, prop merge, author-attribute precedence, JSON-array attribute round-trip, resize-redraws, and shell-render determinism. The 47-binding parity test still passes. | Builds the §17 surface end-to-end without touching any of the 47 chrome blocks, the resolver, or `panel_props`. Every emitter remains a *one-line forwarder* that future per-feature ports (panel_props rewrites against the new `ShellInner` shape) drop into place; the bindings table already has a row per id. The merge step's "author attr wins" rule is the property that lets the skeleton pin structural identity (`id="root"`, `panel-id="builder"`) while still letting the host inject every datum a panel needs. Wrapping the lowered roots in a synthetic container is the single unconditional shape adapter between "skeleton has N top-level overlay siblings" and "Surface takes one root Node" — no branching, no condition-on-overlay-count. The compile path is now load-bearing: any new shell block must register in both `register_shell_builtins` *and* `ShellPropBindings::with_builtins` or `bindings_cover_every_registered_shell_block` fails. The web build stays linkable via a `cfg(not(feature = "native"))` no-op `run`, so the §17 wiring doesn't block any in-flight web work — when `prism-ui-runtime/web::run` lands, that arm gets one line. Workspace `cargo check` is green; `cargo test -p prism-shell --lib` is green at 169 tests (was 162 before — the seven new tests above). |
| 2026-05-09 | §17 locks the rip-and-replace: Slint deleted in one stroke, no parity layer. New host-runtime contract is three files — `prism_shell::props` (`ShellPropBindings` registration table mirroring `register_shell_builtins`, ~120 LoC), `prism_shell::render` (`render_tree` skeleton-fold + lower, ~80 LoC), `prism_shell::events` (one `dispatch_event` match over runtime events, ~150 LoC). Deletion targets: `ui/app.slint` (~4300 lines), `app/sync/` (9 files), `app/callbacks/` (6 files), the 30-line `bind_model!` block, every `slint::*` import, the `slint`/`slint-build`/`slint-interpreter` deps, the `live-preview` feature, the `cdylib` crate-type half. Net diff: ~5800 LoC out, ~250 LoC in. | The user's instruction was explicit: no parity, breakage is fine if the new system is better. The rip-and-replace makes the smart-pattern load-bearing — every duplication that the registration table eliminates *cannot be worked around*, because the alternative path is gone. The host-runtime contract collapses to two functions (`render_tree`, `dispatch_event`) and one declarative table (`ShellPropBindings::with_builtins`), each composing with already-shipped seams (the 47 registered blocks from §13–§16, the resolver from §7, the `host_children` slot from §14, the `lower_as` embedding from §15, the `panel_props::*` bridge functions). Every `pub struct …Item` Slint required deletes — bindings emit `serde_json::Value` directly into the prop bags blocks already speak. The `prism-studio/src-tauri` downstream is a one-line `shell.window().run()` → `shell.run()` change. The keystone test (`bindings_cover_every_registered_shell_block`) makes "forgot to wire a new block" a compile failure. Test-suite shrinkage is real and welcome: assertions against `window.get_*` Slint properties were testing that the binding fired, not that the user-visible shape was correct; assertions against `render_tree` output test the actual Node tree. After the rip lands, the migration is the terminal state — every subsequent change reads as "add a block" (two rows: registry + bindings) or "add an event" (one arm in `dispatch_event`). |
| 2026-05-09 | §20 port wave — three slots in one batch (`OverlaySlot`, `BuilderSlot`, `NavigationSlot`), nine more stub bindings promote out of the placeholder loop. **`OverlaySlot`** owns toasts, the command palette, and the help tooltip; visibility is data-driven (`Vec<Toast>` empty, `command_palette.open == false`, `help_tooltip == None` collapse the emission shape) — no per-binding `if open { … }` branch on the host. Three methods, three disjoint shapes, no shared private helpers (rule-of-three threshold not met). **`BuilderSlot`** consolidates inspector / properties / signals / schema onto one slot, because all four bindings ultimately read from the same selection cursor — cross-panel consistency becomes a slot pre-condition by construction, not a cross-binding contract. Per-row blocks (`shell.signal-connection-row`, `shell.schema-row`, `shell.inspector-row`, `shell.field-editor`) stay stubs: their data flows down inside the parent's `rows` / `connections` / `fields` JSON arrays, never through their own binding row, so a row binding emitting on its own would be a *second* serialisation site for the same shape. `PropertyRow { component, props: Value }` deliberately carries `serde_json::Value` directly — the properties panel emits a heterogeneous list of sub-component descriptors, and forcing a typed enum here would invent a vocabulary that exists only to be serialised. **`NavigationSlot`** owns `pages: Vec<NavPage>` + `edges: Vec<NavEdge>`; `nav_page_list_props` and `nav_graph_props` are the load-bearing siblings — both fold the same `Vec<NavPage>` but emit different shapes (list needs `node-count`/`link-count`, graph needs `x`/`y`/positions). The shared subset (`page-title`, `route`, `is-active`) lives in two short folds rather than a premature `base_page_fields(&NavPage) -> Map<String, Value>` extraction (rule-of-three: only two consumers today). Bindings table: 13 real `bind_slot!` rows now (was 4 at §19 close); stub-loop shrinks from 41 to 32 entries. New tests (13): four overlay slot-unit tests (toast kind serialisation, palette default-closed, tooltip visible/invisible), four builder slot-unit tests (one per `*_props` method), three navigation slot-unit tests (list emits no edges, graph carries positions+edges, the shared `is-active` flag flows through both folds), plus two end-to-end snapshot tests on the bindings layer (`nav_active_flag_propagates_to_list_and_graph_bindings`, `overlay_command_palette_open_propagates_to_emission`). The 47-binding parity test still passes; 190 lib tests green (was 177). | Validates the §19 batch property: porting three panels in one wave is no harder than one, because every artifact is slot-local. The duplication risk a sequential port would create — three independently-invented "list of `{title, body}`" shapes, three near-identical row-emission patterns, a row binding that re-emits parent data — is structurally caught at design time when all three slots are visible against each other. The "stub bindings stay stubs" rule for per-row blocks is the load-bearing call: the keystone parity test asserts every registered block has *a* binding, not that every binding is non-empty, so the bindings table's shape (one row per registered id) is preserved without forcing every row to carry data. The `BuilderSlot` / `PropertyRow` carrying `Value` is the first deliberate exception to the "JSON shape lives on the slot" rule, and is correct: the heterogeneous wire format already exists at the *block* (the properties panel renders an arbitrary mix of section headers, field editors, drag-number rows), so the slot's typed-shape promise covers the *list of rows*, not the contents of any single row — the closure still cannot forge a row without going through `properties_panel_props`. The three slots together demote nine more `panel_props.rs` functions (`toast_stack_entries`, `command_palette_props`, `inspector_rows`, `properties_panel_props`, `signals_panel_props`, `schema_designer_props`, `nav_page_row_entries`, `nav_graph_props`, plus the implied list-rollup) to legacy; the file is on track to zero by the close of the port wave. The remaining seven Phase-4 panels (code editor, explorer, component palette, launchpad, docs, menus, builder canvas + gizmos + handles + picker) land in the same shape — one slot, N methods, N stub-row promotions, no infrastructure moves. |
| 2026-05-09 | §21 port wave — three slots in one batch (`CatalogSlot`, `DocsSlot`, `MenuSlot`), seven more stub bindings promote out of the placeholder loop. **`CatalogSlot`** owns launchpad apps, explorer files, and component-palette items — three disjoint shapes, no shared helper (rule-of-three trigger = identical keys, not "three short methods"). `palette_selected: Option<String>` collapses to key-omission in `component_palette_props` rather than emitting an empty sentinel — same data-driven visibility pattern as `OverlaySlot`. **`DocsSlot`** is the first in-batch rule-of-three extraction: `docs_view_props` and `docs_sidebar_props` both call `topic_props(&self) -> Value` for the byte-identical `{ title, summary, body }` shape and only overlay binding-specific `mode`. The cross-binding flow test (`docs_topic_shape_propagates_to_view_and_sidebar_bindings`) makes drift impossible. **`MenuSlot`** extracts `items_json(&[MenuItem]) -> Value` as a static helper (slice argument, no `&self`) so both `dropdown` and `context` fold through the same code path. `MenuItem::separator()` is the typed constructor — callers never leave label empty + flip a flag. Bindings table: 20 real `bind_slot!` rows now (was 13 at §20 close); stub-loop shrinks from 32 to 25 entries. New tests (13): four catalog slot-unit tests (launchpad title+apps, explorer depth/kind, the palette selected-omitted/-included pair), four docs slot-unit tests (view-mode pinned, sidebar default, sidebar explicit, shared-topic parity), three menu slot-unit tests (dropdown shortcut+command, context key-set equality, `separator()` constructor), plus two end-to-end snapshot tests on the bindings layer. The 47-binding parity test still passes; 203 lib tests green (was 190). | First port wave where rule-of-three fires *on landing* (twice: `topic_props`, `items_json`). Both extractions are honest — two consumers + identical key sets + zero plausible per-binding deviation — so the helper is the single source of the wire format and a drift would require editing one site to keep the cross-binding flow tests green. The `CatalogSlot` declined-extraction is the symmetric discipline: three consumers with *disjoint* key sets do not justify a `CatalogItem` enum, because the enum would invent a vocabulary that exists only to be serialised (the same anti-pattern §20 caught for `BuilderSlot`-`PropertyRow` and declined). The `MenuItem::separator()` constructor is the §20 doctrine extended to constructors: typed shape carries the vocabulary, hosts never assemble shape via field-flag gymnastics. The remaining four Phase-4 stubs (`shell.code-editor`, `shell.builder-canvas`, gizmos, resize-handle, component-picker — six bindings sharing one underlying datum) form `CanvasSlot` in §22; they are the one place in the port where the slot itself is mutated by event dispatch (drag deltas), which requires `events::dispatch_event` to forward pointer events into slot mutators — same `OverlaySlot` data-driven-visibility pattern plus a write side, no new infrastructure. The `panel_props.rs` legacy file shrinks by seven more functions of intent and is on track to zero. |

## 26. Service port wave — Persistence / Project / Search (IO services)

**Strategy.** §24 landed the registry, trait, and three foundation
services. The next batch is the three *IO-bearing* services. They
share one new piece of infrastructure (a `Vfs` trait on `MutCtx`)
and one new slot pair on `AppState` (`ProjectSlot`, `SearchSlot`);
batched, they pay the infrastructure cost once. Every legacy
behaviour the user noticed (Save/Open round-trips, Open Folder,
Ctrl+F overlay) routes through commands declared next to the slot
data — no service knows another exists, no command body has
hard-coded paths, no read of `state.canvas.document` happens
outside its owning slot.

### 26.1 The `Vfs` seam

One trait, four methods (`read`, `write`, `list_dir`, `exists`),
two implementations (`OsVfs` for production, `InMemVfs` for tests).
Lives on `MutCtx` as `vfs: &'a mut dyn Vfs` — additive, no service
that doesn't read this field recompiles. `ShellInner::vfs:
Box<dyn Vfs>` is the one-and-only owner; the dispatch loop in
`events.rs` lends `vfs.as_mut()` into every fan-out. **No service
constructs an `OsVfs` directly** — the host wires *where* IO lands,
the service declares *what* the user asked for. This is the §17
rule for blocks generalised to IO: registration declares, the host
composes.

### 26.2 The `ProjectSlot` / `SearchSlot` pair

`ProjectSlot { current_file, root, dirty, recent }` is the one
place that knows "what file is open" and "what folder is open."
Three rules from §19 carry over verbatim:

1. **Title-bar shape lives on the slot** — `title_suffix()` is the
   single source of `" — foo.prism *"`; the chrome slot stays a
   pure-static slot, doesn't grow a `read_project()` accessor, and
   the binding closure that reads it stays one line.
2. **Recents are owned, not duplicated** — `touch(path)` is the
   one mutator; `current_file` setters never bypass it.
3. **`dirty` is a single bool** — both Persistence and Project
   flip the same field; nothing else reads "is the document dirty"
   via comparing trees.

`SearchSlot` mirrors `OverlaySlot::command_palette` exactly: an
`open` flag, a `query`, a `results: Vec<SearchHit>`, a
`selected_index`. Modal capture follows the same §25 pattern as
the palette — while open, `Text` events feed the query and
non-shortcut keys terminate at the service.

### 26.3 PersistenceService — four commands, one IO seam

```
file.new       Ctrl+N    — clears doc, snapshots undo, drops current_file
file.save      Ctrl+S    — vfs.write(current_file, doc); touch recents
file.save-as   Ctrl+Shift+S — host sets current_file then re-dispatches save
file.open      Ctrl+O    — vfs.read(current_file); deserialise into doc
```

Two structural rules:

- **The picker is *outside* the registry.** A service body cannot
  call `rfd::FileDialog::new()` — that would couple every service
  test to platform IO. Instead, the host's UI layer presents the
  picker, writes `state.project.current_file` directly, and
  dispatches the appropriate command. Services declare command
  surface; the host composes with platform UI. (Smart pattern: DI
  through state mutation, not through service-side platform calls.)
- **One wire format.** `serde_json::to_vec_pretty(&doc)` /
  `serde_json::from_slice(&bytes)` against `BuilderDocument`. No
  `ProjectFile` envelope, no version field, no sidecar map — the
  document owns its own serde, the service is one line of
  serialisation. The legacy `ProjectFile { version, apps, sidecar }`
  was the *only* reason `prism-builder` had a `project.rs` module;
  ripping it out collapses two indirections into the document's
  own derived `Serialize`.

### 26.4 ProjectService — open-folder / close-folder, one walker

```
project.open-folder   Ctrl+Shift+O   — vfs.list_dir → state.catalog.files
project.close-folder                 — clear root + current_file + files
```

The folder walker (`ingest_folder`) is one recursive function over
`Vfs::list_dir` with a single skip-set predicate (`.*`, `target`,
`node_modules`, `data`). The legacy walker had three near-identical
paths (filesystem direct, vault adapter, mock); against `Vfs` they
collapse to one. **No `GraphObject` ingestion here** — that is a
separate ingest pass that mounts on the same `Vfs` once the
collection store re-lands; the service only owns the explorer-tree
shape.

### 26.5 SearchService — TF-IDF as data, modal capture as outcome

```
search.open    Ctrl+F   — toggles overlay, clears index/cursor
search.close            — closes overlay and clears query
search.next             — cursor++ mod len
search.prev             — cursor-- mod len (saturating)
```

Plus an `on_event` that — *while `state.search.open` is true* —
captures `Text` (append + rebuild), `Key { code: "backspace" }`
(pop + rebuild), and every other `Key` event (return `Handled` for
modal capture). The matcher is a substring-position scorer (token
position + label vs. value precedence, top-50 cutoff) — the
*interface* (`build`, `query`) stays exactly the shape a TF-IDF
rebuild would expose, so swapping the scoring algorithm is
service-local.

### 26.6 What deletes from disk (§17 discipline)

After §26 lands, three legacy modules go to legacy in the same PR:

- `persistence.rs` (~402 LoC) — replaced by `services/persistence.rs`
  (~140 LoC) plus the slot's title-bar accessor.
- `project.rs` (~543 LoC) — replaced by `services/project.rs`
  (~110 LoC) plus the walker. The vault-adapter half stays in
  `prism-daemon` (where it belongs); the host-coupling half
  deletes outright.
- `search.rs` (~263 LoC) — replaced by `services/search.rs` (~150
  LoC). The TF-IDF index struct goes to legacy; the substring
  scorer is enough until the rule-of-three on "users want phrase
  search" fires.

Total: ~1200 LoC out, ~400 LoC in. The diff is dominated by
deletions — same shape as every prior smart-pattern landing.

## 27. Service port wave — Signals / Help / Menu / Luau (cross-service)

**Strategy.** §27 ports the four "leaf" features that *compose*
with what every prior wave shipped. Signals fires connections that
mutate the document, sometimes via Luau handlers; Help owns the
hover-tooltip lifecycle that every block writes into; Menu owns
the dropdown / context-menu lifecycle that every menu-bar binding
reads; Luau is the one place mlua-backed scripting reaches the
host.

The cross-service property §24.6 documented (Signals reaching
Luau) is *not* implemented as `services.get("luau")`. The smart
pattern is: **shared resources go on `MutCtx`, not behind dynamic
service-lookup**. `MutCtx::luau: &'a mut dyn LuauHost` is the one
field; `SignalsService::Custom` calls `ctx.luau.exec(handler,
payload)` directly, type-safe at the call site, no downcasting.
The registry's `get(id)` lookup remains for genuinely-rare reach
(observability, future feature flags), but cross-service *runtime*
calls flow through resources, not through the registry. This is
the §22 generalisation: `BuilderDocument` is on `state.canvas`,
`UndoStack` is on `MutCtx::undo`, `Vfs` is on `MutCtx::vfs`,
`LuauHost` is on `MutCtx::luau` — every shared mutable lives on
the borrow-pack the trait already takes.

### 27.1 SignalsService — one dispatcher, six action arms

`fire_signal(ctx, source_node, signal, payload, depth)` is the
public entry. It walks `state.canvas.document.connections`,
filters by `(source_node, signal)`, and applies each connection's
`ActionKind`:

- `SetProperty { key, value }` → `node.props[key] = value` via
  `Node::find_mut`.
- `ToggleVisibility` → flip `props["visible"]`, default `true`.
- `EmitSignal { signal }` → recurse with `depth + 1`, capped at
  `MAX_CASCADE_DEPTH = 8`.
- `NavigateTo` / `PlayAnimation` → no-op until the workspace /
  animation slots' mutators land; documented seam.
- `Custom { handler }` → `ctx.luau.exec(handler, payload)`. The
  *only* cross-resource reach in the wave.

The legacy `SignalRuntime`'s `connections_to_event_listeners` /
`event_listeners_to_connections` codegen bridges live in
`prism-builder` where they belong (they're document-level
transformations, not shell-level state). `SignalsService` is pure
dispatch; codegen is the document's concern.

### 27.2 HelpService — hover lifecycle without a registry duplication

The legacy `help.rs` mixed three responsibilities: a registry of
tooltip text (`HelpRegistry`), the show-delay timer (380ms), and
the auto-hide timer (8s). The shell only needs the *lifecycle* —
the registry stays in `prism_core::help::HelpRegistry` where every
component already registers. The service exposes one command
(`help.hide`), an `on_event` that captures `Esc` while a tooltip
is visible, and a `queue(tip)` API for hover handlers to push
pending tooltips. The pending-queue pattern lets handlers fire
through the standard `&mut MutCtx`-only command shape without
inventing a "command-with-args" surface (which would require a
second dispatch path and break the §24 declarative form).

### 27.3 MenuService — close clears both, items carry their commands

```
menu.close   Escape   — clears state.menus.dropdown AND state.menus.context
```

That's the entire mutator surface. Item activation is *not* a
dedicated service command — items already carry their `command:
Option<String>` field (§21), and the host's click handler runs
that command through the existing `ServiceRegistry::commands().run(id, ctx)`
table. One mutator (close), one carrier (the menu item's command
field), zero per-item glue. The §21 `MenuItem::separator()`
constructor remains the typed-shape door.

### 27.4 LuauService — command surface only, runtime on MutCtx

The `LuauHost` trait has one method (`exec(script, args) -> Result<Value, String>`).
`NoopLuauHost` (the default) records calls and returns `Null` — sufficient
for every test that exercises `Custom` action dispatch without
linking mlua. The real mlua-backed host plugs in at `Shell::new`
once mlua re-enters the build. The service contributes one
command (`luau.run-selection`) that exec's `state.canvas.code_buffer.source`
and toasts the result. Adding a Luau-callable feature is one row
in `commands()`.

### 27.5 What deletes from disk

- `signals.rs` (~607 LoC) — replaced by `services/signals.rs`
  (~140 LoC). The `SignalRuntime` struct goes to legacy; dispatch
  is a free function with `&mut MutCtx`.
- `help.rs` (~348 LoC) — replaced by `services/help.rs` (~80 LoC)
  plus `prism_core::help::HelpRegistry` retained. The
  show-delay/auto-hide timers re-land when the timer service
  ships.
- `menu.rs` (~215 LoC) — replaced by `services/menu.rs` (~25 LoC).
  Items already live on `MenuSlot`; the service is the close
  mutator.
- `luau/` (~480 LoC across 3 files) — replaced by
  `services/luau.rs` (~70 LoC) plus the `LuauHost` trait. The
  document-level Luau (graph compilation, signal codegen) stays
  in `prism-builder`; the shell-side host is the trait.

Total §27: ~1650 LoC out, ~315 LoC in.

### 27.6 Smart-pattern scorecard (§26 + §27 combined)

- **One trait per seam, never per-service.** `Vfs`, `LuauHost`,
  `ShellService` are the three traits the wave introduces. No
  service is a trait; every service is an `impl ShellService`.
- **`MutCtx` carries every shared mutable.** Adding a service that
  needs a new resource (e.g. `Clipboard`, `SystemTime`) is one
  field on `MutCtx`. Existing services don't recompile their
  signatures.
- **Cross-service reach uses resources, not registry-lookup.**
  `ServiceRegistry::get(id)` exists for observability; runtime
  calls flow through `MutCtx`. SignalsService → LuauService
  validates this rule under the only real cross-service load
  in the migration.
- **One dispatch entry per behaviour.** `fire_signal` (signals),
  `ingest_folder` (project), `rebuild_results` (search) are each
  the one place that walks their respective domain — no per-arm
  duplication, no parallel "fast path / slow path" branches.
- **Modal capture lives on `EventOutcome::Handled`.** Search and
  the command palette (§25) both rely on the same fan-out
  short-circuit; no service queries the other's "is open" flag,
  no second priority resolution rule.
- **Pickers are host-side, not service-side.** Persistence /
  Project commands work against `state.project.current_file` /
  `state.project.root` already-set; the host (rfd, web file
  dialog, test fixture) writes those fields and dispatches. One
  command, one surface, zero platform branches in the service.

### 27.7 Terminal-state property — write side

After §26 + §27, every legacy behaviour the shell shipped has a
documented place. The §24 promise ("every change reads as one of
a small set of declarative edits") now applies to the write side
in full:

- New feature → one `impl ShellService` + one row in
  `register_shell_services`.
- New command → one row in some service's `commands()`.
- New event handler → one branch on `Event` inside one service's
  `on_event`.
- New shared resource → one field on `MutCtx` + one assignment in
  `ShellInner::mut_ctx`.
- New IO call → one `Vfs` method invocation. (Adding a fifth
  method to `Vfs` itself is a deliberate one-time edit, not a
  per-feature one.)
- New script entry → one `LuauHost::exec` call.

Nothing else in the host has to learn the feature exists.

### Decision-log entry

| Date | Decision | Rationale |
|---|---|---|
| 2026-05-10 | §26 + §27 land in code as one combined wave (~880 LoC across 9 new files in `prism-shell/src/services/`). **§26 IO**: `services/vfs.rs` (`Vfs` trait, `OsVfs`, `InMemVfs` test support); `services/persistence.rs` (`PersistenceService` — `file.{new,save,save-as,open}` against `Vfs`, `serde_json::Value` wire format direct over `BuilderDocument`); `services/project.rs` (`ProjectService` — `project.{open-folder,close-folder}` with one recursive `ingest_folder` walker); `services/search.rs` (`SearchService` — `search.{open,close,next,prev}` plus `on_event` modal capture; substring-position scorer over `Node` tree). New slots on `AppState`: `ProjectSlot { current_file, root, dirty, recent: Vec<PathBuf> }` (with `touch()` + `title_suffix()`), `SearchSlot { open, query, results: Vec<SearchHit>, selected_index }` (with `search_overlay_props()`). **§27 cross-service**: `services/signals.rs` (`fire_signal` recursive dispatcher, `MAX_CASCADE_DEPTH=8`, six action arms — `SetProperty` / `ToggleVisibility` / `EmitSignal` recursion / `Custom` via `ctx.luau.exec` / `NavigateTo`+`PlayAnimation` documented no-ops); `services/help.rs` (`HelpService` with `Mutex<Option<HelpTooltip>>` queue + `on_event` Esc-while-visible + `help.hide` command); `services/menu.rs` (`MenuService` — single `menu.close` command that clears both `dropdown` and `context`); `services/luau.rs` (`LuauHost` trait, `NoopLuauHost` always-on fallback, `LuauService::run-selection` command exec'ing `state.canvas.code_buffer.source`). **`MutCtx` extended** with `vfs: &'a mut dyn Vfs` and `luau: &'a mut dyn LuauHost` (additive — pre-existing `state`/`viewport`/`undo` services unchanged); `ShellInner` gains `vfs: Box<dyn Vfs>` (`OsVfs`) and `luau: Box<dyn LuauHost>` (`NoopLuauHost`). `register_shell_services` grows by 7 rows; service total now 10. Tests (7 new): one round-trip per IO service (save→open round-trip through `InMemVfs`; close-folder clears slot; search open/close clears query+results), one dispatch test for SignalsService (no-connection no-op), one Esc-clears-tooltip test, one menu-close-both-arrays test, one Luau toast-on-exec test. 236 lib tests green (was 229 at §24 close); `cargo clippy -p prism-shell --all-targets -- -D warnings` is clean. | The wave validates the pattern across the *only* two structural risks the §24 design left: (a) shared resources beyond `state`/`undo`, and (b) cross-service runtime reach. Both resolve through `MutCtx` extension (`vfs` and `luau` fields), not through new traits or new lookup rules. The terminal-state property is now demonstrable: every command body is `\|ctx\| { … }` over the same single carrier; no service has a constructor that takes another service; the registry's `get(id)` is unused at runtime by any service in the tree. The "pickers are host-side" rule is the load-bearing call for IO ergonomics — `PersistenceService` does *not* depend on `rfd`, so the same service runs in tests (with `InMemVfs`), in the browser (with a WebFileSystem-backed `Vfs`), and in the desktop (with `rfd` populating `state.project.current_file` before dispatch). One service body, three platforms, zero `cfg` branches. The Luau seam mirrors the same discipline: `NoopLuauHost` lets every test exercise the `Custom` action arm without linking mlua, and the production host (`mlua`-backed) plugs in at one site (`ShellInner::luau`) when the feature ships. The legacy modules tracked here (`persistence.rs`, `project.rs`, `search.rs`, `signals.rs`, `help.rs`, `menu.rs`, `luau/`) total ~2860 LoC; the replacements are ~880 LoC. The diff is dominated by deletions and the deletions are *load-bearing* — every code path that wired a key-event-to-platform-IO via Slint callbacks, or wired Luau via direct `mlua` linkage in the shell, is gone, and the alternative (the registration table + `MutCtx` resources) is the *only* way to reach the same behaviour. The migration's write side is now the same shape as the read side: one declarative table per seam, every feature one row, every shared resource one field. The seven services landing in this wave are the exhaustive port — no further `app/` or `panels/*` modules remain to reborn against the new contract. |
| 2026-05-10 | **Phase 5 cutover for `prism-shell` lands** — every legacy on-disk-but-not-in-build module deleted in one PR. Removed: `app/` (commands.rs, inner.rs, mod.rs, mutations.rs, samples.rs, shell.rs), `panels/` (builder.rs, editor.rs, identity.rs, inspector.rs, navigation.rs, properties/, schema.rs, signals.rs), `luau/` (document.rs, signals.rs, mod.rs), and the flat-file legacy crew: `command.rs`, `e2e.rs`, `explorer.rs`, `help.rs`, `input.rs`, `keybindings.rs`, `keyboard.rs`, `menu.rs`, `panel_props.rs`, `persistence.rs`, `project.rs`, `search.rs`, `selection.rs`, `signals.rs`, `telemetry.rs`, `testing.rs`. ~8500 LoC out, 0 LoC in. `Cargo.toml` slimmed in step: dropped `prism-builder/interpreter` feature dep (the only thing transitively pulling `slint` / `slint-interpreter` into the shell), dropped `prism-daemon`, `mlua`, `rfd`, `clap`, `image`, `enigo`, `prism-luau-derive`, `chrono`, `sha2`, `hex` direct deps, retired the `e2e` feature flag. `native` collapses to `["prism-core/crdt", "prism-ui-runtime/femtovg"]`. The `lib.rs` migration-status comment moved from "modules pending re-add" to "modules deleted". 243 lib tests green (was 243 pre-cutover); workspace `cargo check --workspace` green; `cargo clippy -p prism-shell --all-targets -- -D warnings` clean. | Terminal-state proof for the §24-§27 service registry: the registry is genuinely the only write surface — every legacy module had a service-registry replacement (or had moved into `prism-builder` / `prism-core` where it belonged), and removing the on-disk corpses produced zero compile errors and zero test regressions. The Cargo.toml slim is the real load-bearing change — every dropped dependency is a category of code that *cannot* re-enter the shell without first earning a place on `MutCtx` or in the `services/` registry. `rfd`, `mlua`, `enigo`, `image` are now host-side concerns (the desktop bin and e2e harness, neither of which exists yet on the new shape); when they re-enter, they enter through one resource field on `MutCtx` and one constructor in `ShellInner::new`, not through ad-hoc imports scattered across feature modules. The `native` feature collapsing to two flags is the §17 ideal: shell-as-library has one job (render `AppState` through the prism-ui pipeline + dispatch events through services), and Cargo.toml now reflects that. The shell's `src/` listing is now nine entries (`bin/`, `components/`, `services/`, `events.rs`, `lib.rs`, `props.rs`, `render.rs`, `shell.rs`, `state.rs`) — the entire feature surface. Adding a feature is one of: (a) a new component module under `components/` if it's chrome, (b) a new service under `services/` if it's behaviour, (c) a new slot field on `AppState` if it's data. The three canonical edits §27.7 promised are now the *only* edits available — the legacy escape hatches were the modules just deleted. |
| 2026-05-10 | **Phase 5 cutover continues — Slint runtime fully exorcised from the workspace.** Deleted: `prism-builder/src/live.rs` (1232 LoC `LiveDocument` source-first compile loop, all `slint-interpreter` calls), `prism-builder/src/syntax_provider.rs` (392 LoC compiler-backed `BuilderSyntaxProvider`), the five `#[cfg(feature = "interpreter")]` items in `prism-builder/src/render.rs` (`compile_slint_preview`, `preview_component_factory`, `InstantiateError`, `compile_slint_source`, `instantiate_document` — the Slint compiler / `slint::ComponentFactory` round-trip), and twelve gated test fns. `prism-builder/Cargo.toml` retired the `interpreter` feature outright; `slint`, `slint-interpreter`, and `spin_on` workspace deps deleted from the root `Cargo.toml` and the per-crate `[dependencies]` blocks. `lib.rs` re-exports trimmed: `live::*`, `syntax_provider::*`, and the four `render::compile_*`/`instantiate_*` symbols all gone. Workspace `cargo check` clean; `cargo clippy --workspace --all-targets -- -D warnings` clean; **3123 tests across the workspace green, zero failures**. Slint rows in `Cargo.lock` count: 0. | Validates the §17/§24-27 contract from the *consumer* side — every Phase 5 deletion landed without a single replacement edit elsewhere, because the unified `Component::lower_ui` + `prism-ui-runtime::backends::*` pipeline had already absorbed every render path the deleted code served. The `interpreter` feature was the workspace's load-bearing Slint anchor: it pinned `slint`, `slint-interpreter`, and `spin_on`, and was the only path from prism-shell into the live `.slint` compile/instantiate machinery. With prism-shell already off Slint (previous wave), the feature had no real consumer — the deletion was mechanical confirmation of that. The `render_slint` / `SlintEmitter` source-emission path stays for now (it's still exercised by `prism-builder/src/starter.rs` for the legacy DSL output and is independent of the slint *runtime* — pure string emission, no slint crate dep). Removing the trait method is the next surgical step (every `Block` / `Component` impl has it, ~16 builtins + chrome), but it earns its own decision-log entry once it lands. The point this entry proves: the workspace can now compile the entire codebase with zero `slint*` crates in `Cargo.lock`, and every test passes. The Slint era is functionally over even though the source-emitter API surface lingers; the licence flip to `MIT OR Apache-2.0` (Phase 5 decision #1) is unblocked. |

## 28. Active dock tree → recursive workspace renderer

**Strategy locked 2026-05-10 (post-§27 close-out).** With every
feature module ported off the legacy stack and the §16 panel
table entirely landed, the one missing piece for end-to-end
proof is *connecting* the active workflow page's `DockState` to
the parsed `app.prism-ui` skeleton. The §16 closing claim — "every
panel is one row in a table" — needed a runtime walker before it
could drive the actual Studio chrome.

**The walker.** `shell.dock-workspace` is a 48th shell block. It
reads one prop (`dock`, the serialised active `DockNode`) and
recurses:

- `Split { axis, ratio, first, second }` → a `<container>` with
  `direction = row | column`, two children sized by the ratio
  through `Sizing::Percent` (new variant on the runtime's `Sizing`
  enum — Taffy lowers it natively; cross-axis grow / fixed-cross
  arms unchanged).
- `TabGroup { tabs, active }` → `ctx.lower_as("shell.dock-panel",
  ..)` with `panel-id` set to the active tab. When `tabs.len() > 1`
  the leaf also forwards a `tabs` JSON array to the dock-panel's
  existing tab-bar dispatch.

**The routing table.** `shell.dock-panel` gained one branch in its
body-resolution chain: when neither AST children nor `host_children`
are authored AND `panel-id` is non-empty, fall through to
`components::panel_routing::tag_for_panel(panel_id)` and
`ctx.lower_as` the matching content tag. The table
(`PANEL_ROUTES`) is the single source of truth for "what visual
lives in this leaf?" — adding a dockable panel is one row in the
table; the dock-panel block, the workspace walker, and every
parsed `app.prism-ui` skeleton inherit the new mapping with zero
additional edits. A test in `panel_routing.rs` keeps the table in
lockstep with `register_shell_builtins` (every routed tag must be
a registered shell block).

**The binding.** `WorkspaceSlot::dock_workspace_props()` emits
`{ "dock": <serialised active DockNode> }`. The skeleton is now
three lines of meaningful content: `<shell.app-window>` with one
`<shell.dock-workspace/>` child, plus the seven sibling overlay
tags. Switching the active workflow page (or customising the
layout via `DockWorkspace::active_dock_mut`) flows through the
binding on the next frame — no host-side `push_dock_layout`
recomputation, no pixel rectangles synthesised in the shell.

**The resolver fix.** The synthetic-attribute round-trip path
(`fill_compositions` → `value_for`) needed one defence-in-depth
rule: strings whose first non-whitespace character is `[` or `{`
are auto-parsed back into `Value::Array` / `Value::Object`. Pre-fix,
`workflow-page-bar`, `dock-tab-bar`, `command-palette`, and every
other JSON-array-driven block was *only* exercised through unit
tests that constructed `Node`s directly with structured
`serde_json::Value`s; the actual binding-emission → resolver
pipeline serialised arrays as strings, so blocks reading
`.as_array()` got `None` end-to-end. The fix is one line in
`value_for`; the `value_for` test in `ui_resolver.rs` covers the
bool/number-still-coerce path, and the new render-pipeline test
(`dock_workspace_emission_round_trips_through_resolver_to_routed_panel`)
proves the fix end-to-end through the real skeleton.

**Smart-pattern wins.**

- **One walker, one match.** Splits and tab-groups are the only
  two `DockNode` variants; the walker has exactly two arms.
- **One routing table.** Adding a panel is one row in `PANEL_ROUTES`;
  no router-arm match lives in any block body, no per-panel
  dispatch lives in any service.
- **One binding row.** `bind_slot!(reg, "shell.dock-workspace",
  |s| s.workspace.dock_workspace_props())` is the entire
  host-side wiring; the binding mirrors §19's slot-method
  discipline (no inline JSON in closures).
- **Sizing vocabulary grew once.** `Sizing::Percent(f32)` lands in
  `prism-ui-runtime` as the additive variant; `sizing_to_taffy`,
  the semantic-HTML `push_sizing` walker, and
  `prism-builder::ui_lower::sizing_from_dimension` all gain one
  match arm. Existing `Sizing::Grow` / `Sizing::Fixed` consumers
  recompile unchanged.
- **No host-side pixel math.** `push_dock_layout` (legacy: ~60
  LoC of rectangle flattening into Slint absolutely-positioned
  models) does not return — Taffy does the layout, ratios stay
  on the runtime side end-to-end.

**Verification.** 255 prism-shell lib tests green (was 243 at the
Phase-5 cutover close); workspace `cargo test --workspace --lib`
green at 6300+ tests; `cargo clippy --workspace --all-targets --
-D warnings` clean. New tests: 6 in `dock_workspace.rs` (empty,
single-leaf, horizontal split, vertical split, multi-tab forward,
string-serialised round-trip), 2 in `dock_panel.rs` (auto-dispatch
on panel-id, unknown-panel-id empty body), 1 in `panel_routing.rs`
(every route targets a registered tag), 1 in `render.rs`
(end-to-end dock-workspace emission through the real skeleton).
Registry now at 48 shell primitives.

### Decision-log entry

| Date | Decision | Rationale |
|---|---|---|
| 2026-05-10 | §28 lands: `shell.dock-workspace` block + `panel_routing::PANEL_ROUTES` table + `Sizing::Percent` runtime variant + `value_for` JSON auto-parse for `[`/`{`-prefixed attribute strings + `dock-panel` panel-id auto-dispatch. Skeleton (`ui/app.prism-ui`) collapses to `<shell.app-window><shell.dock-workspace/></shell.app-window>` plus overlay siblings. 255 lib tests, 48 shell primitives, registry/bindings parity restored. | The §16 closing claim ("every panel is one row in a table") was previously a *static* property of the panel registry; §28 makes it a *runtime* property of the skeleton. The active dock tree drives content selection without any new dispatch pattern — the existing `lower_as` seam (§15), the existing slot-method binding shape (§19), and the existing `Block` registration table (§12) all compose without growth. The runtime gained one variant (`Sizing::Percent`) and one defensive parse rule (`value_for` JSON auto-parse) — both additive, both load-bearing across consumers beyond the dock walker (every binding that emits an array prop now round-trips correctly through the resolver). The smart-pattern budget for the wave is two new files (`panel_routing.rs`, `dock_workspace.rs`) totalling ~250 LoC; legacy `push_dock_layout`-style rectangle-flattening stays deleted. |

## 29. `BuilderDocument` → `.prism-ui` source emitter

**Strategy locked 2026-05-10 (post-§28).** The Slint exorcism (§17 +
slint-source rip) deleted `render_document_slint_*`, `SlintEmitter`,
and the `render_slint` trait method. With them went the only path
that auto-populated `Page::source` — `ensure_source` shrank to a
no-op while `Page::document` continued to carry the authoritative
tree. §29 closes that gap with a single declarative emitter inverse
to `prism_core::language::prism_ui::parse`, restoring round-trip
fidelity without re-introducing any of the deleted infrastructure.

**The emitter.** `prism_builder::prism_ui_emit::emit_document(doc)`
(plus the per-node `emit_node`) walks a `BuilderDocument`/`Node`
tree depth-first and produces well-formed `.prism-ui` text. One
function, no two-step IR, no registry dependency — the `Node`
already carries its own `component` tag.

**Attribute table.** Single declarative match over `serde_json::Value`:

| `Value` shape | Emitted as |
|---|---|
| `Null` | omitted |
| `Bool(false)` | omitted (boolean-attribute semantics: absent = false) |
| `Bool(true)` | bare attribute name (`disabled`) |
| `Number(n)` | `key="<n>"` |
| `String(s)` | `key="<escaped>"` |
| `Array(_)` / `Object(_)` | `key={<compact JSON>}` (parser sees an `{expr}` interpolation) |

Object/array values use `{...}` interpolation rather than a quoted
string because `parse_quoted_value` treats `{` as the start of an
interpolation regardless of the surrounding quote — `key="{...}"`
would mis-parse. Wrapping the JSON in `{...}` lets the parser take
it as an `AttributeValue::Expression` whose body is the JSON
literal, which is the only round-trip-safe shape supported by the
grammar today.

**Determinism.** Attributes are emitted in alphabetical order by
key, indentation is exactly two spaces per nesting level, leaves
self-close (`<text/>`). Two equivalent trees produce byte-identical
sources — golden / diff tests stay stable across serde key
permutations.

**Wiring.** `Page::ensure_source` was a Slint-era hook left as a
no-op after the runtime cutover. It now emits-when-empty and stays
idempotent:

- `Page::ensure_source(&registry, &tokens)` — fills `self.source`
  from `self.document` only if `source` is empty. Hand-edited
  source survives the call.
- `Page::regenerate_source()` — force-rewrites `self.source` from
  `self.document` regardless. The reset path for "I changed the
  tree, give me back the canonical text."

**Smart-pattern wins.**

- **One walker, one match.** The attribute table is a single
  `match` over `serde_json::Value` — six arms, no per-block
  awareness. Adding a new value shape is one arm.
- **No registry coupling.** The `Node` already carries its own
  component tag; the emitter never reaches for `ComponentRegistry`.
  This stays orthogonal to §12 (block registration) and §13
  (tag resolver), so neither has to grow.
- **Forward-compatible with `value_for` auto-parse.** The §28
  `value_for` rule that auto-parses `[`/`{`-prefixed attribute
  strings still applies to consumers that bypass the parser; the
  emitter prefers the grammar-native `{expr}` shape for
  non-scalars, so both paths converge on structured `Value`s
  without duplication.
- **No source-of-truth conflict.** `Page::document` remains
  authoritative on serialisation; `source` is a derived view. The
  emit-when-empty discipline mirrors the `ensure_*` shape used
  elsewhere in the host (e.g., bindings populating empty slots) —
  a familiar lazy-fill seam, not a new pattern.

**What stays deleted.** No `render_slint`, no `SlintEmitter`, no
`render_document_slint_*`, no `LiveDocument`, no
`BuilderSyntaxProvider`. The emitter is a fresh ~150-LoC walker, not
a port of the Slint pipeline.

**Verification.** 330 prism-builder lib tests green (was 313 before
§29); 15 new tests in `prism_ui_emit::tests` (empty doc, leaf
self-close, parent open/close, boolean true/false handling, null
omission, numeric formatting, alphabetical determinism, escape
discipline for quotes/backslashes/newlines, array/object
interpolation shape, two-space indent fidelity, document-level emit,
end-to-end parse-without-errors, parsed-tree shape preservation), 2
new tests in `app::tests` (`ensure_source` idempotency,
`regenerate_source` overwrite). `cargo clippy --workspace
--all-targets -- -D warnings` clean.

### Decision-log entry

| Date | Decision | Rationale |
|---|---|---|
| 2026-05-10 | §29 lands: `prism_builder::prism_ui_emit` module + `Page::ensure_source` (emit-when-empty) + `Page::regenerate_source` (force-rewrite). One declarative walker (`emit_document` / `emit_node`), one attribute-shape table, alphabetised attrs for byte-stability. 330 builder lib tests, 17 new emitter tests, all workspace tests + clippy clean. | The Slint exorcism left `Page::source` orphaned — the field still serialised to disk but lost its auto-population path. §29 restores the round-trip without reviving any of the deleted Slint infrastructure: the emitter is the inverse of the canonical `.prism-ui` parser, depends only on `serde_json::Value` shape, and stays orthogonal to the registry / resolver / block layers added in §12-§28. The grammar's existing `{expr}` interpolation absorbs object/array attributes; scalar attributes round-trip as plain quoted text; determinism through alphabetical attr ordering keeps golden tests stable. The wiring discipline (`ensure_source` lazy-fills, `regenerate_source` force-overwrites) mirrors the lazy-fill seams already used elsewhere in the host so no new pattern lands. |

## 30. `TemplateNode` → runtime walker (`lower_template`)

**Strategy locked 2026-05-10 (post-§29).** Every authoring surface
above the registry — `WidgetContribution.template` for the 45
core-engine widgets, `#[derive(PrismBlock)]` for derive-authored
blocks, the Luau-authored `template()` in `LuauComponent` — already
produced a `prism_core::widget::TemplateNode` IR. The Slint era
emitted that IR through a Slint source walker; the cutover deleted
the walker and never replaced it, so the IR was *defined but
unrendered*: every block carrying a template fell through to
`LowerCtx::default_container` and lost its declared shape.
`#[derive(PrismBlock)]` reflected the gap explicitly — the macro
emitted `id()` / `schema()` only, with the user-authored `template()`
function entirely unused (`let _ = template_extract;` in the macro
body).

**The walker.** `prism_builder::template_lower::lower_template(ctx,
template, props, outer_children, style, id_prefix) -> UiNode` is the
single declarative seam. One match over the eight `TemplateNode`
variants, no per-block awareness, no registry dependency beyond
`LowerCtx` (which carries the registry already for the embedded
`Component { component_id, .. }` and `DataBinding` arms). Recursion
threads a `Cell<u32>` counter so synthetic ids are stable across
re-renders (`{prefix}.t0` / `{prefix}.t1` / …) — the runtime layout
cache stays warm without UUID churn.

**The integration.** Two consumers, two-line each:

- `CoreWidgetBlock::lower_ui` (`core_widget.rs`) overrides the
  default container fallback to call `lower_template(ctx,
  &self.contribution.template.root, &node.props, &node.children,
  style, &node.id)`. All 45 wrapped widget contributions inherit a
  working render path with one trait-method override.
- `#[derive(PrismBlock)]` (`prism-luau-derive::prism_block`) now
  emits a `lower_ui` impl alongside `id()` / `schema()`. The body
  evaluates `<Self>::template(...)` (typed-prop variant invokes
  `<Props>::from_value(&node.props)` first) and pipes the resulting
  `TemplateNode` through `lower_template`. The macro user authors a
  pure data-returning function; the rendering wiring is invisible.

**Slint binding derive deletion.** `prism-luau-derive::slint_binding`
(`#[derive(SlintBinding)]`) targeted Slint's generated `set_<field>`
/ `get_<field>` getters on a `slint::ComponentHandle`. With Slint
fully exorcised, the derive had no working consumer left. Deleted
outright (168 LoC + the `derive_slint_binding` proc-macro entry
point + the `mod slint_binding` declaration) — no rename, no shim,
no compatibility hack. The "never deprecate, rip it out" workspace
rule applies.

**Smart-pattern wins.**

- **One walker, one match.** `lower_template` is the entire bridge
  between every template-authored surface and the runtime; the
  match arms map 1:1 with `TemplateNode` variants. Adding a new IR
  variant is one arm in the walker, zero edits at every call site.
- **No registry coupling beyond `LowerCtx`.** `Component` /
  `DataBinding` arms call `ctx.lower_as`, which already honours the
  live registry / cascade. Hosts overriding a registered tag
  (themed `text`, alternate `image`) transparently override the
  template renderings that bind to it.
- **Deterministic ids.** A single `Cell<u32>` counter scoped to the
  walker call produces stable `{node.id}.t{N}` ids — golden / diff
  tests over the lowered tree stay byte-stable.
- **Declarative attribute mapping.** The walker reads
  `props[field]` for `DataBinding` / `Repeater` / `Conditional` /
  `Image` / `Link` lookups using a single `is_truthy` predicate and
  a single string-extract helper; no per-block parsing, no inline
  JSON munging in the call sites.
- **Two-call integration.** `CoreWidgetBlock::lower_ui` is one
  trait-method override; the `#[derive(PrismBlock)]` emission is
  one block-quote in the proc-macro. No new trait, no new context
  type, no per-block opt-in.

**Verification.** 347 prism-builder lib tests green with
`--all-features` (was 330 before §29 + 17 emitter = 347 baseline,
plus 7 new `template_lower::tests` exercising every variant + 1 new
integration test in `tests/derive_macros.rs` proving the derive's
generated `lower_ui` walks through `lower_template`). 12 derive
integration tests green (was 11). All non-luau crates pass
`cargo clippy --all-targets -- -D warnings` clean. Pre-existing
luau-feature warnings in `luau_component.rs` (Send/Sync `Arc`,
trait-recursion lint) are out of scope.

### Decision-log entry

| Date | Decision | Rationale |
|---|---|---|
| 2026-05-10 | §30 lands: `prism_builder::template_lower::lower_template` walker + `CoreWidgetBlock::lower_ui` override + `#[derive(PrismBlock)]` emits a `lower_ui` impl that walks the template through the new function. `prism-luau-derive::slint_binding` (the dead Slint global-state binding derive) deleted outright. 347 builder lib tests, 12 derive tests, clippy clean (non-luau). | `TemplateNode` was a defined IR with no consumer post-Slint — every derive-authored or core-engine-contributed widget rendered as the default container. §30 closes that gap with one declarative walker and two two-line integrations: `CoreWidgetBlock` overrides `lower_ui`; `#[derive(PrismBlock)]` extends its emitted block-quote with `lower_ui`. Synthetic-id stability via a single `Cell<u32>` counter keeps the runtime layout cache warm; declarative `Value` lookups for the data-binding family stay orthogonal to per-block schemas. The Slint binding derive was the last user-facing carry-over from the Slint era in `prism-luau-derive`; deleting it leaves the derive crate Slint-free end-to-end. |

## 31. `ui_runtime` collapse — registry-aware is the only path

**Strategy locked 2026-05-10 (post-§30).** The `ui_runtime` translator
shipped both registry-less and registry-aware function pairs through
the entire migration: `document_to_ui_tree` / `*_with_registry`,
`render_commands` / `*_with_registry`, `lower_html` /
`*_with_registry`, `lower_semantic_html` / `*_with_registry`. The
parallel-build doc on the module said the cutover would retire the
duplicates; the cutover landed but the duplicates didn't. Every
public `_with_registry` consumer (the relay's SSR entry, the shell's
help text) actually wanted the registry-aware path; the registry-less
twins were used only by their own tests.

**The collapse.** Eight functions become four. Each takes
`Option<&ComponentRegistry>` — `None` falls through to the generic
container lowering for every node (the old registry-less behaviour);
`Some(&reg)` dispatches each node through its block's
`Component::lower_ui`. No new abstraction, no new option type — the
existing `LowerCtx::new(registry, parent_style)` already accepted
`Option<&ComponentRegistry>` end-to-end, so the four entry points
just thread it through.

**External callers.** One real consumer (`prism-relay::ssr_routes`,
`lower_semantic_html_with_registry(&doc, &reg)` →
`lower_semantic_html(&doc, Some(&reg))`); two doc-comment references
in `prism-shell` / `prism-builder` `lib.rs`. No semver dance — the
workspace rule is "rename, move, break, fix; never deprecate".

**Adjacent dead-code removal (same wave).**

- `prism_builder::component::RenderContext` — the legacy
  "ad-hoc host-side caller carries `&DesignTokens`" struct. Zero
  consumers anywhere in the workspace; deleted along with its
  re-export in `lib.rs`.
- `prism_builder::core_widget::merge_props` — `#[allow(dead_code)]`
  helper from the Slint-era render path that merged template +
  instance props before pushing into the Slint emitter. The new
  `lower_template` walker reads `props` straight from the host
  node, so the merge step has no place left to live. Deleted along
  with its `merge_props_template_plus_instance` test, replaced by
  a `lower_ui_walks_template_through_template_lower` integration
  test that exercises `CoreWidgetBlock::lower_ui` end-to-end (the
  §30 wiring).

**Smart-pattern wins.**

- **One signature shape.** Every translator entry point now
  matches the same `(doc, Option<&registry>, …)` shape. Adding a
  new translator (e.g., a future `lower_pdf`) is one function, not
  a registry-less / registry-aware pair.
- **No abstraction growth.** `Option<&ComponentRegistry>` is the
  type `LowerCtx::new` already takes — the entry points just
  forward it. Zero new types, zero new traits, zero new docs to
  describe a "host registry" wrapper.
- **Doc references no longer drift.** With one canonical name per
  translator, every CLAUDE.md / module header that points at the
  SSR entry is the same string; future renames are single-edit.

**Verification.** 347 prism-builder lib tests green (1 test
removed — `merge_props_template_plus_instance`; 1 test added —
`lower_ui_walks_template_through_template_lower`). 12 derive tests
green. 255 prism-shell lib tests green. 26 prism-relay lib tests +
8 integration tests green. `cargo clippy --all-targets --
-D warnings` clean across the touched crates.

### Decision-log entry

| Date | Decision | Rationale |
|---|---|---|
| 2026-05-10 | §31 lands: `ui_runtime` collapses to four entry points (`document_to_ui_tree` / `render_commands` / `lower_html` / `lower_semantic_html`), each taking `Option<&ComponentRegistry>`. Eight functions retired; the `_with_registry` suffix is gone. Adjacent dead code (`RenderContext`, `merge_props`) deleted in the same wave. Relay SSR caller updated; doc references collapsed. | The migration plan called for a single render path post-cutover; `ui_runtime` carried the parallel-build duplication forward indefinitely. The collapse is mechanical (four function pairs → four functions, one `Option`) and the fan-out is small (one external caller). The dead-code removal closes the last `#[allow(dead_code)]` and orphaned-public-type from the Slint era — `RenderContext` had zero consumers (it was a tokens-carrier for ad-hoc host code that never materialised); `merge_props` belonged to the deleted Slint emit path. Together with §30, the builder's render surface is now: one trait method (`Component::lower_ui`), one walker per IR (`lower_template` for `TemplateNode`), one per-target entry point (`lower_*` for runtime / HTML / semantic-HTML). |

## 32. Starter catalog — declarative `BuiltinSpec` table

**Strategy locked 2026-05-10 (post-§31).** `prism-builder/src/starter.rs`
held 14 hand-written `pub struct XxxBlock { id: ComponentId }` types,
each with a `Block` impl that always followed the same shape:
`id()` → `&self.id`, `schema()` → `schemas::xxx()`, `help_entry()`
→ `Some(HelpEntry::new(key, title, desc))`, `signals()` →
`with_common_signals(vec![...])`, optional `variants()` →
`variant_presets::xxx()`, and a custom `lower_ui()`. ~700 LoC of
boilerplate — every per-method override was the only line in the
function — wrapping ~400 LoC of *actual* per-block lowering logic.

**The collapse.** One `BuiltinBlock` type, one `&'static BuiltinSpec`,
one `BUILTINS: &[&BuiltinSpec]` const table. Each spec carries
`(id, schema_fn, help, signals_fn, variants_fn, lower_fn)` with
const-fn builder methods (`.help()` / `.signals()` /
`.variants()`) for the optional fields; `signals` defaults to the
common-signals helper, `variants` to `vec![]`. The `lower` fn is a
free function — the same body that used to live inside `impl Block
for XxxBlock`. `register_builtins` is a one-line loop over
`BUILTINS` plus the two non-builtin registrations (`card` prefab,
`facet` component).

**Smart-pattern wins.**

- **Adding a builtin = one const + one row.** The hand-written
  trait impl, the per-block struct, the `pub struct` declaration,
  the `register_builtins` macro arm, the `id` field plumbing — all
  collapse into one `const SPEC = BuiltinSpec::new(...).help(...).signals(...)`
  literal and one `&SPEC` pushed onto the table.
- **No type proliferation.** 14 `pub struct XxxBlock` types
  vanish. Tests that constructed them by literal struct (`TextBlock
  { id: "text".into() }`) now ask the registry for the dispatched
  block, which is the only call shape that matters at runtime.
- **`prism-luau-derive` stays orthogonal.** `#[derive(PrismBlock)]`
  remains the path for *user-authored* blocks (template-based,
  walks `lower_template`). `BuiltinSpec` is the path for *built-in*
  blocks where bespoke `lower_ui` outperforms the IR walker. Both
  paths produce `Block` impls and feed the same registry; neither
  knows about the other.
- **`level_font_weight` deletion.** The last
  `#[allow(dead_code)]` helper in starter.rs (Slint-era artefact)
  vanishes alongside the trait-impl boilerplate.

**Adjacent doc cleanup (same wave).** Stale Slint references in
the root `CLAUDE.md` ("mid-way through the Slint migration"), the
`prism-builder` / `prism-shell` / `prism-studio` Cargo.toml
descriptions, and the workspace `Cargo.toml` dep comments are
rewritten to reflect the post-cutover `prism-ui-runtime` stack.
The `slint-migration-plan.md` reference in `CLAUDE.md` is repointed
at this plan.

**Verification.** 338 prism-builder lib tests green (6 starter
tests rewritten to query the registry instead of constructing
per-block structs; 1 new `builtin_block_factory_returns_known_ids`
test added). Full workspace test suite green. `cargo clippy
--workspace --all-targets -- -D warnings` clean.

### Decision-log entry

| Date | Decision | Rationale |
|---|---|---|
| 2026-05-10 | §32 lands: starter catalog collapses to `BUILTINS: &[&BuiltinSpec]`. 14 `pub struct XxxBlock` + `impl Block` pairs deleted (~700 LoC of boilerplate gone); `BuiltinBlock` + `BuiltinSpec` + const builder API land in their place. Stale Slint references in root `CLAUDE.md` + Cargo.toml descriptions updated to match the post-cutover stack. | The 14 starter blocks were the largest remaining instance of "one struct + one trait impl per registered thing" boilerplate in the workspace. The new pattern matches the user's "DI/Builders/registration/Declarative" rubric exactly: each block is *data* (a `BuiltinSpec`), the type that interprets the data (`BuiltinBlock`) exists once, and registration is a const table. `prism-luau-derive`'s `#[derive(PrismBlock)]` stays in its lane (user-authored template-based blocks); the two paths compose without knowing about each other. The Slint doc-string carryover was the last surface-level lie about what the codebase actually is. |

## 33. Shell components → declarative `BlockSpec` table

**Strategy locked 2026-05-10 (post-§32).** §32 collapsed the 14
starter blocks via `BuiltinSpec`. The same pattern — one `struct
Foo { id: ComponentId }` + one `impl Block for Foo` per registered
thing — was duplicated **48 times** in
`packages/prism-shell/src/components/`, the largest remaining
instance of registration boilerplate in the workspace. Every shell
primitive followed the identical shape: `id()` returned `&self.id`,
`schema()` returned a hard-coded `Vec<FieldSpec>`, optionally
`signals()` returned a custom signal list, and `lower_ui()` did the
actual rendering work. The struct's only field, `id: ComponentId`,
existed solely so the trait method could borrow it back out.

**The collapse — §32 generalised.** `BuiltinSpec` lifts to
`prism-builder/src/block.rs` as `BlockSpec`, with the wrapping
`BuiltinBlock` renamed to `SpecBlock`. The const-fn builder API
gains `.lower(...)` (so `lower` is no longer required at construction
— it defaults to the same generic-container fallback as
`Block::lower_ui`'s default), plus a `BlockSpec::leaf(id)` shortcut
for blocks with no schema fields. Helper free functions
`default_lower`, `default_signals`, `no_variants`, `no_schema`
ship from the same module so any spec can compose against them.
A new `register_specs(&mut ComponentRegistry, &[&BlockSpec])`
helper is the one-line fan-out for any spec table.

The shell registry shrinks from a 50-line `reg!(…)` macro table to
a `pub static SHELL_BUILTINS: &[&BlockSpec] = &[…]` const table,
each row pointing at a `pub const FOO_SPEC: BlockSpec` declared in
the matching `components/foo.rs` file. `register_shell_builtins`
becomes a one-liner: `register_specs(&mut reg.inner, SHELL_BUILTINS)`.

**Smart-pattern wins.**

- **Adding a shell primitive = one const + one row.** The hand-written
  `pub struct Foo { pub id: ComponentId }`, the four-method `impl
  Block for Foo` boilerplate, the `pub use foo::Foo` re-export in
  `mod.rs`, and the `reg!("shell.foo", Foo)` macro arm all collapse
  to a `pub const FOO_SPEC: BlockSpec = BlockSpec::new("shell.foo",
  foo_schema).lower(foo_lower).signals(foo_signals)` literal and one
  `&super::foo::FOO_SPEC` row in the table.
- **One declarative primitive across two crates.** `BlockSpec` /
  `SpecBlock` / `register_specs` live once in `prism-builder` and are
  consumed by both `prism-builder::starter` (17 builtins) and
  `prism-shell::components::registry` (48 primitives). The
  `ShellComponentRegistry` newtype (which keeps shell primitives out
  of the user-facing component palette) is unchanged — it still wraps
  `ComponentRegistry`; only the registration call shape moved.
- **`prism-luau-derive` stays orthogonal.** `#[derive(PrismBlock)]`
  remains the path for *user-authored* blocks (template-based,
  walks `lower_template`). `BlockSpec` is the path for *built-in*
  blocks (Rust-authored, bespoke `lower` fn). Both paths produce
  `Block` impls and feed the same `ComponentRegistry`; neither knows
  about the other. The derive macro's emitted `impl ::prism_builder::Block
  for #ident` is unchanged — it still routes through the trait, not
  the spec — because user-authored blocks legitimately benefit from
  the named-struct ergonomics (they often want associated functions,
  Default impls, etc.).
- **Tests stay readable.** The `let block = Foo { id: "shell.foo".into() }`
  ctor in each component's `#[cfg(test)] mod tests` becomes
  `let block = SpecBlock::new(&FOO_SPEC)`, and every `block.schema()`
  / `block.signals()` / `block.lower_ui()` call keeps working
  unchanged via the `Block` trait impl on `SpecBlock`. No test had
  to change its assertions.
- **Re-export hygiene.** `prism-shell/src/components/mod.rs` drops
  the 48 `pub use foo::Foo;` re-exports — nothing outside the
  registry consumed them, the per-component types existed only to
  satisfy the macro registration shape. `mod.rs` is now 49 `pub
  mod foo;` lines + one `pub use registry::{…, SHELL_BUILTINS};`.

**Diff scorecard.**

- 53 files changed, ~3.4k insertions / ~3.8k deletions = ~412 LoC
  net deletion across `prism-shell` + `prism-builder`.
- 48 `pub struct Foo { pub id: ComponentId }` declarations deleted.
- 48 `impl Block for Foo { fn id() … fn schema() … fn lower_ui() …
  }` blocks deleted.
- 48 `pub use foo::Foo;` re-exports deleted from `mod.rs`.
- 50-line `reg!()` macro table replaced by a const `&[&BlockSpec]`
  array (same length, no macro).

**Verification.** 255 prism-shell lib tests green, 338 prism-builder
lib tests green, **3057 workspace tests total green**. `cargo clippy
--workspace --all-targets -- -D warnings` clean. The two doc-comment
trail-edges (one in `prism-shell/CLAUDE.md`, one in
`prism-builder/CLAUDE.md`) are updated to point at §33.

### Decision-log entry

| Date | Decision | Rationale |
|---|---|---|
| 2026-05-10 | §33 lands: 48 shell components collapse to a single `SHELL_BUILTINS: &[&BlockSpec]` table; `BuiltinSpec`/`BuiltinBlock` from §32 generalise to `BlockSpec`/`SpecBlock` in `prism-builder/src/block.rs` and serve both the 17 starter builtins and the 48 shell primitives. ~412 LoC net deletion across 53 files. | §32 collapsed 14 starter blocks; the same pattern was duplicated 48× in shell components — the largest remaining instance of "one struct + one trait impl per registered thing" boilerplate. Lifting the primitive to `block.rs` lets both crates share one declarative spec without coupling shell to starter or vice-versa. `BlockSpec::new(id, schema).lower(fn).signals(fn).help(…)` is the smart-pattern user requested: data-driven, builder-style, registration via const table. `prism-luau-derive`'s `#[derive(PrismBlock)]` stays in its lane (template-IR-walking for user-authored blocks); the two paths compose without knowing about each other. |

## 34. Dock panel catalog → declarative `PanelKind` table

**Strategy locked 2026-05-10 (post-§33).** `prism-dock`'s
`PanelKind` was an enum with a 14-arm `meta()` match returning
`PanelMeta`, alongside an `ALL: &[PanelKind]` const list and a
serde-roundtripping `id()` method. `prism-shell::components::panel_routing`
was a parallel 10-row `&[(&str, &str)]` table mapping panel-id →
shell content tag. Three sources of truth, all describing "what
dockable panels exist and how do we render them?"

**The collapse — same pattern as §32/§33.** `PanelKind` becomes a
`Copy` data struct holding `id`, `label`, `icon_hint`, `min_width`,
`min_height`, `allow_multiple`, and `tag: Option<&'static str>`
(the shell content tag — folded in from `panel_routing`). Each
panel is declared as a `pub const PanelKind`
(`PanelKind::BUILDER`, `PanelKind::INSPECTOR`, …); the
`pub const ALL: &[&PanelKind]` table is the single registration
surface. `from_id`, `tag_for`, and `panel_id` are flat methods over
the table. `PanelMeta` and the 115-line `meta()` match are deleted;
`prism-shell::components::panel_routing` is deleted; `dock_panel.rs`
calls `prism_dock::PanelKind::tag_for(panel_id)` directly.

**Smart-pattern wins.**

- **Adding a dockable panel = one row.** A new panel is one
  `pub const FOO: PanelKind = PanelKind { id, label, icon_hint,
  min_width, min_height, allow_multiple, tag }` plus one
  `&Self::FOO` row in `ALL`. The dock layout, the chrome, and the
  shell content-tag dispatch all pick it up automatically.
- **Three sources of truth → one.** Enum variants, `meta()` match,
  and `panel_routing::PANEL_ROUTES` collapse to one declarative
  table. Stale-tag drift between dock and shell is now structurally
  impossible.
- **No serde-roundtrip dance for `id()`.** The id is a literal
  `&'static str` field; `PanelKind::BUILDER.id` reads as data, no
  `serde_json::to_value` indirection. The serialised representation
  of `PanelKind` is the full struct, but in practice the only
  serialised type is `WorkflowPage`, which already stored panel ids
  as strings — so no on-disk format changed.
- **Layer hygiene preserved.** `prism-dock` carries a
  `tag: Option<&'static str>` field; the field is opaque from the
  dock's perspective (it never dispatches on the value). The shell
  is the only crate that interprets it. No new dependency direction
  was introduced; `prism-shell → prism-dock` was already in place.

**`prism-luau-derive` is unaffected.** The dock layer is below the
component-registry seam; the derive macro works at the block layer
(builder-side) and never named `PanelKind`. No changes in
`prism-luau-derive`, no changes in any user code that consumes
`#[derive(PrismBlock)]`.

**Diff scorecard.**

- `prism-dock/src/panel.rs`: 229 → 273 lines but ~115 lines of
  match-arm boilerplate replaced by ~150 lines of flat
  `pub const`s — net mostly a wash in line count, large win in
  structure (no `match` statements anywhere; adding a panel is one
  row, no compiler-driven exhaustiveness chase across `meta()`).
- `prism-dock/src/page.rs`: every `PanelKind::Builder.id()` site
  rewritten as `PanelKind::BUILDER.panel_id()` (mechanical, ~30
  call sites).
- `prism-shell/src/components/panel_routing.rs`: **deleted**
  (74 lines).
- `prism-shell/src/components/dock_panel.rs`: one call site
  rewritten (`panel_routing::tag_for_panel(id)` →
  `prism_dock::PanelKind::tag_for(id)`).
- `prism-shell/src/components/{mod.rs,dock_workspace.rs}`,
  `prism-shell/src/props.rs`: doc-string + module-decl sweeps to
  point at the new home.
- `PanelMeta` re-export removed from `prism-dock/src/lib.rs`.

**Verification.** `cargo test --workspace` — **3057 tests green**
(same headcount as the §33 baseline: -3 panel_routing tests,
-1 net dock test, +3 new dock tests, +1 new prefab clone-test —
balances). `cargo clippy --workspace --all-targets -- -D warnings`
clean.

### Decision-log entry

| Date | Decision | Rationale |
|---|---|---|
| 2026-05-10 | §34 lands: `PanelKind` collapses from a 14-variant enum + 115-line `meta()` match + sibling `panel_routing::PANEL_ROUTES` table into one `pub const PanelKind` per panel + a single `PanelKind::ALL` table. `PanelMeta` and `prism-shell::components::panel_routing` are deleted; the shell content-tag mapping is now a `tag: Option<&'static str>` field on each panel. | Three sources of truth (enum + match + routing table) for "what panels exist and how do we render them?" was the largest remaining structural duplication after §33. The §32/§33 pattern (data + builder + table) maps onto it exactly: each row of the dock catalog is now data, the only thing that interprets it is the consumer (chrome for sizing, shell for tag dispatch), and adding a panel is one row. The folded-in `tag` field finally retires the parallel routing table without violating the dock's renderer-agnostic stance — the field is opaque to the dock itself. |

## 35. Prefab — `Component` → `Block` + real `lower_ui`

**Strategy locked 2026-05-10 (post-§34).** `PrefabComponent`
implemented `Component` directly (one of the last hand-written
`impl Component` blocks in the codebase) and *had no `lower_ui`
override* — every prefab instance fell through to
`Component::lower_ui`'s default container, dropping the prefab's
authored template entirely on the floor. A prefab `<card title="…"
body="…"/>` rendered as an empty container, not the title + body
text it was supposed to.

**The fix + collapse.** `PrefabComponent` now implements `Block`
(consistent with the rest of the codebase post-§32/§33; the
blanket impl in `crate::block` derives the matching `Component`).
Its `lower_ui`:

1. Deep-clones `def.root`, prefixing every internal id with the
   host node's id (so two `<card/>`s on the same page produce
   non-colliding subtrees).
2. Walks each `ExposedSlot`, reading `host.props[slot.key]` and
   writing it into the namespaced inner node's `target_prop`.
3. Hands the materialised tree to `ctx.lower(&materialised)`,
   which recurses through the same `ComponentRegistry` every
   other block uses.

`apply_prop_to_node` is the existing helper (now `pub(crate)`
since the prefab walker reuses it); a small `clone_with_id_prefix`
is added beside it.

**Smart-pattern wins.**

- **One render path, full stop.** Prefabs no longer have a
  parallel "materialise into the document" half-implementation —
  the same `Block::lower_ui` seam every other block consumes
  handles them. `materialize_prefab` (the document-tree flatten
  used by the inspector "convert to nodes" action) keeps working
  unchanged.
- **`Block` everywhere.** `PrefabComponent` was the last
  hand-written `impl Component` outside the `Block` blanket impl
  + `SpecBlock` declarative form. Every renderable type in the
  workspace now goes through `Block` (or `BlockSpec` → `SpecBlock`
  → `Block`).
- **id-prefixing fixes a latent collision bug.** Two prefab
  instances on the same page used to share inner ids; with the
  new prefix, every instance is isolated.

### Decision-log entry

| Date | Decision | Rationale |
|---|---|---|
| 2026-05-10 | §35 lands: `PrefabComponent` migrates from `impl Component` to `impl Block` and gains a real `lower_ui` that materialises `def.root` against the host node's props (with id-prefixing for collision isolation) and lowers through the unified `ctx.lower` walker. | `PrefabComponent`'s missing `lower_ui` was a pre-§30 incomplete: the prefab body never rendered. Switching to `Block` aligns the type with every other registered block in the codebase post-§32/§33 (the `impl<T: Block> Component` blanket gives `Component` for free). The id-prefix is necessary because two `<card/>` instances on the same page would otherwise share `card-title` / `card-body` ids — a render-time collision that's invisible until two prefab instances co-exist. |

## 36. Luau component — share `WidgetContribution` mappings, generic spec parser

**Strategy locked 2026-05-10 (post-§35).** Two surfaces of
duplication inside `LuauComponent`:

1. **Mapping helpers were duplicated**. Both `CoreWidgetBlock`
   (in `core_widget.rs`) and `LuauComponent` (in
   `luau_component.rs`) wrap a `WidgetContribution` into a
   `Block`. Both need to translate `SignalSpec → SignalDef` and
   `VariantSpec → VariantAxis`. `core_widget.rs` had `map_signal_spec`
   / `map_variant_spec` as private free fns; `luau_component.rs`
   inlined the same conversions in `Block::signals()` and
   `Block::variants()`. Drift risk if either side adds a field.
2. **Three near-identical Lua-array parsers**. `parse_field_array`,
   `parse_signal_array`, `parse_variant_array` were three copies
   of "read `table[key]` as a Lua array, JSON-roundtrip each
   entry, deserialize as `T`, drop entries that fail." Same
   pattern, three concrete types.

**The collapse.**

- `core_widget::map_signal_spec` and `core_widget::map_variant_spec`
  are now `pub(crate)`; `LuauComponent::signals()` /
  `::variants()` reuse them. One conversion, two callers, zero
  drift.
- `parse_field_array` / `parse_signal_array` / `parse_variant_array`
  collapse to one generic
  `fn parse_spec_array<T: serde::de::DeserializeOwned>(table: &Table,
  key: &str) -> mlua::Result<Vec<T>>`. The three call sites in
  `parse_contribution` become `parse_spec_array::<FieldSpec>(…)`,
  `parse_spec_array::<SignalSpec>(…)`, `parse_spec_array::<VariantSpec>(…)`.

**Smart-pattern wins.**

- **One mapping, two callers.** `WidgetContribution`'s shape is the
  contract; both Rust-side (`CoreWidgetBlock`) and Luau-side
  (`LuauComponent`) callers go through the same mapping fns.
  Adding a field to `WidgetContribution` is one edit in
  `core_widget` — the Luau path inherits it automatically.
- **One spec parser, three types.** The Lua-array → typed-spec
  conversion is generic over the deserialiser. New
  `parse_contribution` fields (e.g. `toolbar_actions`) become a
  one-line `parse_spec_array::<T>(table, "key")` call.
- **`prism-luau-derive` integration is unchanged.**
  `#[derive(PrismBlock)]` (template-IR path) and
  `#[luau_expose]` (UserData / type-stub emit) compose with the
  unified `Block` blanket exactly as before. The derive's emitted
  `impl ::prism_builder::Block` automatically benefits from any
  shared infrastructure added in `block.rs` (e.g. the §32 default
  `lower_ui`); the luau_component path benefits from the shared
  `WidgetContribution` mappings; both paths share `Block`'s
  blanket `Component` impl.

**Diff scorecard.**

- `prism-builder/src/luau_component.rs`: 804 → 745 lines.
  - Three 19-line `parse_*_array` fns → one 19-line generic
    `parse_spec_array`.
  - Two ~10-line inline mapping closures → two 1-line
    `core_widget::map_*_spec` calls.
- `prism-builder/src/core_widget.rs`: `map_signal_spec` /
  `map_variant_spec` visibility upgraded `pub(crate)`, with a
  comment explaining the shared-conversion contract.

### Decision-log entry

| Date | Decision | Rationale |
|---|---|---|
| 2026-05-10 | §36 lands: `WidgetContribution → Block` mapping helpers (`map_signal_spec`, `map_variant_spec`) are shared between `CoreWidgetBlock` and `LuauComponent`; the three `parse_*_array` Lua-array parsers collapse to one generic `parse_spec_array<T: DeserializeOwned>`. | Both call sites convert the same `WidgetContribution` shape into the same `Block` surface — there's no good reason for the conversions to live in two places. Generic spec parsing also means new fields in `parse_contribution` (e.g. toolbar actions, hot-reload metadata) are a one-line addition rather than a fourth copy of the same loop. `prism-luau-derive` integration was already clean (the derive emits a `Block` impl and inherits everything `Block` consumers do); these changes only tighten the host-side machinery the derive's emitted code lands on top of. |

## 37. Active-underline tab — `chrome::active_underline_tab` + `TabStyle`

**Strategy locked 2026-05-10 (post-§36).** `shell.dock-tab` and
`shell.workflow-page-button` are the same visual recipe — a column
with a label on top and a 2px underline at the bottom, where the
active state paints a tinted background and the resting state swaps
to a hover-bg. Two files, ~50 lines each, the only difference being
metric/colour constants (height, padding, label/bg/underline hex).

**The collapse.** One helper in `prism-shell/src/components/chrome.rs`:

```rust
pub struct TabStyle {
    pub height: f32,
    pub padding: Padding,
    pub label_size: f32,
    pub label_active: &'static str,
    pub label_resting: &'static str,
    pub active_bg: &'static str,
    pub hover_bg: &'static str,
    pub underline_height: f32,
    pub underline_active: &'static str,
}

pub fn active_underline_tab(
    ctx: &LowerCtx<'_>,
    node: &BuilderNode,
    style: &StyleProperties,
    label_text: String,
    active: bool,
    spec: &TabStyle,
) -> UiNode { ... }
```

`dock_tab.rs` and `workflow_page_button.rs` each declare a
`const TAB_STYLE: TabStyle = TabStyle { ... }` and forward to the
helper. The lowering body shrinks from ~40 lines of manual
`bare_container` / `colored_text_node` / `synthetic_container`
plumbing to a single 7-line call.

**Smart-pattern wins.**

- **Static spec, dynamic state.** Variation across consumers is pure
  styling data (`&'static TabStyle`); variation per call is `(label,
  active)`. No runtime branching on tab kind, no per-consumer helper.
- **Adding a third tab is one literal.** Future tab-shaped chrome
  (search-result tabs, breadcrumb segments) declares a new
  `TabStyle` const and reuses the same helper.
- **Promote-then-reuse, locked at the second consumer.** This is the
  rule-of-two threshold the chrome module documents — when the
  pattern lands twice, the recipe graduates to `chrome.rs`.

**Note on nav-button.** `shell.nav-button` was *not* folded into
this helper. Its shape is row-based (rail + body) with an icon
glyph instead of a label, so unifying would force conditionals that
defeat the point of the helper. Active-state semantics live on
`Semantic::with_attr_if` directly. The "implement only when the
recipe is genuinely shared" discipline holds.

### Decision-log entry

| Date | Decision | Rationale |
|---|---|---|
| 2026-05-10 | §37 lands: `chrome::active_underline_tab` + `TabStyle` extracted; `dock_tab` and `workflow_page_button` lowering bodies forward to it. | Two files were running the same column-with-underline recipe for ~40 LoC each, differing only in metric/colour constants. The helper takes a `&'static TabStyle` literal so per-consumer variation stays declarative; the lowering body is one call. Adding a third tab-shaped chrome primitive is a `TabStyle` literal and a one-line `lower_ui` body. `nav-button` deliberately stays separate — its row+rail+icon shape is structurally different and forcing it through this helper would re-introduce branches the helper exists to avoid. |

## 38. Gizmo arms / handles / root — three composable helpers

**Strategy locked 2026-05-10 (post-§37).** `shell.gizmo-move`,
`shell.gizmo-rotate`, and `shell.gizmo-scale` each emit a small set
of coloured rectangles plus an outer `<div role="group">` wrapper,
all built through hand-rolled `bare_container` calls with the same
semantic-attr boilerplate. Three files, ~5 helpers' worth of
duplication.

**The collapse.** Three free functions in `chrome.rs`:

- `gizmo_axis_arm(id, axis, length, thick, color, rounded)` — one
  red/green axis stroke. `axis` ∈ {`'x'`, `'y'`} drives both the
  width/height swap and the `data-axis` attr. `rounded` toggles
  the half-thickness pill radius (move-gizmo arms are pills,
  scale-gizmo arms are square).
- `gizmo_handle(id, size, radius, color, aria, data_role, axis)` —
  one square/circle handle. Used for the white center hub
  (`data-role="gizmo-hub"`), the rotate handle dot
  (`data-role="gizmo-handle"`), and the scale-arm caps
  (`data-role="gizmo-cap"`, with `axis = Some('x'|'y')`).
- `gizmo_root(id, children, aria, tool, row)` — outer
  `<div role="group">` carrying `data-role="gizmo"` and
  `data-tool="{move|rotate|scale}"`.

The three gizmo modules become almost pure structure: which children,
which colours, which axes. The 40-line ARM/CAP/HUB literal pyramids
collapse to ~6-line calls each.

**Smart-pattern wins.**

- **Three orthogonal helpers, no shared state.** Each helper produces
  one `UiNode`; consumers compose them with `vec![...]`. No builder,
  no DI — just functions over already-shared `ui_lower` constructors.
- **The rotate gizmo's stroked ring stays inline.** It's a
  one-of-a-kind shape (filled circle with `data-stroke`); forcing it
  through `gizmo_handle` would introduce a `fill_color: Option<&str>`
  parameter that exists for one caller. The two-bucket discipline
  holds — only collapse what's genuinely shared.
- **Scale-cap symmetry is now structural.** `cap_x` and `cap_y` differ
  only in `axis` and `color`; under the helper they're two `gizmo_handle`
  calls with `Some('x')` / `Some('y')` and the matching axis colour.
  Adding a Z-axis later is one more line.

### Decision-log entry

| Date | Decision | Rationale |
|---|---|---|
| 2026-05-10 | §38 lands: `gizmo_axis_arm`, `gizmo_handle`, `gizmo_root` extracted into `components::chrome`; all three gizmo blocks (move / rotate / scale) lower through them. | Each gizmo was emitting near-identical `bare_container { width / height / background / radius / Semantic::tag("span").with_attr(...) }` boilerplate three to five times per file. The three-helper split (axis / handle / root) is exactly orthogonal to the three concerns the gizmos express (axis arms, interactive handles, group wrapper). The rotate-ring shape stays inline — it's stroked, not filled, so unifying would force a one-caller parameter. New gizmos (skew, shear, Z-axis) are now a `vec![]` of arm/handle calls plus one `gizmo_root` wrapper. |

## 39. Test-fixture helper — `components::testing`

**Strategy locked 2026-05-10 (post-§38).** Every shell-component
test module repeats the same boilerplate to build a single-block
`BuilderNode` and run it through a default `LowerCtx`:

```rust
fn lower_one(node: &BuilderNode) -> UiNode {
    let cascade = StyleProperties::default();
    let ctx = LowerCtx::new(None, &cascade);
    foo_lower(&ctx, node, &cascade)
}

fn foo_node(props: Value) -> BuilderNode {
    BuilderNode {
        id: "x".into(),
        component: "shell.foo".into(),
        props,
        children: vec![],
        layout_mode: LayoutMode::default(),
        transform: Transform2D::default(),
        modifiers: vec![],
        style: StyleProperties::default(),
    }
}
```

Repeated ~50 times across `components/*.rs`, with imports of
`LayoutMode`, `Transform2D`, `StyleProperties as Cascade`, `LowerCtx`
in every test module.

**The collapse.** One `#[cfg(test)] pub(crate) mod testing` in
`components/mod.rs`:

```rust
pub fn test_node(id: &str, component: &str, props: Value) -> BuilderNode { ... }
pub fn lower_with<F>(node: &BuilderNode, f: F) -> UiNode
where F: FnOnce(&LowerCtx<'_>, &BuilderNode, &StyleProperties) -> UiNode { ... }
```

Test-side imports collapse to `use crate::components::testing::{lower_with, test_node};`
plus whatever the assertion needs. The fixture builder shrinks from
10 lines to one; the `lower_one` adapter shrinks from 4 to 1.

**Migrated as part of §39.** `dock_tab`, `workflow_page_button`,
`gizmo_move`, `gizmo_rotate`, `gizmo_scale`, `icon_button`, `toast`,
`section_header`, `app_card`, `status_bar`. The remaining ~40
component tests can migrate incrementally; the helper is in place
and every new component should use it from the start.

**Smart-pattern wins.**

- **Test imports stop drifting.** Adding a field to `BuilderNode`
  (e.g. the `style` cascade in §28, the `transform` field for §22)
  used to require touching every test fixture. Now: one edit in
  `test_node`, every test inherits.
- **The `lower_with` HOF generalises across blocks.** Because the
  shell has standardised on `fn foo_lower(ctx, node, style) -> UiNode`
  free functions (no struct, no `impl Block` per block), one HOF
  signature covers every block.
- **No production-side cost.** The module is `#[cfg(test)]`-gated, so
  zero LoC ship to consumers, and the `pub(crate)` visibility keeps
  it internal to `prism-shell`.

### Decision-log entry

| Date | Decision | Rationale |
|---|---|---|
| 2026-05-10 | §39 lands: `components::testing::{test_node, lower_with}` consolidates the BuilderNode-fixture + default-cascade-lower boilerplate every shell-component test module was duplicating. Ten initial migrations land alongside the helper; the rest (~40) are an incremental cleanup. | The pattern was repeated ~50 times across `components/*.rs` — 10+ lines of struct-literal-with-defaults plus a 4-line `lower_one` adapter, both purely mechanical. Touching `BuilderNode`'s shape (which §28's `style` cascade and §22's `transform` did within the last sprint) used to require updating every test fixture; the helper centralises that. The function-rather-than-macro approach also means rust-analyzer / rustdoc see normal types, no proc-macro magic. The `pub(crate)` `#[cfg(test)]` gating keeps the helper internal to `prism-shell` with zero LoC shipped to consumers. |

## 40. `components::testing` migration sweep + `test_node_with_children`

**Strategy locked 2026-05-10 (post-§39).** §39 landed the helper plus
ten initial migrations; the remaining ~30 component test modules
were still hand-rolling the `BuilderNode { id, component, props,
children: vec![], layout_mode: …, transform: …, modifiers: vec![],
style: … }` literal and the `let cascade = …; let ctx = LowerCtx::new(None, &cascade); foo_lower(&ctx, &n, &cascade)` ceremony.
Every literal also carried four imports (`document::Node`,
`layout::LayoutMode`, `spatial::Transform2D`, `style::StyleProperties`)
that the helper module already owns.

**The collapse.** Every shell-component test module now imports
`crate::components::testing::{lower_with, test_node}` (or
`test_node_with_children` where the test feeds AST kids into the
resolver) and reduces the fixture to 1-2 lines. Files migrated in
this sweep:

- Plain dispatch: `dock_divider`, `toolbar_separator`, `help_tooltip`,
  `resize_handle`, `menu_item`, `nav_button`, `nav_page_row`,
  `nav_graph`, `schema_row`, `signal_connection_row`,
  `drag_number_field`, `menu_bar_row`, `command_palette`,
  `docs_content`, `field_editor`, `inspector_row`, `inspector_tree`,
  `launchpad`, `transform_editor`, `code_editor`,
  `component_palette`, `component_picker`, `toast_stack`.
- Registry-aware: `dock_tab_bar`, `schema_designer`, `signals_panel`,
  `properties_panel`, `docs_view`, `docs_sidebar`, `explorer`,
  `nav_page_list`, `context_menu`, `menu_dropdown`, `dock_workspace`,
  `workflow_page_bar`, `builder_canvas`. The helper still applies
  here; only the `LowerCtx::new(Some(reg.as_component_registry()), &cascade)` line stays inline.
- Children-bearing: `dock_panel`, `app_window`. Both now lean on
  the new `test_node_with_children(id, component, props, children)`
  variant — an additive 4-line addition next to `test_node` that
  shares the same struct-literal default block via internal
  delegation.

**Smart-pattern wins.**

- **One-stop import.** Every test module now opens with
  `use crate::components::testing::{lower_with, test_node};` instead
  of four `prism_*` paths. Adding a field to `BuilderNode` becomes a
  single edit in `components::testing` regardless of registry shape.
- **`test_node_with_children` is the dispatch handle, not a new
  abstraction.** `test_node` is now a one-liner that delegates to
  `test_node_with_children` with `vec![]`. The two consumers that
  need real kids (`dock_panel`'s `node(props, kids)` helper,
  `app_window`'s child-bearing fixture) compose through the same
  fields. No second source of truth.
- **Net deletion.** The sweep replaces ~330 lines of mechanical
  fixture boilerplate with ~110 lines of helper-mediated calls; the
  `prism_builder::layout::LayoutMode`,
  `prism_builder::style::StyleProperties as Cascade`,
  `prism_core::foundation::spatial::Transform2D` imports vanish from
  every migrated test module.
- **All 252 lib tests still pass** under the new fixture path —
  including the registry-aware ones and the child-bearing
  `dock_panel` / `app_window` cases that drive nested AST through
  the resolver.

### Decision-log entry

| Date | Decision | Rationale |
|---|---|---|
| 2026-05-10 | §40 lands: ~30 additional shell-component test modules migrate to `components::testing::{test_node, lower_with}`; `test_node_with_children` lands as a four-line additive variant for `dock_panel` / `app_window`. The "rest (~40)" punch list from §39's note is now fully retired apart from registry-bearing wrapper code that is structurally distinct. | §39 set up the helper but only migrated 10 of ~50 sites; carrying the boilerplate elsewhere muddies the rule that "every new component should use it from the start." The sweep eliminates the ambiguity and centralises the BuilderNode fixture shape in one place. `test_node_with_children` is additive (no breaking change to `test_node`'s signature) and only exists because two real test sites need to drive AST kids through the resolver — promote-then-reuse held: only collapse the children-bearing case once a second consumer materialised (`app_window` joining `dock_panel`). |

## 41. `props.rs` collapse + `with_common_signals` sweep

**Strategy locked 2026-05-10 (post-§40).** Two parallel chrome-side
duplications were still on the floor:

- **`props.rs` was 27 hand-written `bind_slot!(reg, "shell.X", |s: &AppState| s.SLOT.X_props())` invocations** plus a 20-row stub list, gated behind a pair of `#[macro_export]` macros (`bind!` / `bind_slot!`) used nowhere outside the file. Each row was a trivial forwarder; the macros existed only to hide a `Box::new(|ctx| PropEmission::from_props(...))` wrapping that the table itself can express directly.
- **22 component `*_signals` functions used the imperative push pattern**: `let mut s = common_signals(); s.push(SignalDef::new(...)); s` — re-implementing what `prism_builder::with_common_signals(vec![...])` already does. A further 7 components had a `*_signals` function whose entire body was `common_signals()` — pure restatement of `BlockSpec`'s default, registered via a redundant `.signals(foo_signals)`.

**The collapse.** Two new `'static` const tables in `props.rs`
([`SLOT_BINDINGS: &[(&str, fn(&AppState) -> Value)]`][slot] and
[`STUB_BINDINGS: &[&str]`][stub]) replace the macro-driven invocations:
non-capturing closure literals coerce to function pointers, the table
is heap-free, and `register_builtin_bindings` is now a pair of `for`
loops over the two tables. The `bind!` / `bind_slot!` macros are
deleted along with `#[macro_export]`. The 22 push-style signal
functions become one-liners around `with_common_signals(vec![...])`.
The 7 noop signal functions are deleted entirely; the `BlockSpec`
rows lose their `.signals(foo_signals)` builder call and fall back
to `default_signals` (which is `with_common_signals(vec![])` ==
`common_signals()` by definition).

**Smart-pattern wins.**

- **One declarative table per binding kind.** Adding a slot binding
  is one row in `SLOT_BINDINGS`; adding a stub is one entry in
  `STUB_BINDINGS`. The 27 macro-mediated rows used to read as
  imperative `register` calls — the table form makes "what's the
  full set of shell-block bindings" answerable in one glance.
- **No production-side macro surface.** `#[macro_export]` widens a
  crate's public API even when only used internally; deleting both
  `bind!` and `bind_slot!` shrinks `prism-shell`'s exported macro
  surface to zero. Future consumers can't pick up an undocumented
  binding helper that wasn't designed for them.
- **Default-bias on signals.** `BlockSpec::new` already defaults
  `signals` to `default_signals` (== `common_signals()`); the noop
  `*_signals` functions were ceremonial. Dropping them moves the
  rule "common signals come for free" from "every component must
  remember to call `common_signals()`" to "every `BlockSpec::new`
  inherits the default unless it adds something" — the shape that
  matched the rest of the spec from day one.
- **Conformance to `with_common_signals`.** The 22 push-pattern
  callers now route through one helper that already deduplicates by
  name. Future signal additions can't accidentally shadow a common
  one through field-order luck — the helper enforces the dedup
  contract.

**Net deletion.** ~155 LoC across `props.rs` and 29 component files;
zero new infrastructure. All 252 lib tests still pass under the new
binding table and the consolidated signal helpers; clippy
`-D warnings` clean.

[slot]: ../../packages/prism-shell/src/props.rs
[stub]: ../../packages/prism-shell/src/props.rs

### Decision-log entry

| Date | Decision | Rationale |
|---|---|---|
| 2026-05-10 | §41 lands: `SLOT_BINDINGS` / `STUB_BINDINGS` declarative const tables collapse `register_builtin_bindings`; `bind!` / `bind_slot!` macros deleted. 22 component `*_signals` functions converge on `with_common_signals(vec![...])`; 7 noop `*_signals` functions deleted along with their `.signals(...)` builder calls. | The two patterns shared a root cause: ceremony around defaults. The macros existed to hide a one-line `Box::new` wrap; the noop signal functions existed to restate `BlockSpec`'s default; the push pattern existed to re-implement an already-shipped helper. Collapsing each one to its minimum form (table row, missing builder call, helper call) cut ~155 LoC and tightened the "adding a feature is one row" invariant. The macros' deletion also drops two `#[macro_export]` symbols from `prism-shell`'s public surface, undoing leakage that existed only because the macros were the easiest hiding mechanism at landing time. |
