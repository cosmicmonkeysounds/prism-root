//! Ported from `loom-lsp` `completion.rs` `#[cfg(test)]`.

import { describe, expect, it } from "vitest";
import {
  isDirectivePosition,
  isDivertPosition,
  isIsPosition,
  isTraitArgPosition,
  ownerDivertPrefix,
} from "../src/lsp/index.ts";

describe("lsp completion position detection", () => {
  it("detects divert position", () => {
    expect(isDivertPosition("    -> rin")).toBe(true);
    expect(isDivertPosition("-> ")).toBe(true);
    expect(isDivertPosition("ringing")).toBe(false);
    // A dotted (owner-qualified) tail is NOT a plain divert position.
    expect(isDivertPosition("    -> self.")).toBe(false);
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

  it("detects trait-arg position inside `is Trait(`", () => {
    expect(isTraitArgPosition("CHARACTER C is Scanner(")).toBe(true);
    expect(isTraitArgPosition("CHARACTER C is Scanner(craw")).toBe(true);
    // Closed parens — no longer an open arg list.
    expect(isTraitArgPosition("CHARACTER C is Scanner(x)")).toBe(false);
    // A bare call in prose is not a trait application.
    expect(isTraitArgPosition("  visits(self.")).toBe(false);
    // Multi-trait `is A, B(` — the applied trait is the last one.
    expect(isTraitArgPosition("CHARACTER C is Algo, Scanner(")).toBe(true);
    // Prose/dialogue that merely contains `is Word(` must NOT trigger.
    expect(isTraitArgPosition("  WREN: my name is Bar(none of your")).toBe(false);
    expect(isTraitArgPosition("    the sky is Blue(ish")).toBe(false);
  });

  it("extracts the owner of a qualified divert tail", () => {
    expect(ownerDivertPrefix("    -> self.")).toBe("self");
    expect(ownerDivertPrefix("    -> self.rep")).toBe("self");
    expect(ownerDivertPrefix("    -> Alpha.report")).toBe("Alpha");
    // No dot → not an owner-qualified divert.
    expect(ownerDivertPrefix("    -> report")).toBeNull();
    expect(ownerDivertPrefix("  plain prose")).toBeNull();
  });
});
