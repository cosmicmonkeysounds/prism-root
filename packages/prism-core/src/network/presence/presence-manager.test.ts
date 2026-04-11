import { describe, it, expect, beforeEach } from "vitest";
import { createPresenceManager } from "./presence-manager.js";
import type { PresenceManager } from "./presence-manager.js";
import type {
  PeerIdentity,
  PresenceState,
  PresenceChange,
  TimerProvider,
} from "./presence-types.js";

// ── Helpers ───────────────────────────────────────────────────────────────────

function makeIdentity(peerId: string): PeerIdentity {
  return {
    peerId,
    displayName: `User ${peerId}`,
    color: "#ff0000",
  };
}

function makeRemoteState(peerId: string, overrides?: Partial<PresenceState>): PresenceState {
  return {
    identity: makeIdentity(peerId),
    cursor: null,
    selections: [],
    activeView: null,
    lastSeen: new Date().toISOString(),
    data: {},
    ...overrides,
  };
}

function createMockTimers(startTime = 1000): TimerProvider & {
  advance(ms: number): void;
  callbacks: Map<number, { fn: () => void; interval: number; nextAt: number }>;
} {
  let currentTime = startTime;
  let nextId = 1;
  const callbacks = new Map<number, { fn: () => void; interval: number; nextAt: number }>();

  return {
    callbacks,
    now: () => currentTime,
    setInterval: (fn: () => void, ms: number) => {
      const id = nextId++;
      callbacks.set(id, { fn, interval: ms, nextAt: currentTime + ms });
      return id;
    },
    clearInterval: (id: number) => {
      callbacks.delete(id);
    },
    advance(ms: number) {
      const target = currentTime + ms;
      // Fire any intervals that would trigger during this advance
      while (currentTime < target) {
        let earliest = target;
        for (const cb of callbacks.values()) {
          if (cb.nextAt < earliest) earliest = cb.nextAt;
        }
        currentTime = earliest;
        for (const cb of callbacks.values()) {
          if (cb.nextAt <= currentTime) {
            cb.fn();
            cb.nextAt = currentTime + cb.interval;
          }
        }
      }
      currentTime = target;
    },
  };
}

// ── Tests ─────────────────────────────────────────────────────────────────────

describe("PresenceManager", () => {
  let pm: PresenceManager;
  let timers: ReturnType<typeof createMockTimers>;
  const localId = makeIdentity("local-1");

  beforeEach(() => {
    timers = createMockTimers(1000);
    pm = createPresenceManager({
      localIdentity: localId,
      ttlMs: 30_000,
      sweepIntervalMs: 5_000,
      timers,
    });
  });

  // ── Local state ──────────────────────────────────────────────────────────

  describe("local state", () => {
    it("initialises with local identity and null cursor", () => {
      expect(pm.local.identity.peerId).toBe("local-1");
      expect(pm.local.cursor).toBeNull();
      expect(pm.local.selections).toEqual([]);
      expect(pm.local.activeView).toBeNull();
    });

    it("setCursor updates local cursor", () => {
      pm.setCursor({ objectId: "obj-1", field: "name", offset: 5 });
      expect(pm.local.cursor).toEqual({ objectId: "obj-1", field: "name", offset: 5 });
    });

    it("setCursor(null) clears cursor", () => {
      pm.setCursor({ objectId: "obj-1" });
      pm.setCursor(null);
      expect(pm.local.cursor).toBeNull();
    });

    it("setSelections updates local selections", () => {
      const sels = [
        { objectId: "obj-1", field: "name", anchor: 0, head: 5 },
        { objectId: "obj-2" },
      ];
      pm.setSelections(sels);
      expect(pm.local.selections).toEqual(sels);
    });

    it("setActiveView updates active view", () => {
      pm.setActiveView("collection-abc");
      expect(pm.local.activeView).toBe("collection-abc");
    });

    it("setData updates arbitrary data", () => {
      pm.setData({ status: "typing", draft: true });
      expect(pm.local.data).toEqual({ status: "typing", draft: true });
    });

    it("updateLocal does a bulk update", () => {
      pm.updateLocal({
        cursor: { objectId: "obj-1" },
        activeView: "view-1",
        data: { typing: true },
      });
      expect(pm.local.cursor).toEqual({ objectId: "obj-1" });
      expect(pm.local.activeView).toBe("view-1");
      expect(pm.local.data).toEqual({ typing: true });
      expect(pm.local.selections).toEqual([]); // unchanged
    });

    it("updateLocal only updates provided fields", () => {
      pm.setCursor({ objectId: "obj-1" });
      pm.setActiveView("view-1");
      pm.updateLocal({ activeView: "view-2" });
      expect(pm.local.cursor).toEqual({ objectId: "obj-1" }); // untouched
      expect(pm.local.activeView).toBe("view-2");
    });

    it("local state is accessible via get(localPeerId)", () => {
      expect(pm.get("local-1")).toBe(pm.local);
    });

    it("has() returns true for local peer", () => {
      expect(pm.has("local-1")).toBe(true);
    });
  });

  // ── Remote peers ─────────────────────────────────────────────────────────

  describe("remote peers", () => {
    it("starts with no remote peers", () => {
      expect(pm.peerCount).toBe(0);
      expect(pm.getPeers()).toEqual([]);
    });

    it("receiveRemote adds a new peer", () => {
      pm.receiveRemote(makeRemoteState("remote-1"));
      expect(pm.peerCount).toBe(1);
      expect(pm.has("remote-1")).toBe(true);
      expect(pm.get("remote-1")?.identity.peerId).toBe("remote-1");
    });

    it("receiveRemote updates existing peer", () => {
      pm.receiveRemote(makeRemoteState("remote-1", { cursor: { objectId: "a" } }));
      pm.receiveRemote(makeRemoteState("remote-1", { cursor: { objectId: "b" } }));
      expect(pm.peerCount).toBe(1);
      expect(pm.get("remote-1")?.cursor?.objectId).toBe("b");
    });

    it("receiveRemote ignores self (local peerId)", () => {
      pm.receiveRemote(makeRemoteState("local-1"));
      expect(pm.peerCount).toBe(0);
    });

    it("removePeer removes a remote peer", () => {
      pm.receiveRemote(makeRemoteState("remote-1"));
      pm.removePeer("remote-1");
      expect(pm.peerCount).toBe(0);
      expect(pm.has("remote-1")).toBe(false);
    });

    it("removePeer is a no-op for unknown peers", () => {
      pm.removePeer("unknown");
      expect(pm.peerCount).toBe(0);
    });

    it("getPeers returns only remote peers", () => {
      pm.receiveRemote(makeRemoteState("remote-1"));
      pm.receiveRemote(makeRemoteState("remote-2"));
      const peers = pm.getPeers();
      expect(peers).toHaveLength(2);
      expect(peers.map((p) => p.identity.peerId).sort()).toEqual(["remote-1", "remote-2"]);
    });

    it("getAll returns local + remote", () => {
      pm.receiveRemote(makeRemoteState("remote-1"));
      const all = pm.getAll();
      expect(all).toHaveLength(2);
      expect(all[0].identity.peerId).toBe("local-1");
      expect(all[1].identity.peerId).toBe("remote-1");
    });

    it("receiveRemote stamps lastSeen from timer, not from incoming state", () => {
      timers.advance(5000); // now = 6000
      pm.receiveRemote(makeRemoteState("remote-1", {
        lastSeen: new Date(0).toISOString(), // old timestamp
      }));
      const peer = pm.get("remote-1");
      expect(peer).toBeDefined();
      expect(new Date(peer?.lastSeen ?? "").getTime()).toBe(6000);
    });
  });

  // ── Subscriptions ────────────────────────────────────────────────────────

  describe("subscribe", () => {
    it("fires 'joined' when a new remote peer appears", () => {
      const changes: PresenceChange[] = [];
      pm.subscribe((c) => changes.push(c));

      pm.receiveRemote(makeRemoteState("remote-1"));
      expect(changes).toHaveLength(1);
      expect(changes[0].type).toBe("joined");
      expect(changes[0].peerId).toBe("remote-1");
      expect(changes[0].state).not.toBeNull();
    });

    it("fires 'updated' when an existing remote peer is refreshed", () => {
      pm.receiveRemote(makeRemoteState("remote-1"));
      const changes: PresenceChange[] = [];
      pm.subscribe((c) => changes.push(c));

      pm.receiveRemote(makeRemoteState("remote-1", { cursor: { objectId: "x" } }));
      expect(changes).toHaveLength(1);
      expect(changes[0].type).toBe("updated");
    });

    it("fires 'left' when a peer is removed", () => {
      pm.receiveRemote(makeRemoteState("remote-1"));
      const changes: PresenceChange[] = [];
      pm.subscribe((c) => changes.push(c));

      pm.removePeer("remote-1");
      expect(changes).toHaveLength(1);
      expect(changes[0].type).toBe("left");
      expect(changes[0].state).toBeNull();
    });

    it("fires 'updated' on local cursor change", () => {
      const changes: PresenceChange[] = [];
      pm.subscribe((c) => changes.push(c));

      pm.setCursor({ objectId: "obj-1" });
      expect(changes).toHaveLength(1);
      expect(changes[0].type).toBe("updated");
      expect(changes[0].peerId).toBe("local-1");
    });

    it("fires on local setSelections / setActiveView / setData", () => {
      const changes: PresenceChange[] = [];
      pm.subscribe((c) => changes.push(c));

      pm.setSelections([{ objectId: "a" }]);
      pm.setActiveView("v");
      pm.setData({ x: 1 });
      expect(changes).toHaveLength(3);
      expect(changes.every((c) => c.type === "updated" && c.peerId === "local-1")).toBe(true);
    });

    it("unsubscribe stops notifications", () => {
      const changes: PresenceChange[] = [];
      const unsub = pm.subscribe((c) => changes.push(c));
      unsub();

      pm.receiveRemote(makeRemoteState("remote-1"));
      expect(changes).toHaveLength(0);
    });

    it("multiple listeners all fire", () => {
      let count1 = 0;
      let count2 = 0;
      pm.subscribe(() => count1++);
      pm.subscribe(() => count2++);

      pm.setCursor({ objectId: "x" });
      expect(count1).toBe(1);
      expect(count2).toBe(1);
    });
  });

  // ── TTL eviction ─────────────────────────────────────────────────────────

  describe("TTL eviction", () => {
    it("sweep evicts peers older than ttlMs", () => {
      pm.receiveRemote(makeRemoteState("remote-1"));
      expect(pm.peerCount).toBe(1);

      timers.advance(31_000); // past 30s TTL
      const evicted = pm.sweep();
      expect(evicted).toEqual(["remote-1"]);
      expect(pm.peerCount).toBe(0);
    });

    it("sweep keeps peers within TTL", () => {
      pm.receiveRemote(makeRemoteState("remote-1"));
      timers.advance(10_000); // only 10s
      const evicted = pm.sweep();
      expect(evicted).toEqual([]);
      expect(pm.peerCount).toBe(1);
    });

    it("receiveRemote refreshes lastSeen, preventing eviction", () => {
      pm.receiveRemote(makeRemoteState("remote-1"));
      timers.advance(20_000);
      pm.receiveRemote(makeRemoteState("remote-1")); // refresh
      timers.advance(20_000); // total 40s from start, but 20s since refresh
      const evicted = pm.sweep();
      expect(evicted).toEqual([]);
    });

    it("automatic sweep fires on interval", () => {
      pm.receiveRemote(makeRemoteState("remote-1"));
      timers.advance(35_000); // triggers sweep at 5s, 10s, 15s, 20s, 25s, 30s, 35s
      expect(pm.peerCount).toBe(0); // swept automatically
    });

    it("sweep fires 'left' event for evicted peers", () => {
      pm.receiveRemote(makeRemoteState("remote-1"));
      pm.receiveRemote(makeRemoteState("remote-2"));

      const changes: PresenceChange[] = [];
      pm.subscribe((c) => changes.push(c));

      timers.advance(31_000);
      pm.sweep();

      const leftEvents = changes.filter((c) => c.type === "left");
      expect(leftEvents).toHaveLength(2);
    });

    it("sweep only evicts stale peers, keeps fresh ones", () => {
      pm.receiveRemote(makeRemoteState("remote-1"));
      timers.advance(25_000);
      pm.receiveRemote(makeRemoteState("remote-2")); // fresh
      timers.advance(6_000); // remote-1 at 31s, remote-2 at 6s

      const evicted = pm.sweep();
      expect(evicted).toEqual(["remote-1"]);
      expect(pm.peerCount).toBe(1);
      expect(pm.has("remote-2")).toBe(true);
    });
  });

  // ── Dispose ──────────────────────────────────────────────────────────────

  describe("dispose", () => {
    it("clears all remote peers and fires left events", () => {
      pm.receiveRemote(makeRemoteState("remote-1"));
      pm.receiveRemote(makeRemoteState("remote-2"));

      const changes: PresenceChange[] = [];
      pm.subscribe((c) => changes.push(c));

      pm.dispose();
      expect(pm.peerCount).toBe(0);
      expect(changes.filter((c) => c.type === "left")).toHaveLength(2);
    });

    it("stops the sweep timer", () => {
      pm.dispose();
      // Timer callbacks should be cleared
      expect(timers.callbacks.size).toBe(0);
    });

    it("clears listeners on dispose", () => {
      const changes: PresenceChange[] = [];
      pm.subscribe((c) => changes.push(c));
      pm.dispose();

      // After dispose, new remote additions shouldn't fire (listeners cleared during dispose,
      // but left events from dispose itself still fire)
      const leftCount = changes.length;
      pm.receiveRemote(makeRemoteState("remote-1"));
      expect(changes.length).toBe(leftCount); // no new events
    });
  });

  // ── Edge cases ───────────────────────────────────────────────────────────

  describe("edge cases", () => {
    it("getAll always has local first", () => {
      pm.receiveRemote(makeRemoteState("aaa"));
      pm.receiveRemote(makeRemoteState("zzz"));
      expect(pm.getAll()[0].identity.peerId).toBe("local-1");
    });

    it("handles rapid cursor updates", () => {
      const changes: PresenceChange[] = [];
      pm.subscribe((c) => changes.push(c));

      for (let i = 0; i < 100; i++) {
        pm.setCursor({ objectId: `obj-${i}` });
      }
      expect(changes).toHaveLength(100);
      expect(pm.local.cursor?.objectId).toBe("obj-99");
    });

    it("works with zero sweepIntervalMs (no auto-sweep)", () => {
      const pm2 = createPresenceManager({
        localIdentity: localId,
        ttlMs: 30_000,
        sweepIntervalMs: 0,
        timers,
      });
      pm2.receiveRemote(makeRemoteState("remote-1"));
      timers.advance(60_000);
      // No auto-sweep, peer still there
      expect(pm2.peerCount).toBe(1);
      // Manual sweep works
      pm2.sweep();
      expect(pm2.peerCount).toBe(0);
      pm2.dispose();
    });
  });
});
