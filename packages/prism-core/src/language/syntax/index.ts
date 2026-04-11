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

// ── Codegen Pipeline ──────────────────────────────────────────────────────
export type {
  EmittedFile,
  CodegenMeta,
  CodegenResult,
  Emitter,
} from "./codegen/index.js";
export { SourceBuilder } from "./codegen/index.js";
export { CodegenPipeline } from "./codegen/index.js";
export type { SymbolKind, SymbolParam, SymbolDef } from "./codegen/index.js";
export { constantNamespace, fnSymbol } from "./codegen/index.js";
export type { NameTransform } from "./codegen/index.js";
export {
  SymbolEmitter,
  SymbolTypeScriptEmitter,
  SymbolCSharpEmitter,
  SymbolEmmyDocEmitter,
  SymbolGDScriptEmitter,
  tsNameTransform,
  csNameTransform,
} from "./codegen/index.js";
export { TextEmitter } from "./codegen/index.js";

// ── Language Registry ─────────────────────────────────────────────────────
export type {
  ProcessorDiagnostic,
  ProcessorContext,
  SyntaxPlugin,
  LanguageDefinition,
  PipelineResult,
  ParseOptions,
} from "./language-registry.js";
export { LanguageRegistry, Processor } from "./language-registry.js";

// ── Document Surface Types ───────────────────────────────────────────────
export type {
  SurfaceMode,
  InlineTokenDef,
  DocumentContributionDef,
  DocumentSurfaceEntry,
} from "./document-types.js";
export {
  DocumentSurfaceRegistry,
  createDocumentSurfaceRegistry,
  InlineTokenBuilder,
  inlineToken,
  DocumentSurfaceBuilder,
  documentSurfaceBuilder,
  MARKDOWN_CONTRIBUTION,
  YAML_CONTRIBUTION,
  JSON_CONTRIBUTION,
  PLAINTEXT_CONTRIBUTION,
  HTML_CONTRIBUTION,
  CSV_CONTRIBUTION,
  SVG_CONTRIBUTION,
  WIKILINK_TOKEN,
} from "./document-types.js";

// ── Spell Check Types ─────────────────────────────────────────────────────
export type {
  TokenContext,
  TokenFilter,
  SpellCheckDiagnostic,
  PersonalDictionary,
  SpellChecker,
} from "./spell-check-types.js";

