import { describe, it, expect, vi } from "vitest";
import { PageModel } from "./page-model.js";
import type { PageModelEvent } from "./layout-types.js";

function makePage() {
  return new PageModel({
    id: "page-1",
    target: { kind: "object", id: "task-1" },
    objectId: "task-1",
    defaultViewMode: "list",
    defaultTab: "overview",
  });
}

describe("PageModel", () => {
  it("sets all fields from constructor", () => {
    const page = makePage();
    expect(page.id).toBe("page-1");
    expect(page.target).toEqual({ kind: "object", id: "task-1" });
    expect(page.objectId).toBe("task-1");
    expect(page.viewMode).toBe("list");
    expect(page.activeTab).toBe("overview");
  });

  it("derives inputScopeId from id", () => {
    const page = makePage();
    expect(page.inputScopeId).toBe("page:page-1");
  });

  it("setViewMode updates and emits", () => {
    const page = makePage();
    const events: PageModelEvent[] = [];
    page.on((e) => events.push(e));
    page.setViewMode("kanban");
    expect(page.viewMode).toBe("kanban");
    expect(events).toEqual([{ kind: "viewMode", mode: "kanban" }]);
  });

  it("setViewMode no-ops for same value", () => {
    const page = makePage();
    const fn = vi.fn();
    page.on(fn);
    page.setViewMode("list");
    expect(fn).not.toHaveBeenCalled();
  });

  it("setTab updates and emits", () => {
    const page = makePage();
    const events: PageModelEvent[] = [];
    page.on((e) => events.push(e));
    page.setTab("details");
    expect(page.activeTab).toBe("details");
    expect(events).toEqual([{ kind: "tab", tab: "details" }]);
  });

  it("setTab no-ops for same value", () => {
    const page = makePage();
    const fn = vi.fn();
    page.on(fn);
    page.setTab("overview");
    expect(fn).not.toHaveBeenCalled();
  });

  it("selection is scoped", () => {
    const page = makePage();
    page.selection.select("a");
    page.selection.toggle("b");
    expect(page.selection.size).toBe(2);
  });

  it("dispose marks disposed and emits", () => {
    const page = makePage();
    const events: PageModelEvent[] = [];
    page.on((e) => events.push(e));
    page.dispose();
    expect(page.isDisposed).toBe(true);
    expect(events).toEqual([{ kind: "disposed" }]);
  });

  it("mutations no-op after dispose", () => {
    const page = makePage();
    page.dispose();
    const fn = vi.fn();
    page.on(fn);
    page.setViewMode("kanban");
    page.setTab("details");
    expect(fn).not.toHaveBeenCalled();
    expect(page.viewMode).toBe("list");
  });

  it("persist returns serializable snapshot", () => {
    const page = makePage();
    page.setViewMode("kanban");
    page.selection.select("item-1");
    const data = page.persist();
    expect(data).toEqual({
      id: "page-1",
      target: { kind: "object", id: "task-1" },
      objectId: "task-1",
      viewMode: "kanban",
      activeTab: "overview",
      selectedIds: ["item-1"],
    });
  });

  it("fromSerialized restores page", () => {
    const page = makePage();
    page.setViewMode("kanban");
    page.selection.selectAll(["a", "b"]);
    const data = page.persist();
    const restored = PageModel.fromSerialized(data, {
      defaultViewMode: "list",
      defaultTab: "overview",
    });
    expect(restored.viewMode).toBe("kanban");
    expect(restored.selection.selectedIds).toEqual(["a", "b"]);
    expect(restored.objectId).toBe("task-1");
  });

  it("unsubscribe removes listener", () => {
    const page = makePage();
    const fn = vi.fn();
    const unsub = page.on(fn);
    page.setViewMode("kanban");
    unsub();
    page.setViewMode("board");
    expect(fn).toHaveBeenCalledOnce();
  });
});
