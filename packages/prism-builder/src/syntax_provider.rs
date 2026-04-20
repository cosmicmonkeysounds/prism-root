//! `BuilderSyntaxProvider` — compiler-backed, context-aware Slint
//! syntax provider (ADR-006).
//!
//! Extends the lightweight `SlintSyntaxProvider` in prism-core with:
//! - Real compiler diagnostics from `slint-interpreter::Compiler`
//! - Context-aware completions from `ComponentRegistry` schemas
//! - Context-aware hover from component `help_entry()` and field help

use indexmap::IndexMap;
use prism_core::help::HelpEntry;
use prism_core::language::slint_lang::SlintSyntaxProvider;
use prism_core::language::syntax::{
    CompletionItem, CompletionKind, Diagnostic, DiagnosticSeverity, HoverInfo, SchemaContext,
    SyntaxProvider, TextRange,
};

use crate::component::ComponentId;
use crate::registry::{ComponentRegistry, FieldSpec};

struct ComponentSnapshot {
    schema: Vec<FieldSpec>,
    help: Option<HelpEntry>,
}

pub struct BuilderSyntaxProvider {
    base: SlintSyntaxProvider,
    components: IndexMap<ComponentId, ComponentSnapshot>,
}

impl BuilderSyntaxProvider {
    pub fn new(registry: &ComponentRegistry) -> Self {
        let components = registry
            .iter()
            .map(|(id, comp)| {
                let snap = ComponentSnapshot {
                    schema: comp.schema(),
                    help: comp.help_entry(),
                };
                (id.clone(), snap)
            })
            .collect();
        Self {
            base: SlintSyntaxProvider::new(),
            components,
        }
    }
}

impl SyntaxProvider for BuilderSyntaxProvider {
    fn name(&self) -> &str {
        "prism:slint-builder"
    }

    fn diagnose(&self, source: &str, context: Option<&SchemaContext>) -> Vec<Diagnostic> {
        let compiler = slint_interpreter::Compiler::default();
        let result = spin_on::spin_on(
            compiler.build_from_source(source.to_string(), std::path::PathBuf::new()),
        );

        let compiler_diags: Vec<Diagnostic> = result
            .diagnostics()
            .map(|d| {
                let msg = d.to_string();
                let (range, message) = parse_diagnostic_location(source, &msg);
                Diagnostic {
                    message,
                    severity: DiagnosticSeverity::Error,
                    range,
                    code: Some("slint-compiler".into()),
                }
            })
            .collect();

        if !compiler_diags.is_empty() {
            return compiler_diags;
        }

        self.base.diagnose(source, context)
    }

    fn complete(
        &self,
        source: &str,
        offset: usize,
        context: Option<&SchemaContext>,
    ) -> Vec<CompletionItem> {
        let mut items = self.base.complete(source, offset, context);

        let map = crate::render::build_source_map_from_markers(source);
        if let Some(node_id) = map.node_at_offset(offset) {
            if let Some(span) = map.span_for_node(node_id) {
                if let Some(snap) = self.components.get(&span.component) {
                    let prefix = extract_word_prefix(source, offset).to_lowercase();
                    for field in &snap.schema {
                        if prefix.is_empty() || field.key.to_lowercase().starts_with(&prefix) {
                            items.push(CompletionItem {
                                label: field.key.clone(),
                                kind: CompletionKind::Field,
                                detail: Some(format!("{:?}", field.kind)),
                                documentation: field.help.clone(),
                                sort_order: Some(10),
                                replace_range: None,
                                insert_text: None,
                            });
                        }
                    }
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
        context: Option<&SchemaContext>,
    ) -> Option<HoverInfo> {
        let map = crate::render::build_source_map_from_markers(source);
        if let Some(node_id) = map.node_at_offset(offset) {
            if let Some(span) = map.span_for_node(node_id) {
                if let Some(snap) = self.components.get(&span.component) {
                    if let Some((word, start, end)) = extract_word_at(source, offset) {
                        let range = TextRange { start, end };

                        for field in &snap.schema {
                            if field.key == word {
                                let doc = field
                                    .help
                                    .clone()
                                    .unwrap_or_else(|| format!("{:?}", field.kind));
                                return Some(HoverInfo {
                                    range,
                                    contents: format!(
                                        "**{}** ({}) — {}",
                                        field.key, span.component, doc
                                    ),
                                });
                            }
                        }

                        if let Some(help) = &snap.help {
                            if word == span.component || word == *node_id {
                                return Some(HoverInfo {
                                    range,
                                    contents: format!("**{}**\n\n{}", help.title, help.summary),
                                });
                            }
                        }
                    }
                }
            }
        }

        self.base.hover(source, offset, context)
    }
}

/// Try to parse `path:line:col: level: message` from a Slint compiler
/// diagnostic string. Returns `(TextRange, cleaned_message)`.
fn parse_diagnostic_location(source: &str, msg: &str) -> (TextRange, String) {
    let parts: Vec<&str> = msg.splitn(4, ':').collect();
    if parts.len() >= 4 {
        if let (Ok(line_1), Ok(col_1)) = (
            parts[1].trim().parse::<usize>(),
            parts[2].trim().parse::<usize>(),
        ) {
            let line_0 = line_1.saturating_sub(1);
            let col_0 = col_1.saturating_sub(1);
            let offset = crate::live::line_col_to_byte_offset(source, line_0, col_0);
            let end = (offset + 1).min(source.len());
            let rest = parts[3..].join(":");
            let message = rest
                .trim()
                .trim_start_matches("error:")
                .trim_start_matches("warning:")
                .trim()
                .to_string();
            return (TextRange { start: offset, end }, message);
        }
    }
    (TextRange { start: 0, end: 0 }, msg.to_string())
}

fn extract_word_prefix(source: &str, offset: usize) -> String {
    let before = &source[..offset.min(source.len())];
    let start = before
        .rfind(|c: char| !c.is_alphanumeric() && c != '_' && c != '-')
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
        .rfind(|c: char| !c.is_alphanumeric() && c != '_' && c != '-')
        .map(|i| i + 1)
        .unwrap_or(0);
    let after = &source[offset..];
    let end_offset = after
        .find(|c: char| !c.is_alphanumeric() && c != '_' && c != '-')
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
    use crate::component::{Component, RenderSlintContext};
    use crate::document::{BuilderDocument, Node};
    use crate::slint_source::SlintEmitter;
    use prism_core::design_tokens::DesignTokens;
    use serde_json::json;
    use std::sync::Arc;

    struct TestButton {
        id: ComponentId,
    }
    impl Component for TestButton {
        fn id(&self) -> &ComponentId {
            &self.id
        }
        fn schema(&self) -> Vec<FieldSpec> {
            vec![
                FieldSpec::text("text", "Button text"),
                FieldSpec::boolean("enabled", "Whether enabled"),
            ]
        }
        fn help_entry(&self) -> Option<HelpEntry> {
            Some(HelpEntry::new(
                "button",
                "Button",
                "A clickable push button",
            ))
        }
        fn render_slint(
            &self,
            _ctx: &RenderSlintContext<'_>,
            props: &serde_json::Value,
            _children: &[Node],
            out: &mut SlintEmitter,
        ) -> Result<(), crate::component::RenderError> {
            let text = props.get("text").and_then(|v| v.as_str()).unwrap_or("");
            out.block("Rectangle", |out| {
                out.block("Text", |out| {
                    out.prop_string("text", text);
                    Ok(())
                })
            })
        }
    }

    fn test_registry() -> ComponentRegistry {
        let mut reg = ComponentRegistry::new();
        reg.register(Arc::new(TestButton {
            id: "button".into(),
        }))
        .unwrap();
        reg
    }

    fn make_source_with_markers() -> String {
        let reg = test_registry();
        let tokens = DesignTokens::default();
        let doc = BuilderDocument {
            root: Some(Node {
                id: "n1".into(),
                component: "button".into(),
                props: json!({ "text": "Click" }),
                children: vec![],
                ..Default::default()
            }),
            ..Default::default()
        };
        let (source, _) =
            crate::render::render_document_slint_source_mapped(&doc, &reg, &tokens).unwrap();
        source
    }

    #[test]
    fn provider_name() {
        let reg = test_registry();
        let p = BuilderSyntaxProvider::new(&reg);
        assert_eq!(p.name(), "prism:slint-builder");
    }

    #[test]
    fn diagnose_valid_source() {
        let reg = test_registry();
        let p = BuilderSyntaxProvider::new(&reg);
        let source = make_source_with_markers();
        let diags = p.diagnose(&source, None);
        assert!(
            diags.is_empty(),
            "valid source should have no errors: {diags:?}"
        );
    }

    #[test]
    fn diagnose_invalid_source() {
        let reg = test_registry();
        let p = BuilderSyntaxProvider::new(&reg);
        let diags = p.diagnose("not valid slint {{{", None);
        assert!(!diags.is_empty());
    }

    #[test]
    fn complete_schema_fields_in_context() {
        let reg = test_registry();
        let p = BuilderSyntaxProvider::new(&reg);
        let source = make_source_with_markers();

        let span = crate::render::build_source_map_from_markers(&source)
            .span_for_node("n1")
            .unwrap()
            .clone();
        let mid = (span.start + span.end) / 2;

        let items = p.complete(&source, mid, None);
        assert!(
            items.iter().any(|i| i.label == "text"),
            "should offer 'text' from button schema"
        );
        assert!(
            items.iter().any(|i| i.label == "enabled"),
            "should offer 'enabled' from button schema"
        );
    }

    #[test]
    fn complete_outside_node_has_no_schema_fields() {
        let reg = test_registry();
        let p = BuilderSyntaxProvider::new(&reg);
        let items = p.complete("Rect", 4, None);
        assert!(
            !items
                .iter()
                .any(|i| i.label == "text" && i.sort_order == Some(10)),
            "should not offer schema fields outside a node"
        );
    }

    #[test]
    fn hover_schema_field() {
        let reg = test_registry();
        let p = BuilderSyntaxProvider::new(&reg);
        let source = make_source_with_markers();

        let text_pos = source.find("\"Click\"").unwrap();
        let prop_key_pos = source[..text_pos].rfind("text").unwrap();

        let hover = p.hover(&source, prop_key_pos + 1, None);
        assert!(hover.is_some());
        let info = hover.unwrap();
        assert!(info.contents.contains("text"));
        assert!(info.contents.contains("button"));
    }

    #[test]
    fn hover_falls_back_to_base() {
        let reg = test_registry();
        let p = BuilderSyntaxProvider::new(&reg);
        let hover = p.hover("Rectangle { }", 4, None);
        assert!(hover.is_some());
        assert!(hover.unwrap().contents.contains("element"));
    }

    #[test]
    fn parse_diagnostic_location_extracts_line_col() {
        let source = "line0\nline1\nline2";
        let (range, msg) = parse_diagnostic_location(source, ":2:3: error: bad token");
        assert_eq!(msg, "bad token");
        assert!(range.start > 0);
    }

    #[test]
    fn parse_diagnostic_location_no_location() {
        let (range, msg) = parse_diagnostic_location("", "some error");
        assert_eq!(range, TextRange { start: 0, end: 0 });
        assert_eq!(msg, "some error");
    }
}
