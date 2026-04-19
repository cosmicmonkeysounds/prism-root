//! End-to-end test for the `prism-daemond` stdio JSON binary.
//!
//! Compiles the release binary on demand (via the `CARGO_BIN_EXE_*` env
//! var cargo sets for integration tests), spawns it as a subprocess,
//! and drives a few commands through stdin/stdout. This is the
//! equivalent of the WASM Playwright suite for the CLI transport — it
//! proves the kernel runs as a standalone process and that every built-in
//! module survives a real JSON envelope round trip.
//!
//! The binary is gated on the `cli` feature, which is on by default.
//! Mobile/embedded/wasm builds don't compile it, so this whole test
//! file is gated on `cli` too.

#![cfg(feature = "cli")]

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

/// Spawn `prism-daemond` with a piped stdio and return the child.
fn spawn_daemon() -> std::process::Child {
    let exe = env!("CARGO_BIN_EXE_prism-daemond");
    Command::new(exe)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn prism-daemond")
}

/// Read one JSON line from the child's stdout. A zero-length read means
/// the daemon closed stdout without replying, which is always a bug in
/// the test setup or the daemon itself — panic loudly in that case.
fn read_line(stdout: &mut BufReader<std::process::ChildStdout>) -> String {
    loop {
        let mut line = String::new();
        match stdout.read_line(&mut line) {
            Ok(0) => panic!("prism-daemond closed stdout before replying"),
            Ok(_) => return line,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => panic!("stdout read error: {e}"),
        }
    }
}

/// Drive a full session: banner + capabilities + modules + one command
/// from each built-in module.
#[test]
fn stdio_binary_boots_and_roundtrips_every_module() {
    let mut child = spawn_daemon();
    let mut stdin = child.stdin.take().expect("stdin");
    let mut stdout = BufReader::new(child.stdout.take().expect("stdout"));

    // 1. Banner — first line on stdout must be the startup envelope.
    let banner_line = read_line(&mut stdout);
    let banner: serde_json::Value =
        serde_json::from_str(&banner_line).expect("banner must be JSON");
    assert_eq!(banner["ok"], true, "banner: {banner}");
    let modules = banner["result"]["modules"]
        .as_array()
        .expect("banner.result.modules");
    let mod_ids: Vec<&str> = modules.iter().filter_map(|v| v.as_str()).collect();
    assert!(mod_ids.contains(&"prism.crdt"));
    assert!(mod_ids.contains(&"prism.luau"));
    assert!(mod_ids.contains(&"prism.vfs"));
    assert!(mod_ids.contains(&"prism.crypto"));

    // 2. daemon.capabilities — confirms all feature-gated commands
    //    are registered in the full build.
    writeln!(
        stdin,
        r#"{{"id":1,"command":"daemon.capabilities","payload":{{}}}}"#
    )
    .unwrap();
    stdin.flush().unwrap();
    let caps_line = read_line(&mut stdout);
    let caps: serde_json::Value = serde_json::from_str(&caps_line).unwrap();
    assert_eq!(caps["id"], 1);
    assert_eq!(caps["ok"], true);
    let commands = caps["result"]["commands"].as_array().unwrap();
    let cmd_names: Vec<&str> = commands.iter().filter_map(|v| v.as_str()).collect();
    for expected in [
        "crdt.write",
        "crdt.read",
        "luau.exec",
        "vfs.put",
        "vfs.get",
        "crypto.keypair",
        "crypto.encrypt",
        "crypto.decrypt",
    ] {
        assert!(
            cmd_names.contains(&expected),
            "missing {expected} in stdio caps"
        );
    }

    // 3. crdt.write → crdt.read roundtrip.
    writeln!(
        stdin,
        r#"{{"id":2,"command":"crdt.write","payload":{{"docId":"stdio","key":"hello","value":"world"}}}}"#
    )
    .unwrap();
    stdin.flush().unwrap();
    let write_line = read_line(&mut stdout);
    let write: serde_json::Value = serde_json::from_str(&write_line).unwrap();
    assert_eq!(write["id"], 2);
    assert_eq!(write["ok"], true, "crdt.write: {write}");

    writeln!(
        stdin,
        r#"{{"id":3,"command":"crdt.read","payload":{{"docId":"stdio","key":"hello"}}}}"#
    )
    .unwrap();
    stdin.flush().unwrap();
    let read_line_s = read_line(&mut stdout);
    let read: serde_json::Value = serde_json::from_str(&read_line_s).unwrap();
    assert_eq!(read["ok"], true);
    // DocManager JSON-encodes values, so "world" comes back as `"\"world\""`.
    assert_eq!(read["result"]["value"], "\"world\"");

    // 4. luau.exec.
    writeln!(
        stdin,
        r#"{{"id":4,"command":"luau.exec","payload":{{"script":"return 6 * 7"}}}}"#
    )
    .unwrap();
    stdin.flush().unwrap();
    let luau_line = read_line(&mut stdout);
    let luau: serde_json::Value = serde_json::from_str(&luau_line).unwrap();
    assert_eq!(luau["ok"], true, "luau.exec: {luau}");
    assert_eq!(luau["result"], 42);

    // 5. vfs.put → vfs.get.
    writeln!(
        stdin,
        r#"{{"id":5,"command":"vfs.put","payload":{{"bytes":[112,114,105,115,109]}}}}"#
    )
    .unwrap();
    stdin.flush().unwrap();
    let put_line = read_line(&mut stdout);
    let put: serde_json::Value = serde_json::from_str(&put_line).unwrap();
    assert_eq!(put["ok"], true, "vfs.put: {put}");
    let hash = put["result"]["hash"].as_str().unwrap().to_string();
    assert_eq!(hash.len(), 64);

    writeln!(
        stdin,
        r#"{{"id":6,"command":"vfs.get","payload":{{"hash":"{hash}"}}}}"#
    )
    .unwrap();
    stdin.flush().unwrap();
    let get_line = read_line(&mut stdout);
    let got: serde_json::Value = serde_json::from_str(&get_line).unwrap();
    assert_eq!(got["ok"], true);
    let bytes: Vec<u8> = serde_json::from_value(got["result"]["bytes"].clone()).unwrap();
    assert_eq!(bytes, b"prism");

    // 6. crypto.keypair + crypto.encrypt/decrypt roundtrip.
    writeln!(
        stdin,
        r#"{{"id":7,"command":"crypto.keypair","payload":{{}}}}"#
    )
    .unwrap();
    stdin.flush().unwrap();
    let kp_line = read_line(&mut stdout);
    let kp: serde_json::Value = serde_json::from_str(&kp_line).unwrap();
    assert_eq!(kp["ok"], true, "crypto.keypair: {kp}");
    let sk = kp["result"]["secret_key"].as_str().unwrap();
    assert_eq!(sk.len(), 64);

    // Use a deterministic key so the test doesn't have to plumb two
    // separate secret keys through the json string interpolation dance.
    let symmetric_key = "00".repeat(32);
    let plaintext_hex = "6865782d70726973";
    writeln!(
        stdin,
        r#"{{"id":8,"command":"crypto.encrypt","payload":{{"key":"{symmetric_key}","plaintext":"{plaintext_hex}"}}}}"#
    )
    .unwrap();
    stdin.flush().unwrap();
    let enc_line = read_line(&mut stdout);
    let enc: serde_json::Value = serde_json::from_str(&enc_line).unwrap();
    assert_eq!(enc["ok"], true, "crypto.encrypt: {enc}");
    let ct = enc["result"]["ciphertext"].as_str().unwrap().to_string();
    let nonce = enc["result"]["nonce"].as_str().unwrap().to_string();

    writeln!(
        stdin,
        r#"{{"id":9,"command":"crypto.decrypt","payload":{{"key":"{symmetric_key}","ciphertext":"{ct}","nonce":"{nonce}"}}}}"#
    )
    .unwrap();
    stdin.flush().unwrap();
    let dec_line = read_line(&mut stdout);
    let dec: serde_json::Value = serde_json::from_str(&dec_line).unwrap();
    assert_eq!(dec["ok"], true, "crypto.decrypt: {dec}");
    assert_eq!(dec["result"]["plaintext"], plaintext_hex);

    // 7. Unknown command surfaces as a structured error — the daemon
    //    must stay alive afterwards.
    writeln!(
        stdin,
        r#"{{"id":10,"command":"this.does.not.exist","payload":{{}}}}"#
    )
    .unwrap();
    stdin.flush().unwrap();
    let bad_line = read_line(&mut stdout);
    let bad: serde_json::Value = serde_json::from_str(&bad_line).unwrap();
    assert_eq!(bad["ok"], false);
    assert!(bad["error"]
        .as_str()
        .unwrap()
        .contains("this.does.not.exist"));

    // 8. Final sanity ping after the error.
    writeln!(
        stdin,
        r#"{{"id":11,"command":"luau.exec","payload":{{"script":"return 'still alive'"}}}}"#
    )
    .unwrap();
    stdin.flush().unwrap();
    let post = read_line(&mut stdout);
    let post: serde_json::Value = serde_json::from_str(&post).unwrap();
    assert_eq!(post["ok"], true);
    assert_eq!(post["result"], "still alive");

    // Close stdin so the daemon's read loop exits cleanly.
    drop(stdin);
    let status = child.wait().expect("wait");
    assert!(status.success(), "daemon exited with {status:?}");
}

/// Invalid JSON on stdin must produce a structured error envelope and
/// *not* crash the daemon — a downstream host that sends garbage should
/// still be able to recover.
#[test]
fn stdio_binary_handles_invalid_json_without_dying() {
    let mut child = spawn_daemon();
    let mut stdin = child.stdin.take().expect("stdin");
    let mut stdout = BufReader::new(child.stdout.take().expect("stdout"));

    // Drain banner.
    let _ = read_line(&mut stdout);

    writeln!(stdin, "this is not json").unwrap();
    stdin.flush().unwrap();
    let err_line = read_line(&mut stdout);
    let err: serde_json::Value = serde_json::from_str(&err_line).unwrap();
    assert_eq!(err["ok"], false);
    assert!(err["error"].as_str().unwrap().contains("invalid request"));

    // Sanity: daemon still alive after garbage input.
    writeln!(
        stdin,
        r#"{{"id":1,"command":"daemon.capabilities","payload":{{}}}}"#
    )
    .unwrap();
    stdin.flush().unwrap();
    let ok_line = read_line(&mut stdout);
    let ok: serde_json::Value = serde_json::from_str(&ok_line).unwrap();
    assert_eq!(ok["ok"], true);

    drop(stdin);
    let _ = child.wait();
}
