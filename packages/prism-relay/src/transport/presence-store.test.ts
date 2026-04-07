import { describe, it, expect, vi } from "vitest";
import { createPresenceStore } from "./presence-store.js";
import type { ConnectionRegistry } from "./connection-registry.js";

describe("presence-store", () => {
  it("set/get stores presence for a peer", () => {
    const store = createPresenceStore();
    store.set("peer-1", { cursor: { x: 10, y: 20 }, activeView: "canvas" });

    const presence = store.get("peer-1");
    expect(presence).toEqual({ cursor: { x: 10, y: 20 }, activeView: "canvas" });
  });

  it("get returns undefined for unknown peer", () => {
    const store = createPresenceStore();
    expect(store.get("unknown")).toBeUndefined();
  });

  it("remove deletes presence", () => {
    const store = createPresenceStore();
    store.set("peer-1", { cursor: { x: 0, y: 0 } });
    store.remove("peer-1");

    expect(store.get("peer-1")).toBeUndefined();
  });

  it("getAll returns all peers with presence", () => {
    const store = createPresenceStore();
    store.set("peer-1", { cursor: { x: 1, y: 2 } });
    store.set("peer-2", { selection: { start: 0, end: 10 } });
    store.set("peer-3", { activeView: "editor" });

    const all = store.getAll();
    expect(all).toHaveLength(3);

    const ids = all.map((p) => p.peerId).sort();
    expect(ids).toEqual(["peer-1", "peer-2", "peer-3"]);

    const peer1 = all.find((p) => p.peerId === "peer-1");
    expect(peer1?.cursor).toEqual({ x: 1, y: 2 });
  });

  it("getAll returns empty array initially", () => {
    const store = createPresenceStore();
    expect(store.getAll()).toEqual([]);
  });

  it("broadcast delegates to registry.broadcastAll", () => {
    const store = createPresenceStore();

    const mockRegistry = {
      broadcastAll: vi.fn(),
      add: vi.fn(),
      remove: vi.fn(),
      get: vi.fn(),
      broadcastToCollection: vi.fn(),
    } as unknown as ConnectionRegistry;

    const msg = { type: "presence" as const, peerId: "peer-1", cursor: { x: 5, y: 5 } };
    store.broadcast(mockRegistry, undefined, msg as never);

    expect(mockRegistry.broadcastAll).toHaveBeenCalledOnce();
    expect(mockRegistry.broadcastAll).toHaveBeenCalledWith(msg, undefined);
  });
});
