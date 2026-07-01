import { describe, expect, it } from "vitest";

import { DEFAULT_SPACE, describeChannel, spaceOf } from "../server/chat.ts";

describe("channel descriptors + spaces", () => {
  it("maps every derived channel into the default Internet space", () => {
    for (const id of ["lobby", "faction:Mods", "dm:RECRUITER"]) {
      expect(spaceOf(id)).toBe(DEFAULT_SPACE.id);
      expect(describeChannel(id).spaceId).toBe(DEFAULT_SPACE.id);
    }
  });

  it("describes the lobby with a stable kind, title, and order", () => {
    const d = describeChannel("lobby");
    expect(d.channelKind).toBe("lobby");
    expect(d.title).toBe("The Internet");
    expect(d.order).toBe(0);
    expect(d.members).toBe("all");
  });

  it("describes a faction channel (members start closed, order after the lobby)", () => {
    const d = describeChannel("faction:Mods");
    expect(d.channelKind).toBe("faction");
    expect(d.title).toBe("#mods");
    expect(d.order).toBe(1);
    expect(d.members).toEqual([]);
  });

  it("describes a DM channel by its speaker", () => {
    const d = describeChannel("dm:RECRUITER");
    expect(d.channelKind).toBe("dm");
    expect(d.title).toBe("RECRUITER");
    expect(d.order).toBe(2);
  });

  it("orders the sidebar lobby → faction → dm", () => {
    const orders = ["lobby", "faction:Mods", "dm:X"].map((id) => describeChannel(id).order);
    expect(orders).toEqual([0, 1, 2]);
  });
});
