/**
 * Luau — one directory for parsing, runtime, debugging, and language
 * integration. Folded per ADR-002 §A4 / Phase 2.
 *
 * Usage:
 *
 * ```ts
 * import {
 *   initLuauSyntax,
 *   findUiCalls,
 *   createLuauLanguageDefinition,
 *   createLuauEngine,
 *   createLuauDebugger,
 * } from "@prism/core/luau";
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

// ── Runtime ──────────────────────────────────────────────────────────────────
export { executeLuau, createLuauEngine, fromLuauValue } from "./luau-runtime.js";
export type { LuauEngine } from "./luau-runtime.js";

// ── Debugger ─────────────────────────────────────────────────────────────────
export { createLuauDebugger } from "./luau-debugger.js";
export type { LuauDebugger, TraceFrame, DebugRunResult } from "./luau-debugger.js";

// ── WASM parser bootstrap ────────────────────────────────────────────────────
export {
  ensureLuauParserLoaded,
  getLuauParserSync,
  isLuauParserReady,
} from "./wasm-loader.js";

// ── AST helpers ──────────────────────────────────────────────────────────────
export type { LuauUiCall, LuauUiArg, LuauUiParseResult } from "./luau-ast.js";
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

// ── LanguageDefinition + SyntaxProvider ──────────────────────────────────────
export {
  createLuauLanguageDefinition,
  getLuauParserModule,
} from "./luau-language.js";
export { createLuauSyntaxProvider } from "./luau-provider.js";

// ── Convenience initializer ──────────────────────────────────────────────────

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
