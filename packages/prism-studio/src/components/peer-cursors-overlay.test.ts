import { describe, it, expect } from "vitest";
import { groupPeerSelections } from "./peer-cursors-overlay.js";
import type { PresenceState } from "@prism/core/presence";

function mkPeer(
  peerId: string,
  selections: string[] = [],
  cursorId: string | null = null,
): PresenceState {
  return {
    identity: { peerId, displayName: peerId, color: "#fff" },
    cursor: cursorId ? { objectId: cursorId } : null,
    selections: selections.map((objectId) => ({ objectId })),
    activeView: null,
    lastSeen: new Date().toISOString(),
    data: {},
  };
}

describe("groupPeerSelections", () => {
  it("returns an empty map when no peers", () => {
    expect(groupPeerSelections([], "me").size).toBe(0);
  });

  it("excludes the local peer from the output", () => {
    const peers = [mkPeer("me", ["obj-1"]), mkPeer("alice", ["obj-1"])];
    const result = groupPeerSelections(peers, "me");
    expect(result.get("obj-1")?.map((p) => p.identity.peerId)).toEqual(["alice"]);
  });

  it("includes cursor-only peers under their cursor objectId", () => {
    const result = groupPeerSelections([mkPeer("bob", [], "obj-2")], null);
    expect(result.get("obj-2")?.length).toBe(1);
  });

  it("merges cursor and selection objectIds without duplicates", () => {
    const result = groupPeerSelections([mkPeer("bob", ["obj-1"], "obj-1")], null);
    expect(result.get("obj-1")?.length).toBe(1);
  });

  it("groups multiple peers that selected the same object", () => {
    const peers = [mkPeer("alice", ["obj-1"]), mkPeer("bob", ["obj-1"])];
    const result = groupPeerSelections(peers, null);
    expect(result.get("obj-1")?.length).toBe(2);
  });
});
