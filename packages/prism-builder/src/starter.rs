//! Starter component catalog — the default Slint-side registry.
//!
//! Seventeen blocks land here: `heading`, `text`, `link`, `image`,
//! `container`, `form`, `input`, `button`, `card`, `code`, `divider`,
//! `spacer`, `columns`, `list`, `table`, `tabs`, and `accordion`.
//! Each implements [`Component`] with a `render_slint` method that
//! emits `.slint` DSL via [`SlintEmitter`] for Studio's live builder
//! panel.
//!
//! HTML SSR is handled separately by [`crate::html_starter`] via the
//! [`crate::html_block::HtmlBlock`] trait.

use std::sync::Arc;

use prism_core::help::HelpEntry;
use serde_json::Value;

use crate::component::{Component, ComponentId, RenderError, RenderSlintContext};
use crate::document::Node;
use crate::registry::{ComponentRegistry, FieldSpec, NumericBounds, RegistryError};
use crate::slint_source::SlintEmitter;

/// Register the starter catalog into `reg`. Call this once at boot
/// to get a registry with seventeen ready-to-render components.
pub fn register_builtins(reg: &mut ComponentRegistry) -> Result<(), RegistryError> {
    reg.register(Arc::new(HeadingComponent {
        id: "heading".into(),
    }))?;
    reg.register(Arc::new(TextComponent { id: "text".into() }))?;
    reg.register(Arc::new(LinkComponent { id: "link".into() }))?;
    reg.register(Arc::new(ImageComponent { id: "image".into() }))?;
    reg.register(Arc::new(ContainerComponent {
        id: "container".into(),
    }))?;
    reg.register(Arc::new(FormComponent { id: "form".into() }))?;
    reg.register(Arc::new(InputComponent { id: "input".into() }))?;
    reg.register(Arc::new(ButtonComponent {
        id: "button".into(),
    }))?;
    reg.register(Arc::new(CardComponent { id: "card".into() }))?;
    reg.register(Arc::new(CodeComponent { id: "code".into() }))?;
    reg.register(Arc::new(DividerComponent {
        id: "divider".into(),
    }))?;
    reg.register(Arc::new(SpacerComponent {
        id: "spacer".into(),
    }))?;
    reg.register(Arc::new(ColumnsComponent {
        id: "columns".into(),
    }))?;
    reg.register(Arc::new(ListComponent { id: "list".into() }))?;
    reg.register(Arc::new(TableComponent { id: "table".into() }))?;
    reg.register(Arc::new(TabsComponent { id: "tabs".into() }))?;
    reg.register(Arc::new(AccordionComponent {
        id: "accordion".into(),
    }))?;
    Ok(())
}

fn heading_font_size(level: u64) -> f64 {
    match level {
        1 => 32.0,
        2 => 26.0,
        3 => 22.0,
        4 => 18.0,
        5 => 16.0,
        _ => 14.0,
    }
}

/// `h1`–`h6` depending on `props.level` (clamped to 1..=6). Reads
/// its label from `props.text` and escapes it.
pub struct HeadingComponent {
    pub id: ComponentId,
}

impl Component for HeadingComponent {
    fn id(&self) -> &ComponentId {
        &self.id
    }
    fn schema(&self) -> Vec<FieldSpec> {
        vec![
            FieldSpec::text("text", "Text").required(),
            FieldSpec::integer("level", "Heading level", NumericBounds::min_max(1.0, 6.0))
                .with_default(Value::from(1)),
        ]
    }
    fn help_entry(&self) -> Option<HelpEntry> {
        Some(HelpEntry::new(
            "builder.components.heading",
            "Heading",
            "Section heading (h1\u{2013}h6) with inline editing. Set the level to control size and document outline.",
        ))
    }
    fn render_slint(
        &self,
        _ctx: &RenderSlintContext<'_>,
        props: &Value,
        _children: &[Node],
        out: &mut SlintEmitter,
    ) -> Result<(), RenderError> {
        let level = props
            .get("level")
            .and_then(|v| v.as_u64())
            .unwrap_or(1)
            .clamp(1, 6);
        let text = props.get("text").and_then(|v| v.as_str()).unwrap_or("");
        out.block("Text", |out| {
            out.prop_string("text", text);
            out.prop_px("font-size", heading_font_size(level));
            out.line("font-weight: 700;");
            Ok(())
        })
    }
}

/// Paragraph. Reads `props.body` as the text content.
pub struct TextComponent {
    pub id: ComponentId,
}

impl Component for TextComponent {
    fn id(&self) -> &ComponentId {
        &self.id
    }
    fn schema(&self) -> Vec<FieldSpec> {
        vec![FieldSpec::textarea("body", "Body")]
    }
    fn help_entry(&self) -> Option<HelpEntry> {
        Some(HelpEntry::new(
            "builder.components.text",
            "Text",
            "Paragraph of body text. Supports inline editing when selected in the builder canvas.",
        ))
    }
    fn render_slint(
        &self,
        _ctx: &RenderSlintContext<'_>,
        props: &Value,
        _children: &[Node],
        out: &mut SlintEmitter,
    ) -> Result<(), RenderError> {
        let body = props.get("body").and_then(|v| v.as_str()).unwrap_or("");
        out.block("Text", |out| {
            out.prop_string("text", body);
            out.prop_px("font-size", 14.0);
            out.line("wrap: word-wrap;");
            Ok(())
        })
    }
}

/// Anchor tag. Emits `<a href="…">text</a>`; children (if any) are
/// rendered inside the anchor in addition to the `text` prop.
pub struct LinkComponent {
    pub id: ComponentId,
}

impl Component for LinkComponent {
    fn id(&self) -> &ComponentId {
        &self.id
    }
    fn schema(&self) -> Vec<FieldSpec> {
        vec![
            FieldSpec::text("href", "URL").required(),
            FieldSpec::text("text", "Label"),
        ]
    }
    fn help_entry(&self) -> Option<HelpEntry> {
        Some(HelpEntry::new(
            "builder.components.link",
            "Link",
            "Anchor hyperlink with text label and URL. Opens in a new tab by default.",
        ))
    }
    fn render_slint(
        &self,
        _ctx: &RenderSlintContext<'_>,
        props: &Value,
        _children: &[Node],
        out: &mut SlintEmitter,
    ) -> Result<(), RenderError> {
        // Slint has no built-in anchor element; show the label as
        // underlined text the way the Studio previews external links.
        let text = props
            .get("text")
            .and_then(|v| v.as_str())
            .or_else(|| props.get("href").and_then(|v| v.as_str()))
            .unwrap_or("");
        out.block("Text", |out| {
            out.prop_string("text", text);
            out.prop_px("font-size", 14.0);
            out.line("color: #5aa0ff;");
            Ok(())
        })
    }
}

/// Void `img` element. Reads `props.src` and `props.alt`.
pub struct ImageComponent {
    pub id: ComponentId,
}

impl Component for ImageComponent {
    fn id(&self) -> &ComponentId {
        &self.id
    }
    fn schema(&self) -> Vec<FieldSpec> {
        vec![
            FieldSpec::text("src", "Image source").required(),
            FieldSpec::text("alt", "Alt text"),
        ]
    }
    fn help_entry(&self) -> Option<HelpEntry> {
        Some(HelpEntry::new(
            "builder.components.image",
            "Image",
            "Embedded image with alt text, configurable fit and aspect ratio.",
        ))
    }
    fn render_slint(
        &self,
        _ctx: &RenderSlintContext<'_>,
        props: &Value,
        _children: &[Node],
        out: &mut SlintEmitter,
    ) -> Result<(), RenderError> {
        let alt = props.get("alt").and_then(|v| v.as_str()).unwrap_or("image");
        out.block("Rectangle", |out| {
            out.prop_px("min-height", 120.0);
            out.line("background: #2a3140;");
            out.line("border-radius: 6px;");
            out.block("Text", |out| {
                out.prop_string("text", alt);
                out.prop_px("font-size", 12.0);
                out.line("color: #9ca4b4;");
                out.line("horizontal-alignment: center;");
                out.line("vertical-alignment: center;");
                Ok(())
            })
        })
    }
}

/// Semantic `<section>` wrapper with children rendered inside.
/// Useful as a layout block in the portal body.
pub struct ContainerComponent {
    pub id: ComponentId,
}

impl Component for ContainerComponent {
    fn id(&self) -> &ComponentId {
        &self.id
    }
    fn schema(&self) -> Vec<FieldSpec> {
        vec![FieldSpec::integer(
            "spacing",
            "Child spacing (px)",
            NumericBounds::min_max(0.0, 64.0),
        )
        .with_default(Value::from(12))]
    }
    fn help_entry(&self) -> Option<HelpEntry> {
        Some(HelpEntry::new(
            "builder.components.container",
            "Container",
            "Layout wrapper that groups child components with configurable spacing.",
        ))
    }
    fn render_slint(
        &self,
        ctx: &RenderSlintContext<'_>,
        props: &Value,
        children: &[Node],
        out: &mut SlintEmitter,
    ) -> Result<(), RenderError> {
        let spacing = props
            .get("spacing")
            .and_then(|v| v.as_f64())
            .unwrap_or(12.0);
        out.block("VerticalLayout", |out| {
            out.prop_px("spacing", spacing);
            out.line("alignment: start;");
            ctx.render_children(children, out)
        })
    }
}

/// HTML `<form>` wrapper. Renders children inside a `<form method="post">`.
/// L3 portals use this for interactive submissions.
pub struct FormComponent {
    pub id: ComponentId,
}

impl Component for FormComponent {
    fn id(&self) -> &ComponentId {
        &self.id
    }
    fn schema(&self) -> Vec<FieldSpec> {
        vec![
            FieldSpec::text("action", "Form action URL"),
            FieldSpec::select(
                "method",
                "HTTP method",
                vec![
                    crate::registry::SelectOption::new("post", "POST"),
                    crate::registry::SelectOption::new("get", "GET"),
                ],
            )
            .with_default(Value::from("post")),
        ]
    }
    fn help_entry(&self) -> Option<HelpEntry> {
        Some(HelpEntry::new(
            "builder.components.form",
            "Form",
            "HTML form wrapper. Nest input and button components inside to build forms.",
        ))
    }
    fn render_slint(
        &self,
        ctx: &RenderSlintContext<'_>,
        props: &Value,
        children: &[Node],
        out: &mut SlintEmitter,
    ) -> Result<(), RenderError> {
        let _ = props;
        out.block("VerticalLayout", |out| {
            out.prop_px("spacing", 8.0);
            out.line("alignment: start;");
            ctx.render_children(children, out)
        })
    }
}

/// HTML `<input>`. Renders as a void element with name, type, and placeholder.
pub struct InputComponent {
    pub id: ComponentId,
}

impl Component for InputComponent {
    fn id(&self) -> &ComponentId {
        &self.id
    }
    fn schema(&self) -> Vec<FieldSpec> {
        vec![
            FieldSpec::text("name", "Field name").required(),
            FieldSpec::select(
                "type",
                "Input type",
                vec![
                    crate::registry::SelectOption::new("text", "Text"),
                    crate::registry::SelectOption::new("email", "Email"),
                    crate::registry::SelectOption::new("password", "Password"),
                    crate::registry::SelectOption::new("number", "Number"),
                    crate::registry::SelectOption::new("hidden", "Hidden"),
                ],
            )
            .with_default(Value::from("text")),
            FieldSpec::text("placeholder", "Placeholder"),
            FieldSpec::text("value", "Default value"),
            FieldSpec::boolean("required", "Required"),
            FieldSpec::text("label", "Label text"),
        ]
    }
    fn help_entry(&self) -> Option<HelpEntry> {
        Some(HelpEntry::new(
            "builder.components.input",
            "Input",
            "Text, email, or password field with placeholder and name binding.",
        ))
    }
    fn render_slint(
        &self,
        _ctx: &RenderSlintContext<'_>,
        props: &Value,
        _children: &[Node],
        out: &mut SlintEmitter,
    ) -> Result<(), RenderError> {
        let label = props.get("label").and_then(|v| v.as_str()).unwrap_or("");
        let placeholder = props
            .get("placeholder")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        out.block("VerticalLayout", |out| {
            out.prop_px("spacing", 4.0);
            if !label.is_empty() {
                out.block("Text", |out| {
                    out.prop_string("text", label);
                    out.prop_px("font-size", 12.0);
                    Ok(())
                })?;
            }
            out.block("Rectangle", |out| {
                out.prop_px("height", 32.0);
                out.line("background: #1e2533;");
                out.line("border-radius: 4px;");
                out.block("Text", |out| {
                    let display = if placeholder.is_empty() {
                        "..."
                    } else {
                        placeholder
                    };
                    out.prop_string("text", display);
                    out.prop_px("font-size", 14.0);
                    out.line("color: #6b7280;");
                    out.line("vertical-alignment: center;");
                    Ok(())
                })
            })
        })
    }
}

/// Card with title, body, and optional children. Renders as a bordered
/// rectangle in Slint, `<article>` in HTML.
pub struct CardComponent {
    pub id: ComponentId,
}

impl Component for CardComponent {
    fn id(&self) -> &ComponentId {
        &self.id
    }
    fn schema(&self) -> Vec<FieldSpec> {
        vec![
            FieldSpec::text("title", "Card title").required(),
            FieldSpec::textarea("body", "Card body"),
            FieldSpec::select(
                "variant",
                "Style variant",
                vec![
                    crate::registry::SelectOption::new("default", "Default"),
                    crate::registry::SelectOption::new("outlined", "Outlined"),
                ],
            )
            .with_default(Value::from("default")),
        ]
    }
    fn help_entry(&self) -> Option<HelpEntry> {
        Some(HelpEntry::new(
            "builder.components.card",
            "Card",
            "Bordered content card with title and body text slots.",
        ))
    }
    fn render_slint(
        &self,
        ctx: &RenderSlintContext<'_>,
        props: &Value,
        children: &[Node],
        out: &mut SlintEmitter,
    ) -> Result<(), RenderError> {
        let title = props.get("title").and_then(|v| v.as_str()).unwrap_or("");
        let body = props.get("body").and_then(|v| v.as_str()).unwrap_or("");
        out.block("Rectangle", |out| {
            out.line("border-width: 1px;");
            out.line("border-color: #3b4252;");
            out.line("border-radius: 8px;");
            out.line("background: #2e3440;");
            out.block("VerticalLayout", |out| {
                out.prop_px("padding", 16.0);
                out.prop_px("spacing", 8.0);
                out.block("Text", |out| {
                    out.prop_string("text", title);
                    out.prop_px("font-size", 18.0);
                    out.line("font-weight: 600;");
                    Ok(())
                })?;
                if !body.is_empty() {
                    out.block("Text", |out| {
                        out.prop_string("text", body);
                        out.prop_px("font-size", 14.0);
                        out.line("color: #9ca4b4;");
                        out.line("wrap: word-wrap;");
                        Ok(())
                    })?;
                }
                ctx.render_children(children, out)
            })
        })
    }
}

/// Preformatted code block with monospace font.
pub struct CodeComponent {
    pub id: ComponentId,
}

impl Component for CodeComponent {
    fn id(&self) -> &ComponentId {
        &self.id
    }
    fn schema(&self) -> Vec<FieldSpec> {
        vec![
            FieldSpec::textarea("code", "Code").required(),
            FieldSpec::text("language", "Language"),
        ]
    }
    fn help_entry(&self) -> Option<HelpEntry> {
        Some(HelpEntry::new(
            "builder.components.code",
            "Code",
            "Preformatted code block with optional language label.",
        ))
    }
    fn render_slint(
        &self,
        _ctx: &RenderSlintContext<'_>,
        props: &Value,
        _children: &[Node],
        out: &mut SlintEmitter,
    ) -> Result<(), RenderError> {
        let code = props.get("code").and_then(|v| v.as_str()).unwrap_or("");
        out.block("Rectangle", |out| {
            out.line("background: #1a1e28;");
            out.line("border-radius: 6px;");
            out.block("VerticalLayout", |out| {
                out.prop_px("padding", 12.0);
                out.block("Text", |out| {
                    out.prop_string("text", code);
                    out.prop_px("font-size", 13.0);
                    out.line("color: #a3be8c;");
                    out.line("font-family: \"monospace\";");
                    out.line("wrap: word-wrap;");
                    Ok(())
                })
            })
        })
    }
}

/// Horizontal rule / visual separator.
pub struct DividerComponent {
    pub id: ComponentId,
}

impl Component for DividerComponent {
    fn id(&self) -> &ComponentId {
        &self.id
    }
    fn schema(&self) -> Vec<FieldSpec> {
        vec![]
    }
    fn help_entry(&self) -> Option<HelpEntry> {
        Some(HelpEntry::new(
            "builder.components.divider",
            "Divider",
            "Horizontal separator line between content sections.",
        ))
    }
    fn render_slint(
        &self,
        _ctx: &RenderSlintContext<'_>,
        _props: &Value,
        _children: &[Node],
        out: &mut SlintEmitter,
    ) -> Result<(), RenderError> {
        out.block("Rectangle", |out| {
            out.prop_px("height", 1.0);
            out.line("background: #3b4252;");
            Ok(())
        })
    }
}

/// Empty vertical spacer with configurable height.
pub struct SpacerComponent {
    pub id: ComponentId,
}

impl Component for SpacerComponent {
    fn id(&self) -> &ComponentId {
        &self.id
    }
    fn schema(&self) -> Vec<FieldSpec> {
        vec![
            FieldSpec::integer("height", "Height (px)", NumericBounds::min_max(4.0, 128.0))
                .with_default(Value::from(24)),
        ]
    }
    fn help_entry(&self) -> Option<HelpEntry> {
        Some(HelpEntry::new(
            "builder.components.spacer",
            "Spacer",
            "Vertical spacing element with configurable height in pixels.",
        ))
    }
    fn render_slint(
        &self,
        _ctx: &RenderSlintContext<'_>,
        props: &Value,
        _children: &[Node],
        out: &mut SlintEmitter,
    ) -> Result<(), RenderError> {
        let height = props.get("height").and_then(|v| v.as_f64()).unwrap_or(24.0);
        out.block("Rectangle", |out| {
            out.prop_px("height", height);
            Ok(())
        })
    }
}

/// Multi-column horizontal layout. Children are placed side-by-side.
pub struct ColumnsComponent {
    pub id: ComponentId,
}

impl Component for ColumnsComponent {
    fn id(&self) -> &ComponentId {
        &self.id
    }
    fn schema(&self) -> Vec<FieldSpec> {
        vec![
            FieldSpec::integer("gap", "Column gap (px)", NumericBounds::min_max(0.0, 64.0))
                .with_default(Value::from(16)),
        ]
    }
    fn help_entry(&self) -> Option<HelpEntry> {
        Some(HelpEntry::new(
            "builder.components.columns",
            "Columns",
            "Side-by-side horizontal layout with configurable gap between children.",
        ))
    }
    fn render_slint(
        &self,
        ctx: &RenderSlintContext<'_>,
        props: &Value,
        children: &[Node],
        out: &mut SlintEmitter,
    ) -> Result<(), RenderError> {
        let gap = props.get("gap").and_then(|v| v.as_f64()).unwrap_or(16.0);
        out.block("HorizontalLayout", |out| {
            out.prop_px("spacing", gap);
            ctx.render_children(children, out)
        })
    }
}

/// Ordered or unordered list wrapper. Each child becomes a list item.
pub struct ListComponent {
    pub id: ComponentId,
}

impl Component for ListComponent {
    fn id(&self) -> &ComponentId {
        &self.id
    }
    fn schema(&self) -> Vec<FieldSpec> {
        vec![FieldSpec::boolean("ordered", "Ordered (numbered)")]
    }
    fn help_entry(&self) -> Option<HelpEntry> {
        Some(HelpEntry::new(
            "builder.components.list",
            "List",
            "Ordered or unordered list. Toggle the ordered property to switch between numbered and bulleted styles.",
        ))
    }
    fn render_slint(
        &self,
        ctx: &RenderSlintContext<'_>,
        _props: &Value,
        children: &[Node],
        out: &mut SlintEmitter,
    ) -> Result<(), RenderError> {
        out.block("VerticalLayout", |out| {
            out.prop_px("spacing", 4.0);
            out.line("alignment: start;");
            ctx.render_children(children, out)
        })
    }
}

/// Simple data table with header columns and optional caption.
pub struct TableComponent {
    pub id: ComponentId,
}

impl Component for TableComponent {
    fn id(&self) -> &ComponentId {
        &self.id
    }
    fn schema(&self) -> Vec<FieldSpec> {
        vec![
            FieldSpec::text("headers", "Column headers (comma-separated)").required(),
            FieldSpec::text("caption", "Table caption"),
        ]
    }
    fn help_entry(&self) -> Option<HelpEntry> {
        Some(HelpEntry::new(
            "builder.components.table",
            "Table",
            "Data table with comma-separated column headers and optional caption.",
        ))
    }
    fn render_slint(
        &self,
        _ctx: &RenderSlintContext<'_>,
        props: &Value,
        _children: &[Node],
        out: &mut SlintEmitter,
    ) -> Result<(), RenderError> {
        let headers = props.get("headers").and_then(|v| v.as_str()).unwrap_or("");
        let caption = props.get("caption").and_then(|v| v.as_str()).unwrap_or("");
        out.block("Rectangle", |out| {
            out.line("border-width: 1px;");
            out.line("border-color: #3b4252;");
            out.line("border-radius: 4px;");
            out.block("VerticalLayout", |out| {
                out.prop_px("padding", 8.0);
                out.prop_px("spacing", 4.0);
                if !caption.is_empty() {
                    out.block("Text", |out| {
                        out.prop_string("text", caption);
                        out.prop_px("font-size", 12.0);
                        out.line("color: #9ca4b4;");
                        Ok(())
                    })?;
                }
                out.block("HorizontalLayout", |out| {
                    out.prop_px("spacing", 16.0);
                    for col in headers.split(',') {
                        let col = col.trim();
                        if !col.is_empty() {
                            out.block("Text", |out| {
                                out.prop_string("text", col);
                                out.prop_px("font-size", 13.0);
                                out.line("font-weight: 600;");
                                Ok(())
                            })?;
                        }
                    }
                    Ok(())
                })
            })
        })
    }
}

/// Tabbed content container. Children map to tab panels; the `labels`
/// prop names each panel.
pub struct TabsComponent {
    pub id: ComponentId,
}

impl Component for TabsComponent {
    fn id(&self) -> &ComponentId {
        &self.id
    }
    fn schema(&self) -> Vec<FieldSpec> {
        vec![FieldSpec::text("labels", "Tab labels (comma-separated)").required()]
    }
    fn help_entry(&self) -> Option<HelpEntry> {
        Some(HelpEntry::new(
            "builder.components.tabs",
            "Tabs",
            "Tabbed content panels with comma-separated labels. Each child renders as one tab panel.",
        ))
    }
    fn render_slint(
        &self,
        ctx: &RenderSlintContext<'_>,
        props: &Value,
        children: &[Node],
        out: &mut SlintEmitter,
    ) -> Result<(), RenderError> {
        let labels = props.get("labels").and_then(|v| v.as_str()).unwrap_or("");
        out.block("VerticalLayout", |out| {
            out.prop_px("spacing", 0.0);
            out.block("HorizontalLayout", |out| {
                out.prop_px("spacing", 0.0);
                for (i, label) in labels.split(',').enumerate() {
                    let label = label.trim();
                    if !label.is_empty() {
                        out.block("Rectangle", |out| {
                            out.prop_px("height", 32.0);
                            out.prop_px("min-width", 80.0);
                            if i == 0 {
                                out.line("background: #2e3440;");
                            } else {
                                out.line("background: #1a1e28;");
                            }
                            out.block("Text", |out| {
                                out.prop_string("text", label);
                                out.prop_px("font-size", 13.0);
                                out.line("horizontal-alignment: center;");
                                out.line("vertical-alignment: center;");
                                Ok(())
                            })
                        })?;
                    }
                }
                Ok(())
            })?;
            out.block("VerticalLayout", |out| {
                out.prop_px("padding", 12.0);
                out.prop_px("spacing", 8.0);
                ctx.render_children(children, out)
            })
        })
    }
}

/// Collapsible section with a title. Renders as `<details>/<summary>` in HTML.
pub struct AccordionComponent {
    pub id: ComponentId,
}

impl Component for AccordionComponent {
    fn id(&self) -> &ComponentId {
        &self.id
    }
    fn schema(&self) -> Vec<FieldSpec> {
        vec![
            FieldSpec::text("title", "Section title").required(),
            FieldSpec::boolean("open", "Initially open"),
        ]
    }
    fn help_entry(&self) -> Option<HelpEntry> {
        Some(HelpEntry::new(
            "builder.components.accordion",
            "Accordion",
            "Collapsible content section with a title bar. Toggle open state to expand or collapse.",
        ))
    }
    fn render_slint(
        &self,
        ctx: &RenderSlintContext<'_>,
        props: &Value,
        children: &[Node],
        out: &mut SlintEmitter,
    ) -> Result<(), RenderError> {
        let title = props.get("title").and_then(|v| v.as_str()).unwrap_or("");
        out.block("VerticalLayout", |out| {
            out.prop_px("spacing", 4.0);
            out.block("Rectangle", |out| {
                out.prop_px("height", 32.0);
                out.line("background: #2e3440;");
                out.line("border-radius: 4px;");
                out.block("Text", |out| {
                    let display = format!("▸ {title}");
                    out.prop_string("text", &display);
                    out.prop_px("font-size", 14.0);
                    out.line("font-weight: 600;");
                    out.line("vertical-alignment: center;");
                    Ok(())
                })
            })?;
            out.block("VerticalLayout", |out| {
                out.prop_px("padding-left", 16.0);
                out.prop_px("spacing", 8.0);
                ctx.render_children(children, out)
            })
        })
    }
}

/// HTML `<button>`. Renders as `<button type="submit">text</button>`.
pub struct ButtonComponent {
    pub id: ComponentId,
}

impl Component for ButtonComponent {
    fn id(&self) -> &ComponentId {
        &self.id
    }
    fn schema(&self) -> Vec<FieldSpec> {
        vec![
            FieldSpec::text("text", "Button label").required(),
            FieldSpec::select(
                "type",
                "Button type",
                vec![
                    crate::registry::SelectOption::new("submit", "Submit"),
                    crate::registry::SelectOption::new("button", "Button"),
                    crate::registry::SelectOption::new("reset", "Reset"),
                ],
            )
            .with_default(Value::from("submit")),
            FieldSpec::boolean("disabled", "Disabled"),
        ]
    }
    fn help_entry(&self) -> Option<HelpEntry> {
        Some(HelpEntry::new(
            "builder.components.button",
            "Button",
            "Submit or action button with configurable text and disabled state.",
        ))
    }
    fn render_slint(
        &self,
        _ctx: &RenderSlintContext<'_>,
        props: &Value,
        _children: &[Node],
        out: &mut SlintEmitter,
    ) -> Result<(), RenderError> {
        let text = props
            .get("text")
            .and_then(|v| v.as_str())
            .unwrap_or("Submit");
        out.block("Rectangle", |out| {
            out.prop_px("height", 36.0);
            out.prop_px("min-width", 80.0);
            out.line("background: #3b82f6;");
            out.line("border-radius: 6px;");
            out.block("Text", |out| {
                out.prop_string("text", text);
                out.prop_px("font-size", 14.0);
                out.line("color: #ffffff;");
                out.line("font-weight: 600;");
                out.line("horizontal-alignment: center;");
                out.line("vertical-alignment: center;");
                Ok(())
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::render_document_slint_source;
    use crate::BuilderDocument;
    use prism_core::design_tokens::DesignTokens;
    use serde_json::json;

    fn setup() -> (ComponentRegistry, DesignTokens) {
        let mut reg = ComponentRegistry::new();
        register_builtins(&mut reg).expect("register builtins");
        (reg, DesignTokens::default())
    }

    fn doc(node: Node) -> BuilderDocument {
        BuilderDocument {
            root: Some(node),
            ..Default::default()
        }
    }

    #[test]
    fn heading_schema_has_required_text_field() {
        let comp = HeadingComponent {
            id: "heading".into(),
        };
        let schema = comp.schema();
        assert_eq!(schema.len(), 2);
        assert_eq!(schema[0].key, "text");
        assert!(schema[0].required);
        assert_eq!(schema[1].key, "level");
    }

    #[test]
    fn register_builtins_seeds_seventeen_components() {
        let (reg, _) = setup();
        for id in [
            "heading",
            "text",
            "link",
            "image",
            "container",
            "form",
            "input",
            "button",
            "card",
            "code",
            "divider",
            "spacer",
            "columns",
            "list",
            "table",
            "tabs",
            "accordion",
        ] {
            assert!(reg.get(id).is_some(), "missing component: {id}");
        }
    }

    #[test]
    fn card_schema_has_required_title() {
        let comp = CardComponent { id: "card".into() };
        let schema = comp.schema();
        assert_eq!(schema[0].key, "title");
        assert!(schema[0].required);
    }

    #[test]
    fn divider_schema_is_empty() {
        let comp = DividerComponent {
            id: "divider".into(),
        };
        assert!(comp.schema().is_empty());
    }

    #[test]
    fn slint_walker_renders_card() {
        let (reg, tokens) = setup();
        let d = doc(Node {
            id: "c".into(),
            component: "card".into(),
            props: json!({ "title": "My Card", "body": "details" }),
            children: vec![],
            ..Default::default()
        });
        let source = render_document_slint_source(&d, &reg, &tokens).unwrap();
        assert!(source.contains(r#"text: "My Card";"#));
        assert!(source.contains(r#"text: "details";"#));
    }

    #[test]
    fn slint_walker_renders_code() {
        let (reg, tokens) = setup();
        let d = doc(Node {
            id: "c".into(),
            component: "code".into(),
            props: json!({ "code": "fn main() {}" }),
            children: vec![],
            ..Default::default()
        });
        let source = render_document_slint_source(&d, &reg, &tokens).unwrap();
        assert!(source.contains(r#"text: "fn main() {}";"#));
        assert!(source.contains("monospace"));
    }

    #[test]
    fn slint_walker_renders_divider() {
        let (reg, tokens) = setup();
        let d = doc(Node {
            id: "d".into(),
            component: "divider".into(),
            props: json!({}),
            children: vec![],
            ..Default::default()
        });
        let source = render_document_slint_source(&d, &reg, &tokens).unwrap();
        assert!(source.contains("height: 1px;"));
    }

    #[test]
    fn slint_walker_renders_spacer() {
        let (reg, tokens) = setup();
        let d = doc(Node {
            id: "s".into(),
            component: "spacer".into(),
            props: json!({ "height": 48 }),
            children: vec![],
            ..Default::default()
        });
        let source = render_document_slint_source(&d, &reg, &tokens).unwrap();
        assert!(source.contains("height: 48px;"));
    }

    #[test]
    fn slint_walker_renders_columns() {
        let (reg, tokens) = setup();
        let d = doc(Node {
            id: "cols".into(),
            component: "columns".into(),
            props: json!({ "gap": 24 }),
            children: vec![Node {
                id: "c1".into(),
                component: "text".into(),
                props: json!({ "body": "left" }),
                children: vec![],
                ..Default::default()
            }],
            ..Default::default()
        });
        let source = render_document_slint_source(&d, &reg, &tokens).unwrap();
        assert!(source.contains("HorizontalLayout {"));
        assert!(source.contains("spacing: 24px;"));
    }

    #[test]
    fn slint_walker_renders_table() {
        let (reg, tokens) = setup();
        let d = doc(Node {
            id: "t".into(),
            component: "table".into(),
            props: json!({ "headers": "Name, Age", "caption": "Users" }),
            children: vec![],
            ..Default::default()
        });
        let source = render_document_slint_source(&d, &reg, &tokens).unwrap();
        assert!(source.contains(r#"text: "Name";"#));
        assert!(source.contains(r#"text: "Age";"#));
        assert!(source.contains(r#"text: "Users";"#));
    }

    #[test]
    fn slint_walker_renders_tabs() {
        let (reg, tokens) = setup();
        let d = doc(Node {
            id: "t".into(),
            component: "tabs".into(),
            props: json!({ "labels": "Tab 1, Tab 2" }),
            children: vec![],
            ..Default::default()
        });
        let source = render_document_slint_source(&d, &reg, &tokens).unwrap();
        assert!(source.contains(r#"text: "Tab 1";"#));
        assert!(source.contains(r#"text: "Tab 2";"#));
    }

    #[test]
    fn slint_walker_renders_accordion() {
        let (reg, tokens) = setup();
        let d = doc(Node {
            id: "a".into(),
            component: "accordion".into(),
            props: json!({ "title": "FAQ", "open": true }),
            children: vec![Node {
                id: "a1".into(),
                component: "text".into(),
                props: json!({ "body": "answer" }),
                children: vec![],
                ..Default::default()
            }],
            ..Default::default()
        });
        let source = render_document_slint_source(&d, &reg, &tokens).unwrap();
        assert!(source.contains("FAQ"));
        assert!(source.contains(r#"text: "answer";"#));
    }

    #[test]
    fn slint_walker_covers_full_catalog() {
        let (reg, tokens) = setup();
        let d = doc(Node {
            id: "root".into(),
            component: "container".into(),
            props: json!({ "spacing": 16 }),
            children: vec![
                Node {
                    id: "h".into(),
                    component: "heading".into(),
                    props: json!({ "text": "Welcome", "level": 2 }),
                    children: vec![],
                    ..Default::default()
                },
                Node {
                    id: "p".into(),
                    component: "text".into(),
                    props: json!({ "body": "intro body" }),
                    children: vec![],
                    ..Default::default()
                },
                Node {
                    id: "l".into(),
                    component: "link".into(),
                    props: json!({ "href": "/x", "text": "Read" }),
                    children: vec![],
                    ..Default::default()
                },
                Node {
                    id: "i".into(),
                    component: "image".into(),
                    props: json!({ "src": "/a.png", "alt": "hero" }),
                    children: vec![],
                    ..Default::default()
                },
            ],
            ..Default::default()
        });
        let source = render_document_slint_source(&d, &reg, &tokens).unwrap();
        assert!(source.contains("VerticalLayout {"));
        assert!(source.contains("spacing: 16"));
        assert!(source.contains(r#"text: "Welcome";"#));
        assert!(source.contains(r#"text: "intro body";"#));
        assert!(source.contains(r#"text: "Read";"#));
        assert!(source.contains(r#"text: "hero";"#));
    }
}
