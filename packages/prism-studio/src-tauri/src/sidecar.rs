//! Daemon sidecar lifecycle and IPC client.
//!
//! Per §4.5 of the Clay migration plan (Option C, resolved by Phase 0
//! spikes #5/#6 on 2026-04-14 and revised 2026-04-15), Studio talks
//! to `prism-daemon` as a sibling process rather than linking it as
//! a library. The wire protocol is length-prefixed `postcard` frames
//! over `interprocess::local_socket` — unix domain sockets on Linux
//! and macOS, named pipes on Windows, hidden behind one trait. The
//! wire types themselves (`IpcRequest`, `IpcResponse`) live in
//! `prism_daemon::transport::ipc_local`; this file is the client
//! half.
//!
//! ### Dev vs. packaged
//!
//! In `cargo run -p prism-studio` / `prism dev studio`, the Studio
//! binary lives next to the `prism-daemond` binary in
//! `target/<profile>/`, so [`spawn_dev`] reaches sideways through
//! [`std::env::current_exe`] to find it. The packaged path — dropping
//! the signed `prism-daemond-<triple>` next to the Studio bundle via
//! a `cargo-packager` resource entry — is a Phase 5 concern; Phase 0
//! only had to prove the IPC transport works end to end, not the
//! full packaging pipeline.
//!
//! ### Supervision
//!
//! The returned [`DaemonSidecar`] owns the child handle and the IPC
//! stream. Dropping it kills the child unconditionally via
//! [`std::process::Child::kill`] + [`Child::wait`], so Studio's
//! shutdown path can simply let the handle fall out of scope. The
//! main binary stashes the handle inside the `Option<DaemonSidecar>`
//! captured by the event-loop closure so it lives exactly as long as
//! the window does.

use std::io::{self};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context};
use interprocess::local_socket::Stream;
use prism_daemon::{connect_client, read_frame, write_frame, IpcRequest, IpcResponse};
use serde_json::Value as JsonValue;

/// How long we wait for the daemon to bind its listener before giving
/// up and killing the child. Binding a local socket is essentially
/// instantaneous — we only retry at all to paper over the race between
/// `Command::spawn` returning and the server thread getting as far as
/// `ListenerOptions::create_sync`.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
const CONNECT_RETRY_INTERVAL: Duration = Duration::from_millis(25);

/// A live daemon sidecar plus the IPC stream Studio talks to it over.
///
/// The struct owns both the [`Child`] process handle and the connected
/// [`Stream`]. Dropping it kills the child and waits for the exit —
/// this is the "supervise + kill" half of Phase 0 spike #6.
pub struct DaemonSidecar {
    child: Child,
    stream: Stream,
    next_id: u64,
    /// The display name the daemon is listening on. Kept for error
    /// messages and for the `socket_name()` accessor.
    socket_name: String,
}

impl DaemonSidecar {
    /// Invoke one kernel command, wait for the reply, and return the
    /// parsed JSON result. Blocking on purpose: the Studio frame loop
    /// already runs on a dedicated thread and the kernel is sync all
    /// the way down, so there's nothing to gain from an async shim.
    #[allow(dead_code)]
    pub fn invoke(&mut self, command: &str, payload: JsonValue) -> anyhow::Result<JsonValue> {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1).max(1);

        let req = IpcRequest {
            id,
            command: command.to_string(),
            payload_json: payload.to_string(),
        };
        write_frame(&mut self.stream, &req)
            .with_context(|| format!("ipc write failed for {command}"))?;

        let resp: IpcResponse = read_frame(&mut self.stream)
            .with_context(|| format!("ipc read failed for {command}"))?
            .ok_or_else(|| anyhow!("daemon hung up while waiting for {command} reply"))?;

        if resp.id != id {
            bail!(
                "ipc id mismatch for {command}: expected {id}, got {}",
                resp.id
            );
        }

        if resp.ok {
            match resp.payload_json {
                Some(s) => serde_json::from_str(&s)
                    .with_context(|| format!("failed to parse {command} result JSON")),
                None => Ok(JsonValue::Null),
            }
        } else {
            Err(anyhow!(
                "{command} failed: {}",
                resp.error.unwrap_or_default()
            ))
        }
    }

    /// The display name the daemon is bound to. Useful for logs.
    #[allow(dead_code)]
    pub fn socket_name(&self) -> &str {
        &self.socket_name
    }
}

impl Drop for DaemonSidecar {
    fn drop(&mut self) {
        // Best-effort shutdown: kill the child, then reap it. We
        // swallow errors because Drop can't return them anyway, and
        // every branch (already exited, already reaped, permissions)
        // is effectively the same outcome — the child is gone.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Spawn the in-tree `prism-daemond` binary and connect to it over
/// IPC. Returns the live sidecar on success. On failure, any
/// half-spawned child is already reaped before the error bubbles up.
///
/// Used by `main.rs` immediately before the event loop takes over
/// the main thread. Errors are non-fatal at the call site — Studio
/// logs them and continues rendering without a daemon, which keeps
/// the renderer development loop usable even if the sidecar is
/// broken.
pub fn spawn_dev() -> anyhow::Result<DaemonSidecar> {
    let binary = locate_daemon_binary()
        .context("could not locate prism-daemond next to prism-studio in target/<profile>")?;

    // Unique per-process so concurrent Studio instances (e.g. two
    // `cargo run`s from two terminals) don't collide on the socket
    // name. The display name is ephemeral — on macOS it maps to a
    // file in `std::env::temp_dir()`; elsewhere it lives in the OS
    // namespace.
    let socket_name = format!("prism-daemon-{}.sock", std::process::id());

    let mut child = Command::new(&binary)
        .arg("--ipc-socket")
        .arg(&socket_name)
        .stdin(Stdio::null())
        // Keep daemon logs interleaved with Studio's own stderr so
        // dev iteration stays one-terminal. The packaged shell will
        // rewire this to a log file in Phase 5.
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .with_context(|| format!("failed to spawn {}", binary.display()))?;

    let stream = match connect_with_retry(&socket_name, &mut child) {
        Ok(s) => s,
        Err(e) => {
            // Guarantee the child is reaped on every failure path so
            // callers never leak a zombie.
            let _ = child.kill();
            let _ = child.wait();
            return Err(e);
        }
    };

    let mut sidecar = DaemonSidecar {
        child,
        stream,
        next_id: 1,
        socket_name,
    };

    // Read the banner the server writes on every accept. This is
    // also the spike's handshake step — if we can parse a banner,
    // the wire format is agreed on and spawn/connect worked.
    let banner: IpcResponse = read_frame(&mut sidecar.stream)
        .context("failed to read daemon banner")?
        .ok_or_else(|| anyhow!("daemon closed the stream before sending a banner"))?;

    if !banner.ok {
        bail!(
            "daemon banner reported error: {}",
            banner.error.unwrap_or_default()
        );
    }

    // The banner carries the full list of registered commands. Log a
    // quick one-liner so dev runs have visible proof the IPC wire is
    // alive without needing a debugger.
    if let Some(payload_json) = banner.payload_json.as_deref() {
        if let Ok(payload) = serde_json::from_str::<JsonValue>(payload_json) {
            let transport = payload["transport"].as_str().unwrap_or("?");
            let version = payload["version"].as_str().unwrap_or("?");
            let cmd_count = payload["commands"].as_array().map(|a| a.len()).unwrap_or(0);
            eprintln!(
                "prism-studio: daemon sidecar ready ({} {transport}, {cmd_count} commands) on {}",
                version, sidecar.socket_name
            );
        }
    }

    Ok(sidecar)
}

/// Walk sideways from `prism-studio`'s current exe to the
/// `prism-daemond` sibling. `cargo run -p prism-studio` produces
/// `target/<profile>/prism-studio` and `target/<profile>/prism-daemond`
/// in the same directory, so `current_exe().parent().join(...)` is
/// always the right answer in dev.
fn locate_daemon_binary() -> io::Result<PathBuf> {
    let mut path = std::env::current_exe()?;
    path.pop();
    #[cfg(windows)]
    path.push("prism-daemond.exe");
    #[cfg(not(windows))]
    path.push("prism-daemond");

    if !path.exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("prism-daemond not found at {}", path.display()),
        ));
    }
    Ok(path)
}

/// Connect to the sidecar, retrying briefly while the daemon's
/// listener comes up. Fails fast if the child exits before we manage
/// to attach — that's the signal we'd want to bubble up as an error
/// rather than keep spinning on.
fn connect_with_retry(socket_name: &str, child: &mut Child) -> anyhow::Result<Stream> {
    let start = Instant::now();
    loop {
        match connect_client(socket_name) {
            Ok(stream) => return Ok(stream),
            Err(e) => {
                if let Some(status) = child.try_wait().context("failed to poll sidecar child")? {
                    bail!(
                        "prism-daemond exited before binding ipc socket '{socket_name}': {status}"
                    );
                }
                if start.elapsed() > CONNECT_TIMEOUT {
                    bail!(
                        "timed out waiting for prism-daemond to bind ipc socket '{socket_name}' \
                         after {:?}: {e}",
                        CONNECT_TIMEOUT
                    );
                }
                std::thread::sleep(CONNECT_RETRY_INTERVAL);
            }
        }
    }
}
