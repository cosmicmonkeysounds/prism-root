//! `prism-daemond` — the standalone Prism Daemon.
//!
//! A minimal stdio-JSON loop demonstrating that the same [`DaemonKernel`]
//! Studio embeds over Tauri IPC can also run as a headless background
//! process on any device. Each line on stdin is a JSON envelope:
//!
//! ```json
//! { "id": 1, "command": "crdt.write", "payload": { "docId": "x", "key": "k", "value": "v" } }
//! ```
//!
//! Each line on stdout is the reply:
//!
//! ```json
//! { "id": 1, "ok": true, "result": { "bytes": [...] } }
//! ```
//!
//! Errors:
//!
//! ```json
//! { "id": 1, "ok": false, "error": "…" }
//! ```
//!
//! Two reserved commands are exposed on top of whatever modules are
//! loaded:
//!
//! | Command             | Description                                  |
//! |---------------------|----------------------------------------------|
//! | `daemon.capabilities` | List every registered command name         |
//! | `daemon.modules`    | List every installed module ID               |
//!
//! ### IPC mode
//!
//! Passing `--ipc-socket <display-name>` switches the binary from the
//! stdio loop to the local-IPC transport defined in
//! [`prism_daemon::transport::ipc_local`]. Frames are length-prefixed
//! `postcard` encodings of [`prism_daemon::IpcRequest`] /
//! [`prism_daemon::IpcResponse`]. This is the mode the Tauri 2
//! no-webview Studio shell uses to talk to the daemon sidecar per
//! §4.5 of the Clay migration plan. Only available when the binary
//! is compiled with the `transport-ipc` feature.
//!
//! ### Permission tier
//!
//! The binary accepts `--permission=user` or `--permission=dev` to stamp
//! the kernel with a caller tier. `dev` is the default (so existing host
//! scripts, test harnesses, and developer tooling don't have to change).
//! Published end-user shells (Flux / Lattice / Musica) should always pass
//! `--permission=user` so the kernel refuses every command that wasn't
//! explicitly opted into the user tier. Parsing is deliberately tiny —
//! no `clap` dependency, just `std::env::args`.
//!
//! This binary is deliberately boring: it's a *proof* that the kernel is
//! transport-agnostic. Real deployments will wrap the kernel in Tauri,
//! UniFFI, or an HTTP/gRPC adapter.

use prism_daemon::{CommandError, DaemonBuilder, DaemonKernel, Permission};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

/// Parsed CLI for `prism-daemond`. Tiny on purpose — we hand-roll the
/// argv walk instead of pulling `clap` so the binary stays small and
/// the parser stays grep-able.
#[derive(Debug, Default, PartialEq, Eq)]
struct CliArgs {
    permission: Permission,
    /// When `Some`, run the IPC transport against this display name
    /// instead of the stdio loop.
    ipc_socket: Option<String>,
}

/// Parse `prism-daemond`'s argv:
///
/// * `--permission=user|dev` / `--permission user|dev` — stamp the
///   kernel with a caller tier. Defaults to [`Permission::Dev`].
/// * `--ipc-socket=<name>` / `--ipc-socket <name>` — run the IPC
///   transport instead of the stdio loop. Only meaningful when the
///   binary was built with the `transport-ipc` feature.
fn parse_args<I>(args: I) -> Result<CliArgs, String>
where
    I: IntoIterator<Item = String>,
{
    let mut out = CliArgs::default();
    let mut iter = args.into_iter();
    // Skip argv[0] (the executable name).
    iter.next();
    while let Some(arg) = iter.next() {
        if let Some(rest) = arg.strip_prefix("--permission=") {
            out.permission = Permission::parse(rest).map_err(|e| e.to_string())?;
            continue;
        }
        if arg == "--permission" {
            let value = iter
                .next()
                .ok_or_else(|| "--permission requires a value (user|dev)".to_string())?;
            out.permission = Permission::parse(&value).map_err(|e| e.to_string())?;
            continue;
        }
        if let Some(rest) = arg.strip_prefix("--ipc-socket=") {
            if rest.is_empty() {
                return Err("--ipc-socket requires a non-empty name".to_string());
            }
            out.ipc_socket = Some(rest.to_string());
            continue;
        }
        if arg == "--ipc-socket" {
            let value = iter
                .next()
                .ok_or_else(|| "--ipc-socket requires a value".to_string())?;
            if value.is_empty() {
                return Err("--ipc-socket requires a non-empty name".to_string());
            }
            out.ipc_socket = Some(value);
            continue;
        }
    }
    Ok(out)
}

#[derive(Debug, Deserialize)]
struct Request {
    #[serde(default)]
    id: Option<JsonValue>,
    command: String,
    #[serde(default)]
    payload: JsonValue,
}

#[derive(Debug, Serialize)]
struct Response {
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<JsonValue>,
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<JsonValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

fn handle(kernel: &DaemonKernel, req: Request) -> Response {
    let result = match req.command.as_str() {
        "daemon.capabilities" => Ok(json!({ "commands": kernel.capabilities() })),
        "daemon.modules" => Ok(json!({ "modules": kernel.installed_modules() })),
        other => kernel.invoke(other, req.payload).map_err(Into::into),
    };

    match result {
        Ok(result) => Response {
            id: req.id,
            ok: true,
            result: Some(result),
            error: None,
        },
        Err(e) => Response {
            id: req.id,
            ok: false,
            result: None,
            error: Some(format_error(&e)),
        },
    }
}

fn format_error(e: &HandleError) -> String {
    match e {
        HandleError::Command(c) => c.to_string(),
    }
}

enum HandleError {
    Command(CommandError),
}

impl From<CommandError> for HandleError {
    fn from(c: CommandError) -> Self {
        HandleError::Command(c)
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> std::io::Result<()> {
    let cli = match parse_args(std::env::args()) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("prism-daemond: {e}");
            std::process::exit(2);
        }
    };

    let kernel = Arc::new(
        DaemonBuilder::new()
            .with_permission(cli.permission)
            .with_defaults()
            .build()
            .expect("failed to build DaemonKernel"),
    );

    // IPC mode: hand off to the local-socket transport and never
    // touch stdio. The server is synchronous and blocks the current
    // thread until the listener stops, which is exactly what we want
    // — there's no other work to do in this flavor-current-thread
    // runtime, so parking it on a blocking call is free.
    #[cfg(feature = "transport-ipc")]
    if let Some(display) = cli.ipc_socket.as_ref() {
        match prism_daemon::serve_blocking(kernel.clone(), display) {
            Ok(()) => {
                kernel.dispose();
                return Ok(());
            }
            Err(e) => {
                eprintln!("prism-daemond: ipc transport failed: {e}");
                kernel.dispose();
                std::process::exit(1);
            }
        }
    }

    #[cfg(not(feature = "transport-ipc"))]
    if cli.ipc_socket.is_some() {
        eprintln!(
            "prism-daemond: --ipc-socket requires the `transport-ipc` feature at build time"
        );
        std::process::exit(2);
    }

    let mut stdin = BufReader::new(tokio::io::stdin()).lines();
    let mut stdout = tokio::io::stdout();

    // Banner so hosts know they're talking to a healthy daemon.
    let banner = json!({
        "ok": true,
        "result": {
            "daemon": "prism-daemond",
            "version": env!("CARGO_PKG_VERSION"),
            "permission": kernel.permission().as_str(),
            "modules": kernel.installed_modules(),
            "commands": kernel.capabilities(),
        }
    });
    stdout
        .write_all(format!("{}\n", serde_json::to_string(&banner).unwrap()).as_bytes())
        .await?;
    stdout.flush().await?;

    while let Some(line) = stdin.next_line().await? {
        if line.trim().is_empty() {
            continue;
        }

        let response = match serde_json::from_str::<Request>(&line) {
            Ok(req) => handle(&kernel, req),
            Err(e) => Response {
                id: None,
                ok: false,
                result: None,
                error: Some(format!("invalid request: {e}")),
            },
        };

        let encoded = serde_json::to_string(&response).unwrap_or_else(|e| {
            format!(r#"{{"ok":false,"error":"failed to encode response: {e}"}}"#)
        });
        stdout.write_all(encoded.as_bytes()).await?;
        stdout.write_all(b"\n").await?;
        stdout.flush().await?;
    }

    kernel.dispose();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(rest: &[&str]) -> Vec<String> {
        std::iter::once("prism-daemond".to_string())
            .chain(rest.iter().map(|s| s.to_string()))
            .collect()
    }

    #[test]
    fn parse_permission_defaults_to_dev() {
        assert_eq!(parse_args(argv(&[])).unwrap().permission, Permission::Dev);
    }

    #[test]
    fn parse_permission_accepts_equals_form() {
        assert_eq!(
            parse_args(argv(&["--permission=user"])).unwrap().permission,
            Permission::User
        );
        assert_eq!(
            parse_args(argv(&["--permission=dev"])).unwrap().permission,
            Permission::Dev
        );
    }

    #[test]
    fn parse_permission_accepts_space_form() {
        assert_eq!(
            parse_args(argv(&["--permission", "user"]))
                .unwrap()
                .permission,
            Permission::User
        );
    }

    #[test]
    fn parse_permission_rejects_unknown_value() {
        let err = parse_args(argv(&["--permission=root"])).unwrap_err();
        assert!(err.contains("root"));
    }

    #[test]
    fn parse_permission_rejects_dangling_flag() {
        let err = parse_args(argv(&["--permission"])).unwrap_err();
        assert!(err.contains("requires a value"));
    }

    #[test]
    fn parse_permission_ignores_unrelated_args() {
        assert_eq!(
            parse_args(argv(&["--something", "else", "--permission=user"]))
                .unwrap()
                .permission,
            Permission::User
        );
    }

    #[test]
    fn parse_ipc_socket_equals_form() {
        let cli = parse_args(argv(&["--ipc-socket=prism.sock"])).unwrap();
        assert_eq!(cli.ipc_socket.as_deref(), Some("prism.sock"));
    }

    #[test]
    fn parse_ipc_socket_space_form() {
        let cli = parse_args(argv(&["--ipc-socket", "prism.sock"])).unwrap();
        assert_eq!(cli.ipc_socket.as_deref(), Some("prism.sock"));
    }

    #[test]
    fn parse_ipc_socket_defaults_to_none() {
        assert!(parse_args(argv(&[])).unwrap().ipc_socket.is_none());
    }

    #[test]
    fn parse_ipc_socket_rejects_empty_value() {
        let err = parse_args(argv(&["--ipc-socket="])).unwrap_err();
        assert!(err.contains("non-empty"));
        let err = parse_args(argv(&["--ipc-socket", ""])).unwrap_err();
        assert!(err.contains("non-empty"));
    }

    #[test]
    fn parse_ipc_socket_rejects_dangling_flag() {
        let err = parse_args(argv(&["--ipc-socket"])).unwrap_err();
        assert!(err.contains("requires a value"));
    }

    #[test]
    fn parse_args_combines_permission_and_ipc_socket() {
        let cli = parse_args(argv(&["--permission=user", "--ipc-socket=prism.sock"])).unwrap();
        assert_eq!(cli.permission, Permission::User);
        assert_eq!(cli.ipc_socket.as_deref(), Some("prism.sock"));
    }
}
