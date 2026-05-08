//! Layout pass — typed `UiTree` of `Node`s lowered onto a Taffy
//! `TaffyTree<NodeContext>`, then walked to emit a backend-neutral
//! `Vec<RenderCommand>`.
//!
//! ## Pivot 2026-05-04 — Taffy, not Clay
//!
//! The Phase 1 plan called for a vendored fork of `clay-layout`. We
//! flipped to **Taffy** instead — pure Rust, already a workspace dep
//! (powers `prism-builder`'s editor layout), real CSS Grid + Flex +
//! Block, MIT-licensed. See the pivot note at the top of
//! `docs/dev/clay-migration-plan.md` for the full rationale. The
//! typed `Node` tree, `Surface` retained-mode contract, and
//! `RenderCommand` shape are unchanged — only the engine behind
//! [`compute`] moves.
//!
//! ## Retained-mode contract
//!
//! Layout is **not** recomputed every frame. Per the plan's runtime
//! model, the layout cache is invalidated only when something changes:
//! tree mutation, viewport resize, scroll, an animation tick, or an
//! explicit `invalidate()`. Backends pull the cached `&[RenderCommand]`
//! slice each frame and only pay the layout cost on a dirty cycle.
//! See [`Surface`].
//!
//! ## Luau
//!
//! `Node` / `ContainerProps` / `TextProps` are deliberately
//! `serde`-friendly value types (no lifetimes, no trait objects) so
//! `prism-luau-derive` can wrap them as `mlua::UserData` for Luau
//! authoring of UI trees. See `feedback_clay_luau_authoring` in
//! auto-memory for the full requirement.

use serde::{Deserialize, Serialize};
use taffy::prelude::*;
use taffy::{
    AvailableSpace, Dimension as TaffyDimension, FlexDirection as TaffyFlexDirection, Layout, Size,
    Style, TaffyTree,
};

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

/// One element in the UI tree. Mirrors a CSS-style element vocabulary
/// — containers (flex / block layout parents), text leaves, and
/// explicit spacers. Images and scroll containers land in the same
/// enum once we grow the matching primitives.
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

/// Sizing policy for a single axis.
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
/// **Engine.** Drives a `taffy::TaffyTree<NodeContext>` end-to-end:
/// builds the Taffy tree, runs `compute_layout` against the viewport
/// (using a measure callback for text leaves), then walks the
/// resolved layout to emit `RenderCommand`s. The walk preserves
/// document order, which is the order Prism's renderers paint in.
pub fn compute(tree: &Node, viewport: Viewport) -> Vec<RenderCommand> {
    let mut taffy: TaffyTree<NodeContext> = TaffyTree::new();
    // Root has no parent flex container — pass `None` so its own
    // sizing is honoured directly.
    let root = build_taffy_subtree(&mut taffy, tree, None);
    let available = Size {
        width: AvailableSpace::Definite(viewport.width),
        height: AvailableSpace::Definite(viewport.height),
    };
    if taffy
        .compute_layout_with_measure(root, available, measure_text)
        .is_err()
    {
        return Vec::new();
    }
    let mut out = Vec::new();
    emit_commands(&taffy, root, 0.0, 0.0, &mut out);
    out
}

/// Per-Taffy-node context — what `measure_text` and `emit_commands`
/// need to do their jobs without re-walking the source tree.
#[derive(Debug, Clone)]
enum NodeContext {
    Container {
        background: Option<Color>,
        radius: CornerRadius,
    },
    Text {
        content: String,
        props: TextProps,
    },
    Spacer,
}

fn build_taffy_subtree(
    taffy: &mut TaffyTree<NodeContext>,
    node: &Node,
    parent_direction: Option<Direction>,
) -> NodeId {
    match node {
        Node::Container {
            props, children, ..
        } => {
            let style = container_style(props, parent_direction);
            let own_direction = Some(props.direction);
            let child_ids: Vec<NodeId> = children
                .iter()
                .map(|c| build_taffy_subtree(taffy, c, own_direction))
                .collect();
            let ctx = NodeContext::Container {
                background: props.background,
                radius: props.radius,
            };
            taffy
                .new_with_children(style, &child_ids)
                .and_then(|id| {
                    taffy.set_node_context(id, Some(ctx))?;
                    Ok(id)
                })
                .expect("taffy: container insert")
        }
        Node::Text { content, props, .. } => {
            let style = Style::default();
            let ctx = NodeContext::Text {
                content: content.clone(),
                props: *props,
            };
            taffy
                .new_leaf_with_context(style, ctx)
                .expect("taffy: text leaf insert")
        }
        Node::Spacer { width, height, .. } => {
            let style = Style {
                size: Size {
                    width: TaffyDimension::Length(*width),
                    height: TaffyDimension::Length(*height),
                },
                ..Default::default()
            };
            taffy
                .new_leaf_with_context(style, NodeContext::Spacer)
                .expect("taffy: spacer insert")
        }
    }
}

fn container_style(props: &ContainerProps, parent_direction: Option<Direction>) -> Style {
    let direction = match props.direction {
        Direction::Row => TaffyFlexDirection::Row,
        Direction::Column => TaffyFlexDirection::Column,
    };
    // `Grow` on the parent's main axis lowers to `flex_grow: 1` —
    // Taffy's first-class way to consume free space along the parent
    // direction. On the cross axis (or for the root), `Grow` lowers
    // to `Percent(1.0)` via `sizing_to_taffy`, which fills the
    // available cross-axis extent.
    let flex_grow = match parent_direction {
        Some(Direction::Row) if matches!(props.width, Sizing::Grow) => 1.0,
        Some(Direction::Column) if matches!(props.height, Sizing::Grow) => 1.0,
        _ => 0.0,
    };
    Style {
        display: Display::Flex,
        flex_direction: direction,
        size: Size {
            width: sizing_to_taffy(props.width),
            height: sizing_to_taffy(props.height),
        },
        flex_grow,
        padding: taffy::Rect {
            left: LengthPercentage::Length(props.padding.left),
            right: LengthPercentage::Length(props.padding.right),
            top: LengthPercentage::Length(props.padding.top),
            bottom: LengthPercentage::Length(props.padding.bottom),
        },
        gap: Size {
            width: LengthPercentage::Length(props.gap),
            height: LengthPercentage::Length(props.gap),
        },
        ..Default::default()
    }
}

fn sizing_to_taffy(s: Sizing) -> TaffyDimension {
    match s {
        Sizing::Fit => TaffyDimension::Auto,
        Sizing::Grow => TaffyDimension::Percent(1.0),
        Sizing::Fixed(v) => TaffyDimension::Length(v),
    }
}

/// Crude text measurement — width estimated as `chars * font_size *
/// 0.55`, height as `font_size * 1.2`. Replaced by a real
/// `cosmic-text` shaping pass once the text Phase lands. Same
/// heuristic the Phase-1 hand-rolled engine used, lifted here so
/// snapshot tests remain stable across the pivot.
fn measure_text(
    known_dimensions: Size<Option<f32>>,
    _available: Size<AvailableSpace>,
    _node_id: NodeId,
    node_context: Option<&mut NodeContext>,
    _style: &Style,
) -> Size<f32> {
    if let (Some(w), Some(h)) = (known_dimensions.width, known_dimensions.height) {
        return Size {
            width: w,
            height: h,
        };
    }
    match node_context {
        Some(NodeContext::Text { content, props }) => {
            let width = known_dimensions
                .width
                .unwrap_or_else(|| content.chars().count() as f32 * props.font_size * 0.55);
            let height = known_dimensions.height.unwrap_or(props.font_size * 1.2);
            Size { width, height }
        }
        _ => Size::ZERO,
    }
}

fn emit_commands(
    taffy: &TaffyTree<NodeContext>,
    id: NodeId,
    parent_x: f32,
    parent_y: f32,
    out: &mut Vec<RenderCommand>,
) {
    let layout: &Layout = taffy.layout(id).expect("taffy: layout missing");
    let bounds = Rect {
        x: parent_x + layout.location.x,
        y: parent_y + layout.location.y,
        width: layout.size.width,
        height: layout.size.height,
    };
    match taffy.get_node_context(id) {
        Some(NodeContext::Container { background, radius }) => {
            if let Some(bg) = background {
                out.push(RenderCommand::Rectangle {
                    bounds,
                    color: *bg,
                    radius: *radius,
                });
            }
            for child in taffy.children(id).unwrap_or_default() {
                emit_commands(taffy, child, bounds.x, bounds.y, out);
            }
        }
        Some(NodeContext::Text { content, props }) => {
            out.push(RenderCommand::Text {
                bounds,
                content: content.clone(),
                color: props.color,
                font_size: props.font_size,
            });
        }
        Some(NodeContext::Spacer) | None => {}
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
