# ADR-006: Live Bidirectional Slint Builder

**Status:** Accepted  
**Date:** 2026-04-20

## Context

Prism Builder's current flow is one-directional:

```
BuilderDocument → SlintEmitter → .slint source → slint-interpreter → canvas
```

The user edits a structured `Node` tree through property panels and a
WYSIWYG canvas. Changes update `BuilderDocument`, which regenerates
`.slint` source and recompiles. The generated source is opaque — the
user never sees it, can't hand-edit it, and there's no mapping between
canvas elements and source positions.

Slintpad (Slint's own editor at `tools/slintpad/` + `tools/lsp/`)
demonstrates a superior architecture: a **bidirectional** loop where
GUI interactions generate source text edits and source text edits
update the preview. The visual editing engine (element selection,
property editing, drag-and-drop, component library) is 100% Rust
in `tools/lsp/preview/`, shared between web (Slintpad via WASM) and
native (VS Code extension, standalone LSP). Key modules:

- `element_selection.rs` — maps rendered geometry ↔ source positions
- `properties.rs` — converts property edits into source text changes
- `drop_location.rs` — computes insertion text for drag-and-drop

Prism already has substantial infrastructure that maps to these
concerns:

| Slintpad concern | Prism equivalent |
|-----------------|-----------------|
| Source generation | `SlintEmitter` + `SourceBuilder` (codegen) |
| Source parsing | `Scanner`, `TokenStream`, `SyntaxNode` (prism-core syntax) |
| Diagnostics | `SyntaxProvider::diagnose()` |
| Completions | `SyntaxProvider::complete()` |
| Hover tooltips | `HelpRegistry` + `Component::help_entry()` |
| Text editing | `EditorState` (ropey-backed buffer, cursor, selection) |
| Component catalog | `ComponentRegistry` + `FieldSpec` schema |
| Runtime compile | `slint-interpreter::Compiler` (already used) |

## Decision

Make the builder **bidirectional**: GUI manipulations write `.slint`
source; hand-edited `.slint` source updates the canvas. Three new
types enable this.

### 1. SourceMap on SlintEmitter

Extend `SlintEmitter` to track the byte ranges it emits for each
`BuilderDocument` node. When a component calls `out.block(...)`, the
emitter records the start/end byte offset keyed by `NodeId`.

```rust
pub struct SourceMap {
    spans: IndexMap<NodeId, SourceSpan>,
}

pub struct SourceSpan {
    pub node_id: NodeId,
    pub component: ComponentId,
    pub start: usize,   // byte offset in generated source
    pub end: usize,
    pub props: Vec<PropSpan>,  // per-property spans
}

pub struct PropSpan {
    pub key: String,
    pub value_start: usize,
    pub value_end: usize,
}
```

This is Prism's equivalent of Slintpad's `element_selection.rs` —
the bridge between canvas geometry and source code positions.

Forward mapping: `source_map.span_for_node(node_id)` → `SourceSpan`
Reverse mapping: `source_map.node_at_offset(byte_offset)` → `NodeId`

### 2. Slint SyntaxProvider

Register a `SyntaxProvider` for `.slint` source in `prism-builder`
(behind the `interpreter` feature, since diagnostics come from the
compiler):

- `diagnose()` — compile source via `slint-interpreter::Compiler`,
  map diagnostics to `prism_core::Diagnostic`.
- `complete()` — at a given offset, use the `SourceMap` to determine
  context (inside which component? which property?), then offer
  completions from `ComponentRegistry::get(id).schema()`.
- `hover()` — use `SourceMap` + `HelpRegistry` to return
  `HelpEntry` content for the element/property under the cursor.

This unifies the code editor panel and the builder canvas: both
consume the same `SyntaxProvider` for the same `.slint` source.

### 3. LiveDocument

The type that holds the bidirectional state:

```rust
pub struct LiveDocument {
    pub document: BuilderDocument,
    pub source: String,
    pub source_map: SourceMap,
    pub editor: EditorState,         // ropey-backed, for the code side
    // Behind `interpreter` feature:
    pub compiled: Option<ComponentDefinition>,
    pub diagnostics: Vec<Diagnostic>,
}
```

Mutation flows:

**GUI → Source** (property panel edit, drag-and-drop, canvas
interaction):
1. Mutate `document.root` (update node props, add/remove nodes)
2. Re-render: `render_document_slint_source_mapped()` → new
   `(source, source_map)`
3. Update `editor` buffer to match new source
4. Recompile via `compile_slint_source()` → update `compiled` +
   `diagnostics`

**Source → GUI** (hand-editing in the code panel):
1. `editor` buffer changes via `EditorState::insert_char()` etc.
2. Attempt recompile — if it succeeds, the source is valid
3. Parse the source to extract structure (future: roundtrip parser)
4. Update canvas preview from the compiled `ComponentInstance`

In Phase 1 (this ADR), only the GUI→Source direction is fully
implemented. The Source→GUI direction shows the live preview but
does not roundtrip back to `BuilderDocument` — that requires a
`.slint` → `BuilderDocument` parser, which is Phase 2.

### Integration with existing Prism infrastructure

- **`SourceBuilder`** — `SlintEmitter` already wraps it; the
  source map extends it with byte-offset tracking.
- **`Scanner` / `TokenStream`** — the Slint `SyntaxProvider` uses
  the shared scanning infrastructure from `prism-core::language::syntax`
  for tokenization, not a hand-rolled tokenizer.
- **`EditorState`** — the code editor panel already uses this;
  the `LiveDocument` shares the same instance so changes in the
  code panel flow through.
- **`HelpRegistry`** — property hover in the code editor pulls
  from the same registry the canvas tooltip system uses.
- **`ComponentRegistry`** — schema-driven completions in the code
  editor use the same `FieldSpec` descriptors the property panel
  uses.

### Integration with Slint's own tools

Slint ships several tools (`tools/lsp/`, `tools/viewer/`,
`tools/compiler/`) that complement this architecture:

- **`slint-interpreter`** — already a dependency. The live compile
  loop uses `Compiler::build_from_source()` exactly as the LSP
  preview engine does.
- **Slint LSP diagnostics** — the compiler's diagnostic output
  maps to `prism_core::Diagnostic` for inline error squiggles in
  the code editor.
- **Property widget patterns** — Slint's LSP UI ships
  `property-widgets.slint`, `property-view.slint`, `resizer.slint`,
  and `draggable-panel.slint`. These are reference implementations
  for the Slint rendering side of the dock system (ADR-005).
- **`ComponentFactory` / `ComponentContainer`** — Slint's
  experimental API for embedding interpreter-compiled components
  at runtime. The builder canvas will use this to display the live
  preview inline rather than in a separate window.

## Rationale

- **Bidirectional is what professionals expect.** Every modern
  creative tool (Unity, Godot, Figma, DaVinci Resolve, Slintpad
  itself) treats the visual editor and the code/data as two views
  of the same truth. One-directional GUI→data limits power users.
- **Reuse over reinvent.** Prism already has a scanner, token
  stream, AST types, syntax provider trait, editor state, help
  registry, component registry, and codegen emitter. The
  `SourceMap` and `LiveDocument` bridge these existing pieces
  rather than building a parallel stack.
- **Slint's compiler IS the validator.** Rather than writing a
  Slint parser and validator, we use `slint-interpreter::Compiler`
  as the single source of truth. The `SourceMap` provides the
  structural mapping; the compiler provides the correctness
  guarantee.
- **Phased delivery.** GUI→Source in Phase 1 gives immediate value
  (see and edit the generated code). Source→GUI roundtrip in Phase 2
  enables full power-user workflows.

## Consequences

- `SlintEmitter` gains source-map tracking. A new
  `render_document_slint_source_mapped()` function returns
  `(String, SourceMap)` alongside the existing
  `render_document_slint_source()` (unchanged, backward-compatible).
- New `SourceMap`, `SourceSpan`, `PropSpan` types in
  `prism-builder::slint_source`.
- New `LiveDocument` type in `prism-builder` (behind `interpreter`
  feature).
- `prism-builder` gains an optional dependency on
  `prism-core`'s syntax types for the `SyntaxProvider` impl.
- The code editor panel in `prism-shell` will show the live
  `.slint` source for the active page, updated in real time as
  the user manipulates the GUI. Edits in the code panel trigger
  recompile + preview update.
- The builder canvas and the code editor become two views of the
  same `LiveDocument` — selection in one highlights in the other.
