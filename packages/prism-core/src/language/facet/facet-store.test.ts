import { describe, it, expect, vi } from "vitest";
import { createFacetStore } from "./facet-store.js";
import { createFacetDefinition } from "./facet-schema.js";
import { createStaticValueList } from "./value-list.js";
import { createVisualScript } from "./script-steps.js";

describe("FacetStore", () => {
  function makeDef(id: string, objectType = "contact") {
    return createFacetDefinition(id, objectType, "form");
  }

  describe("facets", () => {
    it("stores and retrieves a facet definition", () => {
      const store = createFacetStore();
      store.putFacet(makeDef("f1"));
      expect(store.getFacet("f1")?.id).toBe("f1");
    });

    it("lists all facets", () => {
      const store = createFacetStore();
      store.putFacet(makeDef("f1"));
      store.putFacet(makeDef("f2"));
      expect(store.listFacets()).toHaveLength(2);
    });

    it("removes a facet", () => {
      const store = createFacetStore();
      store.putFacet(makeDef("f1"));
      expect(store.removeFacet("f1")).toBe(true);
      expect(store.getFacet("f1")).toBeUndefined();
    });

    it("facetsForType filters by objectType", () => {
      const store = createFacetStore();
      store.putFacet(makeDef("f1", "contact"));
      store.putFacet(makeDef("f2", "invoice"));
      store.putFacet(makeDef("f3", "contact"));
      expect(store.facetsForType("contact")).toHaveLength(2);
      expect(store.facetsForType("invoice")).toHaveLength(1);
    });
  });

  describe("scripts", () => {
    it("stores and retrieves a script", () => {
      const store = createFacetStore();
      store.putScript(createVisualScript("s1", "My Script"));
      expect(store.getScript("s1")?.name).toBe("My Script");
    });

    it("lists and removes scripts", () => {
      const store = createFacetStore();
      store.putScript(createVisualScript("s1", "A"));
      store.putScript(createVisualScript("s2", "B"));
      expect(store.listScripts()).toHaveLength(2);
      store.removeScript("s1");
      expect(store.listScripts()).toHaveLength(1);
    });
  });

  describe("value lists", () => {
    it("stores and retrieves a value list", () => {
      const store = createFacetStore();
      store.putValueList(createStaticValueList("vl1", "Status", [{ value: "a" }]));
      expect(store.getValueList("vl1")?.name).toBe("Status");
    });

    it("removes a value list", () => {
      const store = createFacetStore();
      store.putValueList(createStaticValueList("vl1", "S", []));
      expect(store.removeValueList("vl1")).toBe(true);
      expect(store.getValueList("vl1")).toBeUndefined();
    });
  });

  describe("onChange", () => {
    it("notifies on facet put", () => {
      const store = createFacetStore();
      const listener = vi.fn();
      store.onChange(listener);
      store.putFacet(makeDef("f1"));
      expect(listener).toHaveBeenCalledTimes(1);
    });

    it("notifies on script put", () => {
      const store = createFacetStore();
      const listener = vi.fn();
      store.onChange(listener);
      store.putScript(createVisualScript("s1", "S"));
      expect(listener).toHaveBeenCalledTimes(1);
    });

    it("unsubscribe stops notifications", () => {
      const store = createFacetStore();
      const listener = vi.fn();
      const unsub = store.onChange(listener);
      unsub();
      store.putFacet(makeDef("f1"));
      expect(listener).not.toHaveBeenCalled();
    });
  });

  describe("serialize/load", () => {
    it("round-trips all data", () => {
      const store = createFacetStore();
      store.putFacet(makeDef("f1", "contact"));
      store.putScript(createVisualScript("s1", "Script 1"));
      store.putValueList(createStaticValueList("vl1", "Status", [{ value: "x" }]));

      const snapshot = store.serialize();
      expect(snapshot.facets).toHaveLength(1);
      expect(snapshot.scripts).toHaveLength(1);
      expect(snapshot.valueLists).toHaveLength(1);

      const store2 = createFacetStore();
      store2.load(snapshot);
      expect(store2.getFacet("f1")?.objectType).toBe("contact");
      expect(store2.getScript("s1")?.name).toBe("Script 1");
      expect(store2.getValueList("vl1")?.name).toBe("Status");
    });

    it("load replaces existing data", () => {
      const store = createFacetStore();
      store.putFacet(makeDef("old"));
      store.load({ facets: [makeDef("new")], scripts: [], valueLists: [] });
      expect(store.getFacet("old")).toBeUndefined();
      expect(store.getFacet("new")).toBeDefined();
    });
  });

  describe("size", () => {
    it("tracks total size across all registries", () => {
      const store = createFacetStore();
      expect(store.size).toBe(0);
      store.putFacet(makeDef("f1"));
      store.putScript(createVisualScript("s1", "S"));
      store.putValueList(createStaticValueList("v1", "V", []));
      expect(store.size).toBe(3);
    });
  });
});
