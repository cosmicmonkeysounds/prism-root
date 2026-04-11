/**
 * Luau syntax integration — full-moon AST plugged into the Helm-inherited
 * syntax/codegen system.
 *
 * Usage:
 *
 * ```ts
 * import {
 *   initLuauSyntax,
 *   findUiCalls,
 *   createLuauLanguageDefinition,
 * } from "@prism/core/syntax";
 *
 * // Once, at kernel startup:
 * await initLuauSyntax();
 *
 * // From panels / debugger (async):
 * const result = await findUiCalls(source);
 *
 * // From the language registry (sync, post-init):
 * registry.register(createLuauLanguageDefinition());
 * ```
 */

export {
  ensureLuauParserLoaded,
  getLuauParserSync,
  isLuauParserReady,
} from "./wasm-loader.js";

export type {
  LuauUiCall,
  LuauUiArg,
  LuauUiParseResult,
} from "./luau-ast.js";

export {
  parseLuau,
  findUiCalls,
  findStatementLines,
  validateLuau,
  parseLuauSync,
  findUiCallsSync,
  findStatementLinesSync,
  validateLuauSync,
} from "./luau-ast.js";

export {
  createLuauLanguageDefinition,
  getLuauParserModule,
} from "./luau-language.js";

export { createLuauSyntaxProvider } from "./luau-provider.js";

// ── Convenience initializer ─────────────────────────────────────────────────

import { ensureLuauParserLoaded } from "./wasm-loader.js";

/**
 * Initialise the Luau parser. Call once at kernel startup before registering
 * `createLuauLanguageDefinition()` on a `LanguageRegistry` or invoking any
 * of the sync helpers (`parseLuauSync`, `findUiCallsSync`, etc).
 *
 * Idempotent — subsequent calls return the same cached promise.
 */
export async function initLuauSyntax(): Promise<void> {
  await ensureLuauParserLoaded();
}
