//! `@loom/core` — native TypeScript port of the Loom engine.
//!
//! Re-exports the parser + LSP today; the runtime and doc surfaces are
//! added as they land (see the per-subpath package exports).

export * as parser from "./parser/index.ts";
export * as lsp from "./lsp/index.ts";
export { parse } from "./parser/parser.ts";
