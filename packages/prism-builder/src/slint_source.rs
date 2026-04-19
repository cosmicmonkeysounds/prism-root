//! Slint DSL emitter used by the document walker.
//!
//! Mirrors [`crate::html::Html`] for the Slint target. Components stream
//! tags (`Text { … }`, `Rectangle { … }`, `VerticalLayout { … }`) and
//! typed property values into a [`SlintEmitter`] buffer; the document
//! walker in [`crate::render::render_document_slint_source`] hands it
//! back as a finished `.slint` source string the Studio shell feeds
//! into [`slint_interpreter::Compiler`].
//!
//! Internally the emitter reuses
//! [`prism_core::language::codegen::SourceBuilder`] so we share indent
//! / block semantics with every other Prism codegen emitter
//! (TypeScript / C# / EmmyDoc / GDScript).
//!
//! The API is deliberately narrow. It escapes string literals so
//! user-supplied text can't break out of a Slint property binding,
//! but it does *not* try to validate the DSL — the compiler is the
//! safety net.

use prism_core::language::codegen::SourceBuilder;

use crate::component::RenderError;

/// Growing Slint DSL buffer.
pub struct SlintEmitter {
    inner: SourceBuilder,
}

impl Default for SlintEmitter {
    fn default() -> Self {
        Self {
            inner: SourceBuilder::new("    "),
        }
    }
}

impl SlintEmitter {
    /// Construct a fresh emitter with Slint's 4-space indent.
    pub fn new() -> Self {
        Self::default()
    }

    /// Append a literal line at the current indent.
    pub fn line(&mut self, text: impl AsRef<str>) -> &mut Self {
        self.inner.line(text);
        self
    }

    /// Append a blank line.
    pub fn blank(&mut self) -> &mut Self {
        self.inner.blank();
        self
    }

    /// `Header { body }` block with nested indent. The body closure
    /// can return a [`RenderError`] so component implementations can
    /// propagate walker errors through recursion.
    pub fn block<F>(&mut self, header: impl AsRef<str>, body: F) -> Result<(), RenderError>
    where
        F: FnOnce(&mut Self) -> Result<(), RenderError>,
    {
        let opened = format!("{} {{", header.as_ref());
        self.inner.line(opened);
        self.inner.indent();
        let result = body(self);
        self.inner.dedent();
        self.inner.line("}");
        result
    }

    /// `key: value;` property binding. Emit raw DSL values — strings
    /// must be pre-quoted, colors must be in `#rgba` or `Colors.red`
    /// form, etc. Prefer the typed shortcuts below.
    pub fn property(&mut self, key: impl AsRef<str>, value: impl AsRef<str>) -> &mut Self {
        self.inner
            .line(format!("{}: {};", key.as_ref(), value.as_ref()));
        self
    }

    /// Typed string-property shortcut. Escapes the value so
    /// user-supplied text can't break out of the binding.
    pub fn prop_string(&mut self, key: impl AsRef<str>, value: impl AsRef<str>) -> &mut Self {
        self.inner.line(format!(
            "{}: \"{}\";",
            key.as_ref(),
            escape_slint_string(value.as_ref())
        ));
        self
    }

    /// Typed integer-property shortcut.
    pub fn prop_int(&mut self, key: impl AsRef<str>, value: i64) -> &mut Self {
        self.inner.line(format!("{}: {};", key.as_ref(), value));
        self
    }

    /// Typed float-property shortcut (no unit suffix — the DSL
    /// inspector infers `length` / `angle` from the target property).
    pub fn prop_float(&mut self, key: impl AsRef<str>, value: f64) -> &mut Self {
        self.inner.line(format!("{}: {};", key.as_ref(), value));
        self
    }

    /// Typed length-property shortcut. Emits `value px`.
    pub fn prop_px(&mut self, key: impl AsRef<str>, value: f64) -> &mut Self {
        self.inner.line(format!("{}: {}px;", key.as_ref(), value));
        self
    }

    /// Typed bool-property shortcut.
    pub fn prop_bool(&mut self, key: impl AsRef<str>, value: bool) -> &mut Self {
        self.inner.line(format!("{}: {};", key.as_ref(), value));
        self
    }

    /// Typed color-property shortcut. Accepts any Slint-valid color
    /// literal — `"#1a2b3c"`, `"Colors.red"`, `"rgba(…)"`. Does not
    /// validate — the compiler complains loudly enough on typos.
    pub fn prop_color(&mut self, key: impl AsRef<str>, value: impl AsRef<str>) -> &mut Self {
        self.inner
            .line(format!("{}: {};", key.as_ref(), value.as_ref()));
        self
    }

    /// Collapse the accumulated lines into one `\n`-joined string.
    pub fn build(&self) -> String {
        self.inner.build()
    }

    /// Borrow the current buffer as a string slice. Tests use this
    /// to assert on intermediate state without collapsing.
    pub fn as_string(&self) -> String {
        self.inner.build()
    }
}

/// Sanitiser for Slint identifiers. Slint element ids accept
/// `[A-Za-z_][A-Za-z0-9_-]*`; we map every other character to `_` so
/// a `BuilderDocument` node id is safe to splat into the DSL verbatim.
#[derive(Debug, Clone)]
pub struct SlintIdent(pub String);

impl SlintIdent {
    /// Normalise `raw` into a Slint-safe identifier. Empty or
    /// digit-leading inputs get a leading `n` so the result always
    /// parses.
    pub fn normalize(raw: &str) -> String {
        let mut out = String::with_capacity(raw.len() + 1);
        let mut chars = raw.chars();
        match chars.next() {
            None => return "n".to_string(),
            Some(c) if c.is_ascii_alphabetic() || c == '_' => out.push(c),
            Some(c) if c.is_ascii_digit() => {
                out.push('n');
                out.push(c);
            }
            Some(_) => out.push('_'),
        }
        for c in chars {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                out.push(c);
            } else {
                out.push('_');
            }
        }
        out
    }
}

/// Escape a string for safe inclusion inside a `".."` Slint literal.
/// Replaces backslash, double-quote, newline, carriage return, and
/// tab with their escape sequences. Matches what the Slint compiler
/// itself accepts in string literals (see `i-slint-compiler` tests).
pub fn escape_slint_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out
}

/// Format a `prism_core::design_tokens::Rgba` as a Slint `#rrggbbaa`
/// literal the DSL accepts in any `color` property.
pub fn rgba_to_slint_literal(c: prism_core::design_tokens::Rgba) -> String {
    format!("#{:02x}{:02x}{:02x}{:02x}", c.r, c.g, c.b, c.a)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_indents_nested_body() {
        let mut e = SlintEmitter::new();
        e.block("Rectangle", |e| {
            e.prop_px("width", 100.0);
            Ok(())
        })
        .unwrap();
        assert_eq!(e.build(), "Rectangle {\n    width: 100px;\n}");
    }

    #[test]
    fn string_literals_are_escaped() {
        let mut e = SlintEmitter::new();
        e.prop_string("text", "hello \"world\"\nsecond line");
        assert_eq!(e.build(), r#"text: "hello \"world\"\nsecond line";"#);
    }

    #[test]
    fn numeric_properties_emit_plain_tokens() {
        let mut e = SlintEmitter::new();
        e.prop_int("level", 3);
        e.prop_float("opacity", 0.5);
        e.prop_px("height", 48.0);
        assert_eq!(e.build(), "level: 3;\nopacity: 0.5;\nheight: 48px;");
    }

    #[test]
    fn bool_property_emits_true_false() {
        let mut e = SlintEmitter::new();
        e.prop_bool("visible", true);
        assert_eq!(e.build(), "visible: true;");
    }

    #[test]
    fn block_propagates_body_errors() {
        let mut e = SlintEmitter::new();
        let err = e
            .block("Rectangle", |_| Err(RenderError::Failed("nope".into())))
            .unwrap_err();
        assert!(matches!(err, RenderError::Failed(_)));
        // Block is still closed even on failure.
        assert!(e.build().ends_with("}"));
    }

    #[test]
    fn nested_block_indents_properly() {
        let mut e = SlintEmitter::new();
        e.block("VerticalLayout", |e| {
            e.block("Text", |e| {
                e.prop_string("text", "hi");
                Ok(())
            })
        })
        .unwrap();
        assert_eq!(
            e.build(),
            "VerticalLayout {\n    Text {\n        text: \"hi\";\n    }\n}"
        );
    }

    #[test]
    fn slint_ident_normalizes_weird_input() {
        assert_eq!(SlintIdent::normalize("hello"), "hello");
        assert_eq!(SlintIdent::normalize("1node"), "n1node");
        assert_eq!(SlintIdent::normalize(""), "n");
        assert_eq!(SlintIdent::normalize("foo bar!"), "foo_bar_");
        assert_eq!(SlintIdent::normalize("_underscore"), "_underscore");
        assert_eq!(SlintIdent::normalize("kebab-ok"), "kebab-ok");
    }

    #[test]
    fn rgba_literal_is_eight_digit_hex() {
        let c = prism_core::design_tokens::Rgba::new(0x1a, 0x2b, 0x3c, 0xff);
        assert_eq!(rgba_to_slint_literal(c), "#1a2b3cff");
    }
}
