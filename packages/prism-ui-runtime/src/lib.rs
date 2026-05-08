//! # prism-ui-runtime
//!
//! Taffy-backed layout + pluggable backends for Prism's UI layer.
//! See ADR-008 and `docs/dev/clay-migration-plan.md` (and the
//! 2026-05-04 pivot note at the top of that doc — the engine is
//! Taffy, not Clay; everything else stays as written).
//!
//! ## Pipeline
//!
//! ```text
//! BuilderDocument ──► layout::compute ──► taffy layout pass ──► Vec<RenderCommand>
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

    /// Sanity check that the **Taffy** layout engine is wired through
    /// `compute` end-to-end. Builds a one-container scene with a
    /// background, runs a layout, and confirms a `Rectangle` command
    /// comes back with the viewport-sized bounds. If the Taffy
    /// integration ever regresses (new Taffy version breaks the
    /// `Style` shape, the measure callback signature changes, etc.)
    /// this catches it immediately.
    #[test]
    fn taffy_layout_pass_runs() {
        use crate::command::{Color, RenderCommand};
        use crate::layout::{compute, ContainerProps, Node, Sizing, Viewport};

        let tree = Node::Container {
            id: "r".into(),
            props: ContainerProps {
                width: Sizing::Grow,
                height: Sizing::Grow,
                background: Some(Color {
                    r: 1,
                    g: 2,
                    b: 3,
                    a: 255,
                }),
                ..Default::default()
            },
            children: vec![],
        };
        let cmds = compute(
            &tree,
            Viewport {
                width: 320.0,
                height: 240.0,
            },
        );
        assert_eq!(cmds.len(), 1);
        let RenderCommand::Rectangle { bounds, color, .. } = &cmds[0] else {
            panic!("expected rectangle, got {:?}", cmds[0]);
        };
        assert_eq!(bounds.width, 320.0);
        assert_eq!(bounds.height, 240.0);
        assert_eq!(color.r, 1);
    }
}
