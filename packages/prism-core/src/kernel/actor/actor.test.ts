import { describe, it, expect } from "vitest";
import {
  createProcessQueue,
  createLuauActorRuntime,
  createSidecarRuntime,
  createTestRuntime,
} from "./actor.js";
import { DEFAULT_CAPABILITY_SCOPE } from "./actor-types.js";
import type { QueueEvent, ProcessTask, SidecarExecutor } from "./index.js";

// ── ProcessQueue basics ─────────────────────────────────────────────────────

describe("ProcessQueue", () => {
  function mathRuntime() {
    return createTestRuntime("math", (payload) => {
      const p = payload as { op: string; a: number; b: number };
      if (p.op === "add") return p.a + p.b;
      if (p.op === "mul") return p.a * p.b;
      throw new Error(`Unknown op: ${p.op}`);
    });
  }

  it("enqueues a task with defaults", () => {
    const queue = createProcessQueue();
    queue.registerRuntime(mathRuntime());

    const task = queue.enqueue({
      name: "add",
      runtime: "math",
      payload: { op: "add", a: 1, b: 2 },
    });

    expect(task.id).toBeTruthy();
    expect(task.name).toBe("add");
    expect(task.status).toBe("pending");
    expect(task.target).toBe("local");
    expect(task.priority).toBe(10);
    expect(task.scope).toEqual(DEFAULT_CAPABILITY_SCOPE);
    expect(queue.pendingCount).toBe(1);
  });

  it("processes a task and returns result", async () => {
    const queue = createProcessQueue();
    queue.registerRuntime(mathRuntime());

    queue.enqueue({
      name: "add",
      runtime: "math",
      payload: { op: "add", a: 3, b: 4 },
    });

    const task = await queue.processNext();
    expect(task).toBeDefined();
    expect((task as ProcessTask).status).toBe("completed");
    expect((task as ProcessTask).result).toBe(7);
    expect((task as ProcessTask).startedAt).toBeTruthy();
    expect((task as ProcessTask).completedAt).toBeTruthy();
  });

  it("handles runtime errors gracefully", async () => {
    const queue = createProcessQueue();
    queue.registerRuntime(mathRuntime());

    queue.enqueue({
      name: "bad-op",
      runtime: "math",
      payload: { op: "div", a: 1, b: 0 },
    });

    const task = await queue.processNext();
    expect((task as ProcessTask).status).toBe("failed");
    expect((task as ProcessTask).error).toContain("Unknown op: div");
  });

  it("fails when runtime is not registered", async () => {
    const queue = createProcessQueue();

    queue.enqueue({
      name: "orphan",
      runtime: "nonexistent",
      payload: {},
    });

    const task = await queue.processNext();
    expect((task as ProcessTask).status).toBe("failed");
    expect((task as ProcessTask).error).toContain("not registered");
  });

  it("returns null when queue is empty", async () => {
    const queue = createProcessQueue();
    const task = await queue.processNext();
    expect(task).toBeNull();
  });

  it("processes by priority (lower = higher)", async () => {
    const queue = createProcessQueue();
    queue.registerRuntime(
      createTestRuntime("echo", (p) => (p as { v: string }).v),
    );

    queue.enqueue({ name: "low", runtime: "echo", payload: { v: "low" }, priority: 20 });
    queue.enqueue({ name: "high", runtime: "echo", payload: { v: "high" }, priority: 1 });
    queue.enqueue({ name: "mid", runtime: "echo", payload: { v: "mid" }, priority: 10 });

    const t1 = await queue.processNext();
    expect((t1 as ProcessTask).result).toBe("high");
    const t2 = await queue.processNext();
    expect((t2 as ProcessTask).result).toBe("mid");
    const t3 = await queue.processNext();
    expect((t3 as ProcessTask).result).toBe("low");
  });

  it("cancels a pending task", () => {
    const queue = createProcessQueue();
    queue.registerRuntime(mathRuntime());

    const task = queue.enqueue({ name: "cancel-me", runtime: "math", payload: {} });
    expect(queue.cancel(task.id)).toBe(true);
    expect(queue.get(task.id)?.status).toBe("cancelled");
    expect(queue.pendingCount).toBe(0);
  });

  it("cannot cancel a completed task", async () => {
    const queue = createProcessQueue();
    queue.registerRuntime(mathRuntime());

    const task = queue.enqueue({
      name: "done",
      runtime: "math",
      payload: { op: "add", a: 1, b: 1 },
    });
    await queue.processNext();
    expect(queue.cancel(task.id)).toBe(false);
  });

  it("lists tasks by status", async () => {
    const queue = createProcessQueue();
    queue.registerRuntime(mathRuntime());

    queue.enqueue({ name: "a", runtime: "math", payload: { op: "add", a: 1, b: 1 } });
    queue.enqueue({ name: "b", runtime: "math", payload: { op: "add", a: 2, b: 2 } });

    expect(queue.list("pending")).toHaveLength(2);
    await queue.processNext();
    expect(queue.list("completed")).toHaveLength(1);
    expect(queue.list("pending")).toHaveLength(1);
  });

  it("prunes completed/failed/cancelled tasks", async () => {
    const queue = createProcessQueue();
    queue.registerRuntime(mathRuntime());

    queue.enqueue({ name: "a", runtime: "math", payload: { op: "add", a: 1, b: 1 } });
    queue.enqueue({ name: "b", runtime: "math", payload: { op: "add", a: 2, b: 2 } });

    await queue.processNext();
    await queue.processNext();

    const pruned = queue.prune();
    expect(pruned).toBe(2);
    expect(queue.list()).toHaveLength(0);
  });

  it("emits queue events", async () => {
    const queue = createProcessQueue();
    queue.registerRuntime(mathRuntime());

    const events: QueueEvent[] = [];
    queue.subscribe((e) => events.push(e));

    queue.enqueue({ name: "tracked", runtime: "math", payload: { op: "add", a: 1, b: 1 } });
    await queue.processNext();

    const types = events.map(e => e.type);
    expect(types).toContain("enqueued");
    expect(types).toContain("started");
    expect(types).toContain("completed");
  });

  it("unsubscribe stops events", () => {
    const queue = createProcessQueue();
    queue.registerRuntime(mathRuntime());

    const events: QueueEvent[] = [];
    const unsub = queue.subscribe((e) => events.push(e));
    unsub();

    queue.enqueue({ name: "silent", runtime: "math", payload: {} });
    expect(events).toHaveLength(0);
  });

  it("registers and queries runtimes", () => {
    const queue = createProcessQueue();
    queue.registerRuntime(mathRuntime());
    queue.registerRuntime(createTestRuntime("echo", (p) => p));

    expect(queue.runtimeNames()).toEqual(["math", "echo"]);
    expect(queue.getRuntime("math")).toBeDefined();
    expect(queue.getRuntime("unknown")).toBeUndefined();
  });

  it("dispose clears everything", () => {
    const queue = createProcessQueue();
    queue.registerRuntime(mathRuntime());
    queue.enqueue({ name: "a", runtime: "math", payload: {} });
    queue.start();

    queue.dispose();
    expect(queue.list()).toHaveLength(0);
    expect(queue.processing).toBe(false);
  });

  it("custom scope overrides defaults", () => {
    const queue = createProcessQueue();
    queue.registerRuntime(mathRuntime());

    const task = queue.enqueue({
      name: "scoped",
      runtime: "math",
      payload: {},
      scope: { network: true, crdtWrite: true },
    });

    expect(task.scope.network).toBe(true);
    expect(task.scope.crdtWrite).toBe(true);
    expect(task.scope.fsRead).toBe(false); // default preserved
  });
});

// ── Auto-processing ─────────────────────────────────────────────────────────

describe("ProcessQueue auto-processing", () => {
  it("start/stop controls processing", async () => {
    const queue = createProcessQueue();
    queue.registerRuntime(createTestRuntime("echo", (p) => p));

    queue.start();
    expect(queue.processing).toBe(true);

    queue.stop();
    expect(queue.processing).toBe(false);
  });

  it("auto-processes enqueued tasks", async () => {
    const queue = createProcessQueue({ concurrency: 1 });
    queue.registerRuntime(createTestRuntime("echo", (p) => p));

    const events: QueueEvent[] = [];
    queue.subscribe((e) => events.push(e));

    queue.start();
    queue.enqueue({ name: "auto", runtime: "echo", payload: "hello" });

    // Wait for async processing
    await new Promise(r => setTimeout(r, 20));

    const completed = events.filter(e => e.type === "completed");
    expect(completed.length).toBeGreaterThanOrEqual(1);

    queue.stop();
  });
});

// ── Luau Actor Runtime ──────────────────────────────────────────────────────

describe("LuauActorRuntime", () => {
  const mockLuauExecute = async (script: string, _args?: Record<string, unknown>) => {
    // Simple mock: evaluate "return X" as JSON
    if (script.startsWith("return ")) {
      const expr = script.slice(7);
      try {
        return { success: true, value: JSON.parse(expr) };
      } catch {
        return { success: true, value: expr };
      }
    }
    if (script === "error") {
      return { success: false, value: null, error: "luau error" };
    }
    return { success: true, value: null };
  };

  it("executes a luau payload", async () => {
    const runtime = createLuauActorRuntime(mockLuauExecute);
    const result = await runtime.execute(
      { script: "return 42", args: {} },
      DEFAULT_CAPABILITY_SCOPE,
    );

    expect(result.success).toBe(true);
    expect(result.value).toBe(42);
    expect(result.durationMs).toBeGreaterThanOrEqual(0);
  });

  it("reports luau errors", async () => {
    const runtime = createLuauActorRuntime(mockLuauExecute);
    const result = await runtime.execute(
      { script: "error" },
      DEFAULT_CAPABILITY_SCOPE,
    );

    expect(result.success).toBe(false);
    expect(result.error).toBe("luau error");
  });

  it("checks availability", async () => {
    const runtime = createLuauActorRuntime(async () => ({ success: true, value: true }));
    expect(await runtime.isAvailable()).toBe(true);
  });

  it("name is luau", () => {
    const runtime = createLuauActorRuntime(mockLuauExecute);
    expect(runtime.name).toBe("luau");
  });
});

// ── Sidecar Runtime ─────────────────────────────────────────────────────────

describe("SidecarRuntime", () => {
  function mockExecutor(): SidecarExecutor {
    return {
      async run(source: string) {
        if (source === "fail") return { success: false, value: null, error: "sidecar crash" };
        return { success: true, value: `executed: ${source}` };
      },
      async isAvailable() { return true; },
      async dispose() {},
    };
  }

  it("executes via sidecar", async () => {
    const runtime = createSidecarRuntime("typescript", mockExecutor());
    const result = await runtime.execute(
      { source: "console.log('hi')" },
      DEFAULT_CAPABILITY_SCOPE,
    );

    expect(result.success).toBe(true);
    expect(result.value).toBe("executed: console.log('hi')");
  });

  it("reports sidecar errors", async () => {
    const runtime = createSidecarRuntime("python", mockExecutor());
    const result = await runtime.execute(
      { source: "fail" },
      DEFAULT_CAPABILITY_SCOPE,
    );

    expect(result.success).toBe(false);
    expect(result.error).toBe("sidecar crash");
  });

  it("has the given name", () => {
    const runtime = createSidecarRuntime("typescript", mockExecutor());
    expect(runtime.name).toBe("typescript");
  });
});
