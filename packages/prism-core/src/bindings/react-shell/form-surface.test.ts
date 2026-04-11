/**
 * Tests for FormSurface helpers — pure logic only (no DOM).
 *
 * FormSurface itself is exercised via Studio integration tests; here we
 * verify the field inference and coercion helpers that drive it.
 */

import { describe, it, expect } from "vitest";
import { inferFieldType, titleCase, deriveFields, coerce } from "./form-surface.js";
import type { FieldSchema } from "@prism/core/forms";

describe("inferFieldType", () => {
  it("returns boolean for booleans", () => {
    expect(inferFieldType(true)).toBe("boolean");
    expect(inferFieldType(false)).toBe("boolean");
  });

  it("returns number for numbers", () => {
    expect(inferFieldType(42)).toBe("number");
    expect(inferFieldType(3.14)).toBe("number");
  });

  it("returns date for ISO date strings", () => {
    expect(inferFieldType("2024-03-15")).toBe("date");
  });

  it("returns datetime for ISO datetime strings", () => {
    expect(inferFieldType("2024-03-15T12:00:00Z")).toBe("datetime");
  });

  it("returns textarea for long strings", () => {
    expect(inferFieldType("x".repeat(100))).toBe("textarea");
  });

  it("returns textarea for multiline strings", () => {
    expect(inferFieldType("hello\nworld")).toBe("textarea");
  });

  it("returns text for short strings", () => {
    expect(inferFieldType("hello")).toBe("text");
  });

  it("falls back to text for unknown types", () => {
    expect(inferFieldType(null)).toBe("text");
    expect(inferFieldType(undefined)).toBe("text");
    expect(inferFieldType({ nested: true })).toBe("text");
  });
});

describe("titleCase", () => {
  it("converts snake_case", () => {
    expect(titleCase("first_name")).toBe("First Name");
  });

  it("converts kebab-case", () => {
    expect(titleCase("first-name")).toBe("First Name");
  });

  it("handles already-cased strings", () => {
    expect(titleCase("name")).toBe("Name");
  });

  it("handles mixed separators", () => {
    expect(titleCase("due_date-iso")).toBe("Due Date Iso");
  });
});

describe("deriveFields", () => {
  it("creates one FieldSchema per key", () => {
    const fields = deriveFields({ name: "Alice", age: 30, active: true });
    expect(fields).toHaveLength(3);
  });

  it("assigns types by value inference", () => {
    const fields = deriveFields({ name: "Alice", age: 30, active: true });
    expect(fields.find((f) => f.id === "name")?.type).toBe("text");
    expect(fields.find((f) => f.id === "age")?.type).toBe("number");
    expect(fields.find((f) => f.id === "active")?.type).toBe("boolean");
  });

  it("title-cases labels from ids", () => {
    const fields = deriveFields({ due_date: "2024-01-01" });
    expect(fields[0]?.label).toBe("Due Date");
    expect(fields[0]?.type).toBe("date");
  });

  it("returns empty array for empty values", () => {
    expect(deriveFields({})).toEqual([]);
  });
});

describe("coerce", () => {
  const textField: FieldSchema = { id: "x", label: "X", type: "text" };
  const numField: FieldSchema = { id: "n", label: "N", type: "number" };
  const boolField: FieldSchema = { id: "b", label: "B", type: "boolean" };
  const currencyField: FieldSchema = { id: "c", label: "C", type: "currency" };

  it("passes text through unchanged", () => {
    expect(coerce(textField, "hello")).toBe("hello");
  });

  it("converts numeric strings to numbers", () => {
    expect(coerce(numField, "42")).toBe(42);
    expect(coerce(numField, "3.14")).toBe(3.14);
  });

  it("returns null for empty number input", () => {
    expect(coerce(numField, "")).toBe(null);
  });

  it("returns raw text for non-numeric number input", () => {
    expect(coerce(numField, "abc")).toBe("abc");
  });

  it("converts 'true'/'false' to booleans", () => {
    expect(coerce(boolField, "true")).toBe(true);
    expect(coerce(boolField, "false")).toBe(false);
  });

  it("coerces currency like number", () => {
    expect(coerce(currencyField, "99.99")).toBe(99.99);
  });
});
