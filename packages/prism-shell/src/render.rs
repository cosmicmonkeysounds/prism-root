//! Renderer seams. The host picks one of these at runtime:
//!
//! * [`NativeRenderer`] — `wgpu` on top of a `tao` window. Used by the
//!   desktop Tauri shell and the `prism-shell` dev binary.
//! * [`WebRenderer`]    — wasm-bindgen + `<canvas>`. Used by the
//!   browser bundle Trunk emits.
//!
//! Both implement [`Renderer`] so `render_app` doesn't care which one
//! is active. Stub bodies until the Phase 0 spike wires Clay.

pub trait Renderer {
    fn begin_frame(&mut self, width: u32, height: u32);
    fn submit_draw_count(&mut self, count: usize);
    fn end_frame(&mut self);
}

#[cfg(feature = "native")]
pub struct NativeRenderer {
    // TODO(phase0-spike-1): wgpu device / queue / surface / pipeline.
    _stub: (),
}

#[cfg(feature = "native")]
impl NativeRenderer {
    pub fn new() -> Self {
        Self { _stub: () }
    }
}

#[cfg(feature = "native")]
impl Default for NativeRenderer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "native")]
impl Renderer for NativeRenderer {
    fn begin_frame(&mut self, _width: u32, _height: u32) {}
    fn submit_draw_count(&mut self, _count: usize) {}
    fn end_frame(&mut self) {}
}
