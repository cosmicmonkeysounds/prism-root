//! XML → canonical migration tool — Phase 2 of the expressiveness
//! roadmap (`docs/dev/prui-expressiveness-roadmap.md` §8).
//!
//! Mechanically rewrites a legacy XML-shape PRUI document into the
//! canonical surface (§6.24 cheatsheet). The tool is purely
//! syntactic — it does not lower / typecheck / semantically
//! validate — so its output round-trips through [`super::parse`]
//! straight into the same AST shape. Ambiguous cases (anything the
//! cheatsheet doesn't list) get a leading `-- TODO:` comment so a
//! human review pass cleans them up.
//!
//! ## Cheatsheet rows handled
//!
//! | XML form (legacy)                       | Canonical form                          |
//! |-----------------------------------------|-----------------------------------------|
//! | `<component Name attrs>body</component>`| `component Name(params) { body }`       |
//! | `<trait Name attrs/>`                   | `trait Name { members }`                |
//! | `<mixin Name>body</mixin>`              | `mixin Name { body }`                   |
//! | `<macro Name>match/expand</macro>`      | `macro Name(captures) { match/expand }` |
//! | `<state name=v>`                        | `let name = state(v)`                   |
//! | `<on event>{handler}</on>`              | `on event(e) { handler }`               |
//! | `<style>…</style>`                      | `style { … }`                           |
//! | `<import stylesheet="x"/>`              | `import "x"`                            |
//! | `<import script="x"/> [as h]`           | `import "x" [as h]`                     |
//! | `<import component="x"/>`               | `import "x"`                            |
//! | `<namespace=Foo/>`                      | `namespace Foo`                         |
//! | `extends=Parent` (header attr)          | `use Parent` (body statement)           |
//! | `impls=[A, B]` (header attr)            | `: A, B` (after params)                 |
//! | `derives=[M]` (header attr)             | `use M` (body statement)                |
//! | `capabilities=[c: C]` (header attr)     | `requires c: C` (body statement)        |
//!
//! Other XML shapes — render trees inside component bodies, macro
//! `<match>` / `<expand>` blocks, the `<case>` arms of `<match>`
//! — stay as XML inside the canonical body (§6.23 "what stays
//! XML"). The migration tool emits them verbatim.
//!
//! The entry point is [`rewrite_xml_to_canonical`]. It accepts a
//! source string and returns the canonical rewrite plus any
//! [`super::ast::ParseError`]s the XML reader surfaced (so the
//! migration CLI can refuse to overwrite a malformed file).

use crate::language::syntax::SourceRange;

use super::super::ast::{
    Attribute, AttributeNamespace, AttributeValue, Document, Element, Node, ParseError,
    TemplatePart,
};

/// Rewrite an XML-shape PRUI source into the canonical surface.
///
/// Returns `(canonical_source, parse_errors)`. If
/// `parse_errors` is non-empty the rewrite is best-effort —
/// the migration tool's CLI front-end should refuse to overwrite
/// the file unless `--force` is supplied.
///
/// Input that is **already canonical** (per
/// [`super::canonical::looks_canonical`]) is returned verbatim;
/// the tool is idempotent.
pub fn rewrite_xml_to_canonical(source: &str) -> (String, Vec<ParseError>) {
    if super::canonical::looks_canonical(source) {
        return (source.to_string(), Vec::new());
    }
    let (doc, errs) = super::parse_xml(source);
    let mut out = String::new();
    let mut writer = Writer::new(&mut out);
    writer.emit_document(&doc);
    (out, errs)
}

struct Writer<'b> {
    out: &'b mut String,
    indent: usize,
}

impl<'b> Writer<'b> {
    fn new(out: &'b mut String) -> Self {
        Self { out, indent: 0 }
    }

    fn emit_document(&mut self, doc: &Document) {
        // Top-level nodes come from the XML reader. Emit each one in
        // order, separated by blank lines where natural.
        let mut prev_kind: Option<&'static str> = None;
        for node in &doc.nodes {
            let kind = top_level_kind(node);
            if let Some(p) = prev_kind {
                if needs_blank_line(p, kind) {
                    self.out.push('\n');
                }
            }
            self.emit_top_level(node);
            self.ensure_trailing_newline();
            prev_kind = Some(kind);
        }
    }

    fn emit_top_level(&mut self, node: &Node) {
        match node {
            Node::Element(el) => self.emit_top_level_element(el),
            Node::Comment { value, .. } => {
                // XML comment → `-- ` block. Multi-line comments
                // break on `\n` and re-prefix every line.
                let trimmed: &str = value.trim();
                for line in trimmed.lines() {
                    let stripped: &str = line.trim_end();
                    self.out.push_str("-- ");
                    self.out.push_str(stripped);
                    self.out.push('\n');
                }
            }
            Node::Text { value, .. } => {
                let trimmed: &str = value.trim();
                if trimmed.is_empty() {
                    return;
                }
                // Loose top-level text — preserve as a comment so
                // the reviewer can decide what to do with it.
                self.out
                    .push_str("-- TODO: loose top-level text from migration: ");
                self.out.push_str(trimmed);
                self.out.push('\n');
            }
            Node::Interpolation(expr) => {
                self.out.push_str("-- TODO: loose top-level `{");
                self.out.push_str(&expr.body);
                self.out.push_str("}` from migration\n");
            }
        }
    }

    fn emit_top_level_element(&mut self, el: &Element) {
        match el.tag.as_str() {
            "namespace" => self.emit_namespace(el),
            "import" => self.emit_import(el),
            "component" => self.emit_decl_with_body(el, "component"),
            "trait" => self.emit_decl_with_body(el, "trait"),
            "mixin" => self.emit_decl_with_body(el, "mixin"),
            "macro" => self.emit_decl_with_body(el, "macro"),
            "style" => self.emit_style_block(el),
            _ => {
                // Plain render tree at file scope. Emit verbatim
                // (XML stays canonical for trees — §6.23).
                self.emit_xml_element(el);
            }
        }
    }

    fn emit_namespace(&mut self, el: &Element) {
        let name = attr_string(el, "name").unwrap_or_default();
        self.out.push_str("namespace ");
        self.out.push_str(&name);
        self.out.push('\n');
    }

    fn emit_import(&mut self, el: &Element) {
        // `<import stylesheet="x"/>` / `<import script="x" as h/>` /
        // `<import component="x"/>` → `import "x" [as h]`. We pick
        // whichever projection attribute is present.
        let path = attr_string(el, "stylesheet")
            .or_else(|| attr_string(el, "script"))
            .or_else(|| attr_string(el, "component"))
            .or_else(|| attr_string(el, "dialect"))
            .unwrap_or_default();
        let alias = attr_string(el, "as");
        self.out.push_str("import \"");
        self.out.push_str(&escape_quoted(&path));
        self.out.push('"');
        if let Some(a) = alias {
            self.out.push_str(" as ");
            self.out.push_str(&a);
        }
        self.out.push('\n');
    }

    fn emit_decl_with_body(&mut self, el: &Element, keyword: &str) {
        // The legacy XML form stores the declaration name as either
        // the first bare attribute (`<component Card …>`) or an
        // explicit `name=` attribute. Try both.
        let name = pick_decl_name(el).unwrap_or_default();

        // Property children → parameter list. (Property sub-tags
        // are the §7.1 declaration shape; the canonical version
        // moves them onto the header.)
        let props: Vec<&Element> = el
            .children
            .iter()
            .filter_map(|n| match n {
                Node::Element(e) if e.tag == "property" => Some(e),
                _ => None,
            })
            .collect();
        let header_attrs = collect_header_attrs(el);

        // `extends=` → `use Parent` body statement.
        // `derive=` / `derives=` → `use M` body statement.
        // `impls=` → `: A, B` after params.
        // `capabilities=` → `requires …` body statement.
        let impls = header_attrs
            .iter()
            .find(|(k, _)| *k == "impls" || *k == "implements")
            .map(|(_, v)| v.clone());
        let extends = header_attrs
            .iter()
            .find(|(k, _)| *k == "extends")
            .map(|(_, v)| v.clone());
        let derives = header_attrs
            .iter()
            .find(|(k, _)| *k == "derive" || *k == "derives")
            .map(|(_, v)| v.clone());
        let capabilities = header_attrs
            .iter()
            .find(|(k, _)| *k == "capabilities" || *k == "capability")
            .map(|(_, v)| v.clone());

        // Header line: `<keyword> Name(params)[ : Impls]`.
        self.out.push_str(keyword);
        if !name.is_empty() {
            self.out.push(' ');
            self.out.push_str(&name);
        }
        if !props.is_empty() {
            self.out.push('(');
            let mut first = true;
            for p in &props {
                if !first {
                    self.out.push_str(", ");
                }
                first = false;
                let pn = attr_string(p, "name").unwrap_or_default();
                let pt = attr_string(p, "type");
                let pd = attr_string(p, "default");
                let pr = attr_string(p, "required");
                self.out.push_str(&pn);
                if let Some(t) = pt {
                    self.out.push_str(": ");
                    self.out.push_str(&t);
                }
                if let Some(d) = pd {
                    self.out.push_str(" = ");
                    self.out.push_str(&d);
                }
                if pr.as_deref() == Some("true") {
                    self.out.push_str(" required");
                }
            }
            self.out.push(')');
        }
        if let Some(traits) = impls {
            self.out.push_str(" : ");
            self.out.push_str(&strip_list_brackets(&traits));
        }

        // Body — everything that isn't a `<property>` sub-tag.
        // If the only meaningful body content is one render-tree
        // element and there are no `use` / `requires` / `style` /
        // `let` / `on` additions, emit the trivial form
        // (`= <tree/>`). Otherwise open a `{ … }` block.
        let body_children: Vec<&Node> = el
            .children
            .iter()
            .filter(|n| !matches!(n, Node::Element(e) if e.tag == "property"))
            .collect();

        // Drop whitespace-only text nodes when deciding whether the
        // body is "just one element" — formatting blank lines
        // shouldn't force a block form.
        let meaningful: Vec<&&Node> = body_children
            .iter()
            .filter(|n| match n {
                Node::Text { value, .. } => !value.trim().is_empty(),
                Node::Element(_) | Node::Interpolation(_) | Node::Comment { .. } => true,
            })
            .collect();
        let single_element_body =
            meaningful.len() == 1 && matches!(meaningful[0], Node::Element(_));

        let needs_block = extends.is_some()
            || derives.is_some()
            || capabilities.is_some()
            || !single_element_body;

        if !needs_block {
            // `= <tree/>` form.
            self.out.push_str(" =\n  ");
            if let Some(Node::Element(child)) = meaningful.first().copied() {
                self.emit_xml_element_inline(child);
            }
            self.out.push('\n');
            return;
        }

        // Block form.
        self.out.push_str(" {\n");
        self.indent += 1;

        // Body statements first: `use Extends`, `use Derives`,
        // `requires Capabilities`, `style {…}`, then the render
        // tree.
        if let Some(parent) = extends {
            self.emit_line(&format!("use {parent}"));
        }
        if let Some(d) = derives {
            for n in split_list(&d) {
                self.emit_line(&format!("use {n}"));
            }
        }
        if let Some(c) = capabilities {
            // `[c: C, n: N]` → one `requires` per binding.
            for binding in split_list(&c) {
                self.emit_line(&format!("requires {}", binding.trim()));
            }
        }

        // Other child statements that we know how to lower.
        for child in body_children {
            self.emit_body_child(child);
        }

        self.indent -= 1;
        self.write_indent();
        self.out.push_str("}\n");
    }

    fn emit_body_child(&mut self, node: &Node) {
        match node {
            Node::Element(el) => match el.tag.as_str() {
                "state" => {
                    // `<state name=v>` → `let name = state(v)`.
                    let name = pick_decl_name(el).unwrap_or_default();
                    let value = first_non_name_attr_value(el).unwrap_or_default();
                    if value.is_empty() {
                        self.emit_line(&format!("let {name} = state(nil)"));
                    } else {
                        self.emit_line(&format!("let {name} = state({value})"));
                    }
                }
                "on" => {
                    // `<on event>{handler}</on>` → `on event { handler }`.
                    let event = pick_decl_name(el)
                        .or_else(|| attr_string(el, "event"))
                        .unwrap_or_default();
                    // Body: collect the children's textual rep.
                    self.write_indent();
                    self.out.push_str("on ");
                    self.out.push_str(&event);
                    if let Some(args) = attr_string(el, "args") {
                        self.out.push('(');
                        self.out.push_str(&args);
                        self.out.push(')');
                    }
                    self.out.push_str(" {\n");
                    self.indent += 1;
                    for child in &el.children {
                        self.emit_handler_child(child);
                    }
                    self.indent -= 1;
                    self.write_indent();
                    self.out.push_str("}\n");
                }
                "style" => {
                    // `<style>…</style>` → `style { … }`.
                    self.write_indent();
                    self.out.push_str("style {");
                    let raw = collect_raw_text(el);
                    if raw.contains('\n') {
                        self.out.push('\n');
                        for line in raw.lines() {
                            self.indent += 1;
                            self.write_indent();
                            self.indent -= 1;
                            self.out.push_str(line.trim_start());
                            self.out.push('\n');
                        }
                        self.write_indent();
                    } else {
                        self.out.push(' ');
                        self.out.push_str(raw.trim());
                        self.out.push(' ');
                    }
                    self.out.push_str("}\n");
                }
                "let" => {
                    // Canonical-shape `<let>` (rare in legacy
                    // sources; the canonical AST projection uses
                    // it, but the XML reader might inherit one
                    // through a partially-migrated file).
                    let name = attr_string(el, "name").unwrap_or_default();
                    let value = attr_string(el, "value").unwrap_or_default();
                    self.emit_line(&format!("let {name} = {value}"));
                }
                "requires" => {
                    let names = attr_string(el, "names").unwrap_or_default();
                    for binding in split_list(&names) {
                        self.emit_line(&format!("requires {}", binding.trim()));
                    }
                }
                "use" => {
                    let names = attr_string(el, "names").unwrap_or_default();
                    for n in split_list(&names) {
                        self.emit_line(&format!("use {}", n.trim()));
                    }
                }
                _ => {
                    // Render-tree element — emit XML verbatim.
                    self.write_indent();
                    self.emit_xml_element_inline(el);
                    self.out.push('\n');
                }
            },
            Node::Text { value, .. } => {
                let trimmed = value.trim();
                if !trimmed.is_empty() {
                    self.write_indent();
                    self.out.push_str(trimmed);
                    self.out.push('\n');
                }
            }
            Node::Interpolation(expr) => {
                self.write_indent();
                self.out.push('{');
                self.out.push_str(&expr.body);
                self.out.push_str("}\n");
            }
            Node::Comment { value, .. } => {
                let trimmed: &str = value.trim();
                for line in trimmed.lines() {
                    let stripped: &str = line.trim_end();
                    self.write_indent();
                    self.out.push_str("-- ");
                    self.out.push_str(stripped);
                    self.out.push('\n');
                }
            }
        }
    }

    fn emit_handler_child(&mut self, node: &Node) {
        // Handler bodies are loose `{…}` chunks in the XML form. We
        // emit them as opaque lines — anything that isn't a plain
        // text run becomes a `-- TODO` for the reviewer.
        match node {
            Node::Text { value, .. } => {
                let trimmed = value.trim();
                if !trimmed.is_empty() {
                    self.emit_line(trimmed);
                }
            }
            Node::Interpolation(expr) => {
                self.emit_line(&expr.body);
            }
            Node::Element(el) => {
                self.write_indent();
                self.emit_xml_element_inline(el);
                self.out.push('\n');
            }
            Node::Comment { value, .. } => {
                self.emit_line(&format!("-- {}", value.trim()));
            }
        }
    }

    fn emit_style_block(&mut self, el: &Element) {
        // Top-level `<style>` block → `class <name> { … }`. If
        // there is no `class` attribute (older inline stylesheet
        // form), emit a bare `style { … }` block — that still
        // parses on the canonical side.
        let class = attr_string(el, "class");
        let body = collect_raw_text(el);
        if let Some(c) = class {
            self.out.push_str("class ");
            self.out.push_str(&c);
            self.out.push_str(" {\n");
            for line in body.lines() {
                self.out.push_str("  ");
                self.out.push_str(line.trim_start());
                self.out.push('\n');
            }
            self.out.push_str("}\n");
        } else {
            self.out.push_str("style {\n");
            for line in body.lines() {
                self.out.push_str("  ");
                self.out.push_str(line.trim_start());
                self.out.push('\n');
            }
            self.out.push_str("}\n");
        }
    }

    fn emit_xml_element(&mut self, el: &Element) {
        self.write_indent();
        self.emit_xml_element_inline(el);
        self.out.push('\n');
    }

    /// Emit an element as XML in its original shape — used for
    /// render trees inside a canonical body, where XML stays
    /// canonical (§6.23).
    fn emit_xml_element_inline(&mut self, el: &Element) {
        self.out.push('<');
        self.out.push_str(&el.tag);
        for attr in &el.attributes {
            self.out.push(' ');
            self.out.push_str(&attr.name.raw);
            match &attr.value {
                AttributeValue::Empty => {}
                AttributeValue::String { value, .. } => {
                    self.out.push_str("=\"");
                    self.out.push_str(&escape_quoted(value));
                    self.out.push('"');
                }
                AttributeValue::Expression(expr) => {
                    self.out.push_str("={");
                    self.out.push_str(&expr.body);
                    self.out.push('}');
                }
                AttributeValue::Template { parts, .. } => {
                    self.out.push_str("=\"");
                    for part in parts {
                        match part {
                            TemplatePart::Literal { value, .. } => {
                                self.out.push_str(&escape_quoted(value));
                            }
                            TemplatePart::Expression(expr) => {
                                self.out.push('{');
                                self.out.push_str(&expr.body);
                                self.out.push('}');
                            }
                        }
                    }
                    self.out.push('"');
                }
            }
        }
        if el.self_closing {
            self.out.push_str("/>");
            return;
        }
        self.out.push('>');
        for child in &el.children {
            self.emit_xml_child(child);
        }
        self.out.push_str("</");
        self.out.push_str(&el.tag);
        self.out.push('>');
    }

    fn emit_xml_child(&mut self, node: &Node) {
        match node {
            Node::Element(el) => self.emit_xml_element_inline(el),
            Node::Text { value, .. } => self.out.push_str(value),
            Node::Interpolation(expr) => {
                self.out.push('{');
                self.out.push_str(&expr.body);
                self.out.push('}');
            }
            Node::Comment { value, .. } => {
                self.out.push_str("<!--");
                self.out.push_str(value);
                self.out.push_str("-->");
            }
        }
    }

    fn write_indent(&mut self) {
        for _ in 0..self.indent {
            self.out.push_str("  ");
        }
    }

    fn emit_line(&mut self, content: &str) {
        self.write_indent();
        self.out.push_str(content);
        self.out.push('\n');
    }

    fn ensure_trailing_newline(&mut self) {
        if !self.out.ends_with('\n') {
            self.out.push('\n');
        }
    }
}

// ── helpers ───────────────────────────────────────────────────────

fn attr_string(el: &Element, name: &str) -> Option<String> {
    el.attributes
        .iter()
        .find(|a| a.name.raw == name || a.name.local == name)
        .and_then(|a| match &a.value {
            AttributeValue::String { value, .. } => Some(value.clone()),
            AttributeValue::Expression(e) => Some(format!("{{{}}}", e.body)),
            AttributeValue::Template { parts, .. } => {
                let mut s = String::new();
                for p in parts {
                    match p {
                        TemplatePart::Literal { value, .. } => s.push_str(value),
                        TemplatePart::Expression(e) => {
                            s.push('{');
                            s.push_str(&e.body);
                            s.push('}');
                        }
                    }
                }
                Some(s)
            }
            AttributeValue::Empty => None,
        })
}

fn pick_decl_name(el: &Element) -> Option<String> {
    if let Some(n) = attr_string(el, "name") {
        return Some(n);
    }
    // Legacy XML allowed `<component Card …>` — the name is the
    // first bare attribute with an empty value (boolean attr).
    for a in &el.attributes {
        if a.name.namespace == AttributeNamespace::Bare
            && matches!(a.value, AttributeValue::Empty)
            && !is_reserved_attr(&a.name.raw)
        {
            return Some(a.name.raw.clone());
        }
    }
    None
}

fn first_non_name_attr_value(el: &Element) -> Option<String> {
    for a in &el.attributes {
        if a.name.raw == "name" {
            continue;
        }
        return match &a.value {
            AttributeValue::String { value, .. } => Some(value.clone()),
            AttributeValue::Expression(e) => Some(format!("{{{}}}", e.body)),
            _ => None,
        };
    }
    None
}

fn collect_header_attrs(el: &Element) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for a in &el.attributes {
        if matches!(
            a.name.raw.as_str(),
            "extends"
                | "impls"
                | "implements"
                | "derive"
                | "derives"
                | "capabilities"
                | "capability"
        ) {
            if let AttributeValue::String { value, .. } = &a.value {
                out.push((a.name.raw.clone(), value.clone()));
            }
        }
    }
    out
}

fn is_reserved_attr(name: &str) -> bool {
    matches!(
        name,
        "name"
            | "extends"
            | "impls"
            | "implements"
            | "derive"
            | "derives"
            | "capabilities"
            | "capability"
            | "stylesheet"
            | "script"
            | "component"
            | "dialect"
            | "as"
            | "if"
            | "for"
            | "else"
            | "else-if"
    )
}

fn split_list(raw: &str) -> Vec<String> {
    strip_list_brackets(raw)
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn strip_list_brackets(raw: &str) -> String {
    let t = raw.trim();
    if t.starts_with('[') && t.ends_with(']') {
        t[1..t.len() - 1].trim().to_string()
    } else {
        t.to_string()
    }
}

fn collect_raw_text(el: &Element) -> String {
    let mut out = String::new();
    for child in &el.children {
        if let Node::Text { value, .. } = child {
            out.push_str(value);
        }
    }
    out
}

fn escape_quoted(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn top_level_kind(node: &Node) -> &'static str {
    match node {
        Node::Element(el) => match el.tag.as_str() {
            "namespace" => "namespace",
            "import" => "import",
            "component" | "trait" | "mixin" | "macro" => "decl",
            "style" => "style",
            _ => "other",
        },
        Node::Comment { .. } => "comment",
        Node::Text { .. } => "text",
        Node::Interpolation(_) => "interp",
    }
}

fn needs_blank_line(prev: &str, curr: &str) -> bool {
    match (prev, curr) {
        ("namespace", _) => true,
        ("import", "import") => false,
        ("import", _) => true,
        (_, "decl") => true,
        ("decl", _) => true,
        _ => false,
    }
}

/// **Suppress an unused-import warning when the migrate module
/// otherwise never names `SourceRange`.** Defensive; we may grow
/// to need it for line-anchored TODO comments.
#[allow(dead_code)]
fn _unused(_: SourceRange, _: &Attribute) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrites_namespace() {
        let (out, errs) = rewrite_xml_to_canonical("<namespace name=\"Forms\"/>");
        assert!(errs.is_empty());
        assert!(out.starts_with("namespace Forms"));
    }

    #[test]
    fn rewrites_import_stylesheet() {
        let (out, _) = rewrite_xml_to_canonical("<import stylesheet=\"./theme.prss\"/>");
        assert_eq!(out.trim(), "import \"./theme.prss\"");
    }

    #[test]
    fn rewrites_import_with_alias() {
        let (out, _) = rewrite_xml_to_canonical("<import script=\"./db.luau\" as=\"db\"/>");
        assert_eq!(out.trim(), "import \"./db.luau\" as db");
    }

    #[test]
    fn rewrites_trivial_component() {
        let xml = r#"<component name="Avatar">
  <property name="src" type="string"/>
  <image src={src}/>
</component>"#;
        let (out, errs) = rewrite_xml_to_canonical(xml);
        assert!(errs.is_empty(), "errs: {errs:?}");
        assert!(out.contains("component Avatar(src: string)"));
        assert!(out.contains("<image"));
    }

    #[test]
    fn rewrites_component_with_extends() {
        let xml = r#"<component name="DangerButton" extends="BaseButton">
  <property name="label" type="string"/>
  <button>{label}</button>
</component>"#;
        let (out, _) = rewrite_xml_to_canonical(xml);
        assert!(out.contains("use BaseButton"));
        assert!(out.contains("component DangerButton(label: string)"));
    }

    #[test]
    fn rewrites_component_with_impls() {
        let xml = r#"<component name="TaskRow" impls="Focusable, Pointable">
  <property name="task" type="Task"/>
  <container/>
</component>"#;
        let (out, _) = rewrite_xml_to_canonical(xml);
        assert!(
            out.contains(": Focusable, Pointable"),
            "expected impls in `:` position, got:\n{out}"
        );
    }

    #[test]
    fn rewrites_component_with_capabilities() {
        let xml = r#"<component name="ShareButton" capabilities="clipboard: Clipboard">
  <property name="text" type="string" required="true"/>
  <button/>
</component>"#;
        let (out, _) = rewrite_xml_to_canonical(xml);
        assert!(out.contains("requires clipboard: Clipboard"));
        assert!(out.contains("text: string required"));
    }

    #[test]
    fn idempotent_on_canonical_input() {
        let canon = "component Card(t: string) = <text>{t}</text>\n";
        let (out, errs) = rewrite_xml_to_canonical(canon);
        assert!(errs.is_empty());
        assert_eq!(out, canon);
    }

    #[test]
    fn rewrites_macro() {
        let xml = r#"<macro name="Field">
  <property name="label" type="string"/>
  <property name="value" type="string"/>
  <container/>
</macro>"#;
        let (out, _) = rewrite_xml_to_canonical(xml);
        assert!(out.contains("macro Field(label: string, value: string)"));
    }

    #[test]
    fn rewrites_top_level_style() {
        let xml = r#"<style class="card">
  padding = 8
  radius = 12
</style>"#;
        let (out, _) = rewrite_xml_to_canonical(xml);
        assert!(out.contains("class card {"));
        assert!(out.contains("padding = 8"));
    }

    #[test]
    fn rewrites_full_corpus_round_trips() {
        // Drive the §6.24 cheatsheet through the rewriter, then
        // re-parse the canonical form and confirm the same logical
        // shape comes back out. This is the load-bearing test —
        // it ensures the rewriter is a no-op on AST shape.
        let xml = r#"<namespace name="App"/>
<import stylesheet="./theme.prss"/>
<import script="./db.luau" as="db"/>

<component name="Card">
  <property name="title" type="string" required="true"/>
  <property name="subtitle" type="string" default=""/>
  <container/>
</component>"#;
        let (canonical, errs) = rewrite_xml_to_canonical(xml);
        assert!(errs.is_empty(), "errs: {errs:?}");
        // The migration output must itself parse cleanly.
        let (doc, errs2) = crate::language::prism_ui::parse(&canonical);
        assert!(
            errs2.is_empty(),
            "canonical re-parse failed:\n{canonical}\nerrs: {errs2:?}"
        );
        // Expect: namespace + 2 imports + 1 component.
        let kinds: Vec<&str> = doc
            .nodes
            .iter()
            .filter_map(|n| match n {
                Node::Element(e) => Some(e.tag.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(kinds, vec!["namespace", "import", "import", "component"]);
    }

    #[test]
    fn rewrites_handles_legacy_bare_name_attr() {
        // Legacy XML allowed `<component Card …>` (bare boolean
        // attribute as the name). Rewriter recovers the name and
        // emits the canonical surface.
        let xml = r#"<component Card>
  <property name="t" type="string"/>
  <text>{t}</text>
</component>"#;
        let (out, _) = rewrite_xml_to_canonical(xml);
        assert!(out.contains("component Card(t: string)"), "got: {out}");
    }
}
