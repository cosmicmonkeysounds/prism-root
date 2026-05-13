//! `prism-builder` — the page builder.
//!
//! Post-Slint, this crate emits `.prism-ui` source via §29's
//! [`prism_ui_emit`] and lowers documents to `prism_ui_runtime`
//! through the unified Taffy pipeline.
//! [`component::Component::lower_ui`] emits `prism_ui_runtime::layout::Node`
//! trees consumed by the shell renderer *and* by the relay's
//! semantic-HTML SSR walker
//! ([`ui_runtime::lower_semantic_html`]).

pub mod app;
pub mod asset;
pub mod block;
/// Wave 2.4 — RGB ↔ HSL math + hex parse/format used by the color
/// picker's H/S/L slider gestures and by Luau-authored modifiers
/// that compose colour transforms.
pub mod color;
pub mod component;
pub mod core_widget;
pub mod document;
pub mod facet;
pub mod html;
pub mod layout;
pub mod luau_bindings_consts;
#[cfg(feature = "luau")]
pub mod luau_component;
#[cfg(feature = "luau")]
pub mod luau_modifier;
pub mod luau_types;
pub mod modifier;
pub mod modifier_bootstrap;
pub mod mutator;
pub mod prefab;
pub mod primitives;
pub mod prism_ui_emit;
pub mod project;
pub mod reactive_props;
pub mod registry;
pub mod resource;
pub mod schemas;
#[cfg(feature = "luau")]
pub mod script_loader;
pub mod signal;
pub mod starter;
pub mod style;
pub mod template_lower;
pub mod ui_lower;
pub mod ui_resolver;
pub mod ui_runtime;
pub mod variant;

pub use app::{AppIcon, AppId, NavigationConfig, NavigationStyle, Page, PrismApp};
pub use asset::{collect_vfs_hashes, AssetSource};
pub use block::{
    default_lower, default_signals, no_schema, no_variants, register_block, register_specs, Block,
    BlockSpec, HelpDef, LowerFn, SpecBlock,
};
pub use component::{Component, ComponentId, RenderError};
pub use core_widget::{collect_all_contributions, register_core_widgets, CoreWidgetBlock};
pub use document::{BuilderDocument, Node, NodeId};
pub use facet::{
    apply_aggregate, apply_scalar_bindings, collect_expression_fields, evaluate_calculations,
    parse_filter_expr, promote_inline_to_component, resolve_template_expressions, AggregateOp,
    FacetBinding, FacetDataSource, FacetDef, FacetDirection, FacetKind, FacetLayout, FacetOutput,
    FacetRecord, FacetSchema, FacetSchemaId, FacetTemplate, FacetVariantRule, ResolvedFacetData,
    ScriptLanguage, ValidationError, AGGREGATE_OP_TAGS, FACET_KIND_TAGS,
};
pub use html::{escape_attr, escape_text, Html};
pub use layout::{
    compute_layout, compute_track_sizes, path_from_string, path_to_string, AbsoluteProps, CellEdge,
    ComputedLayout, EdgeHandle, FlatCell, FlowProps, GridCell, GridEditError, GridPlacement,
    LayoutMode, NodeLayout, PageLayout, PageSize, SplitDirection, TrackSize,
};
pub use modifier::{
    register_builtins as register_modifier_builtins, register_specs as register_modifier_specs,
    BehaviourSpec, Modifier, ModifierBehaviour, ModifierDescriptor, ModifierId, ModifierKind,
    ModifierRegistry, ModifierRegistryError, SpecBehaviour,
};
pub use mutator::NodeMutator;
pub use prefab::{ExposedSlot, PrefabComponent, PrefabDef};
pub use prism_ui_emit::{emit_document, emit_node};
pub use project::{ProjectFile, FILE_EXTENSION, FORMAT_VERSION};
pub use reactive_props::{DocumentBindings, ReactiveProps};
pub use registry::{
    ComponentRegistry, FieldKind, FieldSpec, FieldValue, FileFieldConfig, NumericBounds,
    RegistryError, SelectOption,
};
pub use resource::{ResourceDef, ResourceId, ResourceKind};
pub use signal::{
    common_signals, dispatch_signal, generate_signal_type_stubs, signal_contexts, signal_symbols,
    with_common_signals, ActionKind, Connection, ConnectionId, DispatchResult, SignalDef,
    SignalEvent,
};
pub use starter::{builtin_prefab, card_prefab_def, materialize_prefab, register_builtins};
pub use style::{resolve_cascade, StyleProperties};
pub use template_lower::lower_template;
pub use variant::{VariantAxis, VariantOption};

#[cfg(feature = "luau")]
pub use luau_component::{ActiveRegistry, LuauComponent, LuauRenderRegistry, VirtualNode};
#[cfg(feature = "luau")]
pub use luau_modifier::{
    generate_modifier_type_stubs, register_modifier_from_luau, ActiveModifierRegistry,
    LuauModifier, LuauModifierDef, LuauModifierRegistry,
};
#[cfg(feature = "luau")]
pub use script_loader::{
    load_automations, load_build_steps, load_commands, load_widgets, reload_widget_file,
    LoadReport, ScriptLoadError,
};
