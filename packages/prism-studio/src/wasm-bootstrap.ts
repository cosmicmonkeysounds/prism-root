/**
 * Browser-host bootstrap: load the emscripten-compiled prism-daemon
 * bundle and expose it as `window.__prismDaemon`, the global the
 * universal `ipc-bridge` looks for when it detects the WASM transport.
 *
 * Runs only in the browser SPA target — Tauri and Capacitor hosts
 * already have their own native daemon bridge and this bootstrap is a
 * no-op there (it early-returns before touching the DOM).
 *
 * The shape of `window.__prismDaemon` matches the WASM E2E harness at
 * `packages/prism-daemon/e2e/fixtures/harness.html` line-for-line —
 * same ccall wrappers, same envelope-returning `invoke`, same raw
 * pointer surface — so the Playwright tests double as regression
 * coverage for this bootstrap without any extra wiring.
 *
 * ## Why the bootstrap lives here, not in a Vite plugin
 *
 * The emscripten glue is ~50 KB of generated JS plus a ~3 MB .wasm
 * sidecar. We load it with `import(url)` against files served from the
 * site's own `/assets/` so the browser caches them the same way Vite
 * caches every other chunk. A Vite plugin would need to re-invent that
 * caching; a plain dynamic import gives it to us for free.
 */

type EmscriptenModule = {
    cwrap: (
        name: string,
        returnType: string | null,
        argTypes: string[],
    ) => (...args: unknown[]) => unknown;
    UTF8ToString: (ptr: number) => string;
    instantiateWasm?: (
        imports: WebAssembly.Imports,
        receiveInstance: (
            instance: WebAssembly.Instance,
            module: WebAssembly.Module,
        ) => void,
    ) => Record<string, never>;
};

type EmscriptenFactory = (
    overrides?: Partial<EmscriptenModule>,
) => Promise<EmscriptenModule>;

// Envelope shape `prism_daemon_invoke` always returns — matches
// `src/wasm.rs::into_leaked_cstring`.
type Envelope = { ok: boolean; result?: unknown; error?: string };

type PrismDaemonBridge = {
    createKernel: () => number;
    destroyKernel: (k: number) => void;
    invoke: (k: number, name: string, payload: unknown) => Envelope;
};

/**
 * Kick off WASM bootstrap. Returns a promise that resolves once
 * `window.__prismDaemon` is usable. Idempotent — multiple callers share
 * the same underlying boot.
 */
let bootPromise: Promise<void> | null = null;

export function bootWasmDaemon(): Promise<void> {
    if (bootPromise) return bootPromise;
    bootPromise = (async () => {
        if (typeof window === "undefined") return;
        const w = window as unknown as {
            __TAURI_INTERNALS__?: unknown;
            Capacitor?: { isNativePlatform?: () => boolean };
            __prismDaemon?: PrismDaemonBridge;
        };
        // Tauri / Capacitor hosts speak their own IPC — do not load
        // the browser bridge on those hosts, even if the assets are
        // reachable. The universal ipc-bridge's runtime detection will
        // pick the correct transport regardless.
        if (w.__TAURI_INTERNALS__) return;
        if (w.Capacitor?.isNativePlatform?.()) return;
        if (w.__prismDaemon) return;

        // The glue file is produced by `packages/prism-daemon/scripts/
        // build-wasm.sh`. It's staged into `dist-wasm/{dev,prod}/` by
        // that script; the Studio Vite config copies whichever profile
        // it wants into `public/daemon/` so the runtime fetches the
        // files from `/daemon/prism_daemon_wasm.{js,wasm}`.
        // String indirection hides the runtime-served path from tsc's
        // module resolver while leaving the /* @vite-ignore */ hint for
        // Vite's dynamic-import handling.
        const factoryPath = "/daemon/prism_daemon_wasm.js";
        const factory: EmscriptenFactory = (
            await import(/* @vite-ignore */ factoryPath)
        ).default;

        const Module = await factory({
            // Same instantiateWasm trick the E2E harness uses: loro-
            // internal 1.10 transitively pulls wasm-bindgen stubs that
            // reference an absent `__wbindgen_placeholder__` module, so
            // we compile the wasm first and fill any missing import
            // slots with no-ops before handing it back. See
            // e2e/fixtures/harness.html for the long-form comment.
            instantiateWasm(imports, receiveInstance) {
                (async () => {
                    const resp = await fetch("/daemon/prism_daemon_wasm.wasm");
                    const bytes = await resp.arrayBuffer();
                    const compiled = await WebAssembly.compile(bytes);
                    const declared = WebAssembly.Module.imports(compiled);
                    const needed = new Map<string, Set<string>>();
                    for (const imp of declared) {
                        if (!needed.has(imp.module)) {
                            needed.set(imp.module, new Set());
                        }
                        needed.get(imp.module)!.add(imp.name);
                    }
                    for (const [mod, names] of needed) {
                        const table =
                            (imports as Record<string, Record<string, unknown>>)[
                                mod
                            ] ?? {};
                        for (const name of names) {
                            if (!(name in table)) {
                                table[name] = () => 0;
                            }
                        }
                        (imports as Record<string, Record<string, unknown>>)[
                            mod
                        ] = table;
                    }
                    const instance = await WebAssembly.instantiate(
                        compiled,
                        imports,
                    );
                    receiveInstance(instance, compiled);
                })().catch((err) => {
                    console.error("[prism-wasm] instantiate failed", err);
                    throw err;
                });
                return {};
            },
        });

        const rawCreate = Module.cwrap(
            "prism_daemon_create",
            "number",
            [],
        ) as () => number;
        const rawDestroy = Module.cwrap("prism_daemon_destroy", null, [
            "number",
        ]) as (k: number) => void;
        const rawInvoke = Module.cwrap(
            "prism_daemon_invoke",
            "number",
            ["number", "string", "string"],
        ) as (k: number, name: string, payload: string) => number;
        const rawFreeString = Module.cwrap("prism_daemon_free_string", null, [
            "number",
        ]) as (ptr: number) => void;

        w.__prismDaemon = {
            createKernel: () => {
                const k = rawCreate();
                if (k === 0) throw new Error("prism_daemon_create returned null");
                return k;
            },
            destroyKernel: rawDestroy,
            invoke: (kernel, name, payload) => {
                const ptr = rawInvoke(kernel, name, JSON.stringify(payload ?? null));
                if (ptr === 0) {
                    return { ok: false, error: "prism_daemon_invoke returned null" };
                }
                try {
                    const json = Module.UTF8ToString(ptr);
                    return JSON.parse(json) as Envelope;
                } finally {
                    rawFreeString(ptr);
                }
            },
        };
    })();
    return bootPromise;
}
