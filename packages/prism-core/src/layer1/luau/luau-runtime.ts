/**
 * Browser-side Luau runtime via luau-web.
 * The same .luau scripts run here (browser) and in mlua (daemon).
 */

import { LuauState } from "luau-web";
import type { LuauResult } from "@prism/shared/types";

export type { LuauState as LuauEngine };

/**
 * Recursively convert a luau-web LuauTable (duck-typed: has keys() + get())
 * into a plain JS object / array so callers get serialisable values.
 * Arrays with sequential integer keys are kept as arrays (luau-web already
 * converts them when LUA_IMPLICIT_ARRAYS_TO_JS_ARRAYS is true, but this
 * handles the dictionary-table case that the runtime leaves as a proxy).
 */
export function fromLuauValue(val: unknown): unknown {
  if (val === null || typeof val !== "object") return val;
  const obj = val as Record<string | symbol, unknown>;
  if (typeof obj["keys"] === "function" && typeof obj["get"] === "function") {
    const table = obj as { keys(): unknown[]; get(k: unknown): unknown };
    const keys = table.keys();
    const result: Record<string, unknown> = {};
    for (const k of keys) {
      result[String(k)] = fromLuauValue(table.get(k));
    }
    return result;
  }
  if (Array.isArray(val)) return val.map(fromLuauValue);
  return val;
}

/**
 * Execute a Luau script in the browser via luau-web.
 * Optionally inject global variables from the args map.
 */
export async function executeLuau(
  script: string,
  args?: Record<string, unknown>,
): Promise<LuauResult> {
  try {
    const state = await LuauState.createAsync(args ?? {});
    const fn = state.loadstring(script, "script", true);
    const results = await fn();
    // luau-web returns an array of multi-return values; take the first
    const raw = Array.isArray(results) ? (results[0] ?? null) : (results ?? null);
    return { success: true, value: fromLuauValue(raw) };
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return { success: false, value: null, error: message };
  }
}

/**
 * Create a persistent Luau state for long-running contexts.
 * Globals passed here are available to all subsequent loadstring calls.
 */
export async function createLuauEngine(
  globals?: Record<string, unknown>,
): Promise<LuauState> {
  return LuauState.createAsync(globals ?? {});
}
