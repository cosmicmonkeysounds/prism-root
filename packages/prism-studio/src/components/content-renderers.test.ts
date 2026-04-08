/**
 * Pure-function tests for content helpers.
 */

import { describe, it, expect } from "vitest";
import {
  renderInlineMarkdown,
  renderMarkdown,
  isSafeIframeUrl,
} from "./content-renderers.js";

describe("renderInlineMarkdown", () => {
  it("escapes HTML special characters", () => {
    expect(renderInlineMarkdown("<script>&")).toBe("&lt;script&gt;&amp;");
  });

  it("renders bold and italic", () => {
    expect(renderInlineMarkdown("**bold**")).toBe("<strong>bold</strong>");
    expect(renderInlineMarkdown("*italic*")).toBe("<em>italic</em>");
    expect(renderInlineMarkdown("__also bold__")).toBe("<strong>also bold</strong>");
  });

  it("renders inline code", () => {
    expect(renderInlineMarkdown("hello `world`")).toBe("hello <code>world</code>");
  });

  it("renders links with rel + target", () => {
    const html = renderInlineMarkdown("[Prism](https://example.com)");
    expect(html).toContain('href="https://example.com"');
    expect(html).toContain('target="_blank"');
    expect(html).toContain('rel="noopener noreferrer"');
    expect(html).toContain(">Prism</a>");
  });
});

describe("renderMarkdown", () => {
  it("returns empty string for empty input", () => {
    expect(renderMarkdown("")).toBe("");
  });

  it("renders headings of varying levels", () => {
    const html = renderMarkdown("# H1\n## H2\n### H3");
    expect(html).toContain("<h1>H1</h1>");
    expect(html).toContain("<h2>H2</h2>");
    expect(html).toContain("<h3>H3</h3>");
  });

  it("renders unordered lists", () => {
    const html = renderMarkdown("- one\n- two\n- three");
    expect(html).toContain("<ul>");
    expect(html).toContain("<li>one</li>");
    expect(html).toContain("<li>two</li>");
    expect(html).toContain("<li>three</li>");
    expect(html).toContain("</ul>");
  });

  it("renders ordered lists", () => {
    const html = renderMarkdown("1. first\n2. second");
    expect(html).toContain("<ol>");
    expect(html).toContain("<li>first</li>");
    expect(html).toContain("<li>second</li>");
  });

  it("renders horizontal rules", () => {
    const html = renderMarkdown("before\n\n---\n\nafter");
    expect(html).toContain("<hr/>");
  });

  it("renders blockquotes", () => {
    const html = renderMarkdown("> quoted");
    expect(html).toContain("<blockquote>quoted</blockquote>");
  });

  it("renders fenced code blocks with escaping", () => {
    const html = renderMarkdown("```\nlet x = <y>;\n```");
    expect(html).toContain("<pre><code>");
    expect(html).toContain("&lt;y&gt;");
    expect(html).toContain("</code></pre>");
  });

  it("renders paragraphs with inline formatting", () => {
    const html = renderMarkdown("Hello **world**");
    expect(html).toContain("<p>Hello <strong>world</strong></p>");
  });
});

describe("isSafeIframeUrl", () => {
  it("accepts http and https URLs", () => {
    expect(isSafeIframeUrl("https://example.com")).toBe(true);
    expect(isSafeIframeUrl("http://example.com/path?q=1")).toBe(true);
  });

  it("rejects unsafe schemes", () => {
    expect(isSafeIframeUrl("javascript:alert(1)")).toBe(false);
    expect(isSafeIframeUrl("data:text/html,<script/>")).toBe(false);
    expect(isSafeIframeUrl("file:///etc/passwd")).toBe(false);
  });

  it("rejects empty or malformed input", () => {
    expect(isSafeIframeUrl("")).toBe(false);
    expect(isSafeIframeUrl("   ")).toBe(false);
    expect(isSafeIframeUrl("not a url")).toBe(false);
  });
});
