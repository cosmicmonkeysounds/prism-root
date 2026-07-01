import { describe, expect, it } from "vitest";

import { repliesFor, replyCountFor, rootsOf } from "../src/threads.ts";
import type { ChatMessage } from "../src/types.ts";

function msg(seq: number, parentSeq: number | null): ChatMessage {
  return {
    seq,
    channel: "lobby",
    channelKind: "lobby",
    title: "The Internet",
    from: "Alice",
    kind: "line",
    text: `m${seq}`,
    ts: seq,
    audience: "all",
    parentSeq,
    hidden: false,
  };
}

// 0 root, 1 reply→0, 2 root, 3 reply→0, 4 reply→2
const MSGS = [msg(0, null), msg(1, 0), msg(2, null), msg(3, 0), msg(4, 2)];

describe("thread selectors", () => {
  it("rootsOf keeps only top-level messages", () => {
    expect(rootsOf(MSGS).map((m) => m.seq)).toEqual([0, 2]);
  });

  it("repliesFor returns a root's replies, seq-ordered", () => {
    expect(repliesFor(MSGS, 0).map((m) => m.seq)).toEqual([1, 3]);
    expect(repliesFor(MSGS, 2).map((m) => m.seq)).toEqual([4]);
  });

  it("replyCountFor counts replies by parentSeq", () => {
    expect(replyCountFor(MSGS, 0)).toBe(2);
    expect(replyCountFor(MSGS, 2)).toBe(1);
    expect(replyCountFor(MSGS, 1)).toBe(0); // a reply itself has none (single-level)
  });
});
