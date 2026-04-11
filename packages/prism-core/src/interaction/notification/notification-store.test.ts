import { describe, it, expect, beforeEach, vi } from "vitest";
import type {
  NotificationStore,
  NotificationChange,
  Notification,
  NotificationInput,
} from "./notification-store.js";
import { createNotificationStore } from "./notification-store.js";

// ── Helpers ───────────────────────────────────────────────────────────────────

let counter = 0;

function makeInput(overrides: Partial<NotificationInput> = {}): NotificationInput {
  return {
    kind: "info",
    title: `Notification ${++counter}`,
    ...overrides,
  };
}

// ── Tests ────────────────────────────────────────────────────────────────────

describe("NotificationStore", () => {
  let store: NotificationStore;

  beforeEach(() => {
    counter = 0;
    store = createNotificationStore({ generateId: () => `id-${++counter}` });
  });

  // ── Add ─────────────────────────────────────────────────────────────────────

  describe("add", () => {
    it("creates a notification with defaults", () => {
      const n = store.add(makeInput({ title: "Hello" }));
      expect(n.id).toBeTruthy();
      expect(n.title).toBe("Hello");
      expect(n.read).toBe(false);
      expect(n.pinned).toBe(false);
      expect(n.createdAt).toBeTruthy();
      expect(n.dismissedAt).toBeUndefined();
    });

    it("increments size", () => {
      store.add(makeInput());
      store.add(makeInput());
      expect(store.size()).toBe(2);
    });

    it("preserves optional fields", () => {
      const n = store.add(makeInput({
        body: "Details here",
        objectId: "obj-1",
        objectType: "task",
        actorId: "user-1",
        data: { foo: "bar" },
      }));
      expect(n.body).toBe("Details here");
      expect(n.objectId).toBe("obj-1");
      expect(n.objectType).toBe("task");
      expect(n.actorId).toBe("user-1");
      expect(n.data?.foo).toBe("bar");
    });
  });

  // ── Get ─────────────────────────────────────────────────────────────────────

  describe("get", () => {
    it("retrieves by ID", () => {
      const n = store.add(makeInput());
      expect(store.get(n.id)?.title).toBe(n.title);
    });

    it("returns undefined for unknown ID", () => {
      expect(store.get("nope")).toBeUndefined();
    });
  });

  // ── Mark Read ──────────────────────────────────────────────────────────────

  describe("markRead", () => {
    it("marks a notification as read", () => {
      const n = store.add(makeInput());
      const updated = store.markRead(n.id);
      expect(updated?.read).toBe(true);
      expect(updated?.readAt).toBeTruthy();
    });

    it("no-op if already read", () => {
      const n = store.add(makeInput());
      store.markRead(n.id);
      const again = store.markRead(n.id);
      expect(again?.read).toBe(true);
    });

    it("returns undefined for unknown ID", () => {
      expect(store.markRead("nope")).toBeUndefined();
    });
  });

  describe("markAllRead", () => {
    it("marks all unread as read", () => {
      store.add(makeInput());
      store.add(makeInput());
      store.add(makeInput());

      const count = store.markAllRead();
      expect(count).toBe(3);
      expect(store.getUnreadCount()).toBe(0);
    });

    it("respects filter", () => {
      store.add(makeInput({ kind: "info" }));
      store.add(makeInput({ kind: "warning" }));

      const count = store.markAllRead({ kind: ["info"] });
      expect(count).toBe(1);
      expect(store.getUnreadCount()).toBe(1);
    });
  });

  // ── Dismiss ────────────────────────────────────────────────────────────────

  describe("dismiss", () => {
    it("soft-deletes a notification", () => {
      const n = store.add(makeInput());
      const dismissed = store.dismiss(n.id);
      expect(dismissed?.dismissedAt).toBeTruthy();
      expect(store.size()).toBe(0); // not visible
    });

    it("excluded from getAll", () => {
      const n = store.add(makeInput());
      store.dismiss(n.id);
      expect(store.getAll()).toHaveLength(0);
    });
  });

  describe("dismissAll", () => {
    it("dismisses all matching", () => {
      store.add(makeInput({ kind: "info" }));
      store.add(makeInput({ kind: "info" }));
      store.add(makeInput({ kind: "warning" }));

      const count = store.dismissAll({ kind: ["info"] });
      expect(count).toBe(2);
      expect(store.getAll()).toHaveLength(1);
    });
  });

  // ── Pin ────────────────────────────────────────────────────────────────────

  describe("pin / unpin", () => {
    it("pins a notification", () => {
      const n = store.add(makeInput());
      const pinned = store.pin(n.id);
      expect(pinned?.pinned).toBe(true);
    });

    it("unpins a notification", () => {
      const n = store.add(makeInput());
      store.pin(n.id);
      const unpinned = store.unpin(n.id);
      expect(unpinned?.pinned).toBe(false);
    });
  });

  // ── getAll ─────────────────────────────────────────────────────────────────

  describe("getAll", () => {
    it("returns newest first", () => {
      store.add(makeInput({ title: "First", createdAt: "2026-01-01T00:00:00Z" }));
      store.add(makeInput({ title: "Second", createdAt: "2026-02-01T00:00:00Z" }));
      store.add(makeInput({ title: "Third", createdAt: "2026-03-01T00:00:00Z" }));

      const all = store.getAll();
      expect(all.map((n) => n.title)).toEqual(["Third", "Second", "First"]);
    });

    it("excludes dismissed", () => {
      const n = store.add(makeInput());
      store.add(makeInput());
      store.dismiss(n.id);
      expect(store.getAll()).toHaveLength(1);
    });

    it("excludes expired", () => {
      store.add(makeInput({ expiresAt: "2020-01-01T00:00:00Z" }));
      store.add(makeInput());
      expect(store.getAll()).toHaveLength(1);
    });

    it("filters by kind", () => {
      store.add(makeInput({ kind: "info" }));
      store.add(makeInput({ kind: "warning" }));
      store.add(makeInput({ kind: "error" }));

      const result = store.getAll({ kind: ["info", "warning"] });
      expect(result).toHaveLength(2);
    });

    it("filters by read", () => {
      const n = store.add(makeInput());
      store.add(makeInput());
      store.markRead(n.id);

      expect(store.getAll({ read: true })).toHaveLength(1);
      expect(store.getAll({ read: false })).toHaveLength(1);
    });

    it("filters by objectId", () => {
      store.add(makeInput({ objectId: "obj-1" }));
      store.add(makeInput({ objectId: "obj-2" }));

      expect(store.getAll({ objectId: "obj-1" })).toHaveLength(1);
    });

    it("filters by since", () => {
      store.add(makeInput({ createdAt: "2026-01-01T00:00:00Z" }));
      store.add(makeInput({ createdAt: "2026-03-01T00:00:00Z" }));

      expect(store.getAll({ since: "2026-02-01T00:00:00Z" })).toHaveLength(1);
    });
  });

  // ── getUnreadCount ─────────────────────────────────────────────────────────

  describe("getUnreadCount", () => {
    it("counts unread non-dismissed", () => {
      store.add(makeInput());
      store.add(makeInput());
      const n3 = store.add(makeInput());
      store.markRead(n3.id);

      expect(store.getUnreadCount()).toBe(2);
    });

    it("excludes dismissed from count", () => {
      const n = store.add(makeInput());
      store.add(makeInput());
      store.dismiss(n.id);

      expect(store.getUnreadCount()).toBe(1);
    });

    it("respects filter", () => {
      store.add(makeInput({ kind: "info" }));
      store.add(makeInput({ kind: "warning" }));

      expect(store.getUnreadCount({ kind: ["info"] })).toBe(1);
    });
  });

  // ── Eviction ───────────────────────────────────────────────────────────────

  describe("eviction", () => {
    it("evicts dismissed unpinned first when over max", () => {
      const s = createNotificationStore({ maxItems: 3, generateId: () => `id-${++counter}` });

      const n1 = s.add(makeInput({ createdAt: "2026-01-01T00:00:00Z" }));
      s.add(makeInput({ createdAt: "2026-02-01T00:00:00Z" }));
      s.add(makeInput({ createdAt: "2026-03-01T00:00:00Z" }));
      s.dismiss(n1.id);

      // Adding a 4th triggers eviction — dismissed n1 goes first
      s.add(makeInput({ createdAt: "2026-04-01T00:00:00Z" }));
      expect(s.get(n1.id)).toBeUndefined();
    });

    it("evicts read unpinned after dismissed", () => {
      const s = createNotificationStore({ maxItems: 3, generateId: () => `id-${++counter}` });

      const n1 = s.add(makeInput({ createdAt: "2026-01-01T00:00:00Z" }));
      s.add(makeInput({ createdAt: "2026-02-01T00:00:00Z" }));
      s.add(makeInput({ createdAt: "2026-03-01T00:00:00Z" }));
      s.markRead(n1.id);

      s.add(makeInput({ createdAt: "2026-04-01T00:00:00Z" }));
      expect(s.get(n1.id)).toBeUndefined();
    });

    it("never evicts pinned items", () => {
      const s = createNotificationStore({ maxItems: 2, generateId: () => `id-${++counter}` });

      const n1 = s.add(makeInput({ createdAt: "2026-01-01T00:00:00Z" }));
      s.pin(n1.id);
      s.markRead(n1.id);

      const n2 = s.add(makeInput({ createdAt: "2026-02-01T00:00:00Z" }));
      s.markRead(n2.id);

      s.add(makeInput({ createdAt: "2026-03-01T00:00:00Z" }));

      // n1 is pinned — should survive; n2 is read unpinned — evicted
      expect(s.get(n1.id)).toBeDefined();
      expect(s.get(n2.id)).toBeUndefined();
    });
  });

  // ── Subscribe ──────────────────────────────────────────────────────────────

  describe("subscribe", () => {
    it("emits on add", () => {
      const changes: NotificationChange[] = [];
      store.subscribe((c) => changes.push(c));

      store.add(makeInput());
      expect(changes).toHaveLength(1);
      expect(changes[0]?.type).toBe("add");
    });

    it("emits on markRead", () => {
      const n = store.add(makeInput());
      const changes: NotificationChange[] = [];
      store.subscribe((c) => changes.push(c));

      store.markRead(n.id);
      expect(changes).toHaveLength(1);
      expect(changes[0]?.type).toBe("update");
    });

    it("emits on dismiss", () => {
      const n = store.add(makeInput());
      const changes: NotificationChange[] = [];
      store.subscribe((c) => changes.push(c));

      store.dismiss(n.id);
      expect(changes).toHaveLength(1);
      expect(changes[0]?.type).toBe("dismiss");
    });

    it("unsubscribes", () => {
      const handler = vi.fn();
      const unsub = store.subscribe(handler);

      store.add(makeInput());
      expect(handler).toHaveBeenCalledTimes(1);

      unsub();
      store.add(makeInput());
      expect(handler).toHaveBeenCalledTimes(1);
    });
  });

  // ── Hydrate ────────────────────────────────────────────────────────────────

  describe("hydrate", () => {
    it("bulk loads notifications", () => {
      const items: Notification[] = [
        { id: "a", kind: "info", title: "A", read: false, pinned: false, createdAt: "2026-01-01T00:00:00Z" },
        { id: "b", kind: "info", title: "B", read: false, pinned: false, createdAt: "2026-02-01T00:00:00Z" },
      ];
      store.hydrate(items);
      expect(store.size()).toBe(2);
      expect(store.getAll()[0]?.id).toBe("b"); // newest first
    });

    it("clears existing items", () => {
      store.add(makeInput());
      store.hydrate([]);
      expect(store.size()).toBe(0);
    });
  });

  // ── Clear ──────────────────────────────────────────────────────────────────

  describe("clear", () => {
    it("removes dismissed unpinned items", () => {
      const n1 = store.add(makeInput());
      store.add(makeInput());
      store.dismiss(n1.id);

      const count = store.clear();
      expect(count).toBe(1);
      expect(store.get(n1.id)).toBeUndefined();
    });

    it("preserves dismissed pinned items", () => {
      const n = store.add(makeInput());
      store.pin(n.id);
      store.dismiss(n.id);

      store.clear();
      expect(store.get(n.id)).toBeDefined();
    });
  });
});
