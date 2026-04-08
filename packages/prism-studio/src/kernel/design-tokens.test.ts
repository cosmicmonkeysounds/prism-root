/**
 * Tests for design-tokens pure helpers + registry.
 */

import { describe, it, expect } from "vitest";
import {
  DEFAULT_TOKENS,
  tokensToCss,
  lookupToken,
  mergeTokens,
  createDesignTokenRegistry,
} from "./design-tokens.js";

describe("tokensToCss", () => {
  it("emits :root with custom properties from every bucket", () => {
    const css = tokensToCss(DEFAULT_TOKENS);
    expect(css.startsWith(":root {")).toBe(true);
    expect(css).toContain("--color-primary: #3b82f6");
    expect(css).toContain("--space-md: 16px");
    expect(css).toContain("--font-sans:");
  });
});

describe("lookupToken", () => {
  it("resolves dotted paths", () => {
    expect(lookupToken(DEFAULT_TOKENS, "colors.primary")).toBe("#3b82f6");
    expect(lookupToken(DEFAULT_TOKENS, "spacing.md")).toBe(16);
  });
  it("returns undefined on miss", () => {
    expect(lookupToken(DEFAULT_TOKENS, "colors.nope")).toBeUndefined();
    expect(lookupToken(DEFAULT_TOKENS, "bogus")).toBeUndefined();
  });
});

describe("mergeTokens", () => {
  it("shallow merges each bucket", () => {
    const merged = mergeTokens(DEFAULT_TOKENS, {
      colors: { primary: "#000" },
      spacing: { md: 99 },
    });
    expect(merged.colors.primary).toBe("#000");
    expect(merged.colors.secondary).toBe(DEFAULT_TOKENS.colors.secondary);
    expect(merged.spacing.md).toBe(99);
  });
});

describe("DesignTokenRegistry", () => {
  it("notifies subscribers on set and patch", () => {
    const r = createDesignTokenRegistry();
    let calls = 0;
    const unsub = r.subscribe(() => {
      calls++;
    });
    r.patch({ colors: { primary: "#111" } });
    r.patch({ spacing: { md: 12 } });
    expect(calls).toBe(2);
    expect(r.get().colors.primary).toBe("#111");
    expect(r.get().spacing.md).toBe(12);
    unsub();
    r.patch({ colors: { primary: "#222" } });
    expect(calls).toBe(2);
  });
});
