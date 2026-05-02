//! `prism-builder` — the Slint-native page builder.
//!
//! Two independent render paths:
//!
//! * **Slint** — [`component::Component`] + [`registry::ComponentRegistry`].
//!   Components emit `.slint` DSL via [`slint_source::SlintEmitter`]; the
//!   document walker compiles the result through `slint-interpreter`.
//! * **HTML SSR** — [`html_block::HtmlBlock`] + [`html_block::HtmlRegistry`].
//!   Separate trait + registry used by `prism-relay` for Sovereign Portals.
//!   Decoupled from Slint so the relay's dep graph stays Slint-free.
//!
//! Supporting modules:
//! * [`document`]     — the serializable document tree.
//! * [`layout`]       — page grid, per-node layout mode, Taffy computation pass.
//! * [`html`]         — allocation-light HTML builder for SSR.
//! * [`slint_source`] — `.slint` DSL emitter.
//! * [`render`]       — document-level walkers for both backends.
//! * [`starter`]      — 15 built-in Slint components (14 + card prefab).
//! * [`html_starter`] — 15 built-in HTML blocks (14 + card prefab).

pub mod app;
pub mod asset;
pub mod component;
pub mod core_widget;
pub mod document;
pub mod facet;
pub mod html;
pub mod html_block;
pub mod html_starter;
pub mod layout;
pub mod modifier;
pub mod prefab;
pub mod project;
pub mod registry;
pub mod render;
pub mod resource;
pub mod schemas;
pub mod signal;
pub mod slint_source;
pub mod source_map;
pub mod source_parse;
pub mod starter;
pub mod style;
pub mod variant;

pub use app::{AppIcon, AppId, NavigationConfig, NavigationStyle, Page, PrismApp};
pub use asset::{collect_vfs_hashes, AssetSource};
pub use component::{Component, ComponentId, RenderContext, RenderError, RenderSlintContext};
pub use core_widget::{collect_all_contributions, register_core_widgets, CoreWidgetComponent};
pub use document::{BuilderDocument, Node, NodeId};
pub use facet::{
    apply_aggregate, evaluate_calculations, evaluate_filter, get_field, value_sort_key,
    AggregateOp, FacetBinding, FacetDataSource, FacetDef, FacetDirection, FacetKind, FacetLayout,
    FacetRecord, FacetSchema, FacetSchemaId, FacetVariantRule, ResolvedFacetData, ScriptLanguage,
    ValidationError, AGGREGATE_OP_TAGS, FACET_KIND_TAGS,
};
pub use html::{escape_attr, escape_text, Html};
pub use html_block::{HtmlBlock, HtmlRegistry, HtmlRenderContext};
pub use html_starter::register_html_builtins;
pub use layout::{
    compute_layout, compute_track_sizes, path_from_string, path_to_string, AbsoluteProps, CellEdge,
    ComputedLayout, EdgeHandle, FlatCell, FlowProps, GridCell, GridEditError, GridPlacement,
    LayoutMode, NodeLayout, PageLayout, PageSize, SplitDirection, TrackSize,
};
pub use modifier::{Modifier, ModifierKind};
pub use prefab::{ExposedSlot, PrefabComponent, PrefabDef, PrefabHtmlBlock};
pub use project::{ProjectFile, FILE_EXTENSION, FORMAT_VERSION};
pub use registry::{
    ComponentRegistry, FieldKind, FieldSpec, FieldValue, FileFieldConfig, NumericBounds,
    RegistryError, SelectOption,
};
pub use render::{
    build_source_map_from_markers, render_document_html, render_document_slint_preview,
    render_document_slint_preview_with_assets, render_document_slint_source,
    render_document_slint_source_mapped,
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
