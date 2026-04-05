/**
 * @prism/core — FeatureFlags
 *
 * Boolean toggles evaluated against config values.
 * Composable with ConfigModel: a flag can delegate to a config key for override.
 *
 * Resolution order:
 *   1. config.get(flag.settingKey) — if defined, use it (highest specificity)
 *   2. Evaluate conditions in order — first match wins
 *   3. flag.default — fallback
 */

import type {
  FeatureFlagContext,
  FeatureFlagCondition,
} from "./config-types.js";
import type { ConfigRegistry } from "./config-registry.js";
import type { ConfigModel } from "./config-model.js";

export class FeatureFlags {
  private _watchers = new Map<string, Set<(enabled: boolean) => void>>();

  constructor(
    private _registry: ConfigRegistry,
    private _config: ConfigModel,
  ) {
    this._config.on("change", (change) => {
      for (const def of this._registry.allFlags()) {
        if (def.settingKey === change.key) {
          const enabled = this.isEnabled(def.id);
          for (const w of this._watchers.get(def.id) ?? []) w(enabled);
        }
      }
    });
  }

  /**
   * Returns true if the feature flag is enabled for the given context.
   */
  isEnabled(flagId: string, context: FeatureFlagContext = {}): boolean {
    const def = this._registry.getFlag(flagId);
    if (!def) return false;

    // 1. Config override.
    if (def.settingKey) {
      const override = this._config.get<boolean | undefined>(def.settingKey);
      if (override !== undefined && typeof override === "boolean")
        return override;
    }

    // 2. Condition evaluation.
    if (def.conditions) {
      for (const cond of def.conditions) {
        const result = evaluateCondition(cond, context);
        if (result !== null) return result;
      }
    }

    // 3. Default.
    return def.default;
  }

  /**
   * Returns a map of flagId -> boolean for all registered flags.
   */
  getAll(context: FeatureFlagContext = {}): Record<string, boolean> {
    const out: Record<string, boolean> = {};
    for (const def of this._registry.allFlags()) {
      out[def.id] = this.isEnabled(def.id, context);
    }
    return out;
  }

  /**
   * Watch a feature flag for changes triggered by config mutations.
   * Called immediately with current value, then on every change.
   */
  watch(
    flagId: string,
    callback: (enabled: boolean) => void,
    context: FeatureFlagContext = {},
  ): () => void {
    if (!this._watchers.has(flagId)) {
      this._watchers.set(flagId, new Set());
    }
    const wrapped = () => callback(this.isEnabled(flagId, context));
    const watcherSet = this._watchers.get(flagId);
    if (watcherSet) watcherSet.add(wrapped);

    // Immediate call.
    callback(this.isEnabled(flagId, context));

    return () => this._watchers.get(flagId)?.delete(wrapped);
  }
}

// ── Condition evaluator ─────────────────────────────────────────────────────────

function evaluateCondition(
  cond: FeatureFlagCondition,
  ctx: FeatureFlagContext,
): boolean | null {
  switch (cond.type) {
    case "always":
      return cond.value;

    case "config":
      if (!ctx.config) return null;
      return deepEqual(ctx.config[cond.key], cond.equals) ? cond.value : null;

    default:
      return null;
  }
}

function deepEqual(a: unknown, b: unknown): boolean {
  if (a === b) return true;
  if (typeof a !== typeof b) return false;
  if (typeof a !== "object" || a === null || b === null) return false;
  const aK = Object.keys(a as object);
  const bK = Object.keys(b as object);
  if (aK.length !== bK.length) return false;
  return aK.every((k) =>
    deepEqual(
      (a as Record<string, unknown>)[k],
      (b as Record<string, unknown>)[k],
    ),
  );
}
