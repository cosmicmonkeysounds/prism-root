/**
 * Pure-helper tests for button-renderer.
 */

import { describe, it, expect } from "vitest";
import {
  resolveVariant,
  resolveSize,
  resolveRadius,
  resolveShadow,
  resolveTransform,
  buildButtonStyles,
  resolveRel,
} from "./button-renderer.js";

describe("resolveVariant", () => {
  it("returns distinct palettes per variant", () => {
    expect(resolveVariant("primary").background).toBe("#6366f1");
    expect(resolveVariant("secondary").background).toBe("#e2e8f0");
    expect(resolveVariant("outline").background).toBe("transparent");
    expect(resolveVariant("ghost").background).toBe("transparent");
    expect(resolveVariant("danger").background).toBe("#dc2626");
    expect(resolveVariant("success").background).toBe("#16a34a");
    expect(resolveVariant("gradient").background).toContain("linear-gradient");
  });

  it("falls back to primary when variant is undefined", () => {
    expect(resolveVariant(undefined).background).toBe(resolveVariant("primary").background);
  });

  it("outline has a visible border color", () => {
    const p = resolveVariant("outline");
    expect(p.borderColor).toBe("#6366f1");
    expect(p.color).toBe("#6366f1");
  });
});

describe("resolveSize", () => {
  it("scales padding/font monotonically from xs to xl", () => {
    const xs = resolveSize("xs");
    const sm = resolveSize("sm");
    const md = resolveSize("md");
    const lg = resolveSize("lg");
    const xl = resolveSize("xl");
    expect(xs.fontSize).toBeLessThan(sm.fontSize);
    expect(sm.fontSize).toBeLessThan(md.fontSize);
    expect(md.fontSize).toBeLessThan(lg.fontSize);
    expect(lg.fontSize).toBeLessThan(xl.fontSize);
    expect(xs.paddingX).toBeLessThan(xl.paddingX);
  });

  it("defaults to md", () => {
    expect(resolveSize(undefined).fontSize).toBe(resolveSize("md").fontSize);
  });
});

describe("resolveRadius", () => {
  it("maps presets to pixel values", () => {
    expect(resolveRadius("none")).toBe(0);
    expect(resolveRadius("sm")).toBe(4);
    expect(resolveRadius("md")).toBe(6);
    expect(resolveRadius("lg")).toBe(12);
    expect(resolveRadius("full")).toBe(9999);
  });

  it("defaults to md", () => {
    expect(resolveRadius(undefined)).toBe(6);
  });
});

describe("resolveShadow", () => {
  it("returns undefined for 'none' with no hover effect", () => {
    expect(resolveShadow("none", false, "none")).toBeUndefined();
    expect(resolveShadow(undefined, false, undefined)).toBeUndefined();
  });

  it("returns progressively heavier shadows for sm/md/lg", () => {
    const sm = resolveShadow("sm", false, "none") ?? "";
    const md = resolveShadow("md", false, "none") ?? "";
    const lg = resolveShadow("lg", false, "none") ?? "";
    expect(sm).not.toBe("");
    expect(md).not.toBe("");
    expect(lg).not.toBe("");
    expect(sm).not.toBe(md);
    expect(md).not.toBe(lg);
  });

  it("layers a lift halo on top of the base shadow when hovered+lift", () => {
    const lifted = resolveShadow("sm", true, "lift") ?? "";
    expect(lifted).toContain("rgba(15, 23, 42");
    expect(lifted.split(",").length).toBeGreaterThan(1);
  });

  it("returns a glow halo when hovered+glow regardless of base shadow", () => {
    const glowed = resolveShadow("none", true, "glow") ?? "";
    expect(glowed).toContain("rgba(99, 102, 241");
  });
});

describe("resolveTransform", () => {
  it("is undefined when not hovered", () => {
    expect(resolveTransform(false, "lift")).toBeUndefined();
    expect(resolveTransform(false, "scale")).toBeUndefined();
  });

  it("translates up on lift hover", () => {
    expect(resolveTransform(true, "lift")).toBe("translateY(-2px)");
  });

  it("scales up on scale hover", () => {
    expect(resolveTransform(true, "scale")).toBe("scale(1.04)");
  });

  it("returns undefined for none/glow (glow uses box-shadow, not transform)", () => {
    expect(resolveTransform(true, "none")).toBeUndefined();
    expect(resolveTransform(true, "glow")).toBeUndefined();
  });
});

describe("buildButtonStyles", () => {
  it("uses inline-flex by default and flex for fullWidth", () => {
    const s1 = buildButtonStyles({ hovered: false });
    const s2 = buildButtonStyles({ hovered: false, fullWidth: true });
    expect(s1.display).toBe("inline-flex");
    expect(s2.display).toBe("flex");
    expect(s2.width).toBe("100%");
  });

  it("drops opacity and sets not-allowed cursor when disabled", () => {
    const s = buildButtonStyles({ hovered: false, disabled: true });
    expect(s.opacity).toBeLessThan(1);
    expect(s.cursor).toBe("not-allowed");
  });

  it("disabled buttons do not pick up hover styles even when hovered=true", () => {
    const disabledHovered = buildButtonStyles({
      variant: "primary",
      hovered: true,
      disabled: true,
    });
    const idle = buildButtonStyles({ variant: "primary", hovered: false });
    expect(disabledHovered.background).toBe(idle.background);
  });

  it("swaps to hover palette when hovered and enabled", () => {
    const hovered = buildButtonStyles({ variant: "primary", hovered: true });
    const idle = buildButtonStyles({ variant: "primary", hovered: false });
    expect(hovered.background).not.toBe(idle.background);
  });

  it("applies transform on lift hover", () => {
    const s = buildButtonStyles({ variant: "primary", hovered: true, hoverEffect: "lift" });
    expect(s.transform).toBe("translateY(-2px)");
  });
});

describe("resolveRel", () => {
  it("returns author-supplied rel verbatim", () => {
    expect(resolveRel("_blank", "author nofollow")).toBe("author nofollow");
  });

  it("defaults to noopener noreferrer for _blank targets", () => {
    expect(resolveRel("_blank", undefined)).toBe("noopener noreferrer");
    expect(resolveRel("_blank", "")).toBe("noopener noreferrer");
  });

  it("returns undefined for same-tab targets", () => {
    expect(resolveRel("_self", undefined)).toBeUndefined();
    expect(resolveRel(undefined, undefined)).toBeUndefined();
  });
});
