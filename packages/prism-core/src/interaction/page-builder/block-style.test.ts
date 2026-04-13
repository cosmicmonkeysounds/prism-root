/**
 * Tests for block-style pure helpers.
 */

import { describe, it, expect } from "vitest";
import {
  computeBlockStyle,
  extractBlockStyle,
  mergeCss,
  resolveShadow,
  parseCssDeclarations,
  STYLE_FIELD_DEFS,
  BREAKPOINTS,
  mediaRule,
  computeMobileOverride,
  computeTabletOverride,
  extractResponsive,
} from "./block-style.js";

describe("resolveShadow", () => {
  it("maps preset tokens to CSS values", () => {
    expect(resolveShadow("none")).toBe("none");
    expect(resolveShadow("sm")).toContain("0 1px 2px");
    expect(resolveShadow("md")).toContain("0 2px 6px");
    expect(resolveShadow("lg")).toContain("0 8px 24px");
  });

  it("passes through custom strings", () => {
    expect(resolveShadow("0 0 0 1px red")).toBe("0 0 0 1px red");
  });

  it("returns undefined for blank", () => {
    expect(resolveShadow(undefined)).toBeUndefined();
    expect(resolveShadow(null)).toBeUndefined();
    expect(resolveShadow("")).toBeUndefined();
  });
});

describe("computeBlockStyle", () => {
  it("returns empty object for empty input", () => {
    expect(computeBlockStyle(null)).toEqual({});
    expect(computeBlockStyle(undefined)).toEqual({});
    expect(computeBlockStyle({})).toEqual({});
  });

  it("maps background and textColor", () => {
    const css = computeBlockStyle({ background: "#f00", textColor: "#0f0" });
    expect(css.background).toBe("#f00");
    expect(css.color).toBe("#0f0");
  });

  it("emits padding shorthand from paddingX/paddingY", () => {
    expect(computeBlockStyle({ paddingX: 12, paddingY: 8 }).padding).toBe("8px 12px");
    expect(computeBlockStyle({ paddingX: 12 }).padding).toBe("0px 12px");
  });

  it("emits margin shorthand from marginX/marginY", () => {
    expect(computeBlockStyle({ marginX: 4, marginY: 2 }).margin).toBe("2px 4px");
  });

  it("emits border shorthand only when borderWidth > 0", () => {
    expect(computeBlockStyle({ borderWidth: 2, borderColor: "#000" }).border).toBe(
      "2px solid #000",
    );
    expect(computeBlockStyle({ borderWidth: 0 }).border).toBeUndefined();
  });

  it("resolves borderRadius and shadow", () => {
    const css = computeBlockStyle({ borderRadius: 8, shadow: "md" });
    expect(css.borderRadius).toBe(8);
    expect(css.boxShadow).toContain("0 2px 6px");
  });

  it("maps typography fields", () => {
    const css = computeBlockStyle({
      fontFamily: "Inter",
      fontSize: 18,
      fontWeight: 700,
      lineHeight: 1.4,
      letterSpacing: 1,
      textAlign: "center",
    });
    expect(css.fontFamily).toBe("Inter");
    expect(css.fontSize).toBe(18);
    expect(css.fontWeight).toBe(700);
    expect(css.lineHeight).toBe(1.4);
    expect(css.letterSpacing).toBe(1);
    expect(css.textAlign).toBe("center");
  });

  it("coerces stringy fontWeight to number when possible", () => {
    expect(computeBlockStyle({ fontWeight: "600" }).fontWeight).toBe(600);
    expect(computeBlockStyle({ fontWeight: "bold" }).fontWeight).toBe("bold");
  });
});

describe("extractBlockStyle", () => {
  it("returns empty for non-object input", () => {
    expect(extractBlockStyle(null)).toEqual({});
    expect(extractBlockStyle(undefined)).toEqual({});
    expect(extractBlockStyle("x")).toEqual({});
    expect(extractBlockStyle(42)).toEqual({});
  });

  it("picks known keys and ignores unknowns", () => {
    const out = extractBlockStyle({
      background: "#abc",
      fontSize: 16,
      foo: "ignore-me",
      paddingX: 8,
    });
    expect(out).toEqual({ background: "#abc", fontSize: 16, paddingX: 8 });
  });

  it("skips empty string values", () => {
    const out = extractBlockStyle({ background: "", fontSize: 16 });
    expect(out).toEqual({ fontSize: 16 });
  });
});

describe("mergeCss", () => {
  it("merges overlay onto base with overlay winning", () => {
    expect(mergeCss({ color: "red" }, { color: "blue" })).toEqual({ color: "blue" });
    expect(mergeCss({ color: "red", fontSize: 14 }, { fontSize: 18 })).toEqual({
      color: "red",
      fontSize: 18,
    });
  });

  it("tolerates undefined inputs", () => {
    expect(mergeCss(undefined, { color: "red" })).toEqual({ color: "red" });
    expect(mergeCss({ color: "red" }, undefined)).toEqual({ color: "red" });
    expect(mergeCss(undefined, undefined)).toEqual({});
  });
});

describe("responsive overrides", () => {
  it("extractResponsive picks hide flags and mobile overrides", () => {
    const r = extractResponsive({
      hiddenMobile: true,
      hiddenTablet: false,
      mobilePaddingX: 8,
      mobilePaddingY: 4,
      mobileFontSize: 14,
      mobileTextAlign: "center",
      unrelated: "skip",
    });
    expect(r).toEqual({
      hiddenMobile: true,
      hiddenTablet: false,
      mobilePaddingX: 8,
      mobilePaddingY: 4,
      mobileFontSize: 14,
      mobileTextAlign: "center",
    });
  });

  it("computeMobileOverride hides when hiddenMobile is set", () => {
    expect(computeMobileOverride({ hiddenMobile: true })).toEqual({ display: "none" });
  });

  it("computeMobileOverride emits padding + font + align", () => {
    const css = computeMobileOverride({
      mobilePaddingX: 6,
      mobilePaddingY: 4,
      mobileFontSize: 14,
      mobileTextAlign: "center",
    });
    expect(css.padding).toBe("4px 6px");
    expect(css.fontSize).toBe(14);
    expect(css.textAlign).toBe("center");
  });

  it("computeTabletOverride only hides", () => {
    expect(computeTabletOverride({ hiddenTablet: true })).toEqual({ display: "none" });
    expect(computeTabletOverride({ hiddenMobile: true })).toEqual({});
  });

  it("mediaRule emits a @media block below the breakpoint", () => {
    const rule = mediaRule("mobile", ".x", { padding: "4px 6px", display: "none" });
    expect(rule).toContain(`@media (max-width: ${BREAKPOINTS.mobile - 1}px)`);
    expect(rule).toContain(".x");
    expect(rule).toContain("padding: 4px 6px");
    expect(rule).toContain("display: none");
  });

  it("mediaRule returns empty string for empty CSS", () => {
    expect(mediaRule("mobile", ".x", {})).toBe("");
  });
});

describe("parseCssDeclarations", () => {
  it("parses a simple declaration string", () => {
    expect(parseCssDeclarations("color: red; padding: 4px 8px;")).toEqual({
      color: "red",
      padding: "4px 8px",
    });
  });

  it("converts kebab-case keys to camelCase", () => {
    expect(parseCssDeclarations("background-color: #fff; font-weight: 700")).toEqual({
      backgroundColor: "#fff",
      fontWeight: 700,
    });
  });

  it("coerces plain numeric values to numbers", () => {
    expect(parseCssDeclarations("z-index: 5; opacity: 0.5")).toEqual({
      zIndex: 5,
      opacity: 0.5,
    });
  });

  it("keeps values with units as strings", () => {
    const out = parseCssDeclarations("width: 320px; height: 50%");
    expect(out.width).toBe("320px");
    expect(out.height).toBe("50%");
  });

  it("skips malformed declarations", () => {
    expect(parseCssDeclarations("color; : red; valid: yes")).toEqual({ valid: "yes" });
  });
});

describe("positioning and customCss", () => {
  it("maps position + top/left/right/bottom + zIndex", () => {
    const css = computeBlockStyle({
      position: "absolute",
      top: 20,
      left: 40,
      right: 10,
      bottom: 5,
      zIndex: 3,
    });
    expect(css.position).toBe("absolute");
    expect(css.top).toBe(20);
    expect(css.left).toBe(40);
    expect(css.right).toBe(10);
    expect(css.bottom).toBe(5);
    expect(css.zIndex).toBe(3);
  });

  it("customCss is merged last and wins over other fields", () => {
    const css = computeBlockStyle({
      background: "#fff",
      customCss: "background: #000; font-weight: 900",
    });
    expect(css.background).toBe("#000");
    expect(css.fontWeight).toBe(900);
  });

  it("ignores empty position/customCss values", () => {
    expect(computeBlockStyle({ position: "", customCss: "" })).toEqual({});
  });
});

describe("STYLE_FIELD_DEFS", () => {
  it("contains the core styling fields", () => {
    const ids = STYLE_FIELD_DEFS.map((f) => f.id);
    expect(ids).toContain("background");
    expect(ids).toContain("paddingX");
    expect(ids).toContain("paddingY");
    expect(ids).toContain("borderRadius");
    expect(ids).toContain("shadow");
    expect(ids).toContain("fontSize");
    expect(ids).toContain("textAlign");
  });

  it("groups Style and Typography fields in the UI", () => {
    const background = STYLE_FIELD_DEFS.find((f) => f.id === "background");
    const fontSize = STYLE_FIELD_DEFS.find((f) => f.id === "fontSize");
    expect(background?.ui?.group).toBe("Style");
    expect(fontSize?.ui?.group).toBe("Typography");
  });
});
