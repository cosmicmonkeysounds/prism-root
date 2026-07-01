//! `@loom/core/lsp` — the Loom language server surface as plain
//! TypeScript, a port of the Rust `loom-lsp` crate.
//!
//! Construct one long-lived [`Workspace`], push document state through
//! `open` / `update` / `close`, and query `completionAt` / `hoverAt` /
//! `definitionAt` / `documentSymbols` / `referencesAt` /
//! `referencesByName` / `diagnosticsFor` synchronously as the user
//! types. This is the in-process replacement for the old wasm
//! `LspWorkspace`: the request methods return the same serialized
//! `lsp-types` shapes the browser already consumes.

export * from "./types.ts";
export {
  Workspace,
  type OpenDoc,
  type Occurrence,
  type Todo,
  type CharacterInfo,
  type TraitInfo,
  type BeatInfo,
} from "./workspace.ts";
export {
  DIRECTIVES,
  completionAt,
  isDivertPosition,
  isDirectivePosition,
  isIsPosition,
  isTraitArgPosition,
  ownerDivertPrefix,
} from "./completion.ts";
export { hoverAt } from "./hover.ts";
export { definitionAt } from "./definition.ts";
export { documentSymbols } from "./symbols.ts";
export { referencesAt } from "./references.ts";
export {
  findTokenSpans,
  lineAt,
  nameRangeInText,
  positionToLsp,
  spanToRange,
  tokenAt,
} from "./util.ts";
