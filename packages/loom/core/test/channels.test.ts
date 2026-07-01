import { describe, expect, it } from "vitest";

import { Sim, type SimEvent } from "../src/runtime/sim/index.ts";
import { ChatStore, composeGuestMessages } from "../server/chat.ts";

// A minimal world with authored spaces + channels of every access kind.
const SRC = `entry: start

FACTION Mods
  ethos: order

ROLE Guest
  faction: any of FACTION

SPACE Town
  label: The Town Square

  CHANNEL square
    kind: open
    label: # the-square

  CHANNEL backroom
    kind: private
    label: # backroom

CHANNEL mods_room
  space: Town
  kind: faction
  faction: Mods
  label: # mods-only

CHANNEL announcements
  space: Town
  kind: open
  type: announcement
  label: # announcements

== start
  setting: Town
`;

function fresh(): Sim {
  return Sim.fromSources(SRC);
}

describe("authoring — SPACE / CHANNEL compile into the model", () => {
  it("registers spaces + channels with kind/space/title", () => {
    const sim = fresh();
    expect([...sim.model.spaces.keys()]).toContain("Town");
    expect([...sim.model.channels.keys()].sort()).toEqual([
      "room:announcements",
      "room:backroom",
      "room:mods_room",
      "room:square",
    ]);
    const sq = sim.model.channels.get("room:square")!;
    expect(sq.kind).toBe("open");
    expect(sq.spaceId).toBe("Town");
    expect(sq.title).toBe("# the-square");
    expect(sim.model.channels.get("room:mods_room")!.faction).toBe("Mods");
    // The standalone `space: Town` channel is folded into Town's channel list.
    expect(sim.model.spaces.get("Town")!.channelIds).toContain("room:mods_room");
  });
});

describe("access control — visibility by channel kind", () => {
  it("open is visible to all; faction to members; private to invitees only", () => {
    const sim = fresh();
    sim.createPerson("g1", "Alice");
    sim.createPerson("g2", "Bob");
    sim.join("g1", "Mods");

    const vis = (id: string) => sim.visibleChannelsFor(id).map((c) => c.id).sort();
    // #announcements is open, so everyone sees it.
    expect(vis("g1")).toEqual(["room:announcements", "room:mods_room", "room:square"]); // Mod sees the faction room
    expect(vis("g2")).toEqual(["room:announcements", "room:square"]); // not a Mod, not invited
  });

  it("an invite grants visibility + posts a join notice to the room", () => {
    const sim = fresh();
    const store = new ChatStore();
    sim.createPerson("g2", "Bob");
    const msgs = store.append(composeGuestMessages(sim, sim.inviteToChannel("Host", "g2", "room:backroom")));
    expect(sim.visibleChannelsFor("g2").map((c) => c.id).sort()).toEqual([
      "room:announcements",
      "room:backroom",
      "room:square",
    ]);
    const notice = msgs.find((m) => m.channel === "room:backroom");
    expect(notice?.kind).toBe("system");
    expect(notice?.audience).toEqual(["g2"]); // only the new member
    // Leaving drops it again.
    sim.leaveChannel("g2", "room:backroom");
    expect(sim.visibleChannelsFor("g2").map((c) => c.id)).not.toContain("room:backroom");
  });
});

describe("typed chat into authored channels — member-scoped audience", () => {
  it("open → everyone, faction → members, private → the member set", () => {
    const sim = fresh();
    sim.createPerson("g1", "Alice");
    sim.join("g1", "Mods");
    sim.createPerson("g2", "Bob");
    sim.inviteToChannel("Host", "g2", "room:backroom");

    expect(composeGuestMessages(sim, sim.say("g1", "room:square", "hi all"))[0]!.audience).toBe("all");
    expect(composeGuestMessages(sim, sim.say("g1", "room:mods_room", "mods"))[0]!.audience).toEqual(["g1"]);
    expect(composeGuestMessages(sim, sim.say("g2", "room:backroom", "psst"))[0]!.audience).toEqual(["g2"]);
  });
});

describe("channel-type registry — post policy, threadability, routing", () => {
  it("resolves rules from kind + the announcement preset", () => {
    const sim = fresh();
    expect(sim.model.channels.get("room:square")!.rules.post).toEqual({ kind: "everyone" });
    expect(sim.model.channels.get("room:backroom")!.rules.post).toEqual({ kind: "members" });
    const ann = sim.model.channels.get("room:announcements")!.rules;
    expect(ann.post).toEqual({ kind: "none" }); // read-only
    expect(ann.threadable).toBe(false);
    expect(ann.routes).toEqual(["*"]); // mirrors every broadcast
  });

  it("enforces who may post (canPost)", () => {
    const sim = fresh();
    sim.createPerson("g1", "Alice");
    sim.join("g1", "Mods");
    sim.createPerson("g2", "Bob");
    expect(sim.canPost("g1", "room:square")).toBe(true); // open
    expect(sim.canPost("g1", "room:announcements")).toBe(false); // read-only
    expect(sim.canPost("g1", "room:mods_room")).toBe(true); // Mod in the faction
    expect(sim.canPost("g2", "room:mods_room")).toBe(false); // not a Mod
    expect(sim.canPost("g2", "room:backroom")).toBe(false); // not a member
  });

  it("mirrors a broadcast into a routed channel (#announcements)", () => {
    const sim = fresh();
    sim.createPerson("g1", "Alice");
    const synthetic: SimEvent = { type: "broadcast", cue: "lockdown_siren", scope: "", audience: [] };
    const msgs = composeGuestMessages(sim, [synthetic]);
    const ann = msgs.find((m) => m.channel === "room:announcements");
    expect(ann).toBeDefined();
    expect(ann!.audience).toBe("all"); // an open announcement feed
    expect(ann!.kind).toBe("signal");
  });

  it("reports threadability per channel", () => {
    const sim = fresh();
    expect(sim.threadableOf("room:square")).toBe(true);
    expect(sim.threadableOf("room:announcements")).toBe(false);
    expect(sim.threadableOf("lobby")).toBe(true); // derived channels thread
  });
});

describe("channel-type rules — slow mode + ephemeral", () => {
  const SRC2 = `entry: s

ROLE Guest

CHANNEL fast
  kind: open
  slow: 5s
  ephemeral: 10s

== s
`;
  it("resolves the slow-mode + ephemeral windows", () => {
    const sim = Sim.fromSources(SRC2);
    expect(sim.slowModeMsOf("room:fast")).toBe(5000);
    expect(sim.ephemeralMsOf("room:fast")).toBe(10000);
    expect(sim.slowModeMsOf("room:none")).toBe(null);
  });

  it("counts down the slow-mode window from the last post", () => {
    const sim = Sim.fromSources(SRC2);
    sim.createPerson("g1", "Alice");
    expect(sim.slowModeRemainingMs("g1", "room:fast")).toBe(0); // no posts yet
    sim.say("g1", "room:fast", "first");
    expect(sim.slowModeRemainingMs("g1", "room:fast")).toBe(5000); // full window
    sim.tick(3000);
    expect(sim.slowModeRemainingMs("g1", "room:fast")).toBe(2000);
    sim.tick(3000);
    expect(sim.slowModeRemainingMs("g1", "room:fast")).toBe(0); // window elapsed
  });
});

describe("scoped invite roster", () => {
  it("shows faction-mates + gated-room co-members, not the whole guest list", () => {
    const sim = fresh();
    sim.createPerson("g1", "Alice");
    sim.join("g1", "Mods");
    sim.createPerson("g2", "Bob");
    sim.join("g2", "Mods");
    sim.createPerson("g3", "Cara"); // unaffiliated — a stranger to g1

    expect(sim.rosterFor("g1").map((p) => p.id).sort()).toEqual(["g2"]); // faction-mate only
    expect(sim.rosterFor("g3")).toEqual([]); // no faction, no rooms → no one

    // Pull g1 + g3 into the same private room → g1's roster now includes g3.
    sim.inviteToChannel("Host", "g1", "room:backroom");
    sim.inviteToChannel("Host", "g3", "room:backroom");
    expect(sim.rosterFor("g1").map((p) => p.id).sort()).toEqual(["g2", "g3"]);
  });
});

describe("determinism — authored channel ops replay identically", () => {
  it("invite + say reproduce the same seqs, channels, and audiences", () => {
    const run = () => {
      const sim = fresh();
      const store = new ChatStore();
      const step = (evs: SimEvent[]) => store.append(composeGuestMessages(sim, evs));
      step(sim.createPerson("g2", "Bob"));
      step(sim.inviteToChannel("Host", "g2", "room:backroom"));
      step(sim.say("g2", "room:backroom", "anyone here?"));
      return store;
    };
    const a = run();
    const b = run();
    expect(b.all().map((m) => [m.seq, m.channel, m.text, JSON.stringify(m.audience)])).toEqual(
      a.all().map((m) => [m.seq, m.channel, m.text, JSON.stringify(m.audience)]),
    );
  });
});
