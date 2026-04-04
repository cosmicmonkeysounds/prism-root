import { describe, it, expect, vi } from "vitest";
import { WorkspaceManager } from "./workspace-manager.js";
import { PageRegistry } from "./page-registry.js";
import type { WorkspaceManagerEvent } from "./layout-types.js";

type TestTarget = { kind: string; id: string };

function makeRegistry() {
  return new PageRegistry<TestTarget>().register("object", {
    defaultViewMode: "list",
    defaultTab: "overview",
    getObjectId: (t) => t.id,
  });
}

describe("WorkspaceManager", () => {
  it("opens a slot and auto-focuses it", () => {
    const mgr = new WorkspaceManager<TestTarget>();
    const slot = mgr.open("main", makeRegistry(), { kind: "object", id: "a" });
    expect(slot.id).toBe("main");
    expect(mgr.activeSlot).toBe(slot);
    expect(mgr.slotCount).toBe(1);
  });

  it("returns existing slot if id taken", () => {
    const mgr = new WorkspaceManager<TestTarget>();
    const slot1 = mgr.open("main", makeRegistry(), { kind: "object", id: "a" });
    const slot2 = mgr.open("main", makeRegistry(), { kind: "object", id: "b" });
    expect(slot1).toBe(slot2);
    expect(mgr.slotCount).toBe(1);
  });

  it("close disposes slot", () => {
    const mgr = new WorkspaceManager<TestTarget>();
    const slot = mgr.open("main", makeRegistry(), { kind: "object", id: "a" });
    const page = slot.activePage;
    mgr.close("main");
    expect(mgr.slotCount).toBe(0);
    expect(page.isDisposed).toBe(true);
  });

  it("close refocuses last remaining slot", () => {
    const mgr = new WorkspaceManager<TestTarget>();
    mgr.open("a", makeRegistry(), { kind: "object", id: "1" });
    mgr.open("b", makeRegistry(), { kind: "object", id: "2" });
    expect(mgr.activeSlot?.id).toBe("b");
    mgr.close("b");
    expect(mgr.activeSlot?.id).toBe("a");
  });

  it("close sets active to null when last slot removed", () => {
    const mgr = new WorkspaceManager<TestTarget>();
    mgr.open("a", makeRegistry(), { kind: "object", id: "1" });
    mgr.close("a");
    expect(mgr.activeSlot).toBeNull();
    expect(mgr.activePage).toBeNull();
  });

  it("focus switches active slot", () => {
    const mgr = new WorkspaceManager<TestTarget>();
    mgr.open("a", makeRegistry(), { kind: "object", id: "1" });
    mgr.open("b", makeRegistry(), { kind: "object", id: "2" });
    mgr.focus("a");
    expect(mgr.activeSlot?.id).toBe("a");
  });

  it("activePage reads through to active slot", () => {
    const mgr = new WorkspaceManager<TestTarget>();
    mgr.open("main", makeRegistry(), { kind: "object", id: "a" });
    expect(mgr.activePage?.objectId).toBe("a");
  });

  it("getSlot finds by id", () => {
    const mgr = new WorkspaceManager<TestTarget>();
    const slot = mgr.open("main", makeRegistry(), { kind: "object", id: "a" });
    expect(mgr.getSlot("main")).toBe(slot);
    expect(mgr.getSlot("missing")).toBeUndefined();
  });

  it("allSlots returns all slots", () => {
    const mgr = new WorkspaceManager<TestTarget>();
    mgr.open("a", makeRegistry(), { kind: "object", id: "1" });
    mgr.open("b", makeRegistry(), { kind: "object", id: "2" });
    expect(mgr.allSlots).toHaveLength(2);
  });

  it("emits slot-opened event", () => {
    const mgr = new WorkspaceManager<TestTarget>();
    const events: WorkspaceManagerEvent<TestTarget>[] = [];
    mgr.on((e) => events.push(e));
    mgr.open("main", makeRegistry(), { kind: "object", id: "a" });
    expect(events[0]).toEqual({ kind: "slot-opened", slotId: "main" });
  });

  it("emits slot-closed event", () => {
    const mgr = new WorkspaceManager<TestTarget>();
    mgr.open("main", makeRegistry(), { kind: "object", id: "a" });
    const events: WorkspaceManagerEvent<TestTarget>[] = [];
    mgr.on((e) => events.push(e));
    mgr.close("main");
    expect(events).toContainEqual({ kind: "slot-closed", slotId: "main" });
  });

  it("emits slot-focused event", () => {
    const mgr = new WorkspaceManager<TestTarget>();
    mgr.open("a", makeRegistry(), { kind: "object", id: "1" });
    mgr.open("b", makeRegistry(), { kind: "object", id: "2" });
    const events: WorkspaceManagerEvent<TestTarget>[] = [];
    mgr.on((e) => events.push(e));
    mgr.focus("a");
    expect(events).toContainEqual({ kind: "slot-focused", slotId: "a" });
  });

  it("dispose clears everything", () => {
    const mgr = new WorkspaceManager<TestTarget>();
    mgr.open("a", makeRegistry(), { kind: "object", id: "1" });
    mgr.open("b", makeRegistry(), { kind: "object", id: "2" });
    mgr.dispose();
    expect(mgr.slotCount).toBe(0);
    expect(mgr.activeSlot).toBeNull();
  });

  it("unsubscribe stops events", () => {
    const mgr = new WorkspaceManager<TestTarget>();
    const events: WorkspaceManagerEvent<TestTarget>[] = [];
    const unsub = mgr.on((e) => events.push(e));
    mgr.open("a", makeRegistry(), { kind: "object", id: "1" });
    unsub();
    mgr.open("b", makeRegistry(), { kind: "object", id: "2" });
    // Only events from first open
    expect(events.filter((e) => e.kind === "slot-opened")).toHaveLength(1);
  });
});
