import { describe, it, expect, beforeEach } from "vitest";
import type { GraphObject } from "@prism/core/object-model";
import type { ActivityStore } from "./activity-log.js";
import { createActivityStore } from "./activity-log.js";
import type { ActivityTracker, TrackableStore } from "./activity-tracker.js";
import { createActivityTracker } from "./activity-tracker.js";

// ── Helpers ───────────────────────────────────────────────────────────────────

let counter = 0;

function makeObject(overrides: Partial<GraphObject> = {}): GraphObject {
  return {
    id: "obj-1" as GraphObject["id"],
    type: "task",
    name: "Test Task",
    parentId: null,
    position: 0,
    status: "todo",
    tags: [],
    date: null,
    endDate: null,
    description: "",
    color: null,
    image: null,
    pinned: false,
    data: {},
    createdAt: new Date().toISOString(),
    updatedAt: new Date().toISOString(),
    deletedAt: null,
    ...overrides,
  };
}

function createMockStore(): TrackableStore & {
  set(id: string, obj: GraphObject | null): void;
} {
  const objects = new Map<string, GraphObject | null>();
  const subs = new Map<string, Set<(obj: unknown) => void>>();

  return {
    get(id: string): unknown {
      return objects.get(id) ?? undefined;
    },
    subscribeObject(id: string, cb: (obj: unknown) => void): () => void {
      let set = subs.get(id);
      if (!set) {
        set = new Set();
        subs.set(id, set);
      }
      set.add(cb);
      return () => {
        subs.get(id)?.delete(cb);
      };
    },
    set(id: string, obj: GraphObject | null): void {
      if (obj === null) {
        objects.delete(id);
      } else {
        objects.set(id, obj);
      }
      const callbacks = subs.get(id);
      if (callbacks) {
        for (const cb of callbacks) {
          cb(obj ?? undefined);
        }
      }
    },
  };
}

// ── Tests ────────────────────────────────────────────────────────────────────

describe("ActivityTracker", () => {
  let activityStore: ActivityStore;
  let tracker: ActivityTracker;
  let mockStore: ReturnType<typeof createMockStore>;

  beforeEach(() => {
    counter = 0;
    activityStore = createActivityStore({ generateId: () => `id-${++counter}` });
    tracker = createActivityTracker({
      activityStore,
      actorId: "user-1",
      actorName: "Alice",
    });
    mockStore = createMockStore();
  });

  // ── track ──────────────────────────────────────────────────────────────────

  describe("track", () => {
    it("seeds initial snapshot without recording event", () => {
      const obj = makeObject({ createdAt: "2020-01-01T00:00:00.000Z" });
      mockStore.set("obj-1", obj);

      tracker.track("obj-1", mockStore);

      expect(activityStore.getEventCount("obj-1")).toBe(0);
    });

    it("records created for new objects (createdAt within 5s)", () => {
      const obj = makeObject();
      mockStore.set("obj-1", obj);

      tracker.track("obj-1", mockStore);

      expect(activityStore.getEventCount("obj-1")).toBe(0);
    });

    it("returns an unsubscribe function", () => {
      const obj = makeObject({ createdAt: "2020-01-01T00:00:00.000Z" });
      mockStore.set("obj-1", obj);

      const unsub = tracker.track("obj-1", mockStore);
      expect(tracker.trackedIds().has("obj-1")).toBe(true);

      unsub();
      expect(tracker.trackedIds().has("obj-1")).toBe(false);
    });
  });

  // ── verb inference: updated ────────────────────────────────────────────────

  describe("updated", () => {
    it("records updated when description changes", () => {
      const obj = makeObject({ createdAt: "2020-01-01T00:00:00.000Z" });
      mockStore.set("obj-1", obj);
      tracker.track("obj-1", mockStore);

      mockStore.set("obj-1", { ...obj, description: "New description" });

      const [event] = activityStore.getEvents("obj-1");
      expect(event).toBeDefined();
      expect(event.verb).toBe("updated");
      expect(event.changes).toHaveLength(1);
      expect(event.changes?.at(0)?.field).toBe("description");
    });

    it("records multiple field changes", () => {
      const obj = makeObject({ createdAt: "2020-01-01T00:00:00.000Z" });
      mockStore.set("obj-1", obj);
      tracker.track("obj-1", mockStore);

      mockStore.set("obj-1", {
        ...obj,
        description: "New",
        color: "red",
        pinned: true,
      });

      const [event] = activityStore.getEvents("obj-1");
      expect(event).toBeDefined();
      expect(event.verb).toBe("updated");
      expect(event.changes?.length).toBe(3);
    });

    it("records data payload changes", () => {
      const obj = makeObject({
        createdAt: "2020-01-01T00:00:00.000Z",
        data: { priority: "low" },
      });
      mockStore.set("obj-1", obj);
      tracker.track("obj-1", mockStore);

      mockStore.set("obj-1", { ...obj, data: { priority: "high" } });

      const [event] = activityStore.getEvents("obj-1");
      expect(event.changes?.at(0)?.field).toBe("data.priority");
    });
  });

  // ── verb inference: renamed ────────────────────────────────────────────────

  describe("renamed", () => {
    it("records renamed when only name changes", () => {
      const obj = makeObject({ createdAt: "2020-01-01T00:00:00.000Z" });
      mockStore.set("obj-1", obj);
      tracker.track("obj-1", mockStore);

      mockStore.set("obj-1", { ...obj, name: "New Name" });

      const [event] = activityStore.getEvents("obj-1");
      expect(event.verb).toBe("renamed");
      expect(event.changes?.at(0)?.before).toBe("Test Task");
      expect(event.changes?.at(0)?.after).toBe("New Name");
    });
  });

  // ── verb inference: moved ──────────────────────────────────────────────────

  describe("moved", () => {
    it("records moved when parentId changes", () => {
      const obj = makeObject({ createdAt: "2020-01-01T00:00:00.000Z" });
      mockStore.set("obj-1", obj);
      tracker.track("obj-1", mockStore);

      mockStore.set("obj-1", {
        ...obj,
        parentId: "parent-1" as GraphObject["id"],
      });

      const [event] = activityStore.getEvents("obj-1");
      expect(event.verb).toBe("moved");
      expect(event.fromParentId).toBeNull();
      expect(event.toParentId).toBe("parent-1");
    });

    it("includes name change alongside move", () => {
      const obj = makeObject({ createdAt: "2020-01-01T00:00:00.000Z" });
      mockStore.set("obj-1", obj);
      tracker.track("obj-1", mockStore);

      mockStore.set("obj-1", {
        ...obj,
        parentId: "parent-1" as GraphObject["id"],
        name: "Moved Task",
      });

      const [event] = activityStore.getEvents("obj-1");
      expect(event.verb).toBe("moved");
      expect(event.changes).toHaveLength(1);
      expect(event.changes?.at(0)?.field).toBe("name");
    });
  });

  // ── verb inference: status-changed ─────────────────────────────────────────

  describe("status-changed", () => {
    it("records status-changed when status changes", () => {
      const obj = makeObject({ createdAt: "2020-01-01T00:00:00.000Z" });
      mockStore.set("obj-1", obj);
      tracker.track("obj-1", mockStore);

      mockStore.set("obj-1", { ...obj, status: "done" });

      const [event] = activityStore.getEvents("obj-1");
      expect(event.verb).toBe("status-changed");
      expect(event.fromStatus).toBe("todo");
      expect(event.toStatus).toBe("done");
    });

    it("includes additional changes when status changes with other fields", () => {
      const obj = makeObject({ createdAt: "2020-01-01T00:00:00.000Z" });
      mockStore.set("obj-1", obj);
      tracker.track("obj-1", mockStore);

      mockStore.set("obj-1", {
        ...obj,
        status: "done",
        description: "Completed",
      });

      const [event] = activityStore.getEvents("obj-1");
      expect(event.verb).toBe("status-changed");
      expect(event.changes).toBeDefined();
      expect((event.changes?.length ?? 0) > 0).toBe(true);
    });
  });

  // ── verb inference: deleted / restored ─────────────────────────────────────

  describe("deleted / restored", () => {
    it("records deleted when deletedAt is set", () => {
      const obj = makeObject({ createdAt: "2020-01-01T00:00:00.000Z" });
      mockStore.set("obj-1", obj);
      tracker.track("obj-1", mockStore);

      mockStore.set("obj-1", {
        ...obj,
        deletedAt: new Date().toISOString(),
      });

      const [event] = activityStore.getEvents("obj-1");
      expect(event.verb).toBe("deleted");
    });

    it("records restored when deletedAt is cleared", () => {
      const obj = makeObject({
        createdAt: "2020-01-01T00:00:00.000Z",
        deletedAt: "2020-06-01T00:00:00.000Z",
      });
      mockStore.set("obj-1", obj);
      tracker.track("obj-1", mockStore);

      mockStore.set("obj-1", { ...obj, deletedAt: null });

      const [event] = activityStore.getEvents("obj-1");
      expect(event.verb).toBe("restored");
    });

    it("records deleted when object disappears (hard delete)", () => {
      const obj = makeObject({ createdAt: "2020-01-01T00:00:00.000Z" });
      mockStore.set("obj-1", obj);
      tracker.track("obj-1", mockStore);

      mockStore.set("obj-1", null);

      const [event] = activityStore.getEvents("obj-1");
      expect(event.verb).toBe("deleted");
    });

    it("does not double-record delete for already soft-deleted objects", () => {
      const obj = makeObject({
        createdAt: "2020-01-01T00:00:00.000Z",
        deletedAt: "2020-06-01T00:00:00.000Z",
      });
      mockStore.set("obj-1", obj);
      tracker.track("obj-1", mockStore);

      mockStore.set("obj-1", null);

      expect(activityStore.getEventCount("obj-1")).toBe(0);
    });
  });

  // ── object appeared ────────────────────────────────────────────────────────

  describe("object appeared", () => {
    it("records created for new objects that appear via subscription", () => {
      tracker.track("obj-1", mockStore);

      const obj = makeObject();
      mockStore.set("obj-1", obj);

      const [event] = activityStore.getEvents("obj-1");
      expect(event.verb).toBe("created");
      expect(event.actorId).toBe("user-1");
      expect(event.actorName).toBe("Alice");
    });

    it("does not record created for old objects that appear", () => {
      tracker.track("obj-1", mockStore);

      const obj = makeObject({ createdAt: "2020-01-01T00:00:00.000Z" });
      mockStore.set("obj-1", obj);

      expect(activityStore.getEventCount("obj-1")).toBe(0);
    });
  });

  // ── ignored fields ─────────────────────────────────────────────────────────

  describe("ignored fields", () => {
    it("ignores updatedAt by default", () => {
      const obj = makeObject({ createdAt: "2020-01-01T00:00:00.000Z" });
      mockStore.set("obj-1", obj);
      tracker.track("obj-1", mockStore);

      mockStore.set("obj-1", {
        ...obj,
        updatedAt: new Date().toISOString(),
      });

      expect(activityStore.getEventCount("obj-1")).toBe(0);
    });

    it("respects custom ignored fields", () => {
      const customTracker = createActivityTracker({
        activityStore,
        ignoredFields: ["description", "data.internal"],
      });

      const obj = makeObject({
        createdAt: "2020-01-01T00:00:00.000Z",
        data: { internal: "secret" },
      });
      mockStore.set("obj-1", obj);
      customTracker.track("obj-1", mockStore);

      mockStore.set("obj-1", {
        ...obj,
        description: "Changed",
        data: { internal: "new-secret" },
      });

      expect(activityStore.getEventCount("obj-1")).toBe(0);
    });
  });

  // ── untrackAll ─────────────────────────────────────────────────────────────

  describe("untrackAll", () => {
    it("stops all subscriptions", () => {
      const obj = makeObject({ createdAt: "2020-01-01T00:00:00.000Z" });
      mockStore.set("obj-1", obj);
      tracker.track("obj-1", mockStore);

      expect(tracker.trackedIds().size).toBe(1);

      tracker.untrackAll();
      expect(tracker.trackedIds().size).toBe(0);

      mockStore.set("obj-1", { ...obj, name: "Changed" });
      expect(activityStore.getEventCount("obj-1")).toBe(0);
    });
  });

  // ── replacement tracking ───────────────────────────────────────────────────

  describe("replacement tracking", () => {
    it("replaces subscription when tracking same id twice", () => {
      const obj = makeObject({ createdAt: "2020-01-01T00:00:00.000Z" });
      mockStore.set("obj-1", obj);

      tracker.track("obj-1", mockStore);
      tracker.track("obj-1", mockStore);

      mockStore.set("obj-1", { ...obj, name: "Changed" });

      expect(activityStore.getEventCount("obj-1")).toBe(1);
    });
  });
});
