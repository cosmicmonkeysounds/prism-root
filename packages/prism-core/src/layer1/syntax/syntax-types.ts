/**
 * @prism/core — Syntax Engine Types (Layer 1)
 *
 * LSP-like intelligence for the expression and scripting layers.
 * Provides diagnostics, completions, hover info, and schema-aware
 * type checking — all as pure functions with no runtime dependencies.
 */

import type { ExprType, AnyExprNode } from "../expression/index.js";
import type { EntityFieldDef, EntityFieldType } from "../object-model/index.js";

// ── Diagnostics ────────────────────────────────────────────────────────────

export type DiagnosticSeverity = "error" | "warning" | "info" | "hint";

export interface TextRange {
  /** Start offset (0-based byte offset in source). */
  start: number;
  /** End offset (exclusive). */
  end: number;
}

export interface Diagnostic {
  /** Human-readable message. */
  message: string;
  /** Severity level. */
  severity: DiagnosticSeverity;
  /** Source range in the expression. */
  range: TextRange;
  /** Diagnostic code for programmatic identification. */
  code?: string;
}

// ── Completions ────────────────────────────────────────────────────────────

export type CompletionKind =
  | "field"
  | "function"
  | "keyword"
  | "operator"
  | "type"
  | "value";

export interface CompletionItem {
  /** The text to insert. */
  label: string;
  /** Kind of completion (for icon/category). */
  kind: CompletionKind;
  /** One-line description. */
  detail?: string;
  /** Longer documentation. */
  documentation?: string;
  /** Sort priority (lower = higher priority). Default: 10. */
  sortOrder?: number;
  /** If the completion replaces text, the range to replace. */
  replaceRange?: TextRange;
  /** Text to insert (if different from label, e.g. with brackets). */
  insertText?: string;
}

// ── Hover ──────────────────────────────────────────────────────────────────

export interface HoverInfo {
  /** The text range this hover applies to. */
  range: TextRange;
  /** Display content (signature, type info, etc.). */
  contents: string;
}

// ── Type Inference ─────────────────────────────────────────────────────────

/**
 * Maps EntityFieldType to the expression type system.
 */
export type FieldTypeMapping = Record<EntityFieldType, ExprType>;

export interface TypeInfo {
  /** Inferred expression type. */
  exprType: ExprType;
  /** Source of this type (e.g. "field:status", "function:abs", "literal"). */
  source: string;
}

// ── Schema Context ─────────────────────────────────────────────────────────

/**
 * Describes the schema context for a specific object type,
 * providing the fields available for expression resolution.
 */
export interface SchemaContext {
  /** The object type name (e.g. "Task", "Contact"). */
  objectType: string;
  /** Available fields for this type. */
  fields: EntityFieldDef[];
}

// ── Lua Type Definitions ───────────────────────────────────────────────────

export interface LuauTypeDef {
  /** The object type this definition describes. */
  objectType: string;
  /** Generated .d.luau content. */
  content: string;
}

// ── Function Signature ─────────────────────────────────────────────────────

export interface FunctionSignature {
  /** Function name. */
  name: string;
  /** Parameter descriptions. */
  params: Array<{ name: string; type: ExprType }>;
  /** Return type. */
  returnType: ExprType;
  /** Short description. */
  description: string;
}

// ── Syntax Provider ────────────────────────────────────────────────────────

/**
 * A SyntaxProvider produces diagnostics, completions, and hover info
 * for a specific language or expression context.
 */
export interface SyntaxProvider {
  /** Provider identifier (e.g. "expression", "lua"). */
  readonly name: string;

  /** Produce diagnostics for the given source. */
  diagnose(source: string, context?: SchemaContext): Diagnostic[];

  /** Produce completions at the given cursor offset. */
  complete(source: string, offset: number, context?: SchemaContext): CompletionItem[];

  /** Produce hover info at the given offset. */
  hover(source: string, offset: number, context?: SchemaContext): HoverInfo | null;
}

// ── Syntax Engine ──────────────────────────────────────────────────────────

export interface SyntaxEngineOptions {
  /** Additional custom providers beyond the built-in ones. */
  providers?: SyntaxProvider[];
}

/**
 * The SyntaxEngine orchestrates multiple SyntaxProviders and provides
 * schema-aware type checking, Lua typedef generation, and a unified
 * API for IDE integration.
 */
export interface SyntaxEngine {
  /** Get a provider by name. */
  getProvider(name: string): SyntaxProvider | undefined;

  /** List all registered provider names. */
  listProviders(): string[];

  /** Register an additional provider. */
  registerProvider(provider: SyntaxProvider): void;

  /** Diagnose an expression with optional schema context. */
  diagnose(source: string, context?: SchemaContext): Diagnostic[];

  /** Get completions for an expression at cursor offset. */
  complete(source: string, offset: number, context?: SchemaContext): CompletionItem[];

  /** Get hover info for an expression at cursor offset. */
  hover(source: string, offset: number, context?: SchemaContext): HoverInfo | null;

  /** Infer the type of an expression AST node given a schema context. */
  inferType(node: AnyExprNode, context?: SchemaContext): ExprType;

  /** Validate an expression against a schema context (type-level checks). */
  validateTypes(source: string, context: SchemaContext): Diagnostic[];

  /** Generate .d.luau type definitions for an object type's fields. */
  generateLuauTypeDef(context: SchemaContext): LuauTypeDef;

  /** Generate .d.luau type definitions for multiple object types. */
  generateLuauTypeDefs(contexts: SchemaContext[]): LuauTypeDef[];
}
