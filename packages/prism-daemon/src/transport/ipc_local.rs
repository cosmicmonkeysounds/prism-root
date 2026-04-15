//! Local IPC transport — length-prefixed `postcard` frames over
//! `interprocess::local_socket`.
//!
//! This is the wire the Slint-based `prism-studio` shell uses to
//! talk to the daemon sidecar (see `docs/dev/slint-migration-plan.md`).
//! The
//! plan's open question on "postcard vs. tarpc" is resolved to raw
//! postcard here: the daemon kernel is sync and exposes a single
//! `invoke(name, payload_json) -> result_json` entry point, which is a
//! thin enough shape that a typed RPC layer would mostly be dead
//! weight. We keep request IDs for correlation (so the client can pair
//! replies with outstanding requests if we ever pipeline them) and
//! leave `tarpc` on the table for the day the surface grows.
//!
//! ### Wire format
//!
//! Every frame on the wire is:
//!
//! ```text
//! [ u32 LE length N ][ N bytes postcard-encoded body ]
//! ```
//!
//! where `body` is either an [`IpcRequest`] (client → server) or an
//! [`IpcResponse`] (server → client). Payloads are carried as JSON
//! *strings* inside postcard because `serde_json::Value` uses
//! `#[serde(untagged)]`, which postcard's non-self-describing format
//! cannot round-trip. The CLI binary already speaks JSON end-to-end, so
//! this keeps the two transports byte-identical at the payload level
//! and avoids introducing a second schema for the kernel's commands.
//!
//! ### Server lifecycle
//!
//! [`serve_blocking`] binds to a local socket name, accepts
//! connections, and dispatches each into a plain `std::thread::spawn`
//! worker. The kernel is sync and `Clone`, so fanning out to one
//! thread per connection is both the simplest model and the cheapest
//! — no tokio, no channel plumbing, no async fairness concerns. The
//! function returns when `listener.incoming()` ends (which in practice
//! only happens if the listener errors; see
//! [`ServeError::Accept`]). Real shutdown happens the way Unix expects:
//! the host kills the process.
//!
//! ### Client helpers
//!
//! Clients don't have a dedicated struct here (it lives host-side in
//! `prism-studio/src-tauri/src/sidecar.rs`), but they do share the
//! [`IpcRequest`] / [`IpcResponse`] types and the [`read_frame`] /
//! [`write_frame`] helpers so the wire format stays in one place.

use std::io::{self, Read, Write};
use std::sync::Arc;

use interprocess::local_socket::{
    prelude::*, GenericFilePath, GenericNamespaced, ListenerOptions, Stream,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};

use crate::kernel::DaemonKernel;

/// Maximum on-wire frame body size (16 MiB). Larger frames are
/// rejected rather than allocated — defends the daemon from a runaway
/// client sending a bogus length prefix.
pub const MAX_FRAME_BYTES: usize = 16 * 1024 * 1024;

/// One request frame on the wire.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcRequest {
    /// Monotonic request ID minted by the client. The server echoes it
    /// back on the matching [`IpcResponse`]. ID `0` is reserved for
    /// the server-initiated banner frame.
    pub id: u64,
    /// Kernel command name, e.g. `"crdt.write"`, `"daemon.capabilities"`.
    pub command: String,
    /// JSON-encoded payload. Kept as a string inside postcard because
    /// `serde_json::Value` uses `#[serde(untagged)]`, which postcard's
    /// non-self-describing format can't round-trip.
    pub payload_json: String,
}

/// One response frame on the wire.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcResponse {
    /// Echo of the matching [`IpcRequest::id`], or `0` if this is the
    /// server-initiated banner.
    pub id: u64,
    /// `true` on success, `false` on error. Mirrors the stdio bin's
    /// envelope so both transports share a shape.
    pub ok: bool,
    /// JSON-encoded result on success. `None` on error.
    pub payload_json: Option<String>,
    /// Error string on failure. `None` on success.
    pub error: Option<String>,
}

/// Returns the filesystem path the listener / client falls back to
/// when `GenericNamespaced` is unsupported on the host (macOS, BSDs).
/// Lives under `std::env::temp_dir()` so it's writeable by the
/// calling user and gets swept on reboot.
fn fs_socket_path(display: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(display)
}

/// Bind a listener on the given display name, picking the right
/// namespace for the host platform. Returns the bound listener plus
/// the filesystem path it's using (if any) so the caller can unlink
/// it during teardown. On Windows / Linux the filesystem path is
/// `None` because the socket lives in an OS namespace, not on disk.
pub fn bind_listener(
    display: &str,
) -> io::Result<(
    interprocess::local_socket::Listener,
    Option<std::path::PathBuf>,
)> {
    if GenericNamespaced::is_supported() {
        let name = display.to_ns_name::<GenericNamespaced>()?;
        let listener = ListenerOptions::new().name(name).create_sync()?;
        Ok((listener, None))
    } else {
        let fs_path = fs_socket_path(display);
        // Clean up any stale socket file from a previous crash —
        // otherwise `create_sync` will fail with EADDRINUSE.
        let _ = std::fs::remove_file(&fs_path);
        let name = fs_path.as_path().to_fs_name::<GenericFilePath>()?;
        let listener = ListenerOptions::new().name(name).create_sync()?;
        Ok((listener, Some(fs_path)))
    }
}

/// Connect to a server bound by [`bind_listener`] with the same
/// display name. Symmetrical to the server side — picks the right
/// namespace automatically.
pub fn connect_client(display: &str) -> io::Result<Stream> {
    if GenericNamespaced::is_supported() {
        let name = display.to_ns_name::<GenericNamespaced>()?;
        Stream::connect(name)
    } else {
        let fs_path = fs_socket_path(display);
        let name = fs_path.as_path().to_fs_name::<GenericFilePath>()?;
        Stream::connect(name)
    }
}

/// Write one length-prefixed postcard frame to the wire.
pub fn write_frame<W: Write, T: Serialize>(w: &mut W, msg: &T) -> io::Result<()> {
    let bytes =
        postcard::to_allocvec(msg).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let len = u32::try_from(bytes.len()).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("frame body too large: {} bytes", bytes.len()),
        )
    })?;
    w.write_all(&len.to_le_bytes())?;
    w.write_all(&bytes)?;
    w.flush()?;
    Ok(())
}

/// Read one length-prefixed postcard frame from the wire. Returns
/// `Ok(None)` on clean EOF (the peer hung up at a frame boundary), so
/// the caller can distinguish shutdown from protocol errors.
pub fn read_frame<R: Read, T: DeserializeOwned>(r: &mut R) -> io::Result<Option<T>> {
    let mut len_buf = [0u8; 4];
    match r.read_exact(&mut len_buf) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e),
    }
    let len = u32::from_le_bytes(len_buf) as usize;
    if len > MAX_FRAME_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("frame too large: {len} bytes"),
        ));
    }
    let mut body = vec![0u8; len];
    r.read_exact(&mut body)?;
    let msg = postcard::from_bytes::<T>(&body)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    Ok(Some(msg))
}

/// Error shape returned by [`serve_blocking`]. We keep it local to the
/// module (rather than bubbling `io::Error`) because the listener side
/// can fail in two structurally different ways: binding, and then
/// accepting.
#[derive(Debug, thiserror::Error)]
pub enum ServeError {
    #[error("failed to bind local socket {display}: {source}")]
    Bind {
        display: String,
        #[source]
        source: io::Error,
    },
    #[error("fatal accept error: {0}")]
    Accept(#[source] io::Error),
}

/// Run the IPC server synchronously on the calling thread.
///
/// Binds to `display` via [`bind_listener`], spawns one worker thread
/// per accepted connection, and dispatches each frame through
/// [`DaemonKernel::invoke`]. Reserved commands (`daemon.capabilities`,
/// `daemon.modules`) are handled inline so every client gets the same
/// introspection surface the stdio bin does.
///
/// Blocks until the accept loop ends. Propagates the first fatal
/// accept error as [`ServeError::Accept`]; per-connection errors are
/// logged to stderr and don't tear down the server.
///
/// The cleanup path on Unix unlinks the filesystem socket when the
/// function returns. On Linux/Windows there's no on-disk file to
/// clean up in the first place.
pub fn serve_blocking(kernel: Arc<DaemonKernel>, display: &str) -> Result<(), ServeError> {
    let (listener, fs_path) = bind_listener(display).map_err(|source| ServeError::Bind {
        display: display.to_string(),
        source,
    })?;

    // Print a single-line banner to stderr so a supervising host has
    // something to wait on before connecting. Matches the stdio bin's
    // "I'm ready" contract, just on a different stream.
    eprintln!("prism-daemond ipc ready on {display}");

    let result = (|| -> Result<(), ServeError> {
        for conn in listener.incoming() {
            match conn {
                Ok(stream) => {
                    let kernel = kernel.clone();
                    std::thread::spawn(move || {
                        if let Err(err) = handle_connection(&kernel, stream) {
                            eprintln!("prism-daemond ipc connection closed: {err}");
                        }
                    });
                }
                Err(e) => {
                    // Per-accept errors are fatal — if the listener
                    // has stopped handing out sockets we can't
                    // recover without rebinding, and rebinding would
                    // hide the underlying failure from the host.
                    return Err(ServeError::Accept(e));
                }
            }
        }
        Ok(())
    })();

    if let Some(path) = fs_path {
        let _ = std::fs::remove_file(path);
    }

    result
}

fn handle_connection(kernel: &DaemonKernel, mut stream: Stream) -> io::Result<()> {
    // Server-initiated banner so clients can confirm they're talking
    // to a healthy daemon before sending any real traffic. Mirrors
    // the JSON banner `prism-daemond`'s stdio loop emits on startup.
    let banner = IpcResponse {
        id: 0,
        ok: true,
        payload_json: Some(
            json!({
                "daemon": "prism-daemond",
                "version": env!("CARGO_PKG_VERSION"),
                "permission": kernel.permission().as_str(),
                "modules": kernel.installed_modules(),
                "commands": kernel.capabilities(),
                "transport": "ipc",
            })
            .to_string(),
        ),
        error: None,
    };
    write_frame(&mut stream, &banner)?;

    while let Some(req) = read_frame::<_, IpcRequest>(&mut stream)? {
        let response = dispatch(kernel, req);
        write_frame(&mut stream, &response)?;
    }

    Ok(())
}

fn dispatch(kernel: &DaemonKernel, req: IpcRequest) -> IpcResponse {
    let payload: JsonValue = if req.payload_json.is_empty() {
        JsonValue::Null
    } else {
        match serde_json::from_str(&req.payload_json) {
            Ok(v) => v,
            Err(e) => {
                return IpcResponse {
                    id: req.id,
                    ok: false,
                    payload_json: None,
                    error: Some(format!("invalid payload JSON: {e}")),
                };
            }
        }
    };

    let result: Result<JsonValue, String> = match req.command.as_str() {
        "daemon.capabilities" => Ok(json!({ "commands": kernel.capabilities() })),
        "daemon.modules" => Ok(json!({ "modules": kernel.installed_modules() })),
        other => kernel.invoke(other, payload).map_err(|e| e.to_string()),
    };

    match result {
        Ok(value) => IpcResponse {
            id: req.id,
            ok: true,
            payload_json: Some(value.to_string()),
            error: None,
        },
        Err(e) => IpcResponse {
            id: req.id,
            ok: false,
            payload_json: None,
            error: Some(e),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builder::DaemonBuilder;

    /// Minimal end-to-end exercise: server thread binds a unique
    /// socket, main thread connects, reads the banner, sends one
    /// introspection request, asserts the response, then drops the
    /// stream to close the connection. The server worker thread ends
    /// when `read_frame` returns `None`.
    #[test]
    fn roundtrip_capabilities_over_local_socket() {
        let kernel = Arc::new(
            DaemonBuilder::new()
                .with_defaults()
                .build()
                .expect("kernel built"),
        );

        let display = format!("prism-daemon-test-{}.sock", std::process::id());

        let (listener, fs_path) = bind_listener(&display).expect("bind");

        // Spawn a minimal accept loop off the test thread so the main
        // thread can drive the client side. We only need one
        // connection for the test.
        let server = std::thread::spawn({
            let kernel = kernel.clone();
            move || {
                let stream = listener
                    .incoming()
                    .next()
                    .expect("one connection")
                    .expect("accept");
                let _ = handle_connection(&kernel, stream);
            }
        });

        let mut client = connect_client(&display).expect("connect");

        // Banner first.
        let banner: IpcResponse = read_frame(&mut client)
            .expect("banner io")
            .expect("banner frame");
        assert_eq!(banner.id, 0);
        assert!(banner.ok);
        let banner_payload: JsonValue =
            serde_json::from_str(&banner.payload_json.expect("banner payload")).unwrap();
        assert_eq!(banner_payload["daemon"], "prism-daemond");
        assert_eq!(banner_payload["transport"], "ipc");

        // One real request.
        let req = IpcRequest {
            id: 1,
            command: "daemon.capabilities".into(),
            payload_json: String::new(),
        };
        write_frame(&mut client, &req).unwrap();

        let resp: IpcResponse = read_frame(&mut client)
            .expect("resp io")
            .expect("resp frame");
        assert_eq!(resp.id, 1);
        assert!(resp.ok);
        let caps: JsonValue = serde_json::from_str(&resp.payload_json.unwrap()).unwrap();
        let cmd_list = caps["commands"].as_array().expect("commands array");
        assert!(!cmd_list.is_empty());

        // Drop the client stream so the server worker returns.
        drop(client);
        server.join().unwrap();

        if let Some(path) = fs_path {
            let _ = std::fs::remove_file(path);
        }
    }

    #[test]
    fn roundtrip_unknown_command_reports_error() {
        let kernel = Arc::new(
            DaemonBuilder::new()
                .with_defaults()
                .build()
                .expect("kernel built"),
        );
        let display = format!("prism-daemon-test-err-{}.sock", std::process::id());
        let (listener, fs_path) = bind_listener(&display).expect("bind");

        let server = std::thread::spawn({
            let kernel = kernel.clone();
            move || {
                let stream = listener
                    .incoming()
                    .next()
                    .expect("one connection")
                    .expect("accept");
                let _ = handle_connection(&kernel, stream);
            }
        });

        let mut client = connect_client(&display).expect("connect");
        let _banner: Option<IpcResponse> = read_frame(&mut client).unwrap();

        let req = IpcRequest {
            id: 7,
            command: "no.such.command".into(),
            payload_json: "{}".into(),
        };
        write_frame(&mut client, &req).unwrap();

        let resp: IpcResponse = read_frame(&mut client).unwrap().unwrap();
        assert_eq!(resp.id, 7);
        assert!(!resp.ok);
        assert!(resp.error.unwrap().contains("no.such.command"));

        drop(client);
        server.join().unwrap();
        if let Some(path) = fs_path {
            let _ = std::fs::remove_file(path);
        }
    }

    #[test]
    fn read_frame_reports_clean_eof_as_none() {
        let mut empty: &[u8] = &[];
        let got: Option<IpcRequest> = read_frame(&mut empty).unwrap();
        assert!(got.is_none());
    }

    #[test]
    fn write_read_roundtrip_on_cursor() {
        let req = IpcRequest {
            id: 42,
            command: "ping".into(),
            payload_json: r#"{"k":"v"}"#.into(),
        };
        let mut buf: Vec<u8> = Vec::new();
        write_frame(&mut buf, &req).unwrap();
        let mut slice = buf.as_slice();
        let got: IpcRequest = read_frame(&mut slice).unwrap().unwrap();
        assert_eq!(got.id, 42);
        assert_eq!(got.command, "ping");
        assert_eq!(got.payload_json, r#"{"k":"v"}"#);
    }
}
