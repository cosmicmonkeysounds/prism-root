//! Structural edit API (port of `loom_parser::edit`).

import { describe, expect, it } from "vitest";
import {
  EditError,
  applyBeatProperty,
  applyEdits,
  applyInsertBeat,
  applyMoveBeat,
  applyRemoveBeat,
  parse,
  setBeatProperty,
} from "../src/parser/index.ts";

const STORY = `== opening
  cast: Wren
  setting: Lighthouse

A bell rope swings.

== ringing
  setting: Belfry

The bell tolls.
`;

describe("applyEdits", () => {
  it("splices non-overlapping edits over original offsets", () => {
    expect(
      applyEdits("hello world", [
        { start: 0, end: 5, replacement: "HI" },
        { start: 6, end: 11, replacement: "THERE" },
      ]),
    ).toBe("HI THERE");
  });

  it("rejects overlapping edits", () => {
    expect(() =>
      applyEdits("hello", [
        { start: 0, end: 3, replacement: "x" },
        { start: 2, end: 4, replacement: "y" },
      ]),
    ).toThrowError(EditError);
  });
});

describe("setBeatProperty", () => {
  it("rewrites only the changed line, preserving every other byte", () => {
    const out = applyBeatProperty(STORY, "opening", "setting", "Tower");
    expect(out).toBe(STORY.replace("  setting: Lighthouse", "  setting: Tower"));
  });

  it("is a no-op when the value already matches", () => {
    const [file] = parse(STORY);
    expect(setBeatProperty(STORY, file, "opening", "setting", "Lighthouse")).toEqual([]);
    expect(applyBeatProperty(STORY, "opening", "setting", "Lighthouse")).toBe(STORY);
  });

  it("inserts a new contract line after the last existing one", () => {
    const out = applyBeatProperty(STORY, "ringing", "cast", "Wren, Player");
    expect(out).toContain("== ringing\n  setting: Belfry\n  cast: Wren, Player\n");
    // The opening beat is untouched.
    expect(out).toContain("== opening\n  cast: Wren\n  setting: Lighthouse\n");
  });

  it("throws for an unknown beat", () => {
    expect(() => applyBeatProperty(STORY, "nope", "cast", "X")).toThrowError(EditError);
  });

  it("round-trips: the edited source reparses clean", () => {
    const out = applyBeatProperty(STORY, "ringing", "cast", "Wren");
    const [, diags] = parse(out);
    expect(diags).toEqual([]);
  });
});

describe("moveBeat", () => {
  it("reorders beats while keeping their bodies intact", () => {
    const out = applyMoveBeat(STORY, "ringing", "before", "opening");
    expect(out.indexOf("== ringing")).toBeLessThan(out.indexOf("== opening"));
    expect(out).toContain("The bell tolls.");
    expect(out).toContain("A bell rope swings.");
    const [, diags] = parse(out);
    expect(diags).toEqual([]);
  });

  it("is a no-op when moving before its own successor", () => {
    expect(applyMoveBeat(STORY, "opening", "before", "ringing")).toBe(STORY);
  });
});

describe("insertBeat / removeBeat", () => {
  it("inserts a new empty beat at the end", () => {
    const out = applyInsertBeat(STORY, "coda", "end", "");
    expect(out).toContain("== coda");
    expect(out.indexOf("== coda")).toBeGreaterThan(out.indexOf("== ringing"));
    const [file] = parse(out);
    expect(file.items).toHaveLength(3);
  });

  it("removes a beat block whole", () => {
    const out = applyRemoveBeat(STORY, "opening");
    expect(out).not.toContain("== opening");
    expect(out).not.toContain("A bell rope swings.");
    expect(out).toContain("== ringing");
    const [file] = parse(out);
    expect(file.items).toHaveLength(1);
  });

  it("throws removing an unknown beat", () => {
    expect(() => applyRemoveBeat(STORY, "ghost")).toThrowError(EditError);
  });
});
