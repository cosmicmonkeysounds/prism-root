# codegen/

Shared codegen pipeline from ADR-002 §A3. A single `CodegenPipeline` dispatches heterogeneous emitters to matching slots on a `CodegenInputs` bundle, unifying the previously parallel symbol/schema emitter hierarchies. Includes the `SymbolDef` describe-once-emit-many DSL (TS/C#/Luau/GDScript), `TextEmitter` base class for AST round-tripping, and the `SourceBuilder` indented-line helper.

```ts
import { CodegenPipeline, SourceBuilder, SymbolTypeScriptEmitter } from '@prism/core/codegen';
```

## Key exports

- `CodegenPipeline` — registers emitters (`register`) and runs them against a heterogeneous `CodegenInputs` bundle (`run(inputs, meta)`), returning `{ files, errors }`.
- `Emitter<TInput>` — interface: `id`, `inputKind`, `emit(input, meta) => CodegenResult`. Built-in kinds: `'symbols' | 'schema' | 'data' | 'ast' | 'facet'` plus any plugin-custom string.
- `EmittedFile` / `CodegenResult` / `CodegenMeta` / `CodegenInputs` — unified pipeline shapes.
- `SymbolDef` / `SymbolKind` / `SymbolParam` — the describe-once symbol DSL; `constantNamespace` and `fnSymbol` as convenience builders.
- `SymbolEmitter` — abstract base for symbol-input emitters; concrete subclasses `SymbolTypeScriptEmitter`, `SymbolCSharpEmitter`, `SymbolEmmyDocEmitter`, `SymbolGDScriptEmitter`.
- `NameTransform` + `tsNameTransform` / `csNameTransform` — per-language name casing hooks.
- `TextEmitter` — abstract base for serializing `RootNode` ASTs back to source text (markdown/yaml round-trip).
- `SourceBuilder` — indented-line builder used by every emitter (`line`, `blank`, `indent`, `dedent`, `block`, `constBlock`, `build`).

## Usage

```ts
import { CodegenPipeline, SymbolTypeScriptEmitter, constantNamespace } from '@prism/core/codegen';

const pipeline = new CodegenPipeline()
  .register(new SymbolTypeScriptEmitter({ moduleName: 'Conversations' }));

const result = pipeline.run(
  { symbols: [constantNamespace('CONVERSATIONS', { TAVERN_INTRO: 'tavern_intro' })] },
  { projectName: 'MyGame' },
);

for (const file of result.files) console.log(file.filename, file.content);
```
