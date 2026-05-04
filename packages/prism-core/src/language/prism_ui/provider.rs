//! `PrismUiSyntaxProvider` — diagnostics, completions, and hover for
//! `.prism-ui` files.
//!
//! Mirrors the `SlintSyntaxProvider` shape: lightweight, no runtime
//! dependency, intended to give the editor enough intelligence to
//! navigate the DSL while the heavier `prism-builder` provider layers
//! component-registry context on top.

use super::ast::AttributeNamespace;
use super::grammar::parse;
use crate::language::syntax::{
    CompletionItem, CompletionKind, Diagnostic, DiagnosticSeverity, HoverInfo, SchemaContext,
    SyntaxProvider, TextRange,
};

#[derive(Debug, Clone, Default)]
pub struct PrismUiSyntaxProvider;

impl PrismUiSyntaxProvider {
    pub fn new() -> Self {
        Self
    }
}

const PRISM_UI_TAGS: &[(&str, &str)] = &[
    (
        "component",
        "Declares a reusable component (`<component name=\"...\" props=\"...\">`)",
    ),
    ("signals", "Declares signals on the enclosing component"),
    (
        "container",
        "Layout container — flow / grid / scroll children",
    ),
    ("heading", "Heading element — `level=\"1\"..\"6\"`"),
    ("text", "Text run"),
    ("link", "Hyperlink — `href=\"...\"`"),
    ("image", "Image element — `src=\"...\"`"),
    ("button", "Button — supports `on:click`"),
    ("input", "Form input — supports `bind:value`"),
    ("form", "Form container"),
    ("card", "Card surface"),
    ("pill", "Pill / badge"),
    ("divider", "Horizontal / vertical divider"),
    ("spacer", "Empty stretchable spacer"),
    ("columns", "Multi-column layout"),
    ("list", "List / repeated items"),
    ("table", "Tabular data"),
    ("tabs", "Tab strip"),
    (
        "facet",
        "Facet binding (`<facet name=\"...\" from=\"resource:...\">`)",
    ),
];

const PRISM_UI_ATTR_NAMESPACES: &[(&str, &str)] = &[
    ("on:", "Signal connection (`on:click=\"emit save\"`)"),
    ("bind:", "Two-way binding (`bind:value=\"form.email\"`)"),
    (
        "style:",
        "Token-resolved style (`style:background=\"{tokens.colors.surface}\"`)",
    ),
    ("fct:", "Facet binding (`fct:items=\"resource:posts\"`)"),
    ("sig:", "Declared signal shorthand"),
    ("aria:", "ARIA attribute, passes through to HTML lowering"),
    ("data:", "data-* attribute, passes through to HTML lowering"),
];

const PRISM_UI_KEYWORDS: &[(&str, &str)] = &[
    ("if", "Conditional — render if expression is truthy"),
    ("else-if", "Else-if branch in conditional cascade"),
    ("else", "Else branch in conditional cascade"),
    ("for", "Repeat — `for=\"item in items\"`"),
    ("class", "CSS-style class addressing"),
    ("id", "Unique identifier addressing"),
];

impl SyntaxProvider for PrismUiSyntaxProvider {
    fn name(&self) -> &str {
        "prism:prism-ui"
    }

    fn diagnose(&self, source: &str, _context: Option<&SchemaContext>) -> Vec<Diagnostic> {
        let (_doc, errors) = parse(source);
        errors
            .into_iter()
            .map(|err| Diagnostic {
                message: err.message,
                severity: DiagnosticSeverity::Error,
                range: TextRange {
                    start: err.range.start.offset,
                    end: err.range.end.offset.max(err.range.start.offset),
                },
                code: Some(err.code.to_string()),
            })
            .collect()
    }

    fn complete(
        &self,
        source: &str,
        offset: usize,
        _context: Option<&SchemaContext>,
    ) -> Vec<CompletionItem> {
        let mut items = Vec::new();
        let in_tag_position = looks_like_tag_position(source, offset);
        let in_attr_position = looks_like_attribute_position(source, offset);
        let prefix = extract_word_prefix(source, offset);

        if in_tag_position {
            let lower = prefix.to_lowercase();
            for &(tag, doc) in PRISM_UI_TAGS {
                if tag.starts_with(&lower) {
                    items.push(CompletionItem {
                        label: tag.to_string(),
                        kind: CompletionKind::Type,
                        detail: Some("element".into()),
                        documentation: Some(doc.to_string()),
                        sort_order: Some(50),
                        replace_range: None,
                        insert_text: None,
                    });
                }
            }
        }

        if in_attr_position {
            let lower = prefix.to_lowercase();
            for &(ns, doc) in PRISM_UI_ATTR_NAMESPACES {
                if ns.starts_with(&lower) || lower.is_empty() {
                    items.push(CompletionItem {
                        label: ns.to_string(),
                        kind: CompletionKind::Field,
                        detail: Some("attribute namespace".into()),
                        documentation: Some(doc.to_string()),
                        sort_order: Some(60),
                        replace_range: None,
                        insert_text: None,
                    });
                }
            }
            for &(kw, doc) in PRISM_UI_KEYWORDS {
                if kw.starts_with(&lower) {
                    items.push(CompletionItem {
                        label: kw.to_string(),
                        kind: CompletionKind::Keyword,
                        detail: Some("attribute keyword".into()),
                        documentation: Some(doc.to_string()),
                        sort_order: Some(70),
                        replace_range: None,
                        insert_text: None,
                    });
                }
            }
        }

        items.sort_by(|a, b| a.sort_order.cmp(&b.sort_order).then(a.label.cmp(&b.label)));
        items
    }

    fn hover(
        &self,
        source: &str,
        offset: usize,
        _context: Option<&SchemaContext>,
    ) -> Option<HoverInfo> {
        let (word, start, end) = extract_word_at(source, offset)?;
        let range = TextRange { start, end };

        for &(tag, doc) in PRISM_UI_TAGS {
            if tag == word {
                return Some(HoverInfo {
                    range,
                    contents: format!("**<{tag}>** (element)\n\n{doc}"),
                });
            }
        }

        for &(kw, doc) in PRISM_UI_KEYWORDS {
            if kw == word {
                return Some(HoverInfo {
                    range,
                    contents: format!("**{kw}** (attribute keyword)\n\n{doc}"),
                });
            }
        }

        // Namespaced attribute hover — `on:click`, `style:background`, ...
        if let Some((ns_prefix, _local)) = word.split_once(':') {
            let with_colon = format!("{ns_prefix}:");
            for &(ns, doc) in PRISM_UI_ATTR_NAMESPACES {
                if ns == with_colon {
                    let (kind, _) = AttributeNamespace::classify(&word);
                    return Some(HoverInfo {
                        range,
                        contents: format!("**{word}** ({kind:?})\n\n{doc}"),
                    });
                }
            }
        }

        None
    }
}

fn looks_like_tag_position(source: &str, offset: usize) -> bool {
    let before = &source[..offset.min(source.len())];
    if let Some(rest) = before.rsplit_once('<') {
        // `<` was the most recent special char. We're in tag position
        // if no whitespace or `>` has appeared between then and now.
        let after_lt = rest.1;
        return !after_lt.contains(|c: char| c.is_whitespace() || c == '>' || c == '/');
    }
    false
}

fn looks_like_attribute_position(source: &str, offset: usize) -> bool {
    let before = &source[..offset.min(source.len())];
    let last_lt = before.rfind('<');
    let last_gt = before.rfind('>');
    match (last_lt, last_gt) {
        (Some(lt), Some(gt)) => lt > gt && before[lt..].contains(char::is_whitespace),
        (Some(lt), None) => before[lt..].contains(char::is_whitespace),
        _ => false,
    }
}

fn extract_word_prefix(source: &str, offset: usize) -> String {
    let before = &source[..offset.min(source.len())];
    let start = before
        .rfind(|c: char| !c.is_alphanumeric() && c != '_' && c != '-' && c != ':')
        .map(|i| i + 1)
        .unwrap_or(0);
    before[start..].to_string()
}

fn extract_word_at(source: &str, offset: usize) -> Option<(String, usize, usize)> {
    if offset > source.len() {
        return None;
    }
    let before = &source[..offset];
    let start = before
        .rfind(|c: char| !c.is_alphanumeric() && c != '_' && c != '-' && c != ':')
        .map(|i| i + 1)
        .unwrap_or(0);
    let after = &source[offset..];
    let end_offset = after
        .find(|c: char| !c.is_alphanumeric() && c != '_' && c != '-' && c != ':')
        .unwrap_or(after.len());
    let end = offset + end_offset;
    if start == end {
        return None;
    }
    Some((source[start..end].to_string(), start, end))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_name() {
        let p = PrismUiSyntaxProvider::new();
        assert_eq!(p.name(), "prism:prism-ui");
    }

    #[test]
    fn diagnose_clean_source() {
        let p = PrismUiSyntaxProvider::new();
        let diags = p.diagnose(r#"<button label="Save"/>"#, None);
        assert!(diags.is_empty());
    }

    #[test]
    fn diagnose_unclosed_element() {
        let p = PrismUiSyntaxProvider::new();
        let diags = p.diagnose(r#"<container>"#, None);
        assert!(!diags.is_empty());
        assert!(diags.iter().any(|d| d.message.contains("Unclosed")));
    }

    #[test]
    fn diagnose_mismatched_close() {
        let p = PrismUiSyntaxProvider::new();
        let diags = p.diagnose(r#"<container></heading>"#, None);
        assert!(diags.iter().any(|d| d.message.contains("Mismatched")));
    }

    #[test]
    fn complete_tag_after_open_angle() {
        let p = PrismUiSyntaxProvider::new();
        let src = "<butt";
        let items = p.complete(src, src.len(), None);
        assert!(items.iter().any(|i| i.label == "button"));
    }

    #[test]
    fn complete_attribute_namespace_after_whitespace() {
        let p = PrismUiSyntaxProvider::new();
        let src = "<button on";
        let items = p.complete(src, src.len(), None);
        assert!(items.iter().any(|i| i.label == "on:"));
    }

    #[test]
    fn hover_tag() {
        let p = PrismUiSyntaxProvider::new();
        let hover = p.hover("<container>", 5, None);
        assert!(hover.is_some());
        assert!(hover.unwrap().contents.contains("element"));
    }

    #[test]
    fn hover_attribute_keyword() {
        let p = PrismUiSyntaxProvider::new();
        let hover = p.hover("if=\"{x}\"", 1, None);
        assert!(hover.is_some());
        assert!(hover.unwrap().contents.contains("attribute keyword"));
    }

    #[test]
    fn hover_namespaced_attribute() {
        let p = PrismUiSyntaxProvider::new();
        let hover = p.hover("on:click", 4, None);
        assert!(hover.is_some());
        let contents = hover.unwrap().contents;
        assert!(contents.contains("on:click"));
    }

    #[test]
    fn hover_unknown_returns_none() {
        let p = PrismUiSyntaxProvider::new();
        assert!(p.hover("zzz", 1, None).is_none());
    }

    #[test]
    fn complete_empty_outside_element_returns_nothing() {
        let p = PrismUiSyntaxProvider::new();
        let items = p.complete("plain text", 5, None);
        assert!(items.is_empty());
    }
}
