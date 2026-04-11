import { describe, it, expect, beforeEach } from "vitest";
import { createAtomStore } from "./atoms.js";
import type { AtomStore } from "./atoms.js";
import { objectId } from "@prism/core/object-model";

describe("AtomStore", () => {
  let store: AtomStore;

  beforeEach(() => {
    store = createAtomStore();
  });

  describe("initial state", () => {
    it("starts with null selectedId", () => {
      expect(store.getState().selectedId).toBeNull();
    });

    it("starts with empty selectionIds", () => {
      expect(store.getState().selectionIds).toEqual([]);
    });

    it("starts with null editingObjectId", () => {
      expect(store.getState().editingObjectId).toBeNull();
    });

    it("starts with null activePanel", () => {
      expect(store.getState().activePanel).toBeNull();
    });

    it("starts with empty searchQuery", () => {
      expect(store.getState().searchQuery).toBe("");
    });

    it("starts with null navigationTarget", () => {
      expect(store.getState().navigationTarget).toBeNull();
    });
  });

  describe("setSelectedId", () => {
    it("sets the selected ID", () => {
      const id = objectId("obj-1");
      store.getState().setSelectedId(id);
      expect(store.getState().selectedId).toBe(id);
    });

    it("clears with null", () => {
      store.getState().setSelectedId(objectId("obj-1"));
      store.getState().setSelectedId(null);
      expect(store.getState().selectedId).toBeNull();
    });
  });

  describe("setSelectionIds", () => {
    it("sets multiple selection IDs", () => {
      const ids = [objectId("a"), objectId("b"), objectId("c")];
      store.getState().setSelectionIds(ids);
      expect(store.getState().selectionIds).toEqual(ids);
    });
  });

  describe("setEditingObjectId", () => {
    it("sets editing object", () => {
      const id = objectId("edit-1");
      store.getState().setEditingObjectId(id);
      expect(store.getState().editingObjectId).toBe(id);
    });
  });

  describe("setActivePanel", () => {
    it("sets active panel name", () => {
      store.getState().setActivePanel("explorer");
      expect(store.getState().activePanel).toBe("explorer");
    });
  });

  describe("setSearchQuery", () => {
    it("sets search query", () => {
      store.getState().setSearchQuery("find this");
      expect(store.getState().searchQuery).toBe("find this");
    });
  });

  describe("setNavigationTarget", () => {
    it("sets navigation target", () => {
      store.getState().setNavigationTarget({ type: "object", id: "abc" });
      expect(store.getState().navigationTarget).toEqual({
        type: "object",
        id: "abc",
      });
    });
  });

  describe("reset", () => {
    it("resets all state to initial values", () => {
      store.getState().setSelectedId(objectId("x"));
      store.getState().setSearchQuery("test");
      store.getState().setActivePanel("panel");
      store.getState().reset();
      expect(store.getState().selectedId).toBeNull();
      expect(store.getState().searchQuery).toBe("");
      expect(store.getState().activePanel).toBeNull();
    });
  });

  describe("subscribe", () => {
    it("notifies on state change", () => {
      let called = 0;
      store.subscribe(() => called++);
      store.getState().setSelectedId(objectId("x"));
      expect(called).toBe(1);
    });
  });
});
