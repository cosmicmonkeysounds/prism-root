/**
 * BuilderManager — Studio's self-replicating build pipeline.
 *
 * The BuilderManager lets Studio produce focused apps (Flux, Lattice,
 * Cadence, Grip) and Relay deployments from the same monorepo. It
 * mirrors the RelayManager pattern: all side-effects go through an
 * injectable `BuildExecutor` so the browser-only environment can run
 * in "dry-run" mode while a Tauri build can actually drive the daemon.
 *
 * Architecture:
 *   Studio UI (app-builder-panel)
 *     ↓ useKernel().builder.planBuild(profile, target)
 *     ↓ BuilderManager composes BuildPlan via @prism/core/builder
 *     ↓ BuilderManager.runPlan(plan) walks plan.steps one at a time,
 *       dispatching each BuildStep through the injected BuildExecutor
 *     ↓ Executor either:
 *        - Tauri mode: invoke('run_build_step', { step, workingDir, env })
 *          → prism_daemon::commands::build::run_build_step
 *        - Dry-run mode: mark emit-file steps success (buffering contents
 *          into stdout for preview), skip run-command / invoke-ipc
 */

import { createBuildPlan } from "./build-plan.js";
import { listBuiltInProfiles } from "./profiles.js";
import { ALL_BUILD_TARGETS } from "./types.js";
import type {
  AppProfile,
  BuildPlan,
  BuildTarget,
  BuildStep,
  BuildStepResult,
  BuildRun,
  BuiltInProfileId,
  ArtifactDescriptor,
} from "./types.js";

// ── Types ─────────────────────────────────────────────────────────────────

type Listener = () => void;

/**
 * Per-plan context passed to each step. The daemon needs this to resolve
 * relative paths (`emit-file`) and to apply plan-level env vars to child
 * processes (`run-command`). Derived from the BuildPlan by `runPlan`.
 */
export interface BuildExecutionContext {
  workingDir: string;
  env: Record<string, string>;
}

/**
 * Abstract build executor. Injected at construction so the kernel can
 * choose between Tauri IPC (desktop) and dry-run mode (browser/tests).
 */
export interface BuildExecutor {
  readonly mode: "tauri" | "dry-run";
  executeStep(step: BuildStep, context?: BuildExecutionContext): Promise<BuildStepResult>;
}

export interface BuilderManagerOptions {
  executor?: BuildExecutor;
  /** Additional user-authored profiles to register at construction. */
  profiles?: AppProfile[];
}

export interface BuilderManager {
  // ── Profiles ─────────────────────────────────────────────────────────
  /** List all registered profiles (built-in + custom). */
  listProfiles(): AppProfile[];
  /** Get a profile by id. */
  getProfile(id: string): AppProfile | undefined;
  /** Register a new custom profile (or overwrite an existing one). */
  registerProfile(profile: AppProfile): void;
  /** Remove a profile (built-ins cannot be removed). */
  removeProfile(id: string): boolean;

  // ── Active profile (pins Studio's surface) ───────────────────────────
  /**
   * Currently active profile. `null` = unprofiled / universal host mode.
   * When set, downstream consumers (kernel, lens registry) can filter
   * their surface. The manager itself is storage-only — enforcement is
   * up to the consumer.
   */
  getActiveProfile(): AppProfile | null;
  setActiveProfile(id: string | null): void;

  // ── Build planning ───────────────────────────────────────────────────
  readonly targets: readonly BuildTarget[];
  /** Create a BuildPlan for a profile + target pair. */
  planBuild(profileId: string, target: BuildTarget, dryRun?: boolean): BuildPlan;
  /** Create plans for multiple targets at once (common case). */
  planBuilds(profileId: string, targets: BuildTarget[], dryRun?: boolean): BuildPlan[];

  // ── Execution ────────────────────────────────────────────────────────
  /** Execute a plan, returning a BuildRun with step-by-step results. */
  runPlan(plan: BuildPlan): Promise<BuildRun>;
  /** List all completed runs. */
  listRuns(): BuildRun[];
  /** Get a specific run by id. */
  getRun(id: string): BuildRun | undefined;
  /** Clear run history. */
  clearRuns(): void;

  // ── Subscriptions ────────────────────────────────────────────────────
  subscribe(listener: Listener): () => void;
  dispose(): void;
}

// ── Dry-run executor ─────────────────────────────────────────────────────

/**
 * Default executor used in the browser (and tests): treats every step as
 * "successful" for inspection purposes. `emit-file` steps are buffered
 * into the result's `stdout` so the user can inspect what WOULD be
 * written. `run-command` and `invoke-ipc` steps are marked as skipped.
 */
export function createDryRunExecutor(): BuildExecutor {
  return {
    mode: "dry-run",
    async executeStep(step: BuildStep): Promise<BuildStepResult> {
      const startedAt = Date.now();
      if (step.kind === "emit-file") {
        return {
          step,
          status: "success",
          startedAt,
          finishedAt: Date.now(),
          stdout: step.contents,
        };
      }
      // run-command / invoke-ipc are skipped in dry-run mode
      return {
        step,
        status: "skipped",
        startedAt,
        finishedAt: Date.now(),
        stdout: `[dry-run] Skipped: ${step.description}`,
      };
    },
  };
}

// ── Tauri executor (scaffolded) ──────────────────────────────────────────

/**
 * Executor that dispatches steps through Tauri IPC into the Prism
 * Daemon. The daemon is responsible for running the commands, capturing
 * output, and returning structured results. Studio never spawns child
 * processes directly — everything goes through the daemon's sandboxed
 * build runner.
 *
 * The `invokeFn` is injected so that non-Tauri environments (tests) can
 * supply their own mock.
 */
export interface TauriExecutorOptions {
  invoke: (command: string, args: Record<string, unknown>) => Promise<unknown>;
}

export function createTauriExecutor(options: TauriExecutorOptions): BuildExecutor {
  const { invoke } = options;
  return {
    mode: "tauri",
    async executeStep(
      step: BuildStep,
      context?: BuildExecutionContext,
    ): Promise<BuildStepResult> {
      const startedAt = Date.now();
      try {
        // The Rust command is `run_build_step(step, working_dir, env)`.
        // Tauri auto-converts camelCase JS args to snake_case Rust params, so
        // we send `workingDir` / `env` alongside the step.
        const payload: Record<string, unknown> = { step };
        if (context) {
          payload.workingDir = context.workingDir;
          payload.env = context.env;
        }
        const result = (await invoke("run_build_step", payload)) as {
          stdout?: string;
          stderr?: string;
        };
        const stepResult: BuildStepResult = {
          step,
          status: "success",
          startedAt,
          finishedAt: Date.now(),
        };
        if (result.stdout !== undefined) stepResult.stdout = result.stdout;
        if (result.stderr !== undefined) stepResult.stderr = result.stderr;
        return stepResult;
      } catch (err) {
        return {
          step,
          status: "failed",
          startedAt,
          finishedAt: Date.now(),
          errorMessage: err instanceof Error ? err.message : String(err),
        };
      }
    },
  };
}

// ── Factory ──────────────────────────────────────────────────────────────

let runCounter = 0;
function genRunId(): string {
  return `build_${Date.now().toString(36)}_${(runCounter++).toString(36)}`;
}

export function createBuilderManager(options: BuilderManagerOptions = {}): BuilderManager {
  const executor = options.executor ?? createDryRunExecutor();
  const profiles = new Map<string, AppProfile>();
  const runs = new Map<string, BuildRun>();
  const listeners = new Set<Listener>();
  let activeProfileId: string | null = null;

  // Seed with built-ins
  for (const profile of listBuiltInProfiles()) {
    profiles.set(profile.id, profile);
  }
  // Register any user-supplied profiles (may override built-ins intentionally)
  for (const profile of options.profiles ?? []) {
    profiles.set(profile.id, profile);
  }

  const builtInIds = new Set<BuiltInProfileId>([
    "studio",
    "flux",
    "lattice",
    "cadence",
    "grip",
    "relay",
  ]);

  function notify(): void {
    for (const fn of listeners) fn();
  }

  function listProfiles(): AppProfile[] {
    return [...profiles.values()];
  }

  function getProfile(id: string): AppProfile | undefined {
    return profiles.get(id);
  }

  function registerProfile(profile: AppProfile): void {
    profiles.set(profile.id, profile);
    notify();
  }

  function removeProfile(id: string): boolean {
    if (builtInIds.has(id as BuiltInProfileId)) return false;
    const removed = profiles.delete(id);
    if (removed) {
      if (activeProfileId === id) activeProfileId = null;
      notify();
    }
    return removed;
  }

  function getActiveProfile(): AppProfile | null {
    if (!activeProfileId) return null;
    return profiles.get(activeProfileId) ?? null;
  }

  function setActiveProfile(id: string | null): void {
    if (id !== null && !profiles.has(id)) {
      throw new Error(`Unknown profile: ${id}`);
    }
    activeProfileId = id;
    notify();
  }

  function planBuild(profileId: string, target: BuildTarget, dryRun = true): BuildPlan {
    const profile = profiles.get(profileId);
    if (!profile) {
      throw new Error(`Unknown profile: ${profileId}`);
    }
    return createBuildPlan({ profile, target, dryRun });
  }

  function planBuilds(profileId: string, targets: BuildTarget[], dryRun = true): BuildPlan[] {
    return targets.map((t) => planBuild(profileId, t, dryRun));
  }

  async function runPlan(plan: BuildPlan): Promise<BuildRun> {
    const run: BuildRun = {
      id: genRunId(),
      plan,
      startedAt: Date.now(),
      status: "running",
      steps: [],
      producedArtifacts: [],
    };
    runs.set(run.id, run);
    notify();

    const context: BuildExecutionContext = {
      workingDir: plan.workingDir,
      env: plan.env,
    };

    for (const step of plan.steps) {
      const result = await executor.executeStep(step, context);
      run.steps.push(result);
      if (result.status === "failed") {
        run.status = "failed";
        run.finishedAt = Date.now();
        notify();
        return run;
      }
      notify();
    }

    // In dry-run mode we always claim to have produced the declared artifacts,
    // because the whole point of a dry-run is to preview what WOULD be built.
    // In Tauri mode the daemon is responsible for verifying each artifact
    // exists on disk; for now we trust the step results.
    const allSkipped = run.steps.every((s) => s.status === "skipped");
    const producedArtifacts: ArtifactDescriptor[] = executor.mode === "dry-run" || !allSkipped
      ? plan.artifacts
      : [];

    run.producedArtifacts = producedArtifacts;
    run.status = run.steps.some((s) => s.status === "failed") ? "failed" : "success";
    run.finishedAt = Date.now();
    notify();
    return run;
  }

  function listRuns(): BuildRun[] {
    return [...runs.values()].sort((a, b) => b.startedAt - a.startedAt);
  }

  function getRun(id: string): BuildRun | undefined {
    return runs.get(id);
  }

  function clearRuns(): void {
    runs.clear();
    notify();
  }

  function subscribe(listener: Listener): () => void {
    listeners.add(listener);
    return () => listeners.delete(listener);
  }

  function dispose(): void {
    listeners.clear();
    runs.clear();
    profiles.clear();
  }

  return {
    targets: ALL_BUILD_TARGETS,
    listProfiles,
    getProfile,
    registerProfile,
    removeProfile,
    getActiveProfile,
    setActiveProfile,
    planBuild,
    planBuilds,
    runPlan,
    listRuns,
    getRun,
    clearRuns,
    subscribe,
    dispose,
  };
}

