//! `prism-builder` — the page builder.
//!
//! Two render targets, one declaration:
//!
//! * **Slint DSL** — [`component::Component::render_slint`] feeds Studio's
//!   live builder via [`slint_source::SlintEmitter`].
//! * **Unified Taffy pipeline** — [`component::Component::lower_ui`] emits
//!   `prism_ui_runtime::layout::Node` trees consumed by the shell renderer
//!   *and* by the relay's semantic-HTML SSR walker
//!   ([`ui_runtime::lower_semantic_html_with_registry`]).
//!
//! See `docs/dev/clay-migration-plan.md` for the migration history that
//! collapsed the parallel `HtmlBlock` / `HtmlRegistry` SSR walker into
//! the unified pipeline (Phase 5).

pub mod app;
pub mod asset;
pub mod block;
pub mod component;
pub mod core_widget;
pub mod document;
pub mod facet;
pub mod html;
pub mod layout;
#[cfg(feature = "luau")]
pub mod luau_component;
pub mod luau_types;
pub mod modifier;
pub mod prefab;
pub mod project;
pub mod registry;
pub mod render;
pub mod resource;
pub mod schemas;
#[cfg(feature = "luau")]
pub mod script_loader;
pub mod signal;
pub mod slint_source;
pub mod source_map;
pub mod source_parse;
pub mod starter;
pub mod style;
pub mod ui_lower;
pub mod ui_runtime;
pub mod variant;

pub use app::{AppIcon, AppId, NavigationConfig, NavigationStyle, Page, PrismApp};
pub use asset::{collect_vfs_hashes, AssetSource};
pub use block::{register_block, Block};
pub use component::{Component, ComponentId, RenderContext, RenderError, RenderSlintContext};
pub use core_widget::{
    collect_all_contributions, register_core_widgets, render_template_node, CoreWidgetBlock,
};
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
pub use modifier::{Modifier, ModifierKind};
pub use prefab::{ExposedSlot, PrefabComponent, PrefabDef};
pub use project::{ProjectFile, FILE_EXTENSION, FORMAT_VERSION};
pub use registry::{
    ComponentRegistry, FieldKind, FieldSpec, FieldValue, FileFieldConfig, NumericBounds,
    RegistryError, SelectOption,
};
pub use render::{
    build_source_map_from_markers, render_document_slint_preview,
    render_document_slint_preview_with_assets, render_document_slint_preview_with_assets_and_data,
    render_document_slint_source, render_document_slint_source_mapped,
};
pub use resource::{ResourceDef, ResourceId, ResourceKind};
pub use signal::{
    common_signals, dispatch_signal, generate_signal_type_stubs, signal_contexts, signal_symbols,
    with_common_signals, ActionKind, Connection, ConnectionId, DispatchResult, SignalDef,
    SignalEvent,
};
pub use slint_source::{SlintEmitter, SlintIdent};
pub use source_map::{MappedEmitter, PropSpan, SourceMap, SourceSpan};
pub use source_parse::{derive_document_from_source, format_slint_value, parse_slint_value};
pub use starter::{builtin_prefab, card_prefab_def, materialize_prefab, register_builtins};
pub use style::{resolve_cascade, StyleProperties};
pub use variant::{VariantAxis, VariantOption};

#[cfg(feature = "interpreter")]
pub mod live;
#[cfg(feature = "interpreter")]
pub mod syntax_provider;

#[cfg(feature = "interpreter")]
pub use live::{LiveDiagnostic, LiveDocument, SourceEditError, SourceSelection};
#[cfg(feature = "interpreter")]
pub use render::{
    compile_slint_preview, compile_slint_source, instantiate_document, preview_component_factory,
    InstantiateError,
};
#[cfg(feature = "interpreter")]
pub use syntax_provider::BuilderSyntaxProvider;

#[cfg(feature = "luau")]
pub use luau_component::{ActiveRegistry, LuauComponent, LuauRenderRegistry, VirtualNode};
#[cfg(feature = "luau")]
pub use script_loader::{load_widgets, LoadReport, ScriptLoadError};
