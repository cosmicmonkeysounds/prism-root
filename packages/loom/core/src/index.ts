//! `@loom/core` — native TypeScript port of the Loom engine.
//!
//! Re-exports the parser today; the runtime, LSP, and doc surfaces are
//! added as they land (see the per-subpath package exports).

export * as parser from "./parser/index.ts";
export { parse } from "./parser/parser.ts";
