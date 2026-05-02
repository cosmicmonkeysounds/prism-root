//! Starter component catalog — the default Slint-side registry.
//!
//! Seventeen blocks land here: `text`, `image`, `container`, `form`,
//! `input`, `button`, `card` (prefab), `code`, `divider`, `spacer`,
//! `columns`, `list`, `table`, `tabs`, `accordion`, and `graph-view`.
//! Each implements [`Component`] with a `render_slint` method that
//! emits `.slint` DSL via [`SlintEmitter`] for Studio's live builder
//! panel.
//!
//! HTML SSR is handled separately by [`crate::html_starter`] via the
//! [`crate::html_block::HtmlBlock`] trait.

use std::sync::Arc;

use prism_core::help::HelpEntry;
use serde_json::Value;

use serde_json::json;

use crate::asset::AssetSource;
use crate::component::{Component, ComponentId, RenderError, RenderSlintContext};
use crate::document::Node;
use crate::facet::FacetComponent;
use crate::prefab::{ExposedSlot, PrefabComponent, PrefabDef};
use crate::registry::{
    prop_f64, prop_str, ComponentRegistry, FieldSpec, NumericBounds, RegistryError, SelectOption,
};
use crate::schemas;
use crate::signal::{with_common_signals, SignalDef};
use crate::slint_source::{escape_slint_string, SlintEmitter};
use crate::style::StyleProperties;
use crate::variant::{VariantAxis, VariantOption};

/// Register the starter catalog into `reg`. Call this once at boot
/// to get a registry with fifteen ready-to-render components.
pub fn register_builtins(reg: &mut ComponentRegistry) -> Result<(), RegistryError> {
    reg.register(Arc::new(TextComponent { id: "text".into() }))?;
    reg.register(Arc::new(ImageComponent { id: "image".into() }))?;
    reg.register(Arc::new(ContainerComponent {
        id: "container".into(),
    }))?;
    reg.register(Arc::new(FormComponent { id: "form".into() }))?;
    reg.register(Arc::new(InputComponent { id: "input".into() }))?;
    reg.register(Arc::new(ButtonComponent {
        id: "button".into(),
    }))?;
    reg.register(Arc::new(PrefabComponent::new(card_prefab_def())))?;
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
    reg.register(Arc::new(FacetComponent::new()))?;
    reg.register(Arc::new(GraphViewComponent {
        id: "graph-view".into(),
    }))?;
    Ok(())
}

fn level_font_size(level: &str) -> f64 {
    match level {
        "h1" => 32.0,
        "h2" => 26.0,
        "h3" => 22.0,
        "h4" => 18.0,
        "h5" => 16.0,
        "h6" => 14.0,
        _ => 14.0,
    }
}

fn level_font_weight(level: &str) -> u16 {
    match level {
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => 700,
        _ => 400,
    }
}

fn emit_text_style(out: &mut SlintEmitter, style: &StyleProperties) {
    if let Some(ref color) = style.color {
        out.prop_color("color", color);
    }
    if let Some(ref family) = style.font_family {
        out.prop_string("font-family", family);
    }
    if let Some(ls) = style.letter_spacing {
        out.prop_px("letter-spacing", ls as f64);
    }
}

/// Unified text block — paragraph, heading (h1–h6), or link depending
/// on the `level` and `href` props.
pub struct TextComponent {
    pub id: ComponentId,
}

impl Component for TextComponent {
    fn id(&self) -> &ComponentId {
        &self.id
    }
    fn schema(&self) -> Vec<FieldSpec> {
        schemas::text()
    }
    fn help_entry(&self) -> Option<HelpEntry> {
        Some(HelpEntry::new(
            "builder.components.text",
            "Text",
            "Text block — paragraph, heading, or link. Set level for heading sizes, href for hyperlinks.",
        ))
    }
    fn signals(&self) -> Vec<SignalDef> {
        with_common_signals(vec![SignalDef::new(
            "link-clicked",
            "Fires when a hyperlink in the text is clicked",
        )
        .with_payload(vec![FieldSpec::text("href", "Link URL")])])
    }
    fn render_slint(
        &self,
        ctx: &RenderSlintContext<'_>,
        props: &Value,
        _children: &[Node],
        out: &mut SlintEmitter,
    ) -> Result<(), RenderError> {
        let body = prop_str(props, "body", "");
        let level = prop_str(props, "level", "paragraph");
        let href = prop_str(props, "href", "");
        let style = ctx.style();

        let default_size = level_font_size(level);
        let default_weight = level_font_weight(level);

        out.block("Text", |out| {
            out.prop_string("text", body);
            let font_size = style.font_size.map(|s| s as f64).unwrap_or(default_size);
            out.prop_px("font-size", font_size);
            let weight = style.font_weight.unwrap_or(default_weight);
            if weight != 400 {
                out.property("font-weight", weight.to_string());
            }
            if !href.is_empty() {
                let color = style.color.as_deref().unwrap_or("#5aa0ff");
                out.prop_color("color", color);
            }
            out.line("wrap: word-wrap;");
            emit_text_style(out, &style);
            Ok(())
        })
    }
}

/// Image block. Accepts a VFS binary ref or an external URL via
/// the `src` prop and displays a builder placeholder. The HTML SSR
/// path resolves VFS hashes to `/asset/{hash}`.
pub struct ImageComponent {
    pub id: ComponentId,
}

impl Component for ImageComponent {
    fn id(&self) -> &ComponentId {
        &self.id
    }
    fn schema(&self) -> Vec<FieldSpec> {
        schemas::image()
    }
    fn help_entry(&self) -> Option<HelpEntry> {
        Some(HelpEntry::new(
            "builder.components.image",
            "Image",
            "Embedded image. Upload from your device, pick from the current vault, or paste an external URL. Supports configurable object-fit.",
        ))
    }
    fn signals(&self) -> Vec<SignalDef> {
        with_common_signals(vec![SignalDef::new(
            "loaded",
            "Fires when the image finishes loading",
        )])
    }
    fn render_slint(
        &self,
        ctx: &RenderSlintContext<'_>,
        props: &Value,
        _children: &[Node],
        out: &mut SlintEmitter,
    ) -> Result<(), RenderError> {
        let fit = prop_str(props, "fit", "cover");
        let href = prop_str(props, "href", "");
        let source = props.get("src").and_then(AssetSource::from_prop);

        let slint_fit = match fit {
            "contain" => "contain",
            "fill" => "fill",
            "none" => "none",
            _ => "cover",
        };

        let resolved_path: Option<String> = match &source {
            Some(AssetSource::Vfs { hash, .. }) => ctx
                .asset_paths
                .get(hash)
                .map(|p| p.to_string_lossy().into_owned()),
            Some(AssetSource::Url { url }) => Some(url.clone()),
            None => None,
        };

        if let Some(path) = resolved_path {
            out.block("Rectangle", |out| {
                out.line("clip: true;");
                out.line("horizontal-stretch: 1;");
                out.line("vertical-stretch: 1;");
                if !href.is_empty() {
                    out.line("border-width: 2px;");
                    out.line("border-color: #5aa0ff;");
                    out.line("border-radius: 4px;");
                }
                out.block("Image", |out| {
                    out.line(format!(
                        "source: @image-url(\"{}\");",
                        escape_slint_string(&path)
                    ));
                    out.line(format!("image-fit: {slint_fit};"));
                    out.line("width: parent.width;");
                    out.line("height: parent.height;");
                    Ok(())
                })
            })
        } else {
            out.block("Rectangle", |out| {
                out.line("horizontal-stretch: 1;");
                Ok(())
            })
        }
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
        schemas::container()
    }
    fn help_entry(&self) -> Option<HelpEntry> {
        Some(HelpEntry::new(
            "builder.components.container",
            "Container",
            "Layout wrapper that groups child components with configurable spacing.",
        ))
    }
    fn signals(&self) -> Vec<SignalDef> {
        with_common_signals(vec![SignalDef::new(
            "child-added",
            "Fires when a child component is added",
        )
        .with_payload(vec![FieldSpec::text("child_id", "Added child node ID")])])
    }
    fn render_slint(
        &self,
        ctx: &RenderSlintContext<'_>,
        props: &Value,
        children: &[Node],
        out: &mut SlintEmitter,
    ) -> Result<(), RenderError> {
        let style = ctx.style();
        let spacing = style
            .base_spacing
            .map(|s| s as f64)
            .unwrap_or(prop_f64(props, "spacing", 12.0));
        let padding = prop_f64(props, "padding", 0.0);
        let border_width = prop_f64(props, "border_width", 0.0);
        let border_color = prop_str(props, "border_color", "#3b4252");

        let has_visual =
            style.background.is_some() || style.border_radius.is_some() || border_width > 0.0;

        let render_inner = |out: &mut SlintEmitter| -> Result<(), RenderError> {
            out.block("VerticalLayout", |out| {
                out.prop_px("spacing", spacing);
                if padding > 0.0 {
                    out.prop_px("padding", padding);
                }
                out.line("alignment: start;");
                out.line("horizontal-stretch: 1;");
                out.line("vertical-stretch: 1;");
                ctx.render_children(children, out)
            })
        };

        if has_visual {
            out.block("Rectangle", |out| {
                if let Some(ref bg) = style.background {
                    out.prop_color("background", bg);
                }
                if let Some(radius) = style.border_radius {
                    out.prop_px("border-radius", radius as f64);
                }
                if border_width > 0.0 {
                    out.prop_px("border-width", border_width);
                    out.prop_color("border-color", border_color);
                }
                render_inner(out)
            })
        } else {
            render_inner(out)
        }
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
        schemas::form()
    }
    fn help_entry(&self) -> Option<HelpEntry> {
        Some(HelpEntry::new(
            "builder.components.form",
            "Form",
            "HTML form wrapper. Nest input and button components inside to build forms.",
        ))
    }
    fn signals(&self) -> Vec<SignalDef> {
        with_common_signals(vec![
            SignalDef::new("submitted", "Fires when the form is submitted"),
            SignalDef::new("validated", "Fires after form validation runs").with_payload(vec![
                FieldSpec::boolean("valid", "Whether validation passed"),
            ]),
        ])
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
        schemas::input()
    }
    fn help_entry(&self) -> Option<HelpEntry> {
        Some(HelpEntry::new(
            "builder.components.input",
            "Input",
            "Text, email, or password field with placeholder and name binding.",
        ))
    }
    fn signals(&self) -> Vec<SignalDef> {
        with_common_signals(vec![
            SignalDef::new("changed", "Fires when the input value changes").with_payload(vec![
                FieldSpec::text("value", "Current input value"),
                FieldSpec::text("old_value", "Previous input value"),
            ]),
            SignalDef::new("key-pressed", "Fires on each keystroke").with_payload(vec![
                FieldSpec::text("key", "Key name"),
                FieldSpec::text("value", "Current input value"),
            ]),
        ])
    }
    fn render_slint(
        &self,
        _ctx: &RenderSlintContext<'_>,
        props: &Value,
        _children: &[Node],
        out: &mut SlintEmitter,
    ) -> Result<(), RenderError> {
        let label = prop_str(props, "label", "");
        let placeholder = prop_str(props, "placeholder", "");
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

/// Built-in card prefab: Container + title text + body text.
pub fn card_prefab_def() -> PrefabDef {
    PrefabDef {
        id: "card".into(),
        label: "Card".into(),
        description: "Bordered content card with title and body text slots.".into(),
        root: Node {
            id: "card-root".into(),
            component: "container".into(),
            props: json!({
                "spacing": 8,
                "padding": 16,
                "border_width": 1,
                "border_color": "#3b4252"
            }),
            children: vec![
                Node {
                    id: "card-title".into(),
                    component: "text".into(),
                    props: json!({ "body": "", "level": "h3" }),
                    children: vec![],
                    ..Default::default()
                },
                Node {
                    id: "card-body".into(),
                    component: "text".into(),
                    props: json!({ "body": "", "level": "paragraph" }),
                    children: vec![],
                    ..Default::default()
                },
            ],
            style: StyleProperties {
                background: Some("#2e3440".into()),
                border_radius: Some(8.0),
                ..Default::default()
            },
            ..Default::default()
        },
        exposed: vec![
            ExposedSlot {
                key: "title".into(),
                target_node: "card-title".into(),
                target_prop: "body".into(),
                spec: FieldSpec::text("title", "Card title").required(),
            },
            ExposedSlot {
                key: "body".into(),
                target_node: "card-body".into(),
                target_prop: "body".into(),
                spec: FieldSpec::textarea("body", "Card body"),
            },
        ],
        variants: vec![],
        thumbnail: None,
    }
}

/// Instantiate a prefab definition into a document node tree with
/// fresh IDs. Each node in the returned tree is a regular built-in
/// component that the inspector and property panel handle natively.
pub fn materialize_prefab(def: &PrefabDef, counter: &mut u64) -> Node {
    fn assign_ids(node: &Node, counter: &mut u64) -> Node {
        let id = format!("n{}", *counter);
        *counter += 1;
        Node {
            id,
            component: node.component.clone(),
            props: node.props.clone(),
            children: node
                .children
                .iter()
                .map(|c| assign_ids(c, counter))
                .collect(),
            style: node.style.clone(),
            layout_mode: node.layout_mode.clone(),
            transform: node.transform.clone(),
            modifiers: node.modifiers.clone(),
        }
    }
    assign_ids(&def.root, counter)
}

/// Look up a built-in prefab by component type. Returns `None` for
/// non-prefab component types.
pub fn builtin_prefab(component_type: &str) -> Option<PrefabDef> {
    match component_type {
        "card" => Some(card_prefab_def()),
        _ => None,
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
        schemas::code()
    }
    fn help_entry(&self) -> Option<HelpEntry> {
        Some(HelpEntry::new(
            "builder.components.code",
            "Code",
            "Preformatted code block with optional language label.",
        ))
    }
    fn signals(&self) -> Vec<SignalDef> {
        with_common_signals(vec![])
    }
    fn render_slint(
        &self,
        ctx: &RenderSlintContext<'_>,
        props: &Value,
        _children: &[Node],
        out: &mut SlintEmitter,
    ) -> Result<(), RenderError> {
        let code = prop_str(props, "code", "");
        let style = ctx.style();
        let bg = style.background.as_deref().unwrap_or("#1a1e28");
        let radius = style.border_radius.unwrap_or(6.0);
        out.block("Rectangle", |out| {
            out.prop_color("background", bg);
            out.prop_px("border-radius", radius as f64);
            out.block("VerticalLayout", |out| {
                out.prop_px("padding", 12.0);
                out.block("Text", |out| {
                    out.prop_string("text", code);
                    let font_size = style.font_size.unwrap_or(13.0);
                    out.prop_px("font-size", font_size as f64);
                    let color = style.color.as_deref().unwrap_or("#a3be8c");
                    out.prop_color("color", color);
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
        schemas::divider()
    }
    fn help_entry(&self) -> Option<HelpEntry> {
        Some(HelpEntry::new(
            "builder.components.divider",
            "Divider",
            "Horizontal separator line between content sections.",
        ))
    }
    fn signals(&self) -> Vec<SignalDef> {
        with_common_signals(vec![])
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
        schemas::spacer()
    }
    fn help_entry(&self) -> Option<HelpEntry> {
        Some(HelpEntry::new(
            "builder.components.spacer",
            "Spacer",
            "Vertical spacing element with configurable height in pixels.",
        ))
    }
    fn signals(&self) -> Vec<SignalDef> {
        with_common_signals(vec![])
    }
    fn render_slint(
        &self,
        _ctx: &RenderSlintContext<'_>,
        props: &Value,
        _children: &[Node],
        out: &mut SlintEmitter,
    ) -> Result<(), RenderError> {
        let height = prop_f64(props, "height", 24.0);
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
        schemas::columns()
    }
    fn help_entry(&self) -> Option<HelpEntry> {
        Some(HelpEntry::new(
            "builder.components.columns",
            "Columns",
            "Side-by-side horizontal layout with configurable gap between children.",
        ))
    }
    fn signals(&self) -> Vec<SignalDef> {
        with_common_signals(vec![])
    }
    fn render_slint(
        &self,
        ctx: &RenderSlintContext<'_>,
        props: &Value,
        children: &[Node],
        out: &mut SlintEmitter,
    ) -> Result<(), RenderError> {
        let gap = prop_f64(props, "gap", 16.0);
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
        schemas::list()
    }
    fn help_entry(&self) -> Option<HelpEntry> {
        Some(HelpEntry::new(
            "builder.components.list",
            "List",
            "Ordered or unordered list. Toggle the ordered property to switch between numbered and bulleted styles.",
        ))
    }
    fn signals(&self) -> Vec<SignalDef> {
        with_common_signals(vec![SignalDef::new(
            "item-clicked",
            "Fires when a list item is clicked",
        )
        .with_payload(vec![FieldSpec::number(
            "index",
            "Item index",
            Default::default(),
        )])])
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
        schemas::table()
    }
    fn help_entry(&self) -> Option<HelpEntry> {
        Some(HelpEntry::new(
            "builder.components.table",
            "Table",
            "Data table with comma-separated column headers and optional caption.",
        ))
    }
    fn signals(&self) -> Vec<SignalDef> {
        with_common_signals(vec![
            SignalDef::new("row-clicked", "Fires when a table row is clicked").with_payload(vec![
                FieldSpec::number("row", "Row index", Default::default()),
            ]),
            SignalDef::new("cell-clicked", "Fires when a table cell is clicked").with_payload(
                vec![
                    FieldSpec::number("row", "Row index", Default::default()),
                    FieldSpec::number("column", "Column index", Default::default()),
                ],
            ),
            SignalDef::new("header-clicked", "Fires when a column header is clicked").with_payload(
                vec![FieldSpec::number(
                    "column",
                    "Column index",
                    Default::default(),
                )],
            ),
        ])
    }
    fn render_slint(
        &self,
        _ctx: &RenderSlintContext<'_>,
        props: &Value,
        _children: &[Node],
        out: &mut SlintEmitter,
    ) -> Result<(), RenderError> {
        let headers = prop_str(props, "headers", "");
        let caption = prop_str(props, "caption", "");
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
        schemas::tabs()
    }
    fn help_entry(&self) -> Option<HelpEntry> {
        Some(HelpEntry::new(
            "builder.components.tabs",
            "Tabs",
            "Tabbed content panels with comma-separated labels. Each child renders as one tab panel.",
        ))
    }
    fn signals(&self) -> Vec<SignalDef> {
        with_common_signals(vec![SignalDef::new(
            "tab-changed",
            "Fires when the active tab changes",
        )
        .with_payload(vec![
            FieldSpec::number("index", "Active tab index", Default::default()),
            FieldSpec::text("label", "Active tab label"),
        ])])
    }
    fn render_slint(
        &self,
        ctx: &RenderSlintContext<'_>,
        props: &Value,
        children: &[Node],
        out: &mut SlintEmitter,
    ) -> Result<(), RenderError> {
        let labels = prop_str(props, "labels", "");
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
        schemas::accordion()
    }
    fn help_entry(&self) -> Option<HelpEntry> {
        Some(HelpEntry::new(
            "builder.components.accordion",
            "Accordion",
            "Collapsible content section with a title bar. Toggle open state to expand or collapse.",
        ))
    }
    fn signals(&self) -> Vec<SignalDef> {
        with_common_signals(vec![SignalDef::new(
            "toggled",
            "Fires when the section is expanded or collapsed",
        )
        .with_payload(vec![FieldSpec::boolean(
            "open",
            "Whether the section is now open",
        )])])
    }
    fn render_slint(
        &self,
        ctx: &RenderSlintContext<'_>,
        props: &Value,
        children: &[Node],
        out: &mut SlintEmitter,
    ) -> Result<(), RenderError> {
        let title = prop_str(props, "title", "");
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
        schemas::button()
    }
    fn help_entry(&self) -> Option<HelpEntry> {
        Some(HelpEntry::new(
            "builder.components.button",
            "Button",
            "Submit or action button with configurable text and disabled state.",
        ))
    }
    fn signals(&self) -> Vec<SignalDef> {
        with_common_signals(vec![])
    }
    fn variants(&self) -> Vec<VariantAxis> {
        vec![VariantAxis {
            key: "variant".into(),
            label: "Variant".into(),
            options: vec![
                VariantOption {
                    value: "primary".into(),
                    label: "Primary".into(),
                    overrides: serde_json::json!({ "bg": "#3b82f6", "color": "#ffffff" }),
                },
                VariantOption {
                    value: "secondary".into(),
                    label: "Secondary".into(),
                    overrides: serde_json::json!({ "bg": "#4b5563", "color": "#ffffff" }),
                },
                VariantOption {
                    value: "danger".into(),
                    label: "Danger".into(),
                    overrides: serde_json::json!({ "bg": "#ef4444", "color": "#ffffff" }),
                },
                VariantOption {
                    value: "ghost".into(),
                    label: "Ghost".into(),
                    overrides: serde_json::json!({ "bg": "transparent", "color": "#d8dee9" }),
                },
            ],
        }]
    }
    fn render_slint(
        &self,
        _ctx: &RenderSlintContext<'_>,
        props: &Value,
        _children: &[Node],
        out: &mut SlintEmitter,
    ) -> Result<(), RenderError> {
        let text = prop_str(props, "text", "Submit");
        let href = prop_str(props, "href", "");
        out.block("Rectangle", |out| {
            out.prop_px("height", 36.0);
            out.prop_px("min-width", 80.0);
            out.line("background: #3b82f6;");
            out.line("border-radius: 6px;");
            out.block("HorizontalLayout", |out| {
                out.line("alignment: center;");
                out.prop_px("spacing", 4.0);
                if !href.is_empty() {
                    out.block("Text", |out| {
                        out.prop_string("text", "🔗");
                        out.prop_px("font-size", 12.0);
                        out.line("vertical-alignment: center;");
                        Ok(())
                    })?;
                }
                out.block("Text", |out| {
                    out.prop_string("text", text);
                    out.prop_px("font-size", 14.0);
                    out.line("color: #ffffff;");
                    out.line("font-weight: 600;");
                    out.line("vertical-alignment: center;");
                    Ok(())
                })
            })
        })
    }
}

/// Interactive node-and-edge graph visualization. Renders nodes as
/// positioned circles on a canvas with label text.
pub struct GraphViewComponent {
    pub id: ComponentId,
}

impl Component for GraphViewComponent {
    fn id(&self) -> &ComponentId {
        &self.id
    }
    fn schema(&self) -> Vec<FieldSpec> {
        vec![
            FieldSpec::text("node_label_field", "Node label field"),
            FieldSpec::text("node_color_field", "Node color field"),
            FieldSpec::boolean("edge_label", "Show edge labels"),
            FieldSpec::select(
                "layout",
                "Layout algorithm",
                vec![
                    SelectOption::new("force", "Force-directed"),
                    SelectOption::new("tree", "Tree"),
                    SelectOption::new("radial", "Radial"),
                    SelectOption::new("grid", "Grid"),
                ],
            )
            .with_default(Value::from("force")),
            FieldSpec::number(
                "node_size",
                "Node size (px)",
                NumericBounds::min_max(20.0, 120.0),
            )
            .with_default(Value::from(48)),
            FieldSpec::boolean("show_arrows", "Show arrows").with_default(Value::from(true)),
        ]
    }
    fn help_entry(&self) -> Option<HelpEntry> {
        Some(HelpEntry::new(
            "builder.components.graph-view",
            "Graph View",
            "Interactive node-and-edge relationship visualization with configurable layout algorithms and node styling.",
        ))
    }
    fn signals(&self) -> Vec<SignalDef> {
        with_common_signals(vec![
            SignalDef::new("node-clicked", "Fires when a graph node is clicked")
                .with_payload(vec![FieldSpec::text("node_id", "Clicked node ID")]),
            SignalDef::new("edge-clicked", "Fires when a graph edge is clicked").with_payload(
                vec![
                    FieldSpec::text("source_id", "Source node ID"),
                    FieldSpec::text("target_id", "Target node ID"),
                ],
            ),
            SignalDef::new(
                "node-double-clicked",
                "Fires when a graph node is double-clicked",
            )
            .with_payload(vec![FieldSpec::text("node_id", "Double-clicked node ID")]),
        ])
    }
    fn render_slint(
        &self,
        ctx: &RenderSlintContext<'_>,
        props: &Value,
        _children: &[Node],
        out: &mut SlintEmitter,
    ) -> Result<(), RenderError> {
        let label_field = prop_str(props, "node_label_field", "label");
        let node_size = prop_f64(props, "node_size", 48.0);
        let style = ctx.style();
        let bg = style.background.as_deref().unwrap_or("#1e2533");

        // Parse nodes from props — expected as a JSON array of objects
        let nodes: Vec<&Value> = props
            .get("nodes")
            .and_then(|v| v.as_array())
            .map(|a| a.iter().collect())
            .unwrap_or_default();

        let node_count = nodes.len();
        // Grid columns for simple layout
        let cols = (node_count as f64).sqrt().ceil().max(1.0) as usize;

        out.block("VerticalLayout", |out| {
            out.prop_px("spacing", 0.0);
            out.line("horizontal-stretch: 1;");
            out.line("vertical-stretch: 1;");

            // Header
            out.block("Rectangle", |out| {
                out.prop_px("height", 32.0);
                out.prop_color("background", "#2e3440");
                out.block("Text", |out| {
                    let header = format!("Graph View ({node_count} nodes)");
                    out.prop_string("text", &header);
                    out.prop_px("font-size", 13.0);
                    out.line("font-weight: 600;");
                    out.line("vertical-alignment: center;");
                    out.prop_px("x", 8.0);
                    Ok(())
                })
            })?;

            // Canvas area
            out.block("Rectangle", |out| {
                out.prop_color("background", bg);
                out.line("horizontal-stretch: 1;");
                out.line("vertical-stretch: 1;");

                // Render each node at a grid position
                for (i, node_val) in nodes.iter().enumerate() {
                    let label = node_val
                        .get(label_field)
                        .and_then(|v| v.as_str())
                        .unwrap_or("?");
                    let color = node_val
                        .get("color")
                        .and_then(|v| v.as_str())
                        .unwrap_or("#5e81ac");

                    let col = i % cols;
                    let row = i / cols;
                    let x = 24.0 + (col as f64) * (node_size + 32.0);
                    let y = 24.0 + (row as f64) * (node_size + 32.0);
                    let radius = node_size / 2.0;

                    out.block("Rectangle", |out| {
                        out.prop_px("x", x);
                        out.prop_px("y", y);
                        out.prop_px("width", node_size);
                        out.prop_px("height", node_size);
                        out.prop_px("border-radius", radius);
                        out.prop_color("background", color);

                        out.block("Text", |out| {
                            out.prop_string("text", label);
                            out.prop_px("font-size", 11.0);
                            out.line("color: #eceff4;");
                            out.line("horizontal-alignment: center;");
                            out.line("vertical-alignment: center;");
                            Ok(())
                        })
                    })?;
                }

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
    fn text_schema_has_body_level_href() {
        let comp = TextComponent { id: "text".into() };
        let schema = comp.schema();
        assert_eq!(schema.len(), 3);
        assert_eq!(schema[0].key, "body");
        assert_eq!(schema[1].key, "level");
        assert_eq!(schema[2].key, "href");
    }

    #[test]
    fn register_builtins_seeds_seventeen_components() {
        let (reg, _) = setup();
        for id in [
            "text",
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
            "facet",
            "graph-view",
        ] {
            assert!(reg.get(id).is_some(), "missing component: {id}");
        }
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
                    component: "text".into(),
                    props: json!({ "body": "Welcome", "level": "h2" }),
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
                    component: "text".into(),
                    props: json!({ "body": "Read", "href": "/x" }),
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
        assert!(source.contains(r#"@image-url("/a.png")"#));
        assert!(source.contains("image-fit: cover;"));
    }

    #[test]
    fn graph_view_id() {
        let comp = GraphViewComponent {
            id: "graph-view".into(),
        };
        assert_eq!(comp.id(), "graph-view");
    }

    #[test]
    fn graph_view_schema_has_layout_options() {
        let comp = GraphViewComponent {
            id: "graph-view".into(),
        };
        let schema = comp.schema();
        let layout_field = schema
            .iter()
            .find(|f| f.key == "layout")
            .expect("missing layout field");
        match &layout_field.kind {
            crate::registry::FieldKind::Select(options) => {
                assert_eq!(options.len(), 4);
                let values: Vec<&str> = options.iter().map(|o| o.value.as_str()).collect();
                assert!(values.contains(&"force"));
                assert!(values.contains(&"tree"));
                assert!(values.contains(&"radial"));
                assert!(values.contains(&"grid"));
            }
            other => panic!("expected Select, got {:?}", other),
        }
    }

    #[test]
    fn graph_view_signals() {
        let comp = GraphViewComponent {
            id: "graph-view".into(),
        };
        let signals = comp.signals();
        let names: Vec<&str> = signals.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"node-clicked"), "missing node-clicked");
        assert!(names.contains(&"edge-clicked"), "missing edge-clicked");
        assert!(
            names.contains(&"node-double-clicked"),
            "missing node-double-clicked"
        );
        // Should also include common signals
        assert!(names.contains(&"clicked"));
        assert!(names.contains(&"hovered"));
    }

    #[test]
    fn graph_view_renders() {
        let (reg, tokens) = setup();
        let d = doc(Node {
            id: "g".into(),
            component: "graph-view".into(),
            props: json!({
                "node_label_field": "name",
                "nodes": [
                    { "name": "Alice", "color": "#88c0d0" },
                    { "name": "Bob", "color": "#a3be8c" }
                ],
                "edges": [
                    { "source": "Alice", "target": "Bob" }
                ]
            }),
            children: vec![],
            ..Default::default()
        });
        let source = render_document_slint_source(&d, &reg, &tokens).unwrap();
        assert!(source.contains(r#"text: "Alice";"#), "missing Alice label");
        assert!(source.contains(r#"text: "Bob";"#), "missing Bob label");
        assert!(
            source.contains("Graph View (2 nodes)"),
            "missing header with node count"
        );
        assert!(
            source.contains("border-radius:"),
            "missing circular node shape"
        );
    }
}
