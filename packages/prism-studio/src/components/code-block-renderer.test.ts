/**
 * Pure-function tests for code-block helpers.
 */

import { describe, it, expect } from "vitest";
import { splitCodeLines, gutterWidth } from "./code-block-renderer.js";

describe("splitCodeLines", () => {
  it("returns a single empty line for an empty string", () => {
    expect(splitCodeLines("")).toEqual([""]);
  });

  it("splits on \\n", () => {
    expect(splitCodeLines("a\nb\nc")).toEqual(["a", "b", "c"]);
  });

  it("splits on \\r\\n", () => {
    expect(splitCodeLines("a\r\nb\r\nc")).toEqual(["a", "b", "c"]);
  });

  it("preserves trailing newline as an empty final line", () => {
    expect(splitCodeLines("a\nb\n")).toEqual(["a", "b", ""]);
  });
});

describe("gutterWidth", () => {
  it("returns at least 1 for zero/negative", () => {
    expect(gutterWidth(0)).toBe(1);
    expect(gutterWidth(-5)).toBe(1);
  });

  it("returns the digit count of the line number", () => {
    expect(gutterWidth(1)).toBe(1);
    expect(gutterWidth(9)).toBe(1);
    expect(gutterWidth(10)).toBe(2);
    expect(gutterWidth(99)).toBe(2);
    expect(gutterWidth(100)).toBe(3);
    expect(gutterWidth(1234)).toBe(4);
  });
});
