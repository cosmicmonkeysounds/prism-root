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
  LuaTypeDef,
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
  generateLuaTypeDef,
  createSyntaxEngine,
} from "./syntax.js";
