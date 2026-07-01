//! Public surface of the Loom parser — lexer, AST, diagnostics, and the
//! `parse` entry point.

export * from "./source.ts";
export * from "./diagnostics.ts";
export * from "./ast.ts";
export * from "./keywords.ts";
export { parse } from "./parser.ts";
export { scan, scannedLineSpan, type LineKind, type ScannedLine } from "./lexer.ts";
export { strip as stripComments } from "./comments.ts";
export { lower as lowerDeclaration } from "./decl-body.ts";
export {
  type Anchor,
  EditError,
  type EditErrorCode,
  type TextEdit,
  applyBeatProperty,
  applyEdits,
  applyInsertBeat,
  applyMoveBeat,
  applyRemoveBeat,
  insertBeat,
  moveBeat,
  moveBodyItem,
  removeBeat,
  setBeatProperty,
} from "./edit.ts";
