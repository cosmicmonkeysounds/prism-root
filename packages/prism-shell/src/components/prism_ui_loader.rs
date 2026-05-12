//! Wave 11.2 of `docs/dev/composable-builder-plan.md` — load
//! `.prism-ui` source files as registered shell `Block`s.
//!
//! ## Smart-pattern shape
//!
//! Each migrated shell component is a single declarative row in
//! [`SHELL_PRISM_UI_COMPONENTS`]:
//!
//! ```ignore
//! pub static SHELL_PRISM_UI_COMPONENTS: &[PrismUiSpec] = &[
//!     PrismUiSpec::new(
//!         "shell.toolbar-separator",
//!         include_str!("../../ui/components/toolbar-separator.prism-ui"),
//!     ),
//!     // …
//! ];
//! ```
//!
//! [`register_prism_ui_components`] parses each source at boot, wraps
//! it in a [`PrismUiBlock`] that implements [`Block`], and fans the
//! batch through the same [`register_specs`]-style path the native
//! `SHELL_BUILTINS` use. Adding a migration is **one .prism-ui file +
//! one row** — no per-component struct, no per-component test seam
//! (the cross-cutting tests in this module exercise the loader's
//! behaviour against the table).
//!
//! ## Composition seam
//!
//! Each `PrismUiBlock` holds an `Arc<OnceLock<Arc<dyn TagResolver>>>`
//! shared across the whole batch. After all native + DSL blocks
//! register, [`finalize_prism_ui_resolver`] populates the cell with a
//! fresh resolver built over the live registry. At `lower_ui` time,
//! the block reads the resolver, builds a [`LowerScope`] seeded with
//! every `node.prop` as a binding, and runs the parsed AST through
//! [`lower_document_with_scope`]. Composing primitives, hover
//! modifiers, and per-key prop interpolation (`{title}`) all flow
//! through the existing runtime — the loader adds no new vocabulary.

use std::sync::{Arc, OnceLock};

use prism_builder::{
    component::ComponentId,
    document::Node,
    registry::{FieldSpec, NumericBounds, RegistryError},
    signal::{common_signals, with_common_signals, SignalDef},
    style::StyleProperties,
    ui_lower::LowerCtx,
    ui_resolver::RegistryTagResolver,
    Block,
};
use prism_core::language::prism_ui::{parse, Document as AstDocument};
use prism_ui_runtime::interpret::{lower_document_with_scope, LowerScope, TagResolver};
use prism_ui_runtime::layout::Node as UiNode;
use serde_json::Value;

use crate::components::registry::ShellComponentRegistry;

/// Declarative spec for a `.prism-ui`-authored shell component.
/// Mirrors the shape of [`prism_builder::BlockSpec`] but trades the
/// imperative `lower: LowerFn` field for a `source` string that the
/// runtime interprets at render time.
///
/// Authored as a `pub const` in the migrated component's call site
/// or — for the bulk Tier-1 migrations — directly in the
/// [`SHELL_PRISM_UI_COMPONENTS`] table.
pub struct PrismUiSpec {
    pub id: &'static str,
    pub source: &'static str,
    pub schema: fn() -> Vec<FieldSpec>,
    pub signals: fn() -> Vec<SignalDef>,
}

impl PrismUiSpec {
    pub const fn new(id: &'static str, source: &'static str) -> Self {
        Self {
            id,
            source,
            schema: empty_schema,
            signals: common_signals,
        }
    }

    pub const fn schema(mut self, f: fn() -> Vec<FieldSpec>) -> Self {
        self.schema = f;
        self
    }

    pub const fn signals(mut self, f: fn() -> Vec<SignalDef>) -> Self {
        self.signals = f;
        self
    }
}

fn empty_schema() -> Vec<FieldSpec> {
    vec![]
}

/// Shared late-init resolver cell. Populated by
/// [`finalize_prism_ui_resolver`] after every native + DSL block
/// registers, so a DSL block's `lower_ui` can dispatch
/// composed `<shell.*>` / `<prism.*>` tags through the live
/// registry. Using `OnceLock` keeps the contract "set exactly once,
/// no interior mutability after that" — any future hot-reload of
/// the registry rebuilds the entire shell, not the cell.
pub type SharedResolver = Arc<OnceLock<Arc<dyn TagResolver>>>;

/// Runtime [`Block`] backing a `.prism-ui`-authored shell component.
/// Holds the parsed AST plus the shared resolver cell; `lower_ui`
/// snapshots `node.props` into a [`LowerScope`] and walks the AST.
pub struct PrismUiBlock {
    id: ComponentId,
    schema: fn() -> Vec<FieldSpec>,
    signals: fn() -> Vec<SignalDef>,
    parsed: AstDocument,
    resolver: SharedResolver,
}

impl PrismUiBlock {
    pub fn id(&self) -> &str {
        &self.id
    }
}

impl Block for PrismUiBlock {
    fn id(&self) -> &ComponentId {
        &self.id
    }

    fn schema(&self) -> Vec<FieldSpec> {
        (self.schema)()
    }

    fn signals(&self) -> Vec<SignalDef> {
        (self.signals)()
    }

    fn lower_ui(&self, ctx: &LowerCtx<'_>, node: &Node, style: &StyleProperties) -> UiNode {
        // Resolver — present on every production path, absent on
        // some headless test paths. Falling back to no-resolver means
        // composed `<shell.*>` tags inside the DSL drop to the
        // runtime's unknown-tag default (children surface, wrapper
        // disappears) — still a sensible fallback.
        let mut scope = LowerScope::default();
        if let Some(resolver) = self.resolver.get() {
            scope = scope.with_resolver(Arc::clone(resolver));
        } else if let Some(reg) = ctx.registry() {
            // Headless / one-shot path: build a resolver from the live
            // registry on the fly. Cheap — `ComponentRegistry::clone`
            // is an `IndexMap` clone of Arc<dyn Component> entries.
            let arc = Arc::new(reg.clone());
            scope = scope.with_resolver(Arc::new(RegistryTagResolver::new(arc)));
        }

        // Seed schema defaults first so `{title}` / `{enabled}`
        // interpolations have a sensible fallback when the caller
        // didn't pass the prop. Authored `.prism-ui` source treats a
        // missing prop the same as an empty one in `if=` / `else=`
        // checks, so a schema default lets the DSL inherit the
        // Rust-side `with_default` value without re-stating it.
        for field in (self.schema)() {
            if !field.default.is_null() {
                scope = scope.with_binding(field.key, field.default);
            }
        }
        // Then seed the actual `node.props` — overriding defaults
        // where present, mirroring Rust-side `ctx.prop_str` /
        // `ctx.prop_bool` precedence (caller wins over schema default).
        if let Some(map) = node.props.as_object() {
            for (k, v) in map {
                scope = scope.with_binding(k.clone(), v.clone());
            }
        }

        // Wave 11.2 — pre-lowered children the caller (resolver path)
        // handed via `LowerCtx::host_children()` flow into the
        // DSL-side `<host-children/>` element. Composition wrappers
        // (toast-stack, launchpad, app-window) declare a single
        // `<host-children/>` in their body where the caller's children
        // should appear.
        if let Some(host) = ctx.host_children() {
            scope = scope.with_host_children_ui(host.to_vec());
        }
        // Wave 13.1 — named-slot map. The resolver partitioned a
        // dispatched element's AST children by `slot="X"`; this thread
        // surfaces those buckets to `<slot name="X"/>` reads inside
        // the DSL body.
        if let Some(slots) = ctx.host_children_by_slot() {
            scope = scope.with_host_children_by_slot(Arc::clone(slots));
        }

        let nodes = lower_document_with_scope(&self.parsed, &scope);
        match collapse_to_single_root(nodes, node, style, ctx) {
            UiNode::Container {
                id: _,
                props,
                children,
            } => UiNode::Container {
                // Rewrite the outer id to the calling node's id so
                // hit-testing / selection / per-NodeId reactive
                // contexts all find the right node. The DSL author
                // writes a `<container>` with whatever id, but the
                // canvas-facing id is the block instance.
                id: node.id.clone(),
                props,
                children,
            },
            other => other,
        }
    }
}

/// Collapse the AST's lowered root list into a single `UiNode`. The
/// migration convention is **single-root .prism-ui per component**;
/// when an author writes a multi-rooted source we wrap it in a flow
/// container so the shape stays predictable for the caller. Empty
/// sources fall back to the bare default container.
fn collapse_to_single_root(
    mut nodes: Vec<UiNode>,
    node: &Node,
    style: &StyleProperties,
    ctx: &LowerCtx<'_>,
) -> UiNode {
    match nodes.len() {
        0 => ctx.default_container(node, style),
        1 => nodes.remove(0),
        _ => UiNode::Container {
            id: node.id.clone(),
            props: prism_ui_runtime::layout::ContainerProps::default(),
            children: nodes,
        },
    }
}

/// Parse every spec in `specs` and register the resulting
/// [`PrismUiBlock`]s into `reg`. All blocks share the same
/// `resolver` cell — populate it post-registration with
/// [`finalize_prism_ui_resolver`].
pub fn register_prism_ui_components(
    reg: &mut ShellComponentRegistry,
    specs: &[PrismUiSpec],
    resolver: &SharedResolver,
) -> Result<(), PrismUiLoadError> {
    for spec in specs {
        let (parsed, errs) = parse(spec.source);
        if !errs.is_empty() {
            return Err(PrismUiLoadError::Parse {
                id: spec.id,
                errors: errs.into_iter().map(|e| e.message).collect(),
            });
        }
        let block = Arc::new(PrismUiBlock {
            id: spec.id.into(),
            schema: spec.schema,
            signals: spec.signals,
            parsed,
            resolver: Arc::clone(resolver),
        });
        reg.register(block).map_err(PrismUiLoadError::Register)?;
    }
    Ok(())
}

/// Populate the shared resolver cell. Call once after every
/// native + DSL block is registered so DSL `lower_ui` bodies can
/// dispatch composed tags through the live registry. Idempotent:
/// a second call after the cell is populated is a silent no-op
/// (matching `OnceLock::set`'s contract).
pub fn finalize_prism_ui_resolver(resolver: &SharedResolver, reg: &ShellComponentRegistry) {
    let _ = resolver.set(reg.tag_resolver());
}

/// Construct a fresh shared resolver cell. Returned `Arc` is cloned
/// into every `PrismUiBlock` plus the post-registration
/// [`finalize_prism_ui_resolver`] call.
pub fn make_shared_resolver() -> SharedResolver {
    Arc::new(OnceLock::new())
}

#[derive(Debug)]
pub enum PrismUiLoadError {
    Parse {
        id: &'static str,
        errors: Vec<String>,
    },
    Register(RegistryError),
}

impl std::fmt::Display for PrismUiLoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse { id, errors } => {
                write!(f, "parse error in `{id}`: {}", errors.join("; "))
            }
            Self::Register(e) => write!(f, "register error: {e:?}"),
        }
    }
}

impl std::error::Error for PrismUiLoadError {}

// ── Tier-1 migrated component sources ──
//
// Each row: an id in the `shell.*` namespace plus a `.prism-ui`
// source embedded via `include_str!`. Optional `.schema(fn)` /
// `.signals(fn)` to override the empty defaults.
//
// Author a new migration:
//   1. Write `ui/components/<id>.prism-ui` (single-root container).
//   2. Add one `PrismUiSpec::new(...)` row here.
//   3. Delete the old `components/<id>.rs` Rust file + its
//      `pub mod` row in `mod.rs` + its row in `SHELL_BUILTINS`.

fn no_schema() -> Vec<FieldSpec> {
    vec![]
}

fn help_tooltip_schema() -> Vec<FieldSpec> {
    vec![
        FieldSpec::text("title", "Title").required(),
        FieldSpec::text("summary", "Summary"),
    ]
}

fn docs_view_schema() -> Vec<FieldSpec> {
    vec![
        FieldSpec::text("title", "Title").required(),
        FieldSpec::text("summary", "Summary"),
        FieldSpec::text("body", "Body"),
    ]
}

fn docs_sidebar_schema() -> Vec<FieldSpec> {
    vec![
        FieldSpec::text("title", "Title").required(),
        FieldSpec::text("summary", "Summary"),
        FieldSpec::text("body", "Body"),
        FieldSpec::text("mode", "Mode (full|compact)"),
    ]
}

fn launchpad_schema() -> Vec<FieldSpec> {
    vec![FieldSpec::text("title", "Hero title")]
}

// Wave 11.2 batch — Tier-1 migrations landed alongside the loader
// generalisations (schema-default seeding into scope + `props="{expr}"`
// resolver-side spread). Each schema mirrors its Rust-side source
// 1:1 so the Rust → DSL switch is invisible to consumers.

fn explorer_schema() -> Vec<FieldSpec> {
    vec![FieldSpec::text("nodes", "Nodes (JSON array of row props)")]
}

fn docs_content_schema() -> Vec<FieldSpec> {
    vec![
        FieldSpec::text("doc-title", "Title").required(),
        FieldSpec::text("doc-summary", "Summary"),
        FieldSpec::textarea("doc-body", "Body"),
        FieldSpec::boolean("compact", "Compact").with_default(Value::Bool(false)),
    ]
}

fn section_header_schema() -> Vec<FieldSpec> {
    vec![
        FieldSpec::text("label", "Label").required(),
        FieldSpec::boolean("collapsed", "Collapsed").with_default(Value::Bool(false)),
        FieldSpec::text("section-id", "Section ID"),
    ]
}

fn section_header_signals() -> Vec<SignalDef> {
    with_common_signals(vec![SignalDef::new(
        "section-toggled",
        "Fires when the header is clicked — payload carries the section id.",
    )
    .with_payload(vec![FieldSpec::text("section_id", "Section ID")])])
}

fn nav_button_schema() -> Vec<FieldSpec> {
    vec![
        FieldSpec::text("icon", "Icon").required(),
        FieldSpec::boolean("selected", "Selected").with_default(Value::Bool(false)),
        FieldSpec::text("help-id", "Help ID"),
        FieldSpec::text("nav-id", "Activity-bar id"),
    ]
}

fn nav_button_signals() -> Vec<SignalDef> {
    with_common_signals(vec![
        SignalDef::new(
            "hover-start",
            "Pointer entered the button — positional payload for tooltip placement.",
        )
        .with_payload(vec![
            FieldSpec::text("help_id", "Help ID"),
            FieldSpec::number("x", "X (px)", NumericBounds::default()),
            FieldSpec::number("y", "Y (px)", NumericBounds::default()),
        ]),
        SignalDef::new("hover-end", "Pointer left the button."),
    ])
}

fn inspector_tree_schema() -> Vec<FieldSpec> {
    vec![
        FieldSpec::text("aria-label", "ARIA label"),
        FieldSpec::text("nodes", "Inspector rows (JSON array)"),
    ]
}

fn nav_page_list_schema() -> Vec<FieldSpec> {
    vec![FieldSpec::text(
        "pages",
        "Pages (JSON array of nav-page-row props)",
    )]
}

fn signals_panel_schema() -> Vec<FieldSpec> {
    vec![
        FieldSpec::text("title", "Section title"),
        FieldSpec::text(
            "connections",
            "Connections (JSON array of signal-connection-row props)",
        ),
        FieldSpec::boolean("show-add", "Render the add-connection footer")
            .with_default(Value::Bool(true)),
    ]
}

fn workflow_page_bar_schema() -> Vec<FieldSpec> {
    vec![FieldSpec::text("pages", "Pages (JSON array)")]
}

fn items_only_schema() -> Vec<FieldSpec> {
    vec![FieldSpec::text("items", "Items (JSON array)")]
}

fn add_modifier_button_schema() -> Vec<FieldSpec> {
    vec![
        FieldSpec::text("target-id", "Owning node id").required(),
        FieldSpec::text("attached", "Already-attached ids (JSON array)")
            .with_default(Value::Array(Vec::new())),
    ]
}

// ── Batch 3 (Wave 11.2): row variants + ternary / boolean / dispatch ──
//
// Six more Tier-1 migrations landing alongside the DSL substrate
// (ternary, C-style `&&`/`||`/`!`, dotted-path comparison, dynamic
// `<dispatch>`). Each was previously blocked on one of the four
// substrate features the plan called out at §11.2; with the substrate
// in place every block here is a single `.prism-ui` file + one row.

fn toast_schema() -> Vec<FieldSpec> {
    vec![
        FieldSpec::text("title", "Title").required(),
        FieldSpec::text("body", "Body"),
        FieldSpec::text("kind", "Kind").with_default(Value::String("info".into())),
    ]
}

fn toast_signals() -> Vec<SignalDef> {
    with_common_signals(vec![SignalDef::new(
        "dismissed",
        "User dismissed the toast (close click, swipe, or auto-timeout).",
    )])
}

fn menu_item_schema() -> Vec<FieldSpec> {
    vec![
        FieldSpec::text("item-id", "Item id").required(),
        FieldSpec::text("label", "Label").required(),
        FieldSpec::text("shortcut", "Shortcut hint"),
        FieldSpec::boolean("disabled", "Disabled").with_default(Value::Bool(false)),
        FieldSpec::boolean("enabled", "Enabled (state-side authoring)")
            .with_default(Value::Bool(true)),
        FieldSpec::text("command", "Command id to dispatch on click"),
    ]
}

fn signal_connection_row_schema() -> Vec<FieldSpec> {
    vec![
        FieldSpec::text("connection-id", "Connection id (cursor key)"),
        FieldSpec::text("source-signal", "Source signal"),
        FieldSpec::text("action-kind", "Action kind"),
        FieldSpec::text("target-label", "Target label"),
        FieldSpec::boolean("selected", "Selected").with_default(Value::Bool(false)),
        FieldSpec::boolean("show-delete", "Show delete affordance")
            .with_default(Value::Bool(false)),
    ]
}

fn signal_connection_row_signals() -> Vec<SignalDef> {
    with_common_signals(vec![
        SignalDef::new(
            "row-clicked",
            "Row activated; host selects the bound connection.",
        ),
        SignalDef::new("delete-clicked", "Trash button pressed."),
    ])
}

fn schema_row_schema() -> Vec<FieldSpec> {
    vec![
        FieldSpec::text("field-id", "Field id (cursor key)"),
        FieldSpec::text("field-name", "Field name"),
        FieldSpec::text("field-kind", "Field kind"),
        FieldSpec::boolean("required", "Required").with_default(Value::Bool(false)),
        FieldSpec::boolean("selected", "Selected").with_default(Value::Bool(false)),
        FieldSpec::boolean("show-delete", "Show delete affordance")
            .with_default(Value::Bool(false)),
    ]
}

fn schema_row_signals() -> Vec<SignalDef> {
    with_common_signals(vec![
        SignalDef::new(
            "row-clicked",
            "Row activated; host selects the bound field.",
        ),
        SignalDef::new("delete-clicked", "Trash button pressed."),
    ])
}

fn nav_page_row_schema() -> Vec<FieldSpec> {
    vec![
        FieldSpec::text("page-id", "Page id"),
        FieldSpec::text("page-title", "Page title"),
        FieldSpec::text("route", "Route"),
        FieldSpec::boolean("is-active", "Active page").with_default(Value::Bool(false)),
        FieldSpec::number("node-count", "Node count", NumericBounds::min(0.0))
            .with_default(Value::from(0.0)),
        FieldSpec::number("link-count", "Inbound link count", NumericBounds::min(0.0))
            .with_default(Value::from(0.0)),
        FieldSpec::boolean("selected", "Selected").with_default(Value::Bool(false)),
        FieldSpec::boolean("show-delete", "Show delete").with_default(Value::Bool(false)),
    ]
}

fn nav_page_row_signals() -> Vec<SignalDef> {
    with_common_signals(vec![
        SignalDef::new("row-clicked", "Row activated."),
        SignalDef::new("move-up", "Move up clicked."),
        SignalDef::new("move-down", "Move down clicked."),
        SignalDef::new("delete-clicked", "Trash clicked."),
    ])
}

fn properties_panel_schema() -> Vec<FieldSpec> {
    vec![FieldSpec::text(
        "rows",
        "Rows (JSON array of {component, props})",
    )]
}

// ── Batch 4 (Wave 11.2): chrome lifts ──
//
// The remaining Tier-1 components share a small set of visual recipes
// (icon-button glyph, active-underline tab pill). Lifting those recipes
// to .prism-ui source — and authoring every consumer in DSL — removes
// the per-call-site `chrome::*_node` helper coupling. Wave 11.2's
// "shared-chrome lift" substrate, landed declaratively.

fn icon_button_schema() -> Vec<FieldSpec> {
    vec![
        FieldSpec::text("icon", "Icon").required(),
        FieldSpec::boolean("enabled", "Enabled").with_default(Value::Bool(true)),
        FieldSpec::text("tooltip-text", "Tooltip text"),
        FieldSpec::text("help-id", "Help ID"),
        FieldSpec::text("tint", "Glyph tint"),
        FieldSpec::text("command", "Command id to dispatch on click"),
    ]
}

fn icon_button_signals() -> Vec<SignalDef> {
    with_common_signals(vec![
        SignalDef::new(
            "hover-start",
            "Pointer entered the button — positional payload for tooltip placement.",
        )
        .with_payload(vec![
            FieldSpec::text("help_id", "Help ID"),
            FieldSpec::number("x", "X (px)", NumericBounds::default()),
            FieldSpec::number("y", "Y (px)", NumericBounds::default()),
        ]),
        SignalDef::new("hover-end", "Pointer left the button."),
    ])
}

fn dock_tab_schema() -> Vec<FieldSpec> {
    vec![
        FieldSpec::text("tab-id", "Tab id").required(),
        FieldSpec::text("label", "Label").required(),
        FieldSpec::boolean("active", "Active").with_default(Value::Bool(false)),
    ]
}

fn workflow_page_button_schema() -> Vec<FieldSpec> {
    vec![
        FieldSpec::text("page-id", "Page ID").required(),
        FieldSpec::text("label", "Label").required(),
        FieldSpec::boolean("active", "Active").with_default(Value::Bool(false)),
    ]
}

fn tab_button_schema() -> Vec<FieldSpec> {
    vec![
        FieldSpec::text("target-id", "Hit-routing target id").required(),
        FieldSpec::text("label", "Label").required(),
        FieldSpec::text("data-role", "data-role for hit routing").required(),
        FieldSpec::boolean("active", "Active").with_default(Value::Bool(false)),
        FieldSpec::number("height", "Tab height (px)", NumericBounds::min(0.0))
            .with_default(Value::from(26.0)),
        FieldSpec::number(
            "padding-x",
            "Horizontal padding (px)",
            NumericBounds::min(0.0),
        )
        .with_default(Value::from(12.0)),
        FieldSpec::number("padding-top", "Top padding (px)", NumericBounds::min(0.0))
            .with_default(Value::from(6.0)),
        FieldSpec::text("active-bg", "Active background colour")
            .with_default(Value::String("#19000000".into())),
        FieldSpec::text("hover-bg", "Hover background colour")
            .with_default(Value::String("#0f000000".into())),
        FieldSpec::text("underline-active", "Active underline colour")
            .with_default(Value::String("#0060c0".into())),
    ]
}

fn status_bar_schema() -> Vec<FieldSpec> {
    vec![
        FieldSpec::text("text", "Status text"),
        FieldSpec::text("status", "Status text (back-compat alias)"),
        FieldSpec::text("segments", "Segments (JSON array)"),
    ]
}

fn menu_bar_row_schema() -> Vec<FieldSpec> {
    vec![
        FieldSpec::text("app-name", "App name"),
        FieldSpec::boolean("show-tabs", "Show tabs").with_default(Value::Bool(false)),
        FieldSpec::text("menus", "Menu pills (JSON array)"),
        FieldSpec::text("tabs", "Tab pills (JSON array)"),
        FieldSpec::number("active-menu", "Active menu index", NumericBounds::default())
            .with_default(Value::from(-1.0)),
    ]
}

fn menu_bar_row_signals() -> Vec<SignalDef> {
    with_common_signals(vec![
        SignalDef::new(
            "item-clicked",
            "Menu pill clicked — payload is the menu id.",
        ),
        SignalDef::new("tab-activated", "Tab clicked — payload is the tab index."),
        SignalDef::new("add-page", "The trailing + button was clicked."),
    ])
}

// ── Wave 12 (component palette migration) ──
//
// shell.component-palette migrated to DSL alongside the style-prop
// substrate landing. Wave 12 is the first first-class user of the
// resolver-side `style:` / `style="{obj}"` seam: the selected vs.
// hover-only branch on the row container is expressed as ternary
// `style:background` overrides resolved against `selected-id` ==
// `item.item-id`.

fn component_palette_schema() -> Vec<FieldSpec> {
    vec![
        FieldSpec::text("items", "Items (JSON array of palette entries)"),
        FieldSpec::text("selected-id", "Selected item id"),
    ]
}

fn dock_tab_bar_schema() -> Vec<FieldSpec> {
    vec![FieldSpec::text("tabs", "Tabs (JSON array)")]
}

fn component_picker_schema() -> Vec<FieldSpec> {
    vec![
        FieldSpec::boolean("open", "Open").with_default(Value::Bool(false)),
        FieldSpec::text("query", "Query string"),
        FieldSpec::text(
            "categories",
            "Categories (JSON array of {label, items: [{id, label, icon?}]})",
        ),
    ]
}

fn component_picker_signals() -> Vec<SignalDef> {
    with_common_signals(vec![
        SignalDef::new("item-picked", "Item activated."),
        SignalDef::new("query-changed", "Query string changed."),
        SignalDef::new("dismissed", "Popup dismissed."),
    ])
}

fn connection_picker_schema() -> Vec<FieldSpec> {
    vec![
        FieldSpec::boolean("open", "Open").with_default(Value::Bool(false)),
        FieldSpec::text("source-signal", "Source signal"),
        FieldSpec::text("action-kind", "Action kind"),
        FieldSpec::text("target-label", "Target label"),
    ]
}

fn drag_number_field_schema() -> Vec<FieldSpec> {
    vec![
        FieldSpec::text("key", "Key"),
        FieldSpec::text("label", "Label"),
        FieldSpec::number("value", "Value", NumericBounds::default())
            .with_default(Value::from(0.0)),
        FieldSpec::number("step", "Step", NumericBounds::min(0.0)).with_default(Value::from(1.0)),
        FieldSpec::number("min", "Minimum", NumericBounds::default())
            .with_default(Value::from(-99_999.0)),
        FieldSpec::number("max", "Maximum", NumericBounds::default())
            .with_default(Value::from(99_999.0)),
        FieldSpec::text("display-value", "Pre-formatted value (host computes)")
            .with_default(Value::String("0".into())),
    ]
}

fn drag_number_field_signals() -> Vec<SignalDef> {
    with_common_signals(vec![
        SignalDef::new("changed", "Drag updated the value.").with_payload(vec![
            FieldSpec::text("key", "Key"),
            FieldSpec::number("value", "Value", NumericBounds::default()),
        ]),
        SignalDef::new("committed", "Inline edit accepted.").with_payload(vec![
            FieldSpec::text("key", "Key"),
            FieldSpec::text("text", "Raw text"),
        ]),
    ])
}

fn gizmo_move_schema() -> Vec<FieldSpec> {
    vec![FieldSpec::text("axis-active", "Active axis (x|y|hub)")]
}

fn gizmo_move_signals() -> Vec<SignalDef> {
    with_common_signals(vec![
        SignalDef::new("axis-pressed", "Axis arm pressed."),
        SignalDef::new("axis-dragged", "Axis arm dragged."),
    ])
}

fn gizmo_rotate_schema() -> Vec<FieldSpec> {
    vec![FieldSpec::text("active", "Active state")]
}

fn gizmo_rotate_signals() -> Vec<SignalDef> {
    with_common_signals(vec![
        SignalDef::new("ring-pressed", "Ring pressed."),
        SignalDef::new("ring-dragged", "Ring dragged (degrees delta)."),
    ])
}

fn gizmo_scale_schema() -> Vec<FieldSpec> {
    vec![FieldSpec::text("axis-active", "Active axis (x|y|hub)")]
}

fn gizmo_scale_signals() -> Vec<SignalDef> {
    with_common_signals(vec![
        SignalDef::new("axis-pressed", "Axis pressed."),
        SignalDef::new("axis-dragged", "Axis dragged."),
    ])
}

fn resize_handle_schema() -> Vec<FieldSpec> {
    vec![FieldSpec::text("direction", "Handle direction (tl|t|tr|r|br|b|bl|l)").required()]
}

fn resize_handle_signals() -> Vec<SignalDef> {
    with_common_signals(vec![
        SignalDef::new("handle-pressed", "Handle pressed."),
        SignalDef::new("handle-dragged", "Handle dragged."),
    ])
}

fn dock_divider_schema() -> Vec<FieldSpec> {
    vec![
        FieldSpec::text("orientation", "Orientation (vertical|horizontal)")
            .with_default(Value::String("vertical".into())),
        FieldSpec::number("length", "Cross-axis length", NumericBounds::min(0.0))
            .with_default(Value::from(0.0)),
    ]
}

fn app_window_schema() -> Vec<FieldSpec> {
    vec![
        FieldSpec::text("title", "Window title"),
        FieldSpec::text("status", "Status bar text"),
        FieldSpec::text("app-name", "Active app name"),
        // JSON arrays; host populates from workspace state.
        FieldSpec::text("menus", "Menu pills (JSON array)"),
        FieldSpec::text("tabs", "Tab pills (JSON array)"),
        FieldSpec::boolean("show-tabs", "Show tab strip").with_default(Value::Bool(false)),
        FieldSpec::number("active-menu", "Active menu index", NumericBounds::default())
            .with_default(Value::from(-1.0)),
        FieldSpec::text("nav-buttons", "Activity-bar buttons (JSON array)"),
    ]
}

fn app_window_signals() -> Vec<SignalDef> {
    with_common_signals(vec![SignalDef::new(
        "nav-clicked",
        "Activity-bar button clicked — payload is the nav id.",
    )])
}

fn builder_toolbar_schema() -> Vec<FieldSpec> {
    use serde_json::json;
    vec![
        FieldSpec::text("device", "Active device preview (desktop|tablet|mobile)"),
        FieldSpec::number("zoom", "Canvas zoom", NumericBounds::min_max(0.1, 8.0))
            .with_default(Value::from(1.0)),
        FieldSpec::number("node-count", "Total node count", NumericBounds::min(0.0))
            .with_default(Value::from(0.0)),
        FieldSpec::text("tool", "Active tool (move|rotate|scale)"),
        FieldSpec::text("zoom-label", "Pre-formatted zoom percent label")
            .with_default(Value::String("100%".into())),
        // Wave 13.1-era `for=` iteration tables. The native source kept
        // these as `const ALIGN_ENTRIES` / `const DEVICE_ENTRIES`; the
        // DSL migration moves them into prop defaults so the iteration
        // is one `for="entry in align-entries"` over a typed array.
        FieldSpec::text("align-entries", "Alignment buttons (JSON)").with_default(json!([
            { "entry-id": "align-left",   "icon": "icons/align-left.svg",   "tooltip": "Align left",   "command": "builder.align-left" },
            { "entry-id": "align-center", "icon": "icons/align-center.svg", "tooltip": "Align center", "command": "builder.align-center" },
            { "entry-id": "align-right",  "icon": "icons/align-right.svg",  "tooltip": "Align right",  "command": "builder.align-right" },
        ])),
        FieldSpec::text("device-entries", "Device pills (JSON)").with_default(json!([
            { "entry-id": "desktop", "label": "Desktop", "tooltip": "Desktop preview" },
            { "entry-id": "tablet",  "label": "Tablet",  "tooltip": "Tablet preview" },
            { "entry-id": "mobile",  "label": "Mobile",  "tooltip": "Mobile preview" },
        ])),
    ]
}

fn builder_toolbar_signals() -> Vec<SignalDef> {
    with_common_signals(vec![
        SignalDef::new("align-clicked", "Alignment button clicked."),
        SignalDef::new("device-changed", "Device pill clicked."),
        SignalDef::new("zoom-in", "+ zoom clicked."),
        SignalDef::new("zoom-out", "- zoom clicked."),
        SignalDef::new("zoom-reset", "100% label clicked to reset."),
    ])
}

fn modifier_picker_schema() -> Vec<FieldSpec> {
    vec![
        FieldSpec::boolean("open", "Open").with_default(Value::Bool(false)),
        FieldSpec::text("target-id", "Owning node id"),
        FieldSpec::text(
            "options",
            "Options (JSON array of {id, label, description})",
        ),
    ]
}

fn modifier_header_schema() -> Vec<FieldSpec> {
    vec![
        FieldSpec::text("label", "Behaviour label").required(),
        FieldSpec::text("description", "Description"),
        FieldSpec::text("modifier-id", "Registered behaviour id").required(),
        FieldSpec::integer(
            "modifier-idx",
            "Index in node.modifiers",
            NumericBounds::min(0.0),
        )
        .with_default(Value::from(0.0)),
        FieldSpec::boolean("enabled", "Enabled").with_default(Value::Bool(true)),
        FieldSpec::text("target-id", "Owning node id").required(),
    ]
}

fn app_card_schema() -> Vec<FieldSpec> {
    vec![
        FieldSpec::text("app-id", "App ID").required(),
        FieldSpec::text("name", "Name"),
        FieldSpec::text("description", "Description"),
        FieldSpec::text("icon", "Icon"),
        FieldSpec::text("accent-color", "Accent color")
            .with_default(Value::String("#0060c0".into())),
        FieldSpec::integer(
            "page-count",
            "Page count",
            NumericBounds::min_max(0.0, 999.0),
        )
        .with_default(Value::from(1.0)),
        FieldSpec::boolean("is-create", "Is create card").with_default(Value::Bool(false)),
    ]
}

fn component_palette_signals() -> Vec<SignalDef> {
    with_common_signals(vec![SignalDef::new(
        "item-activated",
        "Palette item picked.",
    )])
}

fn inspector_row_schema() -> Vec<FieldSpec> {
    vec![
        FieldSpec::text("node-id", "Node ID"),
        FieldSpec::text("component-id", "Component ID"),
        FieldSpec::text("kind", "Kind").with_default(Value::String("node".into())),
        FieldSpec::number("depth", "Depth", NumericBounds::min(0.0)).with_default(Value::from(0.0)),
        FieldSpec::boolean("selected", "Selected").with_default(Value::Bool(false)),
        FieldSpec::boolean("show-delete", "Show delete (host-driven hover)")
            .with_default(Value::Bool(false)),
    ]
}

fn inspector_row_signals() -> Vec<SignalDef> {
    with_common_signals(vec![
        SignalDef::new(
            "row-clicked",
            "Row was clicked — host selects the bound node-id.",
        ),
        SignalDef::new(
            "row-right-clicked",
            "Row was right-clicked — host opens a context menu at the (x, y).",
        ),
        SignalDef::new(
            "move-up",
            "Move-up chevron clicked (selected node rows only).",
        ),
        SignalDef::new(
            "move-down",
            "Move-down chevron clicked (selected node rows only).",
        ),
        SignalDef::new(
            "delete-track",
            "Trash clicked (row-kind rows with `show-delete=true`).",
        ),
    ])
}

pub static SHELL_PRISM_UI_COMPONENTS: &[PrismUiSpec] = &[
    PrismUiSpec::new(
        "shell.toolbar-separator",
        include_str!("../../ui/components/toolbar-separator.prism-ui"),
    )
    .schema(no_schema),
    PrismUiSpec::new(
        "shell.help-tooltip",
        include_str!("../../ui/components/help-tooltip.prism-ui"),
    )
    .schema(help_tooltip_schema),
    PrismUiSpec::new(
        "shell.docs-view",
        include_str!("../../ui/components/docs-view.prism-ui"),
    )
    .schema(docs_view_schema),
    PrismUiSpec::new(
        "shell.docs-sidebar",
        include_str!("../../ui/components/docs-sidebar.prism-ui"),
    )
    .schema(docs_sidebar_schema),
    PrismUiSpec::new(
        "shell.toast-stack",
        include_str!("../../ui/components/toast-stack.prism-ui"),
    )
    .schema(no_schema),
    PrismUiSpec::new(
        "shell.launchpad",
        include_str!("../../ui/components/launchpad.prism-ui"),
    )
    .schema(launchpad_schema),
    // Wave 11.2 batch — 7 Tier-1 migrations
    PrismUiSpec::new(
        "shell.explorer",
        include_str!("../../ui/components/explorer.prism-ui"),
    )
    .schema(explorer_schema),
    PrismUiSpec::new(
        "shell.docs-content",
        include_str!("../../ui/components/docs-content.prism-ui"),
    )
    .schema(docs_content_schema),
    PrismUiSpec::new(
        "shell.section-header",
        include_str!("../../ui/components/section-header.prism-ui"),
    )
    .schema(section_header_schema)
    .signals(section_header_signals),
    PrismUiSpec::new(
        "shell.nav-button",
        include_str!("../../ui/components/nav-button.prism-ui"),
    )
    .schema(nav_button_schema)
    .signals(nav_button_signals),
    PrismUiSpec::new(
        "shell.inspector-tree",
        include_str!("../../ui/components/inspector-tree.prism-ui"),
    )
    .schema(inspector_tree_schema),
    PrismUiSpec::new(
        "shell.nav-page-list",
        include_str!("../../ui/components/nav-page-list.prism-ui"),
    )
    .schema(nav_page_list_schema),
    PrismUiSpec::new(
        "shell.signals-panel",
        include_str!("../../ui/components/signals-panel.prism-ui"),
    )
    .schema(signals_panel_schema),
    PrismUiSpec::new(
        "shell.workflow-page-bar",
        include_str!("../../ui/components/workflow-page-bar.prism-ui"),
    )
    .schema(workflow_page_bar_schema),
    PrismUiSpec::new(
        "shell.menu-dropdown",
        include_str!("../../ui/components/menu-dropdown.prism-ui"),
    )
    .schema(items_only_schema),
    PrismUiSpec::new(
        "shell.context-menu",
        include_str!("../../ui/components/context-menu.prism-ui"),
    )
    .schema(items_only_schema),
    PrismUiSpec::new(
        "shell.add-modifier-button",
        include_str!("../../ui/components/add-modifier-button.prism-ui"),
    )
    .schema(add_modifier_button_schema),
    PrismUiSpec::new(
        "shell.add-connection-button",
        include_str!("../../ui/components/add-connection-button.prism-ui"),
    )
    .schema(no_schema),
    // Batch 3 — substrate-unblocked migrations.
    PrismUiSpec::new(
        "shell.toast",
        include_str!("../../ui/components/toast.prism-ui"),
    )
    .schema(toast_schema)
    .signals(toast_signals),
    PrismUiSpec::new(
        "shell.menu-item",
        include_str!("../../ui/components/menu-item.prism-ui"),
    )
    .schema(menu_item_schema),
    PrismUiSpec::new(
        "shell.signal-connection-row",
        include_str!("../../ui/components/signal-connection-row.prism-ui"),
    )
    .schema(signal_connection_row_schema)
    .signals(signal_connection_row_signals),
    PrismUiSpec::new(
        "shell.schema-row",
        include_str!("../../ui/components/schema-row.prism-ui"),
    )
    .schema(schema_row_schema)
    .signals(schema_row_signals),
    PrismUiSpec::new(
        "shell.nav-page-row",
        include_str!("../../ui/components/nav-page-row.prism-ui"),
    )
    .schema(nav_page_row_schema)
    .signals(nav_page_row_signals),
    PrismUiSpec::new(
        "shell.properties-panel",
        include_str!("../../ui/components/properties-panel.prism-ui"),
    )
    .schema(properties_panel_schema),
    // Batch 4 — chrome lift. shell.icon-button moves to DSL, taking
    // tooltip/tint/command/enabled with it; shell.tab-button is the
    // new shared recipe powering shell.dock-tab + shell.workflow-page-button.
    PrismUiSpec::new(
        "shell.icon-button",
        include_str!("../../ui/components/icon-button.prism-ui"),
    )
    .schema(icon_button_schema)
    .signals(icon_button_signals),
    PrismUiSpec::new(
        "shell.tab-button",
        include_str!("../../ui/components/tab-button.prism-ui"),
    )
    .schema(tab_button_schema),
    PrismUiSpec::new(
        "shell.dock-tab",
        include_str!("../../ui/components/dock-tab.prism-ui"),
    )
    .schema(dock_tab_schema),
    PrismUiSpec::new(
        "shell.workflow-page-button",
        include_str!("../../ui/components/workflow-page-button.prism-ui"),
    )
    .schema(workflow_page_button_schema),
    PrismUiSpec::new(
        "shell.status-bar",
        include_str!("../../ui/components/status-bar.prism-ui"),
    )
    .schema(status_bar_schema),
    PrismUiSpec::new(
        "shell.menu-bar-row",
        include_str!("../../ui/components/menu-bar-row.prism-ui"),
    )
    .schema(menu_bar_row_schema)
    .signals(menu_bar_row_signals),
    // Wave 12 — first DSL component authored against the style-prop
    // substrate (`attach_style_overrides` in `prism-builder::ui_resolver`).
    PrismUiSpec::new(
        "shell.component-palette",
        include_str!("../../ui/components/component-palette.prism-ui"),
    )
    .schema(component_palette_schema)
    .signals(component_palette_signals),
    PrismUiSpec::new(
        "shell.inspector-row",
        include_str!("../../ui/components/inspector-row.prism-ui"),
    )
    .schema(inspector_row_schema)
    .signals(inspector_row_signals),
    PrismUiSpec::new(
        "shell.app-card",
        include_str!("../../ui/components/app-card.prism-ui"),
    )
    .schema(app_card_schema),
    PrismUiSpec::new(
        "shell.modifier-header",
        include_str!("../../ui/components/modifier-header.prism-ui"),
    )
    .schema(modifier_header_schema),
    PrismUiSpec::new(
        "shell.modifier-picker",
        include_str!("../../ui/components/modifier-picker.prism-ui"),
    )
    .schema(modifier_picker_schema),
    PrismUiSpec::new(
        "shell.builder-toolbar",
        include_str!("../../ui/components/builder-toolbar.prism-ui"),
    )
    .schema(builder_toolbar_schema)
    .signals(builder_toolbar_signals),
    PrismUiSpec::new(
        "shell.app-window",
        include_str!("../../ui/components/app-window.prism-ui"),
    )
    .schema(app_window_schema)
    .signals(app_window_signals),
    PrismUiSpec::new(
        "shell.dock-divider",
        include_str!("../../ui/components/dock-divider.prism-ui"),
    )
    .schema(dock_divider_schema),
    PrismUiSpec::new(
        "shell.resize-handle",
        include_str!("../../ui/components/resize-handle.prism-ui"),
    )
    .schema(resize_handle_schema)
    .signals(resize_handle_signals),
    PrismUiSpec::new(
        "shell.gizmo-move",
        include_str!("../../ui/components/gizmo-move.prism-ui"),
    )
    .schema(gizmo_move_schema)
    .signals(gizmo_move_signals),
    PrismUiSpec::new(
        "shell.gizmo-rotate",
        include_str!("../../ui/components/gizmo-rotate.prism-ui"),
    )
    .schema(gizmo_rotate_schema)
    .signals(gizmo_rotate_signals),
    PrismUiSpec::new(
        "shell.gizmo-scale",
        include_str!("../../ui/components/gizmo-scale.prism-ui"),
    )
    .schema(gizmo_scale_schema)
    .signals(gizmo_scale_signals),
    PrismUiSpec::new(
        "shell.drag-number-field",
        include_str!("../../ui/components/drag-number-field.prism-ui"),
    )
    .schema(drag_number_field_schema)
    .signals(drag_number_field_signals),
    PrismUiSpec::new(
        "shell.connection-picker",
        include_str!("../../ui/components/connection-picker.prism-ui"),
    )
    .schema(connection_picker_schema),
    PrismUiSpec::new(
        "shell.component-picker",
        include_str!("../../ui/components/component-picker.prism-ui"),
    )
    .schema(component_picker_schema)
    .signals(component_picker_signals),
    PrismUiSpec::new(
        "shell.dock-tab-bar",
        include_str!("../../ui/components/dock-tab-bar.prism-ui"),
    )
    .schema(dock_tab_bar_schema),
];

#[cfg(test)]
mod tests {
    use super::*;

    use crate::components::registry::register_shell_builtins;
    use prism_builder::document::Node as BuilderNode;
    use prism_builder::layout::LayoutMode;
    use prism_core::foundation::spatial::Transform2D;
    use serde_json::json;

    fn build_registry() -> (ShellComponentRegistry, SharedResolver) {
        let mut reg = ShellComponentRegistry::new();
        register_shell_builtins(&mut reg).expect("native shell builtins register");
        let resolver = make_shared_resolver();
        register_prism_ui_components(&mut reg, SHELL_PRISM_UI_COMPONENTS, &resolver)
            .expect("prism-ui components register");
        finalize_prism_ui_resolver(&resolver, &reg);
        (reg, resolver)
    }

    fn lower_from_registry(
        reg: &ShellComponentRegistry,
        id: &str,
        props: serde_json::Value,
    ) -> UiNode {
        let node = BuilderNode {
            id: format!("{id}-test"),
            component: id.into(),
            props,
            children: vec![],
            layout_mode: LayoutMode::default(),
            transform: Transform2D::default(),
            modifiers: vec![],
            style: StyleProperties::default(),
        };
        let cascade = StyleProperties::default();
        let ctx = LowerCtx::new(Some(reg.as_component_registry()), &cascade);
        let comp = reg
            .get(id)
            .unwrap_or_else(|| panic!("`{id}` not registered"));
        comp.lower_ui(&ctx, &node, &cascade)
    }

    #[test]
    fn every_prism_ui_spec_id_uses_shell_namespace() {
        for spec in SHELL_PRISM_UI_COMPONENTS {
            assert!(
                spec.id.starts_with("shell."),
                "id `{}` must use the shell.* namespace",
                spec.id
            );
        }
    }

    #[test]
    fn every_prism_ui_spec_parses_without_errors() {
        for spec in SHELL_PRISM_UI_COMPONENTS {
            let (_, errs) = parse(spec.source);
            assert!(
                errs.is_empty(),
                "`{}` source has parse errors: {errs:?}",
                spec.id
            );
        }
    }

    #[test]
    fn loader_populates_resolver_after_finalize() {
        let (_reg, resolver) = build_registry();
        assert!(
            resolver.get().is_some(),
            "shared resolver cell must be populated after finalize",
        );
    }

    #[test]
    fn prism_ui_specs_register_disjoint_from_native_builtins() {
        // Two id namespaces — native `SHELL_BUILTINS` and the new
        // DSL table — must stay disjoint. `register_specs` /
        // `register` rejects duplicates at runtime via
        // `RegistryError::AlreadyRegistered`, but a named pin here
        // surfaces collisions as a literal-table diff instead of a
        // boot panic.
        use crate::components::registry::SHELL_BUILTINS;
        let native: std::collections::HashSet<&str> = SHELL_BUILTINS.iter().map(|s| s.id).collect();
        for spec in SHELL_PRISM_UI_COMPONENTS {
            assert!(
                !native.contains(spec.id),
                "`{}` is in BOTH SHELL_BUILTINS (Rust) and SHELL_PRISM_UI_COMPONENTS (DSL); \
                 delete the Rust row to complete the migration",
                spec.id
            );
        }
    }

    #[test]
    fn shell_new_render_carries_workflow_page_button_hits() {
        // Production parity: a fresh `Shell::new()` boots the default
        // workspace (6 workflow pages), renders, and the resulting
        // tree must include the dispatched workflow-page-button
        // descendants. Regression guard against the DSL migration
        // dropping the `for` loop or its `props="{item}"` spread.
        let shell = crate::Shell::new().expect("boot");
        let nodes = shell.render();
        // Collect every `data-role` across the rendered tree.
        fn walk(n: &UiNode, sink: &mut Vec<String>) {
            if let UiNode::Container {
                props, children, ..
            } = n
            {
                for (k, v) in &props.semantic.attrs {
                    if k == "data-role" {
                        sink.push(v.clone());
                    }
                }
                for c in children {
                    walk(c, sink);
                }
            }
        }
        let mut roles = Vec::new();
        for n in &nodes {
            walk(n, &mut roles);
        }
        roles.sort();
        roles.dedup();
        assert!(
            roles.iter().any(|r| r == "workflow-page-button"),
            "rendered tree missing workflow-page-button; got {roles:?}"
        );
    }

    #[test]
    fn workflow_page_bar_via_render_tree_pipeline_emits_workflow_page_buttons() {
        // Production parity: drive the DSL block through the same
        // `render_tree` pipeline the shell uses at runtime —
        // `fill_compositions` injects the workflow-page-bar emission
        // as a string `pages="[…]"` attribute, the resolver decodes
        // it via `value_for`'s JSON parse rule, and the DSL block
        // iterates and dispatches to `shell.workflow-page-button`.
        use crate::props::{PropCtx, PropEmission, ShellPropBindings};
        use crate::render::{render_tree, Skeleton};

        let skel =
            Skeleton::from_source(r#"<shell.workflow-page-bar id="workflow"/>"#).expect("parse");

        let (reg, _) = build_registry();
        let bindings = {
            let mut b = ShellPropBindings::default();
            b.register(
                "shell.workflow-page-bar",
                Box::new(|_| {
                    PropEmission::from_props(json!({
                        "pages": [
                            { "page-id": "edit", "label": "Edit", "active": true },
                            { "page-id": "code", "label": "Code" },
                        ],
                    }))
                }),
            );
            b
        };
        let state = crate::AppState::default();
        let ctx = PropCtx {
            state: &state,
            viewport_w: 1280.0,
            viewport_h: 800.0,
            canvas_zoom: 1.0,
            registry: None,
            block_invalidator: None,
            modifier_registry: None,
        };
        let nodes = render_tree(&skel, &bindings, reg.tag_resolver(), &ctx);
        let UiNode::Container { children, .. } = &nodes[0] else {
            panic!("not a container")
        };
        let roles: Vec<String> = children
            .iter()
            .filter_map(|c| match c {
                UiNode::Container { props, .. } => props
                    .semantic
                    .attrs
                    .iter()
                    .find(|(k, _)| k == "data-role")
                    .map(|(_, v)| v.clone()),
                _ => None,
            })
            .collect();
        assert!(
            roles.iter().any(|r| r == "workflow-page-button"),
            "no child carries data-role=workflow-page-button; got {roles:?}; \
             children: {children:#?}"
        );
    }

    #[test]
    fn workflow_page_bar_iterates_pages_and_dispatches_through_resolver() {
        let (reg, _) = build_registry();
        let ui = lower_from_registry(
            &reg,
            "shell.workflow-page-bar",
            json!({
                "pages": [
                    { "page-id": "edit", "label": "Edit", "active": true },
                    { "page-id": "code", "label": "Code" },
                ]
            }),
        );
        let UiNode::Container { children, .. } = ui else {
            panic!("not a container")
        };
        // Two flanking spacers + two dispatched workflow-page-button
        // containers = 4 children.
        assert_eq!(
            children.len(),
            4,
            "expected 2 spacers + 2 page buttons, got {children:#?}"
        );
        // Find the dispatched buttons and verify they carry the
        // expected `data-role` attribute.
        let roles: Vec<String> = children
            .iter()
            .filter_map(|c| match c {
                UiNode::Container { props, .. } => props
                    .semantic
                    .attrs
                    .iter()
                    .find(|(k, _)| k == "data-role")
                    .map(|(_, v)| v.clone()),
                _ => None,
            })
            .collect();
        assert!(
            roles.iter().any(|r| r == "workflow-page-button"),
            "no child carries data-role=workflow-page-button; got {roles:?}"
        );
    }

    #[test]
    fn toolbar_separator_lowers_to_1x20_translucent_stroke() {
        let (reg, _) = build_registry();
        let ui = lower_from_registry(&reg, "shell.toolbar-separator", json!({}));
        let UiNode::Container {
            id,
            props,
            children,
        } = ui
        else {
            panic!("toolbar-separator did not lower to a container")
        };
        assert_eq!(id, "shell.toolbar-separator-test");
        assert_eq!(props.width, prism_ui_runtime::layout::Sizing::Fixed(1.0));
        assert_eq!(props.height, prism_ui_runtime::layout::Sizing::Fixed(20.0));
        assert!(props.background.is_some());
        assert!(children.is_empty());
        assert_eq!(props.semantic.role.as_deref(), Some("separator"));
        let oriented = props
            .semantic
            .attrs
            .iter()
            .any(|(k, v)| k == "aria-orientation" && v == "vertical");
        assert!(oriented, "expected aria-orientation=\"vertical\" attr");
    }
}
