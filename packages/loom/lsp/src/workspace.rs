//! Workspace-wide name index + per-document state.
//!
//! Tracks every open document and the cross-file index the LSP
//! request handlers query (spec §17): characters, traits, beats,
//! anchors, todos. Reparses on every edit and emits diagnostics.

use std::collections::HashMap;

use loom_parser::ast::{
    BodyItem, Conditional, Declaration, DeclarationKind, DialogueLine, Item, LoomFile,
};
use loom_parser::{parse, Diagnostic, Position, Severity, Span};
use lsp_types::{
    CompletionItem, DocumentSymbolResponse, GotoDefinitionResponse, Hover,
    PublishDiagnosticsParams, Range, Url,
};

use crate::{completion, definition, hover, symbols};

/// A single open document plus its latest parse.
#[derive(Debug)]
pub struct OpenDoc {
    pub text: String,
    pub file: LoomFile,
    pub diagnostics: Vec<Diagnostic>,
}

/// Cross-file index entry: one occurrence of a name.
#[derive(Clone, Debug)]
pub struct Occurrence {
    pub uri: Url,
    pub range: Range,
}

/// A captured `todo` fence body.
#[derive(Clone, Debug)]
pub struct Todo {
    pub uri: Url,
    pub range: Range,
    pub text: String,
}

/// Per-character snapshot we expose to hover.
#[derive(Clone, Debug)]
pub struct CharacterInfo {
    pub uri: Url,
    pub range: Range,
    /// Declared `is X, Y` mixins.
    pub mixins: Vec<String>,
    /// Raw indented body lines, one per source line.
    pub body: Vec<String>,
}

/// Per-beat snapshot used by hover + definition.
#[derive(Clone, Debug)]
pub struct BeatInfo {
    pub uri: Url,
    pub range: Range,
    pub name_range: Range,
    pub cast: Option<String>,
    pub setting: Option<String>,
}

/// In-memory project index.
#[derive(Debug, Default)]
pub struct Workspace {
    pub docs: HashMap<Url, OpenDoc>,
    pub characters: HashMap<String, CharacterInfo>,
    pub traits: HashMap<String, Occurrence>,
    pub beats: HashMap<String, Vec<BeatInfo>>,
    pub anchors: HashMap<String, Vec<Occurrence>>,
    pub todos: Vec<Todo>,
}

impl Workspace {
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert or replace a document, reparse, rebuild the index.
    pub fn open(&mut self, uri: Url, text: String) {
        self.update(uri, text);
    }

    /// Replace a document's text and reparse.
    pub fn update(&mut self, uri: Url, text: String) {
        let (file, diagnostics) = parse(&text);
        self.docs.insert(
            uri,
            OpenDoc {
                text,
                file,
                diagnostics,
            },
        );
        self.rebuild_index();
    }

    /// Drop a document and rebuild.
    pub fn close(&mut self, uri: &Url) {
        self.docs.remove(uri);
        self.rebuild_index();
    }

    /// Latest diagnostics for a single document, packaged for
    /// `textDocument/publishDiagnostics`.
    pub fn diagnostics_for(&self, uri: &Url) -> Option<PublishDiagnosticsParams> {
        let doc = self.docs.get(uri)?;
        let diagnostics = doc.diagnostics.iter().map(to_lsp_diagnostic).collect();
        Some(PublishDiagnosticsParams {
            uri: uri.clone(),
            diagnostics,
            version: None,
        })
    }

    fn rebuild_index(&mut self) {
        self.characters.clear();
        self.traits.clear();
        self.beats.clear();
        self.anchors.clear();
        self.todos.clear();

        // Collect into temporary owned data so we don't borrow `self.docs`
        // mutably while iterating.
        let snapshot: Vec<(Url, LoomFile, String)> = self
            .docs
            .iter()
            .map(|(uri, doc)| (uri.clone(), doc.file.clone(), doc.text.clone()))
            .collect();

        for (uri, file, text) in snapshot {
            self.index_file(&uri, &file, &text);
        }
    }

    fn index_file(&mut self, uri: &Url, file: &LoomFile, text: &str) {
        for item in &file.items {
            match item {
                Item::Declaration(decl) => self.index_declaration(uri, decl),
                Item::Beat(beat) => {
                    let range = span_to_range(beat.span);
                    let name_range = name_range_in_text(text, beat.span, &beat.name);
                    let info = BeatInfo {
                        uri: uri.clone(),
                        range,
                        name_range,
                        cast: beat.contract.get("cast").map(|v| v.value.clone()),
                        setting: beat.contract.get("setting").map(|v| v.value.clone()),
                    };
                    self.beats.entry(beat.name.clone()).or_default().push(info);
                    for body in &beat.body {
                        self.index_body(uri, body);
                    }
                }
                Item::LetBinding(_) => {}
            }
        }
    }

    fn index_declaration(&mut self, uri: &Url, decl: &Declaration) {
        let range = span_to_range(decl.span);
        match decl.kind {
            DeclarationKind::Character | DeclarationKind::Role => {
                self.characters.insert(
                    decl.name.clone(),
                    CharacterInfo {
                        uri: uri.clone(),
                        range,
                        mixins: decl.mixin.clone(),
                        body: decl.body.iter().map(|l| l.text.clone()).collect(),
                    },
                );
            }
            DeclarationKind::Trait => {
                self.traits.insert(
                    decl.name.clone(),
                    Occurrence {
                        uri: uri.clone(),
                        range,
                    },
                );
            }
            _ => {}
        }
    }

    fn index_body(&mut self, uri: &Url, body: &BodyItem) {
        match body {
            BodyItem::Directive(d) => self.maybe_anchor(uri, &d.raw, d.span),
            BodyItem::DirectiveBlock(b) => {
                self.maybe_anchor(uri, &b.directive.raw, b.directive.span);
                for child in &b.body {
                    self.index_body(uri, child);
                }
            }
            BodyItem::Conditional(Conditional { arms, .. }) => {
                for arm in arms {
                    for child in &arm.body {
                        self.index_body(uri, child);
                    }
                }
            }
            BodyItem::Choice(c) => {
                for child in &c.body {
                    self.index_body(uri, child);
                }
            }
            BodyItem::Metadata(m) => {
                let trimmed = m.value.trim_start();
                if let Some(rest) = trimmed.strip_prefix("```todo") {
                    let text = rest
                        .trim_end_matches("```")
                        .trim_matches(|c: char| c == '\n' || c == '\r' || c == ' ')
                        .to_string();
                    self.todos.push(Todo {
                        uri: uri.clone(),
                        range: span_to_range(m.span),
                        text,
                    });
                }
            }
            BodyItem::Dialogue(block) => {
                for line in &block.lines {
                    if let DialogueLine::Directive(d) = line {
                        self.maybe_anchor(uri, &d.raw, d.span);
                    }
                }
            }
            _ => {}
        }
    }

    fn maybe_anchor(&mut self, uri: &Url, raw: &str, span: Span) {
        // `<anchor: name>` — store the name.
        let inner = raw.trim_start_matches('<').trim_end_matches('>').trim();
        if let Some(args) = inner.strip_prefix("anchor:") {
            let name = args.trim().to_string();
            if !name.is_empty() {
                self.anchors.entry(name).or_default().push(Occurrence {
                    uri: uri.clone(),
                    range: span_to_range(span),
                });
            }
        }
    }

    // ---------- request entry points ----------

    pub fn completion_at(&self, uri: &Url, pos: lsp_types::Position) -> Vec<CompletionItem> {
        completion::completion_at(self, uri, pos)
    }

    pub fn hover_at(&self, uri: &Url, pos: lsp_types::Position) -> Option<Hover> {
        hover::hover_at(self, uri, pos)
    }

    pub fn definition_at(
        &self,
        uri: &Url,
        pos: lsp_types::Position,
    ) -> Option<GotoDefinitionResponse> {
        definition::definition_at(self, uri, pos)
    }

    pub fn document_symbols(&self, uri: &Url) -> Option<DocumentSymbolResponse> {
        symbols::document_symbols(self, uri)
    }

    pub fn references_at(
        &self,
        uri: &Url,
        pos: lsp_types::Position,
    ) -> Vec<lsp_types::Location> {
        crate::references::references_at(self, uri, pos)
    }
}

/// Convert a parser [`Span`] to an LSP [`Range`]. Both use zero-based
/// line + UTF-8 byte column; UTF-16 conversion is deferred until we
/// hit a non-ASCII script. See `source.rs` for the rationale.
pub fn span_to_range(span: Span) -> Range {
    Range {
        start: position_to_lsp(span.start),
        end: position_to_lsp(span.end),
    }
}

pub fn position_to_lsp(p: Position) -> lsp_types::Position {
    lsp_types::Position {
        line: p.line,
        character: p.column,
    }
}

fn to_lsp_diagnostic(d: &Diagnostic) -> lsp_types::Diagnostic {
    lsp_types::Diagnostic {
        range: span_to_range(d.span),
        severity: Some(match d.severity {
            Severity::Error => lsp_types::DiagnosticSeverity::ERROR,
            Severity::Warning => lsp_types::DiagnosticSeverity::WARNING,
        }),
        code: Some(lsp_types::NumberOrString::String(d.code.id().to_string())),
        source: Some("loom".to_string()),
        message: d.message.clone(),
        ..Default::default()
    }
}

/// Locate a `name` substring inside a span's source text so we can
/// return a tight range for go-to-definition / symbol selection.
fn name_range_in_text(text: &str, span: Span, name: &str) -> Range {
    // Walk lines in the span and look for the first occurrence.
    let lines: Vec<&str> = text.lines().collect();
    for line_idx in span.start.line..=span.end.line {
        if let Some(line) = lines.get(line_idx as usize) {
            if let Some(col) = line.find(name) {
                let start = lsp_types::Position {
                    line: line_idx,
                    character: col as u32,
                };
                let end = lsp_types::Position {
                    line: line_idx,
                    character: (col + name.len()) as u32,
                };
                return Range { start, end };
            }
        }
    }
    span_to_range(span)
}

/// Extract the line of text at `pos.line` from `text`, or `""`.
pub fn line_at(text: &str, line: u32) -> &str {
    text.lines().nth(line as usize).unwrap_or("")
}
