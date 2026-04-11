import { describe, it, expect, beforeEach, vi } from "vitest";
import type { GraphObject } from "@prism/core/object-model";
import { objectId } from "@prism/core/object-model";
import { createCollectionStore } from "@prism/core/persistence";
import type { CollectionStore } from "@prism/core/persistence";
import type { SearchEngine, SearchResult } from "./search-engine.js";
import { createSearchEngine } from "./search-engine.js";

// ── Helpers ───────────────────────────────────────────────────────────────────

let objCounter = 0;

function makeObject(overrides: Partial<GraphObject> = {}): GraphObject {
  const id = overrides.id ?? objectId(`obj-${++objCounter}`);
  return {
    id,
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

/** Safe accessor — throws if index is out of bounds (cleaner than `!`). */
function at<T>(arr: T[], i: number): T {
  const val = arr[i];
  if (val === undefined) throw new Error(`Expected element at index ${i}`);
  return val;
}

// ── Tests ────────────────────────────────────────────────────────────────────

describe("SearchEngine", () => {
  let engine: SearchEngine;

  beforeEach(() => {
    objCounter = 0;
    engine = createSearchEngine();
  });

  // ── Indexing ────────────────────────────────────────────────────────────────

  describe("indexCollection / removeCollection", () => {
    it("indexes a collection and tracks it", () => {
      const store = populatedStore([makeObject({ name: "Alpha" })]);
      engine.indexCollection("tasks", store);

      expect(engine.indexedCollections).toEqual(["tasks"]);
      expect(engine.totalDocuments).toBe(1);
    });

    it("removes a collection from the index", () => {
      const store = populatedStore([makeObject({ name: "Alpha" })]);
      engine.indexCollection("tasks", store);
      engine.removeCollection("tasks");

      expect(engine.indexedCollections).toEqual([]);
      expect(engine.totalDocuments).toBe(0);
    });

    it("re-indexes on reindex call", () => {
      const store = createCollectionStore();
      store.putObject(makeObject({ id: objectId("a"), name: "Alpha" }));
      engine.indexCollection("tasks", store);

      // Add another object directly to store
      store.putObject(makeObject({ id: objectId("b"), name: "Beta" }));

      // Reindex picks up the new object
      engine.reindex("tasks", store);
      expect(engine.totalDocuments).toBe(2);
    });
  });

  // ── Full-text search ───────────────────────────────────────────────────────

  describe("full-text search", () => {
    it("finds objects by text query", () => {
      const store = populatedStore([
        makeObject({ id: objectId("a"), name: "Build spaceship" }),
        makeObject({ id: objectId("b"), name: "Cook dinner" }),
      ]);
      engine.indexCollection("tasks", store);

      const result = engine.search({ query: "spaceship" });
      expect(result.hits).toHaveLength(1);
      expect(at(result.hits, 0).objectId).toBe("a");
      expect(result.total).toBe(1);
    });

    it("returns all objects when no query", () => {
      const store = populatedStore([
        makeObject({ id: objectId("a") }),
        makeObject({ id: objectId("b") }),
      ]);
      engine.indexCollection("tasks", store);

      const result = engine.search();
      expect(result.total).toBe(2);
    });

    it("searches across multiple collections", () => {
      const store1 = populatedStore([
        makeObject({ id: objectId("a"), name: "Alpha rocket" }),
      ]);
      const store2 = populatedStore([
        makeObject({ id: objectId("b"), name: "Beta rocket" }),
      ]);
      engine.indexCollection("col-1", store1);
      engine.indexCollection("col-2", store2);

      const result = engine.search({ query: "rocket" });
      expect(result.hits).toHaveLength(2);
    });

    it("includes resolved object in hits", () => {
      const obj = makeObject({ id: objectId("a"), name: "Fancy task" });
      const store = populatedStore([obj]);
      engine.indexCollection("tasks", store);

      const result = engine.search({ query: "fancy" });
      expect(at(result.hits, 0).object.name).toBe("Fancy task");
    });
  });

  // ── Structured filters ─────────────────────────────────────────────────────

  describe("structured filters", () => {
    let store: CollectionStore;

    beforeEach(() => {
      store = populatedStore([
        makeObject({ id: objectId("a"), name: "Task Alpha", type: "task", status: "active", tags: ["urgent", "backend"] }),
        makeObject({ id: objectId("b"), name: "Note Beta", type: "note", status: "draft", tags: ["frontend"] }),
        makeObject({ id: objectId("c"), name: "Task Gamma", type: "task", status: "done", tags: ["urgent"] }),
        makeObject({ id: objectId("d"), name: "Deleted Delta", type: "task", deletedAt: "2026-02-01T00:00:00Z" }),
      ]);
      engine.indexCollection("col", store);
    });

    it("filters by type", () => {
      const result = engine.search({ types: ["note"] });
      expect(result.total).toBe(1);
      expect(at(result.hits, 0).objectId).toBe("b");
    });

    it("filters by multiple types", () => {
      const result = engine.search({ types: ["task", "note"] });
      expect(result.total).toBe(3); // excludes deleted
    });

    it("filters by status", () => {
      const result = engine.search({ statuses: ["done"] });
      expect(result.total).toBe(1);
      expect(at(result.hits, 0).objectId).toBe("c");
    });

    it("filters by tags (AND logic)", () => {
      const result = engine.search({ tags: ["urgent"] });
      expect(result.total).toBe(2);

      const result2 = engine.search({ tags: ["urgent", "backend"] });
      expect(result2.total).toBe(1);
      expect(at(result2.hits, 0).objectId).toBe("a");
    });

    it("excludes deleted by default", () => {
      const result = engine.search();
      expect(result.total).toBe(3);
    });

    it("includes deleted when requested", () => {
      const result = engine.search({ includeDeleted: true });
      expect(result.total).toBe(4);
    });

    it("filters by collectionIds", () => {
      const store2 = populatedStore([
        makeObject({ id: objectId("x"), name: "Other" }),
      ]);
      engine.indexCollection("col-2", store2);

      const result = engine.search({ collectionIds: ["col"] });
      expect(result.total).toBe(3);
    });

    it("combines text query with structured filters", () => {
      const result = engine.search({ query: "task", types: ["task"], statuses: ["active"] });
      expect(result.total).toBe(1);
      expect(at(result.hits, 0).objectId).toBe("a");
    });
  });

  // ── Date range ─────────────────────────────────────────────────────────────

  describe("date range", () => {
    it("filters by dateAfter and dateBefore", () => {
      const store = populatedStore([
        makeObject({ id: objectId("a"), name: "Early", date: "2026-01-01" }),
        makeObject({ id: objectId("b"), name: "Middle", date: "2026-06-15" }),
        makeObject({ id: objectId("c"), name: "Late", date: "2026-12-31" }),
        makeObject({ id: objectId("d"), name: "No date" }),
      ]);
      engine.indexCollection("col", store);

      const result = engine.search({ dateAfter: "2026-03-01", dateBefore: "2026-09-01" });
      expect(result.total).toBe(2); // Middle + No date (null dates pass through)
    });
  });

  // ── Facets ─────────────────────────────────────────────────────────────────

  describe("facets", () => {
    it("computes type facets", () => {
      const store = populatedStore([
        makeObject({ id: objectId("a"), type: "task" }),
        makeObject({ id: objectId("b"), type: "task" }),
        makeObject({ id: objectId("c"), type: "note" }),
      ]);
      engine.indexCollection("col", store);

      const result = engine.search();
      expect(result.facets.types).toEqual({ task: 2, note: 1 });
    });

    it("computes collection facets", () => {
      const store1 = populatedStore([makeObject({ id: objectId("a") })]);
      const store2 = populatedStore([
        makeObject({ id: objectId("b") }),
        makeObject({ id: objectId("c") }),
      ]);
      engine.indexCollection("tasks", store1);
      engine.indexCollection("notes", store2);

      const result = engine.search();
      expect(result.facets.collections).toEqual({ tasks: 1, notes: 2 });
    });

    it("computes tag facets", () => {
      const store = populatedStore([
        makeObject({ id: objectId("a"), tags: ["urgent", "backend"] }),
        makeObject({ id: objectId("b"), tags: ["urgent"] }),
      ]);
      engine.indexCollection("col", store);

      const result = engine.search();
      expect(result.facets.tags).toEqual({ urgent: 2, backend: 1 });
    });

    it("facets reflect full result set, not just page", () => {
      const objects = Array.from({ length: 5 }, (_, i) =>
        makeObject({ id: objectId(`o-${i}`), type: i < 3 ? "task" : "note" }),
      );
      const store = populatedStore(objects);
      engine.indexCollection("col", store);

      const result = engine.search({ limit: 2 });
      expect(result.hits).toHaveLength(2);
      expect(result.total).toBe(5);
      expect(result.facets.types).toEqual({ task: 3, note: 2 });
    });
  });

  // ── Pagination ─────────────────────────────────────────────────────────────

  describe("pagination", () => {
    it("limits results", () => {
      const objects = Array.from({ length: 10 }, (_, i) =>
        makeObject({ id: objectId(`o-${i}`), name: `Item ${i}` }),
      );
      const store = populatedStore(objects);
      engine.indexCollection("col", store);

      const result = engine.search({ limit: 3 });
      expect(result.hits).toHaveLength(3);
      expect(result.total).toBe(10);
    });

    it("offsets results", () => {
      const objects = Array.from({ length: 5 }, (_, i) =>
        makeObject({ id: objectId(`o-${i}`), name: `Item ${String(i).padStart(2, "0")}` }),
      );
      const store = populatedStore(objects);
      engine.indexCollection("col", store);

      const all = engine.search({ sortBy: "name", sortDir: "asc" });
      const page = engine.search({ sortBy: "name", sortDir: "asc", limit: 2, offset: 2 });

      expect(page.hits).toHaveLength(2);
      expect(at(page.hits, 0).objectId).toBe(at(all.hits, 2).objectId);
    });

    it("defaults to 50 limit", () => {
      const objects = Array.from({ length: 60 }, (_, i) =>
        makeObject({ id: objectId(`o-${i}`) }),
      );
      const store = populatedStore(objects);
      engine.indexCollection("col", store);

      const result = engine.search();
      expect(result.hits).toHaveLength(50);
      expect(result.total).toBe(60);
    });

    it("respects custom defaultLimit", () => {
      const eng = createSearchEngine({ defaultLimit: 5 });
      const objects = Array.from({ length: 10 }, (_, i) =>
        makeObject({ id: objectId(`o-${i}`) }),
      );
      const store = populatedStore(objects);
      eng.indexCollection("col", store);

      const result = eng.search();
      expect(result.hits).toHaveLength(5);
    });
  });

  // ── Sorting ────────────────────────────────────────────────────────────────

  describe("sorting", () => {
    it("sorts by name ascending by default when no query", () => {
      const store = populatedStore([
        makeObject({ id: objectId("c"), name: "Zebra" }),
        makeObject({ id: objectId("a"), name: "Apple" }),
        makeObject({ id: objectId("b"), name: "Mango" }),
      ]);
      engine.indexCollection("col", store);

      const result = engine.search();
      expect(result.hits.map((h) => h.object.name)).toEqual([
        "Apple",
        "Mango",
        "Zebra",
      ]);
    });

    it("sorts by relevance descending when query present", () => {
      const store = populatedStore([
        makeObject({ id: objectId("a"), name: "nothing here" }),
        makeObject({ id: objectId("b"), name: "rocket launch" }),
      ]);
      engine.indexCollection("col", store);

      const result = engine.search({ query: "rocket" });
      expect(at(result.hits, 0).objectId).toBe("b");
    });

    it("sorts by createdAt", () => {
      const store = populatedStore([
        makeObject({ id: objectId("a"), name: "Old", createdAt: "2026-01-01T00:00:00Z" }),
        makeObject({ id: objectId("b"), name: "New", createdAt: "2026-06-01T00:00:00Z" }),
      ]);
      engine.indexCollection("col", store);

      const asc = engine.search({ sortBy: "createdAt", sortDir: "asc" });
      expect(at(asc.hits, 0).objectId).toBe("a");

      const desc = engine.search({ sortBy: "createdAt", sortDir: "desc" });
      expect(at(desc.hits, 0).objectId).toBe("b");
    });
  });

  // ── Auto-reindex on changes ────────────────────────────────────────────────

  describe("auto-reindex", () => {
    it("picks up new objects added to a collection store", () => {
      const store = createCollectionStore();
      store.putObject(makeObject({ id: objectId("a"), name: "Alpha" }));
      engine.indexCollection("col", store);
      expect(engine.totalDocuments).toBe(1);

      store.putObject(makeObject({ id: objectId("b"), name: "Beta" }));
      expect(engine.totalDocuments).toBe(2);
      expect(engine.search({ query: "beta" }).total).toBe(1);
    });

    it("picks up updated objects", () => {
      const store = createCollectionStore();
      store.putObject(makeObject({ id: objectId("a"), name: "Old name" }));
      engine.indexCollection("col", store);

      store.putObject(makeObject({ id: objectId("a"), name: "New name" }));
      expect(engine.search({ query: "old" }).total).toBe(0);
      expect(engine.search({ query: "new" }).total).toBe(1);
    });

    it("picks up removed objects", () => {
      const store = createCollectionStore();
      store.putObject(makeObject({ id: objectId("a"), name: "Gone" }));
      engine.indexCollection("col", store);

      store.removeObject(objectId("a"));
      expect(engine.totalDocuments).toBe(0);
      expect(engine.search({ query: "gone" }).total).toBe(0);
    });
  });

  // ── Subscriptions ──────────────────────────────────────────────────────────

  describe("subscribe", () => {
    it("calls handler immediately with current results", () => {
      const store = populatedStore([
        makeObject({ id: objectId("a"), name: "Alpha" }),
      ]);
      engine.indexCollection("col", store);

      let lastResult: SearchResult | null = null;
      engine.subscribe({ query: "alpha" }, (r) => {
        lastResult = r;
      });

      expect(lastResult).not.toBeNull();
      expect((lastResult as SearchResult).total).toBe(1);
    });

    it("re-runs on index changes", () => {
      const store = createCollectionStore();
      store.putObject(makeObject({ id: objectId("a"), name: "Alpha" }));
      engine.indexCollection("col", store);

      const results: SearchResult[] = [];
      engine.subscribe({ query: "alpha" }, (r) => {
        results.push(r);
      });

      expect(results).toHaveLength(1);

      // Add another matching object
      store.putObject(makeObject({ id: objectId("b"), name: "Alpha two" }));

      expect(results).toHaveLength(2);
      expect(at(results, 1).total).toBe(2);
    });

    it("unsubscribe stops notifications", () => {
      const store = createCollectionStore();
      store.putObject(makeObject({ id: objectId("a"), name: "Alpha" }));
      engine.indexCollection("col", store);

      const handler = vi.fn();
      const unsub = engine.subscribe({ query: "alpha" }, handler);

      expect(handler).toHaveBeenCalledTimes(1);
      unsub();

      store.putObject(makeObject({ id: objectId("b"), name: "Alpha again" }));
      expect(handler).toHaveBeenCalledTimes(1); // no additional calls
    });
  });
});
