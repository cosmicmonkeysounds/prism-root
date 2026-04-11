import { describe, it, expect, vi } from "vitest";
import { createMachine } from "./machine.js";
import type { MachineDefinition } from "./machine.js";

type S = "idle" | "running" | "paused" | "done";
type E = "start" | "pause" | "resume" | "stop" | "reset";

const timerDef: MachineDefinition<S, E> = {
  initial: "idle",
  states: [
    { id: "idle" },
    { id: "running" },
    { id: "paused" },
    { id: "done", terminal: true },
  ],
  transitions: [
    { from: "idle", event: "start", to: "running" },
    { from: "running", event: "pause", to: "paused" },
    { from: "paused", event: "resume", to: "running" },
    { from: ["running", "paused"], event: "stop", to: "done" },
    { from: "*", event: "reset", to: "idle" },
  ],
};

const timerFactory = createMachine(timerDef);

describe("Machine", () => {
  describe("start/restore", () => {
    it("starts at initial state", () => {
      const m = timerFactory.start();
      expect(m.state).toBe("idle");
    });

    it("starts at custom initial state", () => {
      const m = timerFactory.start("paused");
      expect(m.state).toBe("paused");
    });

    it("restore does not fire onEnter", () => {
      const enterSpy = vi.fn();
      const def: MachineDefinition<"a" | "b", "go"> = {
        initial: "a",
        states: [{ id: "a", onEnter: enterSpy }, { id: "b" }],
        transitions: [{ from: "a", event: "go", to: "b" }],
      };
      const factory = createMachine(def);
      factory.restore("a");
      expect(enterSpy).not.toHaveBeenCalled();
    });

    it("start fires onEnter for initial state", () => {
      const enterSpy = vi.fn();
      const def: MachineDefinition<"a" | "b", "go"> = {
        initial: "a",
        states: [{ id: "a", onEnter: enterSpy }, { id: "b" }],
        transitions: [{ from: "a", event: "go", to: "b" }],
      };
      createMachine(def).start();
      expect(enterSpy).toHaveBeenCalledOnce();
    });
  });

  describe("send", () => {
    it("transitions on valid event", () => {
      const m = timerFactory.start();
      expect(m.send("start")).toBe(true);
      expect(m.state).toBe("running");
    });

    it("returns false on invalid event", () => {
      const m = timerFactory.start();
      expect(m.send("pause")).toBe(false);
      expect(m.state).toBe("idle");
    });

    it("supports array from", () => {
      const m = timerFactory.start();
      m.send("start");
      m.send("pause");
      expect(m.send("stop")).toBe(true);
      expect(m.state).toBe("done");
    });

    it("supports wildcard from", () => {
      const m = timerFactory.start();
      m.send("start");
      expect(m.send("reset")).toBe(true);
      expect(m.state).toBe("idle");
    });

    it("blocks transition from terminal state", () => {
      const m = timerFactory.start();
      m.send("start");
      m.send("stop");
      expect(m.state).toBe("done");
      expect(m.send("reset")).toBe(false);
      expect(m.state).toBe("done");
    });
  });

  describe("can", () => {
    it("returns true for available events", () => {
      const m = timerFactory.start();
      expect(m.can("start")).toBe(true);
      expect(m.can("pause")).toBe(false);
    });

    it("returns false from terminal state", () => {
      const m = timerFactory.start();
      m.send("start");
      m.send("stop");
      expect(m.can("reset")).toBe(false);
    });
  });

  describe("matches", () => {
    it("matches single state", () => {
      const m = timerFactory.start();
      expect(m.matches("idle")).toBe(true);
      expect(m.matches("running")).toBe(false);
    });

    it("matches array of states", () => {
      const m = timerFactory.start();
      expect(m.matches(["idle", "paused"])).toBe(true);
      expect(m.matches(["running", "done"])).toBe(false);
    });
  });

  describe("guards", () => {
    it("blocks transition when guard returns false", () => {
      let allow = false;
      const def: MachineDefinition<"a" | "b", "go"> = {
        initial: "a",
        states: [{ id: "a" }, { id: "b" }],
        transitions: [
          { from: "a", event: "go", to: "b", guard: () => allow },
        ],
      };
      const m = createMachine(def).start();
      expect(m.send("go")).toBe(false);
      expect(m.state).toBe("a");

      allow = true;
      expect(m.send("go")).toBe(true);
      expect(m.state).toBe("b");
    });

    it("can() respects guards", () => {
      let allow = false;
      const def: MachineDefinition<"a" | "b", "go"> = {
        initial: "a",
        states: [{ id: "a" }, { id: "b" }],
        transitions: [
          { from: "a", event: "go", to: "b", guard: () => allow },
        ],
      };
      const m = createMachine(def).start();
      expect(m.can("go")).toBe(false);
      allow = true;
      expect(m.can("go")).toBe(true);
    });
  });

  describe("lifecycle hooks", () => {
    it("fires onExit, action, onEnter in order", () => {
      const order: string[] = [];
      const def: MachineDefinition<"a" | "b", "go"> = {
        initial: "a",
        states: [
          { id: "a", onExit: () => order.push("exit-a") },
          { id: "b", onEnter: () => order.push("enter-b") },
        ],
        transitions: [
          {
            from: "a",
            event: "go",
            to: "b",
            action: () => order.push("action"),
          },
        ],
      };
      const m = createMachine(def).start();
      m.send("go");
      expect(order).toEqual(["exit-a", "action", "enter-b"]);
    });
  });

  describe("listeners", () => {
    it("notifies on transition", () => {
      const m = timerFactory.start();
      const events: [S, E][] = [];
      m.on((state, event) => events.push([state, event]));
      m.send("start");
      m.send("pause");
      expect(events).toEqual([
        ["running", "start"],
        ["paused", "pause"],
      ]);
    });

    it("unsubscribe stops notifications", () => {
      const m = timerFactory.start();
      let count = 0;
      const unsub = m.on(() => count++);
      m.send("start");
      expect(count).toBe(1);
      unsub();
      m.send("pause");
      expect(count).toBe(1);
    });
  });

  describe("toJSON", () => {
    it("serializes current state", () => {
      const m = timerFactory.start();
      m.send("start");
      expect(m.toJSON()).toEqual({ state: "running" });
    });
  });
});
