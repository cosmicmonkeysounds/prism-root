/**
 * AutomationEngine — orchestrates trigger evaluation and action dispatch.
 *
 * Architecture:
 *   - Object triggers: caller fires engine.handleObjectEvent() on store mutations
 *   - Cron triggers: engine sets up timer loops
 *   - Manual triggers: caller fires engine.run()
 *   - Actions: dispatched to registered ActionHandler implementations
 *
 * The engine orchestrates — it doesn't implement. Action handlers are
 * provided by the app layer.
 */

import type {
  Automation,
  AutomationContext,
  AutomationRun,
  ActionResult,
  ActionHandlerMap,
  ObjectEvent,
} from "./automation-types.js";
import {
  evaluateCondition,
  interpolate,
  matchesObjectTrigger,
} from "./condition-evaluator.js";

// ── Store interface ──────────────────────────────────────────────────────────

export interface AutomationStore {
  list(
    filter?: { enabled?: boolean | undefined; triggerType?: string | undefined } | undefined,
  ): Automation[];
  get(id: string): Automation | undefined;
  save(automation: Automation): void;
  saveRun(run: AutomationRun): void;
}

// ── Options ──────────────────────────────────────────────────────────────────

export interface AutomationEngineOptions {
  /** Called when an automation run completes. */
  onRunComplete?: ((run: AutomationRun) => void) | undefined;
  /** Override Date.now() for testing. */
  now?: (() => number) | undefined;
}

// ── Cron interval parser (simple patterns) ──────────────────────────────────

function parseCronToInterval(cron: string): number | null {
  const parts = cron.trim().split(/\s+/);
  if (parts.length !== 5) return null;
  const minute = parts[0] as string;

  if (cron === "* * * * *") return 60_000;
  if (minute.startsWith("*/")) return parseInt(minute.slice(2)) * 60_000;
  if (minute === "0" && parts[1] === "*") return 3_600_000;
  if (minute === "0" && parts[1] === "0") return 86_400_000;
  return 60_000;
}

// ── Utilities ────────────────────────────────────────────────────────────────

function randomId(): string {
  const bytes = new Uint8Array(8);
  if (typeof globalThis.crypto?.getRandomValues === "function") {
    globalThis.crypto.getRandomValues(bytes);
  } else {
    for (let i = 0; i < 8; i++) bytes[i] = Math.floor(Math.random() * 256);
  }
  return Array.from(bytes)
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

// ── AutomationEngine ────────────────────────────────────────────────────────

export class AutomationEngine {
  private _cronTimers = new Map<string, ReturnType<typeof setInterval>>();
  private _running = false;

  constructor(
    private _store: AutomationStore,
    private _handlers: ActionHandlerMap,
    private _options: AutomationEngineOptions = {},
  ) {}

  get running(): boolean {
    return this._running;
  }

  // ── Lifecycle ─────────────────────────────────────────────────────────────

  /** Activate cron triggers for all enabled automations. */
  start(): void {
    if (this._running) return;
    this._running = true;
    const automations = this._store.list({ enabled: true });
    for (const a of automations) {
      if (a.trigger.type === "cron") this._scheduleCron(a);
    }
  }

  /** Stop all cron timers. */
  stop(): void {
    for (const timer of this._cronTimers.values()) clearInterval(timer);
    this._cronTimers.clear();
    this._running = false;
  }

  /** Re-activate or update cron for a single automation after save. */
  refreshAutomation(automationId: string): void {
    const existing = this._cronTimers.get(automationId);
    if (existing) {
      clearInterval(existing);
      this._cronTimers.delete(automationId);
    }

    const automation = this._store.get(automationId);
    if (automation?.enabled && automation.trigger.type === "cron") {
      this._scheduleCron(automation);
    }
  }

  // ── Trigger handlers ──────────────────────────────────────────────────────

  /** Handle an object lifecycle event. */
  async handleObjectEvent(event: ObjectEvent): Promise<AutomationRun[]> {
    const automations = this._store.list({
      enabled: true,
      triggerType: event.type,
    });
    const runs = await Promise.all(
      automations
        .filter((a) => a.trigger.type === event.type)
        .filter((a) =>
          matchesObjectTrigger(
            a.trigger as {
              objectTypes?: string[] | undefined;
              tags?: string[] | undefined;
              fieldMatch?: Record<string, unknown> | undefined;
            },
            event,
          ),
        )
        .map((a) =>
          this._execute(a, {
            automationId: a.id,
            triggeredAt: this._now(),
            triggerType: event.type,
            object: event.object,
            previousObject: event.previous,
          }),
        ),
    );
    return runs;
  }

  /** Manually trigger an automation. */
  async run(
    automationId: string,
    extra?: Record<string, unknown>,
  ): Promise<AutomationRun> {
    const automation = this._store.get(automationId);
    if (!automation) throw new Error(`Automation '${automationId}' not found`);
    return this._execute(automation, {
      automationId,
      triggeredAt: this._now(),
      triggerType: "manual",
      extra,
    });
  }

  // ── Execution ─────────────────────────────────────────────────────────────

  private async _execute(
    automation: Automation,
    ctx: AutomationContext,
  ): Promise<AutomationRun> {
    const runId = randomId();

    // Evaluate conditions.
    let conditionPassed = true;
    try {
      conditionPassed = automation.conditions.every((c) =>
        evaluateCondition(c, ctx),
      );
    } catch {
      conditionPassed = false;
    }

    const run: AutomationRun = {
      id: runId,
      automationId: automation.id,
      status: "skipped",
      triggeredAt: ctx.triggeredAt,
      conditionPassed,
      actionResults: [],
    };

    if (!conditionPassed) {
      run.status = "skipped";
      this._finalise(run, automation);
      return run;
    }

    // Execute actions in sequence.
    const results: ActionResult[] = [];
    let failed = false;

    for (let i = 0; i < automation.actions.length; i++) {
      const action = automation.actions[i] as (typeof automation.actions)[number];
      const actionStart = Date.now();

      if (action.type === "delay") {
        await sleep((action as { type: "delay"; seconds: number }).seconds * 1000);
        results.push({
          actionIndex: i,
          actionType: action.type,
          status: "success",
          elapsedMs: Date.now() - actionStart,
        });
        continue;
      }

      const handler = this._handlers[action.type];
      if (!handler) {
        results.push({
          actionIndex: i,
          actionType: action.type,
          status: "skipped",
          error: `No handler registered for action type '${action.type}'`,
        });
        continue;
      }

      try {
        await handler(interpolate(action, ctx), ctx);
        results.push({
          actionIndex: i,
          actionType: action.type,
          status: "success",
          elapsedMs: Date.now() - actionStart,
        });
      } catch (err) {
        results.push({
          actionIndex: i,
          actionType: action.type,
          status: "failed",
          error: String(err),
          elapsedMs: Date.now() - actionStart,
        });
        failed = true;
        break;
      }
    }

    run.actionResults = results;
    run.status = failed
      ? results.some((r) => r.status === "success")
        ? "partial"
        : "failed"
      : "success";

    this._finalise(run, automation);
    return run;
  }

  private _finalise(run: AutomationRun, automation: Automation): void {
    run.completedAt = new Date(
      this._options.now?.() ?? Date.now(),
    ).toISOString();
    this._store.saveRun(run);

    if (run.status === "success") {
      this._store.save({
        ...automation,
        lastRunAt: run.completedAt,
        runCount: automation.runCount + 1,
        updatedAt: run.completedAt,
      });
    }

    this._options.onRunComplete?.(run);
  }

  // ── Cron scheduling ───────────────────────────────────────────────────────

  private _scheduleCron(automation: Automation): void {
    const trigger = automation.trigger as { type: "cron"; cron: string };
    const intervalMs = parseCronToInterval(trigger.cron);
    if (!intervalMs) return;

    const timer = setInterval(() => {
      const fresh = this._store.get(automation.id);
      if (!fresh?.enabled) {
        clearInterval(timer);
        this._cronTimers.delete(automation.id);
        return;
      }
      void this._execute(fresh, {
        automationId: fresh.id,
        triggeredAt: this._now(),
        triggerType: "cron",
      });
    }, intervalMs);

    this._cronTimers.set(automation.id, timer);
  }

  private _now(): string {
    return new Date(this._options.now?.() ?? Date.now()).toISOString();
  }
}
