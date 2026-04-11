/**
 * LanguageDefinition for Luau, backed by full-moon via WASM.
 *
 * Implements the synchronous `LanguageDefinition` interface from
 * `language-registry.ts`. Because the interface is sync but the underlying
 * parser needs async WASM init, callers MUST `await ensureLuauParserLoaded()`
 * (or `initLuauSyntax()` from this module's index) exactly once before
 * invoking `Processor.process(...)` or `LanguageDefinition.parse(...)`.
 *
 * Parse errors are reported into the `ProcessorContext.diagnostics` channel
 * and the returned tree is an empty `{ type: 'root', children: [] }` so the
 * pipeline can continue through transform/compile phases without throwing.
 */

import type { LanguageDefinition } from "../language-registry.js";
import type { RootNode } from "../ast-types.js";
import { getLuauParserSync, isLuauParserReady } from "./wasm-loader.js";
import { parseLuauSync, validateLuauSync } from "./luau-ast.js";

/**
 * Create a `LanguageDefinition` for Luau. Register it on a
 * `LanguageRegistry` with `registry.register(createLuauLanguageDefinition())`.
 */
export function createLuauLanguageDefinition(): LanguageDefinition {
  return {
    id: "luau",
    extensions: [".luau", ".lua"],
    mimeTypes: ["text/x-luau", "text/x-lua"],
    parse(source, ctx): RootNode {
      if (!isLuauParserReady()) {
        ctx.report({
          severity: "error",
          message:
            "Luau parser not initialized — call initLuauSyntax() (or ensureLuauParserLoaded()) during kernel startup before processing Luau sources",
          source: "luau",
        });
        return { type: "root", children: [] };
      }
      try {
        // Surface parser diagnostics into the processor context first —
        // full-moon can return partial ASTs even with errors present.
        for (const diag of validateLuauSync(source)) {
          ctx.report({
            severity: "error",
            message: diag.message,
            source: "luau",
          });
        }
        return parseLuauSync(source);
      } catch (err) {
        ctx.report({
          severity: "error",
          message: err instanceof Error ? err.message : String(err),
          source: "luau",
        });
        return { type: "root", children: [] };
      }
    },
  };
}

/**
 * Drain the parser exports so downstream code can run sync lookups without
 * re-importing `wasm-loader`. Useful in contexts that already await init
 * at startup.
 */
export function getLuauParserModule() {
  return getLuauParserSync();
}
