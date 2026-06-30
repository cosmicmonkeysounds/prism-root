//! LSP wire shapes the Loom language server returns.
//!
//! The Rust `loom-lsp` crate leans on the `lsp-types` crate and the
//! browser consumed those shapes through `serde_wasm_bindgen`. This
//! port reproduces the *serialized* form of exactly the subset we
//! emit — numeric `kind` / `severity` discriminants, camelCase
//! `selectionRange`, `{ kind: "markdown", value }` hover markup,
//! untagged goto-definition — so `Workspace` is a drop-in replacement
//! for the old wasm `LspWorkspace` with no adapter on the editor side.

/** Zero-based line + UTF-16 character offset (LSP `Position`). */
export interface Position {
  line: number;
  character: number;
}

export interface Range {
  start: Position;
  end: Position;
}

export interface Location {
  uri: string;
  range: Range;
}

/** `CompletionItemKind` — the subset of LSP values we emit. */
export const CompletionItemKind = {
  Function: 3,
  Class: 7,
  Interface: 8,
  Keyword: 14,
} as const;
export type CompletionItemKind =
  (typeof CompletionItemKind)[keyof typeof CompletionItemKind];

export interface CompletionItem {
  label: string;
  kind?: CompletionItemKind;
}

export type MarkupKind = "markdown" | "plaintext";

export interface MarkupContent {
  kind: MarkupKind;
  value: string;
}

export interface Hover {
  contents: MarkupContent;
  range?: Range;
}

/**
 * Goto-definition response — `lsp-types` serializes its untagged enum
 * to either a single `Location` (Scalar) or an array (Array). We mirror
 * both arms.
 */
export type GotoDefinitionResponse = Location | Location[];

/** Full LSP `SymbolKind` numbering (spec). */
export const SymbolKind = {
  File: 1,
  Module: 2,
  Namespace: 3,
  Package: 4,
  Class: 5,
  Method: 6,
  Property: 7,
  Field: 8,
  Constructor: 9,
  Enum: 10,
  Interface: 11,
  Function: 12,
  Variable: 13,
  Constant: 14,
  String: 15,
  Number: 16,
  Boolean: 17,
  Array: 18,
  Object: 19,
  Key: 20,
  Null: 21,
  EnumMember: 22,
  Struct: 23,
  Event: 24,
  Operator: 25,
  TypeParameter: 26,
} as const;
export type SymbolKind = (typeof SymbolKind)[keyof typeof SymbolKind];

export interface DocumentSymbol {
  name: string;
  detail?: string;
  kind: SymbolKind;
  range: Range;
  selectionRange: Range;
  children?: DocumentSymbol[];
}

/** `lsp-types` serializes `DocumentSymbolResponse::Nested` to an array. */
export type DocumentSymbolResponse = DocumentSymbol[];

/** `DiagnosticSeverity` — only Error / Warning surface from the parser. */
export const DiagnosticSeverity = {
  Error: 1,
  Warning: 2,
  Information: 3,
  Hint: 4,
} as const;
export type DiagnosticSeverity =
  (typeof DiagnosticSeverity)[keyof typeof DiagnosticSeverity];

export interface Diagnostic {
  range: Range;
  severity?: DiagnosticSeverity;
  code?: string;
  source?: string;
  message: string;
}

export interface PublishDiagnosticsParams {
  uri: string;
  diagnostics: Diagnostic[];
  version?: number;
}
