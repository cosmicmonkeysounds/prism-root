//! `prism-builder` — the Slint-native page builder that replaces Puck.
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
//! * [`html`]         — allocation-light HTML builder for SSR.
//! * [`slint_source`] — `.slint` DSL emitter.
//! * [`render`]       — document-level walkers for both backends.
//! * [`starter`]      — 8 built-in Slint components.
//! * [`html_starter`] — 8 built-in HTML blocks (same component IDs).

pub mod component;
pub mod document;
pub mod html;
pub mod html_block;
pub mod html_starter;
pub mod registry;
pub mod render;
pub mod slint_source;
pub mod starter;

pub use component::{Component, ComponentId, RenderContext, RenderError, RenderSlintContext};
pub use document::{BuilderDocument, Node, NodeId};
pub use html::{escape_attr, escape_text, Html};
pub use html_block::{HtmlBlock, HtmlRegistry, HtmlRenderContext};
pub use html_starter::register_html_builtins;
pub use registry::{
    ComponentRegistry, FieldKind, FieldSpec, FieldValue, NumericBounds, RegistryError, SelectOption,
};
pub use render::{render_document_html, render_document_slint_source};
pub use slint_source::{SlintEmitter, SlintIdent};
pub use starter::register_builtins;

#[cfg(feature = "interpreter")]
pub use render::{compile_slint_source, instantiate_document, InstantiateError};
