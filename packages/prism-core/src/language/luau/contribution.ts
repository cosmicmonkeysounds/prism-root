/**
 * Luau `LanguageContribution` — the unified registration used by
 * `LanguageRegistry.register(createLuauContribution())`.
 *
 * Replaces the old `createLuauLanguageDefinition()` + ad-hoc
 * `DocumentSurfaceRegistry` entry pair that Phase 4 of ADR-002 retired.
 *
 * Because the full-moon parser runs in WASM and needs an async init,
 * callers MUST `await initLuauSyntax()` exactly once before registering
 * the contribution on a kernel registry or invoking `parse()` directly.
 * `parse()` is synchronous at the contribution boundary — errors during
 * WASM init surface as an empty root node with no diagnostics (the old
 * code path reported diagnostics into a `ProcessorContext` which the
 * unified model no longer has).
 */

import type { LanguageContribution } from "@prism/core/language-registry";
import type { RootNode } from "@prism/core/syntax";
import { getLuauParserSync, isLuauParserReady } from "./wasm-loader.js";
import { parseLuauSync } from "./luau-ast.js";
import { createLuauSyntaxProvider } from "./luau-provider.js";

/**
 * Create the unified `LanguageContribution` for Luau.
 *
 * Registers both the parser and the surface on a single record. The
 * surface defaults to `code` mode and additionally exposes `preview` so
 * a Luau debugger panel can open the same buffer in a trace-aware view.
 */
export function createLuauContribution(): LanguageContribution {
  return {
    id: "prism:luau",
    extensions: [".luau", ".lua"],
    displayName: "Luau",
    mimeType: "text/x-luau",

    parse(source: string): RootNode {
      if (!isLuauParserReady()) {
        return { type: "root", children: [] };
      }
      try {
        return parseLuauSync(source);
      } catch {
        return { type: "root", children: [] };
      }
    },

    syntaxProvider() {
      return createLuauSyntaxProvider();
    },

    surface: {
      defaultMode: "code",
      availableModes: ["code", "preview"],
    },
  };
}

/**
 * Drain the parser exports so downstream code can run sync lookups
 * without re-importing `wasm-loader`. Useful in contexts that already
 * await init at startup.
 */
export function getLuauParserModule() {
  return getLuauParserSync();
}
