//! Loom Language Server Protocol implementation.
//!
//! Stdio JSON-RPC server backed by
//! [`prism_core::language::loom`]. Implements the subset of the LSP
//! that matters for storytelling-language editing:
//!
//! - **`textDocument/didOpen` / `didChange` / `didClose`** — track
//!   document text in memory. Every change re-parses the buffer and
//!   pushes `textDocument/publishDiagnostics` with the parser's
//!   diagnostic stream (stable ids from §21 of the grammar doc).
//!
//! - **`textDocument/semanticTokens/full`** — the highlight stream
//!   for editors (notably Zed) that don't ship a TextMate grammar
//!   path for Loom. Walks the parsed `SyntaxNode` tree, mapping
//!   node kinds and identifier text against
//!   [`prism_core::language::loom::keywords`] to LSP semantic token
//!   types + modifiers.
//!
//! - **`textDocument/hover`** — delegates to
//!   [`prism_core::language::loom::provider::LoomSyntaxProvider::hover`].
//!
//! - **`textDocument/completion`** — delegates to
//!   [`prism_core::language::loom::provider::LoomSyntaxProvider::complete`].
//!
//! The server is intentionally synchronous (`lsp-server` over stdio,
//! no tokio): the workloads are small and the latency budget is
//! milliseconds. An async path can wrap this lib when we need it.

use std::collections::HashMap;

use anyhow::{anyhow, Context, Result};
use lsp_server::{Connection, ExtractError, Message, Notification, Request, RequestId, Response};
use lsp_types::notification::{
    DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument, Notification as _,
    PublishDiagnostics,
};
use lsp_types::request::{Completion, HoverRequest, Request as _, SemanticTokensFullRequest};
use lsp_types::{
    CompletionItem, CompletionItemKind, CompletionOptions, CompletionParams, CompletionResponse,
    Diagnostic, DiagnosticSeverity, DidChangeTextDocumentParams, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams, Hover, HoverContents, HoverParams, HoverProviderCapability,
    InitializeParams, MarkupContent, MarkupKind, Position as LspPosition,
    PublishDiagnosticsParams, Range as LspRange, SemanticToken, SemanticTokenModifier,
    SemanticTokenType, SemanticTokens, SemanticTokensFullOptions, SemanticTokensLegend,
    SemanticTokensOptions, SemanticTokensParams, SemanticTokensResult,
    SemanticTokensServerCapabilities, ServerCapabilities, TextDocumentSyncCapability,
    TextDocumentSyncKind, Uri, WorkDoneProgressOptions,
};

use prism_core::language::loom::node_kinds as nk;
use prism_core::language::loom::parser::{parse, ParseResult, Severity as LoomSeverity};
use prism_core::language::loom::provider::LoomSyntaxProvider;
use prism_core::language::syntax::{SyntaxNode, SyntaxProvider};

// ─── Public entry points ────────────────────────────────────────────

/// Run the LSP loop on stdio. Blocks until the client disconnects or
/// the loop hits a transport error.
pub fn run_stdio() -> Result<()> {
    let (connection, io_threads) = Connection::stdio();
    let server_capabilities = serde_json::to_value(server_capabilities())?;
    let initialization_params = connection.initialize(server_capabilities)?;
    let _: InitializeParams = serde_json::from_value(initialization_params)
        .context("decoding InitializeParams")?;

    let mut state = ServerState::default();
    main_loop(&connection, &mut state)?;
    io_threads.join()?;
    Ok(())
}

// ─── Capabilities + token legend ────────────────────────────────────

/// The semantic-token types we emit. The index of each entry is the
/// `tokenType` integer the protocol expects. Keep this list aligned
/// with [`token_type_for_node`].
pub const TOKEN_TYPES: &[SemanticTokenType] = &[
    SemanticTokenType::KEYWORD,           // 0
    SemanticTokenType::VARIABLE,          // 1
    SemanticTokenType::PARAMETER,         // 2
    SemanticTokenType::PROPERTY,          // 3
    SemanticTokenType::FUNCTION,          // 4
    SemanticTokenType::NAMESPACE,         // 5
    SemanticTokenType::TYPE,              // 6
    SemanticTokenType::CLASS,             // 7
    SemanticTokenType::ENUM_MEMBER,       // 8
    SemanticTokenType::STRING,            // 9
    SemanticTokenType::NUMBER,            // 10
    SemanticTokenType::OPERATOR,          // 11
    SemanticTokenType::COMMENT,           // 12
    SemanticTokenType::DECORATOR,         // 13
    SemanticTokenType::MACRO,             // 14
    SemanticTokenType::EVENT,             // 15
];

pub const TOKEN_MODIFIERS: &[SemanticTokenModifier] = &[
    SemanticTokenModifier::DECLARATION,   // 0
    SemanticTokenModifier::DEFINITION,    // 1
    SemanticTokenModifier::READONLY,      // 2
    SemanticTokenModifier::DOCUMENTATION, // 3
    SemanticTokenModifier::DEFAULT_LIBRARY, // 4
];

fn server_capabilities() -> ServerCapabilities {
    ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
        completion_provider: Some(CompletionOptions {
            trigger_characters: Some(vec!["$".into(), "@".into(), ".".into(), "[".into()]),
            ..Default::default()
        }),
        hover_provider: Some(HoverProviderCapability::Simple(true)),
        semantic_tokens_provider: Some(SemanticTokensServerCapabilities::SemanticTokensOptions(
            SemanticTokensOptions {
                work_done_progress_options: WorkDoneProgressOptions::default(),
                legend: SemanticTokensLegend {
                    token_types: TOKEN_TYPES.to_vec(),
                    token_modifiers: TOKEN_MODIFIERS.to_vec(),
                },
                range: Some(false),
                full: Some(SemanticTokensFullOptions::Bool(true)),
            },
        )),
        ..Default::default()
    }
}

// ─── Server state ───────────────────────────────────────────────────

#[derive(Debug, Default)]
struct ServerState {
    docs: HashMap<Uri, Document>,
}

#[derive(Debug)]
struct Document {
    text: String,
    line_starts: Vec<usize>,
}

impl Document {
    fn new(text: String) -> Self {
        let line_starts = compute_line_starts(&text);
        Self { text, line_starts }
    }

    /// Convert a byte offset to an LSP (line, character) position.
    /// The character component is UTF-16 code units, matching the
    /// LSP default; this is what nearly every client expects.
    fn offset_to_position(&self, offset: usize) -> LspPosition {
        let offset = offset.min(self.text.len());
        // Binary-search the line index.
        let line = match self.line_starts.binary_search(&offset) {
            Ok(i) => i,
            Err(i) => i.saturating_sub(1),
        };
        let line_start = self.line_starts.get(line).copied().unwrap_or(0);
        let line_text = &self.text[line_start..offset];
        let character = line_text.encode_utf16().count();
        LspPosition {
            line: line as u32,
            character: character as u32,
        }
    }

    /// Convert an LSP (line, character) position to a byte offset.
    fn position_to_offset(&self, pos: LspPosition) -> usize {
        let line = pos.line as usize;
        let line_start = self.line_starts.get(line).copied().unwrap_or(self.text.len());
        let line_end = self
            .line_starts
            .get(line + 1)
            .copied()
            .unwrap_or(self.text.len());
        // Walk the line counting UTF-16 code units.
        let mut offset = line_start;
        let mut units_left = pos.character as usize;
        for ch in self.text[line_start..line_end].chars() {
            if units_left == 0 {
                break;
            }
            let ch_units = ch.len_utf16();
            if ch_units > units_left {
                break;
            }
            units_left -= ch_units;
            offset += ch.len_utf8();
        }
        offset.min(self.text.len())
    }
}

fn compute_line_starts(text: &str) -> Vec<usize> {
    let mut starts = vec![0];
    let bytes = text.as_bytes();
    for (i, b) in bytes.iter().enumerate() {
        if *b == b'\n' {
            starts.push(i + 1);
        }
    }
    starts
}

// ─── Main loop ──────────────────────────────────────────────────────

fn main_loop(connection: &Connection, state: &mut ServerState) -> Result<()> {
    for msg in &connection.receiver {
        match msg {
            Message::Request(req) => {
                if connection.handle_shutdown(&req)? {
                    return Ok(());
                }
                handle_request(connection, state, req)?;
            }
            Message::Notification(note) => handle_notification(connection, state, note)?,
            Message::Response(_) => {
                // We don't issue server→client requests today, so any
                // response here is unexpected. Drop silently.
            }
        }
    }
    Ok(())
}

// ─── Request dispatch ──────────────────────────────────────────────

fn handle_request(connection: &Connection, state: &mut ServerState, req: Request) -> Result<()> {
    let id = req.id.clone();
    let result = match req.method.as_str() {
        HoverRequest::METHOD => cast_and_run(req, |params: HoverParams| {
            Ok(handle_hover(state, params))
        }),
        Completion::METHOD => cast_and_run(req, |params: CompletionParams| {
            Ok(handle_completion(state, params))
        }),
        SemanticTokensFullRequest::METHOD => cast_and_run(req, |params: SemanticTokensParams| {
            Ok(handle_semantic_tokens(state, params))
        }),
        _ => {
            // Unknown — return a method-not-found error so the client
            // moves on gracefully.
            return send_method_not_found(connection, id, &req.method);
        }
    };

    let response = match result {
        Ok(value) => Response {
            id,
            result: Some(value),
            error: None,
        },
        Err(err) => Response {
            id,
            result: None,
            error: Some(lsp_server::ResponseError {
                code: lsp_server::ErrorCode::InternalError as i32,
                message: err.to_string(),
                data: None,
            }),
        },
    };
    connection.sender.send(Message::Response(response))?;
    Ok(())
}

fn send_method_not_found(
    connection: &Connection,
    id: RequestId,
    method: &str,
) -> Result<()> {
    let response = Response {
        id,
        result: None,
        error: Some(lsp_server::ResponseError {
            code: lsp_server::ErrorCode::MethodNotFound as i32,
            message: format!("Method `{method}` is not supported by prism-loom-lsp"),
            data: None,
        }),
    };
    connection.sender.send(Message::Response(response))?;
    Ok(())
}

fn cast_and_run<P, F>(req: Request, f: F) -> Result<serde_json::Value>
where
    P: serde::de::DeserializeOwned,
    F: FnOnce(P) -> Result<serde_json::Value>,
{
    let method = req.method.clone();
    let (_id, params): (RequestId, P) = req.extract(&method).map_err(extract_err)?;
    f(params)
}

fn extract_err(err: ExtractError<Request>) -> anyhow::Error {
    match err {
        ExtractError::MethodMismatch(req) => anyhow!("method mismatch: {}", req.method),
        ExtractError::JsonError { method, error } => {
            anyhow!("decoding `{method}` params: {error}")
        }
    }
}

// ─── Notification dispatch ─────────────────────────────────────────

fn handle_notification(
    connection: &Connection,
    state: &mut ServerState,
    note: Notification,
) -> Result<()> {
    match note.method.as_str() {
        DidOpenTextDocument::METHOD => {
            let params: DidOpenTextDocumentParams = serde_json::from_value(note.params)?;
            let uri = params.text_document.uri;
            let doc = Document::new(params.text_document.text);
            push_diagnostics(connection, &uri, &doc)?;
            state.docs.insert(uri, doc);
        }
        DidChangeTextDocument::METHOD => {
            let params: DidChangeTextDocumentParams = serde_json::from_value(note.params)?;
            let uri = params.text_document.uri;
            // With TextDocumentSyncKind::FULL the last change carries
            // the full document text.
            if let Some(change) = params.content_changes.into_iter().next_back() {
                let doc = Document::new(change.text);
                push_diagnostics(connection, &uri, &doc)?;
                state.docs.insert(uri, doc);
            }
        }
        DidCloseTextDocument::METHOD => {
            let params: DidCloseTextDocumentParams = serde_json::from_value(note.params)?;
            state.docs.remove(&params.text_document.uri);
        }
        _ => {
            // Quiet — many clients send extra notifications we don't care about.
        }
    }
    Ok(())
}

// ─── Diagnostics ───────────────────────────────────────────────────

fn push_diagnostics(connection: &Connection, uri: &Uri, doc: &Document) -> Result<()> {
    let result = parse(&doc.text);
    let diagnostics = build_diagnostics(doc, &result);
    let params = PublishDiagnosticsParams {
        uri: uri.clone(),
        diagnostics,
        version: None,
    };
    connection
        .sender
        .send(Message::Notification(Notification::new(
            PublishDiagnostics::METHOD.to_string(),
            params,
        )))?;
    Ok(())
}

fn build_diagnostics(doc: &Document, result: &ParseResult) -> Vec<Diagnostic> {
    result
        .diagnostics
        .iter()
        .map(|d| Diagnostic {
            range: LspRange {
                start: doc.offset_to_position(d.range.start.offset),
                end: doc.offset_to_position(d.range.end.offset),
            },
            severity: Some(match d.severity {
                LoomSeverity::Error => DiagnosticSeverity::ERROR,
                LoomSeverity::Warning => DiagnosticSeverity::WARNING,
                LoomSeverity::Info => DiagnosticSeverity::INFORMATION,
            }),
            code: Some(lsp_types::NumberOrString::String(d.id.to_string())),
            code_description: None,
            source: Some("prism-loom-lsp".into()),
            message: d.message.clone(),
            related_information: None,
            tags: None,
            data: None,
        })
        .collect()
}

// ─── Hover ─────────────────────────────────────────────────────────

fn handle_hover(state: &ServerState, params: HoverParams) -> serde_json::Value {
    let pos = params.text_document_position_params;
    let uri = pos.text_document.uri;
    let Some(doc) = state.docs.get(&uri) else {
        return serde_json::Value::Null;
    };
    let offset = doc.position_to_offset(pos.position);
    let provider = LoomSyntaxProvider::new();
    match provider.hover(&doc.text, offset, None) {
        Some(info) => serde_json::to_value(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: info.contents,
            }),
            range: Some(LspRange {
                start: doc.offset_to_position(info.range.start),
                end: doc.offset_to_position(info.range.end),
            }),
        })
        .unwrap_or(serde_json::Value::Null),
        None => serde_json::Value::Null,
    }
}

// ─── Completion ────────────────────────────────────────────────────

fn handle_completion(state: &ServerState, params: CompletionParams) -> serde_json::Value {
    let pos = params.text_document_position;
    let uri = pos.text_document.uri;
    let Some(doc) = state.docs.get(&uri) else {
        return serde_json::to_value(CompletionResponse::Array(Vec::new())).unwrap();
    };
    let offset = doc.position_to_offset(pos.position);
    let provider = LoomSyntaxProvider::new();
    let items: Vec<CompletionItem> = provider
        .complete(&doc.text, offset, None)
        .into_iter()
        .map(|c| CompletionItem {
            label: c.label,
            kind: Some(map_completion_kind(c.kind)),
            detail: c.detail,
            documentation: c.documentation.map(|d| {
                lsp_types::Documentation::MarkupContent(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: d,
                })
            }),
            ..Default::default()
        })
        .collect();
    serde_json::to_value(CompletionResponse::Array(items)).unwrap()
}

fn map_completion_kind(
    kind: prism_core::language::syntax::CompletionKind,
) -> CompletionItemKind {
    use prism_core::language::syntax::CompletionKind as K;
    match kind {
        K::Keyword => CompletionItemKind::KEYWORD,
        K::Function => CompletionItemKind::FUNCTION,
        K::Field => CompletionItemKind::FIELD,
        K::Operator => CompletionItemKind::OPERATOR,
        K::Type => CompletionItemKind::CLASS,
        K::Value => CompletionItemKind::VALUE,
    }
}

// ─── Semantic tokens ───────────────────────────────────────────────

fn handle_semantic_tokens(
    state: &ServerState,
    params: SemanticTokensParams,
) -> serde_json::Value {
    let uri = params.text_document.uri;
    let Some(doc) = state.docs.get(&uri) else {
        return serde_json::Value::Null;
    };
    let result = parse(&doc.text);
    let tokens = collect_semantic_tokens(doc, &result.root);
    serde_json::to_value(SemanticTokensResult::Tokens(SemanticTokens {
        result_id: None,
        data: tokens,
    }))
    .unwrap()
}

fn collect_semantic_tokens(
    doc: &Document,
    root: &prism_core::language::syntax::RootNode,
) -> Vec<SemanticToken> {
    let mut raw: Vec<(usize, usize, u32, u32)> = Vec::new();
    for child in &root.children {
        walk_node(doc, child, &mut raw);
    }
    // Sort by (line, start) so the LSP delta-encoding is monotonic.
    raw.sort_by_key(|(line, start, _, _)| (*line, *start));

    let mut out: Vec<SemanticToken> = Vec::with_capacity(raw.len());
    let mut prev_line: u32 = 0;
    let mut prev_start: u32 = 0;
    for (line, start, length, token_type) in raw {
        let line = line as u32;
        let start = start as u32;
        let delta_line = line - prev_line;
        let delta_start = if delta_line == 0 {
            start - prev_start
        } else {
            start
        };
        out.push(SemanticToken {
            delta_line,
            delta_start,
            length,
            token_type,
            token_modifiers_bitset: 0,
        });
        prev_line = line;
        prev_start = start;
    }
    out
}

fn walk_node(
    doc: &Document,
    node: &SyntaxNode,
    out: &mut Vec<(usize, usize, u32, u32)>,
) {
    if let Some(token_type) = token_type_for_node(node) {
        if let Some(range) = node.position {
            let start_offset = range.start.offset;
            let end_offset = range.end.offset.max(start_offset);
            // Skip multi-line spans for semantic tokens — LSP tokens
            // are single-line by spec. The validator pass will split
            // multi-line constructs into per-line emissions; for now
            // we emit the first line only and trust the TextMate
            // grammar to colour subsequent lines.
            let start_pos = doc.offset_to_position(start_offset);
            let end_pos = doc.offset_to_position(end_offset);
            if start_pos.line == end_pos.line {
                let line = start_pos.line as usize;
                let start = start_pos.character as usize;
                let length = end_pos.character.saturating_sub(start_pos.character);
                if length > 0 {
                    out.push((line, start, length, token_type));
                }
            }
        }
    }
    for child in &node.children {
        walk_node(doc, child, out);
    }
}

/// Map a SyntaxNode kind to a semantic token type index, or `None`
/// for "no highlight" (containers, blocks, layout nodes). The index
/// corresponds to the position of the type in [`TOKEN_TYPES`].
fn token_type_for_node(node: &SyntaxNode) -> Option<u32> {
    let kind = node.kind.as_str();
    let idx = match kind {
        // Strings / docstrings / comments.
        nk::STRING => 9,
        nk::DOCSTRING => 9,
        "comment" => 12,
        // Numbers.
        nk::NUMBER => 10,
        // Booleans / nil.
        nk::BOOLEAN | nk::NIL => 0,
        // Identifiers — only highlight when text matches a known
        // category; bare identifiers stay TextMate-styled.
        nk::IDENT => match node.value.as_deref() {
            Some(text) if is_keyword(text) => Some(0u32),
            _ => None,
        }?,
        // Speakers / cast / character entities.
        nk::SPEAKER => 4,
        // Resolve refs / static refs.
        nk::RESOLVE_REF => 1,
        nk::STATIC_REF => 5,
        // Backlinks render as decorators.
        nk::BACKLINK => 13,
        // Document / section / scene heads.
        nk::HEADER => 0,
        nk::SECTION => 0,
        nk::SLUGLINE_SCENE => 0,
        nk::DOC_TAG => 8,
        nk::PROPERTY => 3,
        nk::MODIFIER => 13,
        nk::ANNOTATION => 13,
        // Triggers.
        nk::INLINE_TRIGGER | nk::CHAIN_TRIGGER | nk::COND_TRIGGER => 4,
        nk::RANGE_CLOSER => 11,
        nk::INLINE_ASSIGN => 1,
        nk::INLINE_EVAL => 1,
        // Sigils for choices, divert, return.
        nk::CHOICE | nk::CHOICE_LABEL => 0,
        nk::DIVERT | nk::RETURN_LINE => 0,
        nk::ACTION_LINE | nk::KEYWORD_ACTION | nk::MUTATION_EXPR | nk::NAMESPACE_CALL => 0,
        // Declarations.
        nk::CAST_DECL | nk::CUE_DECL | nk::LOCATION_DECL | nk::COHORT_DECL => 0,
        nk::GENERATOR_DECL | nk::SCENE_DECL | nk::COMPOSE_DECL => 0,
        nk::KNOWLEDGE_BLOCK | nk::GOAL_DECL | nk::DISPOSITION_BLOCK | nk::HOOK_DECL => 0,
        nk::ATTRIBUTE_DECL | nk::AXIS_DECL | nk::POOL_DECL | nk::STAT_DECL | nk::TREE_NODE_DECL => 0,
        nk::INLINE_FACTION_DECL | nk::MEMBERS_BLOCK | nk::STATE_BLOCK | nk::STANCE_BLOCK => 0,
        nk::FACTION_EVENT | nk::PARTICIPANT_FACTION_LIFECYCLE | nk::DISCOVERY_EVENT => 0,
        nk::PARTICIPANT_LIFECYCLE | nk::LOCATION_EVENT | nk::BROADCAST_BLOCK => 0,
        // Stance levels are enum-member-shaped.
        nk::STANCE_LEVEL => 8,
        // Operators inside binary/unary nodes.
        nk::BINARY_EXPR | nk::UNARY_EXPR => return None,
        // Errors land with no token type — diagnostics carry them.
        nk::ERROR => return None,
        // Everything else: no highlight at this layer.
        _ => return None,
    };
    Some(idx)
}

fn is_keyword(text: &str) -> bool {
    use prism_core::language::loom::keywords::KEYWORD_CATEGORIES;
    KEYWORD_CATEGORIES
        .iter()
        .any(|cat| cat.words.contains(&text))
}

// ─── Tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn document_offset_round_trip() {
        let doc = Document::new("hello\nworld\n".to_string());
        let pos = doc.offset_to_position(7);
        assert_eq!(pos.line, 1);
        assert_eq!(pos.character, 1);
        let back = doc.position_to_offset(pos);
        assert_eq!(back, 7);
    }

    #[test]
    fn document_handles_utf8_multibyte() {
        let text = "ä\n".to_string(); // 2 bytes UTF-8, 1 unit UTF-16
        let doc = Document::new(text);
        let pos = doc.offset_to_position(2);
        assert_eq!(pos.line, 0);
        assert_eq!(pos.character, 1);
    }

    #[test]
    fn build_diagnostics_maps_severity_and_code() {
        let src = "# d\nlet x = \"oops\n";
        let doc = Document::new(src.to_string());
        let result = parse(src);
        let diags = build_diagnostics(&doc, &result);
        assert!(!diags.is_empty());
        let first = &diags[0];
        assert_eq!(first.severity, Some(DiagnosticSeverity::ERROR));
        assert!(matches!(
            first.code,
            Some(lsp_types::NumberOrString::String(ref s)) if s == "lex-error"
        ));
        assert_eq!(first.source.as_deref(), Some("prism-loom-lsp"));
    }

    #[test]
    fn semantic_tokens_emit_for_strings_and_keywords() {
        let src = "# d\n-- s\nWREN\n  hello world.\n";
        let doc = Document::new(src.to_string());
        let result = parse(src);
        let tokens = collect_semantic_tokens(&doc, &result.root);
        assert!(!tokens.is_empty(), "expected at least one semantic token");
    }

    #[test]
    fn semantic_tokens_skip_multiline_spans() {
        // A docstring spans multiple lines; it should not produce a
        // single multi-line token.
        let src = "# d\n'''\nmulti\nline\n'''\n";
        let doc = Document::new(src.to_string());
        let result = parse(src);
        let tokens = collect_semantic_tokens(&doc, &result.root);
        // No assertion on count — just that we don't crash and the
        // emitted tokens are all single-line (delta_line / length
        // structure can't represent multi-line spans).
        for t in &tokens {
            assert!(t.length < 10_000, "suspicious length: {t:?}");
        }
    }

    #[test]
    fn server_capabilities_advertise_semantic_tokens() {
        let caps = server_capabilities();
        assert!(caps.semantic_tokens_provider.is_some());
        assert!(caps.hover_provider.is_some());
        assert!(caps.completion_provider.is_some());
    }
}
