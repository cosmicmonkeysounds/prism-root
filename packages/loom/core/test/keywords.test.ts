import { describe, expect, it } from "vitest";
import {
  BUILTIN_DIRECTIVES,
  DECLARATIONS,
  SYNTACTIC_DIRECTIVES,
  declarationKeyword,
  declarationKind,
  declarationsWithKind,
} from "../src/parser/index.ts";

describe("keywords", () => {
  it("round-trips declarations through the AST", () => {
    for (const word of DECLARATIONS) {
      const kind = declarationKind(word);
      expect(kind).not.toBeNull();
      expect(declarationKeyword(kind!)).toBe(word);
    }
  });

  it("keeps the declaration table in lockstep with the AST", () => {
    expect(declarationsWithKind().length).toBe(DECLARATIONS.length);
  });

  it("has non-empty directive lists", () => {
    expect(SYNTACTIC_DIRECTIVES.length).toBeGreaterThan(0);
    expect(BUILTIN_DIRECTIVES.length).toBeGreaterThan(0);
  });
});
