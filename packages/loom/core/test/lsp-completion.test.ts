//! Ported from `loom-lsp` `completion.rs` `#[cfg(test)]`.

import { describe, expect, it } from "vitest";
import { isDirectivePosition, isDivertPosition, isIsPosition } from "../src/lsp/index.ts";

describe("lsp completion position detection", () => {
  it("detects divert position", () => {
    expect(isDivertPosition("    -> rin")).toBe(true);
    expect(isDivertPosition("-> ")).toBe(true);
    expect(isDivertPosition("ringing")).toBe(false);
  });

  it("detects directive position", () => {
    expect(isDirectivePosition("    <sf")).toBe(true);
    expect(isDirectivePosition("<sfx: bell>")).toBe(false);
  });

  it("detects is position", () => {
    expect(isIsPosition("is Wren")).toBe(true);
    expect(isIsPosition("    is ")).toBe(true);
    expect(isIsPosition("    is<sfx>")).toBe(false);
  });
});
