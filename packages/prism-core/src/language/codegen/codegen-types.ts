/**
 * Codegen pipeline types — unified shapes for every emitter.
 *
 * Introduced by ADR-002 §A3. The `inputKind` discriminator lets
 * `CodegenPipeline` accept heterogeneous emitters (symbol tables, schema
 * models, AST trees, facet configs, raw data blobs, plugin-custom
 * shapes) and route each one by its declared input kind. Before the
 * unification there were two parallel emitter hierarchies —
 * `syntax/codegen/` (symbols) and `facet/emitters.ts` (schemas) — that
 * could not compose.
 *
 * `EmitterInputKind` is intentionally an open `string` type: the core
 * ships with a handful of well-known kinds but plugins may introduce
 * their own without forking the pipeline. Callers pass a
 * `CodegenInputs` bundle — a plain map from kind → value — and the
 * pipeline dispatches each registered emitter to the matching slot.
 */

export interface EmittedFile {
  filename: string;
  content: string;
  language: 'typescript' | 'javascript' | 'csharp' | 'rust' | 'json' | string;
}

export interface CodegenMeta {
  projectName: string;
  version?: string;
  [key: string]: unknown;
}

export interface CodegenResult {
  files: EmittedFile[];
  errors: string[];
}

/**
 * The shape of data an `Emitter` consumes. Well-known built-in kinds:
 *
 * - `'symbols'` — `SymbolDef[]` (used by `SymbolEmitter` subclasses for
 *   TS/C#/GDScript/Luau stubs).
 * - `'schema'`  — `SchemaModel` (used by the `TypeScriptWriter` /
 *   `CSharpWriter` / `LuauWriter` family in `facet/emitters.ts`).
 * - `'data'`    — arbitrary plain JS values serialized to a data file
 *   (used by `JsonWriter` / `YamlWriter` / `TomlWriter`).
 * - `'ast'`     — `RootNode` (used by `TextEmitter` subclasses for
 *   round-tripping).
 * - `'facet'`   — facet-builder configs (used by the Luau facet
 *   emitters wrapping `facet-builders.ts`).
 *
 * Plugins may use any other string as a custom kind, as long as callers
 * populate the matching slot on `CodegenInputs`.
 */
export type EmitterInputKind =
  | 'symbols'
  | 'schema'
  | 'data'
  | 'ast'
  | 'facet'
  | (string & {});

export interface Emitter<TInput = unknown> {
  id: string;
  inputKind: EmitterInputKind;
  emit(input: TInput, meta: CodegenMeta): CodegenResult;
}

/**
 * A heterogeneous bundle of inputs keyed by the same discriminator used
 * on `Emitter.inputKind`. The well-known slots are typed as `unknown`
 * for discoverability; the index signature allows plugins to add
 * custom kinds without forking the type.
 */
export interface CodegenInputs {
  symbols?: unknown;
  schema?: unknown;
  data?: unknown;
  ast?: unknown;
  facet?: unknown;
  [kind: string]: unknown;
}
