//! Loom v3 Language Server Protocol implementation.
//!
//! Stdio JSON-RPC server backed by [`loom_parser`] plus a workspace-
//! wide name index (every `CHARACTER`, every `TRAIT`, every registered
//! directive function, every beat, every anchor, every ` ```todo``` `
//! fence — spec §17).
//!
//! The library exposes [`run_stdio`] for the binary plus a
//! [`workspace::Workspace`] type that the test suite drives directly,
//! avoiding stdio plumbing for request-handler coverage.

use anyhow::Result;
use lsp_server::{Connection, ExtractError, Message, Notification, Request, RequestId, Response};
use lsp_types::notification::{
    DidChangeTextDocument, DidOpenTextDocument, DidSaveTextDocument, Notification as _,
    PublishDiagnostics,
};
use lsp_types::request::{
    Completion, DocumentSymbolRequest, GotoDefinition, HoverRequest, Request as _,
};
use lsp_types::{
    CompletionOptions, DidChangeTextDocumentParams, DidOpenTextDocumentParams,
    DidSaveTextDocumentParams, InitializeParams, OneOf, PublishDiagnosticsParams,
    ServerCapabilities, TextDocumentSyncCapability, TextDocumentSyncKind,
};

pub mod completion;
pub mod definition;
pub mod hover;
pub mod symbols;
pub mod workspace;

pub use workspace::{OpenDoc, Workspace};

/// Run the LSP loop on stdin/stdout.
pub fn run_stdio() -> Result<()> {
    let (connection, io_threads) = Connection::stdio();
    let server_capabilities = serde_json::to_value(server_capabilities())?;
    let initialize_params = connection.initialize(server_capabilities)?;
    let _params: InitializeParams = serde_json::from_value(initialize_params)?;
    main_loop(&connection)?;
    io_threads.join()?;
    Ok(())
}

/// Capabilities advertised in the `initialize` response.
pub fn server_capabilities() -> ServerCapabilities {
    ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
        completion_provider: Some(CompletionOptions {
            trigger_characters: Some(vec![">".to_string(), "<".to_string(), " ".to_string()]),
            ..Default::default()
        }),
        hover_provider: Some(lsp_types::HoverProviderCapability::Simple(true)),
        document_symbol_provider: Some(OneOf::Left(true)),
        definition_provider: Some(OneOf::Left(true)),
        ..Default::default()
    }
}

fn main_loop(connection: &Connection) -> Result<()> {
    let mut workspace = Workspace::new();
    for msg in &connection.receiver {
        match msg {
            Message::Request(req) => {
                if connection.handle_shutdown(&req)? {
                    return Ok(());
                }
                let response = dispatch_request(&mut workspace, req);
                connection.sender.send(Message::Response(response))?;
            }
            Message::Notification(note) => {
                if let Some(diags) = dispatch_notification(&mut workspace, note) {
                    for (uri, params) in diags {
                        let note = Notification::new(
                            PublishDiagnostics::METHOD.to_string(),
                            PublishDiagnosticsParams { uri, ..params },
                        );
                        connection.sender.send(Message::Notification(note))?;
                    }
                }
            }
            Message::Response(_) => {}
        }
    }
    Ok(())
}

fn dispatch_request(workspace: &mut Workspace, req: Request) -> Response {
    let id = req.id.clone();
    match req.method.as_str() {
        Completion::METHOD => handle::<Completion, _>(req, |params| {
            let pos = params.text_document_position;
            let items = workspace.completion_at(&pos.text_document.uri, pos.position);
            Ok(Some(lsp_types::CompletionResponse::Array(items)))
        }),
        HoverRequest::METHOD => handle::<HoverRequest, _>(req, |params| {
            let pos = params.text_document_position_params;
            Ok(workspace.hover_at(&pos.text_document.uri, pos.position))
        }),
        GotoDefinition::METHOD => handle::<GotoDefinition, _>(req, |params| {
            let pos = params.text_document_position_params;
            Ok(workspace.definition_at(&pos.text_document.uri, pos.position))
        }),
        DocumentSymbolRequest::METHOD => handle::<DocumentSymbolRequest, _>(req, |params| {
            Ok(workspace.document_symbols(&params.text_document.uri))
        }),
        _ => Response {
            id,
            result: Some(serde_json::Value::Null),
            error: None,
        },
    }
}

fn handle<R, F>(req: Request, f: F) -> Response
where
    R: lsp_types::request::Request,
    R::Params: serde::de::DeserializeOwned,
    R::Result: serde::Serialize,
    F: FnOnce(R::Params) -> Result<R::Result, String>,
{
    let id = req.id.clone();
    match cast_request::<R>(req) {
        Ok((_, params)) => match f(params) {
            Ok(result) => match serde_json::to_value(&result) {
                Ok(value) => Response {
                    id,
                    result: Some(value),
                    error: None,
                },
                Err(err) => error_response(id, err.to_string()),
            },
            Err(msg) => error_response(id, msg),
        },
        Err(err) => error_response(id, err),
    }
}

fn cast_request<R>(req: Request) -> Result<(RequestId, R::Params), String>
where
    R: lsp_types::request::Request,
    R::Params: serde::de::DeserializeOwned,
{
    req.extract::<R::Params>(R::METHOD)
        .map_err(|err| match err {
            ExtractError::MethodMismatch(r) => format!("method mismatch: {}", r.method),
            ExtractError::JsonError { method, error } => format!("json error in {method}: {error}"),
        })
}

fn error_response(id: RequestId, msg: String) -> Response {
    Response {
        id,
        result: None,
        error: Some(lsp_server::ResponseError {
            code: lsp_server::ErrorCode::InternalError as i32,
            message: msg,
            data: None,
        }),
    }
}

fn dispatch_notification(
    workspace: &mut Workspace,
    note: Notification,
) -> Option<Vec<(lsp_types::Url, PublishDiagnosticsParams)>> {
    match note.method.as_str() {
        DidOpenTextDocument::METHOD => {
            let params: DidOpenTextDocumentParams = serde_json::from_value(note.params).ok()?;
            let uri = params.text_document.uri.clone();
            workspace.open(uri.clone(), params.text_document.text);
            Some(vec![(uri.clone(), workspace.diagnostics_for(&uri)?)])
        }
        DidChangeTextDocument::METHOD => {
            let params: DidChangeTextDocumentParams = serde_json::from_value(note.params).ok()?;
            let uri = params.text_document.uri.clone();
            if let Some(change) = params.content_changes.into_iter().last() {
                workspace.update(uri.clone(), change.text);
            }
            Some(vec![(uri.clone(), workspace.diagnostics_for(&uri)?)])
        }
        DidSaveTextDocument::METHOD => {
            let params: DidSaveTextDocumentParams = serde_json::from_value(note.params).ok()?;
            let uri = params.text_document.uri.clone();
            if let Some(text) = params.text {
                workspace.update(uri.clone(), text);
            }
            Some(vec![(uri.clone(), workspace.diagnostics_for(&uri)?)])
        }
        _ => None,
    }
}
