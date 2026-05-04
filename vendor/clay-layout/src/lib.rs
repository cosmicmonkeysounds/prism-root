//! Vendored Prism fork of the `clay-layout` Rust binding.
//!
//! **Placeholder crate.** Phase 1 (see ADR-008 / clay-migration-plan)
//! imports the real upstream binding with its C sources. Until then,
//! this crate exposes only enough surface for `prism-ui-runtime` to
//! reference without unresolved imports.

/// Opaque layout-tree handle. Real impl will wrap `Clay_LayoutDimensions`
/// + the arena passed into `Clay_BeginLayout`.
#[derive(Debug, Default, Clone, Copy)]
pub struct LayoutHandle;

/// Layout dimensions in logical pixels.
#[derive(Debug, Default, Clone, Copy)]
pub struct Dimensions {
    pub width: f32,
    pub height: f32,
}
