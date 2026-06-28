import { describe, expect, it } from "vitest";
import { stripComments } from "../src/parser/index.ts";
import { Code } from "../src/parser/index.ts";

describe("comments::strip", () => {
  it("strips line comment to EOL", () => {
    const [out, diags] = stripComments("hello // tail\nworld\n");
    expect(diags).toHaveLength(0);
    expect(out).toBe("hello        \nworld\n");
  });

  it("preserves newlines in block comments", () => {
    const [out, diags] = stripComments("a /* one\ntwo */ b\n");
    expect(diags).toHaveLength(0);
    expect(out).toBe("a       \n       b\n");
  });

  it("diagnoses an unterminated block", () => {
    const [, diags] = stripComments("a /* open and never closed\nstill open\n");
    expect(diags).toHaveLength(1);
    expect(diags[0]!.code).toBe(Code.L1007UnterminatedBlockComment);
  });

  it("keeps slashes in URLs safe", () => {
    const [out, diags] = stripComments("see https://example.com/path\n");
    expect(diags).toHaveLength(0);
    expect(out).toBe("see https://example.com/path\n");
  });

  it("preserves comments inside a fence", () => {
    const src = "```note\n// not a comment here\n/* also fine */\n```\n";
    const [out, diags] = stripComments(src);
    expect(diags).toHaveLength(0);
    expect(out).toBe(src);
  });

  it("handles an inline fence", () => {
    const [out] = stripComments("```warn lx14```\n// real comment\nWREN\n");
    expect(out).toBe("```warn lx14```\n               \nWREN\n");
  });

  it("replaces non-ASCII inside a comment with spaces", () => {
    const [out] = stripComments("/* café */ end\n");
    expect(out.length).toBe("/* café */ end\n".length);
    expect(out.endsWith(" end\n")).toBe(true);
  });
});
