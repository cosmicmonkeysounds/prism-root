//! `prism-shell` — single source of truth for the Prism UI tree.
//!
//! Every Studio panel, the page builder, every lens — all of it lives
//! behind [`render_app`], which takes the current [`AppState`] and
//! returns a tree of Clay layout calls. Downstream renderers (wgpu on
//! native, a canvas-based wasm-bindgen glue on web) only know how to
//! walk that tree and turn it into pixels.
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

pub use app::{AppState, render_app};
