/**
 * Pure-function tests for page-export.
 */

import { describe, it, expect } from "vitest";
import type { GraphObject } from "@prism/core/object-model";
import {
  exportPageToJson,
  exportPageToHtml,
  escapeHtml,
  escapeAttr,
  cssToInline,
  renderNodeHtml,
  toExportedNode,
} from "./page-export.js";

function obj(
  id: string,
  type: string,
  name: string,
  position: number,
  parentId: string | null,
  data: Record<string, unknown> = {},
): GraphObject {
  return {
    id: id as GraphObject["id"],
    type,
    name,
    parentId: parentId as GraphObject["parentId"],
    position,
    status: "draft",
    tags: [],
    date: null,
    data,
    createdAt: new Date("2026-01-01T00:00:00Z"),
    updatedAt: new Date("2026-01-01T00:00:00Z"),
    deletedAt: null,
  } as unknown as GraphObject;
}

describe("escapeHtml / escapeAttr", () => {
  it("escapes angle brackets, ampersands, and quotes", () => {
    expect(escapeHtml('<a href="x">&</a>')).toBe("&lt;a href=&quot;x&quot;&gt;&amp;&lt;/a&gt;");
  });
  it("escapes single quotes in HTML", () => {
    expect(escapeHtml("O'Brien")).toBe("O&#39;Brien");
  });
  it("escapes double quotes and angle brackets in attributes", () => {
    expect(escapeAttr('x"y<z')).toBe("x&quot;y&lt;z");
  });
});

describe("cssToInline", () => {
  it("converts camelCase keys to kebab-case", () => {
    expect(cssToInline({ fontSize: 18, color: "red" })).toBe("font-size: 18px; color: red");
  });
  it("skips undefined, null, and empty values", () => {
    expect(cssToInline({ color: undefined, background: null, fontSize: "" })).toBe("");
  });
  it("appends px to bare numbers", () => {
    expect(cssToInline({ width: 100 })).toBe("width: 100px");
  });
  it("omits px for line-height", () => {
    expect(cssToInline({ lineHeight: 1.4 })).toBe("line-height: 1.4");
  });
});

describe("toExportedNode + exportPageToJson", () => {
  it("inlines children in position order", () => {
    const page = obj("p1", "page", "Home", 0, null, { title: "Home" });
    const s1 = obj("s1", "section", "S1", 1, "p1");
    const s0 = obj("s0", "section", "S0", 0, "p1");
    const h = obj("h", "heading", "H", 0, "s0", { text: "Hi", level: "h1" });

    const result = exportPageToJson(page, [page, s0, s1, h], () => new Date("2026-04-08T12:00:00Z"));
    expect(result.format).toBe("prism-page/v1");
    expect(result.exportedAt).toBe("2026-04-08T12:00:00.000Z");
    expect(result.page.id).toBe("p1");
    expect(result.page.children.map((c) => c.id)).toEqual(["s0", "s1"]);
    expect(result.page.children[0]?.children[0]?.data["text"]).toBe("Hi");
  });

  it("skips deleted objects", () => {
    const page = obj("p1", "page", "Home", 0, null);
    const deletedChild = {
      ...obj("dc", "heading", "Del", 0, "p1"),
      deletedAt: new Date().toISOString(),
    } as unknown as GraphObject;
    const byParent = new Map<string | null, GraphObject[]>();
    const result = exportPageToJson(page, [page, deletedChild]);
    expect(result.page.children).toHaveLength(0);
    // Silence unused var for clarity
    expect(byParent.size).toBe(0);
  });

  it("toExportedNode clones data to avoid mutation", () => {
    const page = obj("p1", "page", "Home", 0, null, { title: "Home" });
    const root = toExportedNode(page, new Map());
    root.data["title"] = "Mutated";
    expect(page.data["title"]).toBe("Home");
  });
});

describe("renderNodeHtml", () => {
  it("renders a heading with the right level", () => {
    const node = toExportedNode(
      obj("h", "heading", "H", 0, null, { text: "Hello", level: "h3" }),
      new Map(),
    );
    expect(renderNodeHtml(node)).toBe("<h3>Hello</h3>");
  });

  it("renders unknown levels as h2", () => {
    const node = toExportedNode(
      obj("h", "heading", "H", 0, null, { text: "x", level: "bogus" }),
      new Map(),
    );
    expect(renderNodeHtml(node)).toBe("<h2>x</h2>");
  });

  it("renders a button as a link when href is set", () => {
    const node = toExportedNode(
      obj("b", "button", "B", 0, null, { label: "Go", href: "https://a.test" }),
      new Map(),
    );
    expect(renderNodeHtml(node)).toContain('href="https://a.test"');
    expect(renderNodeHtml(node)).toContain(">Go</a>");
  });

  it("escapes text content", () => {
    const node = toExportedNode(
      obj("t", "text-block", "T", 0, null, { content: "<script>alert(1)</script>" }),
      new Map(),
    );
    expect(renderNodeHtml(node)).toBe("<p>&lt;script&gt;alert(1)&lt;/script&gt;</p>");
  });

  it("renders a code block with language attribute", () => {
    const node = toExportedNode(
      obj("c", "code-block", "C", 0, null, { source: "let x = 1;", language: "ts" }),
      new Map(),
    );
    const html = renderNodeHtml(node);
    expect(html).toContain('data-language="ts"');
    expect(html).toContain("let x = 1;");
  });

  it("applies inline style from block style data", () => {
    const node = toExportedNode(
      obj("t", "text-block", "T", 0, null, { content: "x", background: "#abc", paddingX: 8, paddingY: 4 }),
      new Map(),
    );
    const html = renderNodeHtml(node);
    expect(html).toContain("background: #abc");
    expect(html).toContain("padding: 4px 8px");
  });

  it("renders iframe-widget with safe attributes", () => {
    const node = toExportedNode(
      obj("i", "iframe-widget", "I", 0, null, { src: "https://example.com", title: "T", height: 400 }),
      new Map(),
    );
    const html = renderNodeHtml(node);
    expect(html).toContain('src="https://example.com"');
    expect(html).toContain('height="400"');
    expect(html).toContain("loading=\"lazy\"");
  });
});

describe("exportPageToHtml", () => {
  it("wraps output in a full HTML document by default", () => {
    const page = obj("p1", "page", "Home", 0, null, { title: "My Page" });
    const html = exportPageToHtml(page, [page]);
    expect(html).toContain("<!DOCTYPE html>");
    expect(html).toContain("<title>My Page</title>");
    expect(html).toContain("<style>");
    expect(html).toContain("<main data-page-id=\"p1\"");
  });

  it("returns a fragment when fragmentOnly is true", () => {
    const page = obj("p1", "page", "Home", 0, null);
    const html = exportPageToHtml(page, [page], { fragmentOnly: true });
    expect(html).not.toContain("<!DOCTYPE html>");
    expect(html.startsWith("<main")).toBe(true);
  });

  it("escapes the document title", () => {
    const page = obj("p1", "page", "Home", 0, null, { title: "<danger>" });
    const html = exportPageToHtml(page, [page]);
    expect(html).toContain("<title>&lt;danger&gt;</title>");
  });

  it("allows overriding CSS", () => {
    const page = obj("p1", "page", "Home", 0, null);
    const html = exportPageToHtml(page, [page], { inlineCss: "body{color:red}" });
    expect(html).toContain("<style>body{color:red}</style>");
  });
});
