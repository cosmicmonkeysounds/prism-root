import { describe, it, expect, vi } from "vitest";
import {
  createSavedView,
  createSavedViewRegistry,
  type SavedView,
} from "./saved-view.js";

// ── createSavedView factory ─────────────────────────────────────────────────

describe("createSavedView", () => {
  it("creates a view with correct id, objectType, and config", () => {
    const view = createSavedView("v1", "contact", {
      filters: [{ field: "status", op: "eq", value: "active" }],
    });
    expect(view.id).toBe("v1");
    expect(view.objectType).toBe("contact");
    expect(view.config.filters).toHaveLength(1);
  });

  it("defaults name to the id", () => {
    const view = createSavedView("active-contacts", "contact", {});
    expect(view.name).toBe("active-contacts");
  });

  it("accepts a custom name", () => {
    const view = createSavedView("v1", "contact", {}, "Active Contacts");
    expect(view.name).toBe("Active Contacts");
  });

  it("defaults mode to list", () => {
    const view = createSavedView("v1", "task", {});
    expect(view.mode).toBe("list");
  });

  it("defaults pinned and shared to false", () => {
    const view = createSavedView("v1", "task", {});
    expect(view.pinned).toBe(false);
    expect(view.shared).toBe(false);
  });

  it("sets createdAt and updatedAt timestamps", () => {
    const view = createSavedView("v1", "task", {});
    expect(view.createdAt).toBeTruthy();
    expect(view.updatedAt).toBeTruthy();
  });
});

// ── SavedViewRegistry ──────────────────────────────────────────────────────

describe("SavedViewRegistry", () => {
  function makeView(id: string, objectType = "contact"): SavedView {
    return createSavedView(id, objectType, {
      filters: [{ field: "status", op: "eq", value: "active" }],
    });
  }

  describe("add/get/remove", () => {
    it("adds and retrieves a view", () => {
      const reg = createSavedViewRegistry();
      reg.add(makeView("v1"));
      expect(reg.get("v1")).toBeDefined();
      expect(reg.get("v1")?.id).toBe("v1");
    });

    it("throws on duplicate id", () => {
      const reg = createSavedViewRegistry();
      reg.add(makeView("v1"));
      expect(() => reg.add(makeView("v1"))).toThrow("already exists");
    });

    it("returns undefined for unknown id", () => {
      const reg = createSavedViewRegistry();
      expect(reg.get("nope")).toBeUndefined();
    });

    it("removes a view and returns true", () => {
      const reg = createSavedViewRegistry();
      reg.add(makeView("v1"));
      expect(reg.remove("v1")).toBe(true);
      expect(reg.get("v1")).toBeUndefined();
    });

    it("returns false for removing unknown id", () => {
      const reg = createSavedViewRegistry();
      expect(reg.remove("nope")).toBe(false);
    });

    it("tracks size correctly", () => {
      const reg = createSavedViewRegistry();
      expect(reg.size).toBe(0);
      reg.add(makeView("v1"));
      reg.add(makeView("v2"));
      expect(reg.size).toBe(2);
      reg.remove("v1");
      expect(reg.size).toBe(1);
    });
  });

  describe("update", () => {
    it("updates name and description", () => {
      const reg = createSavedViewRegistry();
      reg.add(makeView("v1"));
      reg.update("v1", { name: "New Name", description: "A description" });
      const updated = reg.get("v1");
      expect(updated?.name).toBe("New Name");
      expect(updated?.description).toBe("A description");
    });

    it("throws for unknown id", () => {
      const reg = createSavedViewRegistry();
      expect(() => reg.update("nope", { name: "x" })).toThrow("not found");
    });

    it("preserves id and createdAt", () => {
      const reg = createSavedViewRegistry();
      const view = makeView("v1");
      reg.add(view);
      reg.update("v1", { name: "Changed" });
      const updated = reg.get("v1");
      expect(updated?.id).toBe("v1");
      expect(updated?.createdAt).toBe(view.createdAt);
    });

    it("bumps updatedAt on update", () => {
      vi.useFakeTimers();
      try {
        vi.setSystemTime(new Date("2026-01-01T00:00:00Z"));
        const reg = createSavedViewRegistry();
        reg.add(makeView("v1"));
        const before = reg.get("v1")?.updatedAt;
        vi.setSystemTime(new Date("2026-01-01T00:01:00Z"));
        reg.update("v1", { name: "Changed" });
        const after = reg.get("v1")?.updatedAt;
        expect(after).not.toBe(before);
      } finally {
        vi.useRealTimers();
      }
    });
  });

  describe("queries", () => {
    it("all() returns all views", () => {
      const reg = createSavedViewRegistry();
      reg.add(makeView("v1", "contact"));
      reg.add(makeView("v2", "task"));
      expect(reg.all()).toHaveLength(2);
    });

    it("forObjectType() filters by type", () => {
      const reg = createSavedViewRegistry();
      reg.add(makeView("v1", "contact"));
      reg.add(makeView("v2", "task"));
      reg.add(makeView("v3", "contact"));
      expect(reg.forObjectType("contact")).toHaveLength(2);
      expect(reg.forObjectType("task")).toHaveLength(1);
      expect(reg.forObjectType("invoice")).toHaveLength(0);
    });

    it("pinned() returns only pinned views", () => {
      const reg = createSavedViewRegistry();
      reg.add(makeView("v1"));
      reg.add(makeView("v2"));
      reg.pin("v1");
      expect(reg.pinned()).toHaveLength(1);
      expect(reg.pinned()[0]?.id).toBe("v1");
    });

    it("search() matches name case-insensitively", () => {
      const reg = createSavedViewRegistry();
      const v = makeView("v1");
      v.name = "Active Contacts";
      reg.add(v);
      reg.add(makeView("v2"));
      expect(reg.search("active")).toHaveLength(1);
      expect(reg.search("ACTIVE")).toHaveLength(1);
    });

    it("search() matches description", () => {
      const reg = createSavedViewRegistry();
      const v = makeView("v1");
      v.description = "Shows only active records";
      reg.add(v);
      expect(reg.search("active records")).toHaveLength(1);
    });
  });

  describe("pin", () => {
    it("toggles pin state", () => {
      const reg = createSavedViewRegistry();
      reg.add(makeView("v1"));
      expect(reg.get("v1")?.pinned).toBe(false);
      reg.pin("v1");
      expect(reg.get("v1")?.pinned).toBe(true);
      reg.pin("v1");
      expect(reg.get("v1")?.pinned).toBe(false);
    });

    it("throws for unknown id", () => {
      const reg = createSavedViewRegistry();
      expect(() => reg.pin("nope")).toThrow("not found");
    });
  });

  describe("subscribe", () => {
    it("notifies on add", () => {
      const reg = createSavedViewRegistry();
      const listener = vi.fn();
      reg.subscribe(listener);
      reg.add(makeView("v1"));
      expect(listener).toHaveBeenCalledTimes(1);
      expect(listener).toHaveBeenCalledWith([expect.objectContaining({ id: "v1" })]);
    });

    it("notifies on remove", () => {
      const reg = createSavedViewRegistry();
      reg.add(makeView("v1"));
      const listener = vi.fn();
      reg.subscribe(listener);
      reg.remove("v1");
      expect(listener).toHaveBeenCalledWith([]);
    });

    it("unsubscribe stops notifications", () => {
      const reg = createSavedViewRegistry();
      const listener = vi.fn();
      const unsub = reg.subscribe(listener);
      unsub();
      reg.add(makeView("v1"));
      expect(listener).not.toHaveBeenCalled();
    });
  });

  describe("serialize/load", () => {
    it("round-trips through serialize/load", () => {
      const reg = createSavedViewRegistry();
      reg.add(makeView("v1", "contact"));
      reg.add(makeView("v2", "task"));
      reg.pin("v1");

      const data = reg.serialize();
      const reg2 = createSavedViewRegistry();
      reg2.load(data);

      expect(reg2.size).toBe(2);
      expect(reg2.get("v1")?.pinned).toBe(true);
      expect(reg2.get("v2")?.objectType).toBe("task");
    });

    it("load replaces existing data", () => {
      const reg = createSavedViewRegistry();
      reg.add(makeView("old"));
      reg.load([makeView("new")]);
      expect(reg.size).toBe(1);
      expect(reg.get("old")).toBeUndefined();
      expect(reg.get("new")).toBeDefined();
    });
  });
});
