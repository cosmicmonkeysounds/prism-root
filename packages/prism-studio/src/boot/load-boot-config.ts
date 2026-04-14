/**
 * Boot config resolver.
 *
 * Studio picks up its runtime `BootConfig` from one of four sources,
 * in precedence order:
 *
 *   1. Query params on the current URL (`?profile=…&mode=…&permission=…`).
 *      Browser-only; these are visible to the user and therefore trusted
 *      only for *narrowing* the build-time ceiling. A URL that tries to
 *      escalate permission beyond the build-time default is clamped.
 *
 *   2. `VITE_PRISM_BOOT_CONFIG` Vite env var. Injected by the `prism-
 *      studio` CLI at spawn time: the CLI writes a boot.json, stringifies
 *      it, and passes it to `vite`/`tauri` through the env. Vite's
 *      default env behaviour exposes any `VITE_*` variable on
 *      `import.meta.env`, so no vite.config changes are needed.
 *
 *   3. Build-time default from `import.meta.env.VITE_PRISM_BOOT_DEFAULT`
 *      (optional). Capacitor / mobile builds set this to pin a
 *      permission ceiling that query params can't exceed.
 *
 *   4. `DEFAULT_BOOT_CONFIG` from `@prism/core/lens` — full IDE at dev
 *      permission. This is the unchanged pre-mode-system behaviour.
 *
 * The resolver is deliberately synchronous — it runs once at module load
 * so the kernel can read the result inside its synchronous factory.
 * Parse errors log a warning and fall through to the next source rather
 * than throwing, so a malformed query param never bricks Studio.
 */

import type { BootConfig, ResolvedBootConfig } from "@prism/core/lens";
import {
  DEFAULT_BOOT_CONFIG,
  isPermission,
  isShellMode,
  resolveBootConfig,
} from "@prism/core/lens";

// ── Vite env shim ───────────────────────────────────────────────────────────
// `import.meta.env` is populated by Vite at build time. In vitest the
// same variable exists but without our VITE_PRISM_* keys. We read it
// defensively so tests running under node don't blow up on a missing
// `import.meta.env`.

interface ViteEnvShape {
  readonly VITE_PRISM_BOOT_CONFIG?: string;
  readonly VITE_PRISM_BOOT_DEFAULT?: string;
}

function readViteEnv(): ViteEnvShape {
  try {
    const env = (import.meta as unknown as { env?: ViteEnvShape }).env;
    return env ?? {};
  } catch {
    return {};
  }
}

// ── Parser helpers ──────────────────────────────────────────────────────────

function safeJsonParse(raw: string): BootConfig | null {
  try {
    const parsed = JSON.parse(raw) as unknown;
    if (parsed && typeof parsed === "object") return parsed as BootConfig;
    return null;
  } catch (err) {
    console.warn("[prism-studio] failed to parse boot config JSON", err);
    return null;
  }
}

function readQueryBootConfig(): BootConfig | null {
  if (typeof window === "undefined") return null;
  const params = new URLSearchParams(window.location.search);
  const out: Record<string, unknown> = {};
  const profile = params.get("profile");
  if (profile) out.profile = profile;
  const mode = params.get("mode");
  if (mode) {
    if (!isShellMode(mode)) {
      console.warn(`[prism-studio] query param mode=${mode} ignored`);
    } else {
      out.shellMode = mode;
    }
  }
  const permission = params.get("permission");
  if (permission) {
    if (!isPermission(permission)) {
      console.warn(`[prism-studio] query param permission=${permission} ignored`);
    } else {
      out.permission = permission;
    }
  }
  return Object.keys(out).length > 0 ? (out as BootConfig) : null;
}

function readEnvBootConfig(): BootConfig | null {
  const env = readViteEnv();
  const raw = env.VITE_PRISM_BOOT_CONFIG;
  if (!raw) return null;
  return safeJsonParse(raw);
}

function readBuildTimeDefault(): BootConfig | null {
  const env = readViteEnv();
  const raw = env.VITE_PRISM_BOOT_DEFAULT;
  if (!raw) return null;
  return safeJsonParse(raw);
}

// ── Permission clamp ────────────────────────────────────────────────────────
// The build-time ceiling is the maximum permission a runtime override is
// allowed to reach. For browser builds the CLI bakes `permission: "user"`
// into VITE_PRISM_BOOT_DEFAULT so a hand-crafted `?permission=dev` URL
// can't escalate. Tauri/desktop builds ship without a ceiling and the
// env var is authoritative.

function clampToCeiling(
  merged: BootConfig,
  ceiling: BootConfig | null,
): BootConfig {
  if (!ceiling) return merged;
  if (ceiling.permission === "user" && merged.permission === "dev") {
    console.warn(
      "[prism-studio] build-time ceiling blocked permission escalation to dev",
    );
    return { ...merged, permission: "user" };
  }
  return merged;
}

// ── Public API ──────────────────────────────────────────────────────────────

/**
 * Resolve the effective boot config for this Studio instance. Pure —
 * safe to call from a module-scope const.
 *
 * Optional `overrides` are useful in tests: pass them to short-circuit
 * query/env/build-time reads. Production callers pass nothing.
 */
export function loadBootConfig(overrides?: {
  readonly query?: BootConfig | null;
  readonly env?: BootConfig | null;
  readonly buildTime?: BootConfig | null;
}): ResolvedBootConfig {
  const buildTime =
    overrides?.buildTime !== undefined
      ? overrides.buildTime
      : readBuildTimeDefault();
  const envBoot =
    overrides?.env !== undefined ? overrides.env : readEnvBootConfig();
  const queryBoot =
    overrides?.query !== undefined ? overrides.query : readQueryBootConfig();

  const merged: BootConfig = {
    ...(buildTime ?? {}),
    ...(envBoot ?? {}),
    ...(queryBoot ?? {}),
  };
  const clamped = clampToCeiling(merged, buildTime);
  return resolveBootConfig(clamped);
}

/** Re-export so callers don't need a second import from `@prism/core/lens`. */
export { DEFAULT_BOOT_CONFIG };
export type { BootConfig, ResolvedBootConfig };
