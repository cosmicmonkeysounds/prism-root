/**
 * @prism/core — Actor System (Layer 0B)
 *
 * Process queue for ordered execution of automation/script tasks.
 * Pluggable ActorRuntime for Luau, TypeScript, and Python.
 * Capability-scoped sandboxing per task.
 *
 * Usage:
 *   const queue = createProcessQueue({ concurrency: 2 });
 *   queue.registerRuntime(createLuauActorRuntime());
 *   queue.enqueue({ name: "calc", runtime: "luau", payload: { script: "return 1+1" } });
 *   queue.start();
 */

import type {
  CapabilityScope,
  ProcessTask,
  TaskStatus,
  ActorRuntime,
  RuntimeResult,
  QueueEvent,
  QueueListener,
  ProcessQueue,
  ProcessQueueOptions,
} from "./actor-types.js";
import { DEFAULT_CAPABILITY_SCOPE } from "./actor-types.js";
import type { ExecutionTarget } from "./actor-types.js";

// ── Helpers ─────────────────────────────────────────────────────────────────

let idCounter = 0;
function uid(prefix: string): string {
  return `${prefix}-${Date.now().toString(36)}-${(idCounter++).toString(36)}`;
}

// ── Process Queue ───────────────────────────────────────────────────────────

export function createProcessQueue(
  options: ProcessQueueOptions = {},
): ProcessQueue {
  const { concurrency = 1, autoStart = false } = options;

  const tasks = new Map<string, ProcessTask>();
  const runtimes = new Map<string, ActorRuntime>();
  const listeners = new Set<QueueListener>();
  let processing = false;
  let runningCount = 0;

  function notify(event: QueueEvent): void {
    for (const listener of listeners) listener(event);
  }

  function enqueue<TPayload>(params: {
    name: string;
    runtime: string;
    payload: TPayload;
    target?: ExecutionTarget;
    scope?: Partial<CapabilityScope>;
    priority?: number;
  }): ProcessTask<TPayload> {
    const task: ProcessTask<TPayload> = {
      id: uid("task"),
      name: params.name,
      runtime: params.runtime,
      target: params.target ?? "local",
      payload: params.payload,
      scope: { ...DEFAULT_CAPABILITY_SCOPE, ...params.scope },
      priority: params.priority ?? 10,
      status: "pending",
      result: null,
      error: null,
      enqueuedAt: new Date().toISOString(),
      startedAt: null,
      completedAt: null,
    };

    tasks.set(task.id, task as ProcessTask);
    notify({ type: "enqueued", taskId: task.id, task: task as ProcessTask });

    // Kick processing if auto-running
    if (processing) scheduleNext();

    return task;
  }

  function cancel(taskId: string): boolean {
    const task = tasks.get(taskId);
    if (!task) return false;
    if (task.status !== "pending" && task.status !== "running") return false;

    task.status = "cancelled";
    task.completedAt = new Date().toISOString();
    notify({ type: "cancelled", taskId, task });
    return true;
  }

  function get(taskId: string): ProcessTask | undefined {
    return tasks.get(taskId);
  }

  function list(status?: TaskStatus): ProcessTask[] {
    const all = [...tasks.values()];
    if (!status) return all;
    return all.filter(t => t.status === status);
  }

  function getNextPending(): ProcessTask | undefined {
    const pending = [...tasks.values()]
      .filter(t => t.status === "pending")
      .sort((a, b) => a.priority - b.priority);
    return pending[0];
  }

  async function executeTask(task: ProcessTask): Promise<void> {
    const runtime = runtimes.get(task.runtime);
    if (!runtime) {
      task.status = "failed";
      task.error = `Runtime "${task.runtime}" not registered`;
      task.completedAt = new Date().toISOString();
      notify({ type: "failed", taskId: task.id, task });
      return;
    }

    task.status = "running";
    task.startedAt = new Date().toISOString();
    runningCount++;
    notify({ type: "started", taskId: task.id, task });

    try {
      const result: RuntimeResult = await runtime.execute(task.payload, task.scope);

      if ((task as ProcessTask).status === "cancelled") {
        runningCount--;
        return;
      }

      if (result.success) {
        task.status = "completed";
        task.result = result.value;
      } else {
        task.status = "failed";
        task.error = result.error ?? "Unknown error";
      }
    } catch (err) {
      task.status = "failed";
      task.error = err instanceof Error ? err.message : String(err);
    }

    task.completedAt = new Date().toISOString();
    runningCount--;
    notify({
      type: task.status === "completed" ? "completed" : "failed",
      taskId: task.id,
      task,
    });

    // Continue processing
    if (processing) scheduleNext();
  }

  function scheduleNext(): void {
    if (runningCount >= concurrency) return;
    const next = getNextPending();
    if (!next) return;
    // Fire-and-forget (tracked via events)
    void executeTask(next);
  }

  async function processNext(): Promise<ProcessTask | null> {
    const next = getNextPending();
    if (!next) return null;
    await executeTask(next);
    return next;
  }

  function start(): void {
    if (processing) return;
    processing = true;
    // Fill up to concurrency
    for (let i = 0; i < concurrency; i++) {
      scheduleNext();
    }
  }

  function stop(): void {
    processing = false;
  }

  function subscribe(listener: QueueListener): () => void {
    listeners.add(listener);
    return () => { listeners.delete(listener); };
  }

  function registerRuntime(runtime: ActorRuntime): void {
    runtimes.set(runtime.name, runtime);
  }

  function getRuntime(name: string): ActorRuntime | undefined {
    return runtimes.get(name);
  }

  function runtimeNames(): string[] {
    return [...runtimes.keys()];
  }

  function prune(): number {
    let pruned = 0;
    for (const [id, task] of tasks) {
      if (task.status === "completed" || task.status === "failed" || task.status === "cancelled") {
        tasks.delete(id);
        pruned++;
      }
    }
    return pruned;
  }

  function dispose(): void {
    processing = false;
    tasks.clear();
    listeners.clear();
  }

  if (autoStart) start();

  return {
    enqueue,
    cancel,
    get,
    list,
    get pendingCount() {
      return [...tasks.values()].filter(t => t.status === "pending").length;
    },
    get runningCount() {
      return runningCount;
    },
    processNext,
    start,
    stop,
    get processing() {
      return processing;
    },
    subscribe,
    registerRuntime,
    getRuntime,
    runtimeNames,
    prune,
    dispose,
  };
}

// ── Luau Actor Runtime ──────────────────────────────────────────────────────

/** Payload for the Luau actor runtime. */
export interface LuauPayload {
  script: string;
  args?: Record<string, unknown>;
}

/**
 * Create a Luau actor runtime (browser-side via luau-web).
 * Wraps the existing language/luau/luau-runtime.ts.
 *
 * The executeLuau function is injected to avoid a hard import dependency
 * on luau-web (which requires WASM loading).
 */
export function createLuauActorRuntime(
  executeFn: (script: string, args?: Record<string, unknown>) => Promise<{ success: boolean; value: unknown; error?: string }>,
): ActorRuntime {
  return {
    name: "luau",

    async isAvailable(): Promise<boolean> {
      try {
        const result = await executeFn("return true");
        return result.success;
      } catch {
        return false;
      }
    },

    async execute<TPayload, TResult>(
      payload: TPayload,
      _scope: CapabilityScope,
    ): Promise<RuntimeResult<TResult>> {
      const { script, args } = payload as unknown as LuauPayload;
      const start = performance.now();

      try {
        const result = await executeFn(script, args);
        const durationMs = performance.now() - start;

        if (result.success) {
          return { success: true, value: result.value as TResult, durationMs };
        }
        return { success: false, value: null, error: result.error ?? "Unknown error", durationMs };
      } catch (err) {
        const durationMs = performance.now() - start;
        return {
          success: false,
          value: null,
          error: err instanceof Error ? err.message : String(err),
          durationMs,
        };
      }
    },

    async dispose(): Promise<void> {
      // luau-web states are GC'd; no explicit teardown needed
    },
  };
}

// ── Sidecar Runtime (interface for TypeScript/Python) ───────────────────────

/** Payload for sidecar runtimes. */
export interface SidecarPayload {
  /** Script source or file path. */
  source: string;
  /** Arguments passed to the script. */
  args?: Record<string, unknown>;
}

export interface SidecarExecutor {
  run(source: string, args?: Record<string, unknown>): Promise<{ success: boolean; value: unknown; error?: string }>;
  isAvailable(): Promise<boolean>;
  dispose(): Promise<void>;
}

/**
 * Create a sidecar actor runtime for TypeScript (Deno) or Python.
 * The actual process spawning is handled by the SidecarExecutor,
 * which is provided by the Daemon layer (Tauri shell).
 */
export function createSidecarRuntime(
  name: string,
  executor: SidecarExecutor,
): ActorRuntime {
  return {
    name,

    async isAvailable(): Promise<boolean> {
      return executor.isAvailable();
    },

    async execute<TPayload, TResult>(
      payload: TPayload,
      _scope: CapabilityScope,
    ): Promise<RuntimeResult<TResult>> {
      const { source, args } = payload as unknown as SidecarPayload;
      const start = performance.now();

      try {
        const result = await executor.run(source, args);
        const durationMs = performance.now() - start;

        if (result.success) {
          return { success: true, value: result.value as TResult, durationMs };
        }
        return { success: false, value: null, error: result.error ?? "Unknown error", durationMs };
      } catch (err) {
        const durationMs = performance.now() - start;
        return {
          success: false,
          value: null,
          error: err instanceof Error ? err.message : String(err),
          durationMs,
        };
      }
    },

    async dispose(): Promise<void> {
      await executor.dispose();
    },
  };
}

// ── In-memory Test Runtime ──────────────────────────────────────────────────

/**
 * Simple in-memory runtime for testing. Executes a synchronous function.
 */
export function createTestRuntime(
  name: string,
  handler: (payload: unknown) => unknown,
): ActorRuntime {
  return {
    name,
    async isAvailable() { return true; },
    async execute<TPayload, TResult>(
      payload: TPayload,
      _scope: CapabilityScope,
    ): Promise<RuntimeResult<TResult>> {
      const start = performance.now();
      try {
        const value = handler(payload);
        return { success: true, value: value as TResult, durationMs: performance.now() - start };
      } catch (err) {
        return {
          success: false,
          value: null,
          error: err instanceof Error ? err.message : String(err),
          durationMs: performance.now() - start,
        };
      }
    },
    async dispose() {},
  };
}
