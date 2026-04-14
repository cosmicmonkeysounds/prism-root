/**
 * Universal IPC bridge.
 *
 * Studio is a universal host: the same TSX tree runs inside Tauri on
 * desktop, inside Capacitor on iOS/Android, and as a plain SPA in a
 * browser tab that loads `prism-daemon` via the WASM C ABI. Every
 * frontend call to the daemon goes through `daemon.invoke(name,
 * payload)` in this file, and the transport is picked once at module
 * load based on what's actually present on the host global object.
 *
 * The four transports all speak the same envelope shape:
 *
 *     { ok: true,  result: <any> }
 *     { ok: false, error:  string }
 *
 * which matches `prism-daemon::wasm::prism_daemon_invoke` byte-for-byte
 * on WASM and Capacitor, and is produced by `commands::daemon_invoke`
 * on the Tauri side.
 *
 * Runtime detection order (first match wins):
 *   1. Tauri   — `window.__TAURI_INTERNALS__` is present
 *   2. Capacitor — `window.Capacitor?.isNativePlatform()` returns true
 *   3. WASM    — `window.__prismDaemon` was set by the harness/bootstrap
 *   4. No-op   — error on every invoke (dev-time SSR / unit tests)
 */

// ── Envelope shape ─────────────────────────────────────────────────────

export type DaemonEnvelope<T = unknown> =
    | { ok: true; result: T }
    | { ok: false; error: string };

export type DaemonTransport = "tauri" | "capacitor" | "wasm" | "none";

// ── Transport-specific invokers ────────────────────────────────────────
//
// Each returns a parsed envelope. They are intentionally untyped at the
// call site — the caller passes the expected result type as a generic
// argument to `daemon.invoke`. The three real transports differ only in
// where the bytes physically travel; the envelope shape is identical.

type RawInvoker = (
    name: string,
    payload: unknown,
) => Promise<DaemonEnvelope<unknown>>;

async function makeTauriInvoker(): Promise<RawInvoker> {
    // Tauri is the only transport where we import a real package —
    // `@tauri-apps/api/core` ships with the Studio bundle on every host,
    // but calling it on non-Tauri pages throws, so we only reach this
    // branch if runtime detection has already confirmed Tauri is present.
    const { invoke } = await import("@tauri-apps/api/core");
    return async (name, payload) => {
        // The Tauri command returns the already-shaped envelope, so we
        // just forward it. Errors at the transport layer (unregistered
        // command, serialization failure) surface as rejected promises;
        // we catch and re-envelope them so callers see one shape.
        try {
            const envelope = await invoke<DaemonEnvelope<unknown>>(
                "daemon_invoke",
                { name, payload },
            );
            return envelope;
        } catch (err) {
            return {
                ok: false,
                error: err instanceof Error ? err.message : String(err),
            };
        }
    };
}

async function makeCapacitorInvoker(): Promise<RawInvoker> {
    // Lazily register the native Capacitor plugin and wire it up.
    // The plugin is part of @prism/daemon (mobile/ios, mobile/android)
    // rather than a separate package — the bridge lives here because
    // this is the only consumer.
    const { registerPlugin } = await import("@capacitor/core");
    const PrismDaemon = registerPlugin<{
        invoke(options: {
            name: string;
            payloadJson: string;
        }): Promise<{ responseJson: string }>;
    }>("PrismDaemon");
    return async (name, payload) => {
        const { responseJson } = await PrismDaemon.invoke({
            name,
            payloadJson: JSON.stringify(payload ?? null),
        });
        return JSON.parse(responseJson) as DaemonEnvelope<unknown>;
    };
}

type WasmBridge = {
    createKernel: () => number;
    destroyKernel: (k: number) => void;
    invoke: (
        k: number,
        name: string,
        payload: unknown,
    ) => { ok: boolean; result?: unknown; error?: string };
};

async function makeWasmInvoker(): Promise<RawInvoker> {
    // The WASM bridge is set up during app bootstrap (see
    // `src/wasm-bootstrap.ts` / the harness-style loader). We pull a
    // long-lived kernel from `window.__prismDaemon` and reuse it across
    // calls — each Studio tab owns a single kernel, the same way mobile
    // and Tauri do.
    const bridge = (
        window as unknown as { __prismDaemon?: WasmBridge }
    ).__prismDaemon;
    if (!bridge) {
        throw new Error(
            "WASM transport selected but window.__prismDaemon is missing",
        );
    }
    const kernel = bridge.createKernel();
    return async (name, payload) => {
        const envelope = bridge.invoke(kernel, name, payload);
        if (envelope.ok) {
            return { ok: true, result: envelope.result };
        }
        return { ok: false, error: envelope.error ?? "unknown error" };
    };
}

function makeNoopInvoker(): RawInvoker {
    return async (name) => ({
        ok: false,
        error: `no daemon transport available (tried to invoke "${name}")`,
    });
}

// ── Runtime detection ──────────────────────────────────────────────────

function detectTransport(): DaemonTransport {
    if (typeof window === "undefined") return "none";
    const w = window as unknown as {
        __TAURI_INTERNALS__?: unknown;
        Capacitor?: { isNativePlatform?: () => boolean };
        __prismDaemon?: unknown;
    };
    if (w.__TAURI_INTERNALS__) return "tauri";
    if (w.Capacitor?.isNativePlatform?.()) return "capacitor";
    if (w.__prismDaemon) return "wasm";
    return "none";
}

// Lazy-resolved invoker. We detect the transport synchronously at module
// init but only instantiate the matching invoker the first time someone
// calls `daemon.invoke` — this keeps module load cost low and avoids
// pulling in unused transports.
let _invokerPromise: Promise<RawInvoker> | null = null;
let _transport: DaemonTransport = detectTransport();

function resolveInvoker(): Promise<RawInvoker> {
    if (_invokerPromise) return _invokerPromise;
    switch (_transport) {
        case "tauri":
            _invokerPromise = makeTauriInvoker();
            break;
        case "capacitor":
            _invokerPromise = makeCapacitorInvoker();
            break;
        case "wasm":
            _invokerPromise = makeWasmInvoker();
            break;
        case "none":
            _invokerPromise = Promise.resolve(makeNoopInvoker());
            break;
    }
    return _invokerPromise;
}

// ── Public facade ──────────────────────────────────────────────────────

/**
 * Force a specific transport. Only used by tests and the web-bootstrap
 * code that has to install the WASM bridge before anything else runs.
 * Production call sites never touch this.
 */
export function setDaemonTransport(transport: DaemonTransport): void {
    _transport = transport;
    _invokerPromise = null;
}

/**
 * Which transport the bridge resolved to. Useful for the status bar and
 * for conditional feature surfaces (e.g. hiding "spawn build step" on
 * mobile/browser hosts that can't run processes).
 */
export function currentTransport(): DaemonTransport {
    return _transport;
}

/**
 * The universal daemon entry point. Every capability — CRDT, Luau,
 * build, watcher, custom modules — is reachable through this one call.
 *
 * Command errors come back as `{ ok: false, error }` envelopes rather
 * than thrown exceptions, so callers can handle them exhaustively via
 * `switch (envelope.ok)`.
 */
export const daemon = {
    async invoke<T = unknown>(
        name: string,
        payload: unknown = null,
    ): Promise<DaemonEnvelope<T>> {
        const invoker = await resolveInvoker();
        return (await invoker(name, payload)) as DaemonEnvelope<T>;
    },
};

