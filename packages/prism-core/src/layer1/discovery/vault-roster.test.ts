import { describe, it, expect, beforeEach, vi } from "vitest";
import type { RosterEntry, RosterChange, VaultRoster } from "./vault-roster.js";
import { createVaultRoster, createMemoryRosterStore } from "./vault-roster.js";

// ── Helpers ───────────────────────────────────────────────────────────────────

function makeEntry(overrides: Partial<RosterEntry> = {}): Omit<RosterEntry, "addedAt"> & { addedAt?: string } {
  return {
    id: "vault-1",
    name: "My Workspace",
    path: "/home/user/vaults/workspace-1",
    lastOpenedAt: "2026-03-01T00:00:00Z",
    pinned: false,
    ...overrides,
  };
}

// ── Tests ────────────────────────────────────────────────────────────────────

describe("VaultRoster", () => {
  let roster: VaultRoster;

  beforeEach(() => {
    roster = createVaultRoster();
  });

  // ── CRUD ────────────────────────────────────────────────────────────────────

  describe("add / get / remove", () => {
    it("adds and retrieves an entry", () => {
      const entry = roster.add(makeEntry());
      expect(entry.id).toBe("vault-1");
      expect(entry.addedAt).toBeDefined();

      const got = roster.get("vault-1");
      expect(got).toBeDefined();
      expect(got?.name).toBe("My Workspace");
    });

    it("assigns addedAt automatically", () => {
      const entry = roster.add(makeEntry());
      expect(entry.addedAt).toBeTruthy();
    });

    it("preserves explicit addedAt", () => {
      const entry = roster.add(makeEntry({ addedAt: "2025-01-01T00:00:00Z" }));
      expect(entry.addedAt).toBe("2025-01-01T00:00:00Z");
    });

    it("removes an entry", () => {
      roster.add(makeEntry());
      expect(roster.remove("vault-1")).toBe(true);
      expect(roster.get("vault-1")).toBeUndefined();
      expect(roster.size()).toBe(0);
    });

    it("returns false when removing non-existent", () => {
      expect(roster.remove("nope")).toBe(false);
    });

    it("looks up by path", () => {
      roster.add(makeEntry());
      const entry = roster.getByPath("/home/user/vaults/workspace-1");
      expect(entry?.id).toBe("vault-1");
    });

    it("returns undefined for unknown path", () => {
      expect(roster.getByPath("/nope")).toBeUndefined();
    });

    it("tracks size", () => {
      expect(roster.size()).toBe(0);
      roster.add(makeEntry({ id: "a", path: "/a" }));
      roster.add(makeEntry({ id: "b", path: "/b" }));
      expect(roster.size()).toBe(2);
    });
  });

  // ── Deduplication ──────────────────────────────────────────────────────────

  describe("deduplication", () => {
    it("deduplicates by path — newer entry replaces older", () => {
      roster.add(makeEntry({ id: "old", path: "/same/path", name: "Old" }));
      roster.add(makeEntry({ id: "new", path: "/same/path", name: "New" }));

      expect(roster.size()).toBe(1);
      expect(roster.get("new")?.name).toBe("New");
      expect(roster.get("old")).toBeUndefined();
    });

    it("updates existing entry when same ID is added again", () => {
      roster.add(makeEntry({ name: "Original" }));
      roster.add(makeEntry({ name: "Updated" }));

      expect(roster.size()).toBe(1);
      expect(roster.get("vault-1")?.name).toBe("Updated");
    });
  });

  // ── Update ─────────────────────────────────────────────────────────────────

  describe("update", () => {
    it("patches entry fields", () => {
      roster.add(makeEntry());
      const updated = roster.update("vault-1", { name: "Renamed", description: "A description" });

      expect(updated?.name).toBe("Renamed");
      expect(updated?.description).toBe("A description");
      expect(roster.get("vault-1")?.name).toBe("Renamed");
    });

    it("returns undefined for unknown ID", () => {
      expect(roster.update("nope", { name: "X" })).toBeUndefined();
    });

    it("updates path index when path changes", () => {
      roster.add(makeEntry());
      roster.update("vault-1", { path: "/new/path" });

      expect(roster.getByPath("/new/path")?.id).toBe("vault-1");
      expect(roster.getByPath("/home/user/vaults/workspace-1")).toBeUndefined();
    });
  });

  // ── Touch ──────────────────────────────────────────────────────────────────

  describe("touch", () => {
    it("bumps lastOpenedAt to now", () => {
      roster.add(makeEntry({ lastOpenedAt: "2020-01-01T00:00:00Z" }));
      const before = roster.get("vault-1")?.lastOpenedAt;

      const touched = roster.touch("vault-1");
      expect(touched).toBeDefined();
      expect(touched?.lastOpenedAt).not.toBe(before);
      // ISO strings are lexicographically comparable
      expect((touched?.lastOpenedAt ?? "") > (before ?? "")).toBe(true);
    });

    it("returns undefined for unknown ID", () => {
      expect(roster.touch("nope")).toBeUndefined();
    });
  });

  // ── Pin ────────────────────────────────────────────────────────────────────

  describe("pin", () => {
    it("pins an entry", () => {
      roster.add(makeEntry());
      const pinned = roster.pin("vault-1", true);
      expect(pinned?.pinned).toBe(true);
    });

    it("unpins an entry", () => {
      roster.add(makeEntry({ pinned: true }));
      const unpinned = roster.pin("vault-1", false);
      expect(unpinned?.pinned).toBe(false);
    });
  });

  // ── List & Sort ────────────────────────────────────────────────────────────

  describe("list", () => {
    beforeEach(() => {
      roster.add(makeEntry({
        id: "a", name: "Alpha", path: "/a",
        lastOpenedAt: "2026-01-01T00:00:00Z", pinned: false, tags: ["work"],
      }));
      roster.add(makeEntry({
        id: "b", name: "Beta", path: "/b",
        lastOpenedAt: "2026-03-01T00:00:00Z", pinned: true, tags: ["personal"],
      }));
      roster.add(makeEntry({
        id: "c", name: "Gamma", path: "/c",
        lastOpenedAt: "2026-02-01T00:00:00Z", pinned: false, tags: ["work", "personal"],
      }));
    });

    it("sorts by lastOpenedAt desc by default", () => {
      const list = roster.list();
      const names = list.map((e) => e.name);
      // Pinned first, then by recency
      expect(names).toEqual(["Beta", "Gamma", "Alpha"]);
    });

    it("sorts by name asc", () => {
      const list = roster.list({ sortBy: "name", sortDir: "asc" });
      const names = list.map((e) => e.name);
      // Pinned first, then alphabetical
      expect(names).toEqual(["Beta", "Alpha", "Gamma"]);
    });

    it("filters by pinned", () => {
      const pinned = roster.list({ pinned: true });
      expect(pinned).toHaveLength(1);
      expect(pinned[0]?.name).toBe("Beta");
    });

    it("filters by tags", () => {
      const work = roster.list({ tags: ["work"] });
      expect(work).toHaveLength(2);

      const both = roster.list({ tags: ["work", "personal"] });
      expect(both).toHaveLength(1);
      expect(both[0]?.name).toBe("Gamma");
    });

    it("filters by search text", () => {
      const result = roster.list({ search: "gamma" });
      expect(result).toHaveLength(1);
      expect(result[0]?.name).toBe("Gamma");
    });

    it("limits results", () => {
      const result = roster.list({ limit: 2 });
      expect(result).toHaveLength(2);
    });
  });

  // ── Change events ──────────────────────────────────────────────────────────

  describe("onChange", () => {
    it("emits add event", () => {
      const changes: RosterChange[] = [];
      roster.onChange((c) => changes.push(...c));

      roster.add(makeEntry());
      expect(changes).toHaveLength(1);
      expect(changes[0]?.type).toBe("add");
    });

    it("emits remove event", () => {
      roster.add(makeEntry());

      const changes: RosterChange[] = [];
      roster.onChange((c) => changes.push(...c));

      roster.remove("vault-1");
      expect(changes).toHaveLength(1);
      expect(changes[0]?.type).toBe("remove");
    });

    it("emits update event on update", () => {
      roster.add(makeEntry());

      const changes: RosterChange[] = [];
      roster.onChange((c) => changes.push(...c));

      roster.update("vault-1", { name: "New Name" });
      expect(changes).toHaveLength(1);
      expect(changes[0]?.type).toBe("update");
    });

    it("emits update event on touch", () => {
      roster.add(makeEntry());

      const changes: RosterChange[] = [];
      roster.onChange((c) => changes.push(...c));

      roster.touch("vault-1");
      expect(changes).toHaveLength(1);
      expect(changes[0]?.type).toBe("update");
    });

    it("unsubscribes", () => {
      const handler = vi.fn();
      const unsub = roster.onChange(handler);

      roster.add(makeEntry());
      expect(handler).toHaveBeenCalledTimes(1);

      unsub();
      roster.add(makeEntry({ id: "vault-2", path: "/other" }));
      expect(handler).toHaveBeenCalledTimes(1);
    });
  });

  // ── Persistence ────────────────────────────────────────────────────────────

  describe("persistence", () => {
    it("saves to and reloads from a RosterStore", () => {
      const store = createMemoryRosterStore();
      const r1 = createVaultRoster(store);

      r1.add(makeEntry({ id: "a", path: "/a", name: "Alpha" }));
      r1.add(makeEntry({ id: "b", path: "/b", name: "Beta" }));
      r1.save();

      // Create new roster from same store
      const r2 = createVaultRoster(store);
      expect(r2.size()).toBe(2);
      expect(r2.get("a")?.name).toBe("Alpha");
      expect(r2.get("b")?.name).toBe("Beta");
    });

    it("reload refreshes from store", () => {
      const store = createMemoryRosterStore();
      const r = createVaultRoster(store);

      r.add(makeEntry({ id: "a", path: "/a" }));
      r.save();

      // Simulate external change by saving directly to store
      store.save([
        { ...makeEntry({ id: "a", path: "/a", name: "Changed" }), addedAt: "2026-01-01T00:00:00Z" },
        { ...makeEntry({ id: "b", path: "/b", name: "New" }), addedAt: "2026-01-01T00:00:00Z" },
      ]);

      r.reload();
      expect(r.size()).toBe(2);
      expect(r.get("a")?.name).toBe("Changed");
      expect(r.get("b")?.name).toBe("New");
    });

    it("hydrates from store on creation", () => {
      const store = createMemoryRosterStore();
      store.save([
        { ...makeEntry({ id: "x", path: "/x" }), addedAt: "2026-01-01T00:00:00Z" },
      ]);

      const r = createVaultRoster(store);
      expect(r.size()).toBe(1);
      expect(r.get("x")).toBeDefined();
    });
  });

  // ── all() ──────────────────────────────────────────────────────────────────

  describe("all", () => {
    it("returns all entries as array", () => {
      roster.add(makeEntry({ id: "a", path: "/a" }));
      roster.add(makeEntry({ id: "b", path: "/b" }));

      const all = roster.all();
      expect(all).toHaveLength(2);
    });
  });
});
