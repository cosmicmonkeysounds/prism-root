# ADR-008: Replace Slint with Clay + `prism-ui` DSL

**Status:** Accepted
**Date:** 2026-05-04
**Supersedes:** the runtime-topology decision in ADR-007 (which kept
Slint and scoped the interpreter to the builder). ADR-007 remains the
authoritative record of *why* Slint's interpreter was constrained;
this ADR is the record of *why we are leaving Slint entirely*.

## Context

Prism's UI runs on Slint with `slint-interpreter` driving live edits.
Six months in, the integration works, but it imposes recurring costs
the framework cannot grow into:

1. **Concept impedance.** Every Prism primitive (signals, facets,
   connections, properties with type-aware editors, manifests, design
   tokens, the cascade) is lowered into Slint vocabulary
   (`callbacks`, `properties`, `ComponentFactory`) and raised back out
   on the codegen + inspector side. The
   `BuilderDocument` ↔ `.slint source` ↔ `slint-interpreter` triangle
   is load-bearing and every feature touches all three legs.
2. **Two render paths.** `Component::render_slint` (live UI) and
   `Component::render_html` (relay SSR) are independent walkers.
   Adding a block means writing it twice; drift between live and SSR
   output is a permanent risk.
3. **Interpreter fragility.** ADR-007 disabled live-preview on the
   full `app.slint` because of the `VRc` / `ChangeTracker` panic in
   `Flickable`. `ComponentContainer` / `component-factory` don't work
   in interpreted mode. Live edit features keep routing around
   interpreter limitations.
4. **Foreign DSL.** `.slint` is parsed by a separate
   `SlintSyntaxProvider`, has its own grammar, its own type system,
   its own tooling. Prism already owns `language::syntax::Scanner`
   and `language::codegen::SourceBuilder`; we are not using them on
   our own UI layer.
5. **Licence pressure.** Slint's royalty-free terms forced the
   workspace to GPL-3.0-or-later. Removing Slint reopens MIT/Apache.

## Decision

Replace Slint with a three-piece stack we own end-to-end:

1. **Layout: Clay** ([nicbarker/clay](https://github.com/nicbarker/clay))
   via a vendored Rust binding at `vendor/clay-layout/`. Clay does
   layout + render-command emission only — no windowing, no renderer,
   no input. We provide each.
2. **Renderer: pluggable backends** in a new
   `packages/prism-ui-runtime` crate — `femtovg` (native, via direct
   `winit` + `femtovg`), `web` (winit-on-wasm + WebGL2), and
   `html` (pure render-command → HTML/CSS string lowering for SSR).
   Three backends, one render-command stream.
3. **DSL: `prism-ui`** — an HTMX-inspired tag-element language with
   attribute-namespace behaviour (`on:click`, `bind:value`, `if`,
   `for`, `style:*`, `fct:*`, `sig:*`). Parsed by
   `prism-core::language::syntax::Scanner` (no regex, no string
   indexing — per the Prism Syntax rule). Compile-time codegen via a
   new `packages/prism-ui-build` crate; runtime parsing for live
   edits goes through the same parser, no separate interpreter.

## Three locked sub-decisions (Phase 0, 2026-05-04)

### 1. Licence → `MIT OR Apache-2.0`

The workspace flips to dual MIT/Apache at Phase 5 cutover (when the
last Slint dep leaves the tree). New crates created for this
migration (`prism-ui-runtime`, `prism-ui-build`,
`vendor/clay-layout`) ship under `MIT OR Apache-2.0` from day one to
mark the boundary; existing crates stay on `GPL-3.0-or-later` until
cutover. Clay itself is zlib-licensed; no copyleft pressure.

### 2. Clay binding → vendored fork

`vendor/clay-layout/` becomes a workspace member, forked from the
upstream `clay-layout` crate at a pinned commit. Rationale:

- The binding is small (~2 KLOC of FFI + helpers).
- Its FFI surface needs to evolve in lockstep with
  `prism-ui-runtime`'s layout-tree builder; upstream-velocity risk
  removed.
- Upstream-fix backports go through `git` cherry-picks; out-bound
  contributions go upstream as PRs.

### 3. DSL flavour → HTMX-inspired

A v0 strawman in the prior plan used curly-brace blocks
(`component Card { ... }`). We rejected it in favour of a tag-element
syntax with attribute-namespace behaviour:

```prism-ui
<container layout="flow" gap="{tokens.spacing.md}" on:click="emit save">
  <heading level="3">{title}</heading>
  <pill if="{badge}" tone="accent">{badge}</pill>
  <facet name="items" from="resource:posts" limit="5">
    <link href="{item.url}">{item.title}</link>
  </facet>
</container>
```

Rationale:

- **Lower floor.** "It's HTML with extra attributes" is true and
  one-sentence-explainable. Non-programmers who edit web pages can
  edit `.prism-ui` files immediately.
- **No ceiling penalty.** Every primitive from the strawman survives
  — facets, signals, control flow, design tokens, Luau handlers — as
  attribute namespaces with 1:1 lowerings to existing runtime types
  (`Connection`, `FacetDef`, `FieldSpec`, `ActionKind`).
- **HTML lowering is nearly free.** The DSL *is* HTML-shaped, so SSR
  becomes attribute-namespace serialisation rather than a parallel
  walker. `Component::render_html` and `HtmlRegistry` go away.
- **Editor tooling for free.** HTML highlighters, formatters, LSP
  clients all work on raw markup; Prism-aware diagnostics +
  completions layer on via `SyntaxProvider`.
- **Familiar to LLMs and humans.** Both populations have seen
  thousands of HTML examples; onboarding cost ≈ 0.

The action grammar inside attribute values (`emit signal`,
`set target.prop = expr`, `navigate to page`, `luau { ... }`) is a
single-line expression dialect parsed by the same Scanner, with
`luau { ... }` as the unbounded escape hatch.

## Consequences

### Positive

- **One render path.** `Component::layout(...) -> UiTree`; Clay lays
  out; the active backend lowers render commands to its native
  surface (femtovg / WebGL / HTML string). SSR and live UI render
  the same tree.
- **No interpreter.** Live edits re-parse `.prism-ui` → diff
  `BuilderDocument` → re-layout. The class of bugs ADR-007 documents
  cannot exist.
- **Prism owns the language.** Grammar, diagnostics, completions,
  codegen, and runtime all live under `prism-core::language` and
  `prism-ui-*`. No foreign DSL.
- **Licence freedom restored.**
- **HTML is a first-class output**, not a parallel walker.

### Negative

- **We own the renderer integration.** Slint bundled winit + femtovg
  + cosmic-text into one crate. We pull the three directly; Phase 1
  must prove the spike on native + web before Phase 4 can begin.
- **DSL design lock-in.** v0 grammar is permanent. Mitigation: lock
  v0 to the smallest grammar that covers `app.slint` today (~30
  keywords); grow conservatively.
- **Clay maturity risk.** Mitigated by the vendored fork; surface
  area we depend on stays small.
- **Mobile loses winit-bundled iOS/Android support.** Phase 6 picks a
  GL/Metal context.
- **Migration cliff.** Slint and `prism-ui` cannot coexist in a single
  `Shell`; Phase 4 builds `prism-ui` to feature-parity behind a cargo
  feature on a side branch; Phase 5 is the cutover.

## Implementation pointer

Full phase roadmap, scope inventory, and file-by-file Slint footprint
to retire live in `docs/dev/clay-migration-plan.md`. Phase 0
(decisions + scaffolding) starts on the same date as this ADR.
