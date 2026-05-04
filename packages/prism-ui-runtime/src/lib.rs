//! # prism-ui-runtime
//!
//! Clay-backed layout + pluggable backends for Prism's UI layer.
//! See ADR-008 and `docs/dev/clay-migration-plan.md`.
//!
//! ## Pipeline
//!
//! ```text
//! BuilderDocument ──► layout::build_tree ──► clay layout pass ──► Vec<RenderCommand>
//!                                                                       │
//!                                              ┌────────────────────────┼────────────────────────┐
//!                                              ▼                        ▼                        ▼
//!                                       backends::femtovg        backends::web            backends::html
//!                                          (native)                  (wasm)                  (SSR)
//! ```
//!
//! All three backends consume the same render-command stream. The
//! HTML backend is a pure function and the only thing `prism-relay`
//! needs to call.

pub mod backends;
pub mod command;
pub mod event;
pub mod layout;

#[cfg(test)]
mod tests {
    #[test]
    fn crate_compiles() {}
}
