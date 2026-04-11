import { describe, it, expect } from "vitest";
import {
  aggregate,
  buildFormulaContext,
  readObjectField,
  resolveComputedField,
  resolveFormulaField,
  resolveLookupField,
  resolveRollupField,
  type FieldResolverStores,
} from "./field-resolver.js";
import type {
  EntityFieldDef,
  GraphObject,
  ObjectEdge,
  ObjectId,
  EdgeId,
} from "@prism/core/object-model";

function makeObject(partial: Partial<GraphObject> & Pick<GraphObject, "id" | "type" | "name">): GraphObject {
  return {
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
    createdAt: "2025-01-01T00:00:00Z",
    updatedAt: "2025-01-01T00:00:00Z",
    ...partial,
  };
}

function makeEdge(partial: Partial<ObjectEdge> & Pick<ObjectEdge, "id" | "sourceId" | "targetId" | "relation">): ObjectEdge {
  return {
    createdAt: "2025-01-01T00:00:00Z",
    data: {},
    ...partial,
  };
}

function makeStores(objects: GraphObject[], edges: ObjectEdge[]): FieldResolverStores {
  const objectMap = new Map(objects.map((o) => [o.id, o]));
  return {
    objects: { getObject: (id) => objectMap.get(id) },
    edges: {
      getEdges: (sourceId, relation) =>
        edges.filter((e) => e.sourceId === sourceId && e.relation === relation),
    },
  };
}

describe("readObjectField", () => {
  const obj = makeObject({
    id: "o1" as ObjectId,
    type: "task",
    name: "Buy milk",
    data: { price: 4.5, nested: { city: "NYC" } },
  });

  it("reads shell fields", () => {
    expect(readObjectField(obj, "name")).toBe("Buy milk");
    expect(readObjectField(obj, "type")).toBe("task");
  });

  it("reads top-level data fields", () => {
    expect(readObjectField(obj, "price")).toBe(4.5);
  });

  it("reads nested data fields via dot-path", () => {
    expect(readObjectField(obj, "nested.city")).toBe("NYC");
  });

  it("returns undefined for missing fields", () => {
    expect(readObjectField(obj, "missing")).toBeUndefined();
    expect(readObjectField(obj, "nested.zip")).toBeUndefined();
  });
});

describe("buildFormulaContext", () => {
  it("flattens shell and data", () => {
    const obj = makeObject({
      id: "o1" as ObjectId,
      type: "task",
      name: "Task",
      status: "done",
      data: { score: 10, label: "hi" },
    });
    const ctx = buildFormulaContext(obj);
    expect(ctx.name).toBe("Task");
    expect(ctx.status).toBe("done");
    expect(ctx.score).toBe(10);
    expect(ctx.label).toBe("hi");
  });
});

describe("resolveFormulaField", () => {
  it("evaluates a formula using data fields", () => {
    const obj = makeObject({
      id: "o1" as ObjectId,
      type: "invoice",
      name: "Inv 1",
      data: { subtotal: 100, tax: 10 },
    });
    const def: EntityFieldDef = {
      id: "total",
      type: "float",
      expression: "subtotal + tax",
    };
    expect(resolveFormulaField(obj, def)).toBe(110);
  });

  it("returns 0 when expression is missing", () => {
    const obj = makeObject({ id: "o1" as ObjectId, type: "x", name: "x" });
    const def: EntityFieldDef = { id: "f", type: "float" };
    expect(resolveFormulaField(obj, def)).toBe(0);
  });
});

describe("resolveLookupField", () => {
  it("follows an edge and reads a target field", () => {
    const contact = makeObject({
      id: "c1" as ObjectId,
      type: "contact",
      name: "Alice",
      data: { email: "alice@example.com" },
    });
    const deal = makeObject({ id: "d1" as ObjectId, type: "deal", name: "Deal 1" });
    const edge = makeEdge({
      id: "e1" as EdgeId,
      sourceId: "d1" as ObjectId,
      targetId: "c1" as ObjectId,
      relation: "primary-contact",
    });
    const stores = makeStores([contact, deal], [edge]);
    const def: EntityFieldDef = {
      id: "contactEmail",
      type: "lookup",
      lookupRelation: "primary-contact",
      lookupField: "email",
    };
    expect(resolveLookupField(deal, def, stores)).toBe("alice@example.com");
  });

  it("returns undefined when no edges", () => {
    const deal = makeObject({ id: "d1" as ObjectId, type: "deal", name: "Deal 1" });
    const stores = makeStores([deal], []);
    const def: EntityFieldDef = {
      id: "x",
      type: "lookup",
      lookupRelation: "primary-contact",
      lookupField: "email",
    };
    expect(resolveLookupField(deal, def, stores)).toBeUndefined();
  });
});

describe("aggregate", () => {
  it("count", () => expect(aggregate([1, 2, 3], "count")).toBe(3));
  it("sum", () => expect(aggregate([1, 2, 3], "sum")).toBe(6));
  it("avg", () => expect(aggregate([2, 4, 6], "avg")).toBe(4));
  it("min", () => expect(aggregate([3, 1, 2], "min")).toBe(1));
  it("max", () => expect(aggregate([3, 1, 2], "max")).toBe(3));
  it("list", () => expect(aggregate(["a", "b", "c"], "list")).toBe("a, b, c"));
  it("empty avg → 0", () => expect(aggregate([], "avg")).toBe(0));
  it("empty count → 0", () => expect(aggregate([], "count")).toBe(0));
});

describe("resolveRollupField", () => {
  it("sums child amounts", () => {
    const parent = makeObject({ id: "p1" as ObjectId, type: "project", name: "P" });
    const t1 = makeObject({ id: "t1" as ObjectId, type: "task", name: "T1", data: { hours: 3 } });
    const t2 = makeObject({ id: "t2" as ObjectId, type: "task", name: "T2", data: { hours: 5 } });
    const edges = [
      makeEdge({ id: "e1" as EdgeId, sourceId: "p1" as ObjectId, targetId: "t1" as ObjectId, relation: "has-task" }),
      makeEdge({ id: "e2" as EdgeId, sourceId: "p1" as ObjectId, targetId: "t2" as ObjectId, relation: "has-task" }),
    ];
    const stores = makeStores([parent, t1, t2], edges);
    const def: EntityFieldDef = {
      id: "totalHours",
      type: "rollup",
      rollupRelation: "has-task",
      rollupField: "hours",
      rollupFunction: "sum",
    };
    expect(resolveRollupField(parent, def, stores)).toBe(8);
  });

  it("counts children", () => {
    const parent = makeObject({ id: "p1" as ObjectId, type: "x", name: "P" });
    const c1 = makeObject({ id: "c1" as ObjectId, type: "x", name: "c1" });
    const c2 = makeObject({ id: "c2" as ObjectId, type: "x", name: "c2" });
    const edges = [
      makeEdge({ id: "e1" as EdgeId, sourceId: "p1" as ObjectId, targetId: "c1" as ObjectId, relation: "child" }),
      makeEdge({ id: "e2" as EdgeId, sourceId: "p1" as ObjectId, targetId: "c2" as ObjectId, relation: "child" }),
    ];
    const stores = makeStores([parent, c1, c2], edges);
    const def: EntityFieldDef = {
      id: "childCount",
      type: "rollup",
      rollupRelation: "child",
      rollupField: "name",
      rollupFunction: "count",
    };
    expect(resolveRollupField(parent, def, stores)).toBe(2);
  });
});

describe("resolveComputedField", () => {
  const stores = makeStores([], []);

  it("dispatches formula expressions", () => {
    const obj = makeObject({ id: "o1" as ObjectId, type: "x", name: "x", data: { a: 2, b: 3 } });
    const def: EntityFieldDef = { id: "sum", type: "float", expression: "a + b" };
    expect(resolveComputedField(obj, def, stores)).toBe(5);
  });

  it("returns undefined for non-computed fields", () => {
    const obj = makeObject({ id: "o1" as ObjectId, type: "x", name: "x" });
    const def: EntityFieldDef = { id: "plain", type: "string" };
    expect(resolveComputedField(obj, def, stores)).toBeUndefined();
  });
});
