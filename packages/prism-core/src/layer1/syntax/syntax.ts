/**
 * @prism/core — Syntax Engine (Layer 1)
 *
 * LSP-like intelligence for the expression and scripting layers.
 *
 * Features:
 *   - Expression diagnostics with source positions
 *   - Schema-aware type checking (field types → expression types)
 *   - Completions: fields, functions, keywords, operators
 *   - Hover info: field type/description, function signatures
 *   - .d.luau type definition generation from ObjectRegistry schemas
 */

import type { ExprType, AnyExprNode, ExprError } from "../expression/index.js";
import { tokenize, parse } from "../expression/index.js";
import type { Token } from "../expression/index.js";
import type { EntityFieldDef, EntityFieldType } from "../object-model/index.js";
import type {
  Diagnostic,
  TextRange,
  CompletionItem,
  CompletionKind,
  HoverInfo,
  SchemaContext,
  LuauTypeDef,
  FunctionSignature,
  SyntaxProvider,
  SyntaxEngine,
  SyntaxEngineOptions,
  FieldTypeMapping,
} from "./syntax-types.js";

// ── Constants ──────────────────────────────────────────────────────────────

/** Maps EntityFieldType to the expression type system. */
export const FIELD_TYPE_MAP: FieldTypeMapping = {
  bool: "boolean",
  int: "number",
  float: "number",
  string: "string",
  text: "string",
  color: "string",
  enum: "string",
  object_ref: "string",
  date: "string",
  datetime: "string",
  url: "string",
  // Computed field types — runtime resolves to underlying value type; default unknown.
  lookup: "unknown",
  rollup: "number",
};

/** Maps EntityFieldType to Luau type names. */
const LUAU_TYPE_MAP: Record<EntityFieldType, string> = {
  bool: "boolean",
  int: "integer",
  float: "number",
  string: "string",
  text: "string",
  color: "string",
  enum: "string",
  object_ref: "string",
  date: "string",
  datetime: "string",
  url: "string",
  lookup: "any",
  rollup: "number",
};

/** Built-in function signatures. */
export const BUILTIN_FUNCTIONS: FunctionSignature[] = [
  { name: "abs", params: [{ name: "x", type: "number" }], returnType: "number", description: "Absolute value" },
  { name: "ceil", params: [{ name: "x", type: "number" }], returnType: "number", description: "Round up to nearest integer" },
  { name: "floor", params: [{ name: "x", type: "number" }], returnType: "number", description: "Round down to nearest integer" },
  { name: "round", params: [{ name: "x", type: "number" }], returnType: "number", description: "Round to nearest integer" },
  { name: "sqrt", params: [{ name: "x", type: "number" }], returnType: "number", description: "Square root" },
  { name: "pow", params: [{ name: "base", type: "number" }, { name: "exp", type: "number" }], returnType: "number", description: "Raise base to exponent" },
  { name: "min", params: [{ name: "a", type: "number" }, { name: "b", type: "number" }], returnType: "number", description: "Minimum of two values" },
  { name: "max", params: [{ name: "a", type: "number" }, { name: "b", type: "number" }], returnType: "number", description: "Maximum of two values" },
  { name: "clamp", params: [{ name: "value", type: "number" }, { name: "low", type: "number" }, { name: "high", type: "number" }], returnType: "number", description: "Clamp value between low and high" },

  // String
  { name: "len", params: [{ name: "s", type: "string" }], returnType: "number", description: "Length of a string" },
  { name: "lower", params: [{ name: "s", type: "string" }], returnType: "string", description: "Lowercase a string" },
  { name: "upper", params: [{ name: "s", type: "string" }], returnType: "string", description: "Uppercase a string" },
  { name: "trim", params: [{ name: "s", type: "string" }], returnType: "string", description: "Trim leading/trailing whitespace" },
  { name: "concat", params: [{ name: "...parts", type: "string" }], returnType: "string", description: "Concatenate strings" },
  { name: "left", params: [{ name: "s", type: "string" }, { name: "n", type: "number" }], returnType: "string", description: "Leftmost n characters" },
  { name: "right", params: [{ name: "s", type: "string" }, { name: "n", type: "number" }], returnType: "string", description: "Rightmost n characters" },
  { name: "mid", params: [{ name: "s", type: "string" }, { name: "start", type: "number" }, { name: "len", type: "number" }], returnType: "string", description: "Substring from start of length len" },
  { name: "substitute", params: [{ name: "s", type: "string" }, { name: "old", type: "string" }, { name: "new", type: "string" }], returnType: "string", description: "Replace all occurrences of old with new" },

  // Date
  { name: "today", params: [], returnType: "string", description: "Today's date as YYYY-MM-DD" },
  { name: "now", params: [], returnType: "string", description: "Current ISO timestamp" },
  { name: "year", params: [{ name: "iso", type: "string" }], returnType: "number", description: "Year component of an ISO date" },
  { name: "month", params: [{ name: "iso", type: "string" }], returnType: "number", description: "Month component (1-12)" },
  { name: "day", params: [{ name: "iso", type: "string" }], returnType: "number", description: "Day of month (1-31)" },
  { name: "datediff", params: [{ name: "a", type: "string" }, { name: "b", type: "string" }, { name: "unit", type: "string" }], returnType: "number", description: "Difference between two dates (days/months/years)" },

  // Aggregate (variadic over literal arg lists)
  { name: "sum", params: [{ name: "...values", type: "number" }], returnType: "number", description: "Sum of values" },
  { name: "avg", params: [{ name: "...values", type: "number" }], returnType: "number", description: "Average of values" },
  { name: "count", params: [{ name: "...values", type: "unknown" }], returnType: "number", description: "Count of arguments" },
];

const BUILTIN_FN_MAP = new Map(BUILTIN_FUNCTIONS.map(f => [f.name, f]));

/** Keywords that can appear in expressions. */
const KEYWORDS = ["true", "false", "and", "or", "not"];

/** Operators for completions. */
const OPERATOR_COMPLETIONS: CompletionItem[] = [
  { label: "+", kind: "operator", detail: "Addition / string concatenation" },
  { label: "-", kind: "operator", detail: "Subtraction" },
  { label: "*", kind: "operator", detail: "Multiplication" },
  { label: "/", kind: "operator", detail: "Division" },
  { label: "^", kind: "operator", detail: "Exponentiation" },
  { label: "%", kind: "operator", detail: "Modulo" },
  { label: "==", kind: "operator", detail: "Equal" },
  { label: "!=", kind: "operator", detail: "Not equal" },
  { label: "<", kind: "operator", detail: "Less than" },
  { label: "<=", kind: "operator", detail: "Less than or equal" },
  { label: ">", kind: "operator", detail: "Greater than" },
  { label: ">=", kind: "operator", detail: "Greater than or equal" },
  { label: "and", kind: "operator", detail: "Logical AND" },
  { label: "or", kind: "operator", detail: "Logical OR" },
];

// ── Helpers ────────────────────────────────────────────────────────────────

function exprErrorToDiagnostic(err: ExprError, source: string): Diagnostic {
  const start = err.offset ?? 0;
  const end = Math.min(start + 1, source.length);
  return {
    message: err.message,
    severity: "error",
    range: { start, end },
    code: "parse-error",
  };
}

function fieldToExprType(fieldType: EntityFieldType): ExprType {
  return FIELD_TYPE_MAP[fieldType] ?? "unknown";
}

function findFieldDef(
  fieldId: string,
  context?: SchemaContext,
): EntityFieldDef | undefined {
  if (!context) return undefined;
  return context.fields.find(f => f.id === fieldId);
}

function tokenAtOffset(tokens: Token[], offset: number): Token | undefined {
  for (const t of tokens) {
    if (t.kind === "EOF") continue;
    if (offset >= t.offset && offset < t.offset + t.raw.length) {
      return t;
    }
  }
  return undefined;
}

/**
 * Find the token the cursor is touching (inside or immediately after).
 * Used for completions where the cursor may be at the end of a partial identifier.
 */
function tokenTouchingOffset(tokens: Token[], offset: number): Token | undefined {
  // First try exact match
  const exact = tokenAtOffset(tokens, offset);
  if (exact) return exact;
  // If cursor is right after a token, return that token
  for (const t of tokens) {
    if (t.kind === "EOF") continue;
    if (offset === t.offset + t.raw.length) {
      return t;
    }
  }
  return undefined;
}

function tokenBeforeOffset(tokens: Token[], offset: number): Token | undefined {
  let prev: Token | undefined;
  for (const t of tokens) {
    if (t.kind === "EOF") break;
    if (t.offset + t.raw.length > offset) break;
    prev = t;
  }
  return prev;
}

// ── Type Inference ─────────────────────────────────────────────────────────

export function inferNodeType(
  node: AnyExprNode,
  context?: SchemaContext,
): ExprType {
  switch (node.kind) {
    case "literal":
      return node.exprType;

    case "operand": {
      const field = findFieldDef(node.id, context);
      if (field) return fieldToExprType(field.type);
      if (node.subfield) return "unknown";
      return "unknown";
    }

    case "unary":
      if (node.op === "not") return "boolean";
      return "number"; // negation

    case "binary": {
      const op = node.op;
      // Comparison operators always return boolean
      if (op === "==" || op === "!=" || op === "<" || op === "<=" || op === ">" || op === ">=" || op === "and" || op === "or") {
        return "boolean";
      }
      // String concatenation
      if (op === "+") {
        const lt = inferNodeType(node.left, context);
        const rt = inferNodeType(node.right, context);
        if (lt === "string" || rt === "string") return "string";
      }
      return "number";
    }

    case "call": {
      const sig = BUILTIN_FN_MAP.get(node.name.toLowerCase());
      if (sig) return sig.returnType;
      return "unknown";
    }
  }
}

// ── Type Validation ────────────────────────────────────────────────────────

function validateNodeTypes(
  node: AnyExprNode,
  context: SchemaContext,
  diagnostics: Diagnostic[],
  source: string,
): void {
  switch (node.kind) {
    case "operand": {
      const field = findFieldDef(node.id, context);
      if (!field) {
        // Find the operand in source for position
        const idx = source.indexOf(`[field:${node.id}]`);
        const bareIdx = idx < 0 ? source.indexOf(node.id) : idx;
        const start = bareIdx >= 0 ? bareIdx : 0;
        const end = start + (idx >= 0 ? `[field:${node.id}]`.length : node.id.length);
        diagnostics.push({
          message: `Unknown field "${node.id}" for type "${context.objectType}"`,
          severity: "warning",
          range: { start, end },
          code: "unknown-field",
        });
      }
      break;
    }

    case "call": {
      const sig = BUILTIN_FN_MAP.get(node.name.toLowerCase());
      if (!sig) {
        const idx = source.toLowerCase().indexOf(node.name.toLowerCase());
        const start = idx >= 0 ? idx : 0;
        diagnostics.push({
          message: `Unknown function "${node.name}"`,
          severity: "error",
          range: { start, end: start + node.name.length },
          code: "unknown-function",
        });
      } else if (node.args.length !== sig.params.length) {
        const idx = source.toLowerCase().indexOf(node.name.toLowerCase());
        const start = idx >= 0 ? idx : 0;
        diagnostics.push({
          message: `Function "${node.name}" expects ${sig.params.length} argument(s), got ${node.args.length}`,
          severity: "error",
          range: { start, end: start + node.name.length },
          code: "wrong-arity",
        });
      }
      // Recurse into args
      for (const arg of node.args) {
        validateNodeTypes(arg, context, diagnostics, source);
      }
      break;
    }

    case "binary": {
      const lt = inferNodeType(node.left, context);
      const rt = inferNodeType(node.right, context);
      const op = node.op;

      // Arithmetic operators need numbers (except + which allows string concat)
      if ((op === "-" || op === "*" || op === "/" || op === "^" || op === "%") &&
          lt !== "unknown" && rt !== "unknown") {
        if (lt !== "number" || rt !== "number") {
          diagnostics.push({
            message: `Operator "${op}" expects number operands, got ${lt} and ${rt}`,
            severity: "warning",
            range: { start: 0, end: source.length },
            code: "type-mismatch",
          });
        }
      }

      validateNodeTypes(node.left, context, diagnostics, source);
      validateNodeTypes(node.right, context, diagnostics, source);
      break;
    }

    case "unary":
      validateNodeTypes(node.operand, context, diagnostics, source);
      break;

    case "literal":
      // Literals are always valid
      break;
  }
}

// ── Expression Syntax Provider ─────────────────────────────────────────────

export function createExpressionProvider(): SyntaxProvider {
  return {
    name: "expression",

    diagnose(source: string, context?: SchemaContext): Diagnostic[] {
      const result = parse(source);
      const diagnostics: Diagnostic[] = [];

      // Parse errors
      for (const err of result.errors) {
        diagnostics.push(exprErrorToDiagnostic(err, source));
      }

      // Type-level validation if we have a schema context and a valid AST
      if (result.node && context) {
        validateNodeTypes(result.node, context, diagnostics, source);
      }

      return diagnostics;
    },

    complete(source: string, offset: number, context?: SchemaContext): CompletionItem[] {
      const tokens = tokenize(source);
      const items: CompletionItem[] = [];

      const tokenAt = tokenTouchingOffset(tokens, offset);
      const tokenBefore = tokenBeforeOffset(tokens, offset);

      // Determine prefix for filtering
      const prefix = tokenAt?.kind === "IDENT"
        ? source.slice(tokenAt.offset, offset).toLowerCase()
        : tokenAt?.kind === "OPERAND" && tokenAt.operandData
          ? tokenAt.operandData.id.toLowerCase()
          : "";

      // After an operator or at start, suggest fields + functions + keywords
      const afterOperator = !tokenBefore ||
        tokenBefore.kind === "PLUS" || tokenBefore.kind === "MINUS" ||
        tokenBefore.kind === "STAR" || tokenBefore.kind === "SLASH" ||
        tokenBefore.kind === "CARET" || tokenBefore.kind === "PERCENT" ||
        tokenBefore.kind === "EQ" || tokenBefore.kind === "NEQ" ||
        tokenBefore.kind === "LT" || tokenBefore.kind === "LTE" ||
        tokenBefore.kind === "GT" || tokenBefore.kind === "GTE" ||
        tokenBefore.kind === "AND" || tokenBefore.kind === "OR" ||
        tokenBefore.kind === "NOT" || tokenBefore.kind === "LPAREN" ||
        tokenBefore.kind === "COMMA";

      const afterValue = tokenBefore?.kind === "NUMBER" ||
        tokenBefore?.kind === "STRING" ||
        tokenBefore?.kind === "BOOL" ||
        tokenBefore?.kind === "RPAREN" ||
        tokenBefore?.kind === "OPERAND" ||
        tokenBefore?.kind === "IDENT";

      // Field completions
      if (context && (afterOperator || (tokenAt?.kind === "IDENT") || (tokenAt?.kind === "OPERAND"))) {
        for (const field of context.fields) {
          if (prefix && !field.id.toLowerCase().startsWith(prefix)) continue;
          const exprType = fieldToExprType(field.type);
          items.push({
            label: `[field:${field.id}]`,
            kind: "field",
            detail: `${field.type} → ${exprType}`,
            documentation: field.description ?? field.label ?? field.id,
            sortOrder: 1,
            insertText: `[field:${field.id}]`,
          });
        }
      }

      // Function completions
      if (afterOperator || tokenAt?.kind === "IDENT") {
        for (const fn of BUILTIN_FUNCTIONS) {
          if (prefix && !fn.name.startsWith(prefix)) continue;
          const paramStr = fn.params.map(p => `${p.name}: ${p.type}`).join(", ");
          items.push({
            label: fn.name,
            kind: "function",
            detail: `(${paramStr}) → ${fn.returnType}`,
            documentation: fn.description,
            sortOrder: 5,
            insertText: `${fn.name}(`,
          });
        }
      }

      // Keyword completions
      if (afterOperator || afterValue || tokenAt?.kind === "IDENT") {
        for (const kw of KEYWORDS) {
          if (prefix && !kw.startsWith(prefix)) continue;
          const kwKind: CompletionKind = (kw === "and" || kw === "or" || kw === "not")
            ? "keyword"
            : "value";
          items.push({
            label: kw,
            kind: kwKind,
            detail: kw === "true" || kw === "false" ? "boolean literal" : `logical ${kw.toUpperCase()}`,
            sortOrder: kw === "and" || kw === "or" || kw === "not" ? 8 : 6,
          });
        }
      }

      // Operator completions (after a value)
      if (afterValue) {
        for (const op of OPERATOR_COMPLETIONS) {
          items.push({ ...op, sortOrder: 10 });
        }
      }

      return items;
    },

    hover(source: string, offset: number, context?: SchemaContext): HoverInfo | null {
      const tokens = tokenize(source);
      const token = tokenAtOffset(tokens, offset);
      if (!token) return null;

      const range: TextRange = { start: token.offset, end: token.offset + token.raw.length };

      // Hover over operand [field:name]
      if (token.kind === "OPERAND" && token.operandData) {
        const fieldId = token.operandData.id;
        const field = findFieldDef(fieldId, context);
        if (field) {
          const exprType = fieldToExprType(field.type);
          const lines = [
            `**${field.label ?? field.id}** (${field.type} → ${exprType})`,
          ];
          if (field.description) lines.push(field.description);
          if (field.required) lines.push("Required");
          if (field.expression) lines.push(`Computed: \`${field.expression}\``);
          if (field.enumOptions) {
            lines.push(`Values: ${field.enumOptions.map(o => o.value).join(", ")}`);
          }
          return { range, contents: lines.join("\n") };
        }
        return { range, contents: `Field: ${fieldId} (unknown)` };
      }

      // Hover over identifier (bare field name or function)
      if (token.kind === "IDENT") {
        const name = token.raw;
        const lowerName = name.toLowerCase();

        // Check if it's a function call (next token is LPAREN)
        const tokenIdx = tokens.indexOf(token);
        const nextToken = tokenIdx >= 0 ? tokens[tokenIdx + 1] : undefined;
        if (nextToken?.kind === "LPAREN") {
          const sig = BUILTIN_FN_MAP.get(lowerName);
          if (sig) {
            const paramStr = sig.params.map(p => `${p.name}: ${p.type}`).join(", ");
            return {
              range,
              contents: `**${sig.name}**(${paramStr}) → ${sig.returnType}\n${sig.description}`,
            };
          }
          return { range, contents: `Unknown function: ${name}` };
        }

        // Check if it's a bare field reference
        const field = findFieldDef(name, context);
        if (field) {
          const exprType = fieldToExprType(field.type);
          return {
            range,
            contents: `**${field.label ?? field.id}** (${field.type} → ${exprType})${field.description ? "\n" + field.description : ""}`,
          };
        }

        // Could be a keyword
        if (lowerName === "true" || lowerName === "false") {
          return { range, contents: `Boolean literal: ${lowerName}` };
        }
        if (lowerName === "and" || lowerName === "or" || lowerName === "not") {
          return { range, contents: `Logical operator: ${lowerName.toUpperCase()}` };
        }

        return null;
      }

      // Hover over number
      if (token.kind === "NUMBER") {
        return { range, contents: `Number literal: ${token.numberValue}` };
      }

      // Hover over string
      if (token.kind === "STRING") {
        return { range, contents: `String literal: ${token.raw}` };
      }

      // Hover over boolean
      if (token.kind === "BOOL") {
        return { range, contents: `Boolean literal: ${token.boolValue}` };
      }

      // Hover over keyword operators (and, or, not)
      if (token.kind === "AND") {
        return { range, contents: "Logical operator: AND\nShort-circuit: returns left if falsy, else right" };
      }
      if (token.kind === "OR") {
        return { range, contents: "Logical operator: OR\nShort-circuit: returns left if truthy, else right" };
      }
      if (token.kind === "NOT") {
        return { range, contents: "Logical operator: NOT\nNegates a boolean value" };
      }

      return null;
    },
  };
}

// ── Luau Type Definition Generator ──────────────────────────────────────────

export function generateLuauTypeDef(context: SchemaContext): LuauTypeDef {
  const lines: string[] = [
    `--- Type definitions for ${context.objectType}`,
    `--- Generated by Prism Syntax Engine`,
    ``,
    `---@class ${context.objectType}`,
  ];

  for (const field of context.fields) {
    const luaType = LUAU_TYPE_MAP[field.type] ?? "any";
    const desc = field.description ?? field.label ?? field.id;
    if (field.enumOptions) {
      const values = field.enumOptions.map(o => `"${o.value}"`).join("|");
      lines.push(`---@field ${field.id} ${values} ${desc}`);
    } else {
      const optional = field.required ? "" : "?";
      lines.push(`---@field ${field.id}${optional} ${luaType} ${desc}`);
    }
  }

  lines.push("");

  // Standard GraphObject fields
  lines.push(`--- Standard GraphObject fields available on all objects`);
  lines.push(`---@field id string Unique object ID`);
  lines.push(`---@field type string Object type name`);
  lines.push(`---@field name string Display name`);
  lines.push(`---@field parentId string|nil Parent object ID`);
  lines.push(`---@field status string|nil Current status`);
  lines.push(`---@field tags string[] Tags`);
  lines.push(`---@field description string Description text`);
  lines.push(`---@field createdAt string ISO-8601 creation timestamp`);
  lines.push(`---@field updatedAt string ISO-8601 last update timestamp`);
  lines.push("");

  // Builtin functions
  lines.push(`--- Built-in expression functions`);
  for (const fn of BUILTIN_FUNCTIONS) {
    const paramStr = fn.params.map(p => `---@param ${p.name} number`).join("\n");
    lines.push(paramStr);
    lines.push(`---@return number`);
    lines.push(`function ${fn.name}(${fn.params.map(p => p.name).join(", ")}) end`);
    lines.push("");
  }

  return {
    objectType: context.objectType,
    content: lines.join("\n"),
  };
}

// ── Syntax Engine ──────────────────────────────────────────────────────────

export function createSyntaxEngine(
  options: SyntaxEngineOptions = {},
): SyntaxEngine {
  const providers = new Map<string, SyntaxProvider>();

  // Register built-in expression provider
  const exprProvider = createExpressionProvider();
  providers.set(exprProvider.name, exprProvider);

  // Register custom providers
  if (options.providers) {
    for (const p of options.providers) {
      providers.set(p.name, p);
    }
  }

  return {
    getProvider(name: string): SyntaxProvider | undefined {
      return providers.get(name);
    },

    listProviders(): string[] {
      return [...providers.keys()];
    },

    registerProvider(provider: SyntaxProvider): void {
      providers.set(provider.name, provider);
    },

    diagnose(source: string, context?: SchemaContext): Diagnostic[] {
      const expr = providers.get("expression");
      if (!expr) return [];
      return expr.diagnose(source, context);
    },

    complete(source: string, offset: number, context?: SchemaContext): CompletionItem[] {
      const expr = providers.get("expression");
      if (!expr) return [];
      return expr.complete(source, offset, context);
    },

    hover(source: string, offset: number, context?: SchemaContext): HoverInfo | null {
      const expr = providers.get("expression");
      if (!expr) return null;
      return expr.hover(source, offset, context);
    },

    inferType(node: AnyExprNode, context?: SchemaContext): ExprType {
      return inferNodeType(node, context);
    },

    validateTypes(source: string, context: SchemaContext): Diagnostic[] {
      const result = parse(source);
      if (!result.node) return [];
      const diagnostics: Diagnostic[] = [];
      validateNodeTypes(result.node, context, diagnostics, source);
      return diagnostics;
    },

    generateLuauTypeDef(context: SchemaContext): LuauTypeDef {
      return generateLuauTypeDef(context);
    },

    generateLuauTypeDefs(contexts: SchemaContext[]): LuauTypeDef[] {
      return contexts.map(c => generateLuauTypeDef(c));
    },
  };
}
