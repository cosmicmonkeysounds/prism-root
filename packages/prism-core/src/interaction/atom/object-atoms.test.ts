import { describe, it, expect, beforeEach } from "vitest";
import {
  createObjectAtomStore,
  selectObject,
  selectQuery,
  selectChildren,
  selectEdgesFrom,
  selectEdgesTo,
  selectAllObjects,
  selectAllEdges,
} from "./object-atoms.js";
import type { ObjectAtomStore } from "./object-atoms.js";
import type { GraphObject, ObjectEdge } from "@prism/core/object-model";
import { objectId, edgeId } from "@prism/core/object-model";

function makeObj(id: string, overrides: Partial<GraphObject> = {}): GraphObject {
  return {
    id: objectId(id),
    type: "task",
    name: `Task ${id}`,
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
    createdAt: "2024-01-01T00:00:00Z",
    updatedAt: "2024-01-01T00:00:00Z",
    ...overrides,
  };
}

function makeEdge(id: string, src: string, tgt: string, relation = "depends-on"): ObjectEdge {
  return {
    id: edgeId(id),
    sourceId: objectId(src),
    targetId: objectId(tgt),
    relation,
    createdAt: "2024-01-01T00:00:00Z",
    data: {},
  };
}

describe("ObjectAtomStore", () => {
  let store: ObjectAtomStore;

  beforeEach(() => {
    store = createObjectAtomStore();
  });

  describe("setObject", () => {
    it("adds an object to the cache", () => {
      const obj = makeObj("1");
      store.getState().setObject(obj);
      expect(store.getState().objects["1"]).toEqual(obj);
    });

    it("updates an existing object", () => {
      store.getState().setObject(makeObj("1", { name: "v1" }));
      store.getState().setObject(makeObj("1", { name: "v2" }));
      expect(store.getState().objects["1"].name).toBe("v2");
    });
  });

  describe("setObjects", () => {
    it("adds multiple objects", () => {
      store.getState().setObjects([makeObj("1"), makeObj("2"), makeObj("3")]);
      expect(Object.keys(store.getState().objects)).toHaveLength(3);
    });
  });

  describe("removeObject", () => {
    it("removes an object from the cache", () => {
      store.getState().setObject(makeObj("1"));
      store.getState().removeObject("1");
      expect(store.getState().objects["1"]).toBeUndefined();
    });
  });

  describe("moveObject", () => {
    it("updates parentId", () => {
      store.getState().setObject(makeObj("1"));
      store.getState().moveObject("1", objectId("parent-1"));
      expect(store.getState().objects["1"].parentId).toBe("parent-1");
    });

    it("sets parentId to null", () => {
      store.getState().setObject(makeObj("1", { parentId: objectId("p") }));
      store.getState().moveObject("1", null);
      expect(store.getState().objects["1"].parentId).toBeNull();
    });

    it("does nothing for unknown object", () => {
      const before = store.getState().objects;
      store.getState().moveObject("unknown", objectId("p"));
      expect(store.getState().objects).toEqual(before);
    });
  });

  describe("edges", () => {
    it("setEdge adds an edge", () => {
      const edge = makeEdge("e1", "1", "2");
      store.getState().setEdge(edge);
      expect(store.getState().edges["e1"]).toEqual(edge);
    });

    it("removeEdge removes an edge", () => {
      store.getState().setEdge(makeEdge("e1", "1", "2"));
      store.getState().removeEdge("e1");
      expect(store.getState().edges["e1"]).toBeUndefined();
    });
  });

  describe("clear", () => {
    it("clears all objects and edges", () => {
      store.getState().setObject(makeObj("1"));
      store.getState().setEdge(makeEdge("e1", "1", "2"));
      store.getState().clear();
      expect(Object.keys(store.getState().objects)).toHaveLength(0);
      expect(Object.keys(store.getState().edges)).toHaveLength(0);
    });
  });

  describe("selectors", () => {
    beforeEach(() => {
      store.getState().setObjects([
        makeObj("1", { type: "task", parentId: objectId("p"), position: 2 }),
        makeObj("2", { type: "task", parentId: objectId("p"), position: 1 }),
        makeObj("3", { type: "note", parentId: null, position: 0 }),
        makeObj("p", { type: "project", parentId: null, position: 0 }),
      ]);
      store.getState().setEdge(makeEdge("e1", "1", "2"));
      store.getState().setEdge(makeEdge("e2", "1", "3", "ref"));
      store.getState().setEdge(makeEdge("e3", "3", "1", "backlink"));
    });

    it("selectObject returns a single object", () => {
      const obj = selectObject(store.getState(), "1");
      expect(obj?.id).toBe("1");
    });

    it("selectObject returns undefined for missing ID", () => {
      expect(selectObject(store.getState(), "nope")).toBeUndefined();
    });

    it("selectQuery filters objects", () => {
      const tasks = selectQuery(
        store.getState(),
        (o) => o.type === "task",
      );
      expect(tasks).toHaveLength(2);
    });

    it("selectQuery sorts objects", () => {
      const sorted = selectQuery(
        store.getState(),
        (o) => o.type === "task",
        (a, b) => a.position - b.position,
      );
      expect(sorted[0].id).toBe("2");
      expect(sorted[1].id).toBe("1");
    });

    it("selectChildren returns children sorted by position", () => {
      const children = selectChildren(store.getState(), "p");
      expect(children).toHaveLength(2);
      expect(children[0].id).toBe("2");
      expect(children[1].id).toBe("1");
    });

    it("selectEdgesFrom returns outgoing edges", () => {
      const edges = selectEdgesFrom(store.getState(), "1");
      expect(edges).toHaveLength(2);
    });

    it("selectEdgesTo returns incoming edges", () => {
      const edges = selectEdgesTo(store.getState(), "1");
      expect(edges).toHaveLength(1);
      expect(edges[0].relation).toBe("backlink");
    });

    it("selectAllObjects returns all objects", () => {
      expect(selectAllObjects(store.getState())).toHaveLength(4);
    });

    it("selectAllEdges returns all edges", () => {
      expect(selectAllEdges(store.getState())).toHaveLength(3);
    });
  });
});
