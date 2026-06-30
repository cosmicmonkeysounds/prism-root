import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

import { Sim, type SimEvent } from "../src/runtime/sim/index.ts";
import {
  ChatStore,
  composeGuestMessages,
  decisionChannelFor,
  visibleTo,
  type ChatMessage,
} from "../server/chat.ts";

const SCENARIO = readFileSync(new URL("../examples/escape-the-internet.loom", import.meta.url), "utf8");

/** Replay a fixed script against a fresh sim, composing as the server would. */
function play(): { sim: Sim; store: ChatStore } {
  const sim = Sim.fromSources(SCENARIO);
  const store = new ChatStore();
  const step = (evs: SimEvent[]) => store.append(composeGuestMessages(sim, evs));
  step(sim.createPerson("g1", "Alice"));
  step(sim.createPerson("g2", "Bob"));
  step(sim.join("g1", "Mods"));
  step(sim.scan("Recruiter", "g1"));
  step(sim.tick(20000)); // one ambient bark
  return { sim, store };
}

describe("chat composition — channel routing", () => {
  it("routes a character's line into that character's DM channel", () => {
    const sim = Sim.fromSources(SCENARIO);
    sim.createPerson("g1", "Alice");
    const msgs = composeGuestMessages(sim, sim.scan("Recruiter", "g1"));
    expect(msgs).toHaveLength(1);
    const dm = msgs[0]!;
    expect(dm.channelKind).toBe("dm");
    expect(dm.channel).toBe("dm:RECRUITER");
    expect(dm.kind).toBe("line");
    expect(dm.audience).toEqual(["g1"]);
  });

  it("routes a personal state transition into the lobby, addressed to one guest", () => {
    const sim = Sim.fromSources(SCENARIO);
    sim.createPerson("g1", "Alice");
    const msgs = composeGuestMessages(sim, sim.join("g1", "Mods"));
    expect(msgs).toHaveLength(1);
    expect(msgs[0]!.channel).toBe("lobby");
    expect(msgs[0]!.kind).toBe("system");
    expect(msgs[0]!.audience).toEqual(["g1"]);
  });

  it("routes an ambient bark into the lobby, audience = everyone", () => {
    const sim = Sim.fromSources(SCENARIO);
    sim.createPerson("g1", "Alice");
    const msgs = composeGuestMessages(sim, sim.tick(20000));
    const bark = msgs.find((m) => m.kind === "narration");
    expect(bark).toBeDefined();
    expect(bark!.channel).toBe("lobby");
    expect(bark!.audience).toBe("all");
  });

  it("mirrors a faction-scoped broadcast into each faction channel (members only)", () => {
    const sim = Sim.fromSources(SCENARIO);
    sim.createPerson("g1", "Alice");
    sim.join("g1", "Mods");
    sim.createPerson("g2", "Bob");
    sim.join("g2", "Chatters");
    const synthetic: SimEvent = {
      type: "broadcast",
      cue: "lockdown_siren",
      scope: "faction(Mods) | faction(Chatters)",
      audience: ["g1", "g2"],
    };
    const msgs = composeGuestMessages(sim, [synthetic]);
    const channels = msgs.map((m) => m.channel).sort();
    expect(channels).toEqual(["faction:Chatters", "faction:Mods"]);
    const mods = msgs.find((m) => m.channel === "faction:Mods")!;
    expect(mods.channelKind).toBe("faction");
    expect(mods.title).toBe("#mods");
    expect(mods.audience).toEqual(["g1"]); // only the Mod
  });

  it("docks a narrative decision under the speaker's DM, world choices in the lobby", () => {
    const sim = Sim.fromSources(SCENARIO);
    sim.createPerson("g1", "Alice");
    const events = sim.scan("Recruiter", "g1");
    expect(decisionChannelFor(events, "g1")).toBe("dm:RECRUITER");
    // No preceding dialogue → falls back to the lobby.
    expect(decisionChannelFor([{ type: "choicePrompted", person: "g1", promptId: "g1", options: [] }], "g1")).toBe(
      "lobby",
    );
  });
});

describe("ChatStore — history, visibility, moderation", () => {
  it("gives each guest only the messages addressed to them", () => {
    const { store } = play();
    const g1 = store.historyFor("g1", false);
    const g2 = store.historyFor("g2", false);
    // g1 got: their join (lobby/system), the Recruiter DM, and the ambient bark.
    expect(g1.some((m) => m.channel === "dm:RECRUITER")).toBe(true);
    expect(g1.some((m) => m.kind === "narration")).toBe(true);
    // g2 never joined or got scanned — but still sees the global ambient bark.
    expect(g2.some((m) => m.channel === "dm:RECRUITER")).toBe(false);
    expect(g2.some((m) => m.audience === "all")).toBe(true);
    expect(g2.every((m) => m.audience === "all")).toBe(true);
  });

  it("assigns dense, monotonic seqs", () => {
    const { store } = play();
    store.all().forEach((m, i) => expect(m.seq).toBe(i));
  });

  it("withholds hidden messages from guests but flags them for admins", () => {
    const { store } = play();
    const dm = store.all().find((m) => m.channel === "dm:RECRUITER")!;
    store.setHidden(dm.seq, true);

    const guest = store.historyFor("g1", false);
    expect(guest.some((m) => m.seq === dm.seq)).toBe(false); // gone for the guest

    const admin = store.historyFor("g1", true);
    const flagged = admin.find((m) => m.seq === dm.seq)!;
    expect(flagged.hidden).toBe(true); // visible + flagged for the moderator

    // Restoring brings it back for the guest.
    store.setHidden(dm.seq, false);
    expect(store.historyFor("g1", false).some((m) => m.seq === dm.seq)).toBe(true);
  });

  it("re-applies persisted moderation after a deterministic rebuild", () => {
    const live = play();
    const dm = live.store.all().find((m) => m.channel === "dm:RECRUITER")!;
    live.store.setHidden(dm.seq, true);
    const persistedHidden = live.store.hiddenSeqs();

    // A fresh process: replay the identical script, then re-apply moderation.
    const rebuilt = play();
    rebuilt.store.loadHidden(persistedHidden);

    // Same content, same seqs (history is deterministic) …
    expect(rebuilt.store.all().map((m) => m.text)).toEqual(live.store.all().map((m) => m.text));
    // … so the hidden flag lands on the same message.
    expect(rebuilt.store.get(dm.seq)!.hidden).toBe(true);
    expect(rebuilt.store.historyFor("g1", false).some((m) => m.seq === dm.seq)).toBe(false);
  });
});

describe("visibleTo", () => {
  const base: Omit<ChatMessage, "audience"> = {
    seq: 0,
    channel: "lobby",
    channelKind: "lobby",
    title: "The Internet",
    from: "",
    kind: "narration",
    text: "hi",
    ts: 0,
    hidden: false,
  };
  it('"all" reaches everyone', () => {
    expect(visibleTo({ ...base, audience: "all" }, "anyone")).toBe(true);
  });
  it("a fixed audience reaches only its members", () => {
    expect(visibleTo({ ...base, audience: ["g1"] }, "g1")).toBe(true);
    expect(visibleTo({ ...base, audience: ["g1"] }, "g2")).toBe(false);
  });
});
