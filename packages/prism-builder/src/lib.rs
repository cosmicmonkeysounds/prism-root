//! `prism-builder` — the Clay-native page builder that replaces Puck.
//!
//! Scope (per `docs/dev/clay-migration-plan.md` §8):
//!
//! * [`registry`] — component-type registry, DI entry point, field factories.
//! * [`component`] — the `Component` trait every renderable block implements.
//! * [`document`]  — the serializable document tree (the thing saved to disk).
//! * [`puck_json`] — one-way reader for legacy Puck `{ type, props, children }`
//!   documents. We read Puck JSON forever; new docs are written in our native
//!   schema.
//!
//! Everything here is scaffold: types and traits are defined, bodies are
//! stubs until Phase 3 lands.

pub mod component;
pub mod document;
pub mod puck_json;
pub mod registry;

pub use component::{Component, ComponentId, RenderContext};
pub use document::{BuilderDocument, Node, NodeId};
pub use registry::{ComponentRegistry, RegistryError};
