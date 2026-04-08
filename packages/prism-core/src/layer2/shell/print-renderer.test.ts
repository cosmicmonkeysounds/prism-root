/**
 * Tests for print-renderer — page CSS generation and full document wrapping.
 */

import { describe, it, expect } from "vitest";
import { resolvePageSize, buildPageCss, renderForPrint } from "./print-renderer.js";
import type { PrintConfig } from "../../layer1/facet/facet-schema.js";

describe("resolvePageSize", () => {
  it("resolves letter size", () => {
    expect(resolvePageSize({ pageSize: "letter" })).toEqual({ width: 612, height: 792 });
  });

  it("resolves a4 size", () => {
    expect(resolvePageSize({ pageSize: "a4" })).toEqual({ width: 595, height: 842 });
  });

  it("resolves legal size", () => {
    expect(resolvePageSize({ pageSize: "legal" })).toEqual({ width: 612, height: 1008 });
  });

  it("uses custom dimensions when pageSize is 'custom'", () => {
    expect(
      resolvePageSize({ pageSize: "custom", customWidth: 400, customHeight: 600 }),
    ).toEqual({ width: 400, height: 600 });
  });

  it("falls back to letter defaults for unset custom dimensions", () => {
    expect(resolvePageSize({ pageSize: "custom" })).toEqual({ width: 612, height: 792 });
  });
});

describe("buildPageCss", () => {
  it("emits @page with size and margins", () => {
    const css = buildPageCss({ pageSize: "letter" });
    expect(css).toContain("@page");
    expect(css).toContain("612pt 792pt");
    expect(css).toContain("margin: 36pt 36pt 36pt 36pt");
  });

  it("swaps dimensions for landscape", () => {
    const css = buildPageCss({ pageSize: "letter", orientation: "landscape" });
    expect(css).toContain("792pt 612pt");
  });

  it("honours explicit margins", () => {
    const css = buildPageCss({
      pageSize: "letter",
      margins: { top: 10, right: 20, bottom: 30, left: 40 },
    });
    expect(css).toContain("10pt 20pt 30pt 40pt");
  });

  it("emits header rule when pageHeader set", () => {
    const css = buildPageCss({ pageSize: "a4", pageHeader: "My Report" });
    expect(css).toContain("@top-center");
    expect(css).toContain('"My Report"');
  });

  it("emits footer rule when pageFooter set", () => {
    const css = buildPageCss({ pageSize: "a4", pageFooter: "Confidential" });
    expect(css).toContain("@bottom-center");
    expect(css).toContain('"Confidential"');
  });

  it("emits page number counter when showPageNumbers set", () => {
    const css = buildPageCss({ pageSize: "letter", showPageNumbers: true });
    expect(css).toContain("counter(page)");
  });
});

describe("renderForPrint", () => {
  const config: PrintConfig = { pageSize: "letter" };

  it("produces a full HTML document", () => {
    const html = renderForPrint("<h1>Hi</h1>", config);
    expect(html).toMatch(/^<!DOCTYPE html>/);
    expect(html).toContain("<html>");
    expect(html).toContain("</html>");
    expect(html).toContain("<h1>Hi</h1>");
  });

  it("embeds @page CSS inline", () => {
    const html = renderForPrint("", config);
    expect(html).toContain("@page");
    expect(html).toContain("612pt 792pt");
  });

  it("includes group page break CSS when requested", () => {
    const html = renderForPrint("", { pageSize: "letter", pageBreakBeforeGroup: true });
    expect(html).toContain("page-break-before: always");
  });

  it("does not include page break rules by default", () => {
    const html = renderForPrint("", config);
    expect(html).not.toContain("page-break-before: always");
  });
});
