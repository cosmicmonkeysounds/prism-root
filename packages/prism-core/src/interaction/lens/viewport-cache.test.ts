import { describe, it, expect, beforeEach } from "vitest";
import { createViewportCache } from "./viewport-cache.js";
import type { ViewportCache } from "./viewport-cache.js";
import type { StoreApi } from "zustand";

describe("ViewportCache", () => {
  let store: StoreApi<ViewportCache>;

  beforeEach(() => {
    store = createViewportCache();
  });

  it("starts with an empty viewports map", () => {
    expect(store.getState().viewports).toEqual({});
  });

  it("get() returns undefined for unknown key", () => {
    expect(store.getState().get("nope")).toBeUndefined();
  });

  it("set() stores a viewport and get() reads it back", () => {
    store.getState().set("graph", { x: 100, y: -50, zoom: 1.5 });
    expect(store.getState().get("graph")).toEqual({ x: 100, y: -50, zoom: 1.5 });
  });

  it("set() replaces an existing entry", () => {
    store.getState().set("graph", { x: 0, y: 0, zoom: 1 });
    store.getState().set("graph", { x: 200, y: 200, zoom: 2 });
    expect(store.getState().get("graph")).toEqual({ x: 200, y: 200, zoom: 2 });
  });

  it("set() preserves entries under other keys", () => {
    store.getState().set("graph", { x: 1, y: 1, zoom: 1 });
    store.getState().set("sitemap", { x: 2, y: 2, zoom: 2 });
    expect(store.getState().get("graph")).toEqual({ x: 1, y: 1, zoom: 1 });
    expect(store.getState().get("sitemap")).toEqual({ x: 2, y: 2, zoom: 2 });
  });

  it("clear() removes a single entry", () => {
    store.getState().set("graph", { x: 1, y: 1, zoom: 1 });
    store.getState().set("sitemap", { x: 2, y: 2, zoom: 2 });
    store.getState().clear("graph");
    expect(store.getState().get("graph")).toBeUndefined();
    expect(store.getState().get("sitemap")).toEqual({ x: 2, y: 2, zoom: 2 });
  });

  it("clear() on missing key is a no-op", () => {
    store.getState().set("graph", { x: 1, y: 1, zoom: 1 });
    store.getState().clear("nope");
    expect(store.getState().get("graph")).toEqual({ x: 1, y: 1, zoom: 1 });
  });

  it("clearAll() empties the cache", () => {
    store.getState().set("graph", { x: 1, y: 1, zoom: 1 });
    store.getState().set("sitemap", { x: 2, y: 2, zoom: 2 });
    store.getState().clearAll();
    expect(store.getState().viewports).toEqual({});
  });

  it("notifies subscribers on set", () => {
    let calls = 0;
    const unsub = store.subscribe(() => {
      calls++;
    });
    store.getState().set("graph", { x: 1, y: 1, zoom: 1 });
    store.getState().set("graph", { x: 2, y: 2, zoom: 2 });
    unsub();
    expect(calls).toBe(2);
  });

  it("notifies subscribers on clear", () => {
    store.getState().set("graph", { x: 1, y: 1, zoom: 1 });
    let calls = 0;
    const unsub = store.subscribe(() => {
      calls++;
    });
    store.getState().clear("graph");
    unsub();
    expect(calls).toBe(1);
  });
});
