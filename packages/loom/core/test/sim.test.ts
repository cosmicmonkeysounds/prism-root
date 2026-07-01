import { describe, expect, it } from "vitest";
import { Sim } from "../src/runtime/sim/index.ts";
import type { SimEvent } from "../src/runtime/sim/index.ts";
import { scenarioFiles, scenarioSource } from "../examples/load.ts";

const SCENARIO = scenarioSource("escape-the-internet");

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
    // Two public factions, two hidden ones (the villain + the resistance).
    expect([...sim.model.factions.keys()].sort()).toEqual([
      "Chatters",
      "Glitchers",
      "Mods",
      "TheAlgorithm",
    ]);
    expect(sim.model.factions.get("TheAlgorithm")!.hidden).toBe(true);
    expect(sim.model.factions.get("Glitchers")!.hidden).toBe(true);
    expect(sim.model.factions.get("Mods")!.rival).toBe("Chatters");
    expect(sim.model.locations.get("Internet")!.prison).toBe(true);
    expect(sim.model.locations.get("Servers")!.prison).toBe(true);
    expect(sim.model.locations.get("Party")!.prison).toBe(false);
    expect([...sim.model.characters.keys()].sort()).toEqual([
      "Ad_Popup",
      "Captcha",
      "Comment_Section",
      "Cookie_Banner",
      "Crawler",
      "DJ",
      "Download_Station",
      "Firewall_Terminal",
      "Glitch",
      "Hacker_Zero",
      "Leaderboard",
      "Like_Button",
      "ModBot",
      "Mod_Karen",
      "Moderator_Prime",
      "Newscaster",
      "Notification_Bell",
      "Paywall",
      "Profile_Mirror",
      "Recruiter",
      "Recycle_Bin",
      "Search_Oracle",
      "Sentinel",
      "Surveillance",
      "Sysadmin",
      "Terminal",
      "TheAdmin",
      "The_Banned",
      "Troll_King",
      "VPN_Node",
      "Verified_Vera",
    ]);
    expect(sim.model.defaultRole).toBe("Guest");
    expect(sim.model.entry).toBe("doors_open");
    // The scanner characters carry `on scan` hooks; TheAdmin reacts to captures.
    expect(sim.model.characters.get("ModBot")!.hooks[0]!.verb).toBe("scan");
    expect(sim.model.roles.get("Guest")!.hooks.map((h) => h.verb).sort()).toEqual([
      "betray",
      "captured",
      "defect",
      "enters",
      "escape",
      "scan",
    ]);
  });

  it("compiles the same world from the multi-file bundle as from the concatenation", () => {
    // The project is authored across many files; bundling them separately
    // (the real `Bundle` multi-file path) must yield the identical model.
    const bundle = Sim.fromSources(...scenarioFiles("escape-the-internet"));
    expect([...bundle.model.characters.keys()].sort()).toEqual(
      [...fresh().model.characters.keys()].sort(),
    );
    expect(bundle.model.entry).toBe("doors_open");
    expect(bundle.model.defaultRole).toBe("Guest");
    // A divert target declared in `beats/algorithm.loom` resolves even though
    // the hook that reaches it lives in `cast/algorithm.loom`.
    expect(bundle.model.beats.has("lockdown")).toBe(true);
    expect(bundle.model.gens.map((g) => g.id)).toContain("FeedHum");
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

describe("Escape the Internet — functional redesign (traits / owned beats / SELF)", () => {
  it("resolves a badge trait's faction onto every scanner prop", () => {
    const sim = fresh();
    // `is AlgoScanner(beat)` factors the faction badge in from `cast/kit.loom`.
    expect(sim.model.characters.get("Crawler")!.faction).toBe("TheAlgorithm");
    expect(sim.model.characters.get("Moderator_Prime")!.faction).toBe("Mods");
    expect(sim.model.characters.get("Glitch")!.faction).toBe("Glitchers");
    // A neutral prop uses the bare router — no faction.
    expect(sim.model.characters.get("Recycle_Bin")!.faction).toBeNull();
  });

  it("runs an owned beat via `-> self.beat` and speaks it as the owner", () => {
    const sim = fresh();
    sim.createPerson("g1", "Ada");
    // Firewall_Terminal owns `firewall`, keyed Owner.name and reached by the
    // inline `on scan guest -> self.firewall`.
    expect(sim.model.beats.has("Firewall_Terminal.firewall")).toBe(true);
    expect(sim.model.beats.has("firewall")).toBe(false); // no longer global
    const ev = sim.scan("Firewall_Terminal", "g1");
    // Free guest: the wall hums, spoken as FIREWALL_TERMINAL (SELF).
    const line = ev.find((e) => e.type === "dialogue") as { speaker: string; text: string };
    expect(line.speaker).toBe("FIREWALL_TERMINAL");
    expect(line.text).toContain("The firewall hums");
    // The `<else>` branch raised heat and funnelled to the still-global lockdown.
    expect(sim.world.get("g1.heat")).toEqual({ kind: "number", value: 15 });
  });

  it("keeps TheAdmin's tally on self.captures (seeded to 0)", () => {
    const sim = fresh();
    expect(sim.world.get("TheAdmin.captures")).toEqual({ kind: "number", value: 0 });
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
    // A room-wide `blackout` the operator triggers from the booth — the DJ
    // (a `self`-only `on blackout` hook) rallies both public factions. This
    // is the one legitimate global cue; the Algorithm's lockdowns are never
    // broadcast to the whole room.
    const sim = fresh();
    sim.createPerson("g1", "A");
    sim.createPerson("g2", "B");
    sim.join("g1", "Mods");
    sim.join("g2", "Chatters");
    const events = sim.signal("blackout");
    const siren = broadcasts(events).find((b) => b.cue === "lights_out");
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

describe("Escape the Internet — the clock (autonomous life)", () => {
  function ambient(events: SimEvent[]): string[] {
    return events.filter((e) => e.type === "ambient").map((e) => (e as { text: string }).text);
  }
  /** Only FeedHum's barks — several ambient generators run concurrently. */
  function feedHum(events: SimEvent[]): string[] {
    return events
      .filter((e) => e.type === "ambient" && (e as { source: string }).source === "FeedHum")
      .map((e) => (e as { text: string }).text);
  }

  it("emits ambient generator barks only on the interval", () => {
    const sim = fresh();
    expect(ambient(sim.tick(19000))).toHaveLength(0); // under every generator's interval
    const evs = sim.tick(2000); // crosses 20s — only FeedHum is due
    expect(feedHum(evs)).toHaveLength(1);
    expect((evs.find((e) => e.type === "ambient") as { source: string }).source).toBe("FeedHum");
  });

  it("cycles bark text deterministically through the generator", () => {
    const sim = fresh();
    const texts: string[] = [];
    for (let i = 0; i < 4; i++) texts.push(...feedHum(sim.tick(20000)));
    expect(texts).toHaveLength(4);
    expect(new Set(texts).size).toBe(4); // four distinct barks, in order
  });

  it("exposes Time.* on the world clock", () => {
    const sim = fresh();
    sim.tick(65000);
    expect(sim.world.get("Time.minute")).toEqual({ kind: "number", value: 1 });
    expect(sim.elapsed()).toBe(65000);
  });

  it("runs a time-driven hook on tick but never via an ordinary signal", () => {
    const sim = fresh();
    const before = sim.log.len();
    // The clock is pure bookkeeping now — no ambient threat, no auto-lockdown.
    sim.signal("every"); // must not trip the `on every 60s` sweep hook
    expect(sim.world.peek("TheAlgorithm.sweeps")).toBeNull();
    expect(sim.log.since(before).filter((e) => e.type === "ambient")).toHaveLength(0);
    // Only the tick advances it.
    sim.tick(60000);
    expect(sim.world.get("TheAlgorithm.sweeps")).toEqual({ kind: "number", value: 1 });
  });
});

describe("Escape the Internet — targeted lockdown (anger the Algorithm)", () => {
  it("locks down only the guest who angered the Algorithm, never the room", () => {
    const sim = fresh();
    sim.createPerson("g1", "A");
    sim.createPerson("g2", "B");
    // One flag from the Crawler is a warning (heat 40 < 75) — nobody jailed.
    sim.scan("Crawler", "g1");
    expect(sim.isCaptured("g1")).toBe(false);
    expect(sim.world.get("g1.heat")).toEqual({ kind: "number", value: 40 });
    // A second flag tips g1's *own* heat over the line → personal lockdown.
    sim.scan("Crawler", "g1");
    expect(sim.isCaptured("g1")).toBe(true);
    expect(sim.locationOf("g1")).toBe("Internet");
    expect(sim.world.get("g1.heat")).toEqual({ kind: "number", value: 0 }); // reset on lockdown
    // g2 angered nobody beyond one flag → still free. No ambient lockdown.
    sim.scan("Crawler", "g2");
    expect(sim.isCaptured("g2")).toBe(false);
    expect(sim.locationOf("g2")).not.toBe("Internet");
  });

  it("drags a severe offender straight into the deep Servers", () => {
    const sim = fresh();
    sim.createPerson("g1", "A");
    sim.scan("Terminal", "g1");
    expect(sim.pendingChoiceFor("g1")).toContain("Leak the Algorithm's source code");
    sim.choose("g1", 2); // leak the source — heat 95 ≥ 90
    expect(sim.isCaptured("g1")).toBe(true);
    expect(sim.locationOf("g1")).toBe("Servers");
  });

  it("offers a captured guest the Glitchers' exploit, revealing the resistance", () => {
    const sim = fresh();
    sim.createPerson("g1", "A");
    sim.scan("ModBot", "g1"); // jailed in the Internet
    expect(sim.isCaptured("g1")).toBe(true);
    sim.scan("Glitch", "g1"); // the resistance contact offers a way out
    expect(sim.pendingChoiceFor("g1")).toEqual(["Take the exploit — run for it", "Not yet. Lay low."]);
    expect(sim.factionRevealed("Glitchers")).toBe(false);
    sim.choose("g1", 0); // run for it
    expect(sim.isCaptured("g1")).toBe(false);
    expect(sim.factionRevealed("Glitchers")).toBe(true); // first escape exposes them
  });
});

describe("Escape the Internet — interactive props & performers", () => {
  it("lets a guest manage their own heat: a prop can wipe the record", () => {
    const sim = fresh();
    sim.createPerson("g1", "Ada");
    sim.scan("Crawler", "g1"); // flagged → heat 40
    expect(sim.world.get("g1.heat")).toEqual({ kind: "number", value: 40 });
    sim.scan("Recycle_Bin", "g1"); // the counter-play prop
    sim.choose("g1", 0); // empty the bin
    expect(sim.world.get("g1.heat")).toEqual({ kind: "number", value: 0 });
    // ...so a follow-up flag is once again only a warning, not a lockdown.
    sim.scan("Crawler", "g1");
    expect(sim.isCaptured("g1")).toBe(false);
  });

  it("the Troll King's dare conscripts an unaligned guest into the Chatters", () => {
    const sim = fresh();
    sim.createPerson("g1", "Ada"); // no faction yet
    expect(sim.factionOf("g1")).toBeNull();
    sim.scan("Troll_King", "g1");
    sim.choose("g1", 0); // light them up
    expect(sim.factionOf("g1")).toBe("Chatters"); // `guest.faction == null` branch fired
    expect(sim.world.get("g1.heat")).toEqual({ kind: "number", value: 25 });
    // The Troll King's individual regard for the guest went up.
    expect(sim.world.get("Troll_King.trusts.g1")).toEqual({ kind: "number", value: 40 });
  });

  it("a read-only prop interpolates the guest's live profile", () => {
    const sim = fresh();
    sim.createPerson("g1", "Ada");
    sim.join("g1", "Mods");
    sim.scan("Moderator_Prime", "g1"); // +50 score, verified stays false
    const lines = sim
      .scan("Profile_Mirror", "g1")
      .filter((e) => e.type === "dialogue")
      .map((e) => (e as { text: string }).text)
      .join(" ");
    expect(lines).toContain("Score 50.");
    expect(lines).toContain("Heat 0.");
  });
});

describe("Escape the Internet — relationship-driven dialogue", () => {
  function lines(events: SimEvent[]): string {
    return events
      .filter((e) => e.type === "dialogue")
      .map((e) => (e as { text: string }).text)
      .join(" ");
  }
  it("gates a moderator's line on accumulated trust", () => {
    const sim = fresh();
    sim.createPerson("g1", "A");
    sim.join("g1", "Mods");
    // First scan: trust starts at 50 → the ordinary greeting, trust → 60.
    expect(lines(sim.scan("Moderator_Prime", "g1"))).toContain("A fellow Mod");
    expect(sim.world.get("Moderator_Prime.trusts.g1")).toEqual({ kind: "number", value: 60 });
    // Second scan: trust 60 (> 55) → the trusted-face line.
    expect(lines(sim.scan("Moderator_Prime", "g1"))).toContain("face I trust");
  });
});

describe("Escape the Internet — operator/admin moderation", () => {
  it("capture() imprisons a guest and escape() frees them, scanner-less", () => {
    const sim = fresh();
    sim.createPerson("g1", "Alice");
    sim.join("g1", "Chatters");
    expect(sim.isCaptured("g1")).toBe(false);

    sim.capture("g1");
    expect(sim.isCaptured("g1")).toBe(true);

    sim.capture("g1"); // idempotent — re-capturing is a no-op
    expect(sim.isCaptured("g1")).toBe(true);

    sim.escape("g1");
    expect(sim.isCaptured("g1")).toBe(false);
  });
});

describe("Loom — conditionals + directives inside a dialogue block", () => {
  const SRC = `FACTION Mods
FACTION Chatters

ROLE Guest
  score: 0 to 1000 = 0

CHARACTER Host
  faction: Mods
  trusts Guest: 50 of 100
  on scan guest
    -> greet

== greet(guest)
  cast: Host, guest

  HOST
    <if: guest.faction == Chatters>
      Careful, troublemaker.
      <set: Host.trusts.guest -= 10>
    <else>
      <if: Host.trusts.guest > 55>
        Welcome back, friend.
      <else>
        Good to see you.
        <set: guest.score += 5>
        <set: Host.trusts.guest += 10>
`;

  it("picks the branch's spoken line and runs only that branch's effects", () => {
    const sim = Sim.fromSources(SRC);

    // A Chatter → the warning line + the trust penalty; nothing else.
    sim.createPerson("c", "Cara");
    sim.join("c", "Chatters");
    const chatter = dialogue(sim.scan("Host", "c")).join(" ");
    expect(chatter).toContain("Careful, troublemaker");
    expect(chatter).not.toContain("Good to see you");
    expect(sim.world.get("Host.trusts.c")).toEqual({ kind: "number", value: 40 });
    expect(sim.scoreOf("c")).toBe(0); // the else-branch effects never ran

    // A Mod, first scan (trust 50) → inner else: greeting + score/trust bumps.
    sim.createPerson("m", "Mo");
    sim.join("m", "Mods");
    const first = dialogue(sim.scan("Host", "m")).join(" ");
    expect(first).toContain("Good to see you");
    expect(sim.scoreOf("m")).toBe(5);
    expect(sim.world.get("Host.trusts.m")).toEqual({ kind: "number", value: 60 });

    // Same Mod, second scan (trust 60 > 55) → inner if: the trusted line,
    // and NO further effects (that branch has none).
    const second = dialogue(sim.scan("Host", "m")).join(" ");
    expect(second).toContain("Welcome back, friend");
    expect(second).not.toContain("Good to see you");
    expect(sim.scoreOf("m")).toBe(5); // unchanged
    expect(sim.world.get("Host.trusts.m")).toEqual({ kind: "number", value: 60 });
  });
});

describe("Loom — match / each-visit / divert inside a dialogue block", () => {
  const SRC = `FACTION Mods
FACTION Chatters

ROLE Guest
  score: 0 to 1000 = 0

CHARACTER Host
  on scan guest
    -> greet

== greet(guest)
  cast: Host, guest

  HOST
    <match: guest.faction>
      Mods
        Welcome, mod.
      Chatters
        Watch it, chatter.
    Anyway —
    -> tag

== tag(guest)
  cast: Host, guest
  HOST
    You're tagged.
    <set: guest.score += 1>

== mood(guest)
  cast: Host, guest
  HOST
    <each visit>
      first
        Hello, newcomer.
      then
        Oh, you again.
`;

  it("runs <match>, a plain speaker line, and a divert — all inside one speaker block", () => {
    const sim = Sim.fromSources(SRC);
    sim.createPerson("m", "Mo");
    sim.join("m", "Mods");
    const out = dialogue(sim.scan("Host", "m")).join(" | ");
    expect(out).toContain("Welcome, mod."); // matched arm, spoken by Host
    expect(out).not.toContain("Watch it");
    expect(out).toContain("Anyway —"); // plain speaker line after the match
    expect(out).toContain("You're tagged."); // the in-dialogue divert reached `tag`
    expect(sim.scoreOf("m")).toBe(1); // and `tag`'s effect ran

    const sim2 = Sim.fromSources(SRC);
    sim2.createPerson("c", "Cy");
    sim2.join("c", "Chatters");
    const out2 = dialogue(sim2.scan("Host", "c")).join(" | ");
    expect(out2).toContain("Watch it, chatter.");
    expect(out2).not.toContain("Welcome, mod.");
  });

  it("runs <each visit> inside a speaker block, attributed to the speaker", () => {
    const sim = Sim.fromSources(SRC);
    sim.createPerson("g", "Gee");
    const from = sim.log.len();
    sim.playBeat("mood", new Map([["self", "Host"], ["guest", "g"]]));
    const said = sim.log.since(from).filter((e) => e.type === "dialogue") as Array<{ speaker: string; text: string }>;
    expect(said.map((e) => e.text).join(" ")).toContain("Hello, newcomer.");
    expect(said[0]!.speaker).toBe("HOST"); // spoken line (speaker cue), not narration
  });
});

describe("operator mutations — setScore + fireBeat", () => {
  it("setScore writes a person's score directly and emits a worldSet", () => {
    const sim = fresh();
    sim.createPerson("g1", "Alice");
    expect(sim.scoreOf("g1")).toBe(0);
    const evs = sim.setScore("g1", 42);
    expect(sim.scoreOf("g1")).toBe(42);
    expect(evs.some((e) => e.type === "worldSet")).toBe(true);
  });

  it("fireBeat plays a named beat and records beatEntered", () => {
    const sim = Sim.fromSources(`
== announce
  The internet awakens.
`);
    const evs = sim.fireBeat("announce");
    expect(evs.some((e) => e.type === "beatEntered" && (e as { beat: string }).beat === "announce")).toBe(true);
  });

  it("fireBeat binds the subject as `guest` so a per-guest beat targets one person", () => {
    const sim = Sim.fromSources(`
== tag
  <set: guest.score += 10>
`);
    sim.createPerson("g1", "Alice");
    sim.createPerson("g2", "Bob");
    sim.fireBeat("tag", "g1");
    expect(sim.scoreOf("g1")).toBe(10);
    expect(sim.scoreOf("g2")).toBe(0); // untouched
  });

  it("fireBeat on an unknown beat is a quiet no-op", () => {
    const sim = fresh();
    expect(sim.fireBeat("does-not-exist")).toEqual([]);
  });

  it("reveal exposes a hidden faction (secret-villain reveal)", () => {
    const sim = fresh();
    sim.createPerson("g1", "Alice");
    sim.join("g1", "TheAlgorithm"); // a hidden faction — masked until revealed
    expect(sim.publicFactionOf("g1")).toBeNull();
    expect(sim.factionRevealed("TheAlgorithm")).toBe(false);
    const evs = sim.reveal("TheAlgorithm");
    expect(sim.factionRevealed("TheAlgorithm")).toBe(true);
    expect(sim.publicFactionOf("g1")).toBe("TheAlgorithm"); // now visible to all
    expect(evs.some((e) => e.type === "factionRevealed")).toBe(true);
    expect(sim.reveal("TheAlgorithm")).toEqual([]); // idempotent
  });
});
