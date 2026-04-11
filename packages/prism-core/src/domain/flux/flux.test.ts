import { describe, it, expect, beforeEach } from "vitest";
import { createFluxRegistry } from "./flux.js";
import { FLUX_TYPES, FLUX_EDGES, FLUX_CATEGORIES } from "./flux-types.js";
import type { FluxRegistry } from "./flux-types.js";

describe("Flux Registry", () => {
  let registry: FluxRegistry;

  beforeEach(() => {
    registry = createFluxRegistry();
  });

  // ── Entity Definitions ──────────────────────────────────────────────

  describe("Entity Definitions", () => {
    it("registers 11 entity types", () => {
      expect(registry.getEntityDefs()).toHaveLength(11);
    });

    it("covers all FLUX_TYPES", () => {
      const types = registry.getEntityDefs().map(d => d.type);
      for (const fluxType of Object.values(FLUX_TYPES)) {
        expect(types).toContain(fluxType);
      }
    });

    it("assigns NSIDs to all entities", () => {
      for (const def of registry.getEntityDefs()) {
        expect(def.nsid).toBeDefined();
        expect(def.nsid).toMatch(/^io\.prismapp\.flux\./);
      }
    });

    it("assigns categories to all entities", () => {
      const categories = new Set(registry.getEntityDefs().map(d => d.category));
      expect(categories.size).toBe(4);
      for (const cat of Object.values(FLUX_CATEGORIES)) {
        expect(categories).toContain(cat);
      }
    });

    it("retrieves task entity by type", () => {
      const task = registry.getEntityDef(FLUX_TYPES.TASK);
      expect(task).toBeDefined();
      expect(task?.label).toBe("Task");
      expect(task?.category).toBe(FLUX_CATEGORIES.PRODUCTIVITY);
    });

    it("task has required fields", () => {
      const task = registry.getEntityDef(FLUX_TYPES.TASK);
      const fieldIds = task?.fields?.map(f => f.id) ?? [];
      expect(fieldIds).toContain("priority");
      expect(fieldIds).toContain("dueDate");
      expect(fieldIds).toContain("effort");
      expect(fieldIds).toContain("recurring");
      expect(fieldIds).toContain("estimateHours");
    });

    it("contact has CRM fields", () => {
      const contact = registry.getEntityDef(FLUX_TYPES.CONTACT);
      const fieldIds = contact?.fields?.map(f => f.id) ?? [];
      expect(fieldIds).toContain("email");
      expect(fieldIds).toContain("phone");
      expect(fieldIds).toContain("dealValue");
      expect(fieldIds).toContain("dealStage");
    });

    it("transaction fields have required markers", () => {
      const txn = registry.getEntityDef(FLUX_TYPES.TRANSACTION);
      const amount = txn?.fields?.find(f => f.id === "amount");
      expect(amount?.required).toBe(true);
      expect(amount?.type).toBe("float");
    });

    it("invoice has computed fields", () => {
      const inv = registry.getEntityDef(FLUX_TYPES.INVOICE);
      const taxAmount = inv?.fields?.find(f => f.id === "taxAmount");
      const total = inv?.fields?.find(f => f.id === "total");
      expect(taxAmount?.expression).toBeDefined();
      expect(total?.expression).toBeDefined();
    });

    it("item has stock value formula", () => {
      const item = registry.getEntityDef(FLUX_TYPES.ITEM);
      const stockValue = item?.fields?.find(f => f.id === "stockValue");
      expect(stockValue?.expression).toBe("quantity * costPrice");
    });

    it("milestone is childOnly", () => {
      const ms = registry.getEntityDef(FLUX_TYPES.MILESTONE);
      expect(ms?.childOnly).toBe(true);
    });

    it("project has extraChildTypes", () => {
      const proj = registry.getEntityDef(FLUX_TYPES.PROJECT);
      expect(proj?.extraChildTypes).toContain(FLUX_TYPES.TASK);
      expect(proj?.extraChildTypes).toContain(FLUX_TYPES.MILESTONE);
    });

    it("returns undefined for unknown type", () => {
      expect(registry.getEntityDef("unknown" as never)).toBeUndefined();
    });
  });

  // ── Edge Definitions ────────────────────────────────────────────────

  describe("Edge Definitions", () => {
    it("registers 7 edge types", () => {
      expect(registry.getEdgeDefs()).toHaveLength(7);
    });

    it("covers all FLUX_EDGES", () => {
      const relations = registry.getEdgeDefs().map(d => d.relation);
      for (const rel of Object.values(FLUX_EDGES)) {
        expect(relations).toContain(rel);
      }
    });

    it("assigned-to edge has correct constraints", () => {
      const edge = registry.getEdgeDef(FLUX_EDGES.ASSIGNED_TO);
      expect(edge?.behavior).toBe("assignment");
      expect(edge?.targetTypes).toContain(FLUX_TYPES.CONTACT);
      expect(edge?.sourceTypes).toContain(FLUX_TYPES.TASK);
    });

    it("depends-on is a dependency edge", () => {
      const edge = registry.getEdgeDef(FLUX_EDGES.DEPENDS_ON);
      expect(edge?.behavior).toBe("dependency");
      expect(edge?.sourceCategories).toContain(FLUX_CATEGORIES.PRODUCTIVITY);
    });

    it("related-to is undirected", () => {
      const edge = registry.getEdgeDef(FLUX_EDGES.RELATED_TO);
      expect(edge?.undirected).toBe(true);
      expect(edge?.behavior).toBe("weak");
    });

    it("stored-at links items to locations", () => {
      const edge = registry.getEdgeDef(FLUX_EDGES.STORED_AT);
      expect(edge?.sourceTypes).toContain(FLUX_TYPES.ITEM);
      expect(edge?.targetTypes).toContain(FLUX_TYPES.LOCATION);
    });

    it("returns undefined for unknown relation", () => {
      expect(registry.getEdgeDef("unknown" as never)).toBeUndefined();
    });
  });

  // ── Automation Presets ──────────────────────────────────────────────

  describe("Automation Presets", () => {
    it("has 8 built-in presets", () => {
      expect(registry.getAutomationPresets()).toHaveLength(8);
    });

    it("filters presets by entity type", () => {
      const taskPresets = registry.getPresetsForEntity(FLUX_TYPES.TASK);
      expect(taskPresets.length).toBeGreaterThanOrEqual(2);
      for (const p of taskPresets) {
        expect(p.entityType).toBe(FLUX_TYPES.TASK);
      }
    });

    it("task completion preset sets timestamp", () => {
      const presets = registry.getPresetsForEntity(FLUX_TYPES.TASK);
      const complete = presets.find(p => p.id === "flux:auto:task-complete-timestamp");
      expect(complete).toBeDefined();
      expect(complete?.trigger).toBe("on_status_change");
      expect(complete?.condition).toContain("done");
      expect(complete?.actions[0]?.kind).toBe("set_field");
    });

    it("invoice overdue preset changes status", () => {
      const presets = registry.getPresetsForEntity(FLUX_TYPES.INVOICE);
      const overdue = presets.find(p => p.id === "flux:auto:invoice-overdue");
      expect(overdue).toBeDefined();
      expect(overdue?.trigger).toBe("on_due_date");
      expect(overdue?.actions[0]?.kind).toBe("move_to_status");
    });

    it("item low stock preset sends notification", () => {
      const presets = registry.getPresetsForEntity(FLUX_TYPES.ITEM);
      const lowStock = presets.find(p => p.id === "flux:auto:item-low-stock");
      expect(lowStock).toBeDefined();
      expect(lowStock?.actions).toHaveLength(2);
    });

    it("returns empty array for entity without presets", () => {
      const presets = registry.getPresetsForEntity(FLUX_TYPES.LOCATION);
      expect(presets).toHaveLength(0);
    });
  });

  // ── Import/Export ──────────────────────────────────────────────────

  describe("CSV Export/Import", () => {
    it("exports objects to CSV", () => {
      const data = [
        { name: "Task 1", priority: "high", amount: 100 },
        { name: "Task 2", priority: "low", amount: 200 },
      ];
      const csv = registry.exportData(data, {
        entityType: FLUX_TYPES.TASK,
        format: "csv",
      });
      expect(csv).toContain("name,priority,amount");
      expect(csv).toContain("Task 1,high,100");
      expect(csv).toContain("Task 2,low,200");
    });

    it("exports with specific fields", () => {
      const data = [
        { name: "Task 1", priority: "high", amount: 100 },
      ];
      const csv = registry.exportData(data, {
        entityType: FLUX_TYPES.TASK,
        format: "csv",
        fields: ["name", "priority"],
      });
      expect(csv).toContain("name,priority");
      expect(csv).not.toContain("amount");
    });

    it("escapes CSV values with commas", () => {
      const data = [{ name: "Task, with comma", value: 42 }];
      const csv = registry.exportData(data, {
        entityType: FLUX_TYPES.TASK,
        format: "csv",
      });
      expect(csv).toContain('"Task, with comma"');
    });

    it("parses CSV back to objects", () => {
      const csv = "name,priority,amount\nTask 1,high,100\nTask 2,low,200";
      const parsed = registry.parseImport(csv, "csv");
      expect(parsed).toHaveLength(2);
      expect(parsed[0]).toEqual({ name: "Task 1", priority: "high", amount: 100 });
      expect(parsed[1]).toEqual({ name: "Task 2", priority: "low", amount: 200 });
    });

    it("returns empty array for empty CSV", () => {
      expect(registry.parseImport("", "csv")).toHaveLength(0);
    });

    it("returns empty array for header-only CSV", () => {
      expect(registry.parseImport("name,priority", "csv")).toHaveLength(0);
    });
  });

  describe("JSON Export/Import", () => {
    it("exports objects to JSON", () => {
      const data = [{ name: "Test", value: 42 }];
      const json = registry.exportData(data, {
        entityType: FLUX_TYPES.TASK,
        format: "json",
      });
      const parsed = JSON.parse(json);
      expect(parsed).toEqual(data);
    });

    it("parses JSON back to objects", () => {
      const json = '[{"name":"Task 1","priority":"high"}]';
      const parsed = registry.parseImport(json, "json");
      expect(parsed).toHaveLength(1);
      expect(parsed[0]).toEqual({ name: "Task 1", priority: "high" });
    });

    it("throws on invalid JSON", () => {
      expect(() => registry.parseImport("not json", "json")).toThrow();
    });

    it("throws on non-array JSON", () => {
      expect(() => registry.parseImport('{"key":"val"}', "json")).toThrow("Expected JSON array");
    });
  });

  // ── Empty data ─────────────────────────────────────────────────────

  describe("Edge cases", () => {
    it("exports empty array to CSV", () => {
      const csv = registry.exportData([], { entityType: FLUX_TYPES.TASK, format: "csv" });
      expect(csv).toBe("");
    });

    it("exports empty array to JSON", () => {
      const json = registry.exportData([], { entityType: FLUX_TYPES.TASK, format: "json" });
      expect(json).toBe("[]");
    });
  });
});
