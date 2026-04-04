import { describe, it, expect, vi } from "vitest";
import { createLoroBridge } from "./loro-bridge.js";

describe("createLoroBridge", () => {
  it("should set and get values on the root map", () => {
    const bridge = createLoroBridge();
    bridge.set("greeting", "hello");
    expect(bridge.get("greeting")).toBe("hello");
  });

  it("should return undefined for missing keys", () => {
    const bridge = createLoroBridge();
    expect(bridge.get("nonexistent")).toBeUndefined();
  });

  it("should delete keys", () => {
    const bridge = createLoroBridge();
    bridge.set("temp", "value");
    expect(bridge.get("temp")).toBe("value");
    bridge.delete("temp");
    expect(bridge.get("temp")).toBeUndefined();
  });

  it("should export and import snapshots", () => {
    const bridge1 = createLoroBridge(1n);
    bridge1.set("key1", "value1");
    bridge1.set("key2", "value2");

    const snapshot = bridge1.exportSnapshot();
    expect(snapshot).toBeInstanceOf(Uint8Array);
    expect(snapshot.length).toBeGreaterThan(0);

    const bridge2 = createLoroBridge(2n);
    bridge2.import(snapshot);
    expect(bridge2.get("key1")).toBe("value1");
    expect(bridge2.get("key2")).toBe("value2");
  });

  it("should merge state from two independent peers", () => {
    const peerA = createLoroBridge(1n);
    const peerB = createLoroBridge(2n);

    peerA.set("from_a", "hello");
    peerB.set("from_b", "world");

    const snapA = peerA.exportSnapshot();
    const snapB = peerB.exportSnapshot();

    // Cross-import
    peerA.import(snapB);
    peerB.import(snapA);

    // Both peers should have both keys
    expect(peerA.get("from_a")).toBe("hello");
    expect(peerA.get("from_b")).toBe("world");
    expect(peerB.get("from_a")).toBe("hello");
    expect(peerB.get("from_b")).toBe("world");
  });

  it("should resolve concurrent edits to the same key", () => {
    const peerA = createLoroBridge(1n);
    const peerB = createLoroBridge(2n);

    peerA.set("conflict", "version_a");
    peerB.set("conflict", "version_b");

    const snapA = peerA.exportSnapshot();
    const snapB = peerB.exportSnapshot();

    peerA.import(snapB);
    peerB.import(snapA);

    // Both peers converge to the same value (CRDT deterministic merge)
    expect(peerA.get("conflict")).toBe(peerB.get("conflict"));
  });

  it("should notify onChange listeners", () => {
    const bridge = createLoroBridge();
    const handler = vi.fn();

    bridge.onChange(handler);
    bridge.set("observed", "test");

    // Loro subscription fires synchronously on commit
    expect(handler).toHaveBeenCalledWith("observed", "test");
  });

  it("should allow unsubscribing from onChange", () => {
    const bridge = createLoroBridge();
    const handler = vi.fn();

    const unsub = bridge.onChange(handler);
    bridge.set("key1", "val1");
    expect(handler).toHaveBeenCalledTimes(1);

    unsub();
    bridge.set("key2", "val2");
    expect(handler).toHaveBeenCalledTimes(1); // no additional call
  });

  it("should serialize to JSON", () => {
    const bridge = createLoroBridge();
    bridge.set("a", "1");
    bridge.set("b", "2");

    const json = bridge.toJSON();
    expect(json).toEqual({ a: "1", b: "2" });
  });
});
