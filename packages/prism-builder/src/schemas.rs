//! Shared component schemas — single source of truth for field
//! definitions used by both the live render path and the relay's
//! semantic-HTML SSR walker.
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
pub(crate) struct TextProps {
    #[field(label = "Body", multiline)]
    pub(crate) body: String,
    #[field(
        label = "Level",
        select("paragraph", "h1", "h2", "h3", "h4", "h5", "h6"),
        default = "paragraph"
    )]
    pub(crate) level: String,
    #[field(label = "Link URL")]
    pub(crate) href: String,
}

pub fn text() -> Vec<FieldSpec> {
    TextProps::field_specs()
}

#[derive(PrismField)]
pub(crate) struct CardProps {
    #[field(label = "Card title", required)]
    pub(crate) title: String,
    #[field(label = "Card body", multiline)]
    pub(crate) body: String,
}

pub fn card() -> Vec<FieldSpec> {
    CardProps::field_specs()
}

#[derive(PrismField)]
pub(crate) struct ImageProps {
    #[field(label = "Image source", kind = "file", accept = "image/*", required)]
    pub(crate) src: String,
    #[field(label = "Alt text")]
    pub(crate) alt: String,
    #[field(
        label = "Object fit",
        select("cover", "contain", "fill", "none"),
        default = "cover"
    )]
    pub(crate) fit: String,
    #[field(label = "Link URL")]
    pub(crate) href: String,
    #[field(label = "Border radius (px)", default = 0, min = 0.0, max = 64.0)]
    pub(crate) border_radius: i64,
}

pub fn image() -> Vec<FieldSpec> {
    ImageProps::field_specs()
}

#[derive(PrismField)]
pub(crate) struct ContainerProps {
    #[field(label = "Child spacing (px)", default = 12, min = 0.0, max = 64.0)]
    pub(crate) spacing: i64,
    #[field(label = "Padding (px)", default = 0, min = 0.0, max = 64.0)]
    pub(crate) padding: i64,
    #[field(label = "Border width (px)", default = 0, min = 0.0, max = 8.0)]
    pub(crate) border_width: i64,
    #[field(label = "Border color", default = "#3b4252")]
    pub(crate) border_color: String,
}

pub fn container() -> Vec<FieldSpec> {
    ContainerProps::field_specs()
}

#[derive(PrismField)]
pub(crate) struct FormProps {
    #[field(label = "Form action URL")]
    pub(crate) action: String,
    #[field(label = "HTTP method", select("post", "get"), default = "post")]
    pub(crate) method: String,
}

pub fn form() -> Vec<FieldSpec> {
    FormProps::field_specs()
}

#[derive(PrismField)]
pub(crate) struct InputProps {
    #[field(label = "Field name", required)]
    pub(crate) name: String,
    #[field(
        label = "Input type",
        select("text", "email", "password", "number", "hidden"),
        default = "text"
    )]
    pub(crate) r#type: String,
    #[field(label = "Placeholder")]
    pub(crate) placeholder: String,
    #[field(label = "Default value")]
    pub(crate) value: String,
    #[field(label = "Required")]
    pub(crate) required: bool,
    #[field(label = "Label text")]
    pub(crate) label: String,
}

pub fn input() -> Vec<FieldSpec> {
    InputProps::field_specs()
}

#[derive(PrismField)]
pub(crate) struct ButtonProps {
    #[field(label = "Button label", required, default = "Submit")]
    pub(crate) text: String,
    #[field(
        label = "Button type",
        select("submit", "button", "reset"),
        default = "submit"
    )]
    pub(crate) r#type: String,
    #[field(label = "Disabled")]
    pub(crate) disabled: bool,
    #[field(label = "Link URL")]
    pub(crate) href: String,
}

pub fn button() -> Vec<FieldSpec> {
    ButtonProps::field_specs()
}

#[derive(PrismField)]
pub(crate) struct CodeProps {
    #[field(label = "Code", multiline, required)]
    pub(crate) code: String,
    #[field(label = "Language")]
    pub(crate) language: String,
    /// Empty defers to the style cascade or to a dark-theme default
    /// (html SSR). Set explicitly to override either path.
    #[field(label = "Background color", kind = "color")]
    pub(crate) bg: String,
    #[field(label = "Text color", kind = "color")]
    pub(crate) color: String,
}

pub fn code() -> Vec<FieldSpec> {
    CodeProps::field_specs()
}

pub fn divider() -> Vec<FieldSpec> {
    vec![]
}

#[derive(PrismField)]
pub(crate) struct SpacerProps {
    #[field(label = "Height (px)", default = 24, min = 4.0, max = 128.0)]
    pub(crate) height: i64,
}

pub fn spacer() -> Vec<FieldSpec> {
    SpacerProps::field_specs()
}

#[derive(PrismField)]
pub(crate) struct ColumnsProps {
    #[field(label = "Column gap (px)", default = 16, min = 0.0, max = 64.0)]
    pub(crate) gap: i64,
}

pub fn columns() -> Vec<FieldSpec> {
    ColumnsProps::field_specs()
}

#[derive(PrismField)]
pub(crate) struct ListProps {
    #[field(label = "Ordered (numbered)")]
    pub(crate) ordered: bool,
    #[field(label = "Item spacing (px)", default = 4, min = 0.0, max = 32.0)]
    pub(crate) item_spacing: i64,
}

pub fn list() -> Vec<FieldSpec> {
    ListProps::field_specs()
}

#[derive(PrismField)]
pub(crate) struct TableProps {
    #[field(label = "Column headers (comma-separated)", required)]
    pub(crate) headers: String,
    #[field(label = "Table caption")]
    pub(crate) caption: String,
}

pub fn table() -> Vec<FieldSpec> {
    TableProps::field_specs()
}

#[derive(PrismField)]
pub(crate) struct TabsProps {
    #[field(label = "Tab labels (comma-separated)", required)]
    pub(crate) labels: String,
}

pub fn tabs() -> Vec<FieldSpec> {
    TabsProps::field_specs()
}

#[derive(PrismField)]
pub(crate) struct AccordionProps {
    #[field(label = "Section title", required)]
    pub(crate) title: String,
    #[field(label = "Initially open")]
    pub(crate) open: bool,
    #[field(label = "Border width (px)", default = 0, min = 0.0, max = 8.0)]
    pub(crate) border_width: i64,
    #[field(label = "Border color", default = "#3b4252", kind = "color")]
    pub(crate) border_color: String,
    #[field(label = "Section gap (px)", default = 4, min = 0.0, max = 32.0)]
    pub(crate) section_gap: i64,
}

pub fn accordion() -> Vec<FieldSpec> {
    AccordionProps::field_specs()
}

#[derive(PrismField)]
pub(crate) struct GraphViewProps {
    #[field(label = "Node label field", default = "label")]
    pub(crate) node_label_field: String,
    #[field(label = "Node color field")]
    pub(crate) node_color_field: String,
    #[field(label = "Show edge labels")]
    pub(crate) edge_label: bool,
    #[field(
        label = "Layout algorithm",
        select("force", "tree", "radial", "grid"),
        default = "force"
    )]
    pub(crate) layout: String,
    #[field(label = "Node size (px)", default = 48, min = 20.0, max = 120.0)]
    pub(crate) node_size: i64,
    #[field(label = "Show arrows", default = true)]
    pub(crate) show_arrows: bool,
}

pub fn graph_view() -> Vec<FieldSpec> {
    GraphViewProps::field_specs()
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
