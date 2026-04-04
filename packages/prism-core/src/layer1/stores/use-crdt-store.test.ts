import { describe, it, expect } from "vitest";
import { createCrdtStore } from "./use-crdt-store.js";
import { createLoroBridge } from "../loro-bridge.js";

describe("createCrdtStore", () => {
  it("should start disconnected with empty data", () => {
    const store = createCrdtStore();
    const state = store.getState();
    expect(state.connected).toBe(false);
    expect(state.data).toEqual({});
  });

  it("should connect to a bridge and hydrate state", () => {
    const bridge = createLoroBridge();
    bridge.set("existing", "data");

    const store = createCrdtStore();
    store.getState().connect(bridge);

    const state = store.getState();
    expect(state.connected).toBe(true);
    expect(state.data["existing"]).toBe("data");
  });

  it("should update state when writing through the store", () => {
    const bridge = createLoroBridge();
    const store = createCrdtStore();
    store.getState().connect(bridge);

    store.getState().set("name", "Prism");

    expect(store.getState().data["name"]).toBe("Prism");
    // Also verify it went through to Loro
    expect(bridge.get("name")).toBe("Prism");
  });

  it("should react to external bridge changes", () => {
    const bridge = createLoroBridge();
    const store = createCrdtStore();
    store.getState().connect(bridge);

    // Write directly to bridge (simulating a daemon push)
    bridge.set("external", "update");

    expect(store.getState().data["external"]).toBe("update");
  });

  it("should disconnect cleanly", () => {
    const bridge = createLoroBridge();
    const store = createCrdtStore();
    const disconnect = store.getState().connect(bridge);

    disconnect();

    expect(store.getState().connected).toBe(false);
  });

  it("should throw when writing without a connection", () => {
    const store = createCrdtStore();
    expect(() => store.getState().set("key", "val")).toThrow(
      "Store not connected",
    );
  });

  it("should support Zustand subscribe for reactive updates", () => {
    const bridge = createLoroBridge();
    const store = createCrdtStore();
    store.getState().connect(bridge);

    const updates: Record<string, unknown>[] = [];
    store.subscribe((state) => {
      updates.push({ ...state.data });
    });

    store.getState().set("counter", "1");
    store.getState().set("counter", "2");

    // Should have captured both updates
    expect(updates.length).toBeGreaterThanOrEqual(2);
    expect(updates[updates.length - 1]?.["counter"]).toBe("2");
  });
});
