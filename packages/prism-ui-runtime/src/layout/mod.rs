//! Layout pass — typed `UiTree` of `Node`s plus a flex-style layout
//! engine that emits a backend-neutral `Vec<RenderCommand>`.
//!
//! ## Retained-mode contract
//!
//! Layout is **not** recomputed every frame. Per the Clay-migration
//! plan's runtime model (and explicit guidance from the design
//! conversation on 2026-05-04), the layout cache is invalidated only
//! when something changes: tree mutation, viewport resize, scroll, an
//! animation tick, or an explicit `invalidate()`. Backends pull the
//! cached `&[RenderCommand]` slice each frame and only pay the layout
//! cost on a dirty cycle. See [`Surface`].
//!
//! ## Clay vs. this module
//!
//! Phase 1 of the migration vendors `clay-layout` as a placeholder
//! (`vendor/clay-layout/`). Until the real Clay C sources land, this
//! module ships a small hand-rolled flex layout that emits the *same*
//! `RenderCommand` shape Clay will. Swapping in real Clay is a
//! drop-in replacement for [`compute`] — every other surface in the
//! crate (the [`Surface`] retained-mode wrapper, the backends, the
//! HTML lowering) is layout-engine-agnostic.
//!
//! ## Luau
//!
//! `Node` / `ContainerProps` / `TextProps` are deliberately
//! `serde`-friendly value types (no lifetimes, no trait objects) so
//! `prism-luau-derive` can wrap them as `mlua::UserData` for Luau
//! authoring of UI trees. See `feedback_clay_luau_authoring` in
//! auto-memory for the full requirement.

use serde::{Deserialize, Serialize};

use crate::command::{Color, CornerRadius, Rect, RenderCommand};

/// Logical pixel dimensions of the rendering surface.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Viewport {
    pub width: f32,
    pub height: f32,
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            width: 0.0,
            height: 0.0,
        }
    }
}

/// One element in the UI tree. Mirrors Clay's element vocabulary —
/// containers (flex-style layout parents), text leaves, and explicit
/// spacers. Images and scroll containers land in the same enum once
/// the real Clay binding arrives.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Node {
    Container {
        /// Stable identifier — survives re-layout, used by hit-testing,
        /// inspector addressing, and Luau handles. Empty string means
        /// "anonymous" (assigned a path-based id by the layout pass).
        #[serde(default)]
        id: String,
        props: ContainerProps,
        children: Vec<Node>,
    },
    Text {
        #[serde(default)]
        id: String,
        content: String,
        props: TextProps,
    },
    Spacer {
        #[serde(default)]
        id: String,
        width: f32,
        height: f32,
    },
}

impl Node {
    pub fn id(&self) -> &str {
        match self {
            Node::Container { id, .. } | Node::Text { id, .. } | Node::Spacer { id, .. } => id,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Row,
    #[default]
    Column,
}

/// CSS-style box-model padding in logical pixels.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Padding {
    pub left: f32,
    pub right: f32,
    pub top: f32,
    pub bottom: f32,
}

impl Padding {
    pub fn all(v: f32) -> Self {
        Self {
            left: v,
            right: v,
            top: v,
            bottom: v,
        }
    }
}

/// Sizing policy for a single axis. Mirrors Clay's `Sizing` modes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", content = "value", rename_all = "snake_case")]
pub enum Sizing {
    /// Tight to children's intrinsic size.
    #[default]
    Fit,
    /// Fill available space along the axis (flex-grow analogue).
    Grow,
    /// Exact pixel size.
    Fixed(f32),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct ContainerProps {
    #[serde(default)]
    pub direction: Direction,
    #[serde(default)]
    pub gap: f32,
    #[serde(default)]
    pub padding: Padding,
    #[serde(default)]
    pub width: Sizing,
    #[serde(default)]
    pub height: Sizing,
    #[serde(default)]
    pub background: Option<Color>,
    #[serde(default)]
    pub radius: CornerRadius,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TextProps {
    pub font_size: f32,
    pub color: Color,
}

impl Default for TextProps {
    fn default() -> Self {
        Self {
            font_size: 14.0,
            color: Color {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            },
        }
    }
}

/// Compute layout for `tree` against `viewport` and emit a backend-
/// neutral render-command stream.
///
/// **Replaceable seam.** When the real Clay C binding is wired up,
/// this function becomes a thin adapter over `Clay_BeginLayout` /
/// `Clay_EndLayout`. The hand-rolled flex pass below covers Phase 1's
/// 5-element-scene acceptance and gives the rest of the crate
/// something to integration-test against.
pub fn compute(tree: &Node, viewport: Viewport) -> Vec<RenderCommand> {
    let mut out = Vec::new();
    let bounds = Rect {
        x: 0.0,
        y: 0.0,
        width: viewport.width,
        height: viewport.height,
    };
    layout_node(tree, bounds, &mut out);
    out
}

fn layout_node(node: &Node, bounds: Rect, out: &mut Vec<RenderCommand>) {
    match node {
        Node::Container {
            props, children, ..
        } => {
            if let Some(bg) = props.background {
                out.push(RenderCommand::Rectangle {
                    bounds,
                    color: bg,
                    radius: props.radius,
                });
            }
            let inner = Rect {
                x: bounds.x + props.padding.left,
                y: bounds.y + props.padding.top,
                width: (bounds.width - props.padding.left - props.padding.right).max(0.0),
                height: (bounds.height - props.padding.top - props.padding.bottom).max(0.0),
            };
            layout_children(props, &inner, children, out);
        }
        Node::Text { content, props, .. } => {
            out.push(RenderCommand::Text {
                bounds,
                content: content.clone(),
                color: props.color,
                font_size: props.font_size,
            });
        }
        Node::Spacer { .. } => {}
    }
}

/// One-pass flex sizing: fixed children take their pixels, fit
/// children take an intrinsic estimate, grow children share the
/// remainder. Good enough for the 5-element scene; real Clay handles
/// the tricky cases (min/max, percentages, wrapping).
fn layout_children(
    props: &ContainerProps,
    inner: &Rect,
    children: &[Node],
    out: &mut Vec<RenderCommand>,
) {
    if children.is_empty() {
        return;
    }
    let axis_extent = match props.direction {
        Direction::Row => inner.width,
        Direction::Column => inner.height,
    };
    let total_gap = props.gap * (children.len().saturating_sub(1)) as f32;

    let intrinsic: Vec<f32> = children.iter().map(|c| intrinsic_main(c, props)).collect();
    let grow_count = children.iter().filter(|c| is_grow(c, props)).count();

    let fixed_total: f32 = children
        .iter()
        .zip(&intrinsic)
        .map(|(c, sz)| if is_grow(c, props) { 0.0 } else { *sz })
        .sum();
    let leftover = (axis_extent - fixed_total - total_gap).max(0.0);
    let grow_each = if grow_count > 0 {
        leftover / grow_count as f32
    } else {
        0.0
    };

    let mut cursor = match props.direction {
        Direction::Row => inner.x,
        Direction::Column => inner.y,
    };
    for (child, intrinsic_size) in children.iter().zip(&intrinsic) {
        let main_size = if is_grow(child, props) {
            grow_each
        } else {
            *intrinsic_size
        };
        let cross_size = match props.direction {
            Direction::Row => inner.height,
            Direction::Column => inner.width,
        };
        let child_bounds = match props.direction {
            Direction::Row => Rect {
                x: cursor,
                y: inner.y,
                width: main_size,
                height: cross_size,
            },
            Direction::Column => Rect {
                x: inner.x,
                y: cursor,
                width: cross_size,
                height: main_size,
            },
        };
        layout_node(child, child_bounds, out);
        cursor += main_size + props.gap;
    }
}

fn is_grow(node: &Node, parent: &ContainerProps) -> bool {
    match node {
        Node::Container { props, .. } => {
            matches!(axis_sizing(props, parent.direction), Sizing::Grow)
        }
        _ => false,
    }
}

fn intrinsic_main(node: &Node, parent: &ContainerProps) -> f32 {
    match node {
        Node::Container {
            props, children, ..
        } => match axis_sizing(props, parent.direction) {
            Sizing::Fixed(v) => v,
            Sizing::Grow => 0.0,
            Sizing::Fit => {
                let pad = match parent.direction {
                    Direction::Row => props.padding.left + props.padding.right,
                    Direction::Column => props.padding.top + props.padding.bottom,
                };
                let gap = props.gap * children.len().saturating_sub(1) as f32;
                let kids: f32 = children.iter().map(|c| intrinsic_main(c, props)).sum();
                pad + gap + kids
            }
        },
        Node::Text { content, props, .. } => match parent.direction {
            // Crude width estimate — real Clay uses a measure callback
            // into the active text shaper. Phase 1 placeholder.
            Direction::Row => content.chars().count() as f32 * props.font_size * 0.55,
            Direction::Column => props.font_size * 1.2,
        },
        Node::Spacer { width, height, .. } => match parent.direction {
            Direction::Row => *width,
            Direction::Column => *height,
        },
    }
}

fn axis_sizing(props: &ContainerProps, direction: Direction) -> Sizing {
    match direction {
        Direction::Row => props.width,
        Direction::Column => props.height,
    }
}

/// Retained-mode rendering surface.
///
/// Holds a typed `Node` tree, a viewport, and the most recently
/// computed `Vec<RenderCommand>`. Layout is recomputed only when the
/// surface is **dirty** — i.e. on `set_tree`, `set_viewport`,
/// explicit `invalidate()`, or a future scroll/animation tick. This
/// is the explicit retained-mode contract: backends call
/// [`Surface::commands`] every frame, but layout cost is only paid on
/// a change.
#[derive(Debug, Clone)]
pub struct Surface {
    tree: Node,
    viewport: Viewport,
    cache: Vec<RenderCommand>,
    dirty: bool,
}

impl Surface {
    pub fn new(tree: Node, viewport: Viewport) -> Self {
        Self {
            tree,
            viewport,
            cache: Vec::new(),
            dirty: true,
        }
    }

    pub fn tree(&self) -> &Node {
        &self.tree
    }

    pub fn viewport(&self) -> Viewport {
        self.viewport
    }

    pub fn set_tree(&mut self, tree: Node) {
        self.tree = tree;
        self.dirty = true;
    }

    /// Mutate the tree in place. The closure is called with a mutable
    /// reference; the surface is marked dirty unconditionally — there
    /// is no diffing here, that's the caller's job. Designed so a
    /// Luau handler can grab the root, mutate sub-trees, and let the
    /// next `commands()` call pick up the change.
    pub fn with_tree_mut<F: FnOnce(&mut Node)>(&mut self, f: F) {
        f(&mut self.tree);
        self.dirty = true;
    }

    pub fn set_viewport(&mut self, viewport: Viewport) {
        if viewport != self.viewport {
            self.viewport = viewport;
            self.dirty = true;
        }
    }

    /// Force a recompute on the next `commands()` call. Hook for
    /// scroll, animation ticks, theme changes, anything that doesn't
    /// flow through `set_tree` / `set_viewport`.
    pub fn invalidate(&mut self) {
        self.dirty = true;
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Lazy accessor — recomputes layout iff dirty, otherwise returns
    /// the cached slice. **This is the hot-path API the backends
    /// call.** Calling it every frame is cheap when nothing changed.
    pub fn commands(&mut self) -> &[RenderCommand] {
        if self.dirty {
            self.cache = compute(&self.tree, self.viewport);
            self.dirty = false;
        }
        &self.cache
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rgb(r: u8, g: u8, b: u8) -> Color {
        Color { r, g, b, a: 255 }
    }

    fn five_element_scene() -> Node {
        Node::Container {
            id: "root".into(),
            props: ContainerProps {
                direction: Direction::Column,
                gap: 8.0,
                padding: Padding::all(16.0),
                width: Sizing::Grow,
                height: Sizing::Grow,
                background: Some(rgb(240, 240, 240)),
                ..Default::default()
            },
            children: vec![
                Node::Text {
                    id: "title".into(),
                    content: "Prism".into(),
                    props: TextProps {
                        font_size: 24.0,
                        color: rgb(20, 20, 20),
                    },
                },
                Node::Container {
                    id: "row".into(),
                    props: ContainerProps {
                        direction: Direction::Row,
                        gap: 8.0,
                        width: Sizing::Grow,
                        height: Sizing::Fixed(40.0),
                        background: Some(rgb(255, 255, 255)),
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
        }
    }

    #[test]
    fn five_element_scene_lays_out() {
        let mut surface = Surface::new(
            five_element_scene(),
            Viewport {
                width: 800.0,
                height: 600.0,
            },
        );
        let commands = surface.commands();
        // Root background + row background + 3 text leaves = 5 commands.
        assert_eq!(commands.len(), 5);
    }

    #[test]
    fn surface_is_retained_layout_only_runs_when_dirty() {
        let mut surface = Surface::new(
            five_element_scene(),
            Viewport {
                width: 800.0,
                height: 600.0,
            },
        );
        assert!(surface.is_dirty());
        let _ = surface.commands();
        assert!(!surface.is_dirty(), "after compute, surface is clean");
        let _ = surface.commands();
        assert!(!surface.is_dirty(), "second call is a no-op");

        surface.set_viewport(Viewport {
            width: 1024.0,
            height: 768.0,
        });
        assert!(surface.is_dirty(), "resize invalidates");
        let _ = surface.commands();

        surface.set_viewport(Viewport {
            width: 1024.0,
            height: 768.0,
        });
        assert!(
            !surface.is_dirty(),
            "setting the same viewport must not invalidate"
        );

        surface.invalidate();
        assert!(surface.is_dirty(), "explicit invalidate works");
    }

    #[test]
    fn empty_container_emits_nothing() {
        let mut surface = Surface::new(
            Node::Container {
                id: "empty".into(),
                props: ContainerProps::default(),
                children: vec![],
            },
            Viewport {
                width: 100.0,
                height: 100.0,
            },
        );
        assert!(surface.commands().is_empty());
    }

    #[test]
    fn row_layout_distributes_along_x() {
        let tree = Node::Container {
            id: "row".into(),
            props: ContainerProps {
                direction: Direction::Row,
                width: Sizing::Grow,
                height: Sizing::Grow,
                background: Some(rgb(0, 0, 0)),
                ..Default::default()
            },
            children: vec![
                Node::Container {
                    id: "a".into(),
                    props: ContainerProps {
                        width: Sizing::Fixed(50.0),
                        height: Sizing::Grow,
                        background: Some(rgb(255, 0, 0)),
                        ..Default::default()
                    },
                    children: vec![],
                },
                Node::Container {
                    id: "b".into(),
                    props: ContainerProps {
                        width: Sizing::Fixed(70.0),
                        height: Sizing::Grow,
                        background: Some(rgb(0, 0, 255)),
                        ..Default::default()
                    },
                    children: vec![],
                },
            ],
        };
        let mut surface = Surface::new(
            tree,
            Viewport {
                width: 200.0,
                height: 100.0,
            },
        );
        let commands = surface.commands().to_vec();
        let rects: Vec<_> = commands
            .iter()
            .filter_map(|c| match c {
                RenderCommand::Rectangle { bounds, .. } => Some(*bounds),
                _ => None,
            })
            .collect();
        assert_eq!(rects.len(), 3);
        assert_eq!(rects[1].x, 0.0);
        assert_eq!(rects[1].width, 50.0);
        assert_eq!(rects[2].x, 50.0);
        assert_eq!(rects[2].width, 70.0);
    }
}
