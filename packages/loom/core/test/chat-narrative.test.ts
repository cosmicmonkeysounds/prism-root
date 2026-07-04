//! The unified conversation contract (docs/dev/loom-conversation-model.md):
//! every line of story lands in exactly one room. Narration + un-addressed
//! dialogue route to the enclosing beat's `setting:` location room (else the
//! lobby); subject-bound dialogue stays a DM thread; location rooms take
//! typed chat from whoever is present.

import { describe, expect, it } from "vitest";
import { Sim } from "../src/runtime/sim/index.ts";
import { composeGuestMessages } from "../server/chat.ts";
import type { DraftMessage } from "../server/chat.ts";

const STAGE = `
entry: doors_open

LOCATION Party
  label: The Party

LOCATION Backstage

ROLE Guest
  score: 0 to 100 = 0

== doors_open
  setting: Party
  The doors hiss open.
  NARRATOR
    Welcome to the party.
  -> soundcheck

== soundcheck
  A hum builds under the floor.

== green_room
  setting: Backstage
  NARRATOR
    It is quiet back here.

== drift
  Nowhere in particular.

== greet(guest)
  cast: HOST, guest
  HOST
    Good to see you.

CHARACTER HOST
  voice: warm
`;

function play(sim: Sim, beat: string, subject?: string): DraftMessage[] {
  return composeGuestMessages(sim, sim.fireBeat(beat, subject));
}

describe("story → room routing", () => {
  it("delivers narration to the setting's location room as the Narrator", () => {
    const sim = Sim.fromSources(STAGE);
    const msgs = play(sim, "doors_open");
    const narration = msgs.find((m) => m.kind === "narration");
    expect(narration).toMatchObject({
      channel: "loc:Party",
      channelKind: "location",
      title: "The Party",
      from: "Narrator",
      audience: "all",
      text: "The doors hiss open.",
      beat: "doors_open",
    });
  });

  it("routes un-addressed dialogue (the entry NARRATOR) to the setting room, not a phantom DM", () => {
    const sim = Sim.fromSources(STAGE);
    const msgs = play(sim, "doors_open");
    const line = msgs.find((m) => m.from === "NARRATOR");
    expect(line).toMatchObject({ channel: "loc:Party", kind: "line", audience: "all" });
    expect(msgs.some((m) => m.channel === "dm:NARRATOR")).toBe(false);
  });

  it("a setting-less sub-beat inherits the caller's room through a divert", () => {
    const sim = Sim.fromSources(STAGE);
    const msgs = play(sim, "doors_open");
    const hum = msgs.find((m) => m.text.startsWith("A hum"));
    expect(hum).toMatchObject({ channel: "loc:Party", beat: "soundcheck" });
  });

  it("a beat with its own setting speaks in its own room", () => {
    const sim = Sim.fromSources(STAGE);
    const msgs = play(sim, "green_room");
    // No label on Backstage — the room titles itself by id.
    expect(msgs[0]).toMatchObject({ channel: "loc:Backstage", title: "Backstage" });
  });

  it("falls back to the lobby when no setting is in scope", () => {
    const sim = Sim.fromSources(STAGE);
    const msgs = play(sim, "drift");
    expect(msgs[0]).toMatchObject({ channel: "lobby", from: "Narrator", kind: "narration" });
  });

  it("keeps subject-bound dialogue in the speaker's DM thread", () => {
    const sim = Sim.fromSources(STAGE);
    sim.createPerson("g1", "Ada");
    const msgs = play(sim, "greet", "g1");
    const line = msgs.find((m) => m.from === "HOST");
    expect(line).toMatchObject({ channel: "dm:HOST", audience: ["g1"] });
  });
});

describe("location rooms", () => {
  it("lists every location room in each view, flagging presence", () => {
    const sim = Sim.fromSources(STAGE);
    sim.createPerson("g1", "Ada");
    sim.arrive("g1", "Party");
    const rooms = sim.visibleChannelsFor("g1").filter((c) => c.kind === "location");
    expect(rooms.map((r) => r.id).sort()).toEqual(["loc:Backstage", "loc:Party"]);
    const party = rooms.find((r) => r.id === "loc:Party")!;
    expect(party).toMatchObject({ title: "The Party", member: true, canPost: true });
    expect(rooms.find((r) => r.id === "loc:Backstage")).toMatchObject({ member: false, canPost: false });
  });

  it("scopes typed chat in a location room to whoever is present", () => {
    const sim = Sim.fromSources(STAGE);
    sim.createPerson("g1", "Ada");
    sim.createPerson("g2", "Bo");
    sim.createPerson("g3", "Cy");
    sim.arrive("g1", "Party");
    sim.arrive("g2", "Party");
    sim.arrive("g3", "Backstage");
    const events = sim.say("g1", "loc:Party", "hello room");
    const chat = events.find((e) => e.type === "chat")!;
    expect(chat.type === "chat" && [...chat.audience].sort()).toEqual(["g1", "g2"]);
    expect(sim.canPost("g3", "loc:Party")).toBe(false);
    expect(sim.canPost("g2", "loc:Party")).toBe(true);
  });

  it("routes a <respond:> readout to the scanning person's lobby feed", () => {
    const sim = Sim.fromSources(STAGE);
    const drafts = composeGuestMessages(sim, [{ type: "respond", to: "g1", text: "CAPTCHA passed." }]);
    expect(drafts[0]).toMatchObject({ channel: "lobby", kind: "narration", audience: ["g1"] });
  });
});
