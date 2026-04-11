// TypeScript facade for the Prism daemon Capacitor plugin.
//
// The native side (Swift on iOS, Kotlin on Android) wraps the C ABI
// exported from `libprism_daemon.a`:
//
//   prism_daemon_create()   -> kernel handle
//   prism_daemon_invoke(kernel, name, payloadJson) -> response JSON
//   prism_daemon_free_string(ptr)
//   prism_daemon_destroy(kernel)
//
// On both platforms a single kernel is lazily created on first `invoke`
// and torn down on plugin `load()`/app teardown — there's no multi-kernel
// use case on mobile (each Studio shell is one kernel). We expose only
// `invoke(name, payload)` to JS so the surface matches the web
// `ipc-bridge` shape: one polymorphic call, JSON in / JSON out.

import { registerPlugin } from "@capacitor/core";

/**
 * Envelope returned by every `prism_daemon_invoke` call. Matches
 * `src/wasm.rs::into_leaked_cstring`'s shape.
 */
export type DaemonEnvelope<T = unknown> =
    | { ok: true; result: T }
    | { ok: false; error: string };

/**
 * Capacitor plugin interface. Both iOS and Android implement `invoke` by
 * forwarding to the C ABI. `payloadJson` is a JSON string (not an object)
 * because Capacitor's bridge would otherwise re-serialize it per-field on
 * its way across; passing a pre-serialized string avoids the double hop.
 */
export interface PrismDaemonPlugin {
    /**
     * Invoke a registered command on the local daemon kernel. Returns the
     * parsed envelope — callers that want `result` can switch on `ok`.
     *
     * Throws only on transport-level failure (JSON parse error on the
     * native side, kernel failed to create). Command-level errors come
     * back as `{ ok: false, error }` and are not thrown.
     */
    invoke(options: { name: string; payloadJson: string }): Promise<{
        responseJson: string;
    }>;
}

const PrismDaemon = registerPlugin<PrismDaemonPlugin>("PrismDaemon");

/**
 * High-level helper: serialize `payload`, call the native plugin, parse
 * the response envelope. This is the single function Studio's
 * `ipc-bridge` calls on Capacitor hosts — same shape as the Tauri and
 * WASM transports.
 */
export async function invokeDaemon<T = unknown>(
    name: string,
    payload: unknown = null,
): Promise<DaemonEnvelope<T>> {
    const { responseJson } = await PrismDaemon.invoke({
        name,
        payloadJson: JSON.stringify(payload ?? null),
    });
    return JSON.parse(responseJson) as DaemonEnvelope<T>;
}

export default PrismDaemon;
