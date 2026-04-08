import { describe, it, expect } from "vitest";
import { parseTabs } from "./tab-container-renderer.js";

describe("parseTabs", () => {
  it("parses comma-separated labels", () => {
    expect(parseTabs("One, Two, Three")).toEqual(["One", "Two", "Three"]);
  });

  it("parses a JSON array", () => {
    expect(parseTabs('["A", "B", "C"]')).toEqual(["A", "B", "C"]);
  });

  it("handles empty input", () => {
    expect(parseTabs("")).toEqual([]);
    expect(parseTabs("   ")).toEqual([]);
  });

  it("drops empty entries from CSV", () => {
    expect(parseTabs("One,,Two,")).toEqual(["One", "Two"]);
  });

  it("falls back to CSV when JSON parse fails", () => {
    expect(parseTabs("[not json")).toEqual(["[not json"]);
  });

  it("coerces non-string JSON array entries", () => {
    expect(parseTabs("[1, 2, 3]")).toEqual(["1", "2", "3"]);
  });
});
