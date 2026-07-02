import { describe, expect, it } from "vitest";
import { scan, type LineKind } from "../src/parser/index.ts";
import { Code } from "../src/parser/index.ts";

function first(text: string): LineKind {
  const [lines] = scan(text);
  return lines[0]!.kind;
}

describe("lexer::scan", () => {
  it("parses a heading", () => {
    expect(first("# Saltmere\n")).toEqual({ kind: "heading", title: "Saltmere" });
  });

  it("requires an identifier key for properties", () => {
    const k = first("entry: opening\n");
    expect(k).toEqual({ kind: "property", key: "entry", value: "opening" });
    // URL-shaped lines fall through to prose.
    expect(first("https://example.com\n").kind).toBe("prose");
  });

  it("parses knot markers with params", () => {
    expect(first("== opening\n")).toEqual({ kind: "knotMarker", name: "opening", params: [] });
    expect(first("== ask_about(topic, NPC)\n")).toEqual({
      kind: "knotMarker",
      name: "ask_about",
      params: ["topic", "NPC"],
    });
  });

  it("parses a declaration opener with mixins", () => {
    expect(first("CHARACTER Wren is Keeper, Combatant\n")).toEqual({
      kind: "declarationOpener",
      kindWord: "CHARACTER",
      name: "Wren",
      mixin: ["Keeper", "Combatant"],
    });
  });

  it("distinguishes speaker from action", () => {
    expect(first("WREN\n").kind).toBe("speaker");
    expect(first("Wren turns away.\n").kind).toBe("prose");
  });

  it("parses choice kinds", () => {
    const once = first("* Ring the bell.\n");
    expect(once.kind === "choice" && once.sticky).toBe(false);
    const sticky = first("+ Keep talking.\n");
    expect(sticky.kind === "choice" && sticky.sticky).toBe(true);
  });

  it("parses divert and tunnel return", () => {
    expect(first("-> ringing\n")).toEqual({ kind: "divertLine", text: "ringing" });
    expect(first("<-\n")).toEqual({ kind: "tunnelReturn" });
  });

  it("classifies a tunnel call as a divert line, not prose", () => {
    expect(first("(ringing) ->\n")).toEqual({ kind: "divertLine", text: "(ringing) ->" });
    expect(first("(ringing with tone: low) ->\n")).toEqual({
      kind: "divertLine",
      text: "(ringing with tone: low) ->",
    });
  });

  it("parses a parenthetical line", () => {
    expect(first("(quietly)\n")).toEqual({ kind: "parenthetical", text: "quietly" });
  });

  it("folds a multiline parenthetical into one token", () => {
    const src =
      "  (Argue about the city. Vex pitches reform; Praxis pitches order.\n   Pull individual audience members into your camp by talking\n   directly to them.)\n";
    const [lines] = scan(src);
    expect(lines).toHaveLength(1);
    const k = lines[0]!.kind;
    expect(k.kind).toBe("parenthetical");
    if (k.kind === "parenthetical") {
      expect(k.text).toBe(
        "Argue about the city. Vex pitches reform; Praxis pitches order. Pull individual audience members into your camp by talking directly to them.",
      );
    }
    expect(lines[0]!.startByte).toBe(2);
    expect(lines[0]!.endByte).toBe(src.replace(/\s+$/u, "").length);
  });

  it("does not let a balanced paren eat the next line", () => {
    const src = "(improv duration: 60s, advance on: quorum(8) [a, b])\nNARRATOR\n";
    const [lines] = scan(src);
    expect(lines).toHaveLength(2);
    expect(lines[0]!.kind.kind).toBe("parenthetical");
    expect(lines[1]!.kind.kind).toBe("speaker");
  });

  it("stops an unterminated paren at a dedent", () => {
    const src = "  (never closed paragraph that keeps going\n   and going\n== next_beat\n";
    const [lines] = scan(src);
    expect(lines).toHaveLength(2);
    expect(lines[1]!.kind.kind).toBe("knotMarker");
  });

  it("parses a let binding", () => {
    expect(first("let trusted = Wren.trusts.Player > 50\n")).toEqual({
      kind: "letBinding",
      name: "trusted",
      expression: "Wren.trusts.Player > 50",
    });
  });

  it("parses a scene heading", () => {
    expect(first("INT. LIGHTHOUSE - DAWN\n")).toEqual({
      kind: "sceneHeading",
      text: "INT. LIGHTHOUSE - DAWN",
    });
  });

  it("distinguishes inline vs block fences", () => {
    expect(first("```warn lx14```\n")).toEqual({
      kind: "fence",
      tail: "warn lx14",
      inlineClose: true,
    });
    expect(first("```note\n")).toEqual({ kind: "fence", tail: "note", inlineClose: false });
  });

  it("warns on tab indentation", () => {
    const [, diags] = scan("\tWREN\n");
    expect(diags.some((d) => d.code === Code.L1001TabIndent)).toBe(true);
  });

  it("makes a line comment invisible to the classifier", () => {
    const [lines, diags] = scan("// rough order: bell, beat\nWREN\n");
    expect(diags).toHaveLength(0);
    expect(lines).toHaveLength(1);
    expect(lines[0]!.kind.kind).toBe("speaker");
  });

  it("strips a trailing line comment from prose", () => {
    const [lines] = scan("It rang. // pickup pace here\n");
    expect(lines).toHaveLength(1);
    const k = lines[0]!.kind;
    expect(k.kind).toBe("prose");
    if (k.kind === "prose") expect(k.text).toBe("It rang.");
  });

  it("does not let a block comment eat the following speaker", () => {
    const [lines, diags] = scan("/* blocking sketch\nlives across lines */\nWREN\n");
    expect(diags).toHaveLength(0);
    expect(lines).toHaveLength(1);
    expect(lines[0]!.kind.kind).toBe("speaker");
  });

  it("diagnoses an unterminated block comment", () => {
    const [, diags] = scan("/* never closed\nstill open\n");
    expect(diags.some((d) => d.code === Code.L1007UnterminatedBlockComment)).toBe(true);
  });

  it("does not mistake a URL in prose for a comment", () => {
    const [lines] = scan("See https://example.com/path for details.\n");
    expect(lines).toHaveLength(1);
    const k = lines[0]!.kind;
    expect(k.kind === "prose" && k.text.includes("https://example.com/path")).toBe(true);
  });

  it("preserves a comment inside a fence", () => {
    const [lines] = scan("```note\n// stage manager: lights low\n```\n");
    expect(lines).toHaveLength(3);
    const k = lines[1]!.kind;
    expect(k.kind === "prose" && k.text.startsWith("//")).toBe(true);
  });

  it("drops blank lines", () => {
    const [lines] = scan("# T\n\n\nentry: x\n");
    expect(lines).toHaveLength(2);
    expect(lines[0]!.kind.kind).toBe("heading");
    expect(lines[1]!.kind.kind).toBe("property");
  });
});
