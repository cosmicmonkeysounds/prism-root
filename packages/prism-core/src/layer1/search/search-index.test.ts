import { describe, it, expect, beforeEach } from "vitest";
import type { GraphObject } from "../object-model/types.js";
import { objectId } from "../object-model/types.js";
import type { SearchIndex } from "./search-index.js";
import { createSearchIndex, tokenize } from "./search-index.js";

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

/** Safe accessor — throws if index is out of bounds (cleaner than `!`). */
function at<T>(arr: T[], i: number): T {
  const val = arr[i];
  if (val === undefined) throw new Error(`Expected element at index ${i}`);
  return val;
}

// ── Tokenizer ─────────────────────────────────────────────────────────────────

describe("tokenize", () => {
  it("splits on whitespace and punctuation", () => {
    expect(tokenize("hello world")).toEqual(["hello", "world"]);
    expect(tokenize("foo-bar_baz")).toEqual(["foo", "bar", "baz"]);
    expect(tokenize("a.b,c;d")).toEqual([]);
  });

  it("lowercases tokens", () => {
    expect(tokenize("Hello World")).toEqual(["hello", "world"]);
  });

  it("filters by minimum length", () => {
    expect(tokenize("a bb ccc", 3)).toEqual(["ccc"]);
    expect(tokenize("a bb ccc", 1)).toEqual(["a", "bb", "ccc"]);
  });

  it("handles empty input", () => {
    expect(tokenize("")).toEqual([]);
    expect(tokenize("   ")).toEqual([]);
  });

  it("splits on common delimiters", () => {
    expect(tokenize("email@example.com")).toEqual(["email", "example", "com"]);
    expect(tokenize("path/to/file")).toEqual(["path", "to", "file"]);
  });
});

// ── SearchIndex ──────────────────────────────────────────────────────────────

describe("SearchIndex", () => {
  let index: SearchIndex;

  beforeEach(() => {
    index = createSearchIndex();
  });

  describe("add / remove / size", () => {
    it("starts empty", () => {
      expect(index.size()).toBe(0);
    });

    it("adds a document", () => {
      index.add("col-1", makeObject());
      expect(index.size()).toBe(1);
    });

    it("removes a document", () => {
      index.add("col-1", makeObject());
      const removed = index.remove("col-1", objectId("obj-1"));
      expect(removed).toBe(true);
      expect(index.size()).toBe(0);
    });

    it("returns false when removing non-existent", () => {
      expect(index.remove("col-1", objectId("nope"))).toBe(false);
    });

    it("clears all documents", () => {
      index.add("col-1", makeObject({ id: objectId("a") }));
      index.add("col-1", makeObject({ id: objectId("b") }));
      index.clear();
      expect(index.size()).toBe(0);
    });

    it("tracks documents per collection", () => {
      index.add("col-1", makeObject({ id: objectId("a") }));
      index.add("col-2", makeObject({ id: objectId("b") }));
      expect(index.size()).toBe(2);
      index.removeCollection("col-1");
      expect(index.size()).toBe(1);
    });
  });

  describe("search", () => {
    it("returns empty for empty index", () => {
      expect(index.search("anything")).toEqual([]);
    });

    it("returns empty for empty query", () => {
      index.add("col-1", makeObject());
      expect(index.search("")).toEqual([]);
    });

    it("finds by name", () => {
      index.add("col-1", makeObject({ name: "Build the spaceship" }));
      const hits = index.search("spaceship");
      expect(hits).toHaveLength(1);
      expect(at(hits, 0).objectId).toBe("obj-1");
      expect(at(hits, 0).collectionId).toBe("col-1");
    });

    it("finds by description", () => {
      index.add(
        "col-1",
        makeObject({ description: "We need to refactor the database layer" }),
      );
      const hits = index.search("refactor");
      expect(hits).toHaveLength(1);
    });

    it("finds by tags", () => {
      index.add("col-1", makeObject({ tags: ["urgent", "backend"] }));
      const hits = index.search("urgent");
      expect(hits).toHaveLength(1);
    });

    it("finds by type", () => {
      index.add("col-1", makeObject({ type: "milestone" }));
      const hits = index.search("milestone");
      expect(hits).toHaveLength(1);
    });

    it("finds by status", () => {
      index.add("col-1", makeObject({ status: "completed" }));
      const hits = index.search("completed");
      expect(hits).toHaveLength(1);
    });

    it("finds by data payload values", () => {
      index.add("col-1", makeObject({ data: { note: "Remember the quantum flux" } }));
      const hits = index.search("quantum");
      expect(hits).toHaveLength(1);
    });

    it("is case-insensitive", () => {
      index.add("col-1", makeObject({ name: "UPPERCASE Title" }));
      const hits = index.search("uppercase");
      expect(hits).toHaveLength(1);
    });

    it("matches partial tokens via multi-word query", () => {
      index.add("col-1", makeObject({ name: "Design review meeting" }));
      const hits = index.search("design meeting");
      expect(hits).toHaveLength(1);
    });

    it("returns no results for non-matching query", () => {
      index.add("col-1", makeObject({ name: "Something else" }));
      expect(index.search("zzzznotfound")).toEqual([]);
    });
  });

  describe("scoring", () => {
    it("ranks name matches higher than description matches", () => {
      index.add(
        "col-1",
        makeObject({
          id: objectId("name-match"),
          name: "spaceship",
          description: "nothing here",
        }),
      );
      index.add(
        "col-1",
        makeObject({
          id: objectId("desc-match"),
          name: "nothing here",
          description: "spaceship is great",
        }),
      );
      const hits = index.search("spaceship");
      expect(hits).toHaveLength(2);
      expect(at(hits, 0).objectId).toBe("name-match");
      expect(at(hits, 0).score).toBeGreaterThan(at(hits, 1).score);
    });

    it("ranks documents with more query term matches higher", () => {
      index.add(
        "col-1",
        makeObject({
          id: objectId("two-terms"),
          name: "design review",
          description: "quarterly design review session",
        }),
      );
      index.add(
        "col-1",
        makeObject({
          id: objectId("one-term"),
          name: "code review",
          description: "code quality check",
        }),
      );
      const hits = index.search("design review");
      expect(hits).toHaveLength(2);
      expect(at(hits, 0).objectId).toBe("two-terms");
    });

    it("scores are positive", () => {
      index.add("col-1", makeObject({ name: "hello world" }));
      const hits = index.search("hello");
      expect(at(hits, 0).score).toBeGreaterThan(0);
    });
  });

  describe("update", () => {
    it("re-indexes on update", () => {
      index.add("col-1", makeObject({ name: "old name" }));
      expect(index.search("old")).toHaveLength(1);

      index.update("col-1", makeObject({ name: "new name" }));
      expect(index.search("old")).toHaveLength(0);
      expect(index.search("new")).toHaveLength(1);
      expect(index.size()).toBe(1);
    });
  });

  describe("removeCollection", () => {
    it("removes all docs from a collection", () => {
      index.add("col-1", makeObject({ id: objectId("a"), name: "alpha" }));
      index.add("col-1", makeObject({ id: objectId("b"), name: "beta" }));
      index.add("col-2", makeObject({ id: objectId("c"), name: "gamma" }));

      index.removeCollection("col-1");
      expect(index.size()).toBe(1);
      expect(index.search("alpha")).toHaveLength(0);
      expect(index.search("gamma")).toHaveLength(1);
    });

    it("no-op for unknown collection", () => {
      index.removeCollection("nope");
      expect(index.size()).toBe(0);
    });
  });

  describe("multi-collection", () => {
    it("searches across multiple collections", () => {
      index.add("col-1", makeObject({ id: objectId("a"), name: "shared term" }));
      index.add("col-2", makeObject({ id: objectId("b"), name: "shared term also" }));
      const hits = index.search("shared");
      expect(hits).toHaveLength(2);
      const collections = hits.map((h) => h.collectionId).sort();
      expect(collections).toEqual(["col-1", "col-2"]);
    });
  });

  describe("custom options", () => {
    it("respects custom weights", () => {
      const idx = createSearchIndex({ weights: { name: 1, description: 10, type: 1, tags: 1, status: 1, data: 1 } });
      idx.add(
        "col-1",
        makeObject({
          id: objectId("name-match"),
          name: "spaceship",
          description: "nothing",
        }),
      );
      idx.add(
        "col-1",
        makeObject({
          id: objectId("desc-match"),
          name: "nothing",
          description: "spaceship is great",
        }),
      );
      const hits = idx.search("spaceship");
      // With description weighted 10x, description match should win
      expect(at(hits, 0).objectId).toBe("desc-match");
    });

    it("respects custom minTokenLength", () => {
      const idx = createSearchIndex({ minTokenLength: 5 });
      idx.add("col-1", makeObject({ name: "big huge enormous" }));
      // "big" and "huge" are < 5 chars, only "enormous" indexed
      expect(idx.search("big")).toHaveLength(0);
      expect(idx.search("enormous")).toHaveLength(1);
    });
  });
});
