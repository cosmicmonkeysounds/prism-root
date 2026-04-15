//! `TextEmitter` — trait for serializing `RootNode` ASTs back to source text.
//!
//! Port of `language/codegen/text-emitter.ts`. Lighter than the symbol
//! emitter family — the symbol emitters produce codegen output from a
//! declarative DSL, while `TextEmitter` is for round-tripping: parse
//! source → AST → serialize back to source text.
//!
//! Each format subclasses this trait by implementing [`emit_node`]
//! and (optionally) overriding [`emit_root`], [`post_process`],
//! [`indent`], or [`inline_node`]. Default implementations handle
//! the common traversal + string extraction patterns.

use super::source_builder::SourceBuilder;
use crate::language::syntax::ast_types::{RootNode, SyntaxNode};

/// Base behaviour for AST → source-text emitters.
///
/// Minimal contract: implementors provide [`emit_node`]. Everything
/// else — root traversal, child walking, inline-text flattening,
/// trailing-whitespace post-processing — has a default.
pub trait TextEmitter {
    /// Indent string used by the internal [`SourceBuilder`]. Default
    /// is two spaces. Override by returning `"\t"` etc.
    fn indent(&self) -> &str {
        "  "
    }

    /// Emit a single node into `b`. The only method implementors
    /// *must* override.
    fn emit_node(&self, node: &SyntaxNode, b: &mut SourceBuilder);

    /// Emit the root node. Default walks `tree.children` through
    /// [`emit_node`]. Override for formats that need a prelude or
    /// postamble at the document level.
    fn emit_root(&self, tree: &RootNode, b: &mut SourceBuilder) {
        for child in &tree.children {
            self.emit_node(child, b);
        }
    }

    /// Post-process the final output string. Default trims trailing
    /// whitespace and ensures exactly one trailing newline — matches
    /// the TS base-class behaviour.
    fn post_process(&self, output: String) -> String {
        let trimmed = output.trim_end_matches(|c: char| c.is_whitespace());
        let mut out = String::with_capacity(trimmed.len() + 1);
        out.push_str(trimmed);
        out.push('\n');
        out
    }

    /// Serialize an AST back to source text. Uses the configured
    /// indent string, walks the root, post-processes the result.
    fn serialize(&self, tree: &RootNode) -> String {
        let mut b = SourceBuilder::new(self.indent());
        self.emit_root(tree, &mut b);
        self.post_process(b.build())
    }

    /// Emit every child of `node` through [`emit_node`].
    fn emit_children(&self, node: &SyntaxNode, b: &mut SourceBuilder) {
        for child in &node.children {
            self.emit_node(child, b);
        }
    }

    /// Concatenate the `value` payloads of every leaf descendant.
    /// Works for both branch and leaf nodes: leaves return their
    /// own value; branches recurse into their children.
    fn text(&self, node: &SyntaxNode) -> String {
        if let Some(v) = node.value.as_ref() {
            return v.clone();
        }
        let mut out = String::new();
        for child in &node.children {
            out.push_str(&self.text(child));
        }
        out
    }

    /// Render every child as inline text, joined without separators.
    fn inline_children(&self, node: &SyntaxNode) -> String {
        let mut out = String::new();
        for child in &node.children {
            out.push_str(&self.inline_node(child));
        }
        out
    }

    /// Render one node as inline text. Default: return `value` or
    /// recurse through children. Override for nodes that need
    /// format-specific wrapping (e.g. `bold` → `**text**`).
    fn inline_node(&self, node: &SyntaxNode) -> String {
        if let Some(v) = node.value.as_ref() {
            return v.clone();
        }
        self.inline_children(node)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::language::syntax::ast_types::RootNode;

    fn branch(kind: &str, children: Vec<SyntaxNode>) -> SyntaxNode {
        SyntaxNode {
            kind: kind.into(),
            children,
            ..SyntaxNode::default()
        }
    }

    fn leaf(kind: &str, value: &str) -> SyntaxNode {
        SyntaxNode {
            kind: kind.into(),
            value: Some(value.into()),
            ..SyntaxNode::default()
        }
    }

    struct MarkdownEmitter;

    impl TextEmitter for MarkdownEmitter {
        fn emit_node(&self, node: &SyntaxNode, b: &mut SourceBuilder) {
            match node.kind.as_str() {
                "heading" => {
                    let level =
                        node.data.get("level").and_then(|v| v.as_u64()).unwrap_or(1) as usize;
                    b.line(format!("{} {}", "#".repeat(level), self.text(node)));
                    b.blank();
                }
                "paragraph" => {
                    b.line(self.inline_children(node));
                    b.blank();
                }
                "bold" => {
                    b.line(format!("**{}**", self.inline_children(node)));
                }
                _ => self.emit_children(node, b),
            }
        }

        fn inline_node(&self, node: &SyntaxNode) -> String {
            match node.kind.as_str() {
                "bold" => format!("**{}**", self.inline_children(node)),
                _ => {
                    if let Some(v) = &node.value {
                        v.clone()
                    } else {
                        self.inline_children(node)
                    }
                }
            }
        }
    }

    #[test]
    fn serialize_walks_children_and_post_processes() {
        let mut heading = branch("heading", vec![leaf("text", "Title")]);
        heading.data.insert("level".into(), serde_json::json!(2));
        let tree = RootNode {
            children: vec![
                heading,
                branch(
                    "paragraph",
                    vec![
                        leaf("text", "hello "),
                        branch("bold", vec![leaf("text", "world")]),
                    ],
                ),
            ],
            ..RootNode::default()
        };
        let emitter = MarkdownEmitter;
        let out = emitter.serialize(&tree);
        assert_eq!(out, "## Title\n\nhello **world**\n");
    }

    #[test]
    fn text_helper_concatenates_leaf_values() {
        let emitter = MarkdownEmitter;
        let node = branch(
            "paragraph",
            vec![leaf("text", "a"), leaf("text", "b"), leaf("text", "c")],
        );
        assert_eq!(emitter.text(&node), "abc");
    }

    #[test]
    fn post_process_adds_single_trailing_newline() {
        struct E;
        impl TextEmitter for E {
            fn emit_node(&self, _: &SyntaxNode, _: &mut SourceBuilder) {}
        }
        let out = E.post_process("hello   \n\n\n".into());
        assert_eq!(out, "hello\n");
    }

    #[test]
    fn default_indent_is_two_spaces() {
        struct E;
        impl TextEmitter for E {
            fn emit_node(&self, _: &SyntaxNode, _: &mut SourceBuilder) {}
        }
        assert_eq!(E.indent(), "  ");
    }
}
