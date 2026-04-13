# luau/

Browser Luau runtime, debugger, AST helpers, and the unified `LanguageContribution` for Luau. One directory for parsing (via a wasm-bindgen Luau parser under `pkg/`), execution (via `luau-web`), debugging, and language-registry integration. The same `.luau` scripts run here and in `mlua` on the daemon side.

```ts
import {
  initLuauSyntax,
  createLuauContribution,
  createLuauEngine,
  executeLuau,
} from '@prism/core/luau';
```

## Key exports

- `initLuauSyntax()` — call once at kernel startup to load the wasm parser; idempotent.
- `executeLuau(script, args?)` — one-shot async runner returning `LuauResult`.
- `createLuauEngine(globals?)` — persistent `LuauEngine` state for long-running contexts (re-export of `luau-web`'s `LuauState`).
- `fromLuauValue(val)` — recursively converts luau-web tables to plain JS objects/arrays.
- `createLuauDebugger(...)` with `LuauDebugger`, `TraceFrame`, `DebugRunResult` — step-through trace runner.
- `parseLuau` / `findUiCalls` / `findStatementLines` / `validateLuau` — async AST helpers; `parseLuauSync` / `findUiCallsSync` / `findStatementLinesSync` / `validateLuauSync` — sync variants (require `initLuauSyntax` first).
- `LuauUiCall` / `LuauUiArg` / `LuauUiParseResult` — AST helper types.
- `ensureLuauParserLoaded` / `getLuauParserSync` / `isLuauParserReady` — wasm bootstrap.
- `createLuauContribution()` — the `LanguageContribution` record for `prism:luau`.
- `createLuauSyntaxProvider()` — `SyntaxProvider` for diagnostics/completions/hover.
- `getLuauParserModule()` — access the raw parser module.

## Usage

```ts
import { initLuauSyntax, executeLuau } from '@prism/core/luau';

await initLuauSyntax();

const result = await executeLuau('return greet .. ", world!"', { greet: 'hello' });
if (result.success) console.log(result.value); // "hello, world!"
```
