import { describe, it, expect, vi } from "vitest";
import { createActionRegistry } from "./focus-depth.js";

describe("createActionRegistry", () => {
  it("should return empty actions when nothing registered", () => {
    const registry = createActionRegistry();
    expect(registry.getActions("global")).toEqual([]);
  });

  it("should register and retrieve global actions", () => {
    const registry = createActionRegistry();
    registry.register("global", [
      { id: "open", name: "Open File" },
      { id: "quit", name: "Quit" },
    ]);

    const actions = registry.getActions("global");
    expect(actions).toHaveLength(2);
    expect(actions[0]?.name).toBe("Open File");
  });

  it("should include shallower depths when querying deeper", () => {
    const registry = createActionRegistry();
    registry.register("global", [{ id: "g1", name: "Global Action" }]);
    registry.register("app", [{ id: "a1", name: "App Action" }]);
    registry.register("plugin", [{ id: "p1", name: "Plugin Action" }]);

    // At app depth, see global + app
    const appActions = registry.getActions("app");
    expect(appActions).toHaveLength(2);

    // At plugin depth, see global + app + plugin
    const pluginActions = registry.getActions("plugin");
    expect(pluginActions).toHaveLength(3);

    // At cursor depth, see all four levels
    registry.register("cursor", [{ id: "c1", name: "Cursor Action" }]);
    const cursorActions = registry.getActions("cursor");
    expect(cursorActions).toHaveLength(4);
  });

  it("should NOT include deeper depth actions when at shallower depth", () => {
    const registry = createActionRegistry();
    registry.register("global", [{ id: "g1", name: "Global" }]);
    registry.register("cursor", [{ id: "c1", name: "Cursor" }]);

    // At global depth, should NOT see cursor actions
    const actions = registry.getActions("global");
    expect(actions).toHaveLength(1);
    expect(actions[0]?.id).toBe("g1");
  });

  it("should unregister actions", () => {
    const registry = createActionRegistry();
    const unsub = registry.register("global", [
      { id: "temp", name: "Temporary" },
    ]);

    expect(registry.getActions("global")).toHaveLength(1);

    unsub();
    expect(registry.getActions("global")).toHaveLength(0);
  });

  it("should notify subscribers on registration changes", () => {
    const registry = createActionRegistry();
    const callback = vi.fn();

    registry.subscribe(callback);
    registry.register("global", [{ id: "new", name: "New Action" }]);

    expect(callback).toHaveBeenCalledTimes(1);
  });

  it("should notify subscribers on unregistration", () => {
    const registry = createActionRegistry();
    const callback = vi.fn();

    const unsub = registry.register("app", [{ id: "a1", name: "App" }]);
    registry.subscribe(callback);
    unsub();

    expect(callback).toHaveBeenCalledTimes(1);
  });

  it("should allow unsubscribing from notifications", () => {
    const registry = createActionRegistry();
    const callback = vi.fn();

    const unsub = registry.subscribe(callback);
    registry.register("global", [{ id: "g1", name: "G1" }]);
    expect(callback).toHaveBeenCalledTimes(1);

    unsub();
    registry.register("global", [{ id: "g2", name: "G2" }]);
    expect(callback).toHaveBeenCalledTimes(1);
  });

  it("should deduplicate actions by ID within a depth", () => {
    const registry = createActionRegistry();
    registry.register("global", [{ id: "dup", name: "First" }]);
    registry.register("global", [{ id: "dup", name: "Second" }]);

    const actions = registry.getActions("global");
    expect(actions).toHaveLength(1);
    expect(actions[0]?.name).toBe("Second"); // latest wins
  });
});
