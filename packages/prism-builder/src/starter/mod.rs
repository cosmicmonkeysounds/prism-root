//! Starter component catalog — the default block registry.
//!
//! Seventeen blocks land here, declared as a single `BUILTINS` table of
//! `BlockSpec` rows. Each row carries `(id, schema, help, signals,
//! variants, lower)`; one [`SpecBlock`] type implements `Block` by
//! delegating to its spec. There are no per-block trait impls — adding
//! a new block is one `const SPEC` and one entry in the table.
//!
//! `facet` is a one-off `Component`; the other 16 (text, image,
//! container, form, input, button, code, divider, spacer, columns,
//! list, table, tabs, accordion, graph-view, **card**) flow through
//! `SpecBlock` + `BlockSpec`. `card` was a `PrefabDef`-backed builtin
//! until §4.4 folded it into a plain `BlockSpec`; `PrefabDef` now
//! exists only as the hidden user/promotion mechanism.

use std::sync::Arc;

use serde_json::json;

use crate::asset::AssetSource;
use crate::block::{register_specs, BlockSpec, SpecBlock};
use crate::document::Node;
use crate::facet::FacetComponent;
use crate::registry::{ComponentRegistry, FieldSpec, RegistryError};
use crate::schemas;
use crate::signal::{with_common_signals, SignalDef};
use crate::style::StyleProperties;
use crate::ui_lower::{
    bare_container, parse_color, spacer_node, text_node, uniform_radius, with_semantic, LowerCtx,
};
use crate::variant::presets as variant_presets;

use prism_ui_runtime::layout::{self as ui, Direction, Padding, Semantic, Sizing};

/// Construct a fresh `Arc<SpecBlock>` for a builtin id, or `None` if
/// the id isn't in [`BUILTINS`]. The blanket `impl<T: Block> Component`
/// makes the result usable directly with `register_block` and
/// `ComponentRegistry::register`.
pub fn builtin_block(id: &str) -> Option<Arc<SpecBlock>> {
    BUILTINS
        .iter()
        .find(|s| s.id == id)
        .map(|spec| SpecBlock::arc(spec))
}

/// Register every entry in [`BUILTINS`] plus the one-off `facet`
/// component. `card` is now a plain `BlockSpec` in the table (§4.4 —
/// the prefab-backed builtin was folded into a SpecBlock; `PrefabDef`
/// stays only as the hidden user/promotion mechanism). The Slint DSL
/// emit path is gone; the unified Taffy/SSR pipeline is the single
/// render path.
pub fn register_builtins(components: &mut ComponentRegistry) -> Result<(), RegistryError> {
    register_specs(components, BUILTINS)?;
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
    // Inner leaf carries the level/anchor semantic + the derived id;
    // the outer wrapping container takes `node.id` so the canvas-preview
    // tagging walk (which only visits Container variants) attaches
    // `data-canvas-node="<id>"` here. Without the wrap, clicking the
    // rendered text bubbled up to the nearest ancestor container — the
    // page root — and users could never select a Text component to
    // edit its body / level.
    let leaf = text_node(format!("{}-text", node.id), content, style, default_size);
    let level_tag = level_to_html_tag(p.level.as_str());
    let inner = if !p.href.is_empty() {
        let anchored = with_semantic(leaf, Semantic::tag("a").with_attr("href", p.href.clone()));
        bare_container(format!("{}-anchor", node.id), vec![anchored], |props| {
            props.semantic = Semantic::tag(level_tag);
        })
    } else {
        with_semantic(leaf, Semantic::tag(level_tag))
    };
    bare_container(node.id.clone(), vec![inner], |props| {
        props.width = Sizing::Grow;
    })
}

fn text_signals() -> Vec<SignalDef> {
    with_common_signals(vec![SignalDef::new(
        "link-clicked",
        "Fires when a hyperlink in the text is clicked",
    )
    .with_payload(vec![FieldSpec::text("href", "Link URL")])])
}

const TEXT: BlockSpec = BlockSpec::new("text", schemas::text).lower(text_lower)
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

const IMAGE: BlockSpec = BlockSpec::new("image", schemas::image).lower(image_lower)
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

const CONTAINER: BlockSpec = BlockSpec::new("container", schemas::container)
    .lower(container_lower)
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

const FORM: BlockSpec = BlockSpec::new("form", schemas::form)
    .lower(form_lower)
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

const INPUT: BlockSpec = BlockSpec::new("input", schemas::input)
    .lower(input_lower)
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

const BUTTON: BlockSpec = BlockSpec::new("button", schemas::button)
    .lower(button_lower)
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

const CODE: BlockSpec = BlockSpec::new("code", schemas::code)
    .lower(code_lower)
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

const DIVIDER: BlockSpec = BlockSpec::new("divider", schemas::divider)
    .lower(divider_lower)
    .help(
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

const SPACER: BlockSpec = BlockSpec::new("spacer", schemas::spacer)
    .lower(spacer_lower)
    .help(
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

const COLUMNS: BlockSpec = BlockSpec::new("columns", schemas::columns)
    .lower(columns_lower)
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

const LIST: BlockSpec = BlockSpec::new("list", schemas::list).lower(list_lower)
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

const TABLE: BlockSpec = BlockSpec::new("table", schemas::table)
    .lower(table_lower)
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

const TABS: BlockSpec = BlockSpec::new("tabs", schemas::tabs)
    .lower(tabs_lower)
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

const ACCORDION: BlockSpec = BlockSpec::new("accordion", schemas::accordion)
    .lower(accordion_lower)
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

const GRAPH_VIEW: BlockSpec = BlockSpec::new("graph-view", schemas::graph_view)
    .help(
        "builder.components.graph-view",
        "Graph View",
        "Interactive node-and-edge relationship visualization with configurable layout algorithms and node styling.",
    )
    .signals(graph_view_signals);

// ── BUILTINS table ──────────────────────────────────────────────────

/// Single source of truth for the default catalog. Adding a builtin is
/// one new `const SPEC` above and one row here.
pub const BUILTINS: &[&BlockSpec] = &[
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
    &CARD,
];

// ── card ─────────────────────────────────────────────────────────

/// Bordered content card: a styled container wrapping a heading
/// (`title`) and a paragraph (`body`). Folded from the former `card`
/// prefab into a declarative `BlockSpec` (§4.4) — same visual, same
/// `"card"` id, but no `PrefabDef` side-table. The two text children
/// lower through the normal `text` block so the inspector / property
/// panel / click paths treat a card like any other container.
fn card_lower(ctx: &LowerCtx<'_>, node: &Node, style: &StyleProperties) -> ui::Node {
    let p = schemas::CardProps::from_value(&node.props);
    let subtree = Node {
        id: node.id.clone(),
        component: "container".into(),
        props: json!({
            "spacing": 8,
            "padding": 16,
            "border_width": 1,
            "border_color": "#3b4252"
        }),
        children: vec![
            Node {
                id: format!("{}-title", node.id),
                component: "text".into(),
                props: json!({ "body": p.title, "level": "h3" }),
                ..Default::default()
            },
            Node {
                id: format!("{}-body", node.id),
                component: "text".into(),
                props: json!({ "body": p.body, "level": "paragraph" }),
                ..Default::default()
            },
        ],
        style: StyleProperties {
            background: Some("#2e3440".into()),
            border_radius: Some(8.0),
            ..style.clone()
        },
        ..Default::default()
    };
    ctx.lower(&subtree)
}

const CARD: BlockSpec = BlockSpec::new("card", schemas::card)
    .lower(card_lower)
    .help(
        "builder.components.card",
        "Card",
        "Bordered content card with a title heading and a body paragraph.",
    );

#[cfg(test)]
mod tests;
