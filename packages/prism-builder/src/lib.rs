//! `prism-builder` — the Slint-native page builder that replaces Puck.
//!
//! Scope (per `docs/dev/slint-migration-plan.md` §8):
//!
//! * [`registry`] — component-type registry, DI entry point, field factories.
//! * [`component`] — the `Component` trait every renderable block implements,
//!   with two render targets: Slint (Studio, stub) and HTML (Sovereign Portal
//!   SSR, live).
//! * [`document`]  — the serializable document tree (the thing saved to disk).
//! * [`html`]      — allocation-light HTML builder used by the SSR render path.
//! * [`render`]    — document-level walker that turns a `BuilderDocument` into
//!   rendered output for a given backend.
//! * [`puck_json`] — one-way reader for legacy Puck `{ type, props, children }`
//!   documents. We read Puck JSON forever; new docs are written in our native
//!   schema.
//!
//! Slint-side render bodies are still stubs until Phase 3 materialises a
//! `BuilderDocument` into a runtime-compiled Slint component tree via
//! `slint-interpreter`; the HTML render path is live and is what
//! `prism-relay` calls to serve Sovereign Portals.

pub mod component;
pub mod document;
pub mod html;
pub mod puck_json;
pub mod registry;
pub mod render;

pub use component::{Component, ComponentId, RenderContext, RenderError, RenderHtmlContext};
pub use document::{BuilderDocument, Node, NodeId};
pub use html::{escape_attr, escape_text, Html};
pub use registry::{ComponentRegistry, RegistryError};
pub use render::render_document_html;
