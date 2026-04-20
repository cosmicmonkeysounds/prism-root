//! Live bidirectional document (ADR-006).
//!
//! [`LiveDocument`] unifies:
//! - A [`BuilderDocument`] (the structured node tree)
//! - Generated `.slint` source with embedded node markers
//! - A [`SourceMap`] bridging node IDs ↔ source byte ranges
//! - An [`EditorState`] (ropey-backed buffer for the code panel)
//! - Compiled [`ComponentDefinition`] from `slint-interpreter`
//! - Compiler diagnostics with line/column positions
//!
//! Mutation flows:
//! - **GUI→Source**: mutate the document, call [`rebuild`] to
//!   regenerate source + source map + sync editor + recompile.
//! - **Source→Preview**: edit via the [`editor`] field, call
//!   [`apply_source_edit`] to sync source + recompile (Phase 2
//!   will roundtrip back to `BuilderDocument`).

use prism_core::design_tokens::DesignTokens;
use prism_core::editor::EditorState;

use crate::document::{BuilderDocument, NodeId};
use crate::registry::ComponentRegistry;
use crate::render::{compile_slint_source, InstantiateError};
use crate::source_map::SourceMap;

#[derive(Debug, Clone)]
pub struct LiveDiagnostic {
    pub message: String,
    pub line: Option<usize>,
    pub column: Option<usize>,
}

/// Line/column range in the source for editor selection highlighting.
/// Used to bridge canvas node selection ↔ code editor selection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceSelection {
    pub start_line: usize,
    pub start_col: usize,
    pub end_line: usize,
    pub end_col: usize,
}

pub struct LiveDocument {
    pub document: BuilderDocument,
    pub source: String,
    pub source_map: SourceMap,
    pub editor: EditorState,
    pub diagnostics: Vec<LiveDiagnostic>,
    compiled: Option<slint_interpreter::ComponentDefinition>,
}

impl LiveDocument {
    pub fn new(
        document: BuilderDocument,
        registry: &ComponentRegistry,
        tokens: &DesignTokens,
    ) -> Self {
        let mut editor = EditorState::new();
        editor.language = "slint".to_string();
        let mut live = Self {
            document,
            source: String::new(),
            source_map: SourceMap::new(),
            editor,
            diagnostics: Vec::new(),
            compiled: None,
        };
        let _ = live.rebuild(registry, tokens);
        live
    }

    /// Regenerate source + source map from the current document,
    /// sync the editor buffer, then recompile.
    pub fn rebuild(
        &mut self,
        registry: &ComponentRegistry,
        tokens: &DesignTokens,
    ) -> Result<(), InstantiateError> {
        match crate::render::render_document_slint_source_mapped(&self.document, registry, tokens) {
            Ok((source, map)) => {
                self.source = source;
                self.source_map = map;
                self.sync_editor();
                self.recompile()
            }
            Err(e) => {
                self.diagnostics = vec![LiveDiagnostic {
                    message: e.to_string(),
                    line: None,
                    column: None,
                }];
                Err(InstantiateError::Render(e))
            }
        }
    }

    /// Recompile the current source text, extracting structured
    /// diagnostics with line/column positions from the compiler.
    pub fn recompile(&mut self) -> Result<(), InstantiateError> {
        self.diagnostics.clear();
        match compile_slint_source(&self.source) {
            Ok(def) => {
                self.compiled = Some(def);
                Ok(())
            }
            Err(e) => {
                self.compiled = None;
                self.diagnostics.push(LiveDiagnostic {
                    message: e.to_string(),
                    line: None,
                    column: None,
                });
                Err(e)
            }
        }
    }

    /// Apply the current editor buffer as the new source text.
    /// Rebuilds the source map from markers and recompiles.
    /// Call after the user edits source in the code panel.
    pub fn apply_source_edit(&mut self) -> Result<(), InstantiateError> {
        self.source = self.editor.text();
        self.source_map = crate::render::build_source_map_from_markers(&self.source);
        self.recompile()
    }

    pub fn compiled(&self) -> Option<&slint_interpreter::ComponentDefinition> {
        self.compiled.as_ref()
    }

    pub fn has_errors(&self) -> bool {
        !self.diagnostics.is_empty()
    }

    /// Find the source byte range for a node.
    pub fn source_for_node(&self, node_id: &str) -> Option<&str> {
        let span = self.source_map.span_for_node(node_id)?;
        self.source.get(span.start..span.end)
    }

    /// Find which node a source byte offset belongs to.
    pub fn node_at_offset(&self, offset: usize) -> Option<&str> {
        self.source_map.node_at_offset(offset)
    }

    /// Find which node the editor cursor is currently inside.
    pub fn node_at_cursor(&self) -> Option<&str> {
        let offset = line_col_to_byte_offset(
            &self.source,
            self.editor.cursor.position.line,
            self.editor.cursor.position.col,
        );
        self.source_map.node_at_offset(offset)
    }

    /// Get the editor line/col range for a node (for highlighting
    /// in the code panel when a canvas node is selected).
    pub fn select_node(&self, node_id: &str) -> Option<SourceSelection> {
        let span = self.source_map.span_for_node(node_id)?;
        let (sl, sc) = byte_offset_to_line_col(&self.source, span.start);
        let (el, ec) = byte_offset_to_line_col(&self.source, span.end);
        Some(SourceSelection {
            start_line: sl,
            start_col: sc,
            end_line: el,
            end_col: ec,
        })
    }

    /// Apply a property edit to the document tree. After calling this,
    /// call [`rebuild`] to regenerate source and recompile.
    pub fn set_prop(&mut self, node_id: &str, key: &str, value: serde_json::Value) -> bool {
        if let Some(root) = &mut self.document.root {
            if let Some(node) = root.find_mut(node_id) {
                if let Some(obj) = node.props.as_object_mut() {
                    obj.insert(key.to_string(), value);
                    return true;
                }
            }
        }
        false
    }

    /// Convenience: set a prop and rebuild in one call.
    pub fn edit_prop(
        &mut self,
        node_id: &str,
        key: &str,
        value: serde_json::Value,
        registry: &ComponentRegistry,
        tokens: &DesignTokens,
    ) -> Result<bool, InstantiateError> {
        if !self.set_prop(node_id, key, value) {
            return Ok(false);
        }
        self.rebuild(registry, tokens)?;
        Ok(true)
    }

    pub fn add_node(&mut self, parent_id: &str, node: crate::document::Node) -> bool {
        if let Some(root) = &mut self.document.root {
            if let Some(parent) = root.find_mut(parent_id) {
                parent.children.push(node);
                return true;
            }
        }
        false
    }

    pub fn remove_node(&mut self, node_id: &NodeId) -> bool {
        if let Some(root) = &mut self.document.root {
            return remove_node_recursive(root, node_id);
        }
        false
    }

    fn sync_editor(&mut self) {
        self.editor.set_text(&self.source);
        self.editor.language = "slint".to_string();
    }
}

fn remove_node_recursive(parent: &mut crate::document::Node, target: &str) -> bool {
    if let Some(idx) = parent.children.iter().position(|c| c.id == target) {
        parent.children.remove(idx);
        return true;
    }
    for child in &mut parent.children {
        if remove_node_recursive(child, target) {
            return true;
        }
    }
    false
}

/// Convert a byte offset in source to (line, col), both 0-based.
pub fn byte_offset_to_line_col(source: &str, offset: usize) -> (usize, usize) {
    let clamped = offset.min(source.len());
    let before = &source[..clamped];
    let line = before.matches('\n').count();
    let col = before
        .rfind('\n')
        .map(|nl| clamped - nl - 1)
        .unwrap_or(clamped);
    (line, col)
}

/// Convert (line, col) to a byte offset in source, both 0-based.
pub fn line_col_to_byte_offset(source: &str, line: usize, col: usize) -> usize {
    let mut offset = 0;
    for (i, l) in source.split('\n').enumerate() {
        if i == line {
            return offset + col.min(l.len());
        }
        offset += l.len() + 1;
    }
    source.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Node;
    use crate::registry::FieldSpec;
    use serde_json::json;
    use std::sync::Arc;

    struct TestHeading {
        id: crate::component::ComponentId,
    }
    impl crate::component::Component for TestHeading {
        fn id(&self) -> &crate::component::ComponentId {
            &self.id
        }
        fn schema(&self) -> Vec<FieldSpec> {
            vec![FieldSpec::text("text", "Text")]
        }
        fn render_slint(
            &self,
            _ctx: &crate::component::RenderSlintContext<'_>,
            props: &serde_json::Value,
            _children: &[Node],
            out: &mut crate::slint_source::SlintEmitter,
        ) -> Result<(), crate::component::RenderError> {
            let text = props.get("text").and_then(|v| v.as_str()).unwrap_or("");
            out.block("Text", |out| {
                out.prop_string("text", text);
                out.prop_px("font-size", 24.0);
                Ok(())
            })
        }
    }

    fn test_registry() -> ComponentRegistry {
        let mut reg = ComponentRegistry::new();
        reg.register(Arc::new(TestHeading {
            id: "heading".into(),
        }))
        .unwrap();
        reg
    }

    #[test]
    fn live_document_builds_source() {
        let doc = BuilderDocument {
            root: Some(Node {
                id: "n1".into(),
                component: "heading".into(),
                props: json!({ "text": "Hello" }),
                children: vec![],
                ..Default::default()
            }),
            ..Default::default()
        };
        let reg = test_registry();
        let tokens = DesignTokens::default();
        let live = LiveDocument::new(doc, &reg, &tokens);

        assert!(!live.source.is_empty());
        assert!(live.source.contains("Hello"));
        assert!(live.source_map.span_for_node("n1").is_some());
        assert!(!live.has_errors());
    }

    #[test]
    fn editor_syncs_with_source() {
        let doc = BuilderDocument {
            root: Some(Node {
                id: "n1".into(),
                component: "heading".into(),
                props: json!({ "text": "Synced" }),
                children: vec![],
                ..Default::default()
            }),
            ..Default::default()
        };
        let reg = test_registry();
        let tokens = DesignTokens::default();
        let live = LiveDocument::new(doc, &reg, &tokens);

        assert_eq!(live.editor.text(), live.source);
        assert_eq!(live.editor.language, "slint");
        assert!(live.editor.text().contains("Synced"));
    }

    #[test]
    fn source_for_node_returns_slice() {
        let doc = BuilderDocument {
            root: Some(Node {
                id: "n1".into(),
                component: "heading".into(),
                props: json!({ "text": "Test" }),
                children: vec![],
                ..Default::default()
            }),
            ..Default::default()
        };
        let reg = test_registry();
        let tokens = DesignTokens::default();
        let live = LiveDocument::new(doc, &reg, &tokens);

        let slice = live.source_for_node("n1").unwrap();
        assert!(slice.contains("heading"));
        assert!(slice.contains("Test"));
    }

    #[test]
    fn set_prop_and_rebuild() {
        let doc = BuilderDocument {
            root: Some(Node {
                id: "n1".into(),
                component: "heading".into(),
                props: json!({ "text": "Before" }),
                children: vec![],
                ..Default::default()
            }),
            ..Default::default()
        };
        let reg = test_registry();
        let tokens = DesignTokens::default();
        let mut live = LiveDocument::new(doc, &reg, &tokens);

        assert!(live.source.contains("Before"));
        live.edit_prop("n1", "text", json!("After"), &reg, &tokens)
            .unwrap();
        assert!(live.source.contains("After"));
        assert!(!live.source.contains("Before"));
        assert_eq!(live.editor.text(), live.source);
    }

    #[test]
    fn node_at_offset_works() {
        let doc = BuilderDocument {
            root: Some(Node {
                id: "n1".into(),
                component: "heading".into(),
                props: json!({ "text": "Hi" }),
                children: vec![],
                ..Default::default()
            }),
            ..Default::default()
        };
        let reg = test_registry();
        let tokens = DesignTokens::default();
        let live = LiveDocument::new(doc, &reg, &tokens);

        let span = live.source_map.span_for_node("n1").unwrap();
        let mid = (span.start + span.end) / 2;
        assert_eq!(live.node_at_offset(mid), Some("n1"));
    }

    #[test]
    fn select_node_returns_line_col_range() {
        let doc = BuilderDocument {
            root: Some(Node {
                id: "n1".into(),
                component: "heading".into(),
                props: json!({ "text": "Select" }),
                children: vec![],
                ..Default::default()
            }),
            ..Default::default()
        };
        let reg = test_registry();
        let tokens = DesignTokens::default();
        let live = LiveDocument::new(doc, &reg, &tokens);

        let sel = live.select_node("n1").unwrap();
        assert!(sel.start_line <= sel.end_line);
        assert!(sel.end_line > 0 || sel.end_col > sel.start_col);
    }

    #[test]
    fn select_node_missing_returns_none() {
        let doc = BuilderDocument {
            root: Some(Node {
                id: "n1".into(),
                component: "heading".into(),
                props: json!({ "text": "X" }),
                children: vec![],
                ..Default::default()
            }),
            ..Default::default()
        };
        let reg = test_registry();
        let tokens = DesignTokens::default();
        let live = LiveDocument::new(doc, &reg, &tokens);
        assert!(live.select_node("nonexistent").is_none());
    }

    #[test]
    fn node_at_cursor_finds_node() {
        let doc = BuilderDocument {
            root: Some(Node {
                id: "n1".into(),
                component: "heading".into(),
                props: json!({ "text": "Cursor" }),
                children: vec![],
                ..Default::default()
            }),
            ..Default::default()
        };
        let reg = test_registry();
        let tokens = DesignTokens::default();
        let mut live = LiveDocument::new(doc, &reg, &tokens);

        let sel = live.select_node("n1").unwrap();
        let mid_line = (sel.start_line + sel.end_line) / 2;
        live.editor.set_cursor_position(mid_line, 0);
        assert_eq!(live.node_at_cursor(), Some("n1"));
    }

    #[test]
    fn apply_source_edit_recompiles() {
        let doc = BuilderDocument {
            root: Some(Node {
                id: "n1".into(),
                component: "heading".into(),
                props: json!({ "text": "Original" }),
                children: vec![],
                ..Default::default()
            }),
            ..Default::default()
        };
        let reg = test_registry();
        let tokens = DesignTokens::default();
        let mut live = LiveDocument::new(doc, &reg, &tokens);
        assert!(!live.has_errors());

        let new_source = live.source.replace("Original", "Edited");
        live.editor.set_text(&new_source);
        live.apply_source_edit().unwrap();
        assert!(live.source.contains("Edited"));
        assert!(!live.has_errors());
    }

    #[test]
    fn apply_source_edit_bad_source_reports_error() {
        let doc = BuilderDocument {
            root: Some(Node {
                id: "n1".into(),
                component: "heading".into(),
                props: json!({ "text": "OK" }),
                children: vec![],
                ..Default::default()
            }),
            ..Default::default()
        };
        let reg = test_registry();
        let tokens = DesignTokens::default();
        let mut live = LiveDocument::new(doc, &reg, &tokens);

        live.editor.set_text("this is not valid slint {{{");
        let result = live.apply_source_edit();
        assert!(result.is_err());
        assert!(live.has_errors());
    }

    #[test]
    fn byte_offset_to_line_col_first_line() {
        assert_eq!(byte_offset_to_line_col("hello", 0), (0, 0));
        assert_eq!(byte_offset_to_line_col("hello", 3), (0, 3));
    }

    #[test]
    fn byte_offset_to_line_col_second_line() {
        assert_eq!(byte_offset_to_line_col("ab\ncd\nef", 3), (1, 0));
        assert_eq!(byte_offset_to_line_col("ab\ncd\nef", 4), (1, 1));
        assert_eq!(byte_offset_to_line_col("ab\ncd\nef", 6), (2, 0));
    }

    #[test]
    fn line_col_to_byte_offset_roundtrip() {
        let src = "line one\nline two\nline three";
        for offset in 0..src.len() {
            let (line, col) = byte_offset_to_line_col(src, offset);
            let back = line_col_to_byte_offset(src, line, col);
            assert_eq!(back, offset, "roundtrip failed at offset {offset}");
        }
    }
}
