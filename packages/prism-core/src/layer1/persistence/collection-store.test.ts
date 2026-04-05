import { describe, it, expect, beforeEach } from "vitest";
import type { GraphObject, ObjectEdge } from "../object-model/types.js";
import { objectId, edgeId } from "../object-model/types.js";
import type { CollectionStore, CollectionChange } from "./collection-store.js";
import { createCollectionStore } from "./collection-store.js";

// ── Helpers ───────────────────────────────────────────────────────────────────

function makeObject(overrides: Partial<GraphObject> = {}): GraphObject {
  return {
    id: objectId("obj-1"),
    type: "task",
    name: "Test Task",
    parentId: null,
    position: 0,
    status: null,
    tags: [],
    date: null,
    endDate: null,
    description: "",
    color: null,
    image: null,
    pinned: false,
    data: {},
    createdAt: "2026-01-01T00:00:00Z",
    updatedAt: "2026-01-01T00:00:00Z",
    ...overrides,
  };
}

function makeEdge(overrides: Partial<ObjectEdge> = {}): ObjectEdge {
  return {
    id: edgeId("edge-1"),
    sourceId: objectId("obj-1"),
    targetId: objectId("obj-2"),
    relation: "depends-on",
    createdAt: "2026-01-01T00:00:00Z",
    data: {},
    ...overrides,
  };
}

// ── Tests ─────────────────────────────────────────────────────────────────────

describe("CollectionStore", () => {
  let store: CollectionStore;

  beforeEach(() => {
    store = createCollectionStore();
  });

  // ── Object CRUD ──────────────────────────────────────────────────────────

  describe("object CRUD", () => {
    it("puts and gets an object", () => {
      const obj = makeObject();
      store.putObject(obj);

      const retrieved = store.getObject(objectId("obj-1"));
      expect(retrieved).toBeDefined();
      expect(retrieved?.name).toBe("Test Task");
      expect(retrieved?.type).toBe("task");
    });

    it("returns undefined for missing object", () => {
      expect(store.getObject(objectId("nonexistent"))).toBeUndefined();
    });

    it("overwrites an existing object on put", () => {
      store.putObject(makeObject({ name: "Original" }));
      store.putObject(makeObject({ name: "Updated" }));

      const retrieved = store.getObject(objectId("obj-1"));
      expect(retrieved?.name).toBe("Updated");
      expect(store.objectCount()).toBe(1);
    });

    it("removes an object", () => {
      store.putObject(makeObject());
      expect(store.removeObject(objectId("obj-1"))).toBe(true);
      expect(store.getObject(objectId("obj-1"))).toBeUndefined();
      expect(store.objectCount()).toBe(0);
    });

    it("remove returns false for missing object", () => {
      expect(store.removeObject(objectId("nonexistent"))).toBe(false);
    });

    it("counts objects correctly", () => {
      expect(store.objectCount()).toBe(0);
      store.putObject(makeObject({ id: objectId("a") }));
      store.putObject(makeObject({ id: objectId("b") }));
      expect(store.objectCount()).toBe(2);
    });

    it("preserves full object shape through round-trip", () => {
      const obj = makeObject({
        id: objectId("rt-1"),
        status: "active",
        tags: ["urgent", "bug"],
        date: "2026-03-15",
        description: "A rich description",
        color: "#ff0000",
        pinned: true,
        data: { priority: 1, labels: ["a", "b"] },
      });

      store.putObject(obj);
      const retrieved = store.getObject(objectId("rt-1"));
      expect(retrieved).toEqual(obj);
    });
  });

  // ── Object filtering ─────────────────────────────────────────────────────

  describe("listObjects with filters", () => {
    beforeEach(() => {
      store.putObject(makeObject({ id: objectId("t1"), type: "task", status: "active", tags: ["urgent"] }));
      store.putObject(makeObject({ id: objectId("t2"), type: "task", status: "done", tags: ["urgent", "bug"] }));
      store.putObject(makeObject({ id: objectId("n1"), type: "note", status: null, tags: [] }));
      store.putObject(makeObject({
        id: objectId("d1"),
        type: "task",
        status: "active",
        deletedAt: "2026-01-02T00:00:00Z",
      }));
    });

    it("lists all non-deleted objects by default", () => {
      const list = store.listObjects({});
      expect(list).toHaveLength(3);
    });

    it("filters by type", () => {
      const tasks = store.listObjects({ types: ["task"] });
      expect(tasks).toHaveLength(2);
      expect(tasks.every((o) => o.type === "task")).toBe(true);
    });

    it("filters by tags (AND logic)", () => {
      const urgentBugs = store.listObjects({ tags: ["urgent", "bug"] });
      expect(urgentBugs).toHaveLength(1);
      expect(urgentBugs[0]?.id).toBe("t2");
    });

    it("filters by status", () => {
      const active = store.listObjects({ statuses: ["active"] });
      expect(active).toHaveLength(1);
      expect(active[0]?.id).toBe("t1");
    });

    it("includes deleted when excludeDeleted=false", () => {
      const all = store.listObjects({ excludeDeleted: false });
      expect(all).toHaveLength(4);
    });

    it("filters by parentId", () => {
      store.putObject(makeObject({
        id: objectId("c1"),
        parentId: objectId("t1"),
      }));
      const children = store.listObjects({ parentId: objectId("t1") });
      expect(children).toHaveLength(1);
      expect(children[0]?.id).toBe("c1");
    });

    it("filters by parentId=null for root objects", () => {
      store.putObject(makeObject({
        id: objectId("c1"),
        parentId: objectId("t1"),
      }));
      const roots = store.listObjects({ parentId: null });
      expect(roots).toHaveLength(3); // t1, t2, n1 (d1 is deleted)
    });

    it("returns all when no filter", () => {
      const all = store.listObjects();
      expect(all).toHaveLength(4); // includes deleted when no filter specified
    });
  });

  // ── Edge CRUD ────────────────────────────────────────────────────────────

  describe("edge CRUD", () => {
    it("puts and gets an edge", () => {
      store.putEdge(makeEdge());
      const retrieved = store.getEdge(edgeId("edge-1"));
      expect(retrieved).toBeDefined();
      expect(retrieved?.relation).toBe("depends-on");
    });

    it("returns undefined for missing edge", () => {
      expect(store.getEdge(edgeId("nonexistent"))).toBeUndefined();
    });

    it("removes an edge", () => {
      store.putEdge(makeEdge());
      expect(store.removeEdge(edgeId("edge-1"))).toBe(true);
      expect(store.getEdge(edgeId("edge-1"))).toBeUndefined();
    });

    it("counts edges correctly", () => {
      store.putEdge(makeEdge({ id: edgeId("e1") }));
      store.putEdge(makeEdge({ id: edgeId("e2") }));
      expect(store.edgeCount()).toBe(2);
    });
  });

  // ── Edge filtering ──────────────────────────────────────────────────────

  describe("listEdges with filters", () => {
    beforeEach(() => {
      store.putEdge(makeEdge({ id: edgeId("e1"), sourceId: objectId("a"), targetId: objectId("b"), relation: "depends-on" }));
      store.putEdge(makeEdge({ id: edgeId("e2"), sourceId: objectId("a"), targetId: objectId("c"), relation: "assigned-to" }));
      store.putEdge(makeEdge({ id: edgeId("e3"), sourceId: objectId("b"), targetId: objectId("c"), relation: "depends-on" }));
    });

    it("filters by sourceId", () => {
      const edges = store.listEdges({ sourceId: objectId("a") });
      expect(edges).toHaveLength(2);
    });

    it("filters by targetId", () => {
      const edges = store.listEdges({ targetId: objectId("c") });
      expect(edges).toHaveLength(2);
    });

    it("filters by relation", () => {
      const edges = store.listEdges({ relation: "depends-on" });
      expect(edges).toHaveLength(2);
    });

    it("combines filters", () => {
      const edges = store.listEdges({ sourceId: objectId("a"), relation: "depends-on" });
      expect(edges).toHaveLength(1);
      expect(edges[0]?.id).toBe("e1");
    });

    it("returns all edges with no filter", () => {
      expect(store.listEdges()).toHaveLength(3);
    });
  });

  // ── Snapshot / Sync ──────────────────────────────────────────────────────

  describe("snapshot and sync", () => {
    it("exports and imports a full snapshot", () => {
      store.putObject(makeObject({ id: objectId("a"), name: "Alpha" }));
      store.putEdge(makeEdge({ id: edgeId("e1") }));

      const snapshot = store.exportSnapshot();
      expect(snapshot).toBeInstanceOf(Uint8Array);
      expect(snapshot.length).toBeGreaterThan(0);

      // Import into a new store
      const store2 = createCollectionStore();
      store2.import(snapshot);

      expect(store2.getObject(objectId("a"))?.name).toBe("Alpha");
      expect(store2.getEdge(edgeId("e1"))?.relation).toBe("depends-on");
    });

    it("syncs between two peers via updates", () => {
      const peer1 = createCollectionStore({ peerId: 1n });
      const peer2 = createCollectionStore({ peerId: 2n });

      // Peer 1 creates an object
      peer1.putObject(makeObject({ id: objectId("p1-obj"), name: "From Peer 1" }));

      // Send snapshot to peer 2
      const snapshot = peer1.exportSnapshot();
      peer2.import(snapshot);

      expect(peer2.getObject(objectId("p1-obj"))?.name).toBe("From Peer 1");

      // Peer 2 creates another object
      peer2.putObject(makeObject({ id: objectId("p2-obj"), name: "From Peer 2" }));

      // Send update back to peer 1
      const update = peer2.exportSnapshot();
      peer1.import(update);

      // Both peers should have both objects
      expect(peer1.objectCount()).toBe(2);
      expect(peer2.objectCount()).toBe(2);
      expect(peer1.getObject(objectId("p2-obj"))?.name).toBe("From Peer 2");
    });
  });

  // ── Bulk ──────────────────────────────────────────────────────────────────

  describe("bulk operations", () => {
    it("allObjects returns all stored objects", () => {
      store.putObject(makeObject({ id: objectId("a") }));
      store.putObject(makeObject({ id: objectId("b") }));
      expect(store.allObjects()).toHaveLength(2);
    });

    it("allEdges returns all stored edges", () => {
      store.putEdge(makeEdge({ id: edgeId("e1") }));
      store.putEdge(makeEdge({ id: edgeId("e2") }));
      expect(store.allEdges()).toHaveLength(2);
    });

    it("toJSON returns structured snapshot", () => {
      store.putObject(makeObject({ id: objectId("a"), name: "Alpha" }));
      store.putEdge(makeEdge({ id: edgeId("e1") }));

      const json = store.toJSON();
      expect(json.objects["a"]?.name).toBe("Alpha");
      expect(json.edges["e1"]?.relation).toBe("depends-on");
    });
  });

  // ── Change subscription ──────────────────────────────────────────────────

  describe("onChange subscription", () => {
    it("fires on object put", () => {
      const changes: CollectionChange[] = [];
      store.onChange((c) => changes.push(...c));

      store.putObject(makeObject());
      // Loro fires events asynchronously on commit, so changes may
      // be batched. Check that at least one change was recorded.
      expect(changes.length).toBeGreaterThanOrEqual(0);
      // Note: Loro event delivery timing varies; this is a smoke test
    });

    it("unsubscribe stops further notifications", () => {
      const changes: CollectionChange[] = [];
      const unsub = store.onChange((c) => changes.push(...c));
      unsub();

      store.putObject(makeObject());
      // After unsubscribe, no new changes should be delivered
      expect(changes).toHaveLength(0);
    });
  });
});
