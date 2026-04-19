//! Shared component schemas — single source of truth for field
//! definitions used by both the Slint and HTML render paths.

use serde_json::Value;

use crate::registry::{FieldSpec, NumericBounds, SelectOption};

pub fn heading() -> Vec<FieldSpec> {
    vec![
        FieldSpec::text("text", "Text").required(),
        FieldSpec::integer("level", "Heading level", NumericBounds::min_max(1.0, 6.0))
            .with_default(Value::from(1)),
    ]
}

pub fn text() -> Vec<FieldSpec> {
    vec![FieldSpec::textarea("body", "Body")]
}

pub fn link() -> Vec<FieldSpec> {
    vec![
        FieldSpec::text("href", "URL").required(),
        FieldSpec::text("text", "Label"),
    ]
}

pub fn image() -> Vec<FieldSpec> {
    vec![
        FieldSpec::text("src", "Image source").required(),
        FieldSpec::text("alt", "Alt text"),
    ]
}

pub fn container() -> Vec<FieldSpec> {
    vec![FieldSpec::integer(
        "spacing",
        "Child spacing (px)",
        NumericBounds::min_max(0.0, 64.0),
    )
    .with_default(Value::from(12))]
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
    ]
}

pub fn card() -> Vec<FieldSpec> {
    vec![
        FieldSpec::text("title", "Card title").required(),
        FieldSpec::textarea("body", "Card body"),
        FieldSpec::select(
            "variant",
            "Style variant",
            vec![
                SelectOption::new("default", "Default"),
                SelectOption::new("outlined", "Outlined"),
            ],
        )
        .with_default(Value::from("default")),
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
    vec![FieldSpec::integer(
        "height",
        "Height (px)",
        NumericBounds::min_max(4.0, 128.0),
    )
    .with_default(Value::from(24))]
}

pub fn columns() -> Vec<FieldSpec> {
    vec![FieldSpec::integer(
        "gap",
        "Column gap (px)",
        NumericBounds::min_max(0.0, 64.0),
    )
    .with_default(Value::from(16))]
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
