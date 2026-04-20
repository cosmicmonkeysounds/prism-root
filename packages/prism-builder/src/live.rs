//! Source-first live document (ADR-006).
//!
//! [`LiveDocument`] makes `.slint` source the canonical format:
//! - Source text lives in the [`editor`] buffer (ropey-backed)
//! - A [`SourceMap`] provides bidirectional node ID ↔ byte range mapping
//! - [`BuilderDocument`] is derived on demand from marker comments
//! - The `slint-interpreter` compiler produces a live preview
//!
//! Mutation flows:
//! - **GUI→Source**: call source-editing methods ([`edit_prop_in_source`],
//!   [`insert_node_in_source`], etc.) which surgically edit the source
//!   text, then recompile and invalidate the derived document.
//! - **Code editor→Preview**: edit via the [`editor`] field, call
//!   [`apply_editor_changes`] to sync source, recompile, and invalidate.
//! - **Import**: [`from_document`] generates marked source from a
//!   [`BuilderDocument`], then proceeds as source-first.

use std::sync::Arc;

use prism_core::design_tokens::DesignTokens;
use prism_core::editor::EditorState;

use crate::document::{BuilderDocument, Node, NodeId};
use crate::registry::ComponentRegistry;
use crate::render::{compile_slint_source, InstantiateError};
use crate::source_map::SourceMap;
use crate::source_parse::{derive_document_from_source, format_slint_value};

#[derive(Debug, Clone)]
pub struct LiveDiagnostic {
    pub message: String,
    pub line: Option<usize>,
    pub column: Option<usize>,
}

/// Line/column range in the source for editor selection highlighting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceSelection {
    pub start_line: usize,
    pub start_col: usize,
    pub end_line: usize,
    pub end_col: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum SourceEditError {
    #[error("node not found in source map: {0}")]
    NodeNotFound(String),
    #[error("property '{key}' not found for node '{node_id}'")]
    PropNotFound { node_id: String, key: String },
    #[error("compile error after edit: {0}")]
    CompileError(String),
}

pub struct LiveDocument {
    // Canonical
    pub source: String,
    pub source_map: SourceMap,
    pub editor: EditorState,
    pub diagnostics: Vec<LiveDiagnostic>,
    compiled: Option<slint_interpreter::ComponentDefinition>,
    // Owned context
    registry: Arc<ComponentRegistry>,
    tokens: DesignTokens,
    // Derived cache
    derived_document: Option<BuilderDocument>,
}

impl LiveDocument {
    /// Create from raw `.slint` source (the primary constructor).
    pub fn from_source(
        source: String,
        registry: Arc<ComponentRegistry>,
        tokens: DesignTokens,
    ) -> Self {
        let source_map = crate::render::build_source_map_from_markers(&source);
        let mut editor = EditorState::new();
        editor.language = "slint".to_string();
        editor.set_text(&source);
        let mut live = Self {
            source,
            source_map,
            editor,
            diagnostics: Vec::new(),
            compiled: None,
            registry,
            tokens,
            derived_document: None,
        };
        let _ = live.recompile();
        live
    }

    /// Create from a [`BuilderDocument`] (import/migration path).
    /// Generates marked source, then proceeds as source-first.
    pub fn from_document(
        document: BuilderDocument,
        registry: Arc<ComponentRegistry>,
        tokens: DesignTokens,
    ) -> Self {
        let (source, source_map) =
            match crate::render::render_document_slint_source_mapped(&document, &registry, &tokens)
            {
                Ok(pair) => pair,
                Err(_) => (String::new(), SourceMap::new()),
            };
        let mut editor = EditorState::new();
        editor.language = "slint".to_string();
        editor.set_text(&source);
        let mut live = Self {
            source,
            source_map,
            editor,
            diagnostics: Vec::new(),
            compiled: None,
            registry,
            tokens,
            derived_document: Some(document),
        };
        let _ = live.recompile();
        live
    }

    /// Backward-compatible constructor (delegates to `from_document`).
    pub fn new(
        document: BuilderDocument,
        registry: Arc<ComponentRegistry>,
        tokens: &DesignTokens,
    ) -> Self {
        Self::from_document(document, registry, *tokens)
    }

    // ── Derived document ───────────────────────────────────────────

    /// Get a [`BuilderDocument`] derived from the current source.
    /// Cached; only rebuilds when source has changed since last call.
    pub fn document(&mut self) -> &BuilderDocument {
        if self.derived_document.is_none() {
            self.derived_document =
                Some(derive_document_from_source(&self.source, &self.source_map));
        }
        self.derived_document.as_ref().unwrap()
    }

    /// Clone the current derived document (avoids borrow issues).
    pub fn document_cloned(&mut self) -> BuilderDocument {
        self.document().clone()
    }

    // ── Source-based mutations ──────────────────────────────────────

    /// Replace a property value in source using the source map.
    pub fn edit_prop_in_source(
        &mut self,
        node_id: &str,
        key: &str,
        value_text: &str,
    ) -> Result<(), SourceEditError> {
        let span = self
            .source_map
            .span_for_node(node_id)
            .ok_or_else(|| SourceEditError::NodeNotFound(node_id.to_string()))?;

        if let Some(prop) = span.props.iter().find(|p| p.key == key) {
            // Replace existing value in-place.
            let mut new_source = String::with_capacity(self.source.len());
            new_source.push_str(&self.source[..prop.value_start]);
            new_source.push_str(value_text);
            new_source.push_str(&self.source[prop.value_end..]);
            self.source = new_source;
        } else {
            // Property doesn't exist — insert before the node's closing marker.
            let end_marker = format!("// @node-end:{node_id}");
            if let Some(marker_pos) = self.source.find(&end_marker) {
                let indent = detect_indent(&self.source, span.start);
                let new_line = format!("{indent}    {key}: {value_text};\n");
                self.source.insert_str(marker_pos, &new_line);
            } else {
                return Err(SourceEditError::NodeNotFound(node_id.to_string()));
            }
        }

        self.after_source_change()
    }

    /// Insert a new node as `.slint` source with markers.
    pub fn insert_node_in_source(
        &mut self,
        parent_id: Option<&str>,
        component: &str,
        node_id: &str,
        props: &serde_json::Value,
    ) -> Result<(), SourceEditError> {
        let snippet = generate_node_snippet(node_id, component, props, &self.registry);

        match parent_id {
            Some(pid) => {
                let end_marker = format!("// @node-end:{pid}");
                if let Some(pos) = self.source.find(&end_marker) {
                    let span = self
                        .source_map
                        .span_for_node(pid)
                        .ok_or_else(|| SourceEditError::NodeNotFound(pid.to_string()))?;
                    let indent = detect_indent(&self.source, span.start);
                    let indented = indent_snippet(&snippet, &format!("{indent}    "));
                    self.source.insert_str(pos, &indented);
                } else {
                    return Err(SourceEditError::NodeNotFound(pid.to_string()));
                }
            }
            None => {
                // Insert before the last `}` in the source (the BuilderRoot closing brace).
                if let Some(insert_pos) = find_root_insert_point(&self.source) {
                    let indented = indent_snippet(&snippet, "        ");
                    self.source.insert_str(insert_pos, &indented);
                }
            }
        }

        self.after_source_change()
    }

    /// Remove a node by excising its marker span.
    pub fn remove_node_from_source(&mut self, node_id: &str) -> Result<(), SourceEditError> {
        let span = self
            .source_map
            .span_for_node(node_id)
            .ok_or_else(|| SourceEditError::NodeNotFound(node_id.to_string()))?;

        // Extend to include the trailing newline if present.
        let end = if self.source.as_bytes().get(span.end) == Some(&b'\n') {
            span.end + 1
        } else {
            span.end
        };
        let start = span.start;

        self.source = format!("{}{}", &self.source[..start], &self.source[end..]);
        self.after_source_change()
    }

    /// Move a node within its siblings by cutting and reinserting its source.
    pub fn move_node_in_source(
        &mut self,
        node_id: &str,
        direction: i32,
    ) -> Result<(), SourceEditError> {
        let doc = derive_document_from_source(&self.source, &self.source_map);
        let sibling_id = find_sibling_id(&doc, node_id, direction);
        let sibling_id = match sibling_id {
            Some(id) => id,
            None => return Ok(()),
        };

        let node_span = self
            .source_map
            .span_for_node(node_id)
            .ok_or_else(|| SourceEditError::NodeNotFound(node_id.to_string()))?;
        let sib_span = self
            .source_map
            .span_for_node(&sibling_id)
            .ok_or_else(|| SourceEditError::NodeNotFound(sibling_id.clone()))?;

        let node_end = if self.source.as_bytes().get(node_span.end) == Some(&b'\n') {
            node_span.end + 1
        } else {
            node_span.end
        };
        let node_text = self.source[node_span.start..node_end].to_string();

        // Remove the node first, then insert at the sibling's position.
        self.source = format!(
            "{}{}",
            &self.source[..node_span.start],
            &self.source[node_end..]
        );

        // Recalculate sibling position after removal.
        let offset_adjust = node_end - node_span.start;
        let insert_pos = if direction < 0 {
            if sib_span.start < node_span.start {
                sib_span.start
            } else {
                sib_span.start - offset_adjust
            }
        } else if sib_span.start > node_span.start {
            let sib_end =
                if self.source.as_bytes().get(sib_span.end - offset_adjust) == Some(&b'\n') {
                    sib_span.end - offset_adjust + 1
                } else {
                    sib_span.end - offset_adjust
                };
            sib_end.min(self.source.len())
        } else {
            sib_span.end
        };

        self.source
            .insert_str(insert_pos.min(self.source.len()), &node_text);
        self.after_source_change()
    }

    // ── Editor integration ─────────────────────────────────────────

    /// Apply the editor buffer as the new canonical source.
    pub fn apply_editor_changes(&mut self) -> Result<(), InstantiateError> {
        self.source = self.editor.text();
        self.source_map = crate::render::build_source_map_from_markers(&self.source);
        self.derived_document = None;
        self.recompile()
    }

    /// Set source directly (e.g., from file load or undo restore).
    pub fn set_source(&mut self, source: String) -> Result<(), InstantiateError> {
        self.source = source;
        self.source_map = crate::render::build_source_map_from_markers(&self.source);
        self.derived_document = None;
        self.sync_editor();
        self.recompile()
    }

    // ── Backward-compatible mutation methods ────────────────────────
    // These delegate to the old document-mutate-then-rebuild pattern
    // for callers that haven't migrated yet.

    /// Regenerate source from a document (import/rebuild path).
    pub fn rebuild_with(
        &mut self,
        registry: &ComponentRegistry,
        tokens: &DesignTokens,
    ) -> Result<(), InstantiateError> {
        let doc = self.derived_document.take().unwrap_or_default();
        match crate::render::render_document_slint_source_mapped(&doc, registry, tokens) {
            Ok((source, map)) => {
                self.source = source;
                self.source_map = map;
                self.derived_document = Some(doc);
                self.sync_editor();
                self.recompile()
            }
            Err(e) => {
                self.derived_document = Some(doc);
                self.diagnostics = vec![LiveDiagnostic {
                    message: e.to_string(),
                    line: None,
                    column: None,
                }];
                Err(InstantiateError::Render(e))
            }
        }
    }

    pub fn set_prop(&mut self, node_id: &str, key: &str, value: serde_json::Value) -> bool {
        let doc = self
            .derived_document
            .get_or_insert_with(|| derive_document_from_source(&self.source, &self.source_map));
        if let Some(root) = &mut doc.root {
            if let Some(node) = root.find_mut(node_id) {
                if let Some(obj) = node.props.as_object_mut() {
                    obj.insert(key.to_string(), value);
                    return true;
                }
            }
        }
        false
    }

    pub fn rebuild(&mut self) -> Result<(), InstantiateError> {
        let registry = Arc::clone(&self.registry);
        let tokens = self.tokens;
        self.rebuild_with(&registry, &tokens)
    }

    pub fn edit_prop(
        &mut self,
        node_id: &str,
        key: &str,
        value: serde_json::Value,
    ) -> Result<bool, InstantiateError> {
        if !self.set_prop(node_id, key, value) {
            return Ok(false);
        }
        self.rebuild()?;
        Ok(true)
    }

    pub fn add_node(&mut self, parent_id: &str, node: Node) -> bool {
        let doc = self
            .derived_document
            .get_or_insert_with(|| derive_document_from_source(&self.source, &self.source_map));
        if let Some(root) = &mut doc.root {
            if let Some(parent) = root.find_mut(parent_id) {
                parent.children.push(node);
                return true;
            }
        }
        false
    }

    pub fn remove_node(&mut self, node_id: &NodeId) -> bool {
        let doc = self
            .derived_document
            .get_or_insert_with(|| derive_document_from_source(&self.source, &self.source_map));
        if let Some(root) = &mut doc.root {
            return remove_node_recursive(root, node_id);
        }
        false
    }

    // ── Queries ────────────────────────────────────────────────────

    pub fn compiled(&self) -> Option<&slint_interpreter::ComponentDefinition> {
        self.compiled.as_ref()
    }

    pub fn has_errors(&self) -> bool {
        !self.diagnostics.is_empty()
    }

    pub fn source_for_node(&self, node_id: &str) -> Option<&str> {
        let span = self.source_map.span_for_node(node_id)?;
        self.source.get(span.start..span.end)
    }

    pub fn node_at_offset(&self, offset: usize) -> Option<&str> {
        self.source_map.node_at_offset(offset)
    }

    pub fn node_at_cursor(&self) -> Option<&str> {
        let offset = line_col_to_byte_offset(
            &self.source,
            self.editor.cursor.position.line,
            self.editor.cursor.position.col,
        );
        self.source_map.node_at_offset(offset)
    }

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

    pub fn registry(&self) -> &ComponentRegistry {
        &self.registry
    }

    pub fn tokens(&self) -> &DesignTokens {
        &self.tokens
    }

    // ── Internal ───────────────────────────────────────────────────

    fn recompile(&mut self) -> Result<(), InstantiateError> {
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

    fn sync_editor(&mut self) {
        self.editor.set_text(&self.source);
        self.editor.language = "slint".to_string();
    }

    fn after_source_change(&mut self) -> Result<(), SourceEditError> {
        self.source_map = crate::render::build_source_map_from_markers(&self.source);
        self.derived_document = None;
        self.sync_editor();
        self.recompile()
            .map_err(|e| SourceEditError::CompileError(e.to_string()))
    }
}

fn remove_node_recursive(parent: &mut Node, target: &str) -> bool {
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

fn detect_indent(source: &str, offset: usize) -> String {
    let before = &source[..offset];
    if let Some(nl) = before.rfind('\n') {
        let line_start = nl + 1;
        let line = &source[line_start..offset];
        let ws: String = line.chars().take_while(|c| c.is_whitespace()).collect();
        ws
    } else {
        String::new()
    }
}

fn indent_snippet(snippet: &str, indent: &str) -> String {
    let mut out = String::new();
    for line in snippet.lines() {
        out.push_str(indent);
        out.push_str(line);
        out.push('\n');
    }
    out
}

fn find_root_insert_point(source: &str) -> Option<usize> {
    // Find the VerticalLayout's closing brace area or the Window's.
    // Insert before the last `}` pair.
    let bytes = source.as_bytes();
    let mut depth = 0i32;
    let mut last_close = None;
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 1 {
                    last_close = Some(i);
                }
            }
            _ => {}
        }
    }
    last_close
}

fn find_sibling_id(doc: &BuilderDocument, node_id: &str, direction: i32) -> Option<String> {
    fn find_in(children: &[Node], target: &str, direction: i32) -> Option<String> {
        if let Some(pos) = children.iter().position(|n| n.id == target) {
            let new_pos = pos as i32 + direction;
            if new_pos >= 0 && (new_pos as usize) < children.len() {
                return Some(children[new_pos as usize].id.clone());
            }
        }
        for child in children {
            if let Some(id) = find_in(&child.children, target, direction) {
                return Some(id);
            }
        }
        None
    }
    if let Some(root) = &doc.root {
        find_in(&root.children, node_id, direction)
    } else {
        None
    }
}

fn generate_node_snippet(
    node_id: &str,
    component: &str,
    props: &serde_json::Value,
    registry: &ComponentRegistry,
) -> String {
    let mut out = String::new();
    out.push_str(&format!("// @node-start:{node_id}:{component}\n"));

    let element_name = match component {
        "heading" | "text" | "code" => "Text",
        "link" => "Text",
        "image" => "Image",
        "button" => "Rectangle",
        "input" => "Rectangle",
        "container" | "section" | "columns" | "form" | "list" | "card" | "tabs" | "accordion" => {
            "VerticalLayout"
        }
        "divider" => "Rectangle",
        "spacer" => "Rectangle",
        "table" => "VerticalLayout",
        _ => "Rectangle",
    };

    out.push_str(&format!("{element_name} {{\n"));

    if let Some(obj) = props.as_object() {
        let schema = registry
            .get(component)
            .map(|c| c.schema())
            .unwrap_or_default();
        for (key, value) in obj {
            let is_px = schema
                .iter()
                .any(|f| f.key == *key && matches!(f.kind, crate::FieldKind::Number(..)));
            let formatted = format_slint_value(value, is_px);
            out.push_str(&format!("    {key}: {formatted};\n"));
        }
    }

    out.push_str("}\n");
    out.push_str(&format!("// @node-end:{node_id}\n"));
    out
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
        let live = LiveDocument::new(doc, Arc::new(reg), &tokens);

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
        let live = LiveDocument::new(doc, Arc::new(reg), &tokens);

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
        let live = LiveDocument::new(doc, Arc::new(reg), &tokens);

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
        let mut live = LiveDocument::new(doc, Arc::new(reg), &tokens);

        assert!(live.source.contains("Before"));
        live.edit_prop("n1", "text", json!("After")).unwrap();
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
        let live = LiveDocument::new(doc, Arc::new(reg), &tokens);

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
        let live = LiveDocument::new(doc, Arc::new(reg), &tokens);

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
        let live = LiveDocument::new(doc, Arc::new(reg), &tokens);
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
        let mut live = LiveDocument::new(doc, Arc::new(reg), &tokens);

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
        let mut live = LiveDocument::new(doc, Arc::new(reg), &tokens);
        assert!(!live.has_errors());

        let new_source = live.source.replace("Original", "Edited");
        live.editor.set_text(&new_source);
        live.apply_editor_changes().unwrap();
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
        let mut live = LiveDocument::new(doc, Arc::new(reg), &tokens);

        live.editor.set_text("this is not valid slint {{{");
        let result = live.apply_editor_changes();
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

    // ── Source-first mutation tests ────────────────────────────────

    #[test]
    fn edit_prop_in_source_replaces_value() {
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
        let mut live = LiveDocument::new(doc, Arc::new(reg), &tokens);

        live.edit_prop_in_source("n1", "text", r#""After""#)
            .unwrap();
        assert!(live.source.contains(r#"text: "After";"#));
        assert!(!live.source.contains("Before"));
        assert!(!live.has_errors());
    }

    #[test]
    fn edit_prop_in_source_node_not_found() {
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
        let mut live = LiveDocument::new(doc, Arc::new(reg), &tokens);

        let result = live.edit_prop_in_source("missing", "text", r#""Y""#);
        assert!(result.is_err());
    }

    #[test]
    fn derived_document_reflects_source() {
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
        let mut live = LiveDocument::new(doc, Arc::new(reg), &tokens);

        live.edit_prop_in_source("n1", "text", r#""Changed""#)
            .unwrap();
        let derived = live.document();
        let root = derived.root.as_ref().unwrap();
        assert_eq!(
            root.props.get("text").and_then(|v| v.as_str()),
            Some("Changed")
        );
    }

    #[test]
    fn remove_node_from_source_works() {
        let doc = BuilderDocument {
            root: Some(Node {
                id: "n1".into(),
                component: "heading".into(),
                props: json!({ "text": "Gone" }),
                children: vec![],
                ..Default::default()
            }),
            ..Default::default()
        };
        let reg = test_registry();
        let tokens = DesignTokens::default();
        let mut live = LiveDocument::new(doc, Arc::new(reg), &tokens);
        assert!(live.source.contains("@node-start:n1"));

        live.remove_node_from_source("n1").unwrap();
        assert!(!live.source.contains("@node-start:n1"));
        assert!(!live.source.contains("Gone"));
    }

    #[test]
    fn from_source_constructor() {
        let source = r#"export component BuilderRoot inherits Window {
    preferred-width: 1280px;
    preferred-height: 800px;
    VerticalLayout {
        // @node-start:n1:heading
        Text {
            text: "From Source";
            font-size: 24px;
        }
        // @node-end:n1
    }
}"#;
        let reg = Arc::new(test_registry());
        let tokens = DesignTokens::default();
        let mut live = LiveDocument::from_source(source.to_string(), reg, tokens);

        assert!(!live.has_errors());
        assert!(live.source_map.span_for_node("n1").is_some());
        let doc = live.document();
        let root = doc.root.as_ref().unwrap();
        assert_eq!(root.id, "n1");
        assert_eq!(
            root.props.get("text").and_then(|v| v.as_str()),
            Some("From Source")
        );
    }
}
