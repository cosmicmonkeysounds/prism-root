# ADR-006: Live Bidirectional Slint Builder

**Status:** Implemented  
**Date:** 2026-04-20

## Context

Prism Builder originally used a one-directional flow:

```
BuilderDocument → SlintEmitter → .slint source → slint-interpreter → canvas
```

The user edited a structured `Node` tree through property panels and a
WYSIWYG canvas. Changes updated `BuilderDocument`, which regenerated
`.slint` source and recompiled. The generated source was opaque — the
user never saw it, couldn't hand-edit it, and there was no mapping
between canvas elements and source positions.

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

Prism already had substantial infrastructure that maps to these
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

Make `.slint` source the **canonical format**. The builder is fully
bidirectional: GUI manipulations perform surgical text edits on
`.slint` source; hand-edited `.slint` source recompiles and derives
the structured document on demand. Three types enable this.

### 1. SourceMap (marker-based)

Marker comments (`// @node-start:id:component` / `// @node-end:id`)
embedded in the generated `.slint` source enable bidirectional mapping
between node IDs and byte ranges. These are valid Slint comments —
the compiler ignores them, but the builder uses them to locate nodes
for surgical text edits.

```rust
pub struct SourceMap {
    spans: IndexMap<NodeId, SourceSpan>,
}

pub struct SourceSpan {
    pub node_id: NodeId,
    pub component: ComponentId,
    pub start: usize,   // byte offset in source
    pub end: usize,
    pub props: Vec<PropSpan>,
}

pub struct PropSpan {
    pub key: String,
    pub value_start: usize,
    pub value_end: usize,
}
```

Forward mapping: `source_map.span_for_node(node_id)` → `SourceSpan`
Reverse mapping: `source_map.node_at_offset(byte_offset)` → `NodeId`

`PropSpan`s are extracted by scanning `key: value;` lines within each
node's byte range, enabling property-level surgical replacement.

### 2. Slint SyntaxProvider

`BuilderSyntaxProvider` in `prism-builder` (behind the `interpreter`
feature) provides compiler-backed language services:

- `diagnose()` — compile source via `slint-interpreter::Compiler`,
  map diagnostics to `prism_core::Diagnostic`.
- `complete()` — at a given offset, use the `SourceMap` to determine
  context (inside which component? which property?), then offer
  completions from `ComponentRegistry::get(id).schema()`.
- `hover()` — use `SourceMap` + `Component::help_entry()` to return
  `HelpEntry` content for the element/property under the cursor.

This unifies the code editor panel and the builder canvas: both
consume the same `SyntaxProvider` for the same `.slint` source.

### 3. LiveDocument (source-first)

The type that holds the canonical source and all derived state:

```rust
pub struct LiveDocument {
    // Canonical
    pub source: String,
    pub source_map: SourceMap,
    pub editor: EditorState,
    pub diagnostics: Vec<LiveDiagnostic>,
    compiled: Option<ComponentDefinition>,
    // Owned context
    registry: Arc<ComponentRegistry>,
    tokens: DesignTokens,
    // Derived cache
    derived_document: Option<BuilderDocument>,
}
```

**Source is truth.** `BuilderDocument` is derived on demand via
`derive_document_from_source()` and cached. The cache is invalidated
after every source mutation.

**Constructors:**
- `from_source(source, registry, tokens)` — primary path. Compiles,
  builds source map, caches nothing until `document()` is called.
- `from_document(doc, registry, tokens)` — migration/import path.
  Generates marked source via `render_document_slint_source_mapped()`,
  then proceeds as source-first.

**Source mutation methods** (all perform surgical text edits):
- `edit_prop_in_source(node_id, key, value_text)` — replaces a
  `PropSpan`'s value bytes.
- `insert_node_in_source(parent_id, component, node_id, props)` —
  inserts a marked block at the end of the parent's children.
- `remove_node_from_source(node_id)` — excises the full span.
- `move_node_in_source(node_id, direction)` — cuts and re-inserts
  the span to reorder siblings.

After every mutation: rebuild source map, invalidate derived cache,
sync editor buffer, recompile.

**Selection bridge:**
- `select_node(id)` → editor line/col range for highlighting.
- `node_at_cursor()` → node ID at the current editor cursor.

### Source parsing (roundtrip)

`source_parse.rs` implements the inverse of the render walker:
- `derive_document_from_source(source, source_map)` — reconstructs
  a `BuilderDocument` from marker-annotated `.slint` source by
  walking `@node-start`/`@node-end` markers, extracting `key: value;`
  properties, and parsing Slint value literals.
- `parse_slint_value(val)` → `serde_json::Value` — parses string
  literals, booleans, numbers (with `px` suffix), colors.
- `format_slint_value(value, kind)` → `String` — the inverse,
  for source-based property edits.

### Persistence

`Page` stores `.slint` source as the canonical field:

```rust
pub struct Page {
    pub source: String,              // canonical .slint source
    #[serde(default, skip_serializing)]
    pub document: BuilderDocument,   // derived, not persisted
    // ...
}
```

The shell's `LiveDocument` syncs to/from `Page.source` on page
switches. Undo/redo snapshots source text (`SourceSnapshot`), not
`BuilderDocument`.

### Integration with existing Prism infrastructure

- **`SourceBuilder`** — `SlintEmitter` already wraps it; the
  `MappedEmitter` extends it with marker injection.
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
- **Source-first simplifies the architecture.** With `.slint` source
  as the single canonical format, there is no synchronization
  problem between document model and source — the document is
  always derived. GUI mutations are text edits, same as code editor
  mutations.
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
- **Marker comments are invisible to the compiler.** The
  `// @node-start` / `// @node-end` annotations are valid Slint
  comments. They enable the builder's structural mapping without
  affecting compilation or runtime behavior.

## Consequences

- `render_document_slint_source_mapped()` returns `(String, SourceMap)`
  with marker comments injected. The unmarked
  `render_document_slint_source()` remains for clean export.
- `SourceMap`, `SourceSpan`, `PropSpan` types in
  `prism-builder::source_map`.
- `MappedEmitter` in `prism-builder::source_map` — alternative
  emitter that injects marker comments during rendering.
- `derive_document_from_source()` and value parsing/formatting
  helpers in `prism-builder::source_parse`.
- `LiveDocument` in `prism-builder::live` (behind `interpreter`
  feature) — source-first type owning `Arc<ComponentRegistry>` and
  `DesignTokens`. All mutations are source text edits; document
  is derived on demand.
- `SourceEditError` — error type for source mutations
  (`NodeNotFound`, `PropNotFound`, `CompileError`).
- `BuilderSyntaxProvider` in `prism-builder::syntax_provider`
  (behind `interpreter`) — compiler-backed completions and hover.
- `Page.source` is the persisted field; `Page.document` is derived
  and `skip_serializing`.
- Shell undo/redo snapshots `.slint` source text, not
  `BuilderDocument`.
- The code editor panel shows the live `.slint` source for the
  active page, updated in real time as the user manipulates the
  GUI. Edits in the code panel trigger recompile + preview update.
- The builder canvas and the code editor are two views of the
  same `LiveDocument` — selection in one highlights in the other.
