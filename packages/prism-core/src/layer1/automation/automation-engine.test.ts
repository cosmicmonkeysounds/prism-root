import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { AutomationEngine } from "./automation-engine.js";
import type { AutomationStore } from "./automation-engine.js";
import type {
  Automation,
  AutomationAction,
  AutomationContext,
  AutomationRun,
  ActionHandlerMap,
} from "./automation-types.js";

// ── Test helpers ────────────────────────────────────────────────────────────

function makeAutomation(overrides: Partial<Automation> = {}): Automation {
  return {
    id: "auto-1",
    name: "Test Automation",
    enabled: true,
    trigger: { type: "object:created" },
    conditions: [],
    actions: [],
    createdAt: "2026-01-01T00:00:00Z",
    updatedAt: "2026-01-01T00:00:00Z",
    runCount: 0,
    ...overrides,
  };
}

function makeStore(automations: Automation[] = []): AutomationStore & { runs: AutomationRun[] } {
  const store: Automation[] = [...automations];
  const runs: AutomationRun[] = [];
  return {
    runs,
    list(filter) {
      return store.filter((a) => {
        if (filter?.enabled !== undefined && a.enabled !== filter.enabled)
          return false;
        if (filter?.triggerType && a.trigger.type !== filter.triggerType)
          return false;
        return true;
      });
    },
    get(id) {
      return store.find((a) => a.id === id);
    },
    save(automation) {
      const idx = store.findIndex((a) => a.id === automation.id);
      if (idx >= 0) store[idx] = automation;
      else store.push(automation);
    },
    saveRun(run) {
      runs.push(run);
    },
  };
}

// ── Tests ───────────────────────────────────────────────────────────────────

describe("AutomationEngine", () => {
  const now = () => new Date("2026-01-15T10:00:00Z").getTime();

  it("runs a manual automation with no conditions", async () => {
    const log: string[] = [];
    const auto = makeAutomation({
      trigger: { type: "manual" },
      actions: [
        { type: "object:create", objectType: "task", template: { name: "auto-task" } },
      ],
    });
    const store = makeStore([auto]);
    const handlers: ActionHandlerMap = {
      "object:create": async (action) => {
        log.push(`create:${(action as { template: { name: string } }).template.name}`);
      },
    };

    const engine = new AutomationEngine(store, handlers, { now });
    const run = await engine.run("auto-1");

    expect(run.status).toBe("success");
    expect(run.conditionPassed).toBe(true);
    expect(run.actionResults).toHaveLength(1);
    expect(run.actionResults[0]!.status).toBe("success");
    expect(log).toEqual(["create:auto-task"]);
    expect(store.runs).toHaveLength(1);
  });

  it("skips when conditions fail", async () => {
    const auto = makeAutomation({
      trigger: { type: "manual" },
      conditions: [
        { type: "field", path: "extra.priority", operator: "eq", value: "high" },
      ],
      actions: [
        { type: "object:create", objectType: "task", template: {} },
      ],
    });
    const store = makeStore([auto]);
    const engine = new AutomationEngine(store, {}, { now });

    const run = await engine.run("auto-1", { priority: "low" });

    expect(run.status).toBe("skipped");
    expect(run.conditionPassed).toBe(false);
    expect(run.actionResults).toHaveLength(0);
  });

  it("passes when conditions match", async () => {
    const auto = makeAutomation({
      trigger: { type: "manual" },
      conditions: [
        { type: "field", path: "extra.priority", operator: "eq", value: "high" },
      ],
      actions: [],
    });
    const store = makeStore([auto]);
    const engine = new AutomationEngine(store, {}, { now });

    const run = await engine.run("auto-1", { priority: "high" });

    expect(run.status).toBe("success");
    expect(run.conditionPassed).toBe(true);
  });

  it("handles object events with trigger filters", async () => {
    const log: string[] = [];
    const auto = makeAutomation({
      trigger: {
        type: "object:created",
        objectTypes: ["task"],
        tags: ["urgent"],
      },
      actions: [
        { type: "object:update", target: "trigger", patch: { status: "flagged" } },
      ],
    });
    const store = makeStore([auto]);
    const handlers: ActionHandlerMap = {
      "object:update": async (_action, ctx) => {
        log.push(`update:${ctx.object?.["type"]}`);
      },
    };
    const engine = new AutomationEngine(store, handlers, { now });

    // Matching event
    const runs = await engine.handleObjectEvent({
      type: "object:created",
      object: { type: "task", tags: ["urgent"] },
    });
    expect(runs).toHaveLength(1);
    expect(runs[0]!.status).toBe("success");
    expect(log).toEqual(["update:task"]);

    // Non-matching event (wrong type)
    const runs2 = await engine.handleObjectEvent({
      type: "object:created",
      object: { type: "goal", tags: ["urgent"] },
    });
    expect(runs2).toHaveLength(0);
  });

  it("records partial status when action fails mid-sequence", async () => {
    const auto = makeAutomation({
      trigger: { type: "manual" },
      actions: [
        { type: "object:create", objectType: "a", template: {} },
        { type: "object:create", objectType: "b", template: {} },
      ],
    });
    const store = makeStore([auto]);
    let callCount = 0;
    const handlers: ActionHandlerMap = {
      "object:create": async () => {
        callCount++;
        if (callCount === 2) throw new Error("boom");
      },
    };
    const engine = new AutomationEngine(store, handlers, { now });

    const run = await engine.run("auto-1");

    expect(run.status).toBe("partial");
    expect(run.actionResults).toHaveLength(2);
    expect(run.actionResults[0]!.status).toBe("success");
    expect(run.actionResults[1]!.status).toBe("failed");
    expect(run.actionResults[1]!.error).toContain("boom");
  });

  it("records failed status when first action fails", async () => {
    const auto = makeAutomation({
      trigger: { type: "manual" },
      actions: [
        { type: "object:create", objectType: "a", template: {} },
      ],
    });
    const store = makeStore([auto]);
    const handlers: ActionHandlerMap = {
      "object:create": async () => {
        throw new Error("fail");
      },
    };
    const engine = new AutomationEngine(store, handlers, { now });

    const run = await engine.run("auto-1");

    expect(run.status).toBe("failed");
  });

  it("skips actions with no handler", async () => {
    const auto = makeAutomation({
      trigger: { type: "manual" },
      actions: [
        { type: "notification:send", target: "someone", title: "Hi", body: "Test" },
      ],
    });
    const store = makeStore([auto]);
    const engine = new AutomationEngine(store, {}, { now });

    const run = await engine.run("auto-1");

    expect(run.status).toBe("success");
    expect(run.actionResults[0]!.status).toBe("skipped");
    expect(run.actionResults[0]!.error).toContain("No handler");
  });

  it("interpolates action templates from context", async () => {
    const captured: Record<string, unknown>[] = [];
    const auto = makeAutomation({
      trigger: { type: "manual" },
      actions: [
        {
          type: "object:create",
          objectType: "task",
          template: { name: "Follow-up: {{extra.source}}" },
        },
      ],
    });
    const store = makeStore([auto]);
    const handlers: ActionHandlerMap = {
      "object:create": async (action) => {
        captured.push((action as { template: Record<string, unknown> }).template);
      },
    };
    const engine = new AutomationEngine(store, handlers, { now });

    await engine.run("auto-1", { source: "email" });

    expect(captured[0]).toEqual({ name: "Follow-up: email" });
  });

  it("updates automation runCount on success", async () => {
    const auto = makeAutomation({
      trigger: { type: "manual" },
      actions: [],
    });
    const store = makeStore([auto]);
    const engine = new AutomationEngine(store, {}, { now });

    await engine.run("auto-1");
    const updated = store.get("auto-1");
    expect(updated!.runCount).toBe(1);
    expect(updated!.lastRunAt).toBeDefined();
  });

  it("does not update runCount on skip", async () => {
    const auto = makeAutomation({
      trigger: { type: "manual" },
      conditions: [
        { type: "field", path: "extra.x", operator: "eq", value: "y" },
      ],
      actions: [],
    });
    const store = makeStore([auto]);
    const engine = new AutomationEngine(store, {}, { now });

    await engine.run("auto-1", { x: "n" });
    const updated = store.get("auto-1");
    expect(updated!.runCount).toBe(0);
  });

  it("throws when running nonexistent automation", async () => {
    const store = makeStore([]);
    const engine = new AutomationEngine(store, {}, { now });

    await expect(engine.run("missing")).rejects.toThrow("not found");
  });

  it("fires onRunComplete callback", async () => {
    const runs: AutomationRun[] = [];
    const auto = makeAutomation({ trigger: { type: "manual" }, actions: [] });
    const store = makeStore([auto]);
    const engine = new AutomationEngine(store, {}, {
      now,
      onRunComplete: (run) => runs.push(run),
    });

    await engine.run("auto-1");
    expect(runs).toHaveLength(1);
    expect(runs[0]!.status).toBe("success");
  });

  // ── Cron scheduling ────────────────────────────────────────────────────────

  describe("cron scheduling", () => {
    beforeEach(() => {
      vi.useFakeTimers();
    });
    afterEach(() => {
      vi.useRealTimers();
    });

    it("starts and stops cron timers", () => {
      const auto = makeAutomation({
        trigger: { type: "cron", cron: "* * * * *" },
        actions: [],
      });
      const store = makeStore([auto]);
      const engine = new AutomationEngine(store, {}, { now });

      expect(engine.running).toBe(false);
      engine.start();
      expect(engine.running).toBe(true);
      engine.stop();
      expect(engine.running).toBe(false);
    });

    it("executes cron automation on interval", async () => {
      const auto = makeAutomation({
        trigger: { type: "cron", cron: "* * * * *" },
        actions: [],
      });
      const store = makeStore([auto]);
      const engine = new AutomationEngine(store, {}, { now });

      engine.start();
      await vi.advanceTimersByTimeAsync(60_001);
      expect(store.runs.length).toBeGreaterThanOrEqual(1);
      engine.stop();
    });

    it("refreshAutomation reschedules cron", () => {
      const auto = makeAutomation({
        trigger: { type: "cron", cron: "* * * * *" },
        actions: [],
      });
      const store = makeStore([auto]);
      const engine = new AutomationEngine(store, {}, { now });

      engine.start();
      // Should not throw
      engine.refreshAutomation("auto-1");
      engine.stop();
    });

    it("does not start cron for disabled automations", () => {
      const auto = makeAutomation({
        enabled: false,
        trigger: { type: "cron", cron: "* * * * *" },
        actions: [],
      });
      const store = makeStore([auto]);
      const engine = new AutomationEngine(store, {}, { now });

      engine.start();
      vi.advanceTimersByTime(120_000);
      expect(store.runs).toHaveLength(0);
      engine.stop();
    });
  });

  // ── Delay action ──────────────────────────────────────────────────────────

  it("handles delay actions", async () => {
    vi.useFakeTimers();
    const auto = makeAutomation({
      trigger: { type: "manual" },
      actions: [{ type: "delay", seconds: 1 }],
    });
    const store = makeStore([auto]);
    const engine = new AutomationEngine(store, {}, { now });

    const promise = engine.run("auto-1");
    await vi.advanceTimersByTimeAsync(1001);
    const run = await promise;

    expect(run.status).toBe("success");
    expect(run.actionResults[0]!.actionType).toBe("delay");
    vi.useRealTimers();
  });
});
