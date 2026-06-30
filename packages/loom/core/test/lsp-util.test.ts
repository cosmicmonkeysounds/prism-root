//! Ported from `loom-lsp` `hover.rs` (`token_at`) and `references.rs`
//! (`find_token_spans`) `#[cfg(test)]` modules.

import { describe, expect, it } from "vitest";
import { findTokenSpans, tokenAt } from "../src/lsp/index.ts";

describe("lsp tokenAt", () => {
  it("finds the token in the middle of a line", () => {
    const found = tokenAt("    -> ringing", 10);
    expect(found).not.toBeNull();
    expect(found).toEqual(["ringing", 7, 14]);
  });

  it("returns null past end of line", () => {
    expect(tokenAt("-> x", 99)).toBeNull();
  });

  it("returns null on whitespace", () => {
    expect(tokenAt("-> ringing", 2)).toBeNull();
  });
});

describe("lsp findTokenSpans", () => {
  it("respects word boundaries", () => {
    const spans = findTokenSpans("WREN bell Wrenched WREN", "WREN");
    expect(spans).toEqual([
      [0, 4],
      [19, 23],
    ]);
  });

  it("returns nothing when absent", () => {
    expect(findTokenSpans("nothing here", "WREN")).toEqual([]);
  });
});
