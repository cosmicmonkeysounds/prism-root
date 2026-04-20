//! Source map for bidirectional Slint editing (ADR-006).
//!
//! Tracks the byte ranges in generated `.slint` source that correspond
//! to each [`BuilderDocument`] node. Enables:
//!
//! * **Forward mapping**: `span_for_node(node_id)` → source byte range.
//! * **Reverse mapping**: `node_at_offset(byte_offset)` → node ID.
//! * **Property mapping**: per-property value spans for surgical edits.

use indexmap::IndexMap;

use crate::component::ComponentId;
use crate::document::NodeId;

/// Byte range of a single property's value within the source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropSpan {
    pub key: String,
    pub value_start: usize,
    pub value_end: usize,
}

/// Byte range of an entire node's block in the source, including
/// its children and any per-property spans.
#[derive(Debug, Clone)]
pub struct SourceSpan {
    pub node_id: NodeId,
    pub component: ComponentId,
    pub start: usize,
    pub end: usize,
    pub props: Vec<PropSpan>,
}

impl SourceSpan {
    pub fn contains_offset(&self, offset: usize) -> bool {
        offset >= self.start && offset < self.end
    }

    pub fn prop_at_offset(&self, offset: usize) -> Option<&PropSpan> {
        self.props
            .iter()
            .find(|p| offset >= p.value_start && offset < p.value_end)
    }
}

/// Maps node IDs to their source byte ranges.
#[derive(Debug, Clone, Default)]
pub struct SourceMap {
    spans: IndexMap<NodeId, SourceSpan>,
}

impl SourceMap {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, span: SourceSpan) {
        self.spans.insert(span.node_id.clone(), span);
    }

    pub fn span_for_node(&self, node_id: &str) -> Option<&SourceSpan> {
        self.spans.get(node_id)
    }

    pub fn node_at_offset(&self, offset: usize) -> Option<&str> {
        let mut best: Option<&SourceSpan> = None;
        for span in self.spans.values() {
            if span.contains_offset(offset) {
                match best {
                    None => best = Some(span),
                    Some(prev) => {
                        let prev_size = prev.end - prev.start;
                        let cur_size = span.end - span.start;
                        if cur_size < prev_size {
                            best = Some(span);
                        }
                    }
                }
            }
        }
        best.map(|s| s.node_id.as_str())
    }

    pub fn prop_at_offset(&self, offset: usize) -> Option<(&str, &str)> {
        let node_id = self.node_at_offset(offset)?;
        let span = self.spans.get(node_id)?;
        let prop = span.prop_at_offset(offset)?;
        Some((node_id, prop.key.as_str()))
    }

    pub fn spans(&self) -> impl Iterator<Item = &SourceSpan> {
        self.spans.values()
    }

    pub fn len(&self) -> usize {
        self.spans.len()
    }

    pub fn is_empty(&self) -> bool {
        self.spans.is_empty()
    }
}

/// Extends [`SlintEmitter`](crate::slint_source::SlintEmitter) with
/// byte-offset tracking for building a [`SourceMap`].
///
/// Rather than modifying `SlintEmitter` itself (which would complicate
/// the non-mapped path), this wrapper accumulates a flat `String` and
/// records spans as nodes are emitted.
pub struct MappedEmitter {
    buf: String,
    depth: usize,
    indent: &'static str,
    source_map: SourceMap,
    node_stack: Vec<NodeFrame>,
}

struct NodeFrame {
    node_id: NodeId,
    component: ComponentId,
    start: usize,
    props: Vec<PropSpan>,
}

impl MappedEmitter {
    pub fn new() -> Self {
        Self {
            buf: String::with_capacity(4096),
            depth: 0,
            indent: "    ",
            source_map: SourceMap::new(),
            node_stack: Vec::new(),
        }
    }

    fn push_indent(&mut self) {
        for _ in 0..self.depth {
            self.buf.push_str(self.indent);
        }
    }

    pub fn line(&mut self, text: &str) -> &mut Self {
        if text.is_empty() {
            self.buf.push('\n');
        } else {
            self.push_indent();
            self.buf.push_str(text);
            self.buf.push('\n');
        }
        self
    }

    pub fn blank(&mut self) -> &mut Self {
        self.line("")
    }

    pub fn block<F>(&mut self, header: &str, body: F) -> Result<(), crate::component::RenderError>
    where
        F: FnOnce(&mut Self) -> Result<(), crate::component::RenderError>,
    {
        self.push_indent();
        self.buf.push_str(header);
        self.buf.push_str(" {\n");
        self.depth += 1;
        let result = body(self);
        self.depth -= 1;
        self.push_indent();
        self.buf.push_str("}\n");
        result
    }

    /// Start tracking a node's span. Call before the node's block.
    pub fn begin_node(&mut self, node_id: &str, component: &str) {
        self.node_stack.push(NodeFrame {
            node_id: node_id.to_string(),
            component: component.to_string(),
            start: self.buf.len(),
            props: Vec::new(),
        });
    }

    /// Finish tracking a node's span. Call after the node's block closes.
    pub fn end_node(&mut self) {
        if let Some(frame) = self.node_stack.pop() {
            self.source_map.insert(SourceSpan {
                node_id: frame.node_id,
                component: frame.component,
                start: frame.start,
                end: self.buf.len(),
                props: frame.props,
            });
        }
    }

    pub fn property(&mut self, key: &str, value: &str) -> &mut Self {
        self.push_indent();
        self.buf.push_str(key);
        self.buf.push_str(": ");
        let value_start = self.buf.len();
        self.buf.push_str(value);
        let value_end = self.buf.len();
        self.buf.push_str(";\n");

        if let Some(frame) = self.node_stack.last_mut() {
            frame.props.push(PropSpan {
                key: key.to_string(),
                value_start,
                value_end,
            });
        }
        self
    }

    pub fn prop_string(&mut self, key: &str, value: &str) -> &mut Self {
        let escaped = crate::slint_source::escape_slint_string(value);
        let val = format!("\"{}\"", escaped);
        self.property(key, &val)
    }

    pub fn prop_int(&mut self, key: &str, value: i64) -> &mut Self {
        self.property(key, &value.to_string())
    }

    pub fn prop_float(&mut self, key: &str, value: f64) -> &mut Self {
        self.property(key, &value.to_string())
    }

    pub fn prop_px(&mut self, key: &str, value: f64) -> &mut Self {
        self.property(key, &format!("{value}px"))
    }

    pub fn prop_bool(&mut self, key: &str, value: bool) -> &mut Self {
        self.property(key, if value { "true" } else { "false" })
    }

    pub fn prop_color(&mut self, key: &str, value: &str) -> &mut Self {
        self.property(key, value)
    }

    pub fn into_parts(self) -> (String, SourceMap) {
        (self.buf, self.source_map)
    }

    pub fn source(&self) -> &str {
        &self.buf
    }

    pub fn source_map(&self) -> &SourceMap {
        &self.source_map
    }
}

impl Default for MappedEmitter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mapped_emitter_tracks_node_spans() {
        let mut e = MappedEmitter::new();
        e.line("// header");
        e.begin_node("n1", "heading");
        e.block("Text", |e| {
            e.prop_string("text", "Hello");
            e.prop_px("font-size", 24.0);
            Ok(())
        })
        .unwrap();
        e.end_node();

        let (source, map) = e.into_parts();
        assert!(source.contains("Text {"));
        assert!(source.contains(r#"text: "Hello";"#));

        let span = map.span_for_node("n1").unwrap();
        assert_eq!(span.component, "heading");
        assert!(span.start < span.end);
        assert_eq!(span.props.len(), 2);
        assert_eq!(span.props[0].key, "text");
        assert_eq!(span.props[1].key, "font-size");

        let text_prop = &span.props[0];
        let value_slice = &source[text_prop.value_start..text_prop.value_end];
        assert_eq!(value_slice, "\"Hello\"");
    }

    #[test]
    fn node_at_offset_finds_innermost() {
        let mut e = MappedEmitter::new();
        e.begin_node("outer", "section");
        e.block("VerticalLayout", |e| {
            e.begin_node("inner", "heading");
            e.block("Text", |e| {
                e.prop_string("text", "hi");
                Ok(())
            })?;
            e.end_node();
            Ok(())
        })
        .unwrap();
        e.end_node();

        let (_, map) = e.into_parts();

        let inner_span = map.span_for_node("inner").unwrap();
        let mid = (inner_span.start + inner_span.end) / 2;
        assert_eq!(map.node_at_offset(mid), Some("inner"));

        let outer_span = map.span_for_node("outer").unwrap();
        assert_eq!(map.node_at_offset(outer_span.start), Some("outer"));
    }

    #[test]
    fn prop_at_offset_returns_key() {
        let mut e = MappedEmitter::new();
        e.begin_node("n1", "button");
        e.block("Button", |e| {
            e.prop_string("text", "Click");
            e.prop_bool("enabled", true);
            Ok(())
        })
        .unwrap();
        e.end_node();

        let (_, map) = e.into_parts();

        let span = map.span_for_node("n1").unwrap();
        let text_prop = &span.props[0];
        let mid = (text_prop.value_start + text_prop.value_end) / 2;
        let (node, key) = map.prop_at_offset(mid).unwrap();
        assert_eq!(node, "n1");
        assert_eq!(key, "text");
    }

    #[test]
    fn offset_outside_all_spans_returns_none() {
        let map = SourceMap::new();
        assert!(map.node_at_offset(42).is_none());
    }

    #[test]
    fn source_map_len() {
        let mut map = SourceMap::new();
        assert!(map.is_empty());
        map.insert(SourceSpan {
            node_id: "a".into(),
            component: "x".into(),
            start: 0,
            end: 10,
            props: vec![],
        });
        assert_eq!(map.len(), 1);
    }

    #[test]
    fn serde_independence() {
        let mut e = MappedEmitter::new();
        e.begin_node("n1", "text");
        e.property("color", "#ff0000");
        e.end_node();
        let (source, map) = e.into_parts();

        let span = map.span_for_node("n1").unwrap();
        let prop = &span.props[0];
        assert_eq!(&source[prop.value_start..prop.value_end], "#ff0000");
    }
}
