//! `SourceBuilder` — line-oriented text buffer with an indent depth.
//!
//! Port of `language/codegen/source-builder.ts`. Used by every
//! emitter in this module to accumulate output. The Rust port keeps
//! the fluent chaining style from the TS version — each mutator
//! returns `&mut Self` — so call sites read the same as before.

/// Line-oriented buffer with an indent stack.
///
/// The indent string is configurable (`"  "` / two spaces by default,
/// `"\t"` for the GDScript emitter). Every [`line`](Self::line) /
/// [`block`](Self::block) / [`const_block`](Self::const_block) call
/// appends at the current depth.
#[derive(Debug, Clone)]
pub struct SourceBuilder {
    lines: Vec<String>,
    depth: usize,
    indent: String,
}

impl Default for SourceBuilder {
    fn default() -> Self {
        Self::new("  ")
    }
}

impl SourceBuilder {
    /// Construct a builder with a custom indent string.
    pub fn new(indent: impl Into<String>) -> Self {
        Self {
            lines: Vec::new(),
            depth: 0,
            indent: indent.into(),
        }
    }

    /// Append one line at the current depth. Empty `text` produces a
    /// blank line (no leading indent), matching the TS semantics.
    pub fn line(&mut self, text: impl AsRef<str>) -> &mut Self {
        let t = text.as_ref();
        if t.is_empty() {
            self.lines.push(String::new());
        } else {
            let mut s = self.indent.repeat(self.depth);
            s.push_str(t);
            self.lines.push(s);
        }
        self
    }

    /// Append a blank line.
    pub fn blank(&mut self) -> &mut Self {
        self.line("")
    }

    /// Push one indent level.
    pub fn indent(&mut self) -> &mut Self {
        self.depth += 1;
        self
    }

    /// Pop one indent level. Clamps at 0.
    pub fn dedent(&mut self) -> &mut Self {
        if self.depth > 0 {
            self.depth -= 1;
        }
        self
    }

    /// Emit a `header {` / indented body / `close` block.
    ///
    /// `close` defaults to `"}"` via [`block`](Self::block); pass an
    /// explicit closer with [`block_with`](Self::block_with) when the
    /// language uses a different terminator.
    pub fn block<F>(&mut self, header: impl AsRef<str>, body: F) -> &mut Self
    where
        F: FnOnce(&mut Self),
    {
        self.block_with(header, body, "}")
    }

    /// Variant of [`block`](Self::block) with an explicit close token.
    pub fn block_with<F>(
        &mut self,
        header: impl AsRef<str>,
        body: F,
        close: impl AsRef<str>,
    ) -> &mut Self
    where
        F: FnOnce(&mut Self),
    {
        let mut opened = String::from(header.as_ref());
        opened.push_str(" {");
        self.line(opened);
        self.indent();
        body(self);
        self.dedent();
        self.line(close.as_ref());
        self
    }

    /// Append a `// text` line at the current depth.
    pub fn comment(&mut self, text: impl AsRef<str>) -> &mut Self {
        let mut s = String::from("// ");
        s.push_str(text.as_ref());
        self.line(s)
    }

    /// Emit a TypeScript `export const name = { ... } as const;` block.
    pub fn const_block<F>(&mut self, name: impl AsRef<str>, body: F) -> &mut Self
    where
        F: FnOnce(&mut Self),
    {
        self.line(format!("export const {} = {{", name.as_ref()));
        self.indent();
        body(self);
        self.dedent();
        self.line("} as const;");
        self
    }

    /// Collapse the accumulated lines into one `\n`-joined string.
    pub fn build(&self) -> String {
        self.lines.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_indents_nonempty_and_leaves_blank_alone() {
        let mut b = SourceBuilder::default();
        b.line("top").indent().line("inner").dedent().blank();
        assert_eq!(b.build(), "top\n  inner\n");
    }

    #[test]
    fn dedent_clamps_at_zero() {
        let mut b = SourceBuilder::default();
        b.dedent().dedent().line("x");
        assert_eq!(b.build(), "x");
    }

    #[test]
    fn block_wraps_body_with_braces() {
        let mut b = SourceBuilder::default();
        b.block("if (true)", |b| {
            b.line("doit();");
        });
        assert_eq!(b.build(), "if (true) {\n  doit();\n}");
    }

    #[test]
    fn block_with_allows_custom_close() {
        let mut b = SourceBuilder::default();
        b.block_with(
            "func x()",
            |b| {
                b.line("pass");
            },
            "end",
        );
        assert_eq!(b.build(), "func x() {\n  pass\nend");
    }

    #[test]
    fn const_block_emits_as_const() {
        let mut b = SourceBuilder::default();
        b.const_block("ID", |b| {
            b.line("FOO: 'foo',");
            b.line("BAR: 'bar',");
        });
        assert_eq!(
            b.build(),
            "export const ID = {\n  FOO: 'foo',\n  BAR: 'bar',\n} as const;"
        );
    }

    #[test]
    fn comment_prefixes_with_slashes() {
        let mut b = SourceBuilder::default();
        b.comment("hello");
        assert_eq!(b.build(), "// hello");
    }

    #[test]
    fn custom_indent_uses_tabs() {
        let mut b = SourceBuilder::new("\t");
        b.line("a").indent().line("b").dedent();
        assert_eq!(b.build(), "a\n\tb");
    }
}
