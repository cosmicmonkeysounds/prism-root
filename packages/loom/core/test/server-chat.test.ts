import { describe, expect, it } from "vitest";

import { Sim, type SimEvent } from "../src/runtime/sim/index.ts";
import {
  ChatStore,
  composeGuestMessages,
  decisionChannelFor,
  visibleTo,
  type ChatMessage,
} from "../server/chat.ts";
import { scenarioSource } from "../examples/load.ts";

const SCENARIO = scenarioSource("escape-the-internet");

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
    const channels = msgs.map((m) => m.channel);
    // The faction broadcast mirrors into each faction channel (plus any
    // routed authored feed like #announcements — scoped to the same audience).
    expect(channels).toContain("faction:Mods");
    expect(channels).toContain("faction:Chatters");
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

describe("typed chat — the journaled `say` command", () => {
  it("every story-composed message is a thread root (parentSeq null)", () => {
    const { store } = play();
    expect(store.all().every((m) => m.parentSeq === null)).toBe(true);
  });

  it("routes a guest's typed lobby message to everyone as a line from their name", () => {
    const sim = Sim.fromSources(SCENARIO);
    sim.createPerson("g1", "Alice");
    const msgs = composeGuestMessages(sim, sim.say("g1", "lobby", "hello internet"));
    expect(msgs).toHaveLength(1);
    const m = msgs[0]!;
    expect(m.channel).toBe("lobby");
    expect(m.kind).toBe("line");
    expect(m.from).toBe("Alice"); // display name, not the id
    expect(m.text).toBe("hello internet");
    expect(m.audience).toBe("all");
    expect(m.parentSeq).toBe(null);
  });

  it("scopes a faction message to that faction's members", () => {
    const sim = Sim.fromSources(SCENARIO);
    sim.createPerson("g1", "Alice");
    sim.join("g1", "Mods");
    sim.createPerson("g2", "Bob");
    sim.join("g2", "Chatters");
    const msgs = composeGuestMessages(sim, sim.say("g1", "faction:Mods", "mods only"));
    expect(msgs[0]!.channel).toBe("faction:Mods");
    expect(msgs[0]!.audience).toEqual(["g1"]);
  });

  it("derives a guest's DM audience as themselves", () => {
    const sim = Sim.fromSources(SCENARIO);
    sim.createPerson("g1", "Alice");
    const msgs = composeGuestMessages(sim, sim.say("g1", "dm:RECRUITER", "are you there?"));
    expect(msgs[0]!.channel).toBe("dm:RECRUITER");
    expect(msgs[0]!.audience).toEqual(["g1"]);
  });

  it("carries an explicit audience (a performer replying into a guest thread)", () => {
    const sim = Sim.fromSources(SCENARIO);
    sim.createPerson("g1", "Alice");
    const msgs = composeGuestMessages(sim, sim.say("Recruiter", "dm:Recruiter", "found you", null, ["g1"]));
    expect(msgs[0]!.from).toBe("Recruiter"); // a character, not in persons → name passes through
    expect(msgs[0]!.audience).toEqual(["g1"]);
  });

  it("links a reply to its root and counts it (Slack threads)", () => {
    const sim = Sim.fromSources(SCENARIO);
    const store = new ChatStore();
    const step = (evs: SimEvent[]) => store.append(composeGuestMessages(sim, evs));
    sim.createPerson("g1", "Alice");
    const [root] = step(sim.say("g1", "lobby", "anyone here?"));
    const [reply] = step(sim.say("g1", "lobby", "guess not", root!.seq));
    expect(reply!.parentSeq).toBe(root!.seq);
    expect(store.replies(root!.seq).map((m) => m.seq)).toEqual([reply!.seq]);
    expect(store.replyCount(root!.seq)).toBe(1);
    // The root itself is not a reply to anything.
    expect(store.replyCount(reply!.seq)).toBe(0);
  });

  it("replays typed chat deterministically (same seqs + thread links)", () => {
    const run = () => {
      const sim = Sim.fromSources(SCENARIO);
      const store = new ChatStore();
      const step = (evs: SimEvent[]) => store.append(composeGuestMessages(sim, evs));
      step(sim.createPerson("g1", "Alice"));
      const [root] = step(sim.say("g1", "lobby", "hi"));
      step(sim.say("g1", "lobby", "still hi", root!.seq));
      return store;
    };
    const a = run();
    const b = run();
    expect(b.all().map((m) => [m.seq, m.from, m.text, m.parentSeq])).toEqual(
      a.all().map((m) => [m.seq, m.from, m.text, m.parentSeq]),
    );
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
    parentSeq: null,
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
