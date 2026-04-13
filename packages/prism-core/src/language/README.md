# language/

Languages, parsers, emitters, and the unified document/language registry. This category depends only on `foundation/` and has no React/DOM deps so it can run on either side of the Tauri bridge.

Loro CRDT is the source of truth and CodeMirror 6 is the sole editor; everything in this directory projects, parses, or emits from text/graph/binary bodies that live inside `PrismFile`.

## Subfolders

- [`document/`](./document/README.md) — `@prism/core/document`. `PrismFile` + `FileBody` discriminated union (text/graph/binary), the single file abstraction from ADR-002 §A1.
- [`registry/`](./registry/README.md) — `@prism/core/language-registry`. `LanguageContribution`, `LanguageSurface`, `LanguageCodegen`, and the unified `LanguageRegistry` that resolves by id, filename, or extension.
- [`codegen/`](./codegen/README.md) — `@prism/core/codegen`. `CodegenPipeline` with `Emitter` interface, `SymbolDef` DSL, `SymbolEmitter`/`TextEmitter` base classes, `SourceBuilder` helper.
- [`markdown/`](./markdown/README.md) — `@prism/core/markdown`. `createMarkdownContribution()` — reuses the single `parseMarkdown` tokenizer from `forms/`.
- [`luau/`](./luau/README.md) — `@prism/core/luau`. Browser Luau runtime via luau-web, debugger, AST helpers, and `createLuauContribution()`.
- [`expression/`](./expression/README.md) — `@prism/core/expression`. Scanner, parser, and evaluator for the Expression Engine, plus field resolvers for formula/lookup/rollup/computed fields.
- [`forms/`](./forms/README.md) — `@prism/core/forms`. `FieldSchema`, `DocumentSchema`, `FormState`, the wiki-link parser, and the canonical markdown block/inline tokenizer.
- [`syntax/`](./syntax/README.md) — `@prism/core/syntax`. LSP-like `SyntaxEngine` (diagnostics/completions/hover), schema-aware type inference, `.d.luau` generation, AST types, scanner, token stream, case utilities.
- [`facet/`](./facet/README.md) — `@prism/core/facet`. Facet system: FileMaker Pro-inspired layouts, spell engine, prose codec, sequencer, visual script steps, multi-language writers (TS/JS/C#/Luau/JSON/YAML/TOML), value lists, FacetStore.
