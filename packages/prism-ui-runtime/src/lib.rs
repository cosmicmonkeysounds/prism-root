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
pub mod interpret;
pub mod layout;
#[cfg(feature = "luau")]
pub mod luau;
pub mod luau_types;

// Render-command → femtovg paint translation + cosmic-text glyph
// cache. Shared by the native (`femtovg`) and web (`web`) backends;
// gated together so a host that pulls just `html` doesn't drag in
// femtovg / cosmic-text.
#[cfg(any(feature = "femtovg", feature = "web"))]
pub mod paint;
#[cfg(any(feature = "femtovg", feature = "web"))]
pub mod text;

#[cfg(test)]
mod tests {
    use crate::command::{Color, RenderCommand};
    use crate::layout::{
        compute, ContainerProps, Direction, Node, Padding, Sizing, TextProps, Viewport,
    };

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

    /// Phase 1 acceptance test (per `docs/dev/clay-migration-plan.md`):
    /// "Hand-build a 5-element scene in Rust (no DSL yet) and render
    /// it to all three backends. Snapshot tests on the HTML backend."
    ///
    /// This walks the same scene through `compute` → `backends::html::lower`
    /// and snapshots the resulting HTML string with `insta`. Re-run with
    /// `cargo insta review` after intentional layout / lowering changes.
    #[cfg(feature = "html")]
    #[test]
    fn five_element_scene_html_snapshot() {
        let tree = Node::Container {
            id: "root".into(),
            props: ContainerProps {
                direction: Direction::Column,
                gap: 8.0,
                padding: Padding::all(16.0),
                width: Sizing::Grow,
                height: Sizing::Grow,
                background: Some(Color {
                    r: 240,
                    g: 240,
                    b: 240,
                    a: 255,
                }),
                ..Default::default()
            },
            children: vec![
                Node::Text {
                    id: "title".into(),
                    content: "Prism".into(),
                    props: TextProps {
                        font_size: 24.0,
                        color: Color {
                            r: 20,
                            g: 20,
                            b: 20,
                            a: 255,
                        },
                    },
                },
                Node::Container {
                    id: "row".into(),
                    props: ContainerProps {
                        direction: Direction::Row,
                        gap: 8.0,
                        width: Sizing::Grow,
                        height: Sizing::Fixed(40.0),
                        background: Some(Color {
                            r: 255,
                            g: 255,
                            b: 255,
                            a: 255,
                        }),
                        ..Default::default()
                    },
                    children: vec![
                        Node::Text {
                            id: "a".into(),
                            content: "A".into(),
                            props: TextProps::default(),
                        },
                        Node::Spacer {
                            id: "gap".into(),
                            width: 16.0,
                            height: 0.0,
                        },
                        Node::Text {
                            id: "b".into(),
                            content: "B".into(),
                            props: TextProps::default(),
                        },
                    ],
                },
            ],
        };
        let cmds = compute(
            &tree,
            Viewport {
                width: 800.0,
                height: 600.0,
            },
        );
        let html = crate::backends::html::lower(&cmds);
        insta::assert_snapshot!(html);
    }
}
