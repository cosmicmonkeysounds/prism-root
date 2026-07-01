import { describe, expect, it } from "vitest";

import { groupBySpace } from "../src/threads.ts";
import type { Channel } from "../src/types.ts";

function ch(id: string, spaceId: string, lastTs: number, decision = false): Channel {
  return {
    id,
    kind: "open",
    title: id,
    spaceId,
    messages: [],
    unread: 0,
    decision: decision ? { title: "?", options: [] } : null,
    lastTs,
  };
}

describe("groupBySpace — Discord sidebar sections", () => {
  it("orders sections internet → booth → guests", () => {
    const groups = groupBySpace([ch("g", "guests", 1), ch("s", "booth", 1), ch("l", "internet", 1)]);
    expect(groups.map((g) => g.id)).toEqual(["internet", "booth", "guests"]);
  });

  it("titles each section from the space table", () => {
    const groups = groupBySpace([ch("l", "internet", 1), ch("s", "booth", 1)]);
    expect(groups.find((g) => g.id === "internet")!.title).toBe("The Internet");
    expect(groups.find((g) => g.id === "booth")!.title).toBe("Booth");
  });

  it("preserves the incoming within-space order (decisions-first / recency stay)", () => {
    // Caller already sorted; groupBySpace must not reshuffle inside a space.
    const sorted = [ch("a", "internet", 9), ch("b", "internet", 5), ch("c", "internet", 2)];
    const internet = groupBySpace(sorted).find((g) => g.id === "internet")!;
    expect(internet.channels.map((c) => c.id)).toEqual(["a", "b", "c"]);
  });

  it("separates booth tools from the room feed", () => {
    const groups = groupBySpace([ch("__scanner", "booth", 1), ch("__feed", "internet", 1), ch("guest:1", "guests", 1)]);
    expect(groups.map((g) => g.channels.map((c) => c.id))).toEqual([["__feed"], ["__scanner"], ["guest:1"]]);
  });
});
