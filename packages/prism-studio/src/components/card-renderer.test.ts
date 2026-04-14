/**
 * Pure-helper tests for card-renderer.
 */

import { describe, it, expect } from "vitest";
import {
  resolveCardVariant,
  resolveCardLayout,
  clampOverlayOpacity,
  buildCardStyles,
} from "./card-renderer.js";

describe("resolveCardVariant", () => {
  it("elevated has a drop shadow by default", () => {
    expect(resolveCardVariant("elevated").shadow).toBeDefined();
  });

  it("outlined has no base shadow but gains one on hover", () => {
    const p = resolveCardVariant("outlined");
    expect(p.shadow).toBeUndefined();
    expect(p.hoverShadow).toBeDefined();
  });

  it("filled has a tinted background", () => {
    expect(resolveCardVariant("filled").background).toBe("#f1f5f9");
  });

  it("ghost has transparent background and no shadow affordance", () => {
    const p = resolveCardVariant("ghost");
    expect(p.background).toBe("transparent");
    expect(p.borderColor).toBe("transparent");
    expect(p.shadow).toBeUndefined();
    expect(p.hoverShadow).toBeUndefined();
  });

  it("defaults to elevated", () => {
    expect(resolveCardVariant(undefined).background).toBe(
      resolveCardVariant("elevated").background,
    );
  });
});

describe("resolveCardLayout", () => {
  it("vertical and overlay lay out as columns", () => {
    expect(resolveCardLayout("vertical").direction).toBe("column");
    expect(resolveCardLayout("overlay").direction).toBe("column");
  });

  it("horizontal lays out as a row with a fixed media basis", () => {
    const tokens = resolveCardLayout("horizontal");
    expect(tokens.direction).toBe("row");
    expect(tokens.mediaBasis).toBe("40%");
  });

  it("overlay gives content more padding than the stacked layouts", () => {
    const overlay = resolveCardLayout("overlay");
    const vertical = resolveCardLayout("vertical");
    expect(overlay.contentPadding).toBeGreaterThan(vertical.contentPadding);
  });

  it("defaults to vertical", () => {
    expect(resolveCardLayout(undefined).direction).toBe("column");
  });
});

describe("clampOverlayOpacity", () => {
  it("clamps values into [0, 1]", () => {
    expect(clampOverlayOpacity(-1)).toBe(0);
    expect(clampOverlayOpacity(0.3)).toBe(0.3);
    expect(clampOverlayOpacity(1)).toBe(1);
    expect(clampOverlayOpacity(2)).toBe(1);
  });

  it("defaults non-numeric and NaN to 0.55", () => {
    expect(clampOverlayOpacity(undefined)).toBe(0.55);
    expect(clampOverlayOpacity(Number.NaN)).toBe(0.55);
  });
});

describe("buildCardStyles", () => {
  it("is a column flex container by default", () => {
    const s = buildCardStyles({ hovered: false });
    expect(s.display).toBe("flex");
    expect(s.flexDirection).toBe("column");
  });

  it("switches to row for horizontal layout", () => {
    const s = buildCardStyles({ layout: "horizontal", hovered: false });
    expect(s.flexDirection).toBe("row");
  });

  it("applies lift transform only when hovered with lift effect", () => {
    const hover = buildCardStyles({ hoverEffect: "lift", hovered: true });
    const idle = buildCardStyles({ hoverEffect: "lift", hovered: false });
    expect(hover.transform).toBe("translateY(-3px)");
    expect(idle.transform).toBeUndefined();
  });

  it("ignores lift on cards with hoverEffect='none'", () => {
    const s = buildCardStyles({ hoverEffect: "none", hovered: true });
    expect(s.transform).toBeUndefined();
  });

  it("glow hover injects an indigo halo", () => {
    const s = buildCardStyles({ hoverEffect: "glow", hovered: true });
    expect(String(s.boxShadow ?? "")).toContain("rgba(99, 102, 241");
  });

  it("elevated cards carry a base shadow even at rest", () => {
    const s = buildCardStyles({ variant: "elevated", hovered: false });
    expect(s.boxShadow).toBeDefined();
  });
});
