import { describe, expect, it } from "vitest";
import { makePass, passOk, resolvePasscodes } from "../server/auth.ts";

const SPEAKABLE = /^[ABCDEFGHJKMNPQRSTUVWXYZ23456789]{6}$/;

describe("event passcodes", () => {
  it("makePass yields a 6-char speakable code (no 0/O/1/I/L)", () => {
    for (let i = 0; i < 100; i++) expect(makePass()).toMatch(SPEAKABLE);
  });

  it("makePass is overwhelmingly unique", () => {
    const seen = new Set(Array.from({ length: 100 }, () => makePass()));
    expect(seen.size).toBeGreaterThan(90);
  });

  it("passOk is trimmed + case-insensitive", () => {
    expect(passOk("k7pq3m", "K7PQ3M")).toBe(true);
    expect(passOk("  K7PQ3M ", "K7PQ3M")).toBe(true);
    expect(passOk("wrong", "K7PQ3M")).toBe(false);
    expect(passOk("", "K7PQ3M")).toBe(false);
  });

  it("resolvePasscodes prefers env, then persisted, then a fresh code", () => {
    expect(resolvePasscodes({ LOOM_EVENT_PASS: "EVT", LOOM_MOD_PASS: "MOD", LOOM_PRIME_PASS: "PRM" }, null)).toEqual({
      event: "EVT",
      mod: "MOD",
      prime: "PRM",
    });

    expect(resolvePasscodes({}, { event: "PE", mod: "PM", prime: "PP" })).toEqual({
      event: "PE",
      mod: "PM",
      prime: "PP",
    });

    // env wins over persisted; persisted fills gaps; the rest is freshly made.
    const mixed = resolvePasscodes({ LOOM_MOD_PASS: "OVERRIDE" }, { event: "KEEP" });
    expect(mixed.mod).toBe("OVERRIDE");
    expect(mixed.event).toBe("KEEP");
    expect(mixed.prime).toMatch(SPEAKABLE);
  });
});
