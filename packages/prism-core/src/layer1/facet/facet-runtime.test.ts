import { describe, it, expect } from "vitest";
import type { GraphObject } from "../object-model/index.js";
import type { FieldSlot, TextSlot } from "./facet-schema.js";
import {
  evaluateConditionalFormats,
  computeFieldStyle,
  interpolateMergeFields,
  renderTextSlot,
  createCollectionValueListResolver,
  getValueListId,
  getBoundFields,
} from "./facet-runtime.js";
import { createFacetDefinition } from "./facet-schema.js";

// ── Test helpers ────────────────────────────────────────────────────────────

function makeObject(data: Record<string, unknown>, overrides?: Partial<GraphObject>): GraphObject {
  return {
    id: "obj-1",
    type: "test",
    name: "Test Object",
    parentId: null,
    position: 0,
    status: "active",
    tags: [],
    date: null,
    endDate: null,
    description: "",
    color: null,
    image: null,
    pinned: false,
    createdAt: "2026-01-01T00:00:00Z",
    updatedAt: "2026-01-01T00:00:00Z",
    deletedAt: null,
    data,
    ...overrides,
  };
}

// ── Conditional Formatting ──────────────────────────────────────────────────

describe("evaluateConditionalFormats", () => {
  it("returns empty style when no formats match", () => {
    const style = evaluateConditionalFormats(
      [{ expression: '[field:amount] > 1000', backgroundColor: "#f00" }],
      makeObject({ amount: 500 }),
    );
    expect(style).toEqual({});
  });

  it("returns style when expression matches", () => {
    const style = evaluateConditionalFormats(
      [{ expression: '[field:amount] > 1000', backgroundColor: "#f00", textColor: "#fff" }],
      makeObject({ amount: 2000 }),
    );
    expect(style.backgroundColor).toBe("#f00");
    expect(style.textColor).toBe("#fff");
  });

  it("supports == operator with string values", () => {
    const style = evaluateConditionalFormats(
      [{ expression: '[field:status] == "overdue"', backgroundColor: "#ff0" }],
      makeObject({ status: "overdue" }),
    );
    expect(style.backgroundColor).toBe("#ff0");
  });

  it("supports != operator", () => {
    const style = evaluateConditionalFormats(
      [{ expression: '[field:status] != "active"', border: "2px solid red" }],
      makeObject({ status: "overdue" }),
    );
    expect(style.border).toBe("2px solid red");
  });

  it("later rules override earlier rules", () => {
    const style = evaluateConditionalFormats(
      [
        { expression: '[field:amount] > 0', backgroundColor: "#0f0" },
        { expression: '[field:amount] > 1000', backgroundColor: "#f00" },
      ],
      makeObject({ amount: 2000 }),
    );
    expect(style.backgroundColor).toBe("#f00");
  });

  it("merges non-overlapping properties", () => {
    const style = evaluateConditionalFormats(
      [
        { expression: '[field:amount] > 0', backgroundColor: "#0f0" },
        { expression: '[field:amount] > 1000', fontWeight: 700 },
      ],
      makeObject({ amount: 2000 }),
    );
    expect(style.backgroundColor).toBe("#0f0");
    expect(style.fontWeight).toBe(700);
  });
});

describe("computeFieldStyle", () => {
  it("returns empty for slot without conditionalFormats", () => {
    const slot: FieldSlot = { fieldPath: "name", part: "body", order: 0 };
    expect(computeFieldStyle(slot, makeObject({}))).toEqual({});
  });

  it("evaluates conditional formats on the slot", () => {
    const slot: FieldSlot = {
      fieldPath: "amount",
      part: "body",
      order: 0,
      conditionalFormats: [
        { expression: '[field:amount] >= 100', backgroundColor: "#eef" },
      ],
    };
    const style = computeFieldStyle(slot, makeObject({ amount: 200 }));
    expect(style.backgroundColor).toBe("#eef");
  });
});

// ── Merge Field Interpolation ───────────────────────────────────────────────

describe("interpolateMergeFields", () => {
  it("replaces {{fieldName}} with field value", () => {
    const result = interpolateMergeFields(
      "Hello, {{firstName}}!",
      makeObject({ firstName: "Alice" }),
    );
    expect(result).toBe("Hello, Alice!");
  });

  it("replaces multiple merge fields", () => {
    const result = interpolateMergeFields(
      "{{firstName}} {{lastName}} ({{role}})",
      makeObject({ firstName: "Bob", lastName: "Smith", role: "Admin" }),
    );
    expect(result).toBe("Bob Smith (Admin)");
  });

  it("replaces missing fields with empty string", () => {
    const result = interpolateMergeFields(
      "Name: {{missing}}",
      makeObject({}),
    );
    expect(result).toBe("Name: ");
  });

  it("supports dot-notation paths", () => {
    const result = interpolateMergeFields(
      "City: {{address.city}}",
      makeObject({ address: { city: "Portland" } }),
    );
    expect(result).toBe("City: Portland");
  });

  it("falls back to shell fields", () => {
    const result = interpolateMergeFields(
      "Record: {{name}} ({{type}})",
      makeObject({}, { name: "Invoice #42", type: "invoice" }),
    );
    expect(result).toBe("Record: Invoice #42 (invoice)");
  });

  it("returns original text when no merge fields", () => {
    expect(interpolateMergeFields("Plain text", makeObject({}))).toBe("Plain text");
  });
});

describe("renderTextSlot", () => {
  it("interpolates text slot content", () => {
    const slot: TextSlot = { text: "Total: {{amount}}", part: "body", order: 0 };
    expect(renderTextSlot(slot, makeObject({ amount: 1500 }))).toBe("Total: 1500");
  });
});

// ── CollectionStore Value List Resolver ──────────────────────────────────────

describe("createCollectionValueListResolver", () => {
  const contacts: GraphObject[] = [
    makeObject({ role: "client" }, { id: "c1", name: "Alice" }),
    makeObject({ role: "vendor" }, { id: "c2", name: "Bob" }),
    makeObject({ role: "client" }, { id: "c3", name: "Charlie" }),
  ];

  const collections = {
    contacts: { allObjects: () => contacts },
  };

  it("resolves items from a collection", () => {
    const resolver = createCollectionValueListResolver(collections);
    const items = resolver.resolve({
      kind: "dynamic",
      collectionId: "contacts",
      valueField: "id",
      displayField: "name",
    });
    expect(items).toHaveLength(3);
    expect(items[0]?.value).toBe("c1");
    expect(items[0]?.label).toBe("Alice");
  });

  it("applies filter", () => {
    const resolver = createCollectionValueListResolver(collections);
    const items = resolver.resolve({
      kind: "dynamic",
      collectionId: "contacts",
      valueField: "id",
      displayField: "name",
      filter: { field: "role", op: "eq", value: "client" },
    });
    expect(items).toHaveLength(2);
    expect(items.map((i) => i.label)).toEqual(["Alice", "Charlie"]);
  });

  it("applies sort", () => {
    const resolver = createCollectionValueListResolver(collections);
    const items = resolver.resolve({
      kind: "dynamic",
      collectionId: "contacts",
      valueField: "id",
      displayField: "name",
      sortField: "name",
      sortDirection: "desc",
    });
    expect(items[0]?.label).toBe("Charlie");
    expect(items[2]?.label).toBe("Alice");
  });

  it("applies limit", () => {
    const resolver = createCollectionValueListResolver(collections);
    const items = resolver.resolve({
      kind: "dynamic",
      collectionId: "contacts",
      valueField: "id",
      displayField: "name",
      limit: 2,
    });
    expect(items).toHaveLength(2);
  });

  it("returns empty for unknown collection", () => {
    const resolver = createCollectionValueListResolver(collections);
    const items = resolver.resolve({
      kind: "dynamic",
      collectionId: "unknown",
      valueField: "id",
      displayField: "name",
    });
    expect(items).toEqual([]);
  });
});

// ── FacetDefinition helpers ─────────────────────────────────────────────────

describe("getValueListId / getBoundFields", () => {
  it("returns undefined when no bindings", () => {
    const def = createFacetDefinition("f", "obj", "form");
    expect(getValueListId(def, "status")).toBeUndefined();
  });

  it("returns value list id for bound field", () => {
    const def = createFacetDefinition("f", "obj", "form");
    def.valueListBindings = { status: "status-list", priority: "priority-list" };
    expect(getValueListId(def, "status")).toBe("status-list");
    expect(getValueListId(def, "priority")).toBe("priority-list");
    expect(getValueListId(def, "name")).toBeUndefined();
  });

  it("getBoundFields returns all bindings", () => {
    const def = createFacetDefinition("f", "obj", "form");
    def.valueListBindings = { status: "status-list", priority: "priority-list" };
    const bindings = getBoundFields(def);
    expect(bindings).toHaveLength(2);
    expect(bindings).toContainEqual({ fieldPath: "status", valueListId: "status-list" });
  });

  it("getBoundFields returns empty when no bindings", () => {
    const def = createFacetDefinition("f", "obj", "form");
    expect(getBoundFields(def)).toEqual([]);
  });
});
