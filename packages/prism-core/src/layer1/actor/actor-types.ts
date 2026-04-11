/**
 * @prism/core — Actor System Types (Layer 0B)
 *
 * Unifies Automation (0B) and Intelligence (0C) into a single federated
 * compute pipeline. Actors are modular, opt-in "employees" that execute
 * tasks against the CRDT graph.
 *
 * Three execution targets:
 *   1. Sovereign Local — zero latency, offline, private
 *   2. Federated Delegate — trusted Relay with GPU via E2EE
 *   3. External Provider — third-party API with Capability Token
 *
 * Runtimes:
 *   - Luau via luau-web (browser) / mlua (daemon)
 *   - TypeScript via Deno sidecar
 *   - Python via sidecar
 */

// ── Execution Targets ───────────────────────────────────────────────────────

export type ExecutionTarget = "local" | "federated" | "external";

// ── Capability Scope (Sandbox) ──────────────────────────────────────────────

/**
 * Fine-grained permission scope for actor sandboxing.
 * Actors get zero capabilities by default; each must be explicitly granted.
 */
export interface CapabilityScope {
  /** Allow network access (fetch, WebSocket). */
  network: boolean;
  /** Allow filesystem read. */
  fsRead: boolean;
  /** Allow filesystem write. */
  fsWrite: boolean;
  /** Allow CRDT read. */
  crdtRead: boolean;
  /** Allow CRDT write. */
  crdtWrite: boolean;
  /** Allow spawning sub-processes. */
  spawn: boolean;
  /** Allowed API endpoints (for external providers). Empty = none. */
  allowedEndpoints: string[];
  /** Maximum execution time in milliseconds. 0 = no limit. */
  maxDurationMs: number;
  /** Maximum memory in bytes. 0 = no limit. */
  maxMemoryBytes: number;
}

/** Default: zero-trust sandbox. */
export const DEFAULT_CAPABILITY_SCOPE: CapabilityScope = {
  network: false,
  fsRead: false,
  fsWrite: false,
  crdtRead: true,
  crdtWrite: false,
  spawn: false,
  allowedEndpoints: [],
  maxDurationMs: 30_000,
  maxMemoryBytes: 0,
};

// ── Process Task ────────────────────────────────────────────────────────────

export type TaskStatus = "pending" | "running" | "completed" | "failed" | "cancelled";

export interface ProcessTask<TPayload = unknown, TResult = unknown> {
  /** Unique task ID. */
  id: string;
  /** Human-readable name. */
  name: string;
  /** Which runtime to use. */
  runtime: string;
  /** Execution target. */
  target: ExecutionTarget;
  /** Task-specific payload (script source, prompt, etc.). */
  payload: TPayload;
  /** Capability scope for sandboxing. */
  scope: CapabilityScope;
  /** Priority (lower = higher priority). Default: 10. */
  priority: number;
  /** Current status. */
  status: TaskStatus;
  /** Result on completion. */
  result: TResult | null;
  /** Error message on failure. */
  error: string | null;
  /** ISO-8601 enqueued timestamp. */
  enqueuedAt: string;
  /** ISO-8601 started timestamp. */
  startedAt: string | null;
  /** ISO-8601 completed timestamp. */
  completedAt: string | null;
}

// ── Actor Runtime ───────────────────────────────────────────────────────────

/** Result from executing a task in a runtime. */
export interface RuntimeResult<T = unknown> {
  success: boolean;
  value: T | null;
  error?: string;
  /** Execution duration in milliseconds. */
  durationMs: number;
}

/**
 * Pluggable language runtime. Implementations:
 *   - "lua" — luau-web (browser) / mlua (daemon)
 *   - "typescript" — Deno sidecar
 *   - "python" — Python sidecar
 */
export interface ActorRuntime {
  /** Runtime identifier (e.g. "lua", "typescript", "python"). */
  readonly name: string;
  /** Whether this runtime is available in the current environment. */
  isAvailable(): Promise<boolean>;
  /** Execute a task payload and return the result. */
  execute<TPayload, TResult>(
    payload: TPayload,
    scope: CapabilityScope,
  ): Promise<RuntimeResult<TResult>>;
  /** Dispose of runtime resources. */
  dispose(): Promise<void>;
}

// ── Process Queue ───────────────────────────────────────────────────────────

export type QueueEventType = "enqueued" | "started" | "completed" | "failed" | "cancelled";

export interface QueueEvent {
  type: QueueEventType;
  taskId: string;
  task: ProcessTask;
}

export type QueueListener = (event: QueueEvent) => void;

export interface ProcessQueue {
  /** Enqueue a task for execution. Returns the task with assigned ID. */
  enqueue<TPayload>(params: {
    name: string;
    runtime: string;
    payload: TPayload;
    target?: ExecutionTarget;
    scope?: Partial<CapabilityScope>;
    priority?: number;
  }): ProcessTask<TPayload>;

  /** Cancel a pending or running task. */
  cancel(taskId: string): boolean;

  /** Get a task by ID. */
  get(taskId: string): ProcessTask | undefined;

  /** List all tasks, optionally filtered by status. */
  list(status?: TaskStatus): ProcessTask[];

  /** Number of pending tasks. */
  readonly pendingCount: number;

  /** Number of currently running tasks. */
  readonly runningCount: number;

  /** Process the next pending task. Returns the task or null if empty. */
  processNext(): Promise<ProcessTask | null>;

  /** Start auto-processing: continuously dequeue and execute tasks. */
  start(): void;

  /** Stop auto-processing. Running tasks complete but no new ones start. */
  stop(): void;

  /** Whether auto-processing is active. */
  readonly processing: boolean;

  /** Subscribe to queue events. */
  subscribe(listener: QueueListener): () => void;

  /** Register an ActorRuntime. */
  registerRuntime(runtime: ActorRuntime): void;

  /** Get a registered runtime by name. */
  getRuntime(name: string): ActorRuntime | undefined;

  /** List registered runtime names. */
  runtimeNames(): string[];

  /** Clear completed/failed/cancelled tasks from the queue. */
  prune(): number;

  /** Dispose: stop processing, clear all tasks. */
  dispose(): void;
}

// ── Options ─────────────────────────────────────────────────────────────────

export interface ProcessQueueOptions {
  /** Maximum concurrent tasks. Default: 1. */
  concurrency?: number;
  /** Auto-start processing on creation. Default: false. */
  autoStart?: boolean;
}
