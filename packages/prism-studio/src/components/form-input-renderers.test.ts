/**
 * Pure-function tests for form input helpers.
 */

import { describe, it, expect } from "vitest";
import { parseSelectOptions } from "./form-input-renderers.js";

describe("parseSelectOptions", () => {
  it("returns empty array for empty input", () => {
    expect(parseSelectOptions("")).toEqual([]);
    expect(parseSelectOptions("   ")).toEqual([]);
  });

  it("parses plain comma-separated values", () => {
    const out = parseSelectOptions("one,two,three");
    expect(out).toEqual([
      { value: "one", label: "one" },
      { value: "two", label: "two" },
      { value: "three", label: "three" },
    ]);
  });

  it("parses value:label pairs", () => {
    const out = parseSelectOptions("a:Apple, b:Banana , c:Cherry");
    expect(out).toEqual([
      { value: "a", label: "Apple" },
      { value: "b", label: "Banana" },
      { value: "c", label: "Cherry" },
    ]);
  });

  it("parses JSON array of strings", () => {
    const out = parseSelectOptions('["alpha","beta"]');
    expect(out).toEqual([
      { value: "alpha", label: "alpha" },
      { value: "beta", label: "beta" },
    ]);
  });

  it("parses JSON array of objects", () => {
    const out = parseSelectOptions('[{"value":"1","label":"One"},{"value":"2","label":"Two"}]');
    expect(out).toEqual([
      { value: "1", label: "One" },
      { value: "2", label: "Two" },
    ]);
  });

  it("falls back to CSV when JSON is malformed", () => {
    const out = parseSelectOptions("[bad json, another");
    // Falls through to CSV
    expect(out.length).toBeGreaterThan(0);
  });

  it("ignores empty segments", () => {
    const out = parseSelectOptions("a,,b,");
    expect(out).toEqual([
      { value: "a", label: "a" },
      { value: "b", label: "b" },
    ]);
  });
});
