//! Shared component schemas — single source of truth for field
//! definitions used by both the Slint and HTML render paths.
//!
//! `text()` is migrated to `#[derive(PrismField)]` as the first
//! consumer of the unified field-derive (declarative-refactorings.md
//! item #2 phase 1). The remaining schemas continue to use the
//! hand-rolled `FieldSpec` builders pending the derive's coverage of
//! `file` / `currency` / `calculation` kinds.

use serde_json::Value;

use crate::registry::{FieldSpec, NumericBounds, SelectOption};
use prism_luau_derive::PrismField;

#[derive(PrismField)]
#[allow(dead_code)]
struct TextProps {
    #[field(label = "Body", multiline)]
    body: String,
    #[field(
        label = "Level",
        select("paragraph", "h1", "h2", "h3", "h4", "h5", "h6"),
        default = "paragraph"
    )]
    level: String,
    #[field(label = "Link URL")]
    href: String,
}

pub fn text() -> Vec<FieldSpec> {
    TextProps::field_specs()
}

pub fn image() -> Vec<FieldSpec> {
    vec![
        FieldSpec::file("src", "Image source", vec!["image/*".into()]).required(),
        FieldSpec::text("alt", "Alt text"),
        FieldSpec::select(
            "fit",
            "Object fit",
            vec![
                SelectOption::new("cover", "Cover"),
                SelectOption::new("contain", "Contain"),
                SelectOption::new("fill", "Fill"),
                SelectOption::new("none", "None"),
            ],
        )
        .with_default(Value::from("cover")),
        FieldSpec::text("href", "Link URL"),
    ]
}

pub fn container() -> Vec<FieldSpec> {
    vec![
        FieldSpec::integer(
            "spacing",
            "Child spacing (px)",
            NumericBounds::min_max(0.0, 64.0),
        )
        .with_default(Value::from(12)),
        FieldSpec::integer("padding", "Padding (px)", NumericBounds::min_max(0.0, 64.0))
            .with_default(Value::from(0)),
        FieldSpec::integer(
            "border_width",
            "Border width (px)",
            NumericBounds::min_max(0.0, 8.0),
        )
        .with_default(Value::from(0)),
        FieldSpec::text("border_color", "Border color"),
    ]
}

pub fn form() -> Vec<FieldSpec> {
    vec![
        FieldSpec::text("action", "Form action URL"),
        FieldSpec::select(
            "method",
            "HTTP method",
            vec![
                SelectOption::new("post", "POST"),
                SelectOption::new("get", "GET"),
            ],
        )
        .with_default(Value::from("post")),
    ]
}

pub fn input() -> Vec<FieldSpec> {
    vec![
        FieldSpec::text("name", "Field name").required(),
        FieldSpec::select(
            "type",
            "Input type",
            vec![
                SelectOption::new("text", "Text"),
                SelectOption::new("email", "Email"),
                SelectOption::new("password", "Password"),
                SelectOption::new("number", "Number"),
                SelectOption::new("hidden", "Hidden"),
            ],
        )
        .with_default(Value::from("text")),
        FieldSpec::text("placeholder", "Placeholder"),
        FieldSpec::text("value", "Default value"),
        FieldSpec::boolean("required", "Required"),
        FieldSpec::text("label", "Label text"),
    ]
}

pub fn button() -> Vec<FieldSpec> {
    vec![
        FieldSpec::text("text", "Button label").required(),
        FieldSpec::select(
            "type",
            "Button type",
            vec![
                SelectOption::new("submit", "Submit"),
                SelectOption::new("button", "Button"),
                SelectOption::new("reset", "Reset"),
            ],
        )
        .with_default(Value::from("submit")),
        FieldSpec::boolean("disabled", "Disabled"),
        FieldSpec::text("href", "Link URL"),
    ]
}

pub fn code() -> Vec<FieldSpec> {
    vec![
        FieldSpec::textarea("code", "Code").required(),
        FieldSpec::text("language", "Language"),
    ]
}

pub fn divider() -> Vec<FieldSpec> {
    vec![]
}

pub fn spacer() -> Vec<FieldSpec> {
    vec![
        FieldSpec::integer("height", "Height (px)", NumericBounds::min_max(4.0, 128.0))
            .with_default(Value::from(24)),
    ]
}

pub fn columns() -> Vec<FieldSpec> {
    vec![
        FieldSpec::integer("gap", "Column gap (px)", NumericBounds::min_max(0.0, 64.0))
            .with_default(Value::from(16)),
    ]
}

pub fn list() -> Vec<FieldSpec> {
    vec![FieldSpec::boolean("ordered", "Ordered (numbered)")]
}

pub fn table() -> Vec<FieldSpec> {
    vec![
        FieldSpec::text("headers", "Column headers (comma-separated)").required(),
        FieldSpec::text("caption", "Table caption"),
    ]
}

pub fn tabs() -> Vec<FieldSpec> {
    vec![FieldSpec::text("labels", "Tab labels (comma-separated)").required()]
}

pub fn accordion() -> Vec<FieldSpec> {
    vec![
        FieldSpec::text("title", "Section title").required(),
        FieldSpec::boolean("open", "Initially open"),
    ]
}

pub fn facet() -> Vec<FieldSpec> {
    vec![
        FieldSpec::text("facet_id", "Facet ID").required(),
        FieldSpec::integer(
            "max_items",
            "Max items",
            NumericBounds::min_max(1.0, 10_000.0),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::FieldKind;

    #[test]
    fn derived_text_schema_matches_legacy_shape() {
        let schema = text();
        assert_eq!(schema.len(), 3);

        assert_eq!(schema[0].key, "body");
        assert_eq!(schema[0].label, "Body");
        assert!(matches!(schema[0].kind, FieldKind::TextArea));

        assert_eq!(schema[1].key, "level");
        assert_eq!(schema[1].label, "Level");
        match &schema[1].kind {
            FieldKind::Select(opts) => {
                let values: Vec<&str> = opts.iter().map(|o| o.value.as_str()).collect();
                assert_eq!(
                    values,
                    vec!["paragraph", "h1", "h2", "h3", "h4", "h5", "h6"]
                );
            }
            other => panic!("expected Select, got {other:?}"),
        }
        assert_eq!(schema[1].default, Value::String("paragraph".into()));

        assert_eq!(schema[2].key, "href");
        assert_eq!(schema[2].label, "Link URL");
        assert!(matches!(schema[2].kind, FieldKind::Text));
    }
}
