/**
 * Tests for data-binding pure helpers.
 */

import { describe, it, expect } from "vitest";
import type { GraphObject } from "@prism/core/object-model";
import {
  resolveObjectRefs,
  evaluateVisibleWhen,
  readPath,
  formatValue,
} from "./data-binding.js";

function obj(name: string, data: Record<string, unknown> = {}, extra: Partial<GraphObject> = {}): GraphObject {
  return {
    id: `id_${name}` as GraphObject["id"],
    type: "test",
    name,
    parentId: null,
    position: 0,
    status: "draft",
    tags: [],
    data,
    deletedAt: null,
    ...extra,
  } as unknown as GraphObject;
}

describe("readPath", () => {
  it("walks a dotted path", () => {
    expect(readPath({ a: { b: { c: 1 } } }, "a.b.c")).toBe(1);
  });
  it("returns undefined on miss", () => {
    expect(readPath({ a: 1 }, "a.b")).toBeUndefined();
    expect(readPath(null, "a")).toBeUndefined();
  });
});

describe("formatValue", () => {
  it("stringifies primitives", () => {
    expect(formatValue("x")).toBe("x");
    expect(formatValue(42)).toBe("42");
    expect(formatValue(true)).toBe("true");
  });
  it("returns empty string for nullish", () => {
    expect(formatValue(undefined)).toBe("");
    expect(formatValue(null)).toBe("");
  });
  it("json-stringifies objects", () => {
    expect(formatValue({ a: 1 })).toBe('{"a":1}');
  });
});

describe("resolveObjectRefs", () => {
  it("returns empty for nullish source", () => {
    expect(resolveObjectRefs(null, [])).toBe("");
    expect(resolveObjectRefs(undefined, [])).toBe("");
  });

  it("replaces [obj:Name] with the target name", () => {
    const pool = [obj("Home")];
    expect(resolveObjectRefs("Go to [obj:Home]", pool)).toBe("Go to Home");
  });

  it("replaces [obj:Name.data.title] with a dotted data field", () => {
    const pool = [obj("Home", { title: "My Page" })];
    expect(resolveObjectRefs("[obj:Home.title]", pool)).toBe("My Page");
  });

  it("leaves unknown refs in place", () => {
    expect(resolveObjectRefs("[obj:Missing]", [])).toBe("[obj:Missing]");
  });

  it("replaces [self:field] against the self object", () => {
    const self = obj("Card", { label: "Hello" });
    expect(resolveObjectRefs("[self:label]", [], self)).toBe("Hello");
  });

  it("leaves [self:…] when no self is provided", () => {
    expect(resolveObjectRefs("[self:x]", [])).toBe("[self:x]");
  });

  it("reads top-level GraphObject fields", () => {
    const self = obj("Card");
    expect(resolveObjectRefs("[self:name]", [], self)).toBe("Card");
    expect(resolveObjectRefs("[self:status]", [], self)).toBe("draft");
  });

  it("skips deleted objects", () => {
    const pool = [obj("Home", {}, { deletedAt: "2026-01-01T00:00:00Z" as unknown as null })];
    expect(resolveObjectRefs("[obj:Home]", pool)).toBe("[obj:Home]");
  });
});

describe("evaluateVisibleWhen", () => {
  it("returns true when the expression is empty", () => {
    expect(evaluateVisibleWhen(undefined, [])).toBe(true);
    expect(evaluateVisibleWhen("", [])).toBe(true);
  });

  it("handles plain boolean literals", () => {
    expect(evaluateVisibleWhen("true", [])).toBe(true);
    expect(evaluateVisibleWhen("false", [])).toBe(false);
  });

  it("compares numeric literals", () => {
    expect(evaluateVisibleWhen("2 > 1", [])).toBe(true);
    expect(evaluateVisibleWhen("2 < 1", [])).toBe(false);
    expect(evaluateVisibleWhen("3 == 3", [])).toBe(true);
    expect(evaluateVisibleWhen("3 != 3", [])).toBe(false);
    expect(evaluateVisibleWhen("3 >= 3", [])).toBe(true);
    expect(evaluateVisibleWhen("4 <= 3", [])).toBe(false);
  });

  it("resolves object refs before comparing", () => {
    const pool = [obj("X", { count: 5 })];
    expect(evaluateVisibleWhen("[obj:X.count] > 3", pool)).toBe(true);
    expect(evaluateVisibleWhen("[obj:X.count] < 3", pool)).toBe(false);
  });

  it("uses self for [self:field]", () => {
    const self = obj("Card", { n: 10 });
    expect(evaluateVisibleWhen("[self:n] == 10", [], self)).toBe(true);
  });

  it("returns true for unparseable expressions (fail-open)", () => {
    expect(evaluateVisibleWhen("bogus > nope", [])).toBe(true);
  });
});
