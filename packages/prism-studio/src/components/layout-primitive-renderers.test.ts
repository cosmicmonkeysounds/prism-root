/**
 * Pure-function tests for layout primitive helpers.
 */

import { describe, it, expect } from "vitest";
import { clampColumns } from "./layout-primitive-renderers.js";

describe("clampColumns", () => {
  it("defaults to 2 for non-numeric input", () => {
    expect(clampColumns(undefined)).toBe(2);
    expect(clampColumns(null)).toBe(2);
    expect(clampColumns(NaN)).toBe(2);
    expect(clampColumns("abc")).toBe(2);
  });

  it("accepts numbers in range", () => {
    expect(clampColumns(1)).toBe(1);
    expect(clampColumns(3)).toBe(3);
    expect(clampColumns(6)).toBe(6);
  });

  it("clamps to minimum 1", () => {
    expect(clampColumns(0)).toBe(1);
    expect(clampColumns(-5)).toBe(1);
  });

  it("clamps to maximum 6", () => {
    expect(clampColumns(7)).toBe(6);
    expect(clampColumns(100)).toBe(6);
  });

  it("coerces numeric strings", () => {
    expect(clampColumns("4")).toBe(4);
  });

  it("floors fractional values", () => {
    expect(clampColumns(2.9)).toBe(2);
    expect(clampColumns(3.1)).toBe(3);
  });
});
