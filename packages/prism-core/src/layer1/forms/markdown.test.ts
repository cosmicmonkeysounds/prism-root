import { describe, it, expect } from "vitest";
import { parseMarkdown, parseInline, inlineToPlainText, extractWikiIds } from "./markdown.js";

describe("parseMarkdown", () => {
  it("parses h1", () => {
    expect(parseMarkdown("# Hello")).toEqual([{ kind: "h1", text: "Hello" }]);
  });

  it("parses h2", () => {
    expect(parseMarkdown("## Hello")).toEqual([{ kind: "h2", text: "Hello" }]);
  });

  it("parses h3", () => {
    expect(parseMarkdown("### Hello")).toEqual([{ kind: "h3", text: "Hello" }]);
  });

  it("parses paragraph", () => {
    expect(parseMarkdown("Some text")).toEqual([{ kind: "p", text: "Some text" }]);
  });

  it("parses empty line", () => {
    expect(parseMarkdown("")).toEqual([{ kind: "empty" }]);
  });

  it("parses horizontal rule variants", () => {
    for (const hr of ["---", "***", "___"]) {
      expect(parseMarkdown(hr)).toEqual([{ kind: "hr" }]);
    }
  });

  it("parses blockquote", () => {
    expect(parseMarkdown("> quote")).toEqual([{ kind: "blockquote", text: "quote" }]);
  });

  it("parses unordered list items", () => {
    const tokens = parseMarkdown("- first\n* second");
    expect(tokens).toEqual([
      { kind: "li", text: "first" },
      { kind: "li", text: "second" },
    ]);
  });

  it("parses ordered list items", () => {
    const tokens = parseMarkdown("1. first\n2. second");
    expect(tokens).toEqual([
      { kind: "oli", text: "first", n: 1 },
      { kind: "oli", text: "second", n: 2 },
    ]);
  });

  it("parses unchecked task", () => {
    expect(parseMarkdown("- [ ] todo")).toEqual([{ kind: "task", text: "todo", checked: false }]);
  });

  it("parses checked task", () => {
    expect(parseMarkdown("- [x] done")).toEqual([{ kind: "task", text: "done", checked: true }]);
  });

  it("parses fenced code block", () => {
    const md = "```ts\nconst x = 1;\n```";
    expect(parseMarkdown(md)).toEqual([{ kind: "code", text: "const x = 1;", lang: "ts" }]);
  });

  it("parses code block without language", () => {
    const md = "```\nhello\n```";
    expect(parseMarkdown(md)).toEqual([{ kind: "code", text: "hello", lang: undefined }]);
  });

  it("handles unclosed code block", () => {
    const md = "```ts\nconst x = 1;";
    const tokens = parseMarkdown(md);
    expect(tokens).toEqual([{ kind: "code", text: "const x = 1;", lang: "ts" }]);
  });

  it("parses mixed content", () => {
    const md = "# Title\n\nSome text\n\n- item";
    const tokens = parseMarkdown(md);
    expect(tokens).toHaveLength(5);
    expect(tokens[0]).toEqual({ kind: "h1", text: "Title" });
    expect(tokens[1]).toEqual({ kind: "empty" });
    expect(tokens[2]).toEqual({ kind: "p", text: "Some text" });
    expect(tokens[3]).toEqual({ kind: "empty" });
    expect(tokens[4]).toEqual({ kind: "li", text: "item" });
  });
});

describe("parseInline", () => {
  it("parses plain text", () => {
    expect(parseInline("hello")).toEqual([{ kind: "text", text: "hello" }]);
  });

  it("parses bold", () => {
    const tokens = parseInline("**bold**");
    expect(tokens).toEqual([{ kind: "bold", children: [{ kind: "text", text: "bold" }] }]);
  });

  it("parses italic", () => {
    const tokens = parseInline("*italic*");
    expect(tokens).toEqual([{ kind: "italic", children: [{ kind: "text", text: "italic" }] }]);
  });

  it("parses inline code", () => {
    expect(parseInline("`code`")).toEqual([{ kind: "code", text: "code" }]);
  });

  it("parses markdown link", () => {
    expect(parseInline("[text](https://example.com)")).toEqual([
      { kind: "link", text: "text", href: "https://example.com" },
    ]);
  });

  it("parses wiki-link", () => {
    expect(parseInline("[[task-1|Fix]]")).toEqual([{ kind: "wiki", id: "task-1", display: "Fix" }]);
  });

  it("parses wiki-link without display", () => {
    expect(parseInline("[[task-1]]")).toEqual([{ kind: "wiki", id: "task-1", display: "task-1" }]);
  });

  it("parses mixed inline content", () => {
    const tokens = parseInline("Hello **bold** and *italic*");
    expect(tokens).toHaveLength(4);
    expect(tokens[0]).toEqual({ kind: "text", text: "Hello " });
    expect(tokens[1]).toEqual({ kind: "bold", children: [{ kind: "text", text: "bold" }] });
    expect(tokens[2]).toEqual({ kind: "text", text: " and " });
    expect(tokens[3]).toEqual({ kind: "italic", children: [{ kind: "text", text: "italic" }] });
  });

  it("handles empty string", () => {
    expect(parseInline("")).toEqual([]);
  });
});

describe("inlineToPlainText", () => {
  it("strips formatting", () => {
    const tokens = parseInline("Hello **bold** and `code` and [[a|Name]]");
    expect(inlineToPlainText(tokens)).toBe("Hello bold and code and Name");
  });
});

describe("extractWikiIds", () => {
  it("extracts wiki link ids from blocks", () => {
    const blocks = parseMarkdown("See [[task-1|Fix]] and [[note-2]]");
    expect(extractWikiIds(blocks)).toEqual(["task-1", "note-2"]);
  });

  it("returns empty for blocks without wiki links", () => {
    expect(extractWikiIds(parseMarkdown("# No links"))).toEqual([]);
  });
});
