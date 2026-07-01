import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { afterAll, describe, expect, it } from "vitest";

import { EventRuntime } from "../server/event-runtime.ts";
import { EventRegistry } from "../server/registry.ts";
import { Store } from "../server/store.ts";
import type { Passcodes } from "../server/auth.ts";

const dirs: string[] = [];
function freshStore(): Store {
  const d = mkdtempSync(join(tmpdir(), "loom-reg-"));
  dirs.push(d);
  return new Store(d);
}
afterAll(() => {
  for (const d of dirs) rmSync(d, { recursive: true, force: true });
});

function runtime(eventId: string, codes: Passcodes): EventRuntime {
  return new EventRuntime({
    eventId,
    store: freshStore(),
    codes,
    scenarioName: "demo",
    scenarioSource: "",
    joinBase: () => "http://localhost",
  });
}

describe("EventRegistry — code resolution", () => {
  const reg = new EventRegistry(tmpdir(), () => "http://localhost");
  reg.register(runtime("evt-a", { event: "AAAA11", prime: "AAAA22", mod: "AAAA33" }));
  reg.register(runtime("evt-b", { event: "BBBB11", prime: "BBBB22", mod: "BBBB33" }));

  it("maps an event code to that event as a guest", () => {
    expect(reg.resolveCode("AAAA11")).toEqual({ eventId: "evt-a", role: "guest" });
    expect(reg.resolveCode("BBBB11")).toEqual({ eventId: "evt-b", role: "guest" });
  });

  it("maps prime / mod codes to the performer / moderator roles", () => {
    expect(reg.resolveCode("AAAA22")).toEqual({ eventId: "evt-a", role: "prime" });
    expect(reg.resolveCode("BBBB33")).toEqual({ eventId: "evt-b", role: "mod" });
  });

  it("is trimmed + case-insensitive (matching passOk)", () => {
    expect(reg.resolveCode("  aaaa11 ")).toEqual({ eventId: "evt-a", role: "guest" });
  });

  it("returns null for an unknown or empty code", () => {
    expect(reg.resolveCode("ZZZZ99")).toBeNull();
    expect(reg.resolveCode("")).toBeNull();
    expect(reg.resolveCode("   ")).toBeNull();
  });

  it("get / has / all reflect what's registered; stop removes", () => {
    expect(reg.has("evt-a")).toBe(true);
    expect(reg.get("evt-a")?.eventId).toBe("evt-a");
    expect(reg.all().map((r) => r.eventId).sort()).toEqual(["evt-a", "evt-b"]);
    reg.stop("evt-b");
    expect(reg.has("evt-b")).toBe(false);
    expect(reg.resolveCode("BBBB11")).toBeNull();
  });
});
