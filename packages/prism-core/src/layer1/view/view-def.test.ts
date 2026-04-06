import { describe, it, expect } from "vitest";
import type { ViewRegistry, ViewDef } from "./view-def.js";
import { createViewRegistry } from "./view-def.js";

describe("ViewRegistry", () => {
  let registry: ViewRegistry;

  beforeEach(() => {
    registry = createViewRegistry();
  });

  describe("built-in modes", () => {
    it("has 7 built-in view modes", () => {
      expect(registry.all()).toHaveLength(7);
    });

    it.each([
      "list", "kanban", "grid", "table", "timeline", "calendar", "graph",
    ] as const)("has %s mode", (mode) => {
      const def = registry.get(mode);
      expect(def).toBeDefined();
      expect(def?.mode).toBe(mode);
      expect(def?.label).toBeTruthy();
      expect(def?.description).toBeTruthy();
    });

    it("returns undefined for unknown mode", () => {
      expect(registry.get("nope" as never)).toBeUndefined();
    });
  });

  describe("capabilities", () => {
    it("list supports sort, filter, grouping, inline edit, bulk select, hierarchy", () => {
      expect(registry.supports("list", "supportsSort")).toBe(true);
      expect(registry.supports("list", "supportsFilter")).toBe(true);
      expect(registry.supports("list", "supportsGrouping")).toBe(true);
      expect(registry.supports("list", "supportsInlineEdit")).toBe(true);
      expect(registry.supports("list", "supportsBulkSelect")).toBe(true);
      expect(registry.supports("list", "supportsHierarchy")).toBe(true);
      expect(registry.supports("list", "supportsColumns")).toBe(false);
    });

    it("table supports columns", () => {
      expect(registry.supports("table", "supportsColumns")).toBe(true);
    });

    it("kanban requires status", () => {
      expect(registry.supports("kanban", "requiresStatus")).toBe(true);
    });

    it("timeline requires date", () => {
      expect(registry.supports("timeline", "requiresDate")).toBe(true);
    });

    it("calendar requires date", () => {
      expect(registry.supports("calendar", "requiresDate")).toBe(true);
    });

    it("graph does not support sort", () => {
      expect(registry.supports("graph", "supportsSort")).toBe(false);
    });

    it("returns false for unknown mode", () => {
      expect(registry.supports("nope" as never, "supportsSort")).toBe(false);
    });
  });

  describe("modesWithCapability", () => {
    it("finds all modes supporting sort", () => {
      const modes = registry.modesWithCapability("supportsSort");
      expect(modes).toContain("list");
      expect(modes).toContain("table");
      expect(modes).toContain("kanban");
      expect(modes).toContain("grid");
      expect(modes).not.toContain("timeline");
      expect(modes).not.toContain("calendar");
      expect(modes).not.toContain("graph");
    });

    it("finds modes requiring date", () => {
      const modes = registry.modesWithCapability("requiresDate");
      expect(modes).toEqual(expect.arrayContaining(["timeline", "calendar"]));
      expect(modes).toHaveLength(2);
    });

    it("finds modes supporting columns", () => {
      const modes = registry.modesWithCapability("supportsColumns");
      expect(modes).toEqual(["table"]);
    });
  });

  describe("custom registration", () => {
    it("registers a custom view mode", () => {
      const custom: ViewDef = {
        mode: "list", // override built-in
        label: "Custom List",
        description: "My custom list",
        supportsSort: true,
        supportsFilter: false,
        supportsGrouping: false,
        supportsColumns: false,
        supportsInlineEdit: false,
        supportsBulkSelect: false,
        supportsHierarchy: false,
        requiresDate: false,
        requiresStatus: false,
      };
      registry.register(custom);

      const def = registry.get("list");
      expect(def?.label).toBe("Custom List");
      expect(def?.supportsFilter).toBe(false);
    });
  });
});
