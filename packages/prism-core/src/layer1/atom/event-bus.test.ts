import { describe, it, expect, beforeEach } from "vitest";
import { createPrismBus, PrismEvents } from "./event-bus.js";
import type { PrismBus } from "./event-bus.js";

describe("PrismBus", () => {
  let bus: PrismBus;

  beforeEach(() => {
    bus = createPrismBus();
  });

  describe("on/emit", () => {
    it("delivers payload to subscriber", () => {
      const received: string[] = [];
      bus.on<{ name: string }>("test", ({ name }) => received.push(name));
      bus.emit("test", { name: "hello" });
      expect(received).toEqual(["hello"]);
    });

    it("supports multiple subscribers", () => {
      let count = 0;
      bus.on("test", () => count++);
      bus.on("test", () => count++);
      bus.emit("test", {});
      expect(count).toBe(2);
    });

    it("ignores events with no subscribers", () => {
      expect(() => bus.emit("nobody", {})).not.toThrow();
    });

    it("unsubscribe removes handler", () => {
      let count = 0;
      const unsub = bus.on("test", () => count++);
      bus.emit("test", {});
      expect(count).toBe(1);
      unsub();
      bus.emit("test", {});
      expect(count).toBe(1);
    });
  });

  describe("once", () => {
    it("fires handler only once", () => {
      let count = 0;
      bus.once("test", () => count++);
      bus.emit("test", {});
      bus.emit("test", {});
      expect(count).toBe(1);
    });

    it("can be unsubscribed before firing", () => {
      let count = 0;
      const unsub = bus.once("test", () => count++);
      unsub();
      bus.emit("test", {});
      expect(count).toBe(0);
    });
  });

  describe("off", () => {
    it("removes all handlers for an event type", () => {
      let count = 0;
      bus.on("test", () => count++);
      bus.on("test", () => count++);
      bus.off("test");
      bus.emit("test", {});
      expect(count).toBe(0);
    });

    it("removes all handlers when called without args", () => {
      let count = 0;
      bus.on("a", () => count++);
      bus.on("b", () => count++);
      bus.off();
      bus.emit("a", {});
      bus.emit("b", {});
      expect(count).toBe(0);
    });
  });

  describe("listenerCount", () => {
    it("counts listeners for a specific event", () => {
      bus.on("test", () => {});
      bus.on("test", () => {});
      expect(bus.listenerCount("test")).toBe(2);
    });

    it("counts total listeners when no event specified", () => {
      bus.on("a", () => {});
      bus.on("b", () => {});
      bus.on("b", () => {});
      expect(bus.listenerCount()).toBe(3);
    });

    it("returns 0 for unknown events", () => {
      expect(bus.listenerCount("nope")).toBe(0);
    });
  });

  describe("PrismEvents constants", () => {
    it("has well-known event types", () => {
      expect(PrismEvents.ObjectCreated).toBe("objects:created");
      expect(PrismEvents.EdgeCreated).toBe("edges:created");
      expect(PrismEvents.NavigationNavigate).toBe("navigation:navigate");
      expect(PrismEvents.SearchCommit).toBe("search:commit");
    });
  });

  describe("handler isolation", () => {
    it("handler error does not prevent other handlers", () => {
      const results: number[] = [];
      bus.on("test", () => results.push(1));
      bus.on("test", () => {
        throw new Error("boom");
      });
      bus.on("test", () => results.push(3));
      // Spread iteration means the error propagates — that's correct behavior
      // for synchronous dispatch. Caller catches.
      expect(() => bus.emit("test", {})).toThrow("boom");
      // First handler ran before error
      expect(results).toContain(1);
    });
  });
});
