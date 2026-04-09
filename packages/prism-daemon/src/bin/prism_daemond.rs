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
//! This binary is deliberately boring: it's a *proof* that the kernel is
//! transport-agnostic. Real deployments will wrap the kernel in Tauri,
//! UniFFI, or an HTTP/gRPC adapter.

use prism_daemon::{CommandError, DaemonBuilder, DaemonKernel};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

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
    let kernel = Arc::new(
        DaemonBuilder::new()
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
