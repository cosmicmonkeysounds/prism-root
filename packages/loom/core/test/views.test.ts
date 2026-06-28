import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { Sim } from "../src/runtime/sim/index.ts";
import { guestView, modView, primeView } from "../server/views.ts";

const SCENARIO = readFileSync(
  new URL("../examples/escape-the-internet.loom", import.meta.url),
  "utf8",
);

describe("server views", () => {
  it("projects a guest's self-view with a masked hidden faction", () => {
    const sim = Sim.fromSources(SCENARIO);
    sim.createPerson("g1", "Alice");
    sim.join("g1", "TheAlgorithm"); // a secret allegiance
    const v = guestView(sim, "g1");
    expect(v.name).toBe("Alice");
    expect(v.role).toBe("Guest");
    expect(v.faction).toBeNull(); // hidden faction reads null to the app
    expect(v.score).toBe(0);
    expect(v.captured).toBe(false);
  });

  it("surfaces a pending choice in the guest view", () => {
    const sim = Sim.fromSources(SCENARIO);
    sim.createPerson("g1", "Alice");
    sim.scan("Recruiter", "g1");
    expect(guestView(sim, "g1").pendingChoice).toEqual(["Join the Chatters", "Stay loyal"]);
  });

  it("gives the moderator the true god-view including hidden allegiances", () => {
    const sim = Sim.fromSources(SCENARIO);
    sim.createPerson("g1", "Alice");
    sim.join("g1", "Mods");
    sim.createPerson("g2", "Bob");
    sim.join("g2", "Chatters");
    const v = modView(sim, "open", "escape-the-internet");
    expect(v.phase).toBe("open");
    expect(v.roster).toHaveLength(2);
    expect(v.roster.find((r) => r.id === "g1")!.faction).toBe("Mods");
    const mods = v.factions.find((f) => f.id === "Mods")!;
    expect(mods.members).toEqual(["g1"]);
    const algo = v.factions.find((f) => f.id === "TheAlgorithm")!;
    expect(algo.hidden).toBe(true);
    expect(algo.revealed).toBe(false);
    expect(v.locations.find((l) => l.id === "Internet")!.prison).toBe(true);
    expect(v.characters).toContain("Moderator_Prime");
  });

  it("gives a performer their part and the scannable guests", () => {
    const sim = Sim.fromSources(SCENARIO);
    sim.createPerson("g1", "Alice");
    const v = primeView(sim, "Moderator_Prime");
    expect(v.character).toBe("Moderator_Prime");
    expect(v.faction).toBe("Mods");
    expect(v.guests.map((g) => g.id)).toEqual(["g1"]);
  });

  it("returns empty views when no scenario is loaded", () => {
    const v = modView(null, "idle", null);
    expect(v.roster).toEqual([]);
    expect(v.characters).toEqual([]);
    expect(primeView(null, "X").guests).toEqual([]);
  });
});
