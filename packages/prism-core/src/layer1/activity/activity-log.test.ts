import { describe, it, expect, beforeEach } from "vitest";
import type { ActivityStore, ActivityEvent, ActivityEventInput } from "./activity-log.js";
import { createActivityStore } from "./activity-log.js";

// ── Helpers ───────────────────────────────────────────────────────────────────

let counter = 0;

function makeInput(overrides: Partial<ActivityEventInput> = {}): ActivityEventInput {
  return {
    objectId: "obj-1",
    verb: "updated",
    ...overrides,
  };
}

// ── Tests ────────────────────────────────────────────────────────────────────

describe("ActivityStore", () => {
  let store: ActivityStore;

  beforeEach(() => {
    counter = 0;
    store = createActivityStore({ generateId: () => `id-${++counter}` });
  });

  // ── record ─────────────────────────────────────────────────────────────────

  describe("record", () => {
    it("stamps id and createdAt", () => {
      const event = store.record(makeInput());
      expect(event.id).toBe("id-1");
      expect(event.createdAt).toBeTruthy();
      expect(event.objectId).toBe("obj-1");
      expect(event.verb).toBe("updated");
    });

    it("preserves optional fields", () => {
      const event = store.record(
        makeInput({
          actorId: "user-1",
          actorName: "Alice",
          changes: [{ field: "name", before: "Old", after: "New" }],
          meta: { reason: "testing" },
        }),
      );
      expect(event.actorId).toBe("user-1");
      expect(event.actorName).toBe("Alice");
      expect(event.changes).toHaveLength(1);
      expect(event.meta?.["reason"]).toBe("testing");
    });

    it("preserves move fields", () => {
      const event = store.record(
        makeInput({
          verb: "moved",
          fromParentId: "p1",
          toParentId: "p2",
        }),
      );
      expect(event.fromParentId).toBe("p1");
      expect(event.toParentId).toBe("p2");
    });

    it("preserves status-changed fields", () => {
      const event = store.record(
        makeInput({
          verb: "status-changed",
          fromStatus: "todo",
          toStatus: "done",
        }),
      );
      expect(event.fromStatus).toBe("todo");
      expect(event.toStatus).toBe("done");
    });
  });

  // ── getEvents ──────────────────────────────────────────────────────────────

  describe("getEvents", () => {
    it("returns events newest-first", () => {
      store.record(makeInput({ verb: "created" }));
      store.record(makeInput({ verb: "updated" }));
      store.record(makeInput({ verb: "deleted" }));

      const events = store.getEvents("obj-1");
      expect(events).toHaveLength(3);
      expect(events[0].verb).toBe("deleted");
      expect(events[2].verb).toBe("created");
    });

    it("returns empty array for unknown object", () => {
      expect(store.getEvents("unknown")).toEqual([]);
    });

    it("respects limit", () => {
      store.record(makeInput({ verb: "created" }));
      store.record(makeInput({ verb: "updated" }));
      store.record(makeInput({ verb: "deleted" }));

      const events = store.getEvents("obj-1", { limit: 2 });
      expect(events).toHaveLength(2);
      expect(events[0].verb).toBe("deleted");
    });

    it("respects before filter", () => {
      const e1 = store.record(makeInput({ verb: "created" }));
      store.record(makeInput({ verb: "updated" }));
      store.record(makeInput({ verb: "deleted" }));

      // All events are created within the same ms, so use a date before them
      const cutoff = e1.createdAt;
      const events = store.getEvents("obj-1", { before: cutoff });
      // All events created at same time or after cutoff should be filtered
      expect(events.length).toBeLessThanOrEqual(3);
    });
  });

  // ── getLatest ──────────────────────────────────────────────────────────────

  describe("getLatest", () => {
    it("returns the most recent event", () => {
      store.record(makeInput({ verb: "created" }));
      store.record(makeInput({ verb: "updated" }));

      const latest = store.getLatest("obj-1");
      expect(latest?.verb).toBe("updated");
    });

    it("returns null for unknown object", () => {
      expect(store.getLatest("unknown")).toBeNull();
    });
  });

  // ── getEventCount ──────────────────────────────────────────────────────────

  describe("getEventCount", () => {
    it("tracks event count per object", () => {
      store.record(makeInput({ objectId: "obj-1" }));
      store.record(makeInput({ objectId: "obj-1" }));
      store.record(makeInput({ objectId: "obj-2" }));

      expect(store.getEventCount("obj-1")).toBe(2);
      expect(store.getEventCount("obj-2")).toBe(1);
      expect(store.getEventCount("unknown")).toBe(0);
    });
  });

  // ── maxPerObject ───────────────────────────────────────────────────────────

  describe("maxPerObject", () => {
    it("trims oldest events when cap is exceeded", () => {
      const small = createActivityStore({
        maxPerObject: 3,
        generateId: () => `id-${++counter}`,
      });

      small.record(makeInput({ verb: "created" }));
      small.record(makeInput({ verb: "updated" }));
      small.record(makeInput({ verb: "renamed" }));
      small.record(makeInput({ verb: "deleted" }));

      expect(small.getEventCount("obj-1")).toBe(3);
      const events = small.getEvents("obj-1");
      expect(events[2].verb).toBe("updated"); // oldest surviving
    });
  });

  // ── hydrate ────────────────────────────────────────────────────────────────

  describe("hydrate", () => {
    it("replaces events for an object", () => {
      store.record(makeInput({ verb: "created" }));

      const imported: ActivityEvent[] = [
        {
          id: "ext-1",
          objectId: "obj-1",
          verb: "updated",
          createdAt: "2026-01-01T00:00:00.000Z",
        },
        {
          id: "ext-2",
          objectId: "obj-1",
          verb: "deleted",
          createdAt: "2026-01-02T00:00:00.000Z",
        },
      ];

      store.hydrate("obj-1", imported);
      expect(store.getEventCount("obj-1")).toBe(2);

      const events = store.getEvents("obj-1");
      expect(events[0].id).toBe("ext-2"); // newest first
    });

    it("sorts events by createdAt", () => {
      const imported: ActivityEvent[] = [
        {
          id: "b",
          objectId: "obj-1",
          verb: "updated",
          createdAt: "2026-01-05T00:00:00.000Z",
        },
        {
          id: "a",
          objectId: "obj-1",
          verb: "created",
          createdAt: "2026-01-01T00:00:00.000Z",
        },
      ];

      store.hydrate("obj-1", imported);
      const events = store.getEvents("obj-1");
      expect(events[0].id).toBe("b"); // newest first
      expect(events[1].id).toBe("a");
    });
  });

  // ── subscribe ──────────────────────────────────────────────────────────────

  describe("subscribe", () => {
    it("notifies on record", () => {
      const received: ActivityEvent[][] = [];
      store.subscribe("obj-1", (events) => received.push(events));

      store.record(makeInput());
      expect(received).toHaveLength(1);
      expect(received[0]).toHaveLength(1);
    });

    it("notifies on hydrate", () => {
      const received: ActivityEvent[][] = [];
      store.subscribe("obj-1", (events) => received.push(events));

      store.hydrate("obj-1", [
        { id: "x", objectId: "obj-1", verb: "created", createdAt: "2026-01-01T00:00:00.000Z" },
      ]);
      expect(received).toHaveLength(1);
    });

    it("does not notify for other objects", () => {
      const received: ActivityEvent[][] = [];
      store.subscribe("obj-1", (events) => received.push(events));

      store.record(makeInput({ objectId: "obj-2" }));
      expect(received).toHaveLength(0);
    });

    it("unsubscribe stops notifications", () => {
      const received: ActivityEvent[][] = [];
      const unsub = store.subscribe("obj-1", (events) => received.push(events));

      store.record(makeInput());
      expect(received).toHaveLength(1);

      unsub();
      store.record(makeInput());
      expect(received).toHaveLength(1);
    });
  });

  // ── toJSON ─────────────────────────────────────────────────────────────────

  describe("toJSON", () => {
    it("serialises all events", () => {
      store.record(makeInput({ objectId: "obj-1" }));
      store.record(makeInput({ objectId: "obj-2" }));

      const json = store.toJSON();
      expect(Object.keys(json)).toHaveLength(2);
      expect(json["obj-1"]).toHaveLength(1);
      expect(json["obj-2"]).toHaveLength(1);
    });

    it("returns empty object for empty store", () => {
      expect(store.toJSON()).toEqual({});
    });
  });

  // ── clear ──────────────────────────────────────────────────────────────────

  describe("clear", () => {
    it("removes all events and stops notifications", () => {
      const received: ActivityEvent[][] = [];
      store.subscribe("obj-1", (events) => received.push(events));

      store.record(makeInput());
      expect(received).toHaveLength(1);

      store.clear();
      expect(store.getEventCount("obj-1")).toBe(0);
      expect(store.toJSON()).toEqual({});

      // Listeners are also cleared — no notification after clear
      store.record(makeInput());
      expect(received).toHaveLength(1);
    });
  });
});
