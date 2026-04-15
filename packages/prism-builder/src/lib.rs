//! `prism-builder` — the Slint-native page builder that replaces Puck.
//!
//! Scope (per `docs/dev/slint-migration-plan.md` §8):
//!
//! * [`registry`]    — component-type registry, DI entry point, field factories.
//! * [`component`]   — the `Component` trait every renderable block implements,
//!   with two render targets: Slint (Studio, live via `slint-interpreter`) and
//!   HTML (Sovereign Portal SSR, live).
//! * [`document`]    — the serializable document tree (the thing saved to disk).
//! * [`html`]        — allocation-light HTML builder used by the SSR render path.
//! * [`slint_source`] — Slint DSL emitter that walks a [`BuilderDocument`] and
//!   produces a self-contained `.slint` source string via
//!   `prism_core::language::codegen::SourceBuilder`.
//! * [`render`]      — document-level walkers that turn a [`BuilderDocument`]
//!   into rendered output for a given backend: HTML today, live
//!   [`slint_interpreter::ComponentInstance`] as of Phase 3.
//!
//! Phase 3 landed the Slint walker: each [`Component`] emits a DSL snippet
//! into a shared [`slint_source::SlintEmitter`]; the walker compiles the
//! synthesized source via [`slint_interpreter::Compiler`] and hands back a
//! live component instance the Studio shell plugs into its builder panel.
//! The HTML render path is untouched and is still what `prism-relay` uses
//! to serve Sovereign Portals.

pub mod component;
pub mod document;
pub mod html;
pub mod registry;
pub mod render;
pub mod slint_source;
pub mod starter;

pub use component::{
    Component, ComponentId, RenderContext, RenderError, RenderHtmlContext, RenderSlintContext,
};
pub use document::{BuilderDocument, Node, NodeId};
pub use html::{escape_attr, escape_text, Html};
pub use registry::{
    ComponentRegistry, FieldKind, FieldSpec, FieldValue, NumericBounds, RegistryError, SelectOption,
};
pub use render::{render_document_html, render_document_slint_source};
pub use slint_source::{SlintEmitter, SlintIdent};
pub use starter::register_builtins;

#[cfg(feature = "interpreter")]
pub use render::{compile_slint_source, instantiate_document, InstantiateError};
