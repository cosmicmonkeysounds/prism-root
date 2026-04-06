import { describe, it, expect, vi } from "vitest";
import { lensId } from "./lens-types.js";
import type { LensManifest } from "./lens-types.js";
import { createLensRegistry } from "./lens-registry.js";
import type { LensRegistryEvent } from "./lens-registry.js";

function makeManifest(id: string, category: "editor" | "visual" | "data" | "debug" | "custom" = "editor"): LensManifest {
  return {
    id: lensId(id),
    name: id.charAt(0).toUpperCase() + id.slice(1),
    icon: id,
    category,
    contributes: {
      views: [{ slot: "main" }],
      commands: [{ id: `switch-${id}`, name: `Switch to ${id}` }],
    },
  };
}

describe("LensRegistry", () => {
  it("registers and retrieves a manifest", () => {
    const reg = createLensRegistry();
    reg.register(makeManifest("editor"));
    expect(reg.get(lensId("editor"))?.name).toBe("Editor");
  });

  it("has() returns true for registered, false for missing", () => {
    const reg = createLensRegistry();
    reg.register(makeManifest("editor"));
    expect(reg.has(lensId("editor"))).toBe(true);
    expect(reg.has(lensId("missing"))).toBe(false);
  });

  it("allLenses() returns all registered manifests", () => {
    const reg = createLensRegistry();
    reg.register(makeManifest("a"));
    reg.register(makeManifest("b"));
    reg.register(makeManifest("c"));
    expect(reg.allLenses()).toHaveLength(3);
  });

  it("getByCategory() filters correctly", () => {
    const reg = createLensRegistry();
    reg.register(makeManifest("editor", "editor"));
    reg.register(makeManifest("graph", "visual"));
    reg.register(makeManifest("crdt", "debug"));
    expect(reg.getByCategory("editor")).toHaveLength(1);
    expect(reg.getByCategory("visual")).toHaveLength(1);
    expect(reg.getByCategory("data")).toHaveLength(0);
  });

  it("unregister() removes the manifest", () => {
    const reg = createLensRegistry();
    reg.register(makeManifest("editor"));
    reg.unregister(lensId("editor"));
    expect(reg.has(lensId("editor"))).toBe(false);
    expect(reg.allLenses()).toHaveLength(0);
  });

  it("register() returns an unregister function", () => {
    const reg = createLensRegistry();
    const unsub = reg.register(makeManifest("editor"));
    expect(reg.has(lensId("editor"))).toBe(true);
    unsub();
    expect(reg.has(lensId("editor"))).toBe(false);
  });

  it("subscribe() fires on register", () => {
    const reg = createLensRegistry();
    const events: LensRegistryEvent[] = [];
    reg.subscribe((e) => events.push(e));
    reg.register(makeManifest("editor"));
    expect(events).toHaveLength(2); // registered + change
    expect(events[0]?.type).toBe("registered");
    expect(events[1]?.type).toBe("change");
  });

  it("subscribe() fires on unregister", () => {
    const reg = createLensRegistry();
    reg.register(makeManifest("editor"));
    const events: LensRegistryEvent[] = [];
    reg.subscribe((e) => events.push(e));
    reg.unregister(lensId("editor"));
    expect(events[0]?.type).toBe("unregistered");
    expect(events[1]?.type).toBe("change");
  });

  it("subscribe() returns working unsubscribe", () => {
    const reg = createLensRegistry();
    const listener = vi.fn();
    const unsub = reg.subscribe(listener);
    unsub();
    reg.register(makeManifest("editor"));
    expect(listener).not.toHaveBeenCalled();
  });

  it("re-registering same id replaces the manifest", () => {
    const reg = createLensRegistry();
    reg.register(makeManifest("editor"));
    const updated = makeManifest("editor");
    updated.name = "Code Editor";
    reg.register(updated);
    expect(reg.get(lensId("editor"))?.name).toBe("Code Editor");
    expect(reg.allLenses()).toHaveLength(1);
  });

  it("empty registry returns empty arrays", () => {
    const reg = createLensRegistry();
    expect(reg.allLenses()).toEqual([]);
    expect(reg.getByCategory("editor")).toEqual([]);
    expect(reg.get(lensId("nope"))).toBeUndefined();
  });

  it("unregister on missing id is a no-op", () => {
    const reg = createLensRegistry();
    const listener = vi.fn();
    reg.subscribe(listener);
    reg.unregister(lensId("nope"));
    expect(listener).not.toHaveBeenCalled();
  });
});
