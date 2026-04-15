# prism-builder

The Slint-native page builder that replaces Puck. **Phase 3 closed
2026-04-15.** Owns the component-type registry, the document tree
schema, two render targets (semantic HTML SSR + `.slint` DSL
emitter), and the property-panel field factories.

## Build & Test
- `cargo build -p prism-builder`
- `cargo build -p prism-builder --features interpreter` — pulls in
  `slint` + `slint-interpreter` + `spin_on` so the runtime compile
  path (`compile_slint_source` / `instantiate_document`) is
  available. `prism-shell` and `prism-studio` flip this on; the
  relay leaves it off so its dep graph stays Slint-free.
- `cargo test -p prism-builder` — 45 unit tests (baseline).
- `cargo test -p prism-builder --features interpreter` — adds one
  interpreter round-trip that compiles the walker's synthesized
  source through `slint_interpreter::Compiler` (46 tests total).

## Public surface
From `src/lib.rs`:

- `Component`, `ComponentId`, `RenderContext`, `RenderSlintContext`,
  `RenderHtmlContext`, `RenderError` — the trait every renderable
  block implements. `Component::schema` returns `Vec<FieldSpec>`;
  `render_slint` emits `.slint` DSL into a shared `SlintEmitter`;
  `render_html` emits HTML into an `Html` buffer. Both have safe
  default impls (Rectangle / div-wrapper) so a bare `impl Component`
  already walks without panicking.
- `BuilderDocument`, `Node`, `NodeId` — the serializable document
  tree Studio saves to disk.
- `ComponentRegistry`, `RegistryError` — the DI entry point.
  `register` is idempotent-per-id; `iter` walks the full catalog in
  registration order.
- `FieldSpec`, `FieldKind`, `NumericBounds`, `SelectOption`,
  `FieldValue` — the property-panel field factories. Typed builders:
  `text`, `textarea`, `number`, `integer`, `boolean`, `select`,
  `color`, plus chainable `required()`, `with_default(Value)`, and
  `with_help(&str)`. `FieldValue::read_{string,number,integer,boolean}`
  pull values from a node's `props` with bounds clamping.
- `Html`, `escape_text`, `escape_attr` — allocation-light HTML
  builder used by `render_html` impls.
- `SlintEmitter`, `SlintIdent` — `.slint` DSL emitter on top of
  `prism_core::language::codegen::SourceBuilder`. `SlintEmitter`
  exposes `line`, `blank`, `block`, `prop_string` (escaped),
  `prop_px`, `prop_int`, `prop_float`, `prop_bool`, `prop_color`,
  `build`. `SlintIdent::normalize` produces safe Slint identifiers.
- `render_document_html(doc, registry, tokens) -> Result<String, _>`
  — walks a document against a registry and returns a ready-to-serve
  HTML fragment. `prism-relay` calls this per request.
- `render_document_slint_source(doc, registry, tokens) -> Result<String, _>`
  — walks the same document and returns a self-contained `.slint`
  source string wrapping the walked tree in
  `export component BuilderRoot inherits Window { … }`.
- `compile_slint_source(source) -> Result<ComponentDefinition, _>`
  and `instantiate_document(doc, registry, tokens) -> Result<ComponentInstance, _>`
  — gated behind `interpreter`. Feeds the walker output into
  `slint_interpreter::Compiler::build_from_source` via `spin_on`
  (the async compile is synchronous as long as no file loader is
  installed).
- `starter::register_builtins(&mut ComponentRegistry)` — seeds the
  shared five-component starter catalog (`heading`, `text`, `link`,
  `image`, `container`). Both `prism-relay::AppState::new` and
  `prism-shell::Shell::from_state` call this on boot; adding a new
  default block means editing `src/starter.rs` once.

## Architecture
Seven modules in `src/`:

- `component.rs` — the `Component` trait + `RenderError` + two
  context types (`RenderSlintContext` and `RenderHtmlContext`, each
  with `render_child` / `render_children` helpers for layout-only
  parents). Plus `RenderContext` — a simpler slot-free ctx kept as
  the bridge for host-side code that wants typed values without
  DSL generation.
- `document.rs` — `BuilderDocument` + `Node` + `NodeId`. The
  serializable tree Studio saves / loads.
- `registry.rs` — `ComponentRegistry` backed by an `IndexMap` keyed
  by `ComponentId`, plus the field-factory primitives (`FieldSpec`,
  `FieldKind`, `NumericBounds`, `SelectOption`, `FieldValue`).
  Register once at boot, look up by id at render time, walk the
  iter to power palettes.
- `html.rs` — `Html` buffer + `escape_text` / `escape_attr`. Thin
  wrapper around `String` with `open` / `open_attrs` / `close` /
  `void` / `text` / `raw` / `doctype` helpers.
- `slint_source.rs` — `SlintEmitter` wrapping `SourceBuilder`
  (4-space indent), `SlintIdent::normalize`, `escape_slint_string`,
  `rgba_to_slint_literal`. The DSL emitter every `render_slint`
  impl writes into.
- `render.rs` — document-level walkers: `render_document_html`,
  `render_document_slint_source`, and (behind `interpreter`)
  `compile_slint_source` + `instantiate_document` +
  `InstantiateError`.
- `starter.rs` — the five built-in components + `register_builtins`.
  Shared between the shell and the relay.

## Adding a new block type
Contributors go through the registry. Do not hand-wire a `Node`,
stash a block factory in a module-level static, or duplicate a
component in a downstream crate.

1. Add a struct implementing `Component` (usually in
   `src/starter.rs` if the block should ship by default).
2. Decide the component's `ComponentId` (stable string, versioned
   when the props shape changes).
3. Implement `schema` using `FieldSpec` builders — never hand-roll
   the `FieldKind`/`NumericBounds` enums inline; the builders
   preserve default/required invariants.
4. Implement `render_html` and `render_slint`. Reuse the context
   helpers (`ctx.render_children`) for layout-only slots.
5. Register it: either in `starter::register_builtins` (default
   catalog) or via a host-side `ComponentRegistry::register` call.
6. Add unit tests alongside the component — both targets should
   round-trip through `render_document_html` and
   `render_document_slint_source`.

## Dependencies
- `prism-core` — `design_tokens`, `language::codegen::SourceBuilder`.
- `slint` / `slint-interpreter` / `spin_on` — optional, behind the
  `interpreter` feature. Used only by `render::compile_slint_source`
  and `render::instantiate_document`. The relay keeps the feature
  off so its dep graph stays Slint-free.
- No hard dep on `prism-shell` or `prism-relay`: both of those
  depend on `prism-builder`, not the other way around. Keep the
  graph that direction.
