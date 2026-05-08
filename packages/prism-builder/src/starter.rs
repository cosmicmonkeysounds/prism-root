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
use crate::facet::FacetComponent;
use crate::prefab::{ExposedSlot, PrefabComponent, PrefabDef};
use crate::registry::{ComponentRegistry, FieldSpec, RegistryError};
use crate::schemas;
use crate::signal::{with_common_signals, SignalDef};
use crate::slint_source::{escape_slint_string, SlintEmitter};
use crate::style::StyleProperties;
use crate::variant::{presets as variant_presets, VariantAxis};

/// Register the starter catalog. Single source of truth for the
/// built-in block list — every entry is one [`Block`] impl, so the
/// Slint DSL emit path and the unified Taffy/SSR pipeline stay in
/// lockstep by construction.
pub fn register_builtins(components: &mut ComponentRegistry) -> Result<(), RegistryError> {
    macro_rules! reg {
        ($id:literal, $ty:ident) => {
            register_block(components, Arc::new($ty { id: $id.into() }))?;
        };
    }
    reg!("text", TextBlock);
    reg!("image", ImageBlock);
    reg!("container", ContainerBlock);
    reg!("form", FormBlock);
    reg!("input", InputBlock);
    reg!("button", ButtonBlock);
    reg!("code", CodeBlock);
    reg!("divider", DividerBlock);
    reg!("spacer", SpacerBlock);
    reg!("columns", ColumnsBlock);
    reg!("list", ListBlock);
    reg!("table", TableBlock);
    reg!("tabs", TabsBlock);
    reg!("accordion", AccordionBlock);

    // `card` is a prefab.
    components.register(Arc::new(PrefabComponent::new(card_prefab_def())))?;

    // `facet` is a one-off Component impl.
    components.register(Arc::new(FacetComponent::new()))?;

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
    fn lower_ui(
        &self,
        _ctx: &crate::ui_lower::LowerCtx<'_>,
        node: &Node,
        style: &StyleProperties,
    ) -> prism_ui_runtime::layout::Node {
        let p = schemas::TextProps::from_value(&node.props);
        // `TextProps` is the schema authored by the Studio panel. Old
        // `BuilderDocument`s authored before this schema may carry the
        // text under `text`/`content` instead of `body`; honour both
        // so legacy fixtures keep round-tripping.
        let content = if !p.body.is_empty() {
            p.body
        } else {
            node.props
                .get("text")
                .or_else(|| node.props.get("content"))
                .and_then(|v| v.as_str())
                .map(str::to_owned)
                .unwrap_or_default()
        };
        let default_size = level_font_size(p.level.as_str()) as f32;
        let leaf = crate::ui_lower::text_node(node.id.clone(), content, style, default_size);
        // Level → tag is a single mapping shared by HTML SSR and
        // (eventually) the inspector. `paragraph` → `<p>`; explicit
        // h1-h6 round-trip; anything else falls through to the
        // walker's font-size bucketing default.
        let level_tag = level_to_html_tag(p.level.as_str());
        // When `href` is set, the legacy SSR walker wraps the inner
        // text in `<a href="…">` inside the heading/paragraph tag.
        // Match that shape: heading container → anchored text leaf.
        // Single source of truth for the `Text.href` lowering rule.
        if !p.href.is_empty() {
            let anchored = crate::ui_lower::with_semantic(
                leaf,
                prism_ui_runtime::layout::Semantic::tag("a").with_attr("href", p.href.clone()),
            );
            return crate::ui_lower::bare_container(
                format!("{}-wrap", node.id),
                vec![anchored],
                |props| {
                    props.semantic = prism_ui_runtime::layout::Semantic::tag(level_tag);
                },
            );
        }
        crate::ui_lower::with_semantic(leaf, prism_ui_runtime::layout::Semantic::tag(level_tag))
    }
}

/// Single source of truth for `Text.level` → HTML tag. `h1`-`h6`
/// round-trip; anything else (including the schema default
/// `"paragraph"`) becomes `<p>`. Adding a new level is one match arm.
fn level_to_html_tag(level: &str) -> &'static str {
    match level {
        "h1" => "h1",
        "h2" => "h2",
        "h3" => "h3",
        "h4" => "h4",
        "h5" => "h5",
        "h6" => "h6",
        _ => "p",
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
    fn lower_ui(
        &self,
        _ctx: &crate::ui_lower::LowerCtx<'_>,
        node: &Node,
        style: &StyleProperties,
    ) -> prism_ui_runtime::layout::Node {
        // Resolve VFS / URL / external sources the same way the SSR
        // walker does — `to_html_src()` is the single source of truth
        // for "what string does the renderer get". Native backends
        // round-trip the same string and resolve it locally.
        let source = node
            .props
            .get("src")
            .and_then(AssetSource::from_prop)
            .map(|s| s.to_html_src())
            .unwrap_or_default();
        // Images grow into their slot (matches `render_slint`'s
        // `width: parent.width; height: parent.height`).
        let img = crate::ui_lower::image_node(
            node.id.clone(),
            source,
            style,
            prism_ui_runtime::layout::Sizing::Grow,
            prism_ui_runtime::layout::Sizing::Grow,
        );
        // Pull the `alt` prop (HTML accessibility requirement) onto
        // the semantic hint. The walker emits it verbatim as an
        // attribute on the `<img>` tag — same lowering the SSR
        // walker had hand-coded. Single source of truth for image
        // SEO/accessibility now lives here.
        let alt = node.props.get("alt").and_then(|v| v.as_str()).unwrap_or("");
        let mut hint = prism_ui_runtime::layout::Semantic::default();
        if !alt.is_empty() {
            hint = hint.with_attr("alt", alt);
        }
        crate::ui_lower::with_semantic(img, hint)
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
    fn lower_ui(
        &self,
        ctx: &crate::ui_lower::LowerCtx<'_>,
        node: &Node,
        style: &StyleProperties,
    ) -> prism_ui_runtime::layout::Node {
        let p = schemas::ContainerProps::from_value(&node.props);
        let container = ctx.container_with(node, style, |props| {
            // Schema-authored values fill in for any field the flow
            // pass left at zero. Flow props (when present) win — that's
            // the layout-engine path; the schema is the legacy / no-flow
            // fallback.
            if props.gap == 0.0 {
                props.gap = p.spacing as f32;
            }
            if p.padding > 0
                && props.padding.left == 0.0
                && props.padding.right == 0.0
                && props.padding.top == 0.0
                && props.padding.bottom == 0.0
            {
                props.padding = prism_ui_runtime::layout::Padding::all(p.padding as f32);
            }
        });
        // ContainerBlock is "Semantic `<section>` wrapper" per its
        // doc comment — same tag the legacy `render_html` emits. The
        // class survives for downstream styling.
        crate::ui_lower::with_semantic(
            container,
            prism_ui_runtime::layout::Semantic::tag("section"),
        )
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
    fn lower_ui(
        &self,
        ctx: &crate::ui_lower::LowerCtx<'_>,
        node: &Node,
        style: &StyleProperties,
    ) -> prism_ui_runtime::layout::Node {
        // Vertical container, gap 8 — matches `render_slint`. Children
        // (inputs / buttons / etc.) are walked by `container_with`.
        let p = schemas::FormProps::from_value(&node.props);
        let container = ctx.container_with(node, style, |props| {
            if props.gap == 0.0 {
                props.gap = 8.0;
            }
        });
        // SSR semantic: `<form>` with method + optional action.
        // Same attribute set the prior `render_html` walker emitted.
        let mut hint =
            prism_ui_runtime::layout::Semantic::tag("form").with_attr("method", p.method.as_str());
        if !p.action.is_empty() {
            hint = hint.with_attr("action", p.action.as_str());
        }
        crate::ui_lower::with_semantic(container, hint)
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
    fn lower_ui(
        &self,
        ctx: &crate::ui_lower::LowerCtx<'_>,
        node: &Node,
        style: &StyleProperties,
    ) -> prism_ui_runtime::layout::Node {
        use crate::ui_lower::{bare_container, parse_color, text_node, uniform_radius};
        let p = schemas::InputProps::from_value(&node.props);

        let mut sections = Vec::with_capacity(2);
        if !p.label.is_empty() {
            sections.push(text_node(
                format!("{}-label", node.id),
                p.label.clone(),
                style,
                12.0,
            ));
        }
        let display = if p.placeholder.is_empty() {
            "...".to_string()
        } else {
            p.placeholder.clone()
        };
        // Placeholder is dim grey regardless of cascade — same "muted"
        // colour render_slint hard-codes.
        let mut placeholder_style = style.clone();
        placeholder_style.color = Some("#6b7280".into());
        let placeholder = text_node(
            format!("{}-placeholder", node.id),
            display,
            &placeholder_style,
            14.0,
        );
        // SSR: the field itself is a real `<input>` — void tag, attrs
        // declared once, layout children dropped by the walker. Order
        // matches `render_html`: type, name, optional placeholder,
        // optional value, optional required.
        let mut input_hint = prism_ui_runtime::layout::Semantic::tag("input")
            .with_attr("type", p.r#type.clone())
            .with_attr("name", p.name.clone());
        if !p.placeholder.is_empty() {
            input_hint = input_hint.with_attr("placeholder", p.placeholder.clone());
        }
        if !p.value.is_empty() {
            input_hint = input_hint.with_attr("value", p.value.clone());
        }
        if p.required {
            input_hint = input_hint.with_attr("required", "required");
        }
        let field = bare_container(format!("{}-field", node.id), vec![placeholder], |fp| {
            fp.height = prism_ui_runtime::layout::Sizing::Fixed(32.0);
            fp.background = parse_color("#1e2533");
            fp.radius = uniform_radius(4.0);
            fp.padding = prism_ui_runtime::layout::Padding {
                left: 8.0,
                right: 8.0,
                top: 0.0,
                bottom: 0.0,
            };
            fp.semantic = input_hint;
        });
        sections.push(field);

        // Outer wrapper is a `<label>` when a label string was authored
        // (matches `render_html`). Without one, fall back to a plain
        // `<div>` group — `<label>` with no text is bad accessibility.
        let outer_semantic = if p.label.is_empty() {
            prism_ui_runtime::layout::Semantic::default()
        } else {
            prism_ui_runtime::layout::Semantic::tag("label")
        };
        ctx.synthetic_container(node, style, sections, |props| {
            if props.gap == 0.0 {
                props.gap = 4.0;
            }
            props.semantic = outer_semantic;
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
    fn lower_ui(
        &self,
        ctx: &crate::ui_lower::LowerCtx<'_>,
        node: &Node,
        style: &StyleProperties,
    ) -> prism_ui_runtime::layout::Node {
        let p = schemas::CodeProps::from_value(&node.props);
        // Resolution order matches `render_slint`: explicit prop →
        // cascade → built-in default. Single source of truth for those
        // defaults lives here so the runtime and Slint stay in sync.
        let bg = if !p.bg.is_empty() {
            crate::ui_lower::parse_color(&p.bg)
        } else if let Some(s) = style.background.as_deref() {
            crate::ui_lower::parse_color(s)
        } else {
            crate::ui_lower::parse_color("#1a1e28")
        };
        let color = if !p.color.is_empty() {
            p.color.clone()
        } else if let Some(s) = style.color.clone() {
            s
        } else {
            "#a3be8c".into()
        };
        let mut text_style = style.clone();
        text_style.color = Some(color);
        // Inner `<code>` (with optional `language-xxx` class), wrapped
        // in the outer `<pre>` declared on the synthetic container.
        // Same semantic shape `render_html` produces.
        let mut code_hint = prism_ui_runtime::layout::Semantic::tag("code");
        if !p.language.is_empty() {
            code_hint = code_hint.with_class(format!("language-{}", p.language));
        }
        let text = crate::ui_lower::with_semantic(
            crate::ui_lower::text_node(
                format!("{}-text", node.id),
                p.code.clone(),
                &text_style,
                13.0,
            ),
            code_hint,
        );
        ctx.synthetic_container(node, style, vec![text], |props| {
            if props.background.is_none() {
                props.background = bg;
            }
            if props.radius.tl == 0.0 {
                props.radius = crate::ui_lower::uniform_radius(style.border_radius.unwrap_or(6.0));
            }
            if props.padding.left == 0.0
                && props.padding.right == 0.0
                && props.padding.top == 0.0
                && props.padding.bottom == 0.0
            {
                props.padding = prism_ui_runtime::layout::Padding::all(12.0);
            }
            props.semantic = prism_ui_runtime::layout::Semantic::tag("pre");
        })
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
    fn lower_ui(
        &self,
        ctx: &crate::ui_lower::LowerCtx<'_>,
        node: &Node,
        style: &StyleProperties,
    ) -> prism_ui_runtime::layout::Node {
        // 1px-tall stroke. Cascade colour wins; `#3b4252` is the
        // historic default both render paths fall back on.
        ctx.synthetic_container(node, style, vec![], |props| {
            props.height = prism_ui_runtime::layout::Sizing::Fixed(1.0);
            if props.background.is_none() {
                props.background = Some(crate::ui_lower::parse_color("#3b4252").unwrap());
            }
            // SSR: `<hr>` is void; the walker drops children + inline
            // styles for it, leaving the browser's default rule.
            props.semantic = prism_ui_runtime::layout::Semantic::tag("hr");
        })
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
    fn lower_ui(
        &self,
        _ctx: &crate::ui_lower::LowerCtx<'_>,
        node: &Node,
        _style: &StyleProperties,
    ) -> prism_ui_runtime::layout::Node {
        let p = schemas::SpacerProps::from_value(&node.props);
        // Schema only carries `height`; legacy `width` prop on raw
        // documents is honoured as a fallback so existing fixtures
        // produce the same Spacer they did pre-migration.
        let width = node
            .props
            .get("width")
            .and_then(|v| v.as_f64())
            .map(|f| f as f32)
            .unwrap_or(0.0);
        crate::ui_lower::spacer_node(node.id.clone(), width, p.height as f32)
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
    fn lower_ui(
        &self,
        ctx: &crate::ui_lower::LowerCtx<'_>,
        node: &Node,
        style: &StyleProperties,
    ) -> prism_ui_runtime::layout::Node {
        let p = schemas::ColumnsProps::from_value(&node.props);
        ctx.container_with(node, style, |props| {
            props.direction = prism_ui_runtime::layout::Direction::Row;
            if props.gap == 0.0 {
                props.gap = p.gap as f32;
            }
        })
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
    fn lower_ui(
        &self,
        ctx: &crate::ui_lower::LowerCtx<'_>,
        node: &Node,
        style: &StyleProperties,
    ) -> prism_ui_runtime::layout::Node {
        let p = schemas::ListProps::from_value(&node.props);
        let container = ctx.container_with(node, style, |props| {
            // Lists are vertical by default — column direction is the
            // builder default, so nothing to set there. Just plumb the
            // schema-authored item spacing through if the flow props
            // didn't set their own.
            if props.gap == 0.0 {
                props.gap = p.item_spacing as f32;
            }
        });
        // `<ul>`/`<ol>` semantic. `<li>` wrappers around children
        // would require child-mutation here; current SSR output puts
        // children inline (good enough for Phase A1 — proper `<li>`
        // wrapping comes when children carry their own list-item
        // hint or the walker grows variant-specific child rules).
        let tag = if p.ordered { "ol" } else { "ul" };
        crate::ui_lower::with_semantic(container, prism_ui_runtime::layout::Semantic::tag(tag))
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
    fn lower_ui(
        &self,
        ctx: &crate::ui_lower::LowerCtx<'_>,
        node: &Node,
        style: &StyleProperties,
    ) -> prism_ui_runtime::layout::Node {
        use crate::ui_lower::{bare_container, text_node, uniform_radius};
        let p = schemas::TableProps::from_value(&node.props);

        let mut sections: Vec<prism_ui_runtime::layout::Node> = Vec::new();
        if !p.caption.is_empty() {
            // Slint paints caption #9ca4b4; let cascade override.
            let mut caption_style = style.clone();
            if caption_style.color.is_none() {
                caption_style.color = Some("#9ca4b4".into());
            }
            sections.push(crate::ui_lower::with_semantic(
                text_node(
                    format!("{}-caption", node.id),
                    p.caption.clone(),
                    &caption_style,
                    12.0,
                ),
                prism_ui_runtime::layout::Semantic::tag("caption"),
            ));
        }
        // SSR: each header cell is a `<th>` text node, the header row
        // is `<tr>`, the whole strip lives inside `<thead>` so the
        // outer `<table>` ends up `<table><caption>?<thead><tr><th>…`
        // — same shape `render_html` produces, declared once here.
        let header_cells: Vec<_> = p
            .headers
            .split(',')
            .filter_map(|c| {
                let c = c.trim();
                (!c.is_empty()).then(|| {
                    crate::ui_lower::with_semantic(
                        text_node(format!("{}-h-{}", node.id, c), c.to_string(), style, 13.0),
                        prism_ui_runtime::layout::Semantic::tag("th"),
                    )
                })
            })
            .collect();
        let header_row = bare_container(format!("{}-headers", node.id), header_cells, |fp| {
            fp.direction = prism_ui_runtime::layout::Direction::Row;
            fp.gap = 16.0;
            fp.semantic = prism_ui_runtime::layout::Semantic::tag("tr");
        });
        sections.push(bare_container(
            format!("{}-thead", node.id),
            vec![header_row],
            |fp| {
                fp.semantic = prism_ui_runtime::layout::Semantic::tag("thead");
            },
        ));

        ctx.synthetic_container(node, style, sections, |props| {
            if props.gap == 0.0 {
                props.gap = 4.0;
            }
            if props.padding.left == 0.0
                && props.padding.right == 0.0
                && props.padding.top == 0.0
                && props.padding.bottom == 0.0
            {
                props.padding = prism_ui_runtime::layout::Padding::all(8.0);
            }
            if props.radius.tl == 0.0 {
                props.radius = uniform_radius(style.border_radius.unwrap_or(4.0));
            }
            props.semantic = prism_ui_runtime::layout::Semantic::tag("table");
        })
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
    fn lower_ui(
        &self,
        ctx: &crate::ui_lower::LowerCtx<'_>,
        node: &Node,
        style: &StyleProperties,
    ) -> prism_ui_runtime::layout::Node {
        use crate::ui_lower::{bare_container, parse_color, text_node};
        let p = schemas::TabsProps::from_value(&node.props);

        // SSR shape mirrors `render_html`: the pill strip is a
        // `<div role="tablist">` of `<button role="tab">` items, the
        // panel host is `<div role="tabpanel">` (the current tab).
        // First pill / first panel get `aria-selected="true"`; the
        // rest are inactive. Hidden state on later panels stays a
        // free-form attr.
        let pills: Vec<_> = p
            .labels
            .split(',')
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .enumerate()
            .map(|(i, label)| {
                let bg = if i == 0 { "#2e3440" } else { "#1a1e28" };
                let selected = if i == 0 { "true" } else { "false" };
                bare_container(
                    format!("{}-tab-{}", node.id, i),
                    vec![text_node(
                        format!("{}-tab-{}-label", node.id, i),
                        label.to_string(),
                        style,
                        13.0,
                    )],
                    |fp| {
                        fp.height = prism_ui_runtime::layout::Sizing::Fixed(32.0);
                        fp.background = parse_color(bg);
                        fp.padding = prism_ui_runtime::layout::Padding {
                            left: 12.0,
                            right: 12.0,
                            top: 0.0,
                            bottom: 0.0,
                        };
                        fp.semantic = prism_ui_runtime::layout::Semantic::tag("button")
                            .with_role("tab")
                            .with_attr("aria-selected", selected);
                    },
                )
            })
            .collect();

        let strip = bare_container(format!("{}-strip", node.id), pills, |fp| {
            fp.direction = prism_ui_runtime::layout::Direction::Row;
            fp.semantic = prism_ui_runtime::layout::Semantic::default().with_role("tablist");
        });
        let panel = bare_container(
            format!("{}-panel", node.id),
            ctx.lower_children(&node.children),
            |fp| {
                fp.padding = prism_ui_runtime::layout::Padding::all(12.0);
                fp.gap = 8.0;
                fp.semantic = prism_ui_runtime::layout::Semantic::default().with_role("tabpanel");
            },
        );
        ctx.synthetic_container(node, style, vec![strip, panel], |_| {})
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
    fn lower_ui(
        &self,
        ctx: &crate::ui_lower::LowerCtx<'_>,
        node: &Node,
        style: &StyleProperties,
    ) -> prism_ui_runtime::layout::Node {
        use crate::ui_lower::{bare_container, parse_color, text_node, uniform_radius};
        let p = schemas::AccordionProps::from_value(&node.props);

        // SSR: `<details><summary>title</summary>…children…</details>`,
        // matching `render_html`. The title wrapper becomes `<summary>`,
        // the content host stays a default `<div>` so children's own
        // semantics flow through unchanged.
        let header = bare_container(
            format!("{}-header", node.id),
            vec![text_node(
                format!("{}-title", node.id),
                format!("▸ {}", p.title),
                style,
                14.0,
            )],
            |fp| {
                fp.height = prism_ui_runtime::layout::Sizing::Fixed(32.0);
                fp.background = parse_color("#2e3440");
                fp.radius = uniform_radius(4.0);
                fp.padding = prism_ui_runtime::layout::Padding {
                    left: 12.0,
                    right: 12.0,
                    top: 0.0,
                    bottom: 0.0,
                };
                fp.semantic = prism_ui_runtime::layout::Semantic::tag("summary");
            },
        );
        let content = bare_container(
            format!("{}-content", node.id),
            ctx.lower_children(&node.children),
            |fp| {
                fp.padding = prism_ui_runtime::layout::Padding {
                    left: 16.0,
                    right: 0.0,
                    top: 0.0,
                    bottom: 0.0,
                };
                fp.gap = 8.0;
            },
        );

        ctx.synthetic_container(node, style, vec![header, content], |props| {
            if props.gap == 0.0 {
                props.gap = p.section_gap as f32;
            }
            let mut hint = prism_ui_runtime::layout::Semantic::tag("details");
            if p.open {
                hint = hint.with_attr("open", "open");
            }
            props.semantic = hint;
        })
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
    fn lower_ui(
        &self,
        ctx: &crate::ui_lower::LowerCtx<'_>,
        node: &Node,
        style: &StyleProperties,
    ) -> prism_ui_runtime::layout::Node {
        let p = schemas::ButtonProps::from_value(&node.props);
        // Mirrors render_slint: blue rect, white label, fixed height.
        // Cascade still wins so theme overrides drop straight in.
        let mut label_style = style.clone();
        if label_style.color.is_none() {
            label_style.color = Some("#ffffff".into());
        }
        let label = crate::ui_lower::text_node(
            format!("{}-label", node.id),
            p.text.clone(),
            &label_style,
            14.0,
        );
        ctx.synthetic_container(node, style, vec![label], |props| {
            if props.background.is_none() {
                props.background = crate::ui_lower::parse_color("#3b82f6");
            }
            if props.radius.tl == 0.0 {
                props.radius = crate::ui_lower::uniform_radius(style.border_radius.unwrap_or(6.0));
            }
            if props.padding.left == 0.0 && props.padding.right == 0.0 {
                props.padding = prism_ui_runtime::layout::Padding {
                    left: 12.0,
                    right: 12.0,
                    top: 8.0,
                    bottom: 8.0,
                };
            }
            if matches!(props.height, prism_ui_runtime::layout::Sizing::Fit) {
                props.height = prism_ui_runtime::layout::Sizing::Fixed(36.0);
            }
            // SSR semantic: `<a href>` when the prop is set (matches
            // `render_html`'s anchor branch), `<button type="…">`
            // otherwise. Disabled state propagates as a free-form attr.
            props.semantic = if !p.href.is_empty() {
                prism_ui_runtime::layout::Semantic::tag("a")
                    .with_attr("href", p.href.clone())
                    .with_role("button")
            } else {
                let mut s = prism_ui_runtime::layout::Semantic::tag("button")
                    .with_attr("type", p.r#type.clone());
                if p.disabled {
                    s = s.with_attr("disabled", "disabled");
                }
                s
            };
        })
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
