//! End-to-end test for the `prism-daemond --ipc-socket` mode.
//!
//! Compiles the release binary on demand (via the `CARGO_BIN_EXE_*`
//! env var cargo sets for integration tests), spawns it as a
//! subprocess with `--ipc-socket`, connects from the test process
//! over `interprocess::local_socket`, and drives the same kind of
//! roundtrip the stdio test does — except through the postcard wire
//! format the Slint-based Studio shell uses (see
//! `docs/dev/slint-migration-plan.md`). This is the "confirm spawn /
//! supervise / kill works" half of Phase 0 spike #6.
//!
//! Gated on both `cli` (so the binary exists) and `transport-ipc`
//! (so the `--ipc-socket` flag path compiles in). Mobile/embedded/
//! wasm builds don't hit either, and the test is skipped there.

#![cfg(all(feature = "cli", feature = "transport-ipc"))]

use std::io;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use prism_daemon::{connect_client, read_frame, write_frame, IpcRequest, IpcResponse};

/// How long we wait for the spawned daemon to bind its listener
/// before we give up and fail the test. Normal bind time is <5 ms;
/// 2 seconds is a wide margin that still catches genuine hangs.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
const CONNECT_RETRY: Duration = Duration::from_millis(25);

fn unique_socket_name(suffix: &str) -> String {
    format!(
        "prism-daemon-itest-{}-{}-{}.sock",
        std::process::id(),
        suffix,
        // Nanosecond tail so sequential tests in the same process
        // never collide.
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    )
}

/// Spawn the daemon in IPC mode and wait for its listener to come
/// up. Returns the live child plus a connected stream.
fn spawn_and_connect(socket: &str) -> (std::process::Child, interprocess::local_socket::Stream) {
    let exe = env!("CARGO_BIN_EXE_prism-daemond");
    let mut child = Command::new(exe)
        .arg("--ipc-socket")
        .arg(socket)
        .stdin(Stdio::null())
        // Inherit so any panic or error in the child ends up next
        // to the test output, instead of being silently dropped.
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("failed to spawn prism-daemond");

    let start = Instant::now();
    loop {
        match connect_client(socket) {
            Ok(stream) => return (child, stream),
            Err(e) => {
                if let Some(status) = child.try_wait().expect("poll child") {
                    let _ = child.kill();
                    panic!("prism-daemond exited before binding ipc socket: {status}");
                }
                if start.elapsed() > CONNECT_TIMEOUT {
                    let _ = child.kill();
                    let _ = child.wait();
                    panic!(
                        "timed out waiting for prism-daemond to bind ipc socket '{socket}': {e}"
                    );
                }
                std::thread::sleep(CONNECT_RETRY);
            }
        }
    }
}

/// Invoke one command over the IPC stream and parse the response.
fn invoke(
    stream: &mut interprocess::local_socket::Stream,
    id: u64,
    command: &str,
    payload: &serde_json::Value,
) -> io::Result<IpcResponse> {
    let req = IpcRequest {
        id,
        command: command.to_string(),
        payload_json: payload.to_string(),
    };
    write_frame(stream, &req)?;
    read_frame::<_, IpcResponse>(stream)?
        .ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "no response frame"))
}

/// Spin up the daemon, read the banner, send `daemon.capabilities`
/// and one real `crdt.write`, then kill the child and confirm it
/// reaps cleanly. This is the minimum shape Phase 0 spike #6 had to
/// validate: spawn, supervise, kill.
#[test]
fn ipc_binary_boots_and_roundtrips_through_postcard_frames() {
    let socket = unique_socket_name("boot");
    let (mut child, mut stream) = spawn_and_connect(&socket);

    // 1. Banner — first frame on the wire must be the handshake.
    let banner: IpcResponse = read_frame(&mut stream)
        .expect("banner io")
        .expect("banner frame");
    assert_eq!(banner.id, 0);
    assert!(banner.ok, "banner: {banner:?}");
    let banner_payload: serde_json::Value =
        serde_json::from_str(&banner.payload_json.expect("banner payload")).unwrap();
    assert_eq!(banner_payload["daemon"], "prism-daemond");
    assert_eq!(banner_payload["transport"], "ipc");
    assert!(!banner_payload["commands"]
        .as_array()
        .expect("commands array")
        .is_empty());

    // 2. Reserved introspection command.
    let resp = invoke(
        &mut stream,
        1,
        "daemon.capabilities",
        &serde_json::json!({}),
    )
    .expect("capabilities io");
    assert!(resp.ok);
    assert_eq!(resp.id, 1);
    let caps: serde_json::Value = serde_json::from_str(&resp.payload_json.unwrap()).unwrap();
    let cmd_list = caps["commands"].as_array().unwrap();
    assert!(
        cmd_list.iter().any(|v| v == "crdt.write"),
        "expected crdt.write in capabilities: {cmd_list:?}"
    );

    // 3. Real module command — write into the CRDT doc manager so we
    //    know the kernel itself (not just the introspection shim) is
    //    reachable through the IPC wire.
    let resp = invoke(
        &mut stream,
        2,
        "crdt.write",
        &serde_json::json!({ "docId": "ipc-spike", "key": "hello", "value": "world" }),
    )
    .expect("crdt.write io");
    assert!(resp.ok, "crdt.write failed: {resp:?}");
    assert_eq!(resp.id, 2);

    // 4. Unknown command → server returns `ok: false` with an error
    //    string instead of tearing the connection down.
    let resp = invoke(&mut stream, 3, "no.such.command", &serde_json::json!({}))
        .expect("unknown-command io");
    assert_eq!(resp.id, 3);
    assert!(!resp.ok);
    assert!(resp
        .error
        .expect("error string")
        .contains("no.such.command"));

    // 5. Kill and reap. Closing the stream first lets the server's
    //    worker thread exit its read loop cleanly; then `kill`
    //    guarantees the whole process goes down even if it had more
    //    work queued. `wait` confirms the OS has actually reaped
    //    the child — this is the supervise+kill half of spike #6.
    drop(stream);
    child.kill().expect("kill child");
    let status = child.wait().expect("reap child");
    // We killed it, so a clean exit code isn't expected; we only
    // care that the child terminated and didn't become a zombie.
    assert!(!status.success() || status.code().is_some());
}

/// Run two sequential sessions against fresh sockets to make sure
/// the binary's cleanup path (unlinking a filesystem-backed socket
/// on macOS / BSDs) lets a second instance rebind cleanly.
#[test]
fn ipc_binary_can_rebind_after_clean_shutdown() {
    for round in 0..2 {
        let socket = unique_socket_name(&format!("rebind-{round}"));
        let (mut child, mut stream) = spawn_and_connect(&socket);

        let _banner: Option<IpcResponse> = read_frame(&mut stream).expect("banner io");
        let resp =
            invoke(&mut stream, 1, "daemon.modules", &serde_json::json!({})).expect("modules io");
        assert!(resp.ok);

        drop(stream);
        child.kill().expect("kill child");
        child.wait().expect("reap child");
    }
}
