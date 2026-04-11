import { describe, it, expect, vi } from "vitest";
import { SelectionModel } from "./selection-model.js";

describe("SelectionModel", () => {
  it("starts empty", () => {
    const sel = new SelectionModel();
    expect(sel.isEmpty).toBe(true);
    expect(sel.size).toBe(0);
    expect(sel.primary).toBeNull();
  });

  it("select replaces selection", () => {
    const sel = new SelectionModel();
    sel.select("a");
    expect(sel.isSelected("a")).toBe(true);
    expect(sel.primary).toBe("a");
    sel.select("b");
    expect(sel.isSelected("a")).toBe(false);
    expect(sel.isSelected("b")).toBe(true);
    expect(sel.primary).toBe("b");
  });

  it("toggle adds item", () => {
    const sel = new SelectionModel();
    sel.select("a");
    sel.toggle("b");
    expect(sel.isSelected("a")).toBe(true);
    expect(sel.isSelected("b")).toBe(true);
    expect(sel.hasMultiple).toBe(true);
    expect(sel.primary).toBe("b");
  });

  it("toggle removes item", () => {
    const sel = new SelectionModel();
    sel.select("a");
    sel.toggle("b");
    sel.toggle("b");
    expect(sel.isSelected("b")).toBe(false);
    expect(sel.primary).toBe("a");
  });

  it("toggle updates primary to last when removing", () => {
    const sel = new SelectionModel();
    sel.selectAll(["a", "b", "c"]);
    sel.toggle("c");
    expect(sel.primary).toBe("b");
  });

  it("selectRange forward", () => {
    const sel = new SelectionModel();
    sel.selectRange(["a", "b", "c", "d"], "b", "d");
    expect(sel.selectedIds).toContain("b");
    expect(sel.selectedIds).toContain("c");
    expect(sel.selectedIds).toContain("d");
    expect(sel.primary).toBe("d");
  });

  it("selectRange reverse", () => {
    const sel = new SelectionModel();
    sel.selectRange(["a", "b", "c", "d"], "d", "b");
    expect(sel.size).toBe(3);
    expect(sel.primary).toBe("b");
  });

  it("selectRange no-ops for invalid ids", () => {
    const sel = new SelectionModel();
    sel.selectRange(["a", "b"], "x", "y");
    expect(sel.isEmpty).toBe(true);
  });

  it("selectAll selects all ids", () => {
    const sel = new SelectionModel();
    sel.selectAll(["a", "b", "c"]);
    expect(sel.size).toBe(3);
    expect(sel.primary).toBe("c");
  });

  it("clear empties selection", () => {
    const sel = new SelectionModel();
    sel.selectAll(["a", "b"]);
    sel.clear();
    expect(sel.isEmpty).toBe(true);
    expect(sel.primary).toBeNull();
  });

  it("emits events on mutation", () => {
    const sel = new SelectionModel();
    const fn = vi.fn();
    sel.on(fn);
    sel.select("a");
    expect(fn).toHaveBeenCalledOnce();
    expect(fn).toHaveBeenCalledWith({ selected: expect.any(Set), primary: "a" });
  });

  it("unsubscribe stops events", () => {
    const sel = new SelectionModel();
    const fn = vi.fn();
    const unsub = sel.on(fn);
    sel.select("a");
    unsub();
    sel.select("b");
    expect(fn).toHaveBeenCalledOnce();
  });
});
