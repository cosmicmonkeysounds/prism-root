/**
 * Tauri IPC bridge — typed wrappers around invoke() calls.
 * All frontend <-> daemon communication goes through here.
 * Never raw HTTP.
 */

import { invoke } from "@tauri-apps/api/core";
import type { LuaResult } from "@prism/shared/types";

/** Write a key-value pair to a CRDT document on the daemon. */
export async function ipcCrdtWrite(
  docId: string,
  key: string,
  value: string,
): Promise<Uint8Array> {
  return invoke<number[]>("crdt_write", {
    docId,
    key,
    value,
  }).then((arr) => new Uint8Array(arr));
}

/** Read a value from a CRDT document on the daemon. */
export async function ipcCrdtRead(
  docId: string,
  key: string,
): Promise<string | null> {
  return invoke<string | null>("crdt_read", { docId, key });
}

/** Export a CRDT document snapshot from the daemon. */
export async function ipcCrdtExport(docId: string): Promise<Uint8Array> {
  return invoke<number[]>("crdt_export", { docId }).then(
    (arr) => new Uint8Array(arr),
  );
}

/** Execute a Lua script on the daemon via mlua. */
export async function ipcLuaExec(
  script: string,
  args?: Record<string, unknown>,
): Promise<LuaResult> {
  try {
    const value = await invoke<unknown>("lua_exec", { script, args });
    return { success: true, value };
  } catch (err) {
    return {
      success: false,
      value: null,
      error: err instanceof Error ? err.message : String(err),
    };
  }
}
