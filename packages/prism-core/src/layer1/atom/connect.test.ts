import { describe, it, expect, beforeEach } from "vitest";
import { createPrismBus, PrismEvents } from "./event-bus.js";
import { createAtomStore } from "./atoms.js";
import { createObjectAtomStore } from "./object-atoms.js";
import { connectBusToAtoms, connectBusToObjectAtoms } from "./connect.js";
import type { PrismBus } from "./event-bus.js";
import type { AtomStore } from "./atoms.js";
import type { ObjectAtomStore } from "./object-atoms.js";
import type { GraphObject, ObjectEdge } from "../object-model/types.js";
import { objectId, edgeId } from "../object-model/types.js";

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

function makeEdge(id: string, src: string, tgt: string): ObjectEdge {
  return {
    id: edgeId(id),
    sourceId: objectId(src),
    targetId: objectId(tgt),
    relation: "depends-on",
    createdAt: "2024-01-01T00:00:00Z",
    data: {},
  };
}

describe("connectBusToObjectAtoms", () => {
  let bus: PrismBus;
  let store: ObjectAtomStore;
  let cleanup: () => void;

  beforeEach(() => {
    bus = createPrismBus();
    store = createObjectAtomStore();
    cleanup = connectBusToObjectAtoms(bus, store);
  });

  it("adds object on ObjectCreated", () => {
    const obj = makeObj("1");
    bus.emit(PrismEvents.ObjectCreated, { object: obj });
    expect(store.getState().objects["1"]).toEqual(obj);
  });

  it("updates object on ObjectUpdated", () => {
    bus.emit(PrismEvents.ObjectCreated, { object: makeObj("1", { name: "v1" }) });
    bus.emit(PrismEvents.ObjectUpdated, { object: makeObj("1", { name: "v2" }) });
    expect(store.getState().objects["1"].name).toBe("v2");
  });

  it("removes object on ObjectDeleted", () => {
    bus.emit(PrismEvents.ObjectCreated, { object: makeObj("1") });
    bus.emit(PrismEvents.ObjectDeleted, { id: "1" });
    expect(store.getState().objects["1"]).toBeUndefined();
  });

  it("moves object on ObjectMoved", () => {
    bus.emit(PrismEvents.ObjectCreated, { object: makeObj("1") });
    bus.emit(PrismEvents.ObjectMoved, { id: "1", newParentId: "parent" });
    expect(store.getState().objects["1"].parentId).toBe("parent");
  });

  it("adds edge on EdgeCreated", () => {
    const edge = makeEdge("e1", "1", "2");
    bus.emit(PrismEvents.EdgeCreated, { edge });
    expect(store.getState().edges["e1"]).toEqual(edge);
  });

  it("removes edge on EdgeDeleted", () => {
    bus.emit(PrismEvents.EdgeCreated, { edge: makeEdge("e1", "1", "2") });
    bus.emit(PrismEvents.EdgeDeleted, { id: "e1" });
    expect(store.getState().edges["e1"]).toBeUndefined();
  });

  it("cleanup unsubscribes all handlers", () => {
    cleanup();
    bus.emit(PrismEvents.ObjectCreated, { object: makeObj("1") });
    expect(store.getState().objects["1"]).toBeUndefined();
  });
});

describe("connectBusToAtoms", () => {
  let bus: PrismBus;
  let store: AtomStore;
  let cleanup: () => void;

  beforeEach(() => {
    bus = createPrismBus();
    store = createAtomStore();
    cleanup = connectBusToAtoms(bus, store);
  });

  it("sets selectedId on NavigationNavigate with object target", () => {
    bus.emit(PrismEvents.NavigationNavigate, {
      target: { type: "object", id: "obj-1" },
    });
    expect(store.getState().selectedId).toBe("obj-1");
  });

  it("sets navigationTarget on NavigationNavigate", () => {
    bus.emit(PrismEvents.NavigationNavigate, {
      target: { type: "view", view: "graph" },
    });
    expect(store.getState().navigationTarget).toEqual({
      type: "view",
      view: "graph",
    });
    // Non-object target doesn't change selectedId
    expect(store.getState().selectedId).toBeNull();
  });

  it("toggles activePanel on NavigationPanelToggled", () => {
    bus.emit(PrismEvents.NavigationPanelToggled, {
      panel: "explorer",
      open: true,
    });
    expect(store.getState().activePanel).toBe("explorer");

    bus.emit(PrismEvents.NavigationPanelToggled, {
      panel: "explorer",
      open: false,
    });
    expect(store.getState().activePanel).toBeNull();
  });

  it("sets editingObjectId on EditModeChanged", () => {
    bus.emit(PrismEvents.EditModeChanged, {
      objectId: "edit-1",
      editing: true,
    });
    expect(store.getState().editingObjectId).toBe("edit-1");

    bus.emit(PrismEvents.EditModeChanged, {
      objectId: "edit-1",
      editing: false,
    });
    expect(store.getState().editingObjectId).toBeNull();
  });

  it("sets selectionIds on SelectionChanged", () => {
    bus.emit(PrismEvents.SelectionChanged, { ids: ["a", "b", "c"] });
    expect(store.getState().selectionIds).toEqual(["a", "b", "c"]);
  });

  it("sets searchQuery on SearchCommit", () => {
    bus.emit(PrismEvents.SearchCommit, { query: "find me" });
    expect(store.getState().searchQuery).toBe("find me");
  });

  it("clears searchQuery on SearchClear", () => {
    bus.emit(PrismEvents.SearchCommit, { query: "test" });
    bus.emit(PrismEvents.SearchClear, {});
    expect(store.getState().searchQuery).toBe("");
  });

  it("cleanup unsubscribes all handlers", () => {
    cleanup();
    bus.emit(PrismEvents.SearchCommit, { query: "after cleanup" });
    expect(store.getState().searchQuery).toBe("");
  });
});
