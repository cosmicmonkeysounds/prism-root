import { describe, it, expect, beforeEach, vi } from "vitest";
import type { GraphObject } from "@prism/core/object-model";
import { objectId } from "@prism/core/object-model";
import { createCollectionStore } from "@prism/core/persistence";
import type { CollectionStore } from "@prism/core/persistence";
import type { LiveView, LiveViewSnapshot } from "./live-view.js";
import { createLiveView } from "./live-view.js";

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

function populatedStore(objects: GraphObject[]): CollectionStore {
  const store = createCollectionStore();
  for (const obj of objects) {
    store.putObject(obj);
  }
  return store;
}

// ── Tests ────────────────────────────────────────────────────────────────────

describe("LiveView", () => {
  let store: CollectionStore;
  let view: LiveView;

  beforeEach(() => {
    store = populatedStore([
      makeObject({ id: objectId("a"), name: "Alpha", type: "task", status: "active", tags: ["urgent"] }),
      makeObject({ id: objectId("b"), name: "Beta", type: "note", status: "draft", tags: ["backend"] }),
      makeObject({ id: objectId("c"), name: "Gamma", type: "task", status: "done", tags: ["urgent", "backend"] }),
    ]);
    view = createLiveView(store);
  });

  // ── Initial snapshot ───────────────────────────────────────────────────────

  describe("initial snapshot", () => {
    it("materializes all objects", () => {
      expect(view.snapshot.objects).toHaveLength(3);
      expect(view.snapshot.total).toBe(3);
    });

    it("defaults to list mode", () => {
      expect(view.mode).toBe("list");
    });

    it("computes type facets", () => {
      expect(view.snapshot.typeFacets).toEqual({ task: 2, note: 1 });
    });

    it("computes tag facets", () => {
      expect(view.snapshot.tagFacets).toEqual({ urgent: 2, backend: 2 });
    });

    it("has a single 'All' group by default", () => {
      expect(view.snapshot.groups).toHaveLength(1);
      expect(view.snapshot.groups[0]?.key).toBe("__all__");
    });
  });

  // ── Config with initial options ────────────────────────────────────────────

  describe("initial options", () => {
    it("accepts initial mode", () => {
      const v = createLiveView(store, { mode: "kanban" });
      expect(v.mode).toBe("kanban");
    });

    it("accepts initial config", () => {
      const v = createLiveView(store, {
        config: { filters: [{ field: "type", op: "eq", value: "task" }] },
      });
      expect(v.snapshot.objects).toHaveLength(2);
    });
  });

  // ── Filters ────────────────────────────────────────────────────────────────

  describe("setFilters", () => {
    it("applies filters and re-materializes", () => {
      view.setFilters([{ field: "type", op: "eq", value: "task" }]);
      expect(view.snapshot.objects).toHaveLength(2);
      expect(view.snapshot.total).toBe(2);
    });

    it("clears filters when empty array", () => {
      view.setFilters([{ field: "type", op: "eq", value: "task" }]);
      expect(view.snapshot.objects).toHaveLength(2);

      view.setFilters([]);
      expect(view.snapshot.objects).toHaveLength(3);
    });

    it("updates facets after filtering", () => {
      view.setFilters([{ field: "type", op: "eq", value: "task" }]);
      expect(view.snapshot.typeFacets).toEqual({ task: 2 });
    });
  });

  // ── Sorts ──────────────────────────────────────────────────────────────────

  describe("setSorts", () => {
    it("sorts objects", () => {
      view.setSorts([{ field: "name", dir: "desc" }]);
      expect(view.snapshot.objects.map((o) => o.name)).toEqual([
        "Gamma", "Beta", "Alpha",
      ]);
    });

    it("supports multi-level sort", () => {
      view.setSorts([
        { field: "type", dir: "asc" },
        { field: "name", dir: "asc" },
      ]);
      const names = view.snapshot.objects.map((o) => o.name);
      // note first, then tasks alphabetically
      expect(names).toEqual(["Beta", "Alpha", "Gamma"]);
    });
  });

  // ── Groups ─────────────────────────────────────────────────────────────────

  describe("setGroups", () => {
    it("groups by field", () => {
      view.setGroups([{ field: "type" }]);
      const groups = view.snapshot.groups;
      expect(groups).toHaveLength(2);
      expect(groups.map((g) => g.key)).toEqual(["task", "note"]);
    });

    it("clears groups", () => {
      view.setGroups([{ field: "type" }]);
      expect(view.snapshot.groups).toHaveLength(2);

      view.setGroups([]);
      expect(view.snapshot.groups).toHaveLength(1);
      expect(view.snapshot.groups[0]?.key).toBe("__all__");
    });
  });

  // ── Limit ──────────────────────────────────────────────────────────────────

  describe("setLimit", () => {
    it("limits materialized objects", () => {
      view.setLimit(2);
      expect(view.snapshot.objects).toHaveLength(2);
      expect(view.snapshot.total).toBe(3); // total is before limit
    });

    it("removes limit", () => {
      view.setLimit(1);
      expect(view.snapshot.objects).toHaveLength(1);

      view.setLimit(undefined);
      expect(view.snapshot.objects).toHaveLength(3);
    });
  });

  // ── Mode ───────────────────────────────────────────────────────────────────

  describe("setMode", () => {
    it("changes mode", () => {
      view.setMode("kanban");
      expect(view.mode).toBe("kanban");
    });
  });

  // ── setConfig ──────────────────────────────────────────────────────────────

  describe("setConfig", () => {
    it("replaces entire config", () => {
      view.setConfig({
        filters: [{ field: "status", op: "eq", value: "done" }],
        sorts: [{ field: "name", dir: "asc" }],
      });
      expect(view.snapshot.objects).toHaveLength(1);
      expect(view.snapshot.objects[0]?.name).toBe("Gamma");
    });
  });

  // ── Columns ────────────────────────────────────────────────────────────────

  describe("setColumns", () => {
    it("updates config columns", () => {
      view.setColumns(["name", "status", "tags"]);
      expect(view.config.columns).toEqual(["name", "status", "tags"]);
    });
  });

  // ── Group collapse ─────────────────────────────────────────────────────────

  describe("toggleGroupCollapsed", () => {
    it("toggles group collapsed state", () => {
      view.setGroups([{ field: "type" }]);

      view.toggleGroupCollapsed("task");
      const taskGroup = view.snapshot.groups.find((g) => g.key === "task");
      expect(taskGroup?.collapsed).toBe(true);

      view.toggleGroupCollapsed("task");
      const taskGroup2 = view.snapshot.groups.find((g) => g.key === "task");
      expect(taskGroup2?.collapsed).toBe(false);
    });
  });

  // ── includes ───────────────────────────────────────────────────────────────

  describe("includes", () => {
    it("returns true for objects in the materialized set", () => {
      expect(view.includes(objectId("a"))).toBe(true);
    });

    it("returns false for objects not in the set", () => {
      view.setFilters([{ field: "type", op: "eq", value: "note" }]);
      expect(view.includes(objectId("a"))).toBe(false);
      expect(view.includes(objectId("b"))).toBe(true);
    });
  });

  // ── Auto-update on store changes ───────────────────────────────────────────

  describe("auto-update", () => {
    it("re-materializes when store object is added", () => {
      expect(view.snapshot.objects).toHaveLength(3);

      store.putObject(makeObject({ id: objectId("d"), name: "Delta", type: "task" }));
      expect(view.snapshot.objects).toHaveLength(4);
    });

    it("re-materializes when store object is updated", () => {
      view.setFilters([{ field: "status", op: "eq", value: "active" }]);
      expect(view.snapshot.objects).toHaveLength(1);

      // Change Beta from "draft" to "active"
      store.putObject(makeObject({ id: objectId("b"), name: "Beta", type: "note", status: "active" }));
      expect(view.snapshot.objects).toHaveLength(2);
    });

    it("re-materializes when store object is removed", () => {
      store.removeObject(objectId("a"));
      expect(view.snapshot.objects).toHaveLength(2);
    });

    it("updates facets on store change", () => {
      store.putObject(makeObject({ id: objectId("d"), type: "note", tags: ["new-tag"] }));
      expect(view.snapshot.typeFacets.note).toBe(2);
      expect(view.snapshot.tagFacets["new-tag"]).toBe(1);
    });
  });

  // ── Subscriptions ──────────────────────────────────────────────────────────

  describe("subscribe", () => {
    it("calls listener immediately with current snapshot", () => {
      let received: LiveViewSnapshot | null = null;
      view.subscribe((s) => {
        received = s;
      });
      expect(received).not.toBeNull();
      expect((received as unknown as LiveViewSnapshot).objects).toHaveLength(3);
    });

    it("notifies on config change", () => {
      const snapshots: LiveViewSnapshot[] = [];
      view.subscribe((s) => snapshots.push(s));

      // Initial
      expect(snapshots).toHaveLength(1);

      view.setFilters([{ field: "type", op: "eq", value: "note" }]);
      expect(snapshots).toHaveLength(2);
      expect(snapshots[1]?.objects).toHaveLength(1);
    });

    it("notifies on store change", () => {
      const snapshots: LiveViewSnapshot[] = [];
      view.subscribe((s) => snapshots.push(s));

      store.putObject(makeObject({ id: objectId("d"), name: "New" }));
      expect(snapshots.length).toBeGreaterThan(1);
    });

    it("unsubscribes", () => {
      const handler = vi.fn();
      const unsub = view.subscribe(handler);
      expect(handler).toHaveBeenCalledTimes(1);

      unsub();
      view.setFilters([{ field: "type", op: "eq", value: "note" }]);
      expect(handler).toHaveBeenCalledTimes(1);
    });
  });

  // ── Refresh ────────────────────────────────────────────────────────────────

  describe("refresh", () => {
    it("forces re-materialization", () => {
      const initial = view.snapshot;
      view.refresh();
      // Snapshot should be a new object
      expect(view.snapshot).not.toBe(initial);
      expect(view.snapshot.objects).toHaveLength(3);
    });
  });

  // ── Dispose ────────────────────────────────────────────────────────────────

  describe("dispose", () => {
    it("stops auto-updates", () => {
      const handler = vi.fn();
      view.subscribe(handler);
      expect(handler).toHaveBeenCalledTimes(1);

      view.dispose();

      store.putObject(makeObject({ id: objectId("d"), name: "New" }));
      expect(handler).toHaveBeenCalledTimes(1); // no new calls
    });
  });

  // ── Deleted objects ────────────────────────────────────────────────────────

  describe("deleted objects", () => {
    it("excludes deleted by default", () => {
      store.putObject(makeObject({ id: objectId("d"), name: "Deleted", deletedAt: "2026-02-01T00:00:00Z" }));
      expect(view.snapshot.objects).toHaveLength(3);
    });

    it("includes deleted when configured", () => {
      store.putObject(makeObject({ id: objectId("d"), name: "Deleted", deletedAt: "2026-02-01T00:00:00Z" }));
      view.setConfig({ excludeDeleted: false });
      expect(view.snapshot.objects).toHaveLength(4);
    });
  });
});
