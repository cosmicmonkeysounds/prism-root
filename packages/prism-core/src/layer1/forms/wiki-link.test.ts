import { describe, it, expect } from "vitest";
import {
  parseWikiLinks,
  extractLinkedIds,
  renderWikiLinks,
  buildWikiLink,
  detectInlineLink,
} from "./wiki-link.js";

describe("parseWikiLinks", () => {
  it("parses simple id link", () => {
    const tokens = parseWikiLinks("[[task-1]]");
    expect(tokens).toEqual([{ kind: "link", id: "task-1", display: "task-1", raw: "[[task-1]]" }]);
  });

  it("parses id with display text", () => {
    const tokens = parseWikiLinks("[[task-1|Fix bug]]");
    expect(tokens).toEqual([
      { kind: "link", id: "task-1", display: "Fix bug", raw: "[[task-1|Fix bug]]" },
    ]);
  });

  it("parses multiple links with text between", () => {
    const tokens = parseWikiLinks("See [[a]] and [[b|Beta]]");
    expect(tokens).toHaveLength(4);
    expect(tokens[0]).toEqual({ kind: "text", text: "See " });
    expect(tokens[1]).toEqual({ kind: "link", id: "a", display: "a", raw: "[[a]]" });
    expect(tokens[2]).toEqual({ kind: "text", text: " and " });
    expect(tokens[3]).toEqual({ kind: "link", id: "b", display: "Beta", raw: "[[b|Beta]]" });
  });

  it("returns single text token when no links", () => {
    expect(parseWikiLinks("hello world")).toEqual([{ kind: "text", text: "hello world" }]);
  });

  it("returns empty array for empty string", () => {
    expect(parseWikiLinks("")).toEqual([]);
  });

  it("trims whitespace from id and display", () => {
    const tokens = parseWikiLinks("[[ abc | Abc Def ]]");
    expect(tokens[0]).toEqual({
      kind: "link",
      id: "abc",
      display: "Abc Def",
      raw: "[[ abc | Abc Def ]]",
    });
  });
});

describe("extractLinkedIds", () => {
  it("extracts all ids", () => {
    expect(extractLinkedIds("[[a]] and [[b|Beta]] and [[c]]")).toEqual(["a", "b", "c"]);
  });

  it("returns empty when no links", () => {
    expect(extractLinkedIds("no links here")).toEqual([]);
  });
});

describe("renderWikiLinks", () => {
  it("replaces with resolver when no display", () => {
    const result = renderWikiLinks("See [[task-1]]", (id) => `Name(${id})`);
    expect(result).toBe("See Name(task-1)");
  });

  it("uses display text when provided", () => {
    const result = renderWikiLinks("See [[task-1|Fix bug]]", () => "UNUSED");
    expect(result).toBe("See Fix bug");
  });
});

describe("buildWikiLink", () => {
  it("builds with display", () => {
    expect(buildWikiLink("task-1", "Fix bug")).toBe("[[task-1|Fix bug]]");
  });

  it("builds without display", () => {
    expect(buildWikiLink("task-1")).toBe("[[task-1]]");
  });

  it("omits display when same as id", () => {
    expect(buildWikiLink("task-1", "task-1")).toBe("[[task-1]]");
  });
});

describe("detectInlineLink", () => {
  it("detects partial link", () => {
    expect(detectInlineLink("See [[Fix", 9)).toBe("Fix");
  });

  it("returns null for closed link", () => {
    expect(detectInlineLink("See [[Fix]]", 11)).toBeNull();
  });

  it("returns null when no opening brackets", () => {
    expect(detectInlineLink("hello", 5)).toBeNull();
  });

  it("returns empty string at cursor right after [[", () => {
    expect(detectInlineLink("See [[", 6)).toBe("");
  });
});
