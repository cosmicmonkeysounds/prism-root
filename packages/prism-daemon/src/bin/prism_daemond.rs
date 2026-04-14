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

/// Parse `--permission=user|dev` out of a raw argv list. Returns the
/// default tier ([`Permission::Dev`]) when the flag is absent so host
/// scripts that never cared about tiers keep running unchanged. Also
/// accepts the two-token form `--permission user` for ergonomics.
fn parse_permission<I>(args: I) -> Result<Permission, String>
where
    I: IntoIterator<Item = String>,
{
    let mut iter = args.into_iter();
    // Skip argv[0] (the executable name).
    iter.next();
    while let Some(arg) = iter.next() {
        if let Some(rest) = arg.strip_prefix("--permission=") {
            return Permission::parse(rest).map_err(|e| e.to_string());
        }
        if arg == "--permission" {
            let value = iter
                .next()
                .ok_or_else(|| "--permission requires a value (user|dev)".to_string())?;
            return Permission::parse(&value).map_err(|e| e.to_string());
        }
    }
    Ok(Permission::default())
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
    let permission = match parse_permission(std::env::args()) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("prism-daemond: {e}");
            std::process::exit(2);
        }
    };

    let kernel = Arc::new(
        DaemonBuilder::new()
            .with_permission(permission)
            .with_defaults()
            .build()
            .expect("failed to build DaemonKernel"),
    );

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
        assert_eq!(parse_permission(argv(&[])).unwrap(), Permission::Dev);
    }

    #[test]
    fn parse_permission_accepts_equals_form() {
        assert_eq!(
            parse_permission(argv(&["--permission=user"])).unwrap(),
            Permission::User
        );
        assert_eq!(
            parse_permission(argv(&["--permission=dev"])).unwrap(),
            Permission::Dev
        );
    }

    #[test]
    fn parse_permission_accepts_space_form() {
        assert_eq!(
            parse_permission(argv(&["--permission", "user"])).unwrap(),
            Permission::User
        );
    }

    #[test]
    fn parse_permission_rejects_unknown_value() {
        let err = parse_permission(argv(&["--permission=root"])).unwrap_err();
        assert!(err.contains("root"));
    }

    #[test]
    fn parse_permission_rejects_dangling_flag() {
        let err = parse_permission(argv(&["--permission"])).unwrap_err();
        assert!(err.contains("requires a value"));
    }

    #[test]
    fn parse_permission_ignores_unrelated_args() {
        assert_eq!(
            parse_permission(argv(&["--something", "else", "--permission=user"])).unwrap(),
            Permission::User
        );
    }
}
