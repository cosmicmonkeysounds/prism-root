//! Daemon sidecar lifecycle. Dev builds spawn an in-tree
//! `prism-daemon` process; packaged builds will use Tauri's sidecar
//! plugin to spawn the signed binary next to the app bundle.
//!
//! IPC goes over `interprocess` (unix domain sockets / named pipes
//! depending on platform) carrying length-prefixed `postcard` frames.
//! Phase-0 stub — the real client/server handshake lands after spike
//! #6.

pub fn spawn_dev() {
    // TODO(phase0-spike-6): spawn the in-tree daemon binary and
    // attach an `interprocess::local_socket` client. For now this is
    // a no-op so the shell can boot without the full sidecar path.
}
