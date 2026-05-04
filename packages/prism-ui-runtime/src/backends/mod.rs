//! Pluggable backends. Each consumes the same `Vec<RenderCommand>`
//! produced by `layout::layout` and lowers it to a target surface.

#[cfg(feature = "femtovg")]
pub mod femtovg;

#[cfg(feature = "web")]
pub mod web;

#[cfg(feature = "html")]
pub mod html;
