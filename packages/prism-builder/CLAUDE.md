# prism-builder

The Slint-native page builder that replaces Puck. Owns the
component-type registry, the document tree schema, the layout engine
(Taffy-backed CSS Grid / Flexbox / Block + free-form positioning),
two independent render paths (Slint DSL + HTML SSR), and the
property-panel field factories.

## Build & Test
- `cargo build -p prism-builder`
- `cargo build -p prism-builder --features interpreter` — pulls in
  `slint` + `slint-interpreter` + `spin_on` so the runtime compile
  path (`compile_slint_source` / `instantiate_document`) is
  available. `prism-shell` and `prism-studio` flip this on; the
  relay leaves it off so its dep graph stays Slint-free.
- `cargo test -p prism-builder` — 80 unit tests (baseline).
- `cargo test -p prism-builder --features interpreter` — adds one
  interpreter round-trip (81 tests total).

## Public surface
From `src/lib.rs`:

### Slint side
- `Component`, `ComponentId`, `RenderContext`, `RenderSlintContext`,
  `RenderError` — the Slint render trait. `Component::schema`
  returns `Vec<FieldSpec>`; `render_slint` emits `.slint` DSL into
  a shared `SlintEmitter`; `help_entry` returns an optional
  `HelpEntry` for context-sensitive tooltips (default `None`).
  Default render impl emits a transparent `Rectangle` wrapper.
- `ComponentRegistry`, `RegistryError` — the DI entry point.
  Implements `HelpProvider` — collects help entries from all
  registered components via `Component::help_entry()`.
- `starter::register_builtins(&mut ComponentRegistry)` — seeds the
  17-component Slint catalog.
- `render_document_slint_source(doc, registry, tokens)` — walks a
  document and returns a self-contained `.slint` source string.
- `compile_slint_source` / `instantiate_document` — gated behind
  `interpreter` feature.

### HTML SSR side
- `HtmlBlock`, `HtmlRegistry`, `HtmlRenderContext` — separate trait
  + registry for HTML rendering. Decoupled from `Component` so the
  relay's dep graph stays Slint-free.
- `register_html_builtins(&mut HtmlRegistry)` — seeds the 17-block
  HTML catalog (same component IDs as the Slint side).
- `render_document_html(doc, html_registry, tokens)` — walks a
  document against an `HtmlRegistry` and returns an HTML fragment.

### Layout engine (ADR-003)
- `PageLayout`, `PageSize`, `Orientation`, `TrackSize` — structural
  page properties (size, margins, bleed, CSS Grid template).
- `LayoutMode` (`Flow` | `Free`), `FlowProps`, `Dimension`,
  `FlowDisplay`, `FlexDirection`, `AlignOption`, `JustifyOption`,
  `GridPlacement` — per-node layout participation.
- `compute_layout(doc, viewport_size) -> ComputedLayout` — runs
  the Taffy layout pass + transform propagation, returns per-node
  `NodeLayout { rect, transform }`.
- `Node` now carries `layout_mode: LayoutMode` and
  `transform: Transform2D` (from `prism-core::foundation::spatial`).
- `BuilderDocument` now carries `page_layout: PageLayout`.

### Shared
- `BuilderDocument`, `Node`, `NodeId` — the serializable document
  tree (extended with layout + transform fields per ADR-003).
- `FieldSpec`, `FieldKind`, `NumericBounds`, `SelectOption`,
  `FieldValue` — the property-panel field factories.
- `Html`, `escape_text`, `escape_attr` — HTML builder.
- `SlintEmitter`, `SlintIdent` — `.slint` DSL emitter.

## Architecture
Ten modules in `src/`:

- `component.rs` — the `Component` trait (Slint-only) + `RenderError`
  + `RenderSlintContext` / `RenderContext`.
- `document.rs` — `BuilderDocument` + `Node` + `NodeId`. Nodes now
  carry `layout_mode` and `transform`; documents carry `page_layout`.
- `layout.rs` — the Taffy-backed layout engine (ADR-003). `PageLayout`,
  `LayoutMode`, `FlowProps`, `compute_layout`. 15 unit tests.
- `html_block.rs` — the `HtmlBlock` trait + `HtmlRegistry` +
  `HtmlRenderContext`. Independent from `component.rs`.
- `html_starter.rs` — 17 built-in HTML blocks + `register_html_builtins`.
- `registry.rs` — `ComponentRegistry` + field-factory primitives.
- `html.rs` — `Html` buffer + escape helpers.
- `slint_source.rs` — `SlintEmitter` wrapping `SourceBuilder`.
- `render.rs` — document-level walkers: `render_document_html`
  (uses `HtmlRegistry`), `render_document_slint_source` (uses
  `ComponentRegistry`), plus `compile_slint_source` /
  `instantiate_document` behind `interpreter`.
- `starter.rs` — 17 built-in Slint components + `register_builtins`.

## Adding a new block type

1. Add a struct implementing `Component` in `src/starter.rs` (Slint
   rendering).
2. Add a struct implementing `HtmlBlock` in `src/html_starter.rs`
   (HTML SSR). Use the same `ComponentId`.
3. Implement `schema` using `FieldSpec` builders.
4. Implement `render_slint` and `render_html` on their respective
   traits.
5. Register in both `register_builtins` and `register_html_builtins`.
6. Add unit tests — Slint tests in `starter.rs`, HTML tests in
   `html_starter.rs`.

## Dependencies
- `prism-core` — `design_tokens`, `language::codegen::SourceBuilder`,
  `foundation::geometry`, `foundation::spatial`.
- `glam` — SIMD-accelerated 2D math (Vec2, Affine2).
- `taffy` — CSS Grid + Flexbox + Block layout engine.
- `slint` / `slint-interpreter` / `spin_on` — optional, behind the
  `interpreter` feature. The relay keeps it off.
- No hard dep on `prism-shell` or `prism-relay`.
