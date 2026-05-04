//! Typed AST for the `prism-ui` DSL.
//!
//! Mirrors the HTMX-flavoured surface from `clay-migration-plan.md` §4:
//! a [`Document`] is a tree of [`Node`]s; an [`Element`] carries
//! tag-name, namespaced [`Attribute`]s, and child nodes. Inline `{...}`
//! interpolations are represented as [`Expression`] bodies — the body
//! string is later handed to the Prism Syntax expression parser.

use serde::{Deserialize, Serialize};

use crate::language::syntax::SourceRange;

/// Top-level parsed `.prism-ui` source file.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Document {
    pub nodes: Vec<Node>,
}

/// A node in the document tree.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Node {
    Element(Element),
    /// Static text run (no `{...}` interpolation).
    Text {
        value: String,
        range: SourceRange,
    },
    /// `{expr}` interpolation that lives directly in element children.
    Interpolation(Expression),
    /// `<!-- ... -->` HTML-style comment. Preserved so the formatter
    /// can round-trip.
    Comment {
        value: String,
        range: SourceRange,
    },
}

/// `<tag attr="value" ns:name="value">...children...</tag>`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Element {
    pub tag: String,
    pub attributes: Vec<Attribute>,
    pub children: Vec<Node>,
    pub self_closing: bool,
    pub range: SourceRange,
    pub tag_range: SourceRange,
}

/// Single attribute on an element. The [`AttributeName`] carries the
/// namespace classification so downstream lowering can dispatch
/// without restringifying.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Attribute {
    pub name: AttributeName,
    pub value: AttributeValue,
    pub range: SourceRange,
}

/// Parsed attribute name. Stores the raw `local` part (without the
/// namespace prefix) plus the classified [`AttributeNamespace`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AttributeName {
    /// Full attribute name as written, e.g. `on:click` or `style:background`.
    pub raw: String,
    /// The local part after the namespace (`click` for `on:click`).
    /// Equals `raw` when the namespace is `Bare`.
    pub local: String,
    pub namespace: AttributeNamespace,
    pub range: SourceRange,
}

/// Attribute namespace — see plan §4.3.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttributeNamespace {
    /// Bare component property (`title="..."`, `level="3"`).
    Bare,
    /// `on:<event>` — signal connection lowered to `Connection`.
    On,
    /// `bind:<prop>` — two-way binding sugar for a signal pair.
    Bind,
    /// Control-flow attribute keyword: `if`, `else-if`, `else`, `for`.
    ControlFlow,
    /// `style:<token>` — token-resolved style attribute.
    Style,
    /// `fct:<name>` — facet binding lowered to `FacetDef`.
    Facet,
    /// `sig:<name>` — declared signal shorthand.
    Signal,
    /// `aria:*` — pass-through to HTML lowering.
    Aria,
    /// `data:*` — pass-through to HTML lowering.
    Data,
    /// `class` / `id` — CSS-style addressing for inspector + HTML.
    Identifier,
}

/// Attribute right-hand side. Either a literal string, a single
/// `{expr}` interpolation, or a mix — held as an ordered template.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AttributeValue {
    /// No `=` after the attribute name (boolean attribute, like `disabled`).
    Empty,
    /// Pure string literal.
    String { value: String, range: SourceRange },
    /// Pure `{...}` interpolation, no surrounding text.
    Expression(Expression),
    /// String + interpolation segments interleaved.
    Template {
        parts: Vec<TemplatePart>,
        range: SourceRange,
    },
}

/// A single segment of a templated attribute value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TemplatePart {
    Literal { value: String, range: SourceRange },
    Expression(Expression),
}

/// `{expr}` body. The body string is parsed by Prism Syntax's
/// expression scanner downstream — at this level we only carry the
/// textual range and contents.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Expression {
    pub body: String,
    pub range: SourceRange,
}

/// Parser error. The `grammar::parse` entry point returns a
/// `Result<Document, Vec<ParseError>>` so editor diagnostics can
/// surface multiple errors at once.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParseError {
    pub message: String,
    pub range: SourceRange,
    pub code: &'static str,
}

impl AttributeNamespace {
    /// Classify a raw attribute name into a namespace + local part.
    /// Returns `(namespace, local_string)` where `local_string` is the
    /// part after the `:` delimiter (or the whole name for bare /
    /// control-flow / identifier attributes).
    pub fn classify(raw: &str) -> (AttributeNamespace, String) {
        if let Some((prefix, rest)) = raw.split_once(':') {
            let ns = match prefix {
                "on" => AttributeNamespace::On,
                "bind" => AttributeNamespace::Bind,
                "style" => AttributeNamespace::Style,
                "fct" => AttributeNamespace::Facet,
                "sig" => AttributeNamespace::Signal,
                "aria" => AttributeNamespace::Aria,
                "data" => AttributeNamespace::Data,
                _ => return (AttributeNamespace::Bare, raw.to_string()),
            };
            return (ns, rest.to_string());
        }
        match raw {
            "if" | "else-if" | "else" | "for" => (AttributeNamespace::ControlFlow, raw.to_string()),
            "class" | "id" => (AttributeNamespace::Identifier, raw.to_string()),
            _ => (AttributeNamespace::Bare, raw.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_bare() {
        let (ns, local) = AttributeNamespace::classify("title");
        assert_eq!(ns, AttributeNamespace::Bare);
        assert_eq!(local, "title");
    }

    #[test]
    fn classify_on_event() {
        let (ns, local) = AttributeNamespace::classify("on:click");
        assert_eq!(ns, AttributeNamespace::On);
        assert_eq!(local, "click");
    }

    #[test]
    fn classify_style_token() {
        let (ns, local) = AttributeNamespace::classify("style:background");
        assert_eq!(ns, AttributeNamespace::Style);
        assert_eq!(local, "background");
    }

    #[test]
    fn classify_control_flow_keyword() {
        let (ns, local) = AttributeNamespace::classify("if");
        assert_eq!(ns, AttributeNamespace::ControlFlow);
        assert_eq!(local, "if");
    }

    #[test]
    fn classify_identifier() {
        let (ns, local) = AttributeNamespace::classify("class");
        assert_eq!(ns, AttributeNamespace::Identifier);
        assert_eq!(local, "class");
    }

    #[test]
    fn classify_unknown_namespace_falls_back_to_bare() {
        let (ns, local) = AttributeNamespace::classify("weird:thing");
        assert_eq!(ns, AttributeNamespace::Bare);
        assert_eq!(local, "weird:thing");
    }
}
