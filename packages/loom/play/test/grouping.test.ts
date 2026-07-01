import { describe, expect, it } from "vitest";

import { groupRuns, RUN_WINDOW_MS } from "../src/threads.ts";
import type { ChatMessage, MessageKind } from "../src/types.ts";

let seq = 0;
function msg(from: string, kind: MessageKind, ts: number): ChatMessage {
  return {
    seq: seq++,
    channel: "lobby",
    channelKind: "lobby",
    title: "The Internet",
    from,
    kind,
    text: `${from}@${ts}`,
    ts,
    audience: "all",
    parentSeq: null,
    hidden: false,
  };
}

describe("groupRuns — Slack/Discord sender banners", () => {
  it("coalesces consecutive same-sender lines into one run", () => {
    const runs = groupRuns([msg("Alice", "line", 0), msg("Alice", "line", 1000), msg("Alice", "line", 2000)]);
    expect(runs).toHaveLength(1);
    expect(runs[0]!.from).toBe("Alice");
    expect(runs[0]!.messages).toHaveLength(3);
  });

  it("starts a new run when the sender changes", () => {
    const runs = groupRuns([msg("Alice", "line", 0), msg("Bob", "line", 100), msg("Alice", "line", 200)]);
    expect(runs.map((r) => r.from)).toEqual(["Alice", "Bob", "Alice"]);
  });

  it("never coalesces non-line kinds (narration / system / signal stand alone)", () => {
    const runs = groupRuns([msg("", "narration", 0), msg("", "narration", 100), msg("", "system", 200)]);
    expect(runs).toHaveLength(3);
    expect(runs.every((r) => r.messages.length === 1)).toBe(true);
  });

  it("splits a run when the story-clock gap reaches the window", () => {
    const runs = groupRuns([msg("Alice", "line", 0), msg("Alice", "line", RUN_WINDOW_MS)]);
    expect(runs).toHaveLength(2);
  });

  it("keeps a run together just under the window", () => {
    const runs = groupRuns([msg("Alice", "line", 0), msg("Alice", "line", RUN_WINDOW_MS - 1)]);
    expect(runs).toHaveLength(1);
  });
});
