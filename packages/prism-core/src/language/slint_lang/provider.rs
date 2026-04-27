//! `SlintSyntaxProvider` — diagnostics, completions, and hover for
//! Slint `.slint` files.
//!
//! Lightweight provider that runs in prism-core without pulling in
//! `slint-interpreter`. For full compiler diagnostics, use the
//! `LiveDocument` in `prism-builder` (behind the `interpreter` feature).

use crate::language::syntax::{
    CompletionItem, CompletionKind, Diagnostic, DiagnosticSeverity, HoverInfo, SchemaContext,
    SyntaxProvider, TextRange,
};

#[derive(Debug, Clone, Default)]
pub struct SlintSyntaxProvider;

impl SlintSyntaxProvider {
    pub fn new() -> Self {
        Self
    }
}

const SLINT_KEYWORDS: &[(&str, &str)] = &[
    ("import", "Import elements from other files or modules"),
    ("from", "Specifies the source module in an import statement"),
    ("export", "Makes a component or type visible to other files"),
    ("component", "Declares a new component"),
    ("inherits", "Specifies the base component to inherit from"),
    ("property", "Declares a property on a component"),
    ("in", "Input property binding direction"),
    ("out", "Output property binding direction"),
    ("in-out", "Bidirectional property binding direction"),
    ("private", "Property visible only within the component"),
    ("callback", "Declares an event handler"),
    ("animate", "Defines property animation"),
    ("states", "Declares named states with property bindings"),
    ("transitions", "Defines animated transitions between states"),
    ("if", "Conditional element or expression"),
    (
        "for",
        "Repeater element — instantiates children from a model",
    ),
    ("pure", "Marks a callback or function as side-effect free"),
    ("function", "Declares a function within a component"),
    ("public", "Makes a function callable from external code"),
    ("struct", "Declares a named struct type"),
    ("enum", "Declares a named enum type"),
    ("global", "Declares a global singleton"),
];

const SLINT_ELEMENTS: &[(&str, &str)] = &[
    ("Rectangle", "A filled or stroked rectangle"),
    ("Image", "Displays an image from a source"),
    ("Text", "Renders a text string"),
    ("TouchArea", "Invisible area that receives pointer events"),
    ("FocusScope", "Manages keyboard focus for its children"),
    ("Flickable", "Scrollable container with inertia"),
    ("TextInput", "Single-line editable text field"),
    ("TextEdit", "Multi-line editable text area"),
    ("HorizontalLayout", "Arranges children in a horizontal row"),
    ("VerticalLayout", "Arranges children in a vertical column"),
    ("GridLayout", "Arranges children in a CSS Grid"),
    ("Row", "A row inside a GridLayout"),
    ("Path", "Draws a vector path"),
    ("PopupWindow", "A popup overlay window"),
    ("Dialog", "A modal dialog with standard buttons"),
    ("Window", "Top-level window element"),
    ("Timer", "Fires a callback at a regular interval"),
    ("ListView", "Virtualized scrollable list"),
    ("ScrollView", "Scrollable container with scrollbars"),
    ("VerticalBox", "VerticalLayout with default spacing/padding"),
    (
        "HorizontalBox",
        "HorizontalLayout with default spacing/padding",
    ),
    ("GridBox", "GridLayout with default spacing/padding"),
    ("Button", "Standard push button"),
    ("CheckBox", "Toggle checkbox with label"),
    ("ComboBox", "Drop-down selection widget"),
    ("LineEdit", "Single-line text input with border"),
    ("Slider", "Horizontal value slider"),
    ("SpinBox", "Numeric input with increment/decrement"),
    ("Switch", "Toggle switch widget"),
    ("TabWidget", "Container with tabbed pages"),
    ("Tab", "A single tab page inside TabWidget"),
    ("ProgressIndicator", "Shows progress as a bar"),
    ("Spinner", "Indeterminate loading spinner"),
    ("GroupBox", "Labeled container with a border"),
    ("StandardButton", "Pre-defined dialog button"),
    ("StandardListView", "List view with standard item rendering"),
    ("StandardTableView", "Table view with column headers"),
    ("AboutSlint", "Built-in About dialog"),
    ("Palette", "Access to the current theme's color palette"),
];

const SLINT_PROPERTIES: &[(&str, &str)] = &[
    ("width", "length — Element width"),
    ("height", "length — Element height"),
    ("x", "length — Horizontal position"),
    ("y", "length — Vertical position"),
    ("background", "brush — Background fill"),
    ("color", "color — Foreground/text color"),
    ("font-size", "length — Text font size"),
    ("font-weight", "int — Text font weight (100–900)"),
    ("font-family", "string — Text font family name"),
    ("text", "string — Text content"),
    ("visible", "bool — Whether the element is visible"),
    ("opacity", "float — Element opacity (0.0–1.0)"),
    ("enabled", "bool — Whether the element is interactive"),
    ("preferred-width", "length — Preferred width for layout"),
    ("preferred-height", "length — Preferred height for layout"),
    ("min-width", "length — Minimum width constraint"),
    ("min-height", "length — Minimum height constraint"),
    ("max-width", "length — Maximum width constraint"),
    ("max-height", "length — Maximum height constraint"),
    (
        "horizontal-alignment",
        "enum — Horizontal alignment within parent",
    ),
    (
        "vertical-alignment",
        "enum — Vertical alignment within parent",
    ),
    (
        "horizontal-stretch",
        "float — Stretch factor in horizontal layouts",
    ),
    (
        "vertical-stretch",
        "float — Stretch factor in vertical layouts",
    ),
    ("padding", "length — Uniform padding on all sides"),
    ("padding-left", "length — Left padding"),
    ("padding-right", "length — Right padding"),
    ("padding-top", "length — Top padding"),
    ("padding-bottom", "length — Bottom padding"),
    ("spacing", "length — Space between layout children"),
    ("alignment", "enum — Layout alignment mode"),
    ("source", "image — Image source (for Image element)"),
    ("checked", "bool — Whether checkbox/switch is on"),
    ("value", "float — Current value (for Slider/SpinBox)"),
    ("minimum", "float — Minimum value"),
    ("maximum", "float — Maximum value"),
    ("placeholder-text", "string — Placeholder for text inputs"),
    ("read-only", "bool — Whether text input is read-only"),
    ("wrap", "enum — Text wrapping mode"),
    ("overflow", "enum — Text overflow handling"),
    ("border-radius", "length — Corner radius"),
    ("border-width", "length — Border stroke width"),
    ("border-color", "color — Border stroke color"),
    ("clip", "bool — Whether to clip children to bounds"),
];

impl SyntaxProvider for SlintSyntaxProvider {
    fn name(&self) -> &str {
        "prism:slint"
    }

    fn diagnose(&self, source: &str, _context: Option<&SchemaContext>) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        check_brace_balance(source, &mut diagnostics);
        diagnostics
    }

    fn complete(
        &self,
        source: &str,
        offset: usize,
        context: Option<&SchemaContext>,
    ) -> Vec<CompletionItem> {
        let prefix = extract_word_prefix(source, offset);
        if prefix.is_empty() {
            return Vec::new();
        }
        let lower = prefix.to_lowercase();

        let mut items = Vec::new();

        for &(kw, doc) in SLINT_KEYWORDS {
            if kw.starts_with(&lower) {
                items.push(CompletionItem {
                    label: kw.to_string(),
                    kind: CompletionKind::Keyword,
                    detail: Some("keyword".into()),
                    documentation: Some(doc.to_string()),
                    sort_order: Some(300),
                    replace_range: None,
                    insert_text: None,
                });
            }
        }

        for &(name, doc) in SLINT_ELEMENTS {
            if name.to_lowercase().starts_with(&lower) || name.starts_with(&prefix) {
                items.push(CompletionItem {
                    label: name.to_string(),
                    kind: CompletionKind::Type,
                    detail: Some("element".into()),
                    documentation: Some(doc.to_string()),
                    sort_order: Some(100),
                    replace_range: None,
                    insert_text: None,
                });
            }
        }

        if looks_like_property_context(source, offset) {
            for &(name, doc) in SLINT_PROPERTIES {
                if name.starts_with(&lower) {
                    items.push(CompletionItem {
                        label: name.to_string(),
                        kind: CompletionKind::Field,
                        detail: Some("property".into()),
                        documentation: Some(doc.to_string()),
                        sort_order: Some(50),
                        replace_range: None,
                        insert_text: None,
                    });
                }
            }
        }

        if let Some(ctx) = context {
            for field in &ctx.fields {
                if field.id.starts_with(&prefix) {
                    items.push(CompletionItem {
                        label: field.id.clone(),
                        kind: CompletionKind::Field,
                        detail: Some(format!("{:?}", field.field_type)),
                        documentation: field.description.clone(),
                        sort_order: Some(25),
                        replace_range: None,
                        insert_text: None,
                    });
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
        _context: Option<&SchemaContext>,
    ) -> Option<HoverInfo> {
        let (word, start, end) = extract_word_at(source, offset)?;
        let range = TextRange { start, end };

        for &(name, doc) in SLINT_ELEMENTS {
            if name == word {
                return Some(HoverInfo {
                    range,
                    contents: format!("**{name}** (element)\n\n{doc}"),
                });
            }
        }

        for &(kw, doc) in SLINT_KEYWORDS {
            if kw == word {
                return Some(HoverInfo {
                    range,
                    contents: format!("**{kw}** (keyword)\n\n{doc}"),
                });
            }
        }

        for &(name, doc) in SLINT_PROPERTIES {
            if name == word {
                return Some(HoverInfo {
                    range,
                    contents: format!("**{name}** — {doc}"),
                });
            }
        }

        None
    }
}

fn check_brace_balance(source: &str, diagnostics: &mut Vec<Diagnostic>) {
    let mut stack: Vec<(char, usize)> = Vec::new();
    let mut in_string = false;
    let mut in_line_comment = false;
    let mut in_block_comment = false;
    let mut prev_char = '\0';

    for (offset, ch) in source.char_indices() {
        if in_line_comment {
            if ch == '\n' {
                in_line_comment = false;
            }
            prev_char = ch;
            continue;
        }
        if in_block_comment {
            if prev_char == '*' && ch == '/' {
                in_block_comment = false;
            }
            prev_char = ch;
            continue;
        }
        if in_string {
            if ch == '"' && prev_char != '\\' {
                in_string = false;
            }
            prev_char = ch;
            continue;
        }

        match ch {
            '/' if source.get(offset + 1..offset + 2) == Some("/") => {
                in_line_comment = true;
            }
            '/' if source.get(offset + 1..offset + 2) == Some("*") => {
                in_block_comment = true;
            }
            '"' => {
                in_string = true;
            }
            '{' | '(' | '[' => {
                stack.push((ch, offset));
            }
            '}' | ')' | ']' => {
                let expected = match ch {
                    '}' => '{',
                    ')' => '(',
                    ']' => '[',
                    _ => unreachable!(),
                };
                match stack.last() {
                    Some(&(open, _)) if open == expected => {
                        stack.pop();
                    }
                    Some(&(open, open_offset)) => {
                        diagnostics.push(Diagnostic {
                            message: format!(
                                "Mismatched bracket: expected closing for '{open}', found '{ch}'"
                            ),
                            severity: DiagnosticSeverity::Error,
                            range: TextRange {
                                start: open_offset,
                                end: offset + 1,
                            },
                            code: Some("bracket-mismatch".into()),
                        });
                        stack.pop();
                    }
                    None => {
                        diagnostics.push(Diagnostic {
                            message: format!("Unexpected closing bracket '{ch}'"),
                            severity: DiagnosticSeverity::Error,
                            range: TextRange {
                                start: offset,
                                end: offset + 1,
                            },
                            code: Some("unmatched-close".into()),
                        });
                    }
                }
            }
            _ => {}
        }
        prev_char = ch;
    }

    for (open, offset) in stack {
        diagnostics.push(Diagnostic {
            message: format!("Unclosed bracket '{open}'"),
            severity: DiagnosticSeverity::Error,
            range: TextRange {
                start: offset,
                end: offset + 1,
            },
            code: Some("unclosed-bracket".into()),
        });
    }
}

fn looks_like_property_context(source: &str, offset: usize) -> bool {
    let before = &source[..offset.min(source.len())];
    let trimmed = before.trim_end();
    if let Some(last_line) = trimmed.lines().last() {
        let stripped = last_line.trim();
        if stripped.is_empty() || stripped.ends_with('{') || stripped.ends_with(';') {
            return true;
        }
    }
    let brace_depth: i32 = before
        .chars()
        .map(|c| match c {
            '{' => 1,
            '}' => -1,
            _ => 0,
        })
        .sum();
    brace_depth > 0
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

    #[test]
    fn provider_name() {
        let p = SlintSyntaxProvider::new();
        assert_eq!(p.name(), "prism:slint");
    }

    #[test]
    fn diagnose_clean_source() {
        let p = SlintSyntaxProvider::new();
        let diags = p.diagnose("component Foo inherits Rectangle { }", None);
        assert!(diags.is_empty());
    }

    #[test]
    fn diagnose_unclosed_brace() {
        let p = SlintSyntaxProvider::new();
        let diags = p.diagnose("component Foo inherits Rectangle {", None);
        assert!(!diags.is_empty());
        assert!(diags[0].message.contains("Unclosed"));
    }

    #[test]
    fn diagnose_mismatched_bracket() {
        let p = SlintSyntaxProvider::new();
        let diags = p.diagnose("component Foo { )", None);
        assert!(!diags.is_empty());
        assert!(diags[0].message.contains("Mismatched"));
    }

    #[test]
    fn diagnose_ignores_strings() {
        let p = SlintSyntaxProvider::new();
        let diags = p.diagnose(r#"Text { text: "{ not a brace }"; }"#, None);
        assert!(diags.is_empty());
    }

    #[test]
    fn diagnose_ignores_comments() {
        let p = SlintSyntaxProvider::new();
        let diags = p.diagnose("// unclosed {\nRectangle { }", None);
        assert!(diags.is_empty());
    }

    #[test]
    fn complete_keyword() {
        let p = SlintSyntaxProvider::new();
        let items = p.complete("comp", 4, None);
        assert!(items.iter().any(|i| i.label == "component"));
    }

    #[test]
    fn complete_element() {
        let p = SlintSyntaxProvider::new();
        let items = p.complete("Rect", 4, None);
        assert!(items.iter().any(|i| i.label == "Rectangle"));
    }

    #[test]
    fn complete_element_case_insensitive() {
        let p = SlintSyntaxProvider::new();
        let items = p.complete("rect", 4, None);
        assert!(items.iter().any(|i| i.label == "Rectangle"));
    }

    #[test]
    fn complete_property_in_block() {
        let p = SlintSyntaxProvider::new();
        let src = "Rectangle {\n    wid";
        let items = p.complete(src, src.len(), None);
        assert!(items.iter().any(|i| i.label == "width"));
    }

    #[test]
    fn complete_empty_prefix_returns_nothing() {
        let p = SlintSyntaxProvider::new();
        assert!(p.complete("", 0, None).is_empty());
    }

    #[test]
    fn hover_element() {
        let p = SlintSyntaxProvider::new();
        let hover = p.hover("Rectangle { }", 4, None);
        assert!(hover.is_some());
        assert!(hover.unwrap().contents.contains("element"));
    }

    #[test]
    fn hover_keyword() {
        let p = SlintSyntaxProvider::new();
        let hover = p.hover("component Foo { }", 4, None);
        assert!(hover.is_some());
        assert!(hover.unwrap().contents.contains("keyword"));
    }

    #[test]
    fn hover_property() {
        let p = SlintSyntaxProvider::new();
        let hover = p.hover("width: 100px;", 2, None);
        assert!(hover.is_some());
        assert!(hover.unwrap().contents.contains("width"));
    }

    #[test]
    fn hover_unknown_returns_none() {
        let p = SlintSyntaxProvider::new();
        assert!(p.hover("myCustomThing", 5, None).is_none());
    }

    #[test]
    fn complete_with_schema_context() {
        use crate::foundation::object_model::types::EntityFieldDef;
        let p = SlintSyntaxProvider::new();
        let field: EntityFieldDef =
            serde_json::from_value(serde_json::json!({"id": "title", "type": "text"})).unwrap();
        let ctx = SchemaContext {
            object_type: "card".into(),
            fields: vec![field],
            signals: vec![],
        };
        let items = p.complete("titl", 4, Some(&ctx));
        assert!(items.iter().any(|i| i.label == "title"));
    }
}
