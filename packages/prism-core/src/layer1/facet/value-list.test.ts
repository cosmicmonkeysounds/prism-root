import { describe, it, expect, vi } from "vitest";
import {
  createStaticValueList,
  createDynamicValueList,
  resolveValueList,
  createValueListRegistry,
  type ValueListItem,
  type ValueListResolver,
  type DynamicValueListSource,
} from "./value-list.js";

// ── Factories ───────────────────────────────────────────────────────────────

describe("createStaticValueList", () => {
  it("creates a static value list", () => {
    const list = createStaticValueList("status", "Status", [
      { value: "active", label: "Active" },
      { value: "inactive", label: "Inactive" },
    ]);
    expect(list.id).toBe("status");
    expect(list.name).toBe("Status");
    expect(list.source.kind).toBe("static");
    if (list.source.kind === "static") {
      expect(list.source.items).toHaveLength(2);
    }
  });

  it("copies items array (no mutation leak)", () => {
    const items: ValueListItem[] = [{ value: "a" }];
    const list = createStaticValueList("test", "Test", items);
    items.push({ value: "b" });
    if (list.source.kind === "static") {
      expect(list.source.items).toHaveLength(1);
    }
  });
});

describe("createDynamicValueList", () => {
  it("creates a dynamic value list", () => {
    const list = createDynamicValueList("clients", "Clients", {
      collectionId: "contacts",
      valueField: "id",
      displayField: "name",
    });
    expect(list.id).toBe("clients");
    expect(list.source.kind).toBe("dynamic");
    if (list.source.kind === "dynamic") {
      expect(list.source.collectionId).toBe("contacts");
      expect(list.source.valueField).toBe("id");
      expect(list.source.displayField).toBe("name");
    }
  });

  it("preserves optional config", () => {
    const list = createDynamicValueList("sorted", "Sorted", {
      collectionId: "items",
      valueField: "id",
      displayField: "name",
      sortField: "name",
      sortDirection: "desc",
      filter: { field: "active", op: "eq", value: true },
      limit: 50,
    });
    if (list.source.kind === "dynamic") {
      expect(list.source.sortField).toBe("name");
      expect(list.source.sortDirection).toBe("desc");
      expect(list.source.filter?.field).toBe("active");
      expect(list.source.limit).toBe(50);
    }
  });
});

// ── Resolution ──────────────────────────────────────────────────────────────

describe("resolveValueList", () => {
  it("resolves static list directly", () => {
    const list = createStaticValueList("s", "S", [
      { value: "a", label: "A" },
      { value: "b", label: "B" },
    ]);
    const items = resolveValueList(list);
    expect(items).toHaveLength(2);
    expect(items[0]?.value).toBe("a");
  });

  it("returns copy for static (no mutation)", () => {
    const list = createStaticValueList("s", "S", [{ value: "a" }]);
    const items1 = resolveValueList(list);
    const items2 = resolveValueList(list);
    items1.push({ value: "z" });
    expect(items2).toHaveLength(1);
  });

  it("returns empty for dynamic without resolver", () => {
    const list = createDynamicValueList("d", "D", {
      collectionId: "c",
      valueField: "id",
      displayField: "name",
    });
    expect(resolveValueList(list)).toEqual([]);
  });

  it("uses resolver for dynamic lists", () => {
    const list = createDynamicValueList("d", "D", {
      collectionId: "contacts",
      valueField: "id",
      displayField: "name",
    });
    const resolver: ValueListResolver = {
      resolve(_source: DynamicValueListSource): ValueListItem[] {
        return [
          { value: "c1", label: "Alice" },
          { value: "c2", label: "Bob" },
        ];
      },
    };
    const items = resolveValueList(list, resolver);
    expect(items).toHaveLength(2);
    expect(items[1]?.label).toBe("Bob");
  });

  it("passes source config to resolver", () => {
    const list = createDynamicValueList("d", "D", {
      collectionId: "contacts",
      valueField: "id",
      displayField: "name",
      sortField: "name",
    });
    const resolveFn = vi.fn().mockReturnValue([]);
    const resolver: ValueListResolver = { resolve: resolveFn };
    resolveValueList(list, resolver);
    expect(resolveFn).toHaveBeenCalledWith(
      expect.objectContaining({
        kind: "dynamic",
        collectionId: "contacts",
        sortField: "name",
      }),
    );
  });
});

// ── Registry ────────────────────────────────────────────────────────────────

describe("ValueListRegistry", () => {
  describe("register/get/remove", () => {
    it("registers and retrieves a list", () => {
      const reg = createValueListRegistry();
      const list = createStaticValueList("s", "Status", [{ value: "a" }]);
      reg.register(list);
      expect(reg.get("s")?.name).toBe("Status");
    });

    it("overwrites existing list with same id", () => {
      const reg = createValueListRegistry();
      reg.register(createStaticValueList("s", "V1", [{ value: "a" }]));
      reg.register(createStaticValueList("s", "V2", [{ value: "b" }]));
      expect(reg.get("s")?.name).toBe("V2");
      expect(reg.size).toBe(1);
    });

    it("removes a list", () => {
      const reg = createValueListRegistry();
      reg.register(createStaticValueList("s", "S", []));
      expect(reg.remove("s")).toBe(true);
      expect(reg.get("s")).toBeUndefined();
    });

    it("returns false removing unknown id", () => {
      const reg = createValueListRegistry();
      expect(reg.remove("nope")).toBe(false);
    });

    it("tracks size", () => {
      const reg = createValueListRegistry();
      expect(reg.size).toBe(0);
      reg.register(createStaticValueList("a", "A", []));
      reg.register(createStaticValueList("b", "B", []));
      expect(reg.size).toBe(2);
    });
  });

  describe("all/search", () => {
    it("all() returns all lists", () => {
      const reg = createValueListRegistry();
      reg.register(createStaticValueList("a", "Alpha", []));
      reg.register(createStaticValueList("b", "Beta", []));
      expect(reg.all()).toHaveLength(2);
    });

    it("search() by name case-insensitive", () => {
      const reg = createValueListRegistry();
      reg.register(createStaticValueList("a", "Task Status", []));
      reg.register(createStaticValueList("b", "Priority", []));
      expect(reg.search("status")).toHaveLength(1);
      expect(reg.search("STATUS")).toHaveLength(1);
    });

    it("search() by description", () => {
      const reg = createValueListRegistry();
      const list = createStaticValueList("a", "A", []);
      list.description = "Workflow states";
      reg.register(list);
      expect(reg.search("workflow")).toHaveLength(1);
    });
  });

  describe("resolve", () => {
    it("resolves static list through registry", () => {
      const reg = createValueListRegistry();
      reg.register(createStaticValueList("s", "S", [{ value: "x", label: "X" }]));
      const items = reg.resolve("s");
      expect(items).toHaveLength(1);
    });

    it("returns empty for unknown id", () => {
      const reg = createValueListRegistry();
      expect(reg.resolve("nope")).toEqual([]);
    });

    it("resolves dynamic list with resolver", () => {
      const reg = createValueListRegistry();
      reg.register(
        createDynamicValueList("d", "D", {
          collectionId: "c",
          valueField: "id",
          displayField: "name",
        }),
      );
      const resolver: ValueListResolver = {
        resolve: () => [{ value: "v1", label: "Val 1" }],
      };
      const items = reg.resolve("d", resolver);
      expect(items).toHaveLength(1);
    });
  });

  describe("subscribe", () => {
    it("notifies on register", () => {
      const reg = createValueListRegistry();
      const listener = vi.fn();
      reg.subscribe(listener);
      reg.register(createStaticValueList("a", "A", []));
      expect(listener).toHaveBeenCalledTimes(1);
    });

    it("notifies on remove", () => {
      const reg = createValueListRegistry();
      reg.register(createStaticValueList("a", "A", []));
      const listener = vi.fn();
      reg.subscribe(listener);
      reg.remove("a");
      expect(listener).toHaveBeenCalledWith([]);
    });

    it("unsubscribe stops notifications", () => {
      const reg = createValueListRegistry();
      const listener = vi.fn();
      const unsub = reg.subscribe(listener);
      unsub();
      reg.register(createStaticValueList("a", "A", []));
      expect(listener).not.toHaveBeenCalled();
    });
  });

  describe("serialize/load", () => {
    it("round-trips through serialize/load", () => {
      const reg = createValueListRegistry();
      reg.register(createStaticValueList("a", "Alpha", [{ value: "x" }]));
      reg.register(
        createDynamicValueList("b", "Beta", {
          collectionId: "c",
          valueField: "id",
          displayField: "name",
        }),
      );

      const data = reg.serialize();
      const reg2 = createValueListRegistry();
      reg2.load(data);

      expect(reg2.size).toBe(2);
      expect(reg2.get("a")?.name).toBe("Alpha");
      expect(reg2.get("b")?.source.kind).toBe("dynamic");
    });

    it("load replaces existing data", () => {
      const reg = createValueListRegistry();
      reg.register(createStaticValueList("old", "Old", []));
      reg.load([createStaticValueList("new", "New", [])]);
      expect(reg.size).toBe(1);
      expect(reg.get("old")).toBeUndefined();
      expect(reg.get("new")).toBeDefined();
    });
  });
});
