import { describe, it, expect, beforeEach } from "vitest";
import { lensId } from "./lens-types.js";
import { createShellStore } from "./workspace-store.js";
import type { ShellStore } from "./workspace-store.js";
import type { StoreApi } from "zustand";

describe("ShellStore", () => {
  let store: StoreApi<ShellStore>;

  beforeEach(() => {
    store = createShellStore();
  });

  it("starts with empty tabs and null activeTabId", () => {
    const s = store.getState();
    expect(s.tabs).toEqual([]);
    expect(s.activeTabId).toBeNull();
  });

  it("starts with default panel layout", () => {
    const s = store.getState();
    expect(s.panelLayout.sidebar).toBe(true);
    expect(s.panelLayout.inspector).toBe(false);
    expect(s.panelLayout.sidebarWidth).toBe(20);
    expect(s.panelLayout.inspectorWidth).toBe(25);
  });

  it("openTab() creates a tab and sets it active", () => {
    const id = store.getState().openTab(lensId("editor"), "Editor");
    const s = store.getState();
    expect(s.tabs).toHaveLength(1);
    expect(s.tabs[0]?.lensId).toBe("editor");
    expect(s.tabs[0]?.label).toBe("Editor");
    expect(s.activeTabId).toBe(id);
  });

  it("openTab() with existing lensId focuses existing tab (singleton)", () => {
    const id1 = store.getState().openTab(lensId("editor"), "Editor");
    const id2 = store.getState().openTab(lensId("editor"), "Editor");
    expect(id1).toBe(id2);
    expect(store.getState().tabs).toHaveLength(1);
  });

  it("openTab() creates new tab when existing is pinned", () => {
    const id1 = store.getState().openTab(lensId("editor"), "Editor");
    store.getState().pinTab(id1);
    const id2 = store.getState().openTab(lensId("editor"), "Editor");
    expect(id1).not.toBe(id2);
    expect(store.getState().tabs).toHaveLength(2);
  });

  it("closeTab() removes tab and activates adjacent", () => {
    store.getState().openTab(lensId("a"), "A");
    const idB = store.getState().openTab(lensId("b"), "B");
    store.getState().openTab(lensId("c"), "C");
    store.getState().setActiveTab(idB);
    store.getState().closeTab(idB);
    const s = store.getState();
    expect(s.tabs).toHaveLength(2);
    expect(s.activeTabId).not.toBeNull();
    expect(s.activeTabId).not.toBe(idB);
  });

  it("closeTab() on last tab sets activeTabId to null", () => {
    const id = store.getState().openTab(lensId("editor"), "Editor");
    store.getState().closeTab(id);
    expect(store.getState().tabs).toHaveLength(0);
    expect(store.getState().activeTabId).toBeNull();
  });

  it("pinTab() and unpinTab() toggle pinned flag", () => {
    const id = store.getState().openTab(lensId("editor"), "Editor");
    expect(store.getState().tabs[0]?.pinned).toBe(false);
    store.getState().pinTab(id);
    expect(store.getState().tabs[0]?.pinned).toBe(true);
    store.getState().unpinTab(id);
    expect(store.getState().tabs[0]?.pinned).toBe(false);
  });

  it("reorderTab() updates order value", () => {
    const id = store.getState().openTab(lensId("editor"), "Editor");
    store.getState().reorderTab(id, 99);
    expect(store.getState().tabs[0]?.order).toBe(99);
  });

  it("setActiveTab() switches active tab", () => {
    const idA = store.getState().openTab(lensId("a"), "A");
    store.getState().openTab(lensId("b"), "B");
    store.getState().setActiveTab(idA);
    expect(store.getState().activeTabId).toBe(idA);
  });

  it("setActiveTab() ignores unknown id", () => {
    const id = store.getState().openTab(lensId("a"), "A");
    store.getState().setActiveTab("nonexistent" as string);
    expect(store.getState().activeTabId).toBe(id);
  });

  it("toggleSidebar() toggles sidebar visibility", () => {
    expect(store.getState().panelLayout.sidebar).toBe(true);
    store.getState().toggleSidebar();
    expect(store.getState().panelLayout.sidebar).toBe(false);
    store.getState().toggleSidebar();
    expect(store.getState().panelLayout.sidebar).toBe(true);
  });

  it("toggleInspector() toggles inspector visibility", () => {
    expect(store.getState().panelLayout.inspector).toBe(false);
    store.getState().toggleInspector();
    expect(store.getState().panelLayout.inspector).toBe(true);
  });

  it("setSidebarWidth() clamps to [10, 50]", () => {
    store.getState().setSidebarWidth(5);
    expect(store.getState().panelLayout.sidebarWidth).toBe(10);
    store.getState().setSidebarWidth(60);
    expect(store.getState().panelLayout.sidebarWidth).toBe(50);
    store.getState().setSidebarWidth(30);
    expect(store.getState().panelLayout.sidebarWidth).toBe(30);
  });

  it("setInspectorWidth() clamps to [10, 50]", () => {
    store.getState().setInspectorWidth(3);
    expect(store.getState().panelLayout.inspectorWidth).toBe(10);
    store.getState().setInspectorWidth(55);
    expect(store.getState().panelLayout.inspectorWidth).toBe(50);
  });

  it("generates unique tab IDs across stores", () => {
    const store2 = createShellStore();
    const id1 = store.getState().openTab(lensId("a"), "A");
    const id2 = store2.getState().openTab(lensId("b"), "B");
    expect(id1).not.toBe(id2);
  });
});
