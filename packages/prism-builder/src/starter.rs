//! Starter component catalog — the default block registry.
//!
//! Seventeen blocks land here, declared as a single `BUILTINS` table of
//! `BuiltinSpec` rows. Each row carries `(id, schema, help, signals,
//! variants, lower)`; one [`BuiltinBlock`] type implements `Block` by
//! delegating to its spec. There are no per-block trait impls — adding
//! a new block is one `const SPEC` and one entry in the table.
//!
//! `card` is a prefab, `facet` is a one-off `Component`, and the
//! remaining 15 blocks (text, image, container, form, input, button,
//! code, divider, spacer, columns, list, table, tabs, accordion,
//! graph-view) flow through `BuiltinBlock` + `BuiltinSpec`.

use std::sync::Arc;

use prism_core::help::HelpEntry;
use serde_json::json;

use crate::asset::AssetSource;
use crate::block::{register_block, Block};
use crate::component::ComponentId;
use crate::document::Node;
use crate::facet::FacetComponent;
use crate::prefab::{ExposedSlot, PrefabComponent, PrefabDef};
use crate::registry::{ComponentRegistry, FieldSpec, RegistryError};
use crate::schemas;
use crate::signal::{with_common_signals, SignalDef};
use crate::style::StyleProperties;
use crate::ui_lower::{
    bare_container, parse_color, spacer_node, text_node, uniform_radius, with_semantic, LowerCtx,
};
use crate::variant::{presets as variant_presets, VariantAxis};

use prism_ui_runtime::layout::{self as ui, Direction, Padding, Semantic, Sizing};

// ── Declarative builtin spec ────────────────────────────────────────

/// One row per built-in block. `id` / `schema` / `lower` are always
/// present; `help` / `signals` / `variants` default to `None` /
/// "common signals only" / "no variant axes".
///
/// Adding a builtin = one `const SPEC` + one row in [`BUILTINS`].
pub struct BuiltinSpec {
    pub id: &'static str,
    pub schema: fn() -> Vec<FieldSpec>,
    pub help: Option<HelpDef>,
    pub signals: fn() -> Vec<SignalDef>,
    pub variants: fn() -> Vec<VariantAxis>,
    pub lower: LowerFn,
}

pub struct HelpDef {
    pub key: &'static str,
    pub title: &'static str,
    pub description: &'static str,
}

pub type LowerFn = fn(&LowerCtx<'_>, &Node, &StyleProperties) -> ui::Node;

fn default_signals() -> Vec<SignalDef> {
    with_common_signals(vec![])
}
fn no_variants() -> Vec<VariantAxis> {
    vec![]
}
fn default_lower(ctx: &LowerCtx<'_>, node: &Node, style: &StyleProperties) -> ui::Node {
    ctx.default_container(node, style)
}

impl BuiltinSpec {
    pub const fn new(id: &'static str, schema: fn() -> Vec<FieldSpec>, lower: LowerFn) -> Self {
        Self {
            id,
            schema,
            help: None,
            signals: default_signals,
            variants: no_variants,
            lower,
        }
    }
    pub const fn help(
        mut self,
        key: &'static str,
        title: &'static str,
        description: &'static str,
    ) -> Self {
        self.help = Some(HelpDef {
            key,
            title,
            description,
        });
        self
    }
    pub const fn signals(mut self, f: fn() -> Vec<SignalDef>) -> Self {
        self.signals = f;
        self
    }
    pub const fn variants(mut self, f: fn() -> Vec<VariantAxis>) -> Self {
        self.variants = f;
        self
    }
}

/// `Block` impl that delegates everything to a `&'static BuiltinSpec`.
pub struct BuiltinBlock {
    spec: &'static BuiltinSpec,
    id: ComponentId,
}

impl BuiltinBlock {
    pub fn new(spec: &'static BuiltinSpec) -> Self {
        Self {
            spec,
            id: spec.id.into(),
        }
    }
}

impl Block for BuiltinBlock {
    fn id(&self) -> &ComponentId {
        &self.id
    }
    fn schema(&self) -> Vec<FieldSpec> {
        (self.spec.schema)()
    }
    fn help_entry(&self) -> Option<HelpEntry> {
        self.spec
            .help
            .as_ref()
            .map(|h| HelpEntry::new(h.key, h.title, h.description))
    }
    fn signals(&self) -> Vec<SignalDef> {
        (self.spec.signals)()
    }
    fn variants(&self) -> Vec<VariantAxis> {
        (self.spec.variants)()
    }
    fn lower_ui(&self, ctx: &LowerCtx<'_>, node: &Node, style: &StyleProperties) -> ui::Node {
        (self.spec.lower)(ctx, node, style)
    }
}

/// Construct a fresh `Arc<BuiltinBlock>` for a builtin id, or `None` if
/// the id isn't in [`BUILTINS`]. The blanket `impl<T: Block> Component`
/// makes the result usable directly with `register_block` and
/// `ComponentRegistry::register`.
pub fn builtin_block(id: &str) -> Option<Arc<BuiltinBlock>> {
    BUILTINS
        .iter()
        .find(|s| s.id == id)
        .map(|spec| Arc::new(BuiltinBlock::new(spec)))
}

/// Register every entry in [`BUILTINS`] plus the `card` prefab and the
/// one-off `facet` component. The Slint DSL emit path is gone; the
/// unified Taffy/SSR pipeline is the single render path.
pub fn register_builtins(components: &mut ComponentRegistry) -> Result<(), RegistryError> {
    for spec in BUILTINS {
        register_block(components, Arc::new(BuiltinBlock::new(spec)))?;
    }
    components.register(Arc::new(PrefabComponent::new(card_prefab_def())))?;
    components.register(Arc::new(FacetComponent::new()))?;
    Ok(())
}

// ── Shared helpers ──────────────────────────────────────────────────

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

/// Single source of truth for `Text.level` → HTML tag. `h1`-`h6`
/// round-trip; anything else (including the schema default
/// `"paragraph"`) becomes `<p>`.
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

// ── text ────────────────────────────────────────────────────────────

fn text_lower(_ctx: &LowerCtx<'_>, node: &Node, style: &StyleProperties) -> ui::Node {
    let p = schemas::TextProps::from_value(&node.props);
    // Old `BuilderDocument`s authored before the `body` schema may
    // carry the text under `text` / `content`; honour both so legacy
    // fixtures keep round-tripping.
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
    let leaf = text_node(node.id.clone(), content, style, default_size);
    let level_tag = level_to_html_tag(p.level.as_str());
    if !p.href.is_empty() {
        let anchored = with_semantic(leaf, Semantic::tag("a").with_attr("href", p.href.clone()));
        return bare_container(format!("{}-wrap", node.id), vec![anchored], |props| {
            props.semantic = Semantic::tag(level_tag);
        });
    }
    with_semantic(leaf, Semantic::tag(level_tag))
}

fn text_signals() -> Vec<SignalDef> {
    with_common_signals(vec![SignalDef::new(
        "link-clicked",
        "Fires when a hyperlink in the text is clicked",
    )
    .with_payload(vec![FieldSpec::text("href", "Link URL")])])
}

const TEXT: BuiltinSpec = BuiltinSpec::new("text", schemas::text, text_lower)
    .help(
        "builder.components.text",
        "Text",
        "Text block — paragraph, heading, or link. Set level for heading sizes, href for hyperlinks.",
    )
    .signals(text_signals);

// ── image ───────────────────────────────────────────────────────────

fn image_lower(_ctx: &LowerCtx<'_>, node: &Node, style: &StyleProperties) -> ui::Node {
    let source = node
        .props
        .get("src")
        .and_then(AssetSource::from_prop)
        .map(|s| s.to_html_src())
        .unwrap_or_default();
    let img =
        crate::ui_lower::image_node(node.id.clone(), source, style, Sizing::Grow, Sizing::Grow);
    let alt = node.props.get("alt").and_then(|v| v.as_str()).unwrap_or("");
    let mut hint = Semantic::default();
    if !alt.is_empty() {
        hint = hint.with_attr("alt", alt);
    }
    with_semantic(img, hint)
}

fn image_signals() -> Vec<SignalDef> {
    with_common_signals(vec![SignalDef::new(
        "loaded",
        "Fires when the image finishes loading",
    )])
}

const IMAGE: BuiltinSpec = BuiltinSpec::new("image", schemas::image, image_lower)
    .help(
        "builder.components.image",
        "Image",
        "Embedded image. Upload from your device, pick from the current vault, or paste an external URL. Supports configurable object-fit.",
    )
    .signals(image_signals)
    .variants(variant_presets::image);

// ── container ───────────────────────────────────────────────────────

fn container_lower(ctx: &LowerCtx<'_>, node: &Node, style: &StyleProperties) -> ui::Node {
    let p = schemas::ContainerProps::from_value(&node.props);
    let container = ctx.container_with(node, style, |props| {
        if props.gap == 0.0 {
            props.gap = p.spacing as f32;
        }
        if p.padding > 0
            && props.padding.left == 0.0
            && props.padding.right == 0.0
            && props.padding.top == 0.0
            && props.padding.bottom == 0.0
        {
            props.padding = Padding::all(p.padding as f32);
        }
    });
    with_semantic(container, Semantic::tag("section"))
}

fn container_signals() -> Vec<SignalDef> {
    with_common_signals(vec![SignalDef::new(
        "child-added",
        "Fires when a child component is added",
    )
    .with_payload(vec![FieldSpec::text("child_id", "Added child node ID")])])
}

const CONTAINER: BuiltinSpec = BuiltinSpec::new("container", schemas::container, container_lower)
    .help(
        "builder.components.container",
        "Container",
        "Layout wrapper that groups child components with configurable spacing.",
    )
    .signals(container_signals)
    .variants(variant_presets::container);

// ── form ────────────────────────────────────────────────────────────

fn form_lower(ctx: &LowerCtx<'_>, node: &Node, style: &StyleProperties) -> ui::Node {
    let p = schemas::FormProps::from_value(&node.props);
    let container = ctx.container_with(node, style, |props| {
        if props.gap == 0.0 {
            props.gap = 8.0;
        }
    });
    let mut hint = Semantic::tag("form").with_attr("method", p.method.as_str());
    if !p.action.is_empty() {
        hint = hint.with_attr("action", p.action.as_str());
    }
    with_semantic(container, hint)
}

fn form_signals() -> Vec<SignalDef> {
    with_common_signals(vec![
        SignalDef::new("submitted", "Fires when the form is submitted"),
        SignalDef::new("validated", "Fires after form validation runs").with_payload(vec![
            FieldSpec::boolean("valid", "Whether validation passed"),
        ]),
    ])
}

const FORM: BuiltinSpec = BuiltinSpec::new("form", schemas::form, form_lower)
    .help(
        "builder.components.form",
        "Form",
        "HTML form wrapper. Nest input and button components inside to build forms.",
    )
    .signals(form_signals);

// ── input ───────────────────────────────────────────────────────────

fn input_lower(ctx: &LowerCtx<'_>, node: &Node, style: &StyleProperties) -> ui::Node {
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
    let mut placeholder_style = style.clone();
    placeholder_style.color = Some("#6b7280".into());
    let placeholder = text_node(
        format!("{}-placeholder", node.id),
        display,
        &placeholder_style,
        14.0,
    );
    let mut input_hint = Semantic::tag("input")
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
        fp.height = Sizing::Fixed(32.0);
        fp.background = parse_color("#1e2533");
        fp.radius = uniform_radius(4.0);
        fp.padding = Padding {
            left: 8.0,
            right: 8.0,
            top: 0.0,
            bottom: 0.0,
        };
        fp.semantic = input_hint;
    });
    sections.push(field);

    let outer_semantic = if p.label.is_empty() {
        Semantic::default()
    } else {
        Semantic::tag("label")
    };
    ctx.synthetic_container(node, style, sections, |props| {
        if props.gap == 0.0 {
            props.gap = 4.0;
        }
        props.semantic = outer_semantic;
    })
}

fn input_signals() -> Vec<SignalDef> {
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

const INPUT: BuiltinSpec = BuiltinSpec::new("input", schemas::input, input_lower)
    .help(
        "builder.components.input",
        "Input",
        "Text, email, or password field with placeholder and name binding.",
    )
    .signals(input_signals)
    .variants(variant_presets::input);

// ── button ──────────────────────────────────────────────────────────

fn button_lower(ctx: &LowerCtx<'_>, node: &Node, style: &StyleProperties) -> ui::Node {
    let p = schemas::ButtonProps::from_value(&node.props);
    let mut label_style = style.clone();
    if label_style.color.is_none() {
        label_style.color = Some("#ffffff".into());
    }
    let label = text_node(
        format!("{}-label", node.id),
        p.text.clone(),
        &label_style,
        14.0,
    );
    ctx.synthetic_container(node, style, vec![label], |props| {
        if props.background.is_none() {
            props.background = parse_color("#3b82f6");
        }
        if props.radius.tl == 0.0 {
            props.radius = uniform_radius(style.border_radius.unwrap_or(6.0));
        }
        if props.padding.left == 0.0 && props.padding.right == 0.0 {
            props.padding = Padding {
                left: 12.0,
                right: 12.0,
                top: 8.0,
                bottom: 8.0,
            };
        }
        if matches!(props.height, Sizing::Fit) {
            props.height = Sizing::Fixed(36.0);
        }
        props.semantic = if !p.href.is_empty() {
            Semantic::tag("a")
                .with_attr("href", p.href.clone())
                .with_role("button")
        } else {
            let mut s = Semantic::tag("button").with_attr("type", p.r#type.clone());
            if p.disabled {
                s = s.with_attr("disabled", "disabled");
            }
            s
        };
    })
}

const BUTTON: BuiltinSpec = BuiltinSpec::new("button", schemas::button, button_lower)
    .help(
        "builder.components.button",
        "Button",
        "Submit or action button with configurable text and disabled state.",
    )
    .variants(variant_presets::button);

// ── code ────────────────────────────────────────────────────────────

fn code_lower(ctx: &LowerCtx<'_>, node: &Node, style: &StyleProperties) -> ui::Node {
    let p = schemas::CodeProps::from_value(&node.props);
    let bg = if !p.bg.is_empty() {
        parse_color(&p.bg)
    } else if let Some(s) = style.background.as_deref() {
        parse_color(s)
    } else {
        parse_color("#1a1e28")
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
    let mut code_hint = Semantic::tag("code");
    if !p.language.is_empty() {
        code_hint = code_hint.with_class(format!("language-{}", p.language));
    }
    let text = with_semantic(
        text_node(
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
            props.radius = uniform_radius(style.border_radius.unwrap_or(6.0));
        }
        if props.padding.left == 0.0
            && props.padding.right == 0.0
            && props.padding.top == 0.0
            && props.padding.bottom == 0.0
        {
            props.padding = Padding::all(12.0);
        }
        props.semantic = Semantic::tag("pre");
    })
}

const CODE: BuiltinSpec = BuiltinSpec::new("code", schemas::code, code_lower)
    .help(
        "builder.components.code",
        "Code",
        "Preformatted code block with optional language label.",
    )
    .variants(variant_presets::code);

// ── divider ─────────────────────────────────────────────────────────

fn divider_lower(ctx: &LowerCtx<'_>, node: &Node, style: &StyleProperties) -> ui::Node {
    ctx.synthetic_container(node, style, vec![], |props| {
        props.height = Sizing::Fixed(1.0);
        if props.background.is_none() {
            props.background = Some(parse_color("#3b4252").unwrap());
        }
        props.semantic = Semantic::tag("hr");
    })
}

const DIVIDER: BuiltinSpec = BuiltinSpec::new("divider", schemas::divider, divider_lower).help(
    "builder.components.divider",
    "Divider",
    "Horizontal separator line between content sections.",
);

// ── spacer ──────────────────────────────────────────────────────────

fn spacer_lower(_ctx: &LowerCtx<'_>, node: &Node, _style: &StyleProperties) -> ui::Node {
    let p = schemas::SpacerProps::from_value(&node.props);
    let width = node
        .props
        .get("width")
        .and_then(|v| v.as_f64())
        .map(|f| f as f32)
        .unwrap_or(0.0);
    spacer_node(node.id.clone(), width, p.height as f32)
}

const SPACER: BuiltinSpec = BuiltinSpec::new("spacer", schemas::spacer, spacer_lower).help(
    "builder.components.spacer",
    "Spacer",
    "Vertical spacing element with configurable height in pixels.",
);

// ── columns ─────────────────────────────────────────────────────────

fn columns_lower(ctx: &LowerCtx<'_>, node: &Node, style: &StyleProperties) -> ui::Node {
    let p = schemas::ColumnsProps::from_value(&node.props);
    ctx.container_with(node, style, |props| {
        props.direction = Direction::Row;
        if props.gap == 0.0 {
            props.gap = p.gap as f32;
        }
    })
}

const COLUMNS: BuiltinSpec = BuiltinSpec::new("columns", schemas::columns, columns_lower)
    .help(
        "builder.components.columns",
        "Columns",
        "Side-by-side horizontal layout with configurable gap between children.",
    )
    .variants(variant_presets::columns);

// ── list ────────────────────────────────────────────────────────────

fn list_lower(ctx: &LowerCtx<'_>, node: &Node, style: &StyleProperties) -> ui::Node {
    let p = schemas::ListProps::from_value(&node.props);
    let container = ctx.container_with(node, style, |props| {
        if props.gap == 0.0 {
            props.gap = p.item_spacing as f32;
        }
    });
    let tag = if p.ordered { "ol" } else { "ul" };
    with_semantic(container, Semantic::tag(tag))
}

fn list_signals() -> Vec<SignalDef> {
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

const LIST: BuiltinSpec = BuiltinSpec::new("list", schemas::list, list_lower)
    .help(
        "builder.components.list",
        "List",
        "Ordered or unordered list. Toggle the ordered property to switch between numbered and bulleted styles.",
    )
    .signals(list_signals)
    .variants(variant_presets::list);

// ── table ───────────────────────────────────────────────────────────

fn table_lower(ctx: &LowerCtx<'_>, node: &Node, style: &StyleProperties) -> ui::Node {
    let p = schemas::TableProps::from_value(&node.props);

    let mut sections: Vec<ui::Node> = Vec::new();
    if !p.caption.is_empty() {
        let mut caption_style = style.clone();
        if caption_style.color.is_none() {
            caption_style.color = Some("#9ca4b4".into());
        }
        sections.push(with_semantic(
            text_node(
                format!("{}-caption", node.id),
                p.caption.clone(),
                &caption_style,
                12.0,
            ),
            Semantic::tag("caption"),
        ));
    }
    let header_cells: Vec<_> = p
        .headers
        .split(',')
        .filter_map(|c| {
            let c = c.trim();
            (!c.is_empty()).then(|| {
                with_semantic(
                    text_node(format!("{}-h-{}", node.id, c), c.to_string(), style, 13.0),
                    Semantic::tag("th"),
                )
            })
        })
        .collect();
    let header_row = bare_container(format!("{}-headers", node.id), header_cells, |fp| {
        fp.direction = Direction::Row;
        fp.gap = 16.0;
        fp.semantic = Semantic::tag("tr");
    });
    sections.push(bare_container(
        format!("{}-thead", node.id),
        vec![header_row],
        |fp| {
            fp.semantic = Semantic::tag("thead");
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
            props.padding = Padding::all(8.0);
        }
        if props.radius.tl == 0.0 {
            props.radius = uniform_radius(style.border_radius.unwrap_or(4.0));
        }
        props.semantic = Semantic::tag("table");
    })
}

fn table_signals() -> Vec<SignalDef> {
    with_common_signals(vec![
        SignalDef::new("row-clicked", "Fires when a table row is clicked").with_payload(vec![
            FieldSpec::number("row", "Row index", Default::default()),
        ]),
        SignalDef::new("cell-clicked", "Fires when a table cell is clicked").with_payload(vec![
            FieldSpec::number("row", "Row index", Default::default()),
            FieldSpec::number("column", "Column index", Default::default()),
        ]),
        SignalDef::new("header-clicked", "Fires when a column header is clicked").with_payload(
            vec![FieldSpec::number(
                "column",
                "Column index",
                Default::default(),
            )],
        ),
    ])
}

const TABLE: BuiltinSpec = BuiltinSpec::new("table", schemas::table, table_lower)
    .help(
        "builder.components.table",
        "Table",
        "Data table with comma-separated column headers and optional caption.",
    )
    .signals(table_signals)
    .variants(variant_presets::table);

// ── tabs ────────────────────────────────────────────────────────────

fn tabs_lower(ctx: &LowerCtx<'_>, node: &Node, style: &StyleProperties) -> ui::Node {
    let p = schemas::TabsProps::from_value(&node.props);

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
                    fp.height = Sizing::Fixed(32.0);
                    fp.background = parse_color(bg);
                    fp.padding = Padding {
                        left: 12.0,
                        right: 12.0,
                        top: 0.0,
                        bottom: 0.0,
                    };
                    fp.semantic = Semantic::tag("button")
                        .with_role("tab")
                        .with_attr("aria-selected", selected);
                },
            )
        })
        .collect();

    let strip = bare_container(format!("{}-strip", node.id), pills, |fp| {
        fp.direction = Direction::Row;
        fp.semantic = Semantic::default().with_role("tablist");
    });
    let panel = bare_container(
        format!("{}-panel", node.id),
        ctx.lower_children(&node.children),
        |fp| {
            fp.padding = Padding::all(12.0);
            fp.gap = 8.0;
            fp.semantic = Semantic::default().with_role("tabpanel");
        },
    );
    ctx.synthetic_container(node, style, vec![strip, panel], |_| {})
}

fn tabs_signals() -> Vec<SignalDef> {
    with_common_signals(vec![SignalDef::new(
        "tab-changed",
        "Fires when the active tab changes",
    )
    .with_payload(vec![
        FieldSpec::number("index", "Active tab index", Default::default()),
        FieldSpec::text("label", "Active tab label"),
    ])])
}

const TABS: BuiltinSpec = BuiltinSpec::new("tabs", schemas::tabs, tabs_lower)
    .help(
        "builder.components.tabs",
        "Tabs",
        "Tabbed content panels with comma-separated labels. Each child renders as one tab panel.",
    )
    .signals(tabs_signals)
    .variants(variant_presets::tabs);

// ── accordion ───────────────────────────────────────────────────────

fn accordion_lower(ctx: &LowerCtx<'_>, node: &Node, style: &StyleProperties) -> ui::Node {
    let p = schemas::AccordionProps::from_value(&node.props);

    let header = bare_container(
        format!("{}-header", node.id),
        vec![text_node(
            format!("{}-title", node.id),
            format!("▸ {}", p.title),
            style,
            14.0,
        )],
        |fp| {
            fp.height = Sizing::Fixed(32.0);
            fp.background = parse_color("#2e3440");
            fp.radius = uniform_radius(4.0);
            fp.padding = Padding {
                left: 12.0,
                right: 12.0,
                top: 0.0,
                bottom: 0.0,
            };
            fp.semantic = Semantic::tag("summary");
        },
    );
    let content = bare_container(
        format!("{}-content", node.id),
        ctx.lower_children(&node.children),
        |fp| {
            fp.padding = Padding {
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
        let mut hint = Semantic::tag("details");
        if p.open {
            hint = hint.with_attr("open", "open");
        }
        props.semantic = hint;
    })
}

fn accordion_signals() -> Vec<SignalDef> {
    with_common_signals(vec![SignalDef::new(
        "toggled",
        "Fires when the section is expanded or collapsed",
    )
    .with_payload(vec![FieldSpec::boolean(
        "open",
        "Whether the section is now open",
    )])])
}

const ACCORDION: BuiltinSpec = BuiltinSpec::new("accordion", schemas::accordion, accordion_lower)
    .help(
        "builder.components.accordion",
        "Accordion",
        "Collapsible content section with a title bar. Toggle open state to expand or collapse.",
    )
    .signals(accordion_signals)
    .variants(variant_presets::accordion);

// ── graph-view ──────────────────────────────────────────────────────

fn graph_view_signals() -> Vec<SignalDef> {
    with_common_signals(vec![
        SignalDef::new("node-clicked", "Fires when a graph node is clicked")
            .with_payload(vec![FieldSpec::text("node_id", "Clicked node ID")]),
        SignalDef::new("edge-clicked", "Fires when a graph edge is clicked").with_payload(vec![
            FieldSpec::text("source_id", "Source node ID"),
            FieldSpec::text("target_id", "Target node ID"),
        ]),
        SignalDef::new(
            "node-double-clicked",
            "Fires when a graph node is double-clicked",
        )
        .with_payload(vec![FieldSpec::text("node_id", "Double-clicked node ID")]),
    ])
}

const GRAPH_VIEW: BuiltinSpec = BuiltinSpec::new("graph-view", schemas::graph_view, default_lower)
    .help(
        "builder.components.graph-view",
        "Graph View",
        "Interactive node-and-edge relationship visualization with configurable layout algorithms and node styling.",
    )
    .signals(graph_view_signals);

// ── BUILTINS table ──────────────────────────────────────────────────

/// Single source of truth for the default catalog. Adding a builtin is
/// one new `const SPEC` above and one row here.
pub const BUILTINS: &[&BuiltinSpec] = &[
    &TEXT,
    &IMAGE,
    &CONTAINER,
    &FORM,
    &INPUT,
    &BUTTON,
    &CODE,
    &DIVIDER,
    &SPACER,
    &COLUMNS,
    &LIST,
    &TABLE,
    &TABS,
    &ACCORDION,
    &GRAPH_VIEW,
];

// ── card prefab ─────────────────────────────────────────────────────

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

// ── tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::FieldKind;

    fn setup() -> ComponentRegistry {
        let mut reg = ComponentRegistry::new();
        register_builtins(&mut reg).expect("register builtins");
        reg
    }

    #[test]
    fn text_schema_has_body_level_href() {
        let reg = setup();
        let comp = reg.get("text").expect("text registered");
        let schema = comp.schema();
        assert_eq!(schema.len(), 3);
        assert_eq!(schema[0].key, "body");
        assert_eq!(schema[1].key, "level");
        assert_eq!(schema[2].key, "href");
    }

    #[test]
    fn register_builtins_seeds_seventeen_components() {
        let reg = setup();
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
        let reg = setup();
        let comp = reg.get("divider").expect("divider registered");
        assert!(comp.schema().is_empty());
    }

    #[test]
    fn graph_view_id() {
        let reg = setup();
        let comp = reg.get("graph-view").expect("graph-view registered");
        assert_eq!(comp.id().as_str(), "graph-view");
    }

    #[test]
    fn graph_view_schema_has_layout_options() {
        let reg = setup();
        let comp = reg.get("graph-view").expect("graph-view registered");
        let schema = comp.schema();
        let layout_field = schema
            .iter()
            .find(|f| f.key == "layout")
            .expect("missing layout field");
        match &layout_field.kind {
            FieldKind::Select(options) => {
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
    fn graph_view_signals_include_node_and_edge() {
        let reg = setup();
        let comp = reg.get("graph-view").expect("graph-view registered");
        let signals = comp.signals();
        let names: Vec<&str> = signals.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"node-clicked"), "missing node-clicked");
        assert!(names.contains(&"edge-clicked"), "missing edge-clicked");
        assert!(
            names.contains(&"node-double-clicked"),
            "missing node-double-clicked"
        );
        assert!(names.contains(&"clicked"));
        assert!(names.contains(&"hovered"));
    }

    #[test]
    fn builtin_block_factory_returns_known_ids() {
        assert!(builtin_block("text").is_some());
        assert!(builtin_block("graph-view").is_some());
        assert!(builtin_block("nope").is_none());
    }
}
