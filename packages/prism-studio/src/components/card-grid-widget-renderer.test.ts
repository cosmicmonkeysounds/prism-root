import { describe, it, expect } from "vitest";
import { clampColumnWidth } from "./card-grid-widget-renderer.js";

describe("clampColumnWidth", () => {
  it("returns the default for non-numeric input", () => {
    expect(clampColumnWidth(undefined)).toBe(220);
    expect(clampColumnWidth("not a number")).toBe(220);
    expect(clampColumnWidth(NaN)).toBe(220);
  });

  it("passes sane values through", () => {
    expect(clampColumnWidth(150)).toBe(150);
    expect(clampColumnWidth(300)).toBe(300);
  });

  it("clamps below minimum", () => {
    expect(clampColumnWidth(50)).toBe(80);
    expect(clampColumnWidth(0)).toBe(80);
  });

  it("clamps above maximum", () => {
    expect(clampColumnWidth(600)).toBe(480);
    expect(clampColumnWidth(10000)).toBe(480);
  });

  it("rounds fractional input", () => {
    expect(clampColumnWidth(220.7)).toBe(221);
  });

  it("accepts numeric strings", () => {
    expect(clampColumnWidth("180")).toBe(180);
  });
});
