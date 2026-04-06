import { describe, it, expect } from "vitest";
import {
  createSyntaxEngine,
  createExpressionProvider,
  generateLuaTypeDef,
  inferNodeType,
  BUILTIN_FUNCTIONS,
  FIELD_TYPE_MAP,
} from "./syntax.js";
import { parse } from "../expression/index.js";
import type { SchemaContext, SyntaxProvider } from "./syntax-types.js";

// ── Test Helpers ───────────────────────────────────────────────────────────

function taskContext(): SchemaContext {
  return {
    objectType: "Task",
    fields: [
      { id: "priority", type: "int", label: "Priority", description: "Task priority (1-5)" },
      { id: "estimate", type: "float", label: "Estimate", description: "Hours estimate" },
      { id: "status", type: "enum", label: "Status", enumOptions: [
        { value: "todo", label: "To Do" },
        { value: "doing", label: "In Progress" },
        { value: "done", label: "Done" },
      ]},
      { id: "title", type: "string", label: "Title" },
      { id: "is_blocked", type: "bool", label: "Blocked", required: true },
      { id: "due_date", type: "date", label: "Due Date" },
      { id: "assignee", type: "object_ref", label: "Assignee", refTypes: ["User"] },
      { id: "computed_cost", type: "float", label: "Cost", expression: "[field:estimate] * 100" },
    ],
  };
}

// ── Expression Provider: Diagnostics ───────────────────────────────────────

describe("ExpressionProvider diagnostics", () => {
  const provider = createExpressionProvider();

  it("returns no diagnostics for valid expression", () => {
    const diags = provider.diagnose("[field:priority] + 1");
    expect(diags).toHaveLength(0);
  });

  it("reports parse errors", () => {
    const diags = provider.diagnose("1 + + +");
    expect(diags.length).toBeGreaterThan(0);
    expect(diags[0].severity).toBe("error");
    expect(diags[0].code).toBe("parse-error");
  });

  it("reports unknown fields with schema context", () => {
    const diags = provider.diagnose("[field:nonexistent] + 1", taskContext());
    const unknownField = diags.find(d => d.code === "unknown-field");
    expect(unknownField).toBeDefined();
    expect(unknownField?.message).toContain("nonexistent");
    expect(unknownField?.severity).toBe("warning");
  });

  it("does not report known fields", () => {
    const diags = provider.diagnose("[field:priority] + [field:estimate]", taskContext());
    const unknownField = diags.find(d => d.code === "unknown-field");
    expect(unknownField).toBeUndefined();
  });

  it("reports unknown functions", () => {
    const diags = provider.diagnose("bogus(1)", taskContext());
    const unknownFn = diags.find(d => d.code === "unknown-function");
    expect(unknownFn).toBeDefined();
    expect(unknownFn?.message).toContain("bogus");
  });

  it("reports wrong arity", () => {
    const diags = provider.diagnose("abs(1, 2)", taskContext());
    const wrongArity = diags.find(d => d.code === "wrong-arity");
    expect(wrongArity).toBeDefined();
    expect(wrongArity?.message).toContain("1 argument");
  });

  it("reports type mismatches for arithmetic on strings", () => {
    const diags = provider.diagnose("[field:title] - [field:priority]", taskContext());
    const mismatch = diags.find(d => d.code === "type-mismatch");
    expect(mismatch).toBeDefined();
    expect(mismatch?.message).toContain("string");
  });

  it("allows string + string concatenation without type mismatch", () => {
    // + is the only operator that allows string concat
    const diags = provider.diagnose("[field:title] + [field:status]", taskContext());
    const mismatch = diags.find(d => d.code === "type-mismatch");
    expect(mismatch).toBeUndefined();
  });

  it("validates nested function args", () => {
    const diags = provider.diagnose("abs([field:nonexistent])", taskContext());
    const unknownField = diags.find(d => d.code === "unknown-field");
    expect(unknownField).toBeDefined();
  });
});

// ── Expression Provider: Completions ───────────────────────────────────────

describe("ExpressionProvider completions", () => {
  const provider = createExpressionProvider();
  const ctx = taskContext();

  it("suggests fields at start of expression", () => {
    const items = provider.complete("", 0, ctx);
    const fieldItems = items.filter(i => i.kind === "field");
    expect(fieldItems.length).toBe(ctx.fields.length);
    expect(fieldItems[0].insertText).toContain("[field:");
  });

  it("suggests functions at start of expression", () => {
    const items = provider.complete("", 0, ctx);
    const fnItems = items.filter(i => i.kind === "function");
    expect(fnItems.length).toBe(BUILTIN_FUNCTIONS.length);
    expect(fnItems[0].insertText).toContain("(");
  });

  it("suggests keywords at start", () => {
    const items = provider.complete("", 0, ctx);
    const kwItems = items.filter(i => i.kind === "keyword" || i.kind === "value");
    expect(kwItems.length).toBe(5); // true, false, and, or, not
  });

  it("filters completions by prefix", () => {
    // Typing "ab" should match "abs"
    const items = provider.complete("ab", 2, ctx);
    const fnItems = items.filter(i => i.kind === "function");
    expect(fnItems.some(i => i.label === "abs")).toBe(true);
    expect(fnItems.some(i => i.label === "ceil")).toBe(false);
  });

  it("suggests operators after a value", () => {
    const items = provider.complete("[field:priority] ", 17, ctx);
    const opItems = items.filter(i => i.kind === "operator");
    expect(opItems.length).toBeGreaterThan(0);
    expect(opItems.some(i => i.label === "+")).toBe(true);
  });

  it("suggests fields after an operator", () => {
    const items = provider.complete("[field:priority] + ", 19, ctx);
    const fieldItems = items.filter(i => i.kind === "field");
    expect(fieldItems.length).toBe(ctx.fields.length);
  });

  it("field completions include type detail", () => {
    const items = provider.complete("", 0, ctx);
    const priorityItem = items.find(i => i.label === "[field:priority]");
    expect(priorityItem).toBeDefined();
    expect(priorityItem?.detail).toContain("int");
    expect(priorityItem?.detail).toContain("number");
  });

  it("function completions include parameter info", () => {
    const items = provider.complete("", 0, ctx);
    const clampItem = items.find(i => i.label === "clamp");
    expect(clampItem).toBeDefined();
    expect(clampItem?.detail).toContain("value");
    expect(clampItem?.detail).toContain("low");
    expect(clampItem?.detail).toContain("high");
  });

  it("returns empty without context fields", () => {
    const items = provider.complete("", 0);
    const fieldItems = items.filter(i => i.kind === "field");
    expect(fieldItems).toHaveLength(0);
  });
});

// ── Expression Provider: Hover ─────────────────────────────────────────────

describe("ExpressionProvider hover", () => {
  const provider = createExpressionProvider();
  const ctx = taskContext();

  it("shows field info on operand hover", () => {
    const source = "[field:priority] + 1";
    const hover = provider.hover(source, 3, ctx);
    expect(hover).not.toBeNull();
    expect(hover?.contents).toContain("Priority");
    expect(hover?.contents).toContain("int");
    expect(hover?.contents).toContain("number");
    expect(hover?.contents).toContain("Task priority");
  });

  it("shows enum values on enum field hover", () => {
    const source = "[field:status]";
    const hover = provider.hover(source, 3, ctx);
    expect(hover).not.toBeNull();
    expect(hover?.contents).toContain("todo");
    expect(hover?.contents).toContain("doing");
    expect(hover?.contents).toContain("done");
  });

  it("shows computed field expression", () => {
    const source = "[field:computed_cost]";
    const hover = provider.hover(source, 3, ctx);
    expect(hover).not.toBeNull();
    expect(hover?.contents).toContain("Computed");
    expect(hover?.contents).toContain("[field:estimate] * 100");
  });

  it("shows function signature on function hover", () => {
    const source = "abs(-5)";
    const hover = provider.hover(source, 1, ctx);
    expect(hover).not.toBeNull();
    expect(hover?.contents).toContain("abs");
    expect(hover?.contents).toContain("number");
    expect(hover?.contents).toContain("Absolute value");
  });

  it("shows unknown for unresolved identifier", () => {
    const source = "[field:bogus]";
    const hover = provider.hover(source, 3, ctx);
    expect(hover).not.toBeNull();
    expect(hover?.contents).toContain("unknown");
  });

  it("shows number literal info", () => {
    const source = "42 + 1";
    const hover = provider.hover(source, 0, ctx);
    expect(hover).not.toBeNull();
    expect(hover?.contents).toContain("42");
  });

  it("shows string literal info", () => {
    const source = `"hello" + "world"`;
    const hover = provider.hover(source, 1, ctx);
    expect(hover).not.toBeNull();
    expect(hover?.contents).toContain("String literal");
  });

  it("shows boolean literal info", () => {
    const source = "true and false";
    const hover = provider.hover(source, 1, ctx);
    expect(hover).not.toBeNull();
    expect(hover?.contents).toContain("Boolean literal");
  });

  it("returns null for whitespace", () => {
    const source = "1 + 2";
    const hover = provider.hover(source, 1, ctx);
    expect(hover).toBeNull();
  });

  it("shows keyword hover for and/or/not", () => {
    const source = "true and false";
    const hover = provider.hover(source, 6, ctx);
    expect(hover).not.toBeNull();
    expect(hover?.contents).toContain("AND");
  });
});

// ── Type Inference ─────────────────────────────────────────────────────────

describe("Type inference", () => {
  const ctx = taskContext();

  function parseNode(source: string) {
    const result = parse(source);
    expect(result.node).not.toBeNull();
    return result.node as NonNullable<typeof result.node>;
  }

  it("infers literal types", () => {
    expect(inferNodeType(parseNode("42"), ctx)).toBe("number");
  });

  it("infers boolean literal", () => {
    expect(inferNodeType(parseNode("true"), ctx)).toBe("boolean");
  });

  it("infers string literal", () => {
    expect(inferNodeType(parseNode(`"hello"`), ctx)).toBe("string");
  });

  it("infers field type from schema", () => {
    expect(inferNodeType(parseNode("[field:priority]"), ctx)).toBe("number");
  });

  it("infers string field type", () => {
    expect(inferNodeType(parseNode("[field:title]"), ctx)).toBe("string");
  });

  it("infers boolean field type", () => {
    expect(inferNodeType(parseNode("[field:is_blocked]"), ctx)).toBe("boolean");
  });

  it("infers unknown for missing field", () => {
    expect(inferNodeType(parseNode("[field:missing]"), ctx)).toBe("unknown");
  });

  it("infers comparison result as boolean", () => {
    expect(inferNodeType(parseNode("[field:priority] > 3"), ctx)).toBe("boolean");
  });

  it("infers arithmetic result as number", () => {
    expect(inferNodeType(parseNode("[field:priority] * 2"), ctx)).toBe("number");
  });

  it("infers string concatenation", () => {
    expect(inferNodeType(parseNode(`[field:title] + " suffix"`), ctx)).toBe("string");
  });

  it("infers unary not as boolean", () => {
    expect(inferNodeType(parseNode("not true"), ctx)).toBe("boolean");
  });

  it("infers unary negation as number", () => {
    expect(inferNodeType(parseNode("-42"), ctx)).toBe("number");
  });

  it("infers builtin function return type", () => {
    expect(inferNodeType(parseNode("abs(-5)"), ctx)).toBe("number");
  });

  it("infers unknown for unknown function", () => {
    expect(inferNodeType(parseNode("bogus(1)"), ctx)).toBe("unknown");
  });

  it("infers and/or as boolean", () => {
    expect(inferNodeType(parseNode("true and false"), ctx)).toBe("boolean");
  });
});

// ── Syntax Engine ──────────────────────────────────────────────────────────

describe("SyntaxEngine", () => {
  it("creates with built-in expression provider", () => {
    const engine = createSyntaxEngine();
    expect(engine.listProviders()).toContain("expression");
    expect(engine.getProvider("expression")).toBeDefined();
  });

  it("registers custom provider", () => {
    const engine = createSyntaxEngine();
    const custom: SyntaxProvider = {
      name: "custom",
      diagnose() { return []; },
      complete() { return []; },
      hover() { return null; },
    };
    engine.registerProvider(custom);
    expect(engine.listProviders()).toContain("custom");
    expect(engine.getProvider("custom")).toBe(custom);
  });

  it("accepts custom providers via options", () => {
    const custom: SyntaxProvider = {
      name: "lua",
      diagnose() { return []; },
      complete() { return []; },
      hover() { return null; },
    };
    const engine = createSyntaxEngine({ providers: [custom] });
    expect(engine.listProviders()).toContain("lua");
  });

  it("delegates diagnose to expression provider", () => {
    const engine = createSyntaxEngine();
    const diags = engine.diagnose("1 + + +");
    expect(diags.length).toBeGreaterThan(0);
  });

  it("delegates complete to expression provider", () => {
    const engine = createSyntaxEngine();
    const items = engine.complete("", 0, taskContext());
    expect(items.length).toBeGreaterThan(0);
  });

  it("delegates hover to expression provider", () => {
    const engine = createSyntaxEngine();
    const hover = engine.hover("[field:priority]", 3, taskContext());
    expect(hover).not.toBeNull();
  });

  it("inferType delegates to inferNodeType", () => {
    const engine = createSyntaxEngine();
    const result = parse("[field:priority]");
    const node = result.node as NonNullable<typeof result.node>;
    expect(engine.inferType(node, taskContext())).toBe("number");
  });

  it("validateTypes returns type-level diagnostics", () => {
    const engine = createSyntaxEngine();
    const diags = engine.validateTypes("[field:nonexistent]", taskContext());
    expect(diags.some(d => d.code === "unknown-field")).toBe(true);
  });

  it("validateTypes returns empty for valid expression", () => {
    const engine = createSyntaxEngine();
    const diags = engine.validateTypes("[field:priority] + 1", taskContext());
    expect(diags).toHaveLength(0);
  });
});

// ── Lua Type Definition Generation ─────────────────────────────────────────

describe("Lua type definition generation", () => {
  it("generates .d.lua for object type", () => {
    const ctx = taskContext();
    const result = generateLuaTypeDef(ctx);

    expect(result.objectType).toBe("Task");
    expect(result.content).toContain("---@class Task");
    expect(result.content).toContain("---@field priority");
    expect(result.content).toContain("integer");
    expect(result.content).toContain("---@field estimate");
    expect(result.content).toContain("number");
    expect(result.content).toContain("---@field title");
    expect(result.content).toContain("string");
  });

  it("marks optional fields with ?", () => {
    const result = generateLuaTypeDef(taskContext());
    // priority is not required, so should have ?
    expect(result.content).toContain("---@field priority?");
  });

  it("marks required fields without ?", () => {
    const result = generateLuaTypeDef(taskContext());
    // is_blocked is required
    expect(result.content).toMatch(/---@field is_blocked boolean/);
    expect(result.content).not.toMatch(/---@field is_blocked\?/);
  });

  it("generates enum values as union type", () => {
    const result = generateLuaTypeDef(taskContext());
    expect(result.content).toContain(`"todo"|"doing"|"done"`);
  });

  it("includes standard GraphObject fields", () => {
    const result = generateLuaTypeDef(taskContext());
    expect(result.content).toContain("---@field id string");
    expect(result.content).toContain("---@field name string");
    expect(result.content).toContain("---@field type string");
    expect(result.content).toContain("---@field status string|nil");
    expect(result.content).toContain("---@field tags string[]");
  });

  it("includes builtin function stubs", () => {
    const result = generateLuaTypeDef(taskContext());
    expect(result.content).toContain("function abs(x) end");
    expect(result.content).toContain("function clamp(value, low, high) end");
    expect(result.content).toContain("---@return number");
  });

  it("generates multiple type defs via engine", () => {
    const engine = createSyntaxEngine();
    const contexts: SchemaContext[] = [
      taskContext(),
      {
        objectType: "Contact",
        fields: [
          { id: "email", type: "string", label: "Email" },
          { id: "phone", type: "string", label: "Phone" },
        ],
      },
    ];

    const defs = engine.generateLuaTypeDefs(contexts);
    expect(defs).toHaveLength(2);
    expect(defs[0].objectType).toBe("Task");
    expect(defs[1].objectType).toBe("Contact");
    expect(defs[1].content).toContain("---@class Contact");
    expect(defs[1].content).toContain("---@field email");
  });
});

// ── FIELD_TYPE_MAP ─────────────────────────────────────────────────────────

describe("FIELD_TYPE_MAP", () => {
  it("maps numeric types to number", () => {
    expect(FIELD_TYPE_MAP.int).toBe("number");
    expect(FIELD_TYPE_MAP.float).toBe("number");
  });

  it("maps bool to boolean", () => {
    expect(FIELD_TYPE_MAP.bool).toBe("boolean");
  });

  it("maps text types to string", () => {
    expect(FIELD_TYPE_MAP.string).toBe("string");
    expect(FIELD_TYPE_MAP.text).toBe("string");
    expect(FIELD_TYPE_MAP.enum).toBe("string");
    expect(FIELD_TYPE_MAP.date).toBe("string");
    expect(FIELD_TYPE_MAP.url).toBe("string");
  });

  it("covers all EntityFieldTypes", () => {
    const allTypes: string[] = [
      "bool", "int", "float", "string", "text", "color",
      "enum", "object_ref", "date", "datetime", "url",
    ];
    for (const t of allTypes) {
      expect(FIELD_TYPE_MAP[t as keyof typeof FIELD_TYPE_MAP]).toBeDefined();
    }
  });
});

// ── Edge Cases ─────────────────────────────────────────────────────────────

describe("Edge cases", () => {
  it("handles empty source", () => {
    const engine = createSyntaxEngine();
    const diags = engine.diagnose("");
    // Empty expression may or may not be an error depending on parser
    expect(Array.isArray(diags)).toBe(true);
  });

  it("completions at end of source", () => {
    const engine = createSyntaxEngine();
    const source = "[field:priority] + ";
    const items = engine.complete(source, source.length, taskContext());
    expect(items.length).toBeGreaterThan(0);
  });

  it("hover beyond source length returns null", () => {
    const engine = createSyntaxEngine();
    const hover = engine.hover("42", 100, taskContext());
    expect(hover).toBeNull();
  });

  it("validates deeply nested expressions", () => {
    const engine = createSyntaxEngine();
    const diags = engine.diagnose(
      "abs(max([field:priority], [field:estimate])) + clamp([field:priority], 1, 10)",
      taskContext(),
    );
    expect(diags.filter(d => d.severity === "error")).toHaveLength(0);
  });

  it("handles schema context with no fields", () => {
    const engine = createSyntaxEngine();
    const ctx: SchemaContext = { objectType: "Empty", fields: [] };
    const diags = engine.diagnose("[field:anything]", ctx);
    expect(diags.some(d => d.code === "unknown-field")).toBe(true);
  });
});
