/**
 * Browser-side Lua 5.4 runtime via wasmoon.
 * The same .lua scripts run here (browser) and in mlua (daemon).
 */

import { LuaFactory, LuaEngine } from "wasmoon";
import type { LuaResult } from "@prism/shared/types";

let factory: LuaFactory | null = null;

/** Get or create the shared LuaFactory (loads WASM once). */
async function getFactory(): Promise<LuaFactory> {
  if (!factory) {
    factory = new LuaFactory();
  }
  return factory;
}

/**
 * Execute a Lua 5.4 script in the browser via wasmoon.
 * Optionally inject global variables from the args map.
 */
export async function executeLua(
  script: string,
  args?: Record<string, unknown>,
): Promise<LuaResult> {
  let engine: LuaEngine | null = null;
  try {
    const f = await getFactory();
    engine = await f.createEngine();

    // Inject arguments as globals
    if (args) {
      for (const [key, value] of Object.entries(args)) {
        engine.global.set(key, value);
      }
    }

    const result = await engine.doString(script);

    return { success: true, value: result };
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return { success: false, value: null, error: message };
  } finally {
    engine?.global.close();
  }
}

/**
 * Create a persistent Lua engine for long-running contexts.
 * Caller is responsible for closing via engine.global.close().
 */
export async function createLuaEngine(): Promise<LuaEngine> {
  const f = await getFactory();
  return f.createEngine();
}
