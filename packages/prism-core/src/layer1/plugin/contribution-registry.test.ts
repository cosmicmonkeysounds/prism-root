import { describe, it, expect, beforeEach } from "vitest";
import { ContributionRegistry } from "./contribution-registry.js";

interface TestItem {
  id: string;
  label: string;
  category?: string;
}

describe("ContributionRegistry", () => {
  let registry: ContributionRegistry<TestItem>;

  beforeEach(() => {
    registry = new ContributionRegistry<TestItem>((item) => item.id);
  });

  describe("register", () => {
    it("registers a single item", () => {
      registry.register({ id: "a", label: "Alpha" }, "plugin-1");
      expect(registry.size).toBe(1);
      expect(registry.get("a")).toEqual({ id: "a", label: "Alpha" });
    });

    it("overwrites by key on collision", () => {
      registry.register({ id: "a", label: "v1" }, "plugin-1");
      registry.register({ id: "a", label: "v2" }, "plugin-2");
      expect(registry.get("a")?.label).toBe("v2");
    });
  });

  describe("registerAll", () => {
    it("registers multiple items from a plugin", () => {
      registry.registerAll(
        [
          { id: "a", label: "Alpha" },
          { id: "b", label: "Beta" },
        ],
        "plugin-1",
      );
      expect(registry.size).toBe(2);
    });

    it("handles undefined items array", () => {
      registry.registerAll(undefined, "plugin-1");
      expect(registry.size).toBe(0);
    });
  });

  describe("unregister", () => {
    it("removes item by key", () => {
      registry.register({ id: "a", label: "Alpha" }, "plugin-1");
      expect(registry.unregister("a")).toBe(true);
      expect(registry.size).toBe(0);
      expect(registry.get("a")).toBeUndefined();
    });

    it("returns false for unknown key", () => {
      expect(registry.unregister("nope")).toBe(false);
    });
  });

  describe("unregisterByPlugin", () => {
    it("removes all items from a plugin", () => {
      registry.register({ id: "a", label: "Alpha" }, "plugin-1");
      registry.register({ id: "b", label: "Beta" }, "plugin-1");
      registry.register({ id: "c", label: "Gamma" }, "plugin-2");
      expect(registry.unregisterByPlugin("plugin-1")).toBe(2);
      expect(registry.size).toBe(1);
      expect(registry.get("c")).toBeDefined();
    });

    it("returns 0 when plugin has no items", () => {
      expect(registry.unregisterByPlugin("nonexistent")).toBe(0);
    });
  });

  describe("queries", () => {
    beforeEach(() => {
      registry.register({ id: "a", label: "Alpha", category: "x" }, "p1");
      registry.register({ id: "b", label: "Beta", category: "y" }, "p1");
      registry.register({ id: "c", label: "Gamma", category: "x" }, "p2");
    });

    it("all() returns all items", () => {
      expect(registry.all()).toHaveLength(3);
    });

    it("has() checks existence", () => {
      expect(registry.has("a")).toBe(true);
      expect(registry.has("z")).toBe(false);
    });

    it("byPlugin() returns items from a specific plugin", () => {
      const items = registry.byPlugin("p1");
      expect(items).toHaveLength(2);
      expect(items.map((i) => i.id)).toContain("a");
      expect(items.map((i) => i.id)).toContain("b");
    });

    it("query() filters by predicate", () => {
      const xItems = registry.query((i) => i.category === "x");
      expect(xItems).toHaveLength(2);
    });

    it("getEntry() returns item with plugin metadata", () => {
      const entry = registry.getEntry("a");
      expect(entry?.pluginId).toBe("p1");
      expect(entry?.item.label).toBe("Alpha");
    });

    it("allEntries() returns copies", () => {
      const entries = registry.allEntries();
      expect(entries).toHaveLength(3);
      expect(entries[0].pluginId).toBeDefined();
    });
  });

  describe("clear", () => {
    it("removes all entries", () => {
      registry.register({ id: "a", label: "Alpha" }, "p1");
      registry.register({ id: "b", label: "Beta" }, "p1");
      registry.clear();
      expect(registry.size).toBe(0);
      expect(registry.all()).toEqual([]);
    });
  });
});
