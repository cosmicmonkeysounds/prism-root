import { describe, it, expect } from "vitest";
import {
  FONT_OPTIONS,
  findFontOption,
  isGoogleFontValue,
  googleFontsHref,
  collectFontFamilies,
} from "./fonts.js";

describe("FONT_OPTIONS", () => {
  it("ships system stacks and google fonts", () => {
    const labels = FONT_OPTIONS.map((o) => o.label);
    expect(labels).toContain("System Sans");
    expect(labels).toContain("Inter");
    expect(labels).toContain("JetBrains Mono");
  });

  it("every google font declares weights", () => {
    for (const o of FONT_OPTIONS) {
      if (o.google) {
        expect(o.weights && o.weights.length > 0).toBe(true);
      }
    }
  });
});

describe("findFontOption", () => {
  it("resolves an exact stack", () => {
    const opt = findFontOption("Inter, sans-serif");
    expect(opt?.label).toBe("Inter");
  });

  it("resolves by leading family name even if fallbacks differ", () => {
    const opt = findFontOption("Inter, Helvetica Neue, Arial, sans-serif");
    expect(opt?.label).toBe("Inter");
  });

  it("returns undefined for unknown fonts", () => {
    expect(findFontOption("Comic Sans MS, cursive")).toBeUndefined();
  });

  it("returns undefined for empty/null input", () => {
    expect(findFontOption("")).toBeUndefined();
    expect(findFontOption(null)).toBeUndefined();
    expect(findFontOption(undefined)).toBeUndefined();
  });
});

describe("isGoogleFontValue", () => {
  it("true for google-hosted fonts", () => {
    expect(isGoogleFontValue("Inter, sans-serif")).toBe(true);
  });

  it("false for system stacks", () => {
    expect(isGoogleFontValue("system-ui, -apple-system, Segoe UI, Roboto, sans-serif")).toBe(false);
  });

  it("false for unknown fonts", () => {
    expect(isGoogleFontValue("Comic Sans MS, cursive")).toBe(false);
  });
});

describe("googleFontsHref", () => {
  it("returns undefined when no google fonts in list", () => {
    expect(googleFontsHref([])).toBeUndefined();
    expect(googleFontsHref(["system-ui, sans-serif"])).toBeUndefined();
  });

  it("builds a single-family css2 url with sorted weights", () => {
    const href = googleFontsHref(["Inter, sans-serif"]);
    expect(href).toBeDefined();
    expect(href).toContain("https://fonts.googleapis.com/css2?");
    expect(href).toContain("family=Inter:wght@400;500;600;700");
    expect(href).toContain("display=swap");
  });

  it("combines multiple google families into one url, deduped", () => {
    const href = googleFontsHref([
      "Inter, sans-serif",
      "Inter, sans-serif",
      "Lora, serif",
      "system-ui, sans-serif",
    ]);
    expect(href).toBeDefined();
    expect(href?.match(/family=Inter/g) ?? []).toHaveLength(1);
    expect(href).toContain("family=Lora:wght@400;700");
  });

  it("URL-encodes multi-word family names with +", () => {
    const href = googleFontsHref(["Playfair Display, serif"]);
    expect(href).toContain("family=Playfair+Display");
  });
});

describe("collectFontFamilies", () => {
  it("walks nested children and dedupes", () => {
    const tree = {
      data: { fontFamily: "Inter, sans-serif" },
      children: [
        {
          data: { fontFamily: "Lora, serif" },
          children: [
            { data: { fontFamily: "Inter, sans-serif" }, children: [] },
          ],
        },
        {
          data: {},
          children: [
            { data: { fontFamily: "system-ui, sans-serif" }, children: [] },
          ],
        },
      ],
    };
    const families = collectFontFamilies(tree);
    expect(families).toContain("Inter, sans-serif");
    expect(families).toContain("Lora, serif");
    expect(families).toContain("system-ui, sans-serif");
    expect(new Set(families).size).toBe(families.length);
  });

  it("ignores missing/empty fontFamily fields", () => {
    const tree = {
      data: {},
      children: [{ data: { fontFamily: "" }, children: [] }],
    };
    expect(collectFontFamilies(tree)).toEqual([]);
  });
});
