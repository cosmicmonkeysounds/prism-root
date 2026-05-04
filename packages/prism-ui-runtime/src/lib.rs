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
#[cfg(feature = "luau")]
pub mod luau;

#[cfg(test)]
mod tests {
    #[test]
    fn crate_compiles() {}

    /// Sanity check that the **vendored fork** of `clay-layout` is
    /// actually linked and callable — not the placeholder stub the
    /// crate started life as. Spins up a real Clay context, runs an
    /// empty layout pass, and confirms the C library returned a
    /// (possibly-empty) command iterator. If the FFI / `cc` build of
    /// `clay.h` ever regresses, this catches it immediately.
    #[test]
    fn vendored_clay_binding_is_live() {
        use clay_layout::math::Dimensions;
        use clay_layout::Clay;

        let mut clay = Clay::new(Dimensions {
            width: 200.0,
            height: 100.0,
        });
        let mut scope = clay.begin::<(), ()>();
        let count = scope.end().count();
        // Empty root — Clay should produce zero render commands.
        assert_eq!(count, 0);
    }
}
