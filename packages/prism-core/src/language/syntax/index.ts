// ── Syntax Engine ──────────────────────────────────────────────────────────
export type {
  DiagnosticSeverity,
  TextRange,
  Diagnostic,
  CompletionKind,
  CompletionItem,
  HoverInfo,
  FieldTypeMapping,
  TypeInfo,
  SchemaContext,
  LuauTypeDef,
  FunctionSignature,
  SyntaxProvider,
  SyntaxEngineOptions,
  SyntaxEngine,
} from "./syntax-types.js";

export {
  FIELD_TYPE_MAP,
  BUILTIN_FUNCTIONS,
  inferNodeType,
  createExpressionProvider,
  generateLuauTypeDef,
  createSyntaxEngine,
} from "./syntax.js";

// ── AST Types ─────────────────────────────────────────────────────────────
export type { Position, SourceRange, SyntaxNode, RootNode } from "./ast-types.js";
export { posAt, range } from "./ast-types.js";

// ── Scanner ───────────────────────────────────────────────────────────────
export type { ScannerState } from "./scanner.js";
export { ScanError, Scanner, isDigit, isIdentStart, isIdentChar } from "./scanner.js";

// ── Token Stream ──────────────────────────────────────────────────────────
export type { BaseToken, Token } from "./token-stream.js";
export { TokenError, TokenStream } from "./token-stream.js";

// ── Case Utils ────────────────────────────────────────────────────────────
export {
  safeIdentifier,
  toCamelCase,
  toPascalCase,
  toScreamingSnake,
  toSnakeCase,
  toCamelIdent,
  toPascalIdent,
  toScreamingSnakeIdent,
} from "./case-utils.js";


// ── Spell Check Types ─────────────────────────────────────────────────────
export type {
  TokenContext,
  TokenFilter,
  SpellCheckDiagnostic,
  PersonalDictionary,
  SpellChecker,
} from "./spell-check-types.js";

