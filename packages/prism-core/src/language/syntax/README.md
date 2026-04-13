# syntax/

LSP-like intelligence layer for the expression and scripting languages. `createSyntaxEngine` exposes `diagnose` / `complete` / `hover` / `inferType` / `validateTypes` through pluggable `SyntaxProvider`s, with a built-in expression provider wired in out of the box. Also hosts the schema-aware type checker that walks `SchemaContext` to check formula types against `EntityFieldDef`s and the `.d.luau` generator that emits LuaLS stubs from an `ObjectRegistry`.

```ts
import { createSyntaxEngine, generateLuauTypeDef } from '@prism/core/syntax';
```

## Key exports

- `createSyntaxEngine(options?)` — builds a `SyntaxEngine` with `getProvider` / `listProviders` / `registerProvider` / `diagnose` / `complete` / `hover` / `inferType` / `validateTypes`.
- `createExpressionProvider()` — the built-in `SyntaxProvider` for the Expression Engine.
- `inferNodeType(node, context?)` — type inference on an `AnyExprNode`.
- `generateLuauTypeDef(context)` — emits a `.d.luau` type definition from a `SchemaContext`.
- `FIELD_TYPE_MAP` — maps `EntityFieldType` to `ExprType`.
- `BUILTIN_FUNCTIONS` — registry of builtin function signatures (arity, params, return type, docs).
- `Diagnostic` / `DiagnosticSeverity` / `CompletionItem` / `CompletionKind` / `HoverInfo` / `TextRange` — LSP-ish shapes returned by providers.
- `SchemaContext` / `TypeInfo` / `FieldTypeMapping` / `LuauTypeDef` / `FunctionSignature` / `SyntaxProvider` / `SyntaxEngineOptions` / `SyntaxEngine` — engine types.
- `Position` / `SourceRange` / `SyntaxNode` / `RootNode` + `posAt` / `range` — generic AST node types shared by every language contribution.
- `Scanner` / `ScannerState` / `ScanError` / `isDigit` / `isIdentStart` / `isIdentChar` — reusable character-class scanner.
- `TokenStream` / `Token` / `BaseToken` / `TokenError` — pull-based token stream over a Scanner.
- `safeIdentifier`, `toCamelCase`, `toPascalCase`, `toSnakeCase`, `toScreamingSnake`, `toCamelIdent`, `toPascalIdent`, `toScreamingSnakeIdent` — case utilities used by every emitter.
- `TokenContext` / `TokenFilter` / `SpellCheckDiagnostic` / `PersonalDictionary` / `SpellChecker` — spell-check adapter types (implemented in `facet/`).

## Usage

```ts
import { createSyntaxEngine } from '@prism/core/syntax';

const engine = createSyntaxEngine();
const diags = engine.diagnose('price * (1 + taxRate');
// diags[0].message describes the unclosed paren
```
