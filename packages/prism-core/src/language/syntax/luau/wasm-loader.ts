/**
 * WASM loader for the full-moon-backed Luau parser.
 *
 * Handles the initialization dance for both the browser (Vite / Tauri webview)
 * and Node (Vitest). The wasm-pack output calls `fetch(new URL(...))` by
 * default, which breaks under Node because Node's built-in `fetch` does not
 * support `file://` URLs. So we read the bytes ourselves and hand them to the
 * generated `default` init function, which accepts `BufferSource` directly.
 *
 * Initialization is idempotent: once the WASM module is instantiated, every
 * subsequent call short-circuits to the cached module. Callers can safely
 * await `ensureLuauParserLoaded()` from anywhere; the first call does the
 * work, every other call resolves immediately.
 */

import init, * as wasmExports from "./pkg/prism_luau_parser.js";

type WasmModule = typeof wasmExports;

let modulePromise: Promise<WasmModule> | null = null;
let cachedModule: WasmModule | null = null;

/**
 * Ensure the Luau parser WASM module is instantiated. Idempotent — returns
 * the same promise on every call until it resolves, then returns a resolved
 * promise forever after.
 */
export async function ensureLuauParserLoaded(): Promise<WasmModule> {
  if (cachedModule) return cachedModule;
  if (!modulePromise) {
    modulePromise = loadWasmModule().then((mod) => {
      cachedModule = mod;
      return mod;
    });
  }
  return modulePromise;
}

/**
 * Synchronous accessor for code paths that have already awaited initialization.
 * Throws if the module has not been loaded yet.
 */
export function getLuauParserSync(): WasmModule {
  if (!cachedModule) {
    throw new Error(
      "Luau parser not initialized — call ensureLuauParserLoaded() first",
    );
  }
  return cachedModule;
}

/** `true` once the WASM module is loaded and ready for sync access. */
export function isLuauParserReady(): boolean {
  return cachedModule !== null;
}

// ── Internal: environment-aware bootstrap ────────────────────────────────────

async function loadWasmModule(): Promise<WasmModule> {
  const bytes = await loadWasmBytes();
  // wasm-bindgen 0.2.100+ expects a single options object rather than
  // positional args. Passing `{ module_or_path }` silences the deprecation
  // warning and is forward-compatible with the generated glue.
  await init({ module_or_path: bytes });
  return wasmExports;
}

async function loadWasmBytes(): Promise<ArrayBuffer> {
  const wasmUrl = new URL("./pkg/prism_luau_parser_bg.wasm", import.meta.url);

  // Node (Vitest) path: built-in `fetch` doesn't speak `file://`, so read
  // the file ourselves. We detect Node via `process.versions.node` — this
  // avoids relying on `typeof window` which is mocked in some test setups.
  const isNode =
    typeof process !== "undefined" &&
    typeof (process as { versions?: { node?: string } }).versions?.node ===
      "string";

  if (isNode) {
    const { readFile } = await import("node:fs/promises");
    const buf = await readFile(wasmUrl);
    // Detach the underlying ArrayBuffer so `WebAssembly.instantiate` gets
    // exactly the range we care about (node Buffers can be views over a
    // larger pool).
    return buf.buffer.slice(
      buf.byteOffset,
      buf.byteOffset + buf.byteLength,
    ) as ArrayBuffer;
  }

  // Browser / Tauri webview: use fetch, which Vite rewrites at build time
  // to the correct asset URL.
  const response = await fetch(wasmUrl);
  return response.arrayBuffer();
}
