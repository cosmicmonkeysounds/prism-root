//! `prism-shell` — single source of truth for the Prism UI tree.
//!
//! Every Studio panel, the page builder, every lens — all of it lives
//! behind [`render_app`], which takes the current [`AppState`] and a
//! borrowed [`clay_layout::Clay`] instance and returns a vector of
//! `RenderCommand`s. Downstream renderers (wgpu on native, a
//! canvas-based wasm-bindgen glue on web) only walk that vector and
//! turn it into pixels.
//!
//! Phase 0 goal: render one hard-coded panel (a sidebar with three
//! buttons) and forward mouse/keyboard events into Clay's input API.
//! Anything beyond that lives behind TODOs until Phase 1.

pub mod app;
pub mod input;
pub mod panels;
pub mod render;

#[cfg(feature = "web")]
pub mod web;

pub use app::AppState;

#[cfg(feature = "clay")]
pub use app::{install_stub_text_measurer, render_app};

// Re-export the Clay type when the feature is on so downstream
// crates (the native dev bin, the Studio shell) don't have to
// depend on `clay-layout` directly to construct a Clay instance.
#[cfg(feature = "clay")]
pub use clay_layout::Clay;
