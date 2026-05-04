//! Web (wasm) backend — Phase 1 stub.
//!
//! Lowers `Vec<RenderCommand>` onto a `<canvas>` via winit's web
//! target plus a WebGL2 / canvas2d adapter. Implementation lands with
//! the Phase 1 runtime spike (see `docs/dev/clay-migration-plan.md`);
//! this stub keeps `cargo fmt` and the cargo feature gate honest
//! until then.
