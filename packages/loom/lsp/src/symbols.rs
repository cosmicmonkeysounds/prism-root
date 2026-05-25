//! Document symbol tree: beats, characters, traits, stats profiles,
//! scenes, generators per file.

use lsp_types::{DocumentSymbol, DocumentSymbolResponse, SymbolKind, Url};
use loom_parser::ast::{DeclarationKind, Item};

use crate::workspace::{span_to_range, Workspace};

pub fn document_symbols(ws: &Workspace, uri: &Url) -> Option<DocumentSymbolResponse> {
    let doc = ws.docs.get(uri)?;
    let mut symbols = Vec::new();
    for item in &doc.file.items {
        match item {
            Item::Beat(beat) => {
                let range = span_to_range(beat.span);
                symbols.push(symbol(beat.name.clone(), SymbolKind::FUNCTION, range));
            }
            Item::Declaration(decl) => {
                let range = span_to_range(decl.span);
                let kind = match decl.kind {
                    DeclarationKind::Character => SymbolKind::CLASS,
                    DeclarationKind::Trait => SymbolKind::INTERFACE,
                    DeclarationKind::Item => SymbolKind::OBJECT,
                    DeclarationKind::Location => SymbolKind::NAMESPACE,
                    DeclarationKind::Faction => SymbolKind::PACKAGE,
                    DeclarationKind::Stats => SymbolKind::STRUCT,
                    DeclarationKind::Tree => SymbolKind::ENUM,
                    DeclarationKind::Generator => SymbolKind::EVENT,
                    DeclarationKind::Scene => SymbolKind::METHOD,
                    DeclarationKind::Cohort => SymbolKind::ARRAY,
                };
                symbols.push(symbol(decl.name.clone(), kind, range));
            }
            Item::LetBinding(let_b) => {
                let range = span_to_range(let_b.span);
                symbols.push(symbol(let_b.name.clone(), SymbolKind::VARIABLE, range));
            }
        }
    }
    Some(DocumentSymbolResponse::Nested(symbols))
}

#[allow(deprecated)]
fn symbol(name: String, kind: SymbolKind, range: lsp_types::Range) -> DocumentSymbol {
    DocumentSymbol {
        name,
        detail: None,
        kind,
        tags: None,
        deprecated: None,
        range,
        selection_range: range,
        children: None,
    }
}
