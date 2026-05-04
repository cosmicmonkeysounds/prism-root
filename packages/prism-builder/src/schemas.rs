//! Shared component schemas — single source of truth for field
//! definitions used by both the Slint and HTML render paths.
//!
//! Every schema is a `#[derive(PrismField)]` struct with a thin
//! free-function wrapper that returns `Vec<FieldSpec>`. The legacy
//! `FieldSpec::*` builder API is still available for callers that
//! need it (custom widgets, plugin contributions); these built-in
//! schemas just go through the derive so the prop struct is the
//! single source of truth.

#![allow(dead_code)] // structs are used only via their derived ::field_specs()

use crate::registry::FieldSpec;
use prism_luau_derive::PrismField;

#[derive(PrismField)]
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

#[derive(PrismField)]
struct ImageProps {
    #[field(label = "Image source", kind = "file", accept = "image/*", required)]
    src: String,
    #[field(label = "Alt text")]
    alt: String,
    #[field(
        label = "Object fit",
        select("cover", "contain", "fill", "none"),
        default = "cover"
    )]
    fit: String,
    #[field(label = "Link URL")]
    href: String,
}

pub fn image() -> Vec<FieldSpec> {
    ImageProps::field_specs()
}

#[derive(PrismField)]
struct ContainerProps {
    #[field(label = "Child spacing (px)", default = 12, min = 0.0, max = 64.0)]
    spacing: i64,
    #[field(label = "Padding (px)", default = 0, min = 0.0, max = 64.0)]
    padding: i64,
    #[field(label = "Border width (px)", default = 0, min = 0.0, max = 8.0)]
    border_width: i64,
    #[field(label = "Border color")]
    border_color: String,
}

pub fn container() -> Vec<FieldSpec> {
    ContainerProps::field_specs()
}

#[derive(PrismField)]
struct FormProps {
    #[field(label = "Form action URL")]
    action: String,
    #[field(label = "HTTP method", select("post", "get"), default = "post")]
    method: String,
}

pub fn form() -> Vec<FieldSpec> {
    FormProps::field_specs()
}

#[derive(PrismField)]
struct InputProps {
    #[field(label = "Field name", required)]
    name: String,
    #[field(
        label = "Input type",
        select("text", "email", "password", "number", "hidden"),
        default = "text"
    )]
    r#type: String,
    #[field(label = "Placeholder")]
    placeholder: String,
    #[field(label = "Default value")]
    value: String,
    #[field(label = "Required")]
    required: bool,
    #[field(label = "Label text")]
    label: String,
}

pub fn input() -> Vec<FieldSpec> {
    InputProps::field_specs()
}

#[derive(PrismField)]
struct ButtonProps {
    #[field(label = "Button label", required)]
    text: String,
    #[field(
        label = "Button type",
        select("submit", "button", "reset"),
        default = "submit"
    )]
    r#type: String,
    #[field(label = "Disabled")]
    disabled: bool,
    #[field(label = "Link URL")]
    href: String,
}

pub fn button() -> Vec<FieldSpec> {
    ButtonProps::field_specs()
}

#[derive(PrismField)]
struct CodeProps {
    #[field(label = "Code", multiline, required)]
    code: String,
    #[field(label = "Language")]
    language: String,
}

pub fn code() -> Vec<FieldSpec> {
    CodeProps::field_specs()
}

pub fn divider() -> Vec<FieldSpec> {
    vec![]
}

#[derive(PrismField)]
struct SpacerProps {
    #[field(label = "Height (px)", default = 24, min = 4.0, max = 128.0)]
    height: i64,
}

pub fn spacer() -> Vec<FieldSpec> {
    SpacerProps::field_specs()
}

#[derive(PrismField)]
struct ColumnsProps {
    #[field(label = "Column gap (px)", default = 16, min = 0.0, max = 64.0)]
    gap: i64,
}

pub fn columns() -> Vec<FieldSpec> {
    ColumnsProps::field_specs()
}

#[derive(PrismField)]
struct ListProps {
    #[field(label = "Ordered (numbered)")]
    ordered: bool,
}

pub fn list() -> Vec<FieldSpec> {
    ListProps::field_specs()
}

#[derive(PrismField)]
struct TableProps {
    #[field(label = "Column headers (comma-separated)", required)]
    headers: String,
    #[field(label = "Table caption")]
    caption: String,
}

pub fn table() -> Vec<FieldSpec> {
    TableProps::field_specs()
}

#[derive(PrismField)]
struct TabsProps {
    #[field(label = "Tab labels (comma-separated)", required)]
    labels: String,
}

pub fn tabs() -> Vec<FieldSpec> {
    TabsProps::field_specs()
}

#[derive(PrismField)]
struct AccordionProps {
    #[field(label = "Section title", required)]
    title: String,
    #[field(label = "Initially open")]
    open: bool,
}

pub fn accordion() -> Vec<FieldSpec> {
    AccordionProps::field_specs()
}

#[derive(PrismField)]
struct FacetProps {
    #[field(label = "Facet ID", required)]
    facet_id: String,
    #[field(label = "Max items", min = 1.0, max = 10_000.0)]
    max_items: i64,
}

pub fn facet() -> Vec<FieldSpec> {
    FacetProps::field_specs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::FieldKind;
    use serde_json::Value;

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

    #[test]
    fn derived_image_schema_uses_file_kind_with_accept() {
        let schema = image();
        assert_eq!(schema[0].key, "src");
        match &schema[0].kind {
            FieldKind::File(cfg) => assert_eq!(cfg.accept, vec!["image/*".to_string()]),
            other => panic!("expected File, got {other:?}"),
        }
    }

    #[test]
    fn derived_container_schema_keeps_default_values() {
        let schema = container();
        assert_eq!(schema[0].key, "spacing");
        assert_eq!(schema[0].default, Value::from(12i64));
        assert_eq!(schema[3].key, "border_color");
    }
}
