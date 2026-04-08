/**
 * Tests for pure markdown-toolbar edit builder used by the editor panel.
 */

import { describe, it, expect } from "vitest";
import { computeMarkdownEdit } from "./editor-panel.js";

describe("computeMarkdownEdit", () => {
  it("wraps a selection with before/after markers", () => {
    const edit = computeMarkdownEdit("hello world", 0, 5, {
      wrap: { before: "**", after: "**" },
    });
    expect(edit).toEqual({
      from: 0,
      to: 5,
      insert: "**hello**",
      anchor: 2,
      head: 7,
    });
  });

  it("wraps an empty selection (caret) so the user can type inside", () => {
    const edit = computeMarkdownEdit("abc", 3, 3, {
      wrap: { before: "_", after: "_" },
    });
    expect(edit.insert).toBe("__");
    expect(edit.anchor).toBe(4);
    expect(edit.head).toBe(4);
  });

  it("prefixes a single line with a heading marker", () => {
    const edit = computeMarkdownEdit("hello", 0, 0, { linePrefix: "# " });
    expect(edit).toEqual({
      from: 0,
      to: 5,
      insert: "# hello",
      anchor: 0,
      head: 7,
    });
  });

  it("prefixes each line in a multi-line selection", () => {
    const doc = "one\ntwo\nthree";
    const edit = computeMarkdownEdit(doc, 0, doc.length, {
      linePrefix: "- ",
    });
    expect(edit.insert).toBe("- one\n- two\n- three");
    expect(edit.from).toBe(0);
    expect(edit.to).toBe(doc.length);
  });

  it("expands mid-line selections to full line boundaries", () => {
    const doc = "alpha\nbravo\ncharlie";
    // caret inside "bravo"
    const edit = computeMarkdownEdit(doc, 8, 8, { linePrefix: "> " });
    expect(edit.from).toBe(6);
    expect(edit.to).toBe(11);
    expect(edit.insert).toBe("> bravo");
  });

  it("inserts a markdown link for an empty selection with placeholder text", () => {
    const edit = computeMarkdownEdit("", 0, 0, { link: true });
    expect(edit.insert).toBe("[link text](https://)");
    // anchor/head wraps the label so the user can overwrite it
    expect(edit.anchor).toBe(1);
    expect(edit.head).toBe(1 + "link text".length);
  });

  it("wraps the existing text as a link label when non-empty", () => {
    const edit = computeMarkdownEdit("see docs", 4, 8, { link: true });
    expect(edit.insert).toBe("[docs](https://)");
    expect(edit.anchor).toBe(5);
    expect(edit.head).toBe(5 + "docs".length);
  });
});
