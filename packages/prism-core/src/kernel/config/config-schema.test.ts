import { describe, it, expect } from "vitest";
import {
  validateConfig,
  coerceConfigValue,
  schemaToValidator,
} from "./config-schema.js";
import type { ConfigSchema } from "./config-schema.js";

// ── validateConfig ──────────────────────────────────────────────────────────────

describe("validateConfig", () => {
  // ── string ──────────────────────────────────────────────────────────────

  it("validates string type", () => {
    const r = validateConfig("hello", { type: "string" });
    expect(r.valid).toBe(true);
  });

  it("rejects non-string", () => {
    const r = validateConfig(42, { type: "string" });
    expect(r.valid).toBe(false);
    expect(r.errors[0].message).toContain("Expected string");
  });

  it("validates minLength", () => {
    const r = validateConfig("ab", { type: "string", minLength: 3 });
    expect(r.valid).toBe(false);
  });

  it("validates maxLength", () => {
    const r = validateConfig("abcdef", { type: "string", maxLength: 3 });
    expect(r.valid).toBe(false);
  });

  it("validates pattern", () => {
    const r = validateConfig("abc", { type: "string", pattern: "^\\d+$" });
    expect(r.valid).toBe(false);
  });

  it("validates enum", () => {
    const r = validateConfig("c", { type: "string", enum: ["a", "b"] });
    expect(r.valid).toBe(false);
    expect(r.errors[0].message).toContain("Must be one of");
  });

  it("passes enum when value matches", () => {
    const r = validateConfig("a", { type: "string", enum: ["a", "b"] });
    expect(r.valid).toBe(true);
  });

  // ── number ──────────────────────────────────────────────────────────────

  it("validates number type", () => {
    expect(validateConfig(42, { type: "number" }).valid).toBe(true);
  });

  it("rejects NaN", () => {
    expect(validateConfig(NaN, { type: "number" }).valid).toBe(false);
  });

  it("validates min", () => {
    expect(validateConfig(5, { type: "number", min: 10 }).valid).toBe(false);
  });

  it("validates max", () => {
    expect(validateConfig(15, { type: "number", max: 10 }).valid).toBe(false);
  });

  it("validates integer", () => {
    expect(validateConfig(3.14, { type: "number", integer: true }).valid).toBe(
      false,
    );
    expect(validateConfig(3, { type: "number", integer: true }).valid).toBe(
      true,
    );
  });

  // ── boolean ─────────────────────────────────────────────────────────────

  it("validates boolean type", () => {
    expect(validateConfig(true, { type: "boolean" }).valid).toBe(true);
    expect(validateConfig("true", { type: "boolean" }).valid).toBe(false);
  });

  // ── array ───────────────────────────────────────────────────────────────

  it("validates array type", () => {
    expect(validateConfig([1, 2], { type: "array" }).valid).toBe(true);
    expect(validateConfig("not array", { type: "array" }).valid).toBe(false);
  });

  it("validates array items", () => {
    const schema: ConfigSchema = {
      type: "array",
      items: { type: "number" },
    };
    expect(validateConfig([1, 2, 3], schema).valid).toBe(true);
    expect(validateConfig([1, "two", 3], schema).valid).toBe(false);
  });

  // ── object ──────────────────────────────────────────────────────────────

  it("validates object type", () => {
    expect(validateConfig({}, { type: "object" }).valid).toBe(true);
    expect(validateConfig(null, { type: "object" }).valid).toBe(false);
    expect(validateConfig([], { type: "object" }).valid).toBe(false);
  });

  it("validates required properties", () => {
    const schema: ConfigSchema = {
      type: "object",
      required: ["name"],
    };
    expect(validateConfig({}, schema).valid).toBe(false);
    expect(validateConfig({ name: "test" }, schema).valid).toBe(true);
  });

  it("validates nested property schemas", () => {
    const schema: ConfigSchema = {
      type: "object",
      properties: {
        port: { type: "number", min: 1, max: 65535 },
      },
    };
    expect(validateConfig({ port: 3000 }, schema).valid).toBe(true);
    expect(validateConfig({ port: 0 }, schema).valid).toBe(false);
  });

  it("includes path in error messages", () => {
    const schema: ConfigSchema = {
      type: "object",
      properties: {
        nested: {
          type: "object",
          properties: {
            value: { type: "number" },
          },
        },
      },
    };
    const r = validateConfig({ nested: { value: "str" } }, schema);
    expect(r.valid).toBe(false);
    expect(r.errors[0].path).toBe("nested.value");
  });
});

// ── coerceConfigValue ───────────────────────────────────────────────────────────

describe("coerceConfigValue", () => {
  it("passes string through", () => {
    expect(coerceConfigValue("hello", { type: "string" })).toBe("hello");
  });

  it("coerces number", () => {
    expect(coerceConfigValue("42", { type: "number" })).toBe(42);
  });

  it("throws on non-numeric string for number", () => {
    expect(() => coerceConfigValue("abc", { type: "number" })).toThrow();
  });

  it("coerces boolean", () => {
    expect(coerceConfigValue("true", { type: "boolean" })).toBe(true);
    expect(coerceConfigValue("1", { type: "boolean" })).toBe(true);
    expect(coerceConfigValue("false", { type: "boolean" })).toBe(false);
  });

  it("coerces array from JSON", () => {
    expect(coerceConfigValue("[1,2]", { type: "array" })).toEqual([1, 2]);
  });

  it("throws on invalid JSON for array", () => {
    expect(() => coerceConfigValue("not json", { type: "array" })).toThrow();
  });

  it("coerces object from JSON", () => {
    expect(coerceConfigValue('{"a":1}', { type: "object" })).toEqual({ a: 1 });
  });
});

// ── schemaToValidator ───────────────────────────────────────────────────────────

describe("schemaToValidator", () => {
  it("returns null for valid value", () => {
    const validate = schemaToValidator({ type: "number", min: 0, max: 100 });
    expect(validate(50)).toBeNull();
  });

  it("returns error string for invalid value", () => {
    const validate = schemaToValidator({ type: "number", min: 0, max: 100 });
    const err = validate(200);
    expect(err).toContain("Too large");
  });

  it("joins multiple errors", () => {
    const validate = schemaToValidator<Record<string, unknown>>({
      type: "object",
      required: ["a", "b"],
    });
    const err = validate({});
    expect(err).toContain("a");
    expect(err).toContain("b");
  });
});
