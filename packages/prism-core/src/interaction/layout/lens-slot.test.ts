import { describe, it, expect } from "vitest";
import { LensSlot } from "./lens-slot.js";
import { PageRegistry } from "./page-registry.js";
import type { LensSlotEvent } from "./layout-types.js";

type TestTarget = { kind: string; id: string };

function makeSlot(opts: { cacheSize?: number } = {}) {
  const registry = new PageRegistry<TestTarget>().register("object", {
    defaultViewMode: "list",
    defaultTab: "overview",
    getObjectId: (t) => t.id,
  });
  return new LensSlot<TestTarget>({
    id: "main",
    registry,
    initialTarget: { kind: "object", id: "a" },
    ...opts,
  });
}

describe("LensSlot", () => {
  it("has initial target and page", () => {
    const slot = makeSlot();
    expect(slot.current).toEqual({ kind: "object", id: "a" });
    expect(slot.activePage.objectId).toBe("a");
  });

  it("go navigates to new target", () => {
    const slot = makeSlot();
    slot.go({ kind: "object", id: "b" });
    expect(slot.current).toEqual({ kind: "object", id: "b" });
    expect(slot.activePage.objectId).toBe("b");
  });

  it("go pushes to back stack", () => {
    const slot = makeSlot();
    slot.go({ kind: "object", id: "b" });
    expect(slot.canGoBack).toBe(true);
  });

  it("go clears forward stack", () => {
    const slot = makeSlot();
    slot.go({ kind: "object", id: "b" });
    slot.back();
    expect(slot.canGoForward).toBe(true);
    slot.go({ kind: "object", id: "c" });
    expect(slot.canGoForward).toBe(false);
  });

  it("back restores previous target", () => {
    const slot = makeSlot();
    slot.go({ kind: "object", id: "b" });
    const result = slot.back();
    expect(result).toBe(true);
    expect(slot.current).toEqual({ kind: "object", id: "a" });
  });

  it("back returns false when empty", () => {
    const slot = makeSlot();
    expect(slot.back()).toBe(false);
    expect(slot.canGoBack).toBe(false);
  });

  it("forward restores next target", () => {
    const slot = makeSlot();
    slot.go({ kind: "object", id: "b" });
    slot.back();
    const result = slot.forward();
    expect(result).toBe(true);
    expect(slot.current).toEqual({ kind: "object", id: "b" });
  });

  it("forward returns false when empty", () => {
    const slot = makeSlot();
    expect(slot.forward()).toBe(false);
    expect(slot.canGoForward).toBe(false);
  });

  it("reuses cached page on revisit", () => {
    const slot = makeSlot();
    const pageA = slot.activePage;
    slot.go({ kind: "object", id: "b" });
    slot.back();
    expect(slot.activePage).toBe(pageA);
  });

  it("evicts oldest page when cache full", () => {
    const slot = makeSlot({ cacheSize: 2 });
    const pageA = slot.activePage;
    slot.go({ kind: "object", id: "b" });
    slot.go({ kind: "object", id: "c" });
    // Cache has b, c (a was evicted). Navigate back to a — should get new page.
    slot.back();
    slot.back();
    expect(slot.activePage).not.toBe(pageA);
    expect(pageA.isDisposed).toBe(true);
  });

  it("emits navigated event on go", () => {
    const slot = makeSlot();
    const events: LensSlotEvent<TestTarget>[] = [];
    slot.on((e) => events.push(e));
    slot.go({ kind: "object", id: "b" });
    expect(events).toHaveLength(1);
    expect(events[0]?.kind).toBe("navigated");
  });

  it("emits back/forward events", () => {
    const slot = makeSlot();
    slot.go({ kind: "object", id: "b" });
    const events: LensSlotEvent<TestTarget>[] = [];
    slot.on((e) => events.push(e));
    slot.back();
    slot.forward();
    expect(events.map((e) => e.kind)).toEqual(["back", "forward"]);
  });

  it("dispose clears everything", () => {
    const slot = makeSlot();
    slot.go({ kind: "object", id: "b" });
    const page = slot.activePage;
    slot.dispose();
    expect(page.isDisposed).toBe(true);
    expect(slot.canGoBack).toBe(false);
    expect(slot.canGoForward).toBe(false);
  });

  it("persistPages serializes cached pages", () => {
    const slot = makeSlot();
    slot.go({ kind: "object", id: "b" });
    const pages = slot.persistPages();
    expect(pages.length).toBeGreaterThanOrEqual(2);
    expect(pages.some((p) => p.objectId === "a")).toBe(true);
    expect(pages.some((p) => p.objectId === "b")).toBe(true);
  });
});
