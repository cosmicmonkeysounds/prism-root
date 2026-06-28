import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { Sim } from "../src/runtime/sim/index.ts";
import type { SimEvent } from "../src/runtime/sim/index.ts";

const SCENARIO = readFileSync(
  new URL("../examples/escape-the-internet.loom", import.meta.url),
  "utf8",
);

function fresh(): Sim {
  return Sim.fromSources(SCENARIO);
}

function dialogue(events: SimEvent[]): string[] {
  return events.filter((e) => e.type === "dialogue").map((e) => (e as { text: string }).text);
}
function broadcasts(events: SimEvent[]): Array<{ cue: string; audience: string[] }> {
  return events
    .filter((e) => e.type === "broadcast")
    .map((e) => e as { cue: string; audience: string[] });
}

describe("Escape the Internet — model compile", () => {
  it("indexes every entity kind from the .loom source", () => {
    const sim = fresh();
    expect([...sim.model.factions.keys()].sort()).toEqual(["Chatters", "Mods", "TheAlgorithm"]);
    expect(sim.model.factions.get("TheAlgorithm")!.hidden).toBe(true);
    expect(sim.model.factions.get("Mods")!.rival).toBe("Chatters");
    expect(sim.model.locations.get("Internet")!.prison).toBe(true);
    expect(sim.model.locations.get("Party")!.prison).toBe(false);
    expect([...sim.model.characters.keys()].sort()).toEqual([
      "ModBot",
      "Moderator_Prime",
      "Recruiter",
      "Sentinel",
      "TheAdmin",
    ]);
    expect(sim.model.defaultRole).toBe("Guest");
    expect(sim.model.entry).toBe("doors_open");
    // The scanner characters carry `on scan` hooks; TheAdmin reacts to captures.
    expect(sim.model.characters.get("ModBot")!.hooks[0]!.verb).toBe("scan");
    expect(sim.model.roles.get("Guest")!.hooks.map((h) => h.verb).sort()).toEqual([
      "captured",
      "escape",
      "scan",
    ]);
  });
});

describe("Escape the Internet — accounts & factions", () => {
  it("casts a fresh party-goer as a Guest with default state", () => {
    const sim = fresh();
    sim.createPerson("g1", "Alice");
    expect(sim.persons.get("g1")!.role).toBe("Guest");
    expect(sim.scoreOf("g1")).toBe(0);
    expect(sim.isCaptured("g1")).toBe(false);
    expect(sim.factionOf("g1")).toBeNull();
  });

  it("joins and defects between factions, keeping membership in sync", () => {
    const sim = fresh();
    sim.createPerson("g1", "Alice");
    sim.join("g1", "Chatters");
    expect(sim.factionOf("g1")).toBe("Chatters");
    expect(sim.factionMembers("Chatters")).toEqual(["g1"]);

    sim.defect("g1", "Mods");
    expect(sim.factionOf("g1")).toBe("Mods");
    expect(sim.factionMembers("Chatters")).toEqual([]);
    expect(sim.factionMembers("Mods")).toEqual(["g1"]);
  });
});

describe("Escape the Internet — QR scans drive character relationships", () => {
  it("a Mod moderator greets a fellow Mod and rewards them", () => {
    const sim = fresh();
    sim.createPerson("g1", "Alice");
    sim.join("g1", "Mods");
    const events = sim.scan("Moderator_Prime", "g1");
    expect(dialogue(events).join(" ")).toContain("A fellow Mod");
    expect(sim.scoreOf("g1")).toBe(50);
    // trust seeded at 50, +10 for a fellow Mod.
    expect(sim.world.get("Moderator_Prime.trusts.g1")).toEqual({ kind: "number", value: 60 });
  });

  it("a Mod moderator warns a Chatter and loses trust in them", () => {
    const sim = fresh();
    sim.createPerson("g2", "Bob");
    sim.join("g2", "Chatters");
    const events = sim.scan("Moderator_Prime", "g2");
    expect(dialogue(events).join(" ")).toContain("A Chatter");
    expect(sim.scoreOf("g2")).toBe(0);
    expect(sim.world.get("Moderator_Prime.trusts.g2")).toEqual({ kind: "number", value: 40 });
  });
});

describe("Escape the Internet — capture & escape", () => {
  it("the villain prop captures a guest, cascading hooks fire", () => {
    const sim = fresh();
    sim.createPerson("g3", "Cara");
    sim.join("g3", "Chatters");
    const events = sim.scan("ModBot", "g3");

    // Physically moved into the Internet prison, flagged captured.
    expect(sim.locationOf("g3")).toBe("Internet");
    expect(sim.isCaptured("g3")).toBe(true);
    // ROLE Guest `on captured` docked 25 points.
    expect(sim.scoreOf("g3")).toBe(-25);
    // TheAdmin reacted: counter + a doom broadcast to the captured guest.
    expect(sim.world.get("TheAdmin.captures")).toEqual({ kind: "number", value: 1 });
    const doom = broadcasts(events).find((b) => b.cue === "doomed");
    expect(doom).toBeTruthy();
    expect(doom!.audience).toEqual(["g3"]);
  });

  it("a captured guest escapes back to the party and is rewarded", () => {
    const sim = fresh();
    sim.createPerson("g4", "Dee");
    sim.scan("ModBot", "g4"); // captured → score -25
    const events = sim.escape("g4");

    expect(sim.isCaptured("g4")).toBe(false);
    expect(sim.locationOf("g4")).toBe("Party");
    expect(sim.scoreOf("g4")).toBe(75); // -25 + 100
    const freedom = broadcasts(events).find((b) => b.cue === "freedom");
    expect(freedom).toBeTruthy();
    expect(freedom!.audience).toEqual(["g4"]);
  });
});

describe("Escape the Internet — broadcast scopes", () => {
  it("a faction broadcast reaches every member of that faction", () => {
    const sim = fresh();
    sim.createPerson("a", "A");
    sim.createPerson("b", "B");
    sim.createPerson("c", "C");
    sim.join("a", "Mods");
    sim.join("b", "Mods");
    sim.join("c", "Chatters");
    expect(sim.audienceFor("faction(Mods)").sort()).toEqual(["a", "b"]);
    expect(sim.audienceFor("faction(Chatters)")).toEqual(["c"]);
    expect(sim.audienceFor("participant(a) | participant(c)").sort()).toEqual(["a", "c"]);
  });
});

describe("Escape the Internet — verification-driven hardening", () => {
  it("fires a character reaction to a global cue (null-param hook)", () => {
    const sim = fresh();
    sim.createPerson("g1", "A");
    sim.createPerson("g2", "B");
    sim.join("g1", "Mods");
    sim.join("g2", "Chatters");
    const events = sim.signal("lockdown");
    const siren = broadcasts(events).find((b) => b.cue === "lockdown_siren");
    expect(siren).toBeTruthy();
    expect(siren!.audience.sort()).toEqual(["g1", "g2"]);
  });

  it("routes a live-participant choice through choose()", () => {
    const sim = fresh();
    sim.createPerson("g1", "A");
    const prompted = sim.scan("Recruiter", "g1");
    const prompt = prompted.find((e) => e.type === "choicePrompted");
    expect(prompt).toBeTruthy();
    expect((prompt as { person: string }).person).toBe("g1");
    expect(sim.pendingChoiceFor("g1")).toEqual(["Join the Chatters", "Stay loyal"]);
    // Option 0 joins the Chatters.
    sim.choose("g1", 0);
    expect(sim.factionOf("g1")).toBe("Chatters");
    expect(sim.pendingChoiceFor("g1")).toBeNull();
  });

  it("keeps the villain faction hidden until revealed by enough captures", () => {
    const sim = fresh();
    sim.createPerson("admin", "Operator", "Guest");
    // The hidden faction reads as null to the app while secret...
    sim.join("admin", "TheAlgorithm");
    expect(sim.factionOf("admin")).toBe("TheAlgorithm");
    expect(sim.publicFactionOf("admin")).toBeNull();
    expect(sim.factionRevealed("TheAlgorithm")).toBe(false);
    // ...two captures trip TheAdmin's reveal.
    sim.createPerson("v1", "V1");
    sim.createPerson("v2", "V2");
    sim.scan("ModBot", "v1");
    sim.scan("ModBot", "v2");
    expect(sim.factionRevealed("TheAlgorithm")).toBe(true);
    expect(sim.publicFactionOf("admin")).toBe("TheAlgorithm");
  });

  it("expresses betrayal as a hidden true-allegiance flip", () => {
    const sim = fresh();
    sim.createPerson("g1", "A");
    sim.join("g1", "Mods");
    const events = sim.betray("g1", "TheAlgorithm");
    // Displayed faction unchanged; true allegiance flipped.
    expect(sim.factionOf("g1")).toBe("Mods");
    expect(sim.trueFactionOf("g1")).toBe("TheAlgorithm");
    expect(events.some((e) => e.type === "betrayed")).toBe(true);
  });

  it("is idempotent: re-capture and false escape don't farm score", () => {
    const sim = fresh();
    sim.createPerson("g1", "A");
    sim.scan("ModBot", "g1"); // -25
    expect(sim.scoreOf("g1")).toBe(-25);
    sim.scan("ModBot", "g1"); // already captured → no-op
    expect(sim.scoreOf("g1")).toBe(-25);
    expect(sim.world.get("TheAdmin.captures")).toEqual({ kind: "number", value: 1 });
    sim.escape("g1"); // +100 → 75
    expect(sim.scoreOf("g1")).toBe(75);
    sim.escape("g1"); // not captured → no reward
    expect(sim.scoreOf("g1")).toBe(75);
  });

  it("supports peer (person-to-person) QR scans via ROLE hooks", () => {
    const sim = fresh();
    sim.createPerson("g1", "A");
    sim.createPerson("g2", "B");
    const events = sim.scan("g1", "g2");
    const ping = broadcasts(events).find((b) => b.cue === "peer_ping");
    expect(ping).toBeTruthy();
    expect(ping!.audience).toEqual(["g2"]);
  });

  it("broadcasts to the triggering guest's own faction via expression scope", () => {
    const sim = fresh();
    sim.createPerson("g1", "A");
    sim.join("g1", "Chatters");
    sim.createPerson("g2", "B");
    sim.join("g2", "Chatters");
    expect(sim.audienceFor("faction(g1.faction)").sort()).toEqual(["g1", "g2"]);
  });

  it("treats a bareword enum value as a string, not a missing path", () => {
    const sim = Sim.fromSources(
      "ROLE Guest\n  state: any\n  on account_created\n    <set: self.state = loyal>\n",
    );
    sim.createPerson("g1", "A");
    expect(sim.world.get("g1.state")).toEqual({ kind: "string", value: "loyal" });
  });
});

describe("Escape the Internet — choice continuations (re-verify round 2)", () => {
  const CHOICE_SCN = `ROLE Guest
  score: 0 to 1000 = 0

CHARACTER R
  on scan guest
    -> menu
    <set: guest.score += 5>

== menu(guest)
  RECRUITER
    Pick a side.
  * A
    <set: guest.score += 1>
  * B
    <set: guest.score += 2>
  <set: guest.score += 100>
`;

  it("suspends across the call stack and resumes the FULL continuation", () => {
    const sim = Sim.fromSources(CHOICE_SCN);
    sim.createPerson("g1", "A");
    sim.scan("R", "g1");
    // Suspended at the menu: nothing past the prompt has run yet —
    // not the post-menu set, not the enclosing hook's trailing set.
    expect(sim.scoreOf("g1")).toBe(0);
    expect(sim.pendingChoiceFor("g1")).toEqual(["A", "B"]);
    sim.choose("g1", 0);
    // option A (+1) → post-menu continuation (+100) → hook continuation (+5).
    expect(sim.scoreOf("g1")).toBe(106);
    expect(sim.pendingChoiceFor("g1")).toBeNull();
  });

  it("does not destroy the prompt on a fat-fingered index", () => {
    const sim = Sim.fromSources(CHOICE_SCN);
    sim.createPerson("g1", "A");
    sim.scan("R", "g1");
    sim.choose("g1", 99); // out of range
    expect(sim.scoreOf("g1")).toBe(0);
    expect(sim.pendingChoiceFor("g1")).toEqual(["A", "B"]); // still answerable
    sim.choose("g1", 1); // option B (+2) + 100 + 5
    expect(sim.scoreOf("g1")).toBe(107);
  });

  it("queues two prompts for one person instead of clobbering", () => {
    const sim = Sim.fromSources(`CHARACTER X
  on alarm guest
    -> menuX
CHARACTER Y
  on alarm guest
    -> menuY

== menuX(guest)
  * ax
== menuY(guest)
  * ay
`);
    sim.createPerson("g1", "A");
    const events = sim.signal("alarm", "g1");
    expect(events.filter((e) => e.type === "choicePrompted")).toHaveLength(2);
    expect(sim.pendingChoiceFor("g1")).toEqual(["ax"]);
    sim.choose("g1", 0);
    expect(sim.pendingChoiceFor("g1")).toEqual(["ay"]); // the second survived
    sim.choose("g1", 0);
    expect(sim.pendingChoiceFor("g1")).toBeNull();
  });

  it("seeds a character's own faction so self.faction comparisons work", () => {
    const sim = Sim.fromSources(`FACTION Mods
FACTION Chatters

ROLE Guest
  score: 0 to 1000 = 0

CHARACTER Ally
  faction: Mods
  on scan guest
    <if: guest.faction == self.faction>
      <set: guest.score += 7>
`);
    expect(sim.world.get("Ally.faction")).toEqual({ kind: "string", value: "Mods" });
    sim.createPerson("g1", "A");
    sim.join("g1", "Mods");
    sim.scan("Ally", "g1");
    expect(sim.scoreOf("g1")).toBe(7); // same faction → branch fired
    sim.createPerson("g2", "B");
    sim.join("g2", "Chatters");
    sim.scan("Ally", "g2");
    expect(sim.scoreOf("g2")).toBe(0); // different faction → no reward
  });

  it("fires enters symmetrically when a guest is captured into a location", () => {
    const sim = Sim.fromSources(`LOCATION Party
LOCATION Internet
  prison: true

ROLE Guest
  on enters Internet
    <broadcast: jailed to participant(self)>

CHARACTER ModBot
  on scan guest
    <capture: guest into Internet>
`);
    sim.createPerson("g1", "A");
    const events = sim.scan("ModBot", "g1");
    const jailed = broadcasts(events).find((b) => b.cue === "jailed");
    expect(jailed).toBeTruthy();
    expect(jailed!.audience).toEqual(["g1"]);
  });

  it("fires a ROLE hook that declares a param on a non-scan trigger", () => {
    const sim = Sim.fromSources(`LOCATION Party
LOCATION Internet
  prison: true

ROLE Guest
  score: 0 to 1000 = 0
  on captured victim
    <set: victim.score -= 5>

CHARACTER ModBot
  on scan guest
    <capture: guest into Internet>
`);
    sim.createPerson("g1", "A");
    sim.scan("ModBot", "g1");
    expect(sim.scoreOf("g1")).toBe(-5);
  });
});
