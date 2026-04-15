//! Minimal, allocation-light HTML builder.
//!
//! Server-side rendering target for [`crate::component::Component::render_html`].
//! Components stream tags, attributes, and escaped text into an [`Html`]
//! buffer; the document walker in [`crate::render`] hands it back to the
//! caller as a finished `String` that's already safe to serve as
//! `text/html; charset=utf-8`.
//!
//! The API is deliberately a thin wrapper around `String` — no tree, no
//! validation, no element-state machine. We lean on the component layer
//! to produce well-formed markup and keep this file reviewable in a
//! single page. Escaping lives here because it's the one thing every
//! caller gets wrong.

use std::fmt::Write as _;

/// Growing HTML buffer. Cheap to create, cheap to extend, single
/// allocation path.
#[derive(Debug, Default)]
pub struct Html {
    buf: String,
}

impl Html {
    pub fn new() -> Self {
        Self { buf: String::new() }
    }

    pub fn with_capacity(cap: usize) -> Self {
        Self {
            buf: String::with_capacity(cap),
        }
    }

    /// `<tag>` with no attributes.
    pub fn open(&mut self, tag: &str) {
        self.buf.push('<');
        self.buf.push_str(tag);
        self.buf.push('>');
    }

    /// `<tag k1="v1" k2="v2">`. Attribute values are HTML-escaped.
    pub fn open_attrs(&mut self, tag: &str, attrs: &[(&str, &str)]) {
        self.buf.push('<');
        self.buf.push_str(tag);
        for (k, v) in attrs {
            self.buf.push(' ');
            self.buf.push_str(k);
            self.buf.push_str("=\"");
            escape_attr_into(v, &mut self.buf);
            self.buf.push('"');
        }
        self.buf.push('>');
    }

    /// `</tag>`.
    pub fn close(&mut self, tag: &str) {
        self.buf.push_str("</");
        self.buf.push_str(tag);
        self.buf.push('>');
    }

    /// Self-closing void element: `<tag k="v">`. Callers are
    /// responsible for picking a void element (`img`, `br`, `meta`,
    /// …); this method just emits the open tag without the closing
    /// sibling.
    pub fn void(&mut self, tag: &str, attrs: &[(&str, &str)]) {
        self.open_attrs(tag, attrs);
    }

    /// Escaped text node.
    pub fn text(&mut self, s: &str) {
        escape_text_into(s, &mut self.buf);
    }

    /// Already-escaped markup passthrough. Use only for content you
    /// built yourself with another [`Html`] — never for user input.
    pub fn raw(&mut self, s: &str) {
        self.buf.push_str(s);
    }

    /// Doctype prelude. The only verbatim-HTML helper we expose.
    pub fn doctype(&mut self) {
        self.buf.push_str("<!doctype html>");
    }

    /// Convenience: format + escape in one call.
    pub fn fmt_text(&mut self, args: std::fmt::Arguments<'_>) {
        // Format into a scratch String, then escape through `text`.
        // We accept the intermediate allocation for clarity; callers
        // that need peak perf should pre-format.
        let mut scratch = String::new();
        let _ = scratch.write_fmt(args);
        self.text(&scratch);
    }

    pub fn as_str(&self) -> &str {
        &self.buf
    }

    pub fn len(&self) -> usize {
        self.buf.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    pub fn into_string(self) -> String {
        self.buf
    }
}

/// Escape a string for use inside an HTML text node. Replaces the
/// five ASCII characters browsers treat specially (`&`, `<`, `>`,
/// `"`, `'`) with their named/numeric entities.
pub fn escape_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    escape_text_into(s, &mut out);
    out
}

/// Escape a string for use inside a double-quoted attribute value.
/// Currently identical to [`escape_text`] — kept as a separate entry
/// point so we can tighten it (e.g. escape whitespace) without
/// touching every call site.
pub fn escape_attr(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    escape_attr_into(s, &mut out);
    out
}

fn escape_text_into(s: &str, out: &mut String) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            other => out.push(other),
        }
    }
}

fn escape_attr_into(s: &str, out: &mut String) {
    // Attribute values need the same five-char set as text nodes;
    // the split function exists so we can diverge later without a
    // second refactor.
    escape_text_into(s, out);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_builder() {
        let html = Html::new();
        assert!(html.is_empty());
        assert_eq!(html.as_str(), "");
    }

    #[test]
    fn open_close_roundtrip() {
        let mut h = Html::new();
        h.open("h1");
        h.text("Hello");
        h.close("h1");
        assert_eq!(h.into_string(), "<h1>Hello</h1>");
    }

    #[test]
    fn attributes_are_escaped() {
        let mut h = Html::new();
        h.open_attrs("a", &[("href", "/search?q=foo&bar")]);
        h.text("Go");
        h.close("a");
        assert_eq!(h.into_string(), r#"<a href="/search?q=foo&amp;bar">Go</a>"#);
    }

    #[test]
    fn text_escapes_all_five_chars() {
        assert_eq!(
            escape_text(r#"<script>alert("x&y's")</script>"#),
            "&lt;script&gt;alert(&quot;x&amp;y&#39;s&quot;)&lt;/script&gt;"
        );
    }

    #[test]
    fn void_element() {
        let mut h = Html::new();
        h.void("img", &[("src", "/portal.png"), ("alt", "Prism")]);
        assert_eq!(h.into_string(), r#"<img src="/portal.png" alt="Prism">"#);
    }

    #[test]
    fn doctype_prelude() {
        let mut h = Html::new();
        h.doctype();
        h.open("html");
        h.close("html");
        assert_eq!(h.into_string(), "<!doctype html><html></html>");
    }

    #[test]
    fn raw_passthrough() {
        let mut h = Html::new();
        h.raw("<!-- generated -->");
        assert_eq!(h.into_string(), "<!-- generated -->");
    }

    #[test]
    fn fmt_text_escapes_result() {
        let mut h = Html::new();
        h.fmt_text(format_args!("count: {}", "<3"));
        assert_eq!(h.into_string(), "count: &lt;3");
    }
}
