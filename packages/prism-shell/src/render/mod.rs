//! Renderer seams. The host picks one of these at runtime:
//!
//! * Native — `wgpu` on top of any `raw-window-handle` window. Used by
//!   the `prism-shell` dev bin (winit) and the packaged Studio shell
//!   (tao). Lives behind `feature = "native"` and, for the real Clay
//!   rasterisation path, `feature = "clay"`.
//! * Web — wasm-bindgen + `<canvas>`. Tracked separately under
//!   `feature = "web"`; the browser build still uses the count-of-
//!   commands stub until the WebGL/WebGPU spike lands in Phase 1.

#[cfg(all(feature = "native", feature = "clay"))]
pub mod graphics_context;
#[cfg(all(feature = "native", feature = "clay"))]
pub mod ui_renderer;

#[cfg(all(feature = "native", feature = "clay"))]
pub use graphics_context::{GraphicsContext, SharedWindow};
#[cfg(all(feature = "native", feature = "clay"))]
pub use ui_renderer::{UiBorderThickness, UiColor, UiCornerRadii, UiRenderer, UiVertex};

/// Tiny façade so the non-Clay build stays compilable while Phase 0
/// work is in flight. Once the WASM backend comes online it will
/// replace this trait.
pub trait Renderer {
    fn begin_frame(&mut self, width: u32, height: u32);
    fn submit_draw_count(&mut self, count: usize);
    fn end_frame(&mut self);
}
