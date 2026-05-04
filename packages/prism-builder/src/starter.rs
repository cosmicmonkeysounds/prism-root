//! Starter component catalog — the default block registry seeding both
//! render targets in lockstep.
//!
//! Seventeen blocks land here: `text`, `image`, `container`, `form`,
//! `input`, `button`, `card` (prefab), `code`, `divider`, `spacer`,
//! `columns`, `list`, `table`, `tabs`, `accordion`, `facet`, and
//! `graph-view`. The first 14 implement [`Block`] (one impl serves
//! both Slint and HTML render paths via the blanket impls in
//! [`crate::block`]); `card`, `facet`, and `graph-view` are special
//! cases — `card` is a prefab with separate Slint/HTML wrappers,
//! `facet` likewise, and `graph-view` is Slint-only.

use std::sync::Arc;

use prism_core::help::HelpEntry;
use serde_json::Value;

use serde_json::json;

use crate::asset::AssetSource;
use crate::block::{register_block, Block};
use crate::component::{ComponentId, RenderError, RenderSlintContext};
use crate::document::Node;
use crate::facet::{FacetComponent, FacetHtmlBlock};
use crate::html::Html;
use crate::html_block::{HtmlRegistry, HtmlRenderContext};
use crate::prefab::{ExposedSlot, PrefabComponent, PrefabDef, PrefabHtmlBlock};
use crate::registry::{ComponentRegistry, FieldSpec, RegistryError};
use crate::schemas;
use crate::signal::{with_common_signals, SignalDef};
use crate::slint_source::{escape_slint_string, SlintEmitter};
use crate::style::StyleProperties;
use crate::variant::{presets as variant_presets, VariantAxis};

/// Register the starter catalog into both registries. Single source of
/// truth for the built-in block list — the Slint side and HTML SSR
/// side stay in lockstep by construction.
pub fn register_builtins(
    components: &mut ComponentRegistry,
    html: &mut HtmlRegistry,
) -> Result<(), RegistryError> {
    register_block(components, html, Arc::new(TextBlock { id: "text".into() }))?;
    register_block(
        components,
        html,
        Arc::new(ImageBlock { id: "image".into() }),
    )?;
    register_block(
        components,
        html,
        Arc::new(ContainerBlock {
            id: "container".into(),
        }),
    )?;
    register_block(components, html, Arc::new(FormBlock { id: "form".into() }))?;
    register_block(
        components,
        html,
        Arc::new(InputBlock { id: "input".into() }),
    )?;
    register_block(
        components,
        html,
        Arc::new(ButtonBlock {
            id: "button".into(),
        }),
    )?;
    register_block(components, html, Arc::new(CodeBlock { id: "code".into() }))?;
    register_block(
        components,
        html,
        Arc::new(DividerBlock {
            id: "divider".into(),
        }),
    )?;
    register_block(
        components,
        html,
        Arc::new(SpacerBlock {
            id: "spacer".into(),
        }),
    )?;
    register_block(
        components,
        html,
        Arc::new(ColumnsBlock {
            id: "columns".into(),
        }),
    )?;
    register_block(components, html, Arc::new(ListBlock { id: "list".into() }))?;
    register_block(
        components,
        html,
        Arc::new(TableBlock { id: "table".into() }),
    )?;
    register_block(components, html, Arc::new(TabsBlock { id: "tabs".into() }))?;
    register_block(
        components,
        html,
        Arc::new(AccordionBlock {
            id: "accordion".into(),
        }),
    )?;

    // `card` is a prefab — separate Slint/HTML impls share the same def.
    let card = card_prefab_def();
    components.register(Arc::new(PrefabComponent::new(card.clone())))?;
    html.register(Arc::new(PrefabHtmlBlock::new(card)))?;

    // `facet` likewise has parallel Slint/HTML impls.
    components.register(Arc::new(FacetComponent::new()))?;
    html.register(Arc::new(FacetHtmlBlock::new()))?;

    // `graph-view` is Slint-only.
    components.register(Arc::new(GraphViewBlock {
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
pub struct TextBlock {
    pub id: ComponentId,
}

impl Block for TextBlock {
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
        let p = schemas::TextProps::from_value(props);
        let style = ctx.style();

        let default_size = level_font_size(p.level.as_str());
        let default_weight = level_font_weight(p.level.as_str());

        out.block("Text", |out| {
            out.prop_string("text", &p.body);
            let font_size = style.font_size.map(|s| s as f64).unwrap_or(default_size);
            out.prop_px("font-size", font_size);
            let weight = style.font_weight.unwrap_or(default_weight);
            if weight != 400 {
                out.property("font-weight", weight.to_string());
            }
            if !p.href.is_empty() {
                let color = style.color.as_deref().unwrap_or("#5aa0ff");
                out.prop_color("color", color);
            }
            out.line("wrap: word-wrap;");
            emit_text_style(out, &style);
            Ok(())
        })
    }
    fn render_html(
        &self,
        _ctx: &HtmlRenderContext<'_>,
        props: &Value,
        _children: &[Node],
        out: &mut Html,
    ) -> Result<(), RenderError> {
        let p = schemas::TextProps::from_value(props);
        let tag = match p.level.as_str() {
            "h1" => "h1",
            "h2" => "h2",
            "h3" => "h3",
            "h4" => "h4",
            "h5" => "h5",
            "h6" => "h6",
            _ => "p",
        };
        out.open(tag);
        if !p.href.is_empty() {
            out.open_attrs("a", &[("href", &p.href)]);
            out.text(&p.body);
            out.close("a");
        } else {
            out.text(&p.body);
        }
        out.close(tag);
        Ok(())
    }
}

/// Image block. Accepts a VFS binary ref or an external URL via
/// the `src` prop and displays a builder placeholder. The HTML SSR
/// path resolves VFS hashes to `/asset/{hash}`.
pub struct ImageBlock {
    pub id: ComponentId,
}

impl Block for ImageBlock {
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
    fn variants(&self) -> Vec<VariantAxis> {
        variant_presets::image()
    }
    fn render_slint(
        &self,
        ctx: &RenderSlintContext<'_>,
        props: &Value,
        _children: &[Node],
        out: &mut SlintEmitter,
    ) -> Result<(), RenderError> {
        let p = schemas::ImageProps::from_value(props);
        let border_radius = p.border_radius as f64;
        let source = props.get("src").and_then(AssetSource::from_prop);

        let slint_fit = match p.fit.as_str() {
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
                if border_radius > 0.0 {
                    out.prop_px("border-radius", border_radius);
                }
                if !p.href.is_empty() {
                    out.line("border-width: 2px;");
                    out.line("border-color: #5aa0ff;");
                    if border_radius == 0.0 {
                        out.line("border-radius: 4px;");
                    }
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
    fn render_html(
        &self,
        _ctx: &HtmlRenderContext<'_>,
        props: &Value,
        _children: &[Node],
        out: &mut Html,
    ) -> Result<(), RenderError> {
        let p = schemas::ImageProps::from_value(props);
        let border_radius = p.border_radius.max(0);
        let src = props
            .get("src")
            .and_then(AssetSource::from_prop)
            .map(|s| s.to_html_src())
            .unwrap_or_default();
        let style = if border_radius > 0 {
            format!("object-fit:{};border-radius:{border_radius}px", p.fit)
        } else {
            format!("object-fit:{}", p.fit)
        };
        if !p.href.is_empty() {
            out.open_attrs("a", &[("href", &p.href)]);
        }
        out.void("img", &[("src", &src), ("alt", &p.alt), ("style", &style)]);
        if !p.href.is_empty() {
            out.close("a");
        }
        Ok(())
    }
}

/// Semantic `<section>` wrapper with children rendered inside.
/// Useful as a layout block in the portal body.
pub struct ContainerBlock {
    pub id: ComponentId,
}

impl Block for ContainerBlock {
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
    fn variants(&self) -> Vec<VariantAxis> {
        variant_presets::container()
    }
    fn render_slint(
        &self,
        ctx: &RenderSlintContext<'_>,
        props: &Value,
        children: &[Node],
        out: &mut SlintEmitter,
    ) -> Result<(), RenderError> {
        let p = schemas::ContainerProps::from_value(props);
        let style = ctx.style();
        let spacing = style
            .base_spacing
            .map(|s| s as f64)
            .unwrap_or(p.spacing as f64);
        let padding = p.padding as f64;
        let border_width = p.border_width as f64;

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
                    out.prop_color("border-color", &p.border_color);
                }
                render_inner(out)
            })
        } else {
            render_inner(out)
        }
    }
    fn render_html(
        &self,
        ctx: &HtmlRenderContext<'_>,
        props: &Value,
        children: &[Node],
        out: &mut Html,
    ) -> Result<(), RenderError> {
        let p = schemas::ContainerProps::from_value(props);
        let padding = p.padding;
        let border_width = p.border_width;
        let mut parts: Vec<String> = Vec::new();
        if padding > 0 {
            parts.push(format!("padding:{padding}px"));
        }
        if border_width > 0 {
            let color = if p.border_color.is_empty() {
                "#000"
            } else {
                p.border_color.as_str()
            };
            parts.push(format!("border:{border_width}px solid {color}"));
        }
        if parts.is_empty() {
            out.open("section");
        } else {
            let style = parts.join(";");
            out.open_attrs("section", &[("style", &style)]);
        }
        ctx.render_children(children, out)?;
        out.close("section");
        Ok(())
    }
}

/// HTML `<form>` wrapper. Renders children inside a `<form method="post">`.
/// L3 portals use this for interactive submissions.
pub struct FormBlock {
    pub id: ComponentId,
}

impl Block for FormBlock {
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
    fn render_html(
        &self,
        ctx: &HtmlRenderContext<'_>,
        props: &Value,
        children: &[Node],
        out: &mut Html,
    ) -> Result<(), RenderError> {
        let p = schemas::FormProps::from_value(props);
        let mut attrs: Vec<(&str, &str)> = vec![("method", p.method.as_str())];
        if !p.action.is_empty() {
            attrs.push(("action", p.action.as_str()));
        }
        out.open_attrs("form", &attrs);
        ctx.render_children(children, out)?;
        out.close("form");
        Ok(())
    }
}

/// HTML `<input>`. Renders as a void element with name, type, and placeholder.
pub struct InputBlock {
    pub id: ComponentId,
}

impl Block for InputBlock {
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
    fn variants(&self) -> Vec<VariantAxis> {
        variant_presets::input()
    }
    fn render_slint(
        &self,
        _ctx: &RenderSlintContext<'_>,
        props: &Value,
        _children: &[Node],
        out: &mut SlintEmitter,
    ) -> Result<(), RenderError> {
        let p = schemas::InputProps::from_value(props);
        out.block("VerticalLayout", |out| {
            out.prop_px("spacing", 4.0);
            if !p.label.is_empty() {
                out.block("Text", |out| {
                    out.prop_string("text", &p.label);
                    out.prop_px("font-size", 12.0);
                    Ok(())
                })?;
            }
            out.block("Rectangle", |out| {
                out.prop_px("height", 32.0);
                out.line("background: #1e2533;");
                out.line("border-radius: 4px;");
                out.block("Text", |out| {
                    let display = if p.placeholder.is_empty() {
                        "..."
                    } else {
                        p.placeholder.as_str()
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
    fn render_html(
        &self,
        _ctx: &HtmlRenderContext<'_>,
        props: &Value,
        _children: &[Node],
        out: &mut Html,
    ) -> Result<(), RenderError> {
        let p = schemas::InputProps::from_value(props);

        if !p.label.is_empty() {
            out.open("label");
            out.text(&p.label);
        }
        let mut attrs = vec![("type", p.r#type.as_str()), ("name", p.name.as_str())];
        if !p.placeholder.is_empty() {
            attrs.push(("placeholder", p.placeholder.as_str()));
        }
        if !p.value.is_empty() {
            attrs.push(("value", p.value.as_str()));
        }
        if p.required {
            attrs.push(("required", "required"));
        }
        out.void("input", &attrs);
        if !p.label.is_empty() {
            out.close("label");
        }
        Ok(())
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
pub struct CodeBlock {
    pub id: ComponentId,
}

impl Block for CodeBlock {
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
    fn variants(&self) -> Vec<VariantAxis> {
        variant_presets::code()
    }
    fn render_slint(
        &self,
        ctx: &RenderSlintContext<'_>,
        props: &Value,
        _children: &[Node],
        out: &mut SlintEmitter,
    ) -> Result<(), RenderError> {
        let p = schemas::CodeProps::from_value(props);
        let style = ctx.style();
        let bg = if p.bg.is_empty() {
            style.background.as_deref().unwrap_or("#1a1e28")
        } else {
            &p.bg
        };
        let color = if p.color.is_empty() {
            style.color.as_deref().unwrap_or("#a3be8c")
        } else {
            &p.color
        };
        let radius = style.border_radius.unwrap_or(6.0);
        out.block("Rectangle", |out| {
            out.prop_color("background", bg);
            out.prop_px("border-radius", radius as f64);
            out.block("VerticalLayout", |out| {
                out.prop_px("padding", 12.0);
                out.block("Text", |out| {
                    out.prop_string("text", &p.code);
                    let font_size = style.font_size.unwrap_or(13.0);
                    out.prop_px("font-size", font_size as f64);
                    out.prop_color("color", color);
                    out.line("font-family: \"monospace\";");
                    out.line("wrap: word-wrap;");
                    Ok(())
                })
            })
        })
    }
    fn render_html(
        &self,
        _ctx: &HtmlRenderContext<'_>,
        props: &Value,
        _children: &[Node],
        out: &mut Html,
    ) -> Result<(), RenderError> {
        let p = schemas::CodeProps::from_value(props);
        let bg = if p.bg.is_empty() { "#1a1e28" } else { &p.bg };
        let color = if p.color.is_empty() {
            "#a3be8c"
        } else {
            &p.color
        };
        let style = format!("background:{bg};color:{color};padding:12px;border-radius:6px");
        out.open_attrs("pre", &[("style", &style)]);
        if p.language.is_empty() {
            out.open("code");
        } else {
            out.open_attrs("code", &[("class", &format!("language-{}", p.language))]);
        }
        out.text(&p.code);
        out.close("code");
        out.close("pre");
        Ok(())
    }
}

/// Horizontal rule / visual separator.
/// Horizontal separator line between content sections.
///
/// First block ported to the unified [`crate::block::Block`] trait —
/// a single impl drives both the Slint and HTML render paths.
pub struct DividerBlock {
    pub id: ComponentId,
}

impl Block for DividerBlock {
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
    fn render_html(
        &self,
        _ctx: &crate::html_block::HtmlRenderContext<'_>,
        _props: &Value,
        _children: &[Node],
        out: &mut crate::html::Html,
    ) -> Result<(), RenderError> {
        out.void("hr", &[]);
        Ok(())
    }
}

/// Empty vertical spacer with configurable height. Unified Block impl.
pub struct SpacerBlock {
    pub id: ComponentId,
}

impl Block for SpacerBlock {
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
        let p = schemas::SpacerProps::from_value(props);
        out.block("Rectangle", |out| {
            out.prop_px("height", p.height as f64);
            Ok(())
        })
    }
    fn render_html(
        &self,
        _ctx: &crate::html_block::HtmlRenderContext<'_>,
        props: &Value,
        _children: &[Node],
        out: &mut crate::html::Html,
    ) -> Result<(), RenderError> {
        let p = schemas::SpacerProps::from_value(props);
        let height = p.height;
        let style = format!("height:{height}px");
        out.open_attrs("div", &[("style", &style), ("aria-hidden", "true")]);
        out.close("div");
        Ok(())
    }
}

/// Multi-column horizontal layout. Children are placed side-by-side.
pub struct ColumnsBlock {
    pub id: ComponentId,
}

impl Block for ColumnsBlock {
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
    fn variants(&self) -> Vec<VariantAxis> {
        variant_presets::columns()
    }
    fn render_slint(
        &self,
        ctx: &RenderSlintContext<'_>,
        props: &Value,
        children: &[Node],
        out: &mut SlintEmitter,
    ) -> Result<(), RenderError> {
        let p = schemas::ColumnsProps::from_value(props);
        out.block("HorizontalLayout", |out| {
            out.prop_px("spacing", p.gap as f64);
            ctx.render_children(children, out)
        })
    }
    fn render_html(
        &self,
        ctx: &HtmlRenderContext<'_>,
        props: &Value,
        children: &[Node],
        out: &mut Html,
    ) -> Result<(), RenderError> {
        let p = schemas::ColumnsProps::from_value(props);
        let gap = p.gap;
        let style = format!("display:flex;gap:{gap}px");
        out.open_attrs("div", &[("style", &style)]);
        for child in children {
            out.open_attrs("div", &[("style", "flex:1")]);
            ctx.render_child(child, out)?;
            out.close("div");
        }
        out.close("div");
        Ok(())
    }
}

/// Ordered or unordered list wrapper. Each child becomes a list item.
pub struct ListBlock {
    pub id: ComponentId,
}

impl Block for ListBlock {
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
    fn variants(&self) -> Vec<VariantAxis> {
        variant_presets::list()
    }
    fn render_slint(
        &self,
        ctx: &RenderSlintContext<'_>,
        props: &Value,
        children: &[Node],
        out: &mut SlintEmitter,
    ) -> Result<(), RenderError> {
        let p = schemas::ListProps::from_value(props);
        out.block("VerticalLayout", |out| {
            out.prop_px("spacing", p.item_spacing as f64);
            out.line("alignment: start;");
            ctx.render_children(children, out)
        })
    }
    fn render_html(
        &self,
        ctx: &HtmlRenderContext<'_>,
        props: &Value,
        children: &[Node],
        out: &mut Html,
    ) -> Result<(), RenderError> {
        let p = schemas::ListProps::from_value(props);
        let tag = if p.ordered { "ol" } else { "ul" };
        out.open(tag);
        for child in children {
            out.open("li");
            ctx.render_child(child, out)?;
            out.close("li");
        }
        out.close(tag);
        Ok(())
    }
}

/// Simple data table with header columns and optional caption.
pub struct TableBlock {
    pub id: ComponentId,
}

impl Block for TableBlock {
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
    fn variants(&self) -> Vec<VariantAxis> {
        variant_presets::table()
    }
    fn render_slint(
        &self,
        _ctx: &RenderSlintContext<'_>,
        props: &Value,
        _children: &[Node],
        out: &mut SlintEmitter,
    ) -> Result<(), RenderError> {
        let p = schemas::TableProps::from_value(props);
        out.block("Rectangle", |out| {
            out.line("border-width: 1px;");
            out.line("border-color: #3b4252;");
            out.line("border-radius: 4px;");
            out.block("VerticalLayout", |out| {
                out.prop_px("padding", 8.0);
                out.prop_px("spacing", 4.0);
                if !p.caption.is_empty() {
                    out.block("Text", |out| {
                        out.prop_string("text", &p.caption);
                        out.prop_px("font-size", 12.0);
                        out.line("color: #9ca4b4;");
                        Ok(())
                    })?;
                }
                out.block("HorizontalLayout", |out| {
                    out.prop_px("spacing", 16.0);
                    for col in p.headers.split(',') {
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
    fn render_html(
        &self,
        _ctx: &HtmlRenderContext<'_>,
        props: &Value,
        _children: &[Node],
        out: &mut Html,
    ) -> Result<(), RenderError> {
        let p = schemas::TableProps::from_value(props);
        out.open("table");
        if !p.caption.is_empty() {
            out.open("caption");
            out.text(&p.caption);
            out.close("caption");
        }
        out.open("thead");
        out.open("tr");
        for col in p.headers.split(',') {
            let col = col.trim();
            if !col.is_empty() {
                out.open("th");
                out.text(col);
                out.close("th");
            }
        }
        out.close("tr");
        out.close("thead");
        out.close("table");
        Ok(())
    }
}

/// Tabbed content container. Children map to tab panels; the `labels`
/// prop names each panel.
pub struct TabsBlock {
    pub id: ComponentId,
}

impl Block for TabsBlock {
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
    fn variants(&self) -> Vec<VariantAxis> {
        variant_presets::tabs()
    }
    fn render_slint(
        &self,
        ctx: &RenderSlintContext<'_>,
        props: &Value,
        children: &[Node],
        out: &mut SlintEmitter,
    ) -> Result<(), RenderError> {
        let p = schemas::TabsProps::from_value(props);
        out.block("VerticalLayout", |out| {
            out.prop_px("spacing", 0.0);
            out.block("HorizontalLayout", |out| {
                out.prop_px("spacing", 0.0);
                for (i, label) in p.labels.split(',').enumerate() {
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
    fn render_html(
        &self,
        ctx: &HtmlRenderContext<'_>,
        props: &Value,
        children: &[Node],
        out: &mut Html,
    ) -> Result<(), RenderError> {
        let p = schemas::TabsProps::from_value(props);
        let tab_labels: Vec<&str> = p
            .labels
            .split(',')
            .map(|l| l.trim())
            .filter(|l| !l.is_empty())
            .collect();
        out.open_attrs("div", &[("role", "tablist")]);
        for (i, label) in tab_labels.iter().enumerate() {
            let selected = if i == 0 { "true" } else { "false" };
            out.open_attrs("button", &[("role", "tab"), ("aria-selected", selected)]);
            out.text(label);
            out.close("button");
        }
        out.close("div");
        for (i, child) in children.iter().take(tab_labels.len()).enumerate() {
            if i == 0 {
                out.open_attrs("div", &[("role", "tabpanel")]);
            } else {
                out.open_attrs("div", &[("role", "tabpanel"), ("hidden", "true")]);
            }
            ctx.render_child(child, out)?;
            out.close("div");
        }
        Ok(())
    }
}

/// Collapsible section with a title. Renders as `<details>/<summary>` in HTML.
pub struct AccordionBlock {
    pub id: ComponentId,
}

impl Block for AccordionBlock {
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
    fn variants(&self) -> Vec<VariantAxis> {
        variant_presets::accordion()
    }
    fn render_slint(
        &self,
        ctx: &RenderSlintContext<'_>,
        props: &Value,
        children: &[Node],
        out: &mut SlintEmitter,
    ) -> Result<(), RenderError> {
        let p = schemas::AccordionProps::from_value(props);
        out.block("VerticalLayout", |out| {
            out.prop_px("spacing", p.section_gap as f64);
            out.block("Rectangle", |out| {
                out.prop_px("height", 32.0);
                out.line("background: #2e3440;");
                out.line("border-radius: 4px;");
                if p.border_width > 0 {
                    out.prop_px("border-width", p.border_width as f64);
                    out.prop_color("border-color", &p.border_color);
                }
                out.block("Text", |out| {
                    let display = format!("▸ {}", p.title);
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
    fn render_html(
        &self,
        ctx: &HtmlRenderContext<'_>,
        props: &Value,
        children: &[Node],
        out: &mut Html,
    ) -> Result<(), RenderError> {
        let p = schemas::AccordionProps::from_value(props);
        if p.open {
            out.open_attrs("details", &[("open", "open")]);
        } else {
            out.open("details");
        }
        out.open("summary");
        out.text(&p.title);
        out.close("summary");
        ctx.render_children(children, out)?;
        out.close("details");
        Ok(())
    }
}

/// HTML `<button>`. Renders as `<button type="submit">text</button>`.
pub struct ButtonBlock {
    pub id: ComponentId,
}

impl Block for ButtonBlock {
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
        variant_presets::button()
    }
    fn render_slint(
        &self,
        _ctx: &RenderSlintContext<'_>,
        props: &Value,
        _children: &[Node],
        out: &mut SlintEmitter,
    ) -> Result<(), RenderError> {
        let p = schemas::ButtonProps::from_value(props);
        out.block("Rectangle", |out| {
            out.prop_px("height", 36.0);
            out.prop_px("min-width", 80.0);
            out.line("background: #3b82f6;");
            out.line("border-radius: 6px;");
            out.block("HorizontalLayout", |out| {
                out.line("alignment: center;");
                out.prop_px("spacing", 4.0);
                if !p.href.is_empty() {
                    out.block("Text", |out| {
                        out.prop_string("text", "🔗");
                        out.prop_px("font-size", 12.0);
                        out.line("vertical-alignment: center;");
                        Ok(())
                    })?;
                }
                out.block("Text", |out| {
                    out.prop_string("text", &p.text);
                    out.prop_px("font-size", 14.0);
                    out.line("color: #ffffff;");
                    out.line("font-weight: 600;");
                    out.line("vertical-alignment: center;");
                    Ok(())
                })
            })
        })
    }
    fn render_html(
        &self,
        _ctx: &HtmlRenderContext<'_>,
        props: &Value,
        _children: &[Node],
        out: &mut Html,
    ) -> Result<(), RenderError> {
        let p = schemas::ButtonProps::from_value(props);
        if !p.href.is_empty() {
            out.open_attrs("a", &[("href", p.href.as_str()), ("role", "button")]);
            out.text(&p.text);
            out.close("a");
        } else {
            let mut attrs = vec![("type", p.r#type.as_str())];
            if p.disabled {
                attrs.push(("disabled", "disabled"));
            }
            out.open_attrs("button", &attrs);
            out.text(&p.text);
            out.close("button");
        }
        Ok(())
    }
}

/// Interactive node-and-edge graph visualization. Renders nodes as
/// positioned circles on a canvas with label text.
pub struct GraphViewBlock {
    pub id: ComponentId,
}

impl Block for GraphViewBlock {
    fn id(&self) -> &ComponentId {
        &self.id
    }
    fn schema(&self) -> Vec<FieldSpec> {
        schemas::graph_view()
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
        let p = schemas::GraphViewProps::from_value(props);
        let node_size = p.node_size as f64;
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
                        .get(p.node_label_field.as_str())
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
        let mut html = HtmlRegistry::new();
        register_builtins(&mut reg, &mut html).expect("register builtins");
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
        let comp = TextBlock { id: "text".into() };
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
        let comp = DividerBlock {
            id: "divider".into(),
        };
        assert!(crate::block::Block::schema(&comp).is_empty());
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
        let comp = GraphViewBlock {
            id: "graph-view".into(),
        };
        assert_eq!(comp.id(), "graph-view");
    }

    #[test]
    fn graph_view_schema_has_layout_options() {
        let comp = GraphViewBlock {
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
        let comp = GraphViewBlock {
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
