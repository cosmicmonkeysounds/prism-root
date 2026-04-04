import { describe, it, expect } from "vitest";
import { tokenize, isDigit, isIdentStart, isIdentChar } from "./scanner.js";

describe("character classification", () => {
  it("isDigit", () => {
    expect(isDigit("0")).toBe(true);
    expect(isDigit("9")).toBe(true);
    expect(isDigit("a")).toBe(false);
  });

  it("isIdentStart", () => {
    expect(isIdentStart("a")).toBe(true);
    expect(isIdentStart("_")).toBe(true);
    expect(isIdentStart("1")).toBe(false);
  });

  it("isIdentChar", () => {
    expect(isIdentChar("a")).toBe(true);
    expect(isIdentChar("1")).toBe(true);
    expect(isIdentChar("+")).toBe(false);
  });
});

describe("tokenize", () => {
  it("tokenizes integers", () => {
    const tokens = tokenize("42");
    expect(tokens[0]).toMatchObject({ kind: "NUMBER", numberValue: 42 });
  });

  it("tokenizes floats", () => {
    const tokens = tokenize("3.14");
    expect(tokens[0]).toMatchObject({ kind: "NUMBER", numberValue: 3.14 });
  });

  it("tokenizes leading dot float", () => {
    const tokens = tokenize(".5");
    expect(tokens[0]).toMatchObject({ kind: "NUMBER", numberValue: 0.5 });
  });

  it("tokenizes double-quoted strings", () => {
    const tokens = tokenize('"hello"');
    expect(tokens[0]).toMatchObject({ kind: "STRING", stringValue: "hello" });
  });

  it("tokenizes single-quoted strings", () => {
    const tokens = tokenize("'world'");
    expect(tokens[0]).toMatchObject({ kind: "STRING", stringValue: "world" });
  });

  it("handles escape sequences in strings", () => {
    const tokens = tokenize('"line\\nbreak"');
    expect(tokens[0]!.stringValue).toBe("line\nbreak");
  });

  it("tokenizes booleans", () => {
    const tokens = tokenize("true false");
    expect(tokens[0]).toMatchObject({ kind: "BOOL", boolValue: true });
    expect(tokens[1]).toMatchObject({ kind: "BOOL", boolValue: false });
  });

  it("tokenizes keywords", () => {
    const tokens = tokenize("and or not");
    expect(tokens[0]!.kind).toBe("AND");
    expect(tokens[1]!.kind).toBe("OR");
    expect(tokens[2]!.kind).toBe("NOT");
  });

  it("tokenizes identifiers", () => {
    const tokens = tokenize("foo bar_1");
    expect(tokens[0]).toMatchObject({ kind: "IDENT", raw: "foo" });
    expect(tokens[1]).toMatchObject({ kind: "IDENT", raw: "bar_1" });
  });

  it("tokenizes operands without subfield", () => {
    const tokens = tokenize("[var:health]");
    expect(tokens[0]).toMatchObject({
      kind: "OPERAND",
      operandData: { operandType: "var", id: "health" },
    });
  });

  it("tokenizes operands with subfield", () => {
    const tokens = tokenize("[obj:player.name]");
    expect(tokens[0]).toMatchObject({
      kind: "OPERAND",
      operandData: { operandType: "obj", id: "player", subfield: "name" },
    });
  });

  it("tokenizes two-char operators", () => {
    const tokens = tokenize("== != <= >=");
    expect(tokens.map((t) => t.kind)).toEqual(["EQ", "NEQ", "LTE", "GTE", "EOF"]);
  });

  it("tokenizes single-char operators", () => {
    const tokens = tokenize("+ - * / ^ % < > ( ) ,");
    const kinds = tokens.slice(0, -1).map((t) => t.kind);
    expect(kinds).toEqual([
      "PLUS", "MINUS", "STAR", "SLASH", "CARET", "PERCENT",
      "LT", "GT", "LPAREN", "RPAREN", "COMMA",
    ]);
  });

  it("tokenizes unknown characters", () => {
    const tokens = tokenize("@");
    expect(tokens[0]!.kind).toBe("UNKNOWN");
  });

  it("always ends with EOF", () => {
    const tokens = tokenize("");
    expect(tokens).toHaveLength(1);
    expect(tokens[0]!.kind).toBe("EOF");
  });

  it("skips whitespace", () => {
    const tokens = tokenize("  1  +  2  ");
    expect(tokens.filter((t) => t.kind !== "EOF")).toHaveLength(3);
  });

  it("tracks offset", () => {
    const tokens = tokenize("1 + 2");
    expect(tokens[0]!.offset).toBe(0);
    expect(tokens[1]!.offset).toBe(2);
    expect(tokens[2]!.offset).toBe(4);
  });

  it("tokenizes complex expression", () => {
    const tokens = tokenize("[var:hp] > 0 and not dead");
    const kinds = tokens.slice(0, -1).map((t) => t.kind);
    expect(kinds).toEqual(["OPERAND", "GT", "NUMBER", "AND", "NOT", "IDENT"]);
  });
});
