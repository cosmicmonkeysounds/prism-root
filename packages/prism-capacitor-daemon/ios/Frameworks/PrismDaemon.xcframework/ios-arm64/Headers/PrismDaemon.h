#ifndef PRISM_DAEMON_H
#define PRISM_DAEMON_H

/**
 * Prism Daemon C ABI — the four functions Swift and Kotlin both wrap.
 *
 * Implementation lives in `packages/prism-daemon/src/wasm.rs`
 * (`#[no_mangle] pub extern "C"`). Linked into the Capacitor plugin as
 * `libprism_daemon.a` via Cargo's `staticlib` crate-type with the
 * `mobile` feature active (CRDT + Luau, no watcher, no build spawns).
 *
 * Ownership rules (repeat of the Rust doc comments — Swift/Kotlin code
 * must honor them):
 *
 *   - `prism_daemon_create` hands out an owning kernel handle. The
 *     caller must pair it with exactly one `prism_daemon_destroy`.
 *   - `prism_daemon_invoke` returns a freshly allocated, nul-terminated
 *     UTF-8 string. The caller must free it with
 *     `prism_daemon_free_string`. Do NOT call `free()` directly — the
 *     Rust-side allocator is not the host's.
 *   - All three `_invoke`-taking pointers (`name`, `payload_json`) must
 *     be valid nul-terminated UTF-8 for the duration of the call.
 *   - Passing a NULL kernel pointer is safe: the adapter returns an
 *     error envelope instead of dereferencing.
 */

#ifdef __cplusplus
extern "C" {
#endif

typedef struct PrismDaemonKernel PrismDaemonKernel;

PrismDaemonKernel* prism_daemon_create(void);

void prism_daemon_destroy(PrismDaemonKernel* kernel);

char* prism_daemon_invoke(
    PrismDaemonKernel* kernel,
    const char* name,
    const char* payload_json
);

void prism_daemon_free_string(char* ptr);

#ifdef __cplusplus
}
#endif

#endif /* PRISM_DAEMON_H */
