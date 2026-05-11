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
    /// External or VFS-resolved image. Same `Sizing` policy as a
    /// container so `grow` / `fit` / fixed-pixel images flow through
    /// Taffy identically. The `source` is whatever the host's renderer
    /// understands (URL, `/asset/<hash>`, file path) — the layout pass
    /// is source-agnostic and just round-trips the string.
    Image {
        #[serde(default)]
        id: String,
        source: String,
        #[serde(default)]
        width: Sizing,
        #[serde(default)]
        height: Sizing,
        #[serde(default)]
        radius: CornerRadius,
        /// Optional colour tint applied to the image. When `Some`,
        /// the renderer treats `source` as a mask and paints `tint`
        /// through it — the canonical icon-tinting pattern (palette
        /// foreground, transparency variants). `None` paints the
        /// image verbatim. Native backends pre-multiply the tint with
        /// the mask alpha; the HTML backend lowers to a `mask-image`
        /// + `background-color` pair.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tint: Option<Color>,
        /// SSR hint — `alt` text usually goes in `attrs`, ARIA in
        /// `aria_label`. Native renderers ignore this field.
        #[serde(default, skip_serializing_if = "Semantic::is_empty")]
        semantic: Semantic,
    },
    /// Editable single-line text input. Composes from existing render
    /// commands at emit time (background `Rectangle` + `Border` +
    /// `Text` for value-or-placeholder), so backends don't grow a new
    /// `RenderCommand` variant — the same paint pipeline that handles
    /// containers and labels handles inputs.
    ///
    /// Editing / focus / IME are deferred: the runtime currently treats
    /// the value as authoritative and re-renders on `Surface::set_tree`.
    /// The host wires keyboard and pointer events through
    /// `event::EventHandler` (the same path button signals already
    /// take) and pushes a new tree on each character.
    TextInput {
        #[serde(default)]
        id: String,
        #[serde(default)]
        value: String,
        #[serde(default)]
        placeholder: String,
        #[serde(default)]
        props: TextProps,
        #[serde(default)]
        width: Sizing,
        #[serde(default)]
        height: Sizing,
        #[serde(default)]
        radius: CornerRadius,
        #[serde(default, skip_serializing_if = "Semantic::is_empty")]
        semantic: Semantic,
    },
}

impl Node {
    pub fn id(&self) -> &str {
        match self {
            Node::Container { id, .. }
            | Node::Text { id, .. }
            | Node::Spacer { id, .. }
            | Node::Image { id, .. }
            | Node::TextInput { id, .. } => id,
        }
    }

    /// Variant tag as a stable kebab-case string. Single source of
    /// truth for "what is this node?" — Luau bindings, debugging, and
    /// any future hint dispatch all route through here so adding a
    /// variant is one place to update, not three.
    pub fn kind(&self) -> &'static str {
        match self {
            Node::Container { .. } => "container",
            Node::Text { .. } => "text",
            Node::Spacer { .. } => "spacer",
            Node::Image { .. } => "image",
            Node::TextInput { .. } => "text-input",
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
    /// Fraction of the parent's main axis (`0.0..=1.0`). Used by
    /// the dock-workspace block to size split children by ratio
    /// without recomputing pixel rectangles host-side.
    Percent(f32),
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
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
    /// Semantic-HTML hint carried into the SSR lowering. Backends
    /// that produce semantic markup (`backends::semantic_html`) read
    /// this; native rendering ignores it. Defaults to "no hint", which
    /// the walker resolves to a plain `<div>`.
    #[serde(default, skip_serializing_if = "Semantic::is_empty")]
    pub semantic: Semantic,
    /// Declarative hover-state overrides. Sparse — only the fields
    /// that *change* on hover are listed. The runtime swaps these in
    /// at command-emit time when the container's `id` matches
    /// [`Surface::hovered_id`]. SSR backends ignore this field —
    /// hover is a native-render-only concern.
    ///
    /// Authoring pattern: a block's `lower_ui` declares the hover
    /// shape alongside the resting shape, in the same impl. No
    /// imperative state machine, no shadow render path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hover: Option<HoverOverrides>,
}

/// Sparse override bundle applied when a node is hovered. Each field
/// is independently optional — most blocks override only `background`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct HoverOverrides {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background: Option<Color>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub radius: Option<CornerRadius>,
}

impl HoverOverrides {
    pub fn is_empty(&self) -> bool {
        self.background.is_none() && self.radius.is_none()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextProps {
    pub font_size: f32,
    pub color: Color,
    /// Semantic-HTML hint — see [`ContainerProps::semantic`]. The
    /// walker uses `tag` to pick `<h1>`/`<p>`/`<span>` etc.; when
    /// empty, font_size buckets to a sensible default.
    #[serde(default, skip_serializing_if = "Semantic::is_empty")]
    pub semantic: Semantic,
}

/// Per-node semantic-HTML hint. Carries the information SSR needs
/// to emit meaningful markup (tag override, CSS class, ARIA, free-form
/// attributes) that the layout pass and native renderers don't care
/// about. Defaults to "no hint" so unset fields skip serialisation
/// and existing JSON round-trips unchanged.
///
/// Each field is independently optional — a block can declare just a
/// tag (`<section>`), just a class (still a `<div>` but with styling),
/// or both, without ceremony.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Semantic {
    /// HTML tag override. `None` falls back to the variant default
    /// (container → div, text → span/p/h1, image → img, spacer → div).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    /// CSS class — multiple classes space-separated, like in HTML.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub class: Option<String>,
    /// `aria-label` value when one is needed for accessibility.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aria_label: Option<String>,
    /// `role` attribute.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    /// Free-form attributes (e.g. `alt` on images, `href` on links,
    /// `data-*`). Emitted verbatim after the structural attributes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attrs: Vec<(String, String)>,
}

impl Semantic {
    pub fn is_empty(&self) -> bool {
        self.tag.is_none()
            && self.class.is_none()
            && self.aria_label.is_none()
            && self.role.is_none()
            && self.attrs.is_empty()
    }

    /// Builder shorthand: a hint that overrides the tag only.
    pub fn tag(t: impl Into<String>) -> Self {
        Self {
            tag: Some(t.into()),
            ..Default::default()
        }
    }

    /// Pre-shaped `<button type="button">` constructor — every shell
    /// button-shaped primitive (IconButton, NavButton, future Tab pill,
    /// MenuBarRow item) starts here and folds in conditional ARIA via
    /// the `*_if` / `*_opt` helpers below. Avoids two lines of
    /// boilerplate per primitive.
    pub fn button() -> Self {
        Self::tag("button").with_attr("type", "button")
    }

    pub fn with_class(mut self, c: impl Into<String>) -> Self {
        self.class = Some(c.into());
        self
    }

    pub fn with_attr(mut self, k: impl Into<String>, v: impl Into<String>) -> Self {
        self.attrs.push((k.into(), v.into()));
        self
    }

    /// Conditional attribute — folds the `if cond { semantic =
    /// semantic.with_attr(k, v) }` boilerplate into a single chainable
    /// call. The attribute is emitted iff `cond` is true.
    pub fn with_attr_if(self, cond: bool, k: impl Into<String>, v: impl Into<String>) -> Self {
        if cond {
            self.with_attr(k, v)
        } else {
            self
        }
    }

    pub fn with_aria_label(mut self, label: impl Into<String>) -> Self {
        self.aria_label = Some(label.into());
        self
    }

    /// Conditional `aria-label`. Common in chrome lowering where the
    /// label comes from an `Option<&str>` prop (tooltip text, help id).
    /// `None` leaves the field unset.
    pub fn with_aria_label_opt(self, label: Option<impl Into<String>>) -> Self {
        match label {
            Some(l) => self.with_aria_label(l),
            None => self,
        }
    }

    pub fn with_role(mut self, role: impl Into<String>) -> Self {
        self.role = Some(role.into());
        self
    }
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
            semantic: Semantic::default(),
        }
    }
}

/// Hit-test record produced alongside the render commands: a
/// container's resolved viewport-space rectangle plus the semantic
/// attribute bag the host needs to route a pointer event.
///
/// **Why containers only.** Hit-testing today is interaction-driven —
/// the host needs to know "what *interactive thing* is under the
/// pointer." Container nodes carry the `Semantic::attrs` bag where
/// host-side routing keys (`data-role`, `data-target-id`, …) live;
/// text / image / spacer leaves never carry handlers, so they don't
/// need to surface here. Future need for leaf-level hit-testing
/// would extend [`Self::attrs`] to include leaf semantics without
/// touching the API shape.
///
/// **Ordering.** Hit rects are emitted in **paint order** — same
/// order as the matching `RenderCommand`s. Callers walking the
/// vector in reverse find the topmost container at a point.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HitRect {
    /// Container's stable id (the `id` field of `Node::Container`).
    /// Empty ids skip the cache — anonymous wrapper containers don't
    /// participate in hit-testing, so the host never has to filter
    /// them out.
    pub id: String,
    /// Resolved viewport-space rectangle (already includes overlay
    /// anchor offsets when this rect comes from an overlay subtree).
    pub bounds: Rect,
    /// Copy of the container's semantic `attrs` — the host reads
    /// `data-role` / `data-target-id` / `data-key` / … to decide
    /// what the pointer event means.
    pub attrs: Vec<(String, String)>,
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
    compute_with_hover(tree, viewport, None)
}

/// Same as [`compute`] but with a hovered-node id. When a container
/// declares `props.hover` and its id matches `hovered_id`, the
/// overrides are folded in at the moment its `NodeContext` is built —
/// layout itself doesn't shift (hover affects paint, not box model),
/// so the only commands that change are the rectangle's colour /
/// radius. Pass `None` for the resting state.
pub fn compute_with_hover(
    tree: &Node,
    viewport: Viewport,
    hovered_id: Option<&str>,
) -> Vec<RenderCommand> {
    compute_full(tree, &[], viewport, hovered_id)
}

/// Hot-path entry point: compute the main tree, then each overlay in
/// declaration order, concatenating the command streams. Overlays
/// paint **after** the main tree, achieving z-order naturally — the
/// last overlay declared sits on top.
///
/// Each overlay lays out independently against the viewport (so
/// `Sizing::Fit` works the same as a top-level tree), then its commands
/// are translated by the resolved anchor offset. The same hover
/// pipeline applies to overlay subtrees, so e.g. a button inside the
/// command-palette overlay can declare a `hover` swap.
pub fn compute_full(
    tree: &Node,
    overlays: &[Overlay],
    viewport: Viewport,
    hovered_id: Option<&str>,
) -> Vec<RenderCommand> {
    let mut out = Vec::new();
    compute_subtree_into(tree, viewport, hovered_id, 0.0, 0.0, &mut out);
    for overlay in overlays {
        compute_overlay_into(overlay, viewport, hovered_id, &mut out);
    }
    out
}

/// Sister to [`compute_full`] that also emits a `Vec<HitRect>` for
/// hit-testing. The two outputs are produced in lockstep through one
/// Taffy build per (main tree + each overlay), so callers paying the
/// render cost on a dirty cycle pay nothing extra for hit-testing.
pub fn compute_full_with_hits(
    tree: &Node,
    overlays: &[Overlay],
    viewport: Viewport,
    hovered_id: Option<&str>,
) -> (Vec<RenderCommand>, Vec<HitRect>) {
    let mut commands = Vec::new();
    let mut hits = Vec::new();
    compute_subtree_into_with_hits(
        tree,
        viewport,
        hovered_id,
        0.0,
        0.0,
        &mut commands,
        &mut hits,
    );
    for overlay in overlays {
        compute_overlay_into_with_hits(overlay, viewport, hovered_id, &mut commands, &mut hits);
    }
    (commands, hits)
}

fn compute_subtree_into(
    tree: &Node,
    viewport: Viewport,
    hovered_id: Option<&str>,
    origin_x: f32,
    origin_y: f32,
    out: &mut Vec<RenderCommand>,
) -> Option<Size<f32>> {
    let mut taffy: TaffyTree<NodeContext> = TaffyTree::new();
    let root = build_taffy_subtree(&mut taffy, tree, None, hovered_id);
    let available = Size {
        width: AvailableSpace::Definite(viewport.width),
        height: AvailableSpace::Definite(viewport.height),
    };
    if taffy
        .compute_layout_with_measure(root, available, measure_text)
        .is_err()
    {
        return None;
    }
    let size = taffy.layout(root).map(|l| l.size).ok()?;
    emit_commands(&taffy, root, origin_x, origin_y, out);
    Some(size)
}

fn compute_subtree_into_with_hits(
    tree: &Node,
    viewport: Viewport,
    hovered_id: Option<&str>,
    origin_x: f32,
    origin_y: f32,
    commands: &mut Vec<RenderCommand>,
    hits: &mut Vec<HitRect>,
) -> Option<Size<f32>> {
    let mut taffy: TaffyTree<NodeContext> = TaffyTree::new();
    let root = build_taffy_subtree(&mut taffy, tree, None, hovered_id);
    let available = Size {
        width: AvailableSpace::Definite(viewport.width),
        height: AvailableSpace::Definite(viewport.height),
    };
    if taffy
        .compute_layout_with_measure(root, available, measure_text)
        .is_err()
    {
        return None;
    }
    let size = taffy.layout(root).map(|l| l.size).ok()?;
    emit_commands(&taffy, root, origin_x, origin_y, commands);
    walk_for_hits(&taffy, root, tree, origin_x, origin_y, hits);
    Some(size)
}

fn compute_overlay_into_with_hits(
    overlay: &Overlay,
    viewport: Viewport,
    hovered_id: Option<&str>,
    commands: &mut Vec<RenderCommand>,
    hits: &mut Vec<HitRect>,
) {
    let mut probe: TaffyTree<NodeContext> = TaffyTree::new();
    let probe_root = build_taffy_subtree(&mut probe, &overlay.node, None, hovered_id);
    let available = Size {
        width: AvailableSpace::Definite(viewport.width),
        height: AvailableSpace::Definite(viewport.height),
    };
    if probe
        .compute_layout_with_measure(probe_root, available, measure_text)
        .is_err()
    {
        return;
    }
    let size = match probe.layout(probe_root) {
        Ok(l) => l.size,
        Err(_) => return,
    };
    let (ox, oy) = overlay.anchor.resolve(viewport, size.width, size.height);
    emit_commands(&probe, probe_root, ox, oy, commands);
    walk_for_hits(&probe, probe_root, &overlay.node, ox, oy, hits);
}

/// Walk the Taffy tree alongside the source `Node` tree in lockstep
/// (build order = paint order = source order), accumulating one
/// [`HitRect`] per non-empty-id container. Leaves don't contribute —
/// see [`HitRect`] for the rationale. The two trees stay in lockstep
/// because `build_taffy_subtree` preserves child order one-for-one.
fn walk_for_hits(
    taffy: &TaffyTree<NodeContext>,
    taffy_id: NodeId,
    source: &Node,
    parent_x: f32,
    parent_y: f32,
    out: &mut Vec<HitRect>,
) {
    let layout: &Layout = match taffy.layout(taffy_id) {
        Ok(l) => l,
        Err(_) => return,
    };
    let bounds = Rect {
        x: parent_x + layout.location.x,
        y: parent_y + layout.location.y,
        width: layout.size.width,
        height: layout.size.height,
    };
    if let Node::Container {
        id,
        props,
        children,
    } = source
    {
        if !id.is_empty() {
            out.push(HitRect {
                id: id.clone(),
                bounds,
                attrs: props.semantic.attrs.clone(),
            });
        }
        // Iterate Taffy children and source children in lockstep.
        let taffy_children = taffy.children(taffy_id).unwrap_or_default();
        for (taffy_child, source_child) in taffy_children.iter().zip(children.iter()) {
            walk_for_hits(taffy, *taffy_child, source_child, bounds.x, bounds.y, out);
        }
    }
}

fn compute_overlay_into(
    overlay: &Overlay,
    viewport: Viewport,
    hovered_id: Option<&str>,
    out: &mut Vec<RenderCommand>,
) {
    // Two-pass: lay out at (0,0) to learn the overlay's resolved size,
    // then re-emit with the anchor-resolved origin. Cost is one extra
    // Taffy build per overlay per dirty cycle; overlays are by nature
    // small subtrees so this is negligible. Keeping the second pass
    // separate avoids threading "post-resolve translate" through
    // `emit_commands`, which would couple the main-tree path to the
    // overlay path.
    let mut probe: TaffyTree<NodeContext> = TaffyTree::new();
    let probe_root = build_taffy_subtree(&mut probe, &overlay.node, None, hovered_id);
    let available = Size {
        width: AvailableSpace::Definite(viewport.width),
        height: AvailableSpace::Definite(viewport.height),
    };
    if probe
        .compute_layout_with_measure(probe_root, available, measure_text)
        .is_err()
    {
        return;
    }
    let size = match probe.layout(probe_root) {
        Ok(l) => l.size,
        Err(_) => return,
    };
    let (ox, oy) = overlay.anchor.resolve(viewport, size.width, size.height);
    emit_commands(&probe, probe_root, ox, oy, out);
}

/// A sub-tree painted in its own viewport-anchored coordinate space,
/// after the main tree. Overlays cover the chrome use cases the
/// runtime can't express in flow: Toasts (bottom-right corner),
/// the command palette (centred), help tooltips (anchored to a point),
/// modal dialogs (centred), context menus (point-anchored), and so on.
///
/// Smart-pattern shape: an overlay is **just a `Node` plus an anchor**.
/// Layout, paint, hover, SSR semantics — all reuse the existing
/// vocabulary. The Block authoring contract is unchanged: a primitive's
/// `lower_ui` produces a `Node`; the *host* decides whether to mount
/// it inside the main tree or as an overlay. This keeps Block impls
/// reusable across contexts (a Toast card can equally well render
/// inline in a notification list panel) and the runtime free of
/// per-primitive z-order knowledge.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Overlay {
    /// Stable identifier — `Surface::remove_overlay` matches on this,
    /// hover hit-testing scopes by it, and the host can dedupe.
    pub id: String,
    pub anchor: OverlayAnchor,
    pub node: Node,
}

impl Overlay {
    pub fn new(id: impl Into<String>, anchor: OverlayAnchor, node: Node) -> Self {
        Self {
            id: id.into(),
            anchor,
            node,
        }
    }
}

/// Where on the viewport an overlay's top-left corner lands. Sparse
/// vocabulary: three variants cover the chrome cases (Toast, command
/// palette, help tooltip / context menu / dialog). Adding more is
/// possible but each new case must justify itself — anchors that
/// require a layout-of-the-main-tree query (e.g. "anchored to node
/// `nav-button-3`") are deliberately deferred.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OverlayAnchor {
    /// Pin to a viewport corner with optional inset in logical pixels.
    /// Toast → `BottomRight` with `(16, 16)` inset.
    Corner { corner: Corner, inset: Inset },
    /// Absolute viewport coordinates (top-left of the overlay box).
    /// Help tooltip → `Point { x: pointer_x + 12, y: pointer_y + 12 }`.
    Point { x: f32, y: f32 },
    /// Centred horizontally; vertical position controlled by `offset_y`
    /// (0 = vertical-centred, positive = below centre, negative = above).
    /// Command palette → `Center { offset_y: -200.0 }`.
    Center { offset_y: f32 },
}

impl OverlayAnchor {
    fn resolve(&self, viewport: Viewport, w: f32, h: f32) -> (f32, f32) {
        match *self {
            OverlayAnchor::Corner { corner, inset } => match corner {
                Corner::TopLeft => (inset.x, inset.y),
                Corner::TopRight => (viewport.width - w - inset.x, inset.y),
                Corner::BottomLeft => (inset.x, viewport.height - h - inset.y),
                Corner::BottomRight => {
                    (viewport.width - w - inset.x, viewport.height - h - inset.y)
                }
            },
            OverlayAnchor::Point { x, y } => (x, y),
            OverlayAnchor::Center { offset_y } => (
                (viewport.width - w) * 0.5,
                (viewport.height - h) * 0.5 + offset_y,
            ),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Corner {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Inset {
    pub x: f32,
    pub y: f32,
}

impl Inset {
    pub fn all(v: f32) -> Self {
        Self { x: v, y: v }
    }
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
    Image {
        source: String,
        radius: CornerRadius,
        tint: Option<Color>,
    },
    /// `value` is whatever the input should *paint*, computed at build
    /// time as `value` if non-empty else `placeholder`. The
    /// `is_placeholder` flag lets the painter dim the colour without
    /// an extra walk of the source tree.
    TextInput {
        text: String,
        is_placeholder: bool,
        props: TextProps,
        radius: CornerRadius,
    },
}

fn build_taffy_subtree(
    taffy: &mut TaffyTree<NodeContext>,
    node: &Node,
    parent_direction: Option<Direction>,
    hovered_id: Option<&str>,
) -> NodeId {
    match node {
        Node::Container {
            id,
            props,
            children,
        } => {
            let style = container_style(props, parent_direction);
            let own_direction = Some(props.direction);
            let child_ids: Vec<NodeId> = children
                .iter()
                .map(|c| build_taffy_subtree(taffy, c, own_direction, hovered_id))
                .collect();
            // Fold hover overrides into the resting paint state when
            // this container is the one being hovered. Layout-affecting
            // hover changes would need to live on `style` instead;
            // intentionally not supported — hover is paint-only.
            let (background, radius) = match (props.hover.as_ref(), hovered_id) {
                (Some(h), Some(hov)) if hov == id => (
                    h.background.or(props.background),
                    h.radius.unwrap_or(props.radius),
                ),
                _ => (props.background, props.radius),
            };
            let ctx = NodeContext::Container { background, radius };
            taffy
                .new_with_children(style, &child_ids)
                .and_then(|id| {
                    taffy.set_node_context(id, Some(ctx))?;
                    Ok(id)
                })
                .expect("taffy: container insert")
        }
        Node::Text { content, props, .. } => {
            // `flex_shrink: 0.0` so Taffy never squeezes a text leaf
            // below its natural width. The paint pass calls
            // `cosmic-text::Buffer::set_size(width, …)` with the
            // Taffy-computed width, and any squeeze cascades into
            // mid-word wrapping ("Components" → "Component / s",
            // "Window" → "Windo / w"). Pinning shrink to 0 here means
            // a text label's parent can shrink the *spacer* siblings
            // but never the text itself.
            let style = Style {
                flex_shrink: 0.0,
                ..Style::default()
            };
            let ctx = NodeContext::Text {
                content: content.clone(),
                props: props.clone(),
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
        Node::TextInput {
            value,
            placeholder,
            props,
            width,
            height,
            radius,
            ..
        } => {
            // Same Grow→flex_grow rule the container/image arms use — a
            // `width: grow` input fills the parent's main axis exactly
            // like a sized container would. `Fit` along the main axis
            // pins `flex_shrink: 0.0` for the same anti-wrap reason
            // text leaves do (the input's placeholder/value would
            // otherwise cosmic-text-wrap under tight parents).
            let flex_grow = match parent_direction {
                Some(Direction::Row) if matches!(width, Sizing::Grow) => 1.0,
                Some(Direction::Column) if matches!(height, Sizing::Grow) => 1.0,
                _ => 0.0,
            };
            let flex_shrink = match parent_direction {
                Some(Direction::Row) if matches!(width, Sizing::Fit) => 0.0,
                Some(Direction::Column) if matches!(height, Sizing::Fit) => 0.0,
                _ => 1.0,
            };
            let style = Style {
                size: Size {
                    width: sizing_to_taffy(*width),
                    height: sizing_to_taffy(*height),
                },
                flex_grow,
                flex_shrink,
                ..Default::default()
            };
            let (text, is_placeholder) = if value.is_empty() {
                (placeholder.clone(), true)
            } else {
                (value.clone(), false)
            };
            let ctx = NodeContext::TextInput {
                text,
                is_placeholder,
                props: props.clone(),
                radius: *radius,
            };
            taffy
                .new_leaf_with_context(style, ctx)
                .expect("taffy: text-input leaf insert")
        }
        Node::Image {
            source,
            width,
            height,
            radius,
            tint,
            ..
        } => {
            // Same sizing vocabulary as containers — `Grow` along the
            // parent main axis becomes `flex_grow: 1`, otherwise lowers
            // through `sizing_to_taffy`. `Fit` images pin
            // `flex_shrink: 0.0` so the resolved bitmap rect keeps its
            // intrinsic size when the parent runs out of room (icon
            // buttons would otherwise vanish under crowded toolbars).
            let flex_grow = match parent_direction {
                Some(Direction::Row) if matches!(width, Sizing::Grow) => 1.0,
                Some(Direction::Column) if matches!(height, Sizing::Grow) => 1.0,
                _ => 0.0,
            };
            let flex_shrink = match parent_direction {
                Some(Direction::Row) if matches!(width, Sizing::Fit) => 0.0,
                Some(Direction::Column) if matches!(height, Sizing::Fit) => 0.0,
                _ => 1.0,
            };
            let style = Style {
                size: Size {
                    width: sizing_to_taffy(*width),
                    height: sizing_to_taffy(*height),
                },
                flex_grow,
                flex_shrink,
                ..Default::default()
            };
            let ctx = NodeContext::Image {
                source: source.clone(),
                radius: *radius,
                tint: *tint,
            };
            taffy
                .new_leaf_with_context(style, ctx)
                .expect("taffy: image leaf insert")
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
    // `Sizing::Fit` is the authoring vocabulary's "tight to content"
    // — pills, tabs, icon buttons, status segments, menu labels. In
    // CSS flex, items default to `flex-shrink: 1`, which lets Taffy
    // squeeze a Fit child below its natural width when the parent is
    // tighter than the sum of children. For text-bearing leaves that
    // squeeze cascades into cosmic-text's `set_size(width, …)` and
    // visibly wraps the label mid-word ("Window" → "Windo / w",
    // "Components" → "Component / s"). Setting `flex_shrink: 0` on
    // Fit children along the main axis preserves their natural
    // width; overflow goes to the spacer / scroll surface rather
    // than the label. `Grow`, `Fixed`, and `Percent` children keep
    // Taffy's default shrink behaviour — they're explicit about
    // their sizing strategy.
    let flex_shrink = match parent_direction {
        Some(Direction::Row) if matches!(props.width, Sizing::Fit) => 0.0,
        Some(Direction::Column) if matches!(props.height, Sizing::Fit) => 0.0,
        _ => 1.0,
    };
    Style {
        display: Display::Flex,
        flex_direction: direction,
        size: Size {
            width: sizing_to_taffy(props.width),
            height: sizing_to_taffy(props.height),
        },
        flex_grow,
        flex_shrink,
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
        Sizing::Percent(p) => TaffyDimension::Percent(p.clamp(0.0, 1.0)),
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
    match node_context {
        Some(NodeContext::Text { content, props }) => {
            // Always report the *natural* width — never shrink below it
            // just because Taffy passed a constrained `known_dimensions.width`.
            // Returning the smaller of the two used to let cosmic-text wrap
            // labels mid-word inside narrow flex containers ("Window" →
            // "Windo / w"); by always reporting the full natural width here
            // and pairing it with `flex_shrink: 0` on text leaves, the
            // text keeps its natural width and the parent flexbox handles
            // any overflow.
            //
            // Height is still honoured when Taffy supplies one — the
            // line-height pass is the only thing that needs that override.
            let natural_w = content.chars().count() as f32 * props.font_size * 0.55;
            let width = match known_dimensions.width {
                Some(w) if w >= natural_w => w,
                _ => natural_w,
            };
            let height = known_dimensions.height.unwrap_or(props.font_size * 1.2);
            Size { width, height }
        }
        Some(NodeContext::TextInput { text, props, .. }) => {
            // Same anti-shrink rule as Text leaves, plus the 12px / 8px
            // input padding so empty inputs still have clickable extent.
            let glyph_count = text.chars().count().max(1);
            let natural_w = glyph_count as f32 * props.font_size * 0.55 + 12.0;
            let width = match known_dimensions.width {
                Some(w) if w >= natural_w => w,
                _ => natural_w,
            };
            let height = known_dimensions
                .height
                .unwrap_or(props.font_size * 1.2 + 8.0);
            Size { width, height }
        }
        _ => {
            if let (Some(w), Some(h)) = (known_dimensions.width, known_dimensions.height) {
                Size {
                    width: w,
                    height: h,
                }
            } else {
                Size::ZERO
            }
        }
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
        Some(NodeContext::Image {
            source,
            radius,
            tint,
        }) => {
            // The source string is round-tripped verbatim — host code
            // resolves it to a concrete asset (URL, `/asset/<hash>`,
            // file path). Radius flows through to the renderer the
            // same way a `Rectangle` carries its corner radius. Tint,
            // when present, instructs the renderer to mask-paint the
            // colour through the image.
            out.push(RenderCommand::Image {
                bounds,
                source: source.clone(),
                radius: *radius,
                tint: *tint,
            });
        }
        // Composed leaf — paint a background Rectangle, a 1px Border,
        // and a Text command for value-or-placeholder. Existing
        // backends consume all three primitives unchanged; no new
        // RenderCommand variant.
        Some(NodeContext::TextInput {
            text,
            is_placeholder,
            props,
            radius,
        }) => {
            out.push(RenderCommand::Rectangle {
                bounds,
                color: Color {
                    r: 255,
                    g: 255,
                    b: 255,
                    a: 255,
                },
                radius: *radius,
            });
            out.push(RenderCommand::Border {
                bounds,
                color: Color {
                    r: 200,
                    g: 200,
                    b: 200,
                    a: 255,
                },
                width: 1.0,
                radius: *radius,
            });
            // Inset the text by the same 6px the measure callback
            // budgeted for, so the glyphs sit centred in the box.
            let text_color = if *is_placeholder {
                Color {
                    r: props.color.r,
                    g: props.color.g,
                    b: props.color.b,
                    a: (props.color.a as u16 * 153 / 255) as u8,
                }
            } else {
                props.color
            };
            out.push(RenderCommand::Text {
                bounds: Rect {
                    x: bounds.x + 6.0,
                    y: bounds.y + 4.0,
                    width: (bounds.width - 12.0).max(0.0),
                    height: (bounds.height - 8.0).max(0.0),
                },
                content: text.clone(),
                color: text_color,
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
    overlays: Vec<Overlay>,
    viewport: Viewport,
    cache: Vec<RenderCommand>,
    hit_cache: Vec<HitRect>,
    dirty: bool,
    hovered_id: Option<String>,
}

impl Surface {
    pub fn new(tree: Node, viewport: Viewport) -> Self {
        Self {
            tree,
            overlays: Vec::new(),
            viewport,
            cache: Vec::new(),
            hit_cache: Vec::new(),
            dirty: true,
            hovered_id: None,
        }
    }

    /// Currently hovered node id, if any. The host (input dispatcher)
    /// owns hit-testing and feeds this through [`Self::set_hovered`].
    pub fn hovered_id(&self) -> Option<&str> {
        self.hovered_id.as_deref()
    }

    /// Update the hovered node. Marks the surface dirty when the id
    /// transitions across a node that declares `props.hover` — there's
    /// no point recomputing if neither the leaving nor the entering
    /// node has hover overrides. Called by the host on every pointer
    /// move whose hit-test result changed.
    pub fn set_hovered(&mut self, id: Option<String>) {
        if id == self.hovered_id {
            return;
        }
        let affects = |i: &str| {
            node_has_hover(&self.tree, i)
                || self.overlays.iter().any(|o| node_has_hover(&o.node, i))
        };
        let prev_affects = self.hovered_id.as_deref().is_some_and(affects);
        let next_affects = id.as_deref().is_some_and(affects);
        self.hovered_id = id;
        if prev_affects || next_affects {
            self.dirty = true;
        }
    }

    /// Snapshot of the current overlay stack, in z-order (last → top).
    pub fn overlays(&self) -> &[Overlay] {
        &self.overlays
    }

    /// Replace the entire overlay stack. Always marks dirty — use the
    /// finer-grained `push_overlay` / `remove_overlay` for partial
    /// updates.
    pub fn set_overlays(&mut self, overlays: Vec<Overlay>) {
        self.overlays = overlays;
        self.dirty = true;
    }

    /// Push an overlay on top of the stack (it paints last → on top).
    /// If an overlay with the same id is already present it's replaced
    /// in place, preserving stacking order — so a host can call this
    /// every time a toast or palette state changes without bouncing
    /// the z-order around.
    pub fn push_overlay(&mut self, overlay: Overlay) {
        if let Some(slot) = self.overlays.iter_mut().find(|o| o.id == overlay.id) {
            *slot = overlay;
        } else {
            self.overlays.push(overlay);
        }
        self.dirty = true;
    }

    /// Remove the overlay with `id`. Returns `true` if one was removed.
    pub fn remove_overlay(&mut self, id: &str) -> bool {
        let before = self.overlays.len();
        self.overlays.retain(|o| o.id != id);
        let removed = self.overlays.len() != before;
        if removed {
            self.dirty = true;
        }
        removed
    }

    pub fn clear_overlays(&mut self) {
        if !self.overlays.is_empty() {
            self.overlays.clear();
            self.dirty = true;
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
        self.rebuild_if_dirty();
        &self.cache
    }

    /// Hit-test the topmost container whose resolved rect contains
    /// `(x, y)`. Returns `None` when the point lands on an anonymous
    /// wrapper (empty id), on a leaf, or outside the tree entirely.
    ///
    /// **Ordering.** Hit rects are stored in paint order (parents
    /// before children, earlier siblings before later ones). Walking
    /// the cache in **reverse** gives "topmost paint = topmost hit",
    /// which is the convention every host (chrome, gizmo, picker)
    /// already expects. Overlays sit after the main tree, so an open
    /// command palette / context menu naturally captures clicks over
    /// the chrome behind it.
    ///
    /// Recomputes the layout cache iff dirty — calling this on every
    /// `PointerDown` is cheap when nothing has changed since the last
    /// `commands()` invocation.
    pub fn hit_test_at(&mut self, x: f32, y: f32) -> Option<&HitRect> {
        self.rebuild_if_dirty();
        self.hit_cache.iter().rev().find(|r| {
            x >= r.bounds.x
                && x <= r.bounds.x + r.bounds.width
                && y >= r.bounds.y
                && y <= r.bounds.y + r.bounds.height
        })
    }

    /// All hit rects from the most recent layout pass, in paint
    /// order. Exposed for diagnostic dumps (e2e screenshot tooling)
    /// and for cross-cutting hit-tests the host might want to perform
    /// without an exact point (e.g. "which container has
    /// `data-role=foo`?"). The hot path is [`Self::hit_test_at`].
    pub fn hit_rects(&mut self) -> &[HitRect] {
        self.rebuild_if_dirty();
        &self.hit_cache
    }

    fn rebuild_if_dirty(&mut self) {
        if !self.dirty {
            return;
        }
        let (cmds, hits) = compute_full_with_hits(
            &self.tree,
            &self.overlays,
            self.viewport,
            self.hovered_id.as_deref(),
        );
        self.cache = cmds;
        self.hit_cache = hits;
        self.dirty = false;
    }
}

/// Walk the tree looking for a container with `id` whose `hover`
/// override is set. Used by [`Surface::set_hovered`] to skip dirty
/// flips when neither the leaving nor entering node would change
/// paint anyway.
fn node_has_hover(tree: &Node, id: &str) -> bool {
    match tree {
        Node::Container {
            id: nid,
            props,
            children,
        } => {
            if nid == id && props.hover.as_ref().is_some_and(|h| !h.is_empty()) {
                return true;
            }
            children.iter().any(|c| node_has_hover(c, id))
        }
        Node::Text { .. } | Node::Spacer { .. } | Node::Image { .. } | Node::TextInput { .. } => {
            false
        }
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
                        ..Default::default()
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

    fn hover_button_tree() -> Node {
        Node::Container {
            id: "btn".into(),
            props: ContainerProps {
                width: Sizing::Fixed(28.0),
                height: Sizing::Fixed(28.0),
                background: Some(rgb(255, 255, 255)),
                hover: Some(HoverOverrides {
                    background: Some(rgb(0, 100, 200)),
                    radius: None,
                }),
                ..Default::default()
            },
            children: vec![],
        }
    }

    fn rectangle_color(commands: &[RenderCommand]) -> Color {
        commands
            .iter()
            .find_map(|c| match c {
                RenderCommand::Rectangle { color, .. } => Some(*color),
                _ => None,
            })
            .expect("at least one rectangle")
    }

    #[test]
    fn hover_overrides_swap_in_when_id_matches() {
        let resting = compute_with_hover(
            &hover_button_tree(),
            Viewport {
                width: 100.0,
                height: 100.0,
            },
            None,
        );
        assert_eq!(rectangle_color(&resting), rgb(255, 255, 255));

        let hovered = compute_with_hover(
            &hover_button_tree(),
            Viewport {
                width: 100.0,
                height: 100.0,
            },
            Some("btn"),
        );
        assert_eq!(rectangle_color(&hovered), rgb(0, 100, 200));
    }

    #[test]
    fn hover_overrides_ignored_when_id_does_not_match() {
        let cmds = compute_with_hover(
            &hover_button_tree(),
            Viewport {
                width: 100.0,
                height: 100.0,
            },
            Some("some-other-node"),
        );
        assert_eq!(rectangle_color(&cmds), rgb(255, 255, 255));
    }

    #[test]
    fn surface_set_hovered_dirty_only_when_paint_actually_changes() {
        let mut surface = Surface::new(
            hover_button_tree(),
            Viewport {
                width: 100.0,
                height: 100.0,
            },
        );
        let _ = surface.commands();
        assert!(!surface.is_dirty());

        // Entering a hover-affecting node → dirty (need to paint hover state).
        surface.set_hovered(Some("btn".into()));
        assert!(surface.is_dirty());
        let _ = surface.commands();

        // Leaving a hover-affecting node → dirty (need to paint resting state).
        surface.set_hovered(Some("nonexistent".into()));
        assert!(surface.is_dirty());
        let _ = surface.commands();

        // Drift between two non-affecting nodes → no recompute.
        surface.set_hovered(Some("also-nonexistent".into()));
        assert!(
            !surface.is_dirty(),
            "moves between non-affecting nodes shouldn't recompute"
        );

        // Idempotent: same id doesn't dirty.
        surface.set_hovered(Some("also-nonexistent".into()));
        assert!(!surface.is_dirty());
    }

    #[test]
    fn surface_hover_swap_round_trip() {
        let mut surface = Surface::new(
            hover_button_tree(),
            Viewport {
                width: 100.0,
                height: 100.0,
            },
        );
        assert_eq!(rectangle_color(surface.commands()), rgb(255, 255, 255));
        surface.set_hovered(Some("btn".into()));
        assert_eq!(rectangle_color(surface.commands()), rgb(0, 100, 200));
        surface.set_hovered(None);
        assert_eq!(rectangle_color(surface.commands()), rgb(255, 255, 255));
    }

    #[test]
    fn semantic_button_constructor_includes_type_attr() {
        let s = Semantic::button();
        assert_eq!(s.tag.as_deref(), Some("button"));
        assert!(s.attrs.iter().any(|(k, v)| k == "type" && v == "button"));
    }

    #[test]
    fn semantic_with_attr_if_branches_on_cond() {
        let on = Semantic::button().with_attr_if(true, "aria-pressed", "true");
        assert!(on
            .attrs
            .iter()
            .any(|(k, v)| k == "aria-pressed" && v == "true"));

        let off = Semantic::button().with_attr_if(false, "aria-pressed", "true");
        assert!(off.attrs.iter().all(|(k, _)| k != "aria-pressed"));
    }

    fn fixed_box(id: &str, w: f32, h: f32, color: Color) -> Node {
        Node::Container {
            id: id.into(),
            props: ContainerProps {
                width: Sizing::Fixed(w),
                height: Sizing::Fixed(h),
                background: Some(color),
                ..Default::default()
            },
            children: vec![],
        }
    }

    fn rectangle_at(commands: &[RenderCommand], color: Color) -> Rect {
        commands
            .iter()
            .find_map(|c| match c {
                RenderCommand::Rectangle {
                    bounds, color: c2, ..
                } if *c2 == color => Some(*bounds),
                _ => None,
            })
            .expect("expected rectangle of given color")
    }

    #[test]
    fn overlay_paints_after_main_tree_at_resolved_corner() {
        let main = fixed_box("main", 100.0, 100.0, rgb(10, 10, 10));
        let toast = fixed_box("toast", 200.0, 60.0, rgb(20, 20, 20));
        let overlay = Overlay::new(
            "toast",
            OverlayAnchor::Corner {
                corner: Corner::BottomRight,
                inset: Inset::all(16.0),
            },
            toast,
        );
        let cmds = compute_full(
            &main,
            std::slice::from_ref(&overlay),
            Viewport {
                width: 800.0,
                height: 600.0,
            },
            None,
        );
        // Main tree paints first, overlay second — z-order via order.
        let main_idx = cmds
            .iter()
            .position(
                |c| matches!(c, RenderCommand::Rectangle { color, .. } if *color == rgb(10,10,10)),
            )
            .unwrap();
        let toast_idx = cmds
            .iter()
            .position(
                |c| matches!(c, RenderCommand::Rectangle { color, .. } if *color == rgb(20,20,20)),
            )
            .unwrap();
        assert!(toast_idx > main_idx, "overlay must paint after main");

        let bounds = rectangle_at(&cmds, rgb(20, 20, 20));
        // BottomRight at (800-200-16, 600-60-16) = (584, 524)
        assert_eq!(bounds.x, 584.0);
        assert_eq!(bounds.y, 524.0);
    }

    #[test]
    fn overlay_anchor_center_resolves_to_viewport_centre_with_offset() {
        let palette = fixed_box("p", 400.0, 100.0, rgb(50, 60, 70));
        let overlay = Overlay::new(
            "palette",
            OverlayAnchor::Center { offset_y: -100.0 },
            palette,
        );
        let cmds = compute_full(
            &fixed_box("root", 1.0, 1.0, rgb(0, 0, 0)),
            std::slice::from_ref(&overlay),
            Viewport {
                width: 800.0,
                height: 600.0,
            },
            None,
        );
        let bounds = rectangle_at(&cmds, rgb(50, 60, 70));
        // x = (800-400)/2 = 200, y = (600-100)/2 - 100 = 250 - 100 = 150
        assert_eq!(bounds.x, 200.0);
        assert_eq!(bounds.y, 150.0);
    }

    #[test]
    fn overlay_anchor_point_translates_verbatim() {
        let tip = fixed_box("tip", 80.0, 24.0, rgb(99, 99, 99));
        let overlay = Overlay::new("tip", OverlayAnchor::Point { x: 312.5, y: 48.0 }, tip);
        let cmds = compute_full(
            &fixed_box("root", 1.0, 1.0, rgb(0, 0, 0)),
            std::slice::from_ref(&overlay),
            Viewport {
                width: 1000.0,
                height: 600.0,
            },
            None,
        );
        let bounds = rectangle_at(&cmds, rgb(99, 99, 99));
        assert_eq!(bounds.x, 312.5);
        assert_eq!(bounds.y, 48.0);
    }

    #[test]
    fn surface_overlay_lifecycle_marks_dirty_only_when_stack_changes() {
        let mut surface = Surface::new(
            fixed_box("root", 1.0, 1.0, rgb(0, 0, 0)),
            Viewport {
                width: 400.0,
                height: 300.0,
            },
        );
        let _ = surface.commands();
        assert!(!surface.is_dirty());

        // Push → dirty.
        surface.push_overlay(Overlay::new(
            "t1",
            OverlayAnchor::Corner {
                corner: Corner::BottomRight,
                inset: Inset::all(8.0),
            },
            fixed_box("t1", 100.0, 40.0, rgb(11, 22, 33)),
        ));
        assert!(surface.is_dirty());
        let cmds = surface.commands();
        assert!(cmds.iter().any(|c| matches!(
            c,
            RenderCommand::Rectangle { color, .. } if *color == rgb(11, 22, 33)
        )));
        assert!(!surface.is_dirty());

        // Push same id → replaces, still dirty.
        surface.push_overlay(Overlay::new(
            "t1",
            OverlayAnchor::Corner {
                corner: Corner::BottomRight,
                inset: Inset::all(8.0),
            },
            fixed_box("t1", 100.0, 40.0, rgb(44, 55, 66)),
        ));
        assert!(surface.is_dirty());
        assert_eq!(surface.overlays().len(), 1, "same id replaces in place");
        let cmds = surface.commands();
        assert!(cmds.iter().any(|c| matches!(
            c,
            RenderCommand::Rectangle { color, .. } if *color == rgb(44, 55, 66)
        )));

        // Remove unknown → no-op.
        let removed = surface.remove_overlay("does-not-exist");
        assert!(!removed);
        assert!(!surface.is_dirty());

        // Remove existing → dirty.
        let removed = surface.remove_overlay("t1");
        assert!(removed);
        assert!(surface.is_dirty());

        // Clear empty → no-op.
        let _ = surface.commands();
        assert!(!surface.is_dirty());
        surface.clear_overlays();
        assert!(!surface.is_dirty(), "clearing empty stack is a no-op");
    }

    #[test]
    fn surface_overlay_hover_swap_dirties_through_overlay_subtree() {
        // Overlay subtree contains a hover-affecting node.
        let inner = Node::Container {
            id: "btn".into(),
            props: ContainerProps {
                width: Sizing::Fixed(40.0),
                height: Sizing::Fixed(20.0),
                background: Some(rgb(255, 255, 255)),
                hover: Some(HoverOverrides {
                    background: Some(rgb(1, 2, 3)),
                    radius: None,
                }),
                ..Default::default()
            },
            children: vec![],
        };
        let mut surface = Surface::new(
            fixed_box("root", 1.0, 1.0, rgb(0, 0, 0)),
            Viewport {
                width: 400.0,
                height: 300.0,
            },
        );
        surface.push_overlay(Overlay::new(
            "popup",
            OverlayAnchor::Center { offset_y: 0.0 },
            inner,
        ));
        let _ = surface.commands();
        assert!(!surface.is_dirty());

        // Hovering the overlay's interior must dirty — the lookup
        // walks both main + overlay subtrees.
        surface.set_hovered(Some("btn".into()));
        assert!(surface.is_dirty());
        let cmds = surface.commands().to_vec();
        assert!(cmds.iter().any(|c| matches!(
            c,
            RenderCommand::Rectangle { color, .. } if *color == rgb(1, 2, 3)
        )));
    }

    #[test]
    fn semantic_with_aria_label_opt_handles_some_and_none() {
        let labelled = Semantic::button().with_aria_label_opt(Some("Close"));
        assert_eq!(labelled.aria_label.as_deref(), Some("Close"));

        let unlabelled: Semantic = Semantic::button().with_aria_label_opt(None::<&str>);
        assert!(unlabelled.aria_label.is_none());
    }

    // ── §43 hit-test surface ──────────────────────────────────────────

    fn id_container(
        id: &str,
        w: f32,
        h: f32,
        attrs: Vec<(&str, &str)>,
        children: Vec<Node>,
    ) -> Node {
        let mut semantic = Semantic::tag("div");
        for (k, v) in attrs {
            semantic = semantic.with_attr(k, v);
        }
        Node::Container {
            id: id.into(),
            props: ContainerProps {
                width: Sizing::Fixed(w),
                height: Sizing::Fixed(h),
                semantic,
                ..Default::default()
            },
            children,
        }
    }

    #[test]
    fn hit_test_returns_topmost_container_for_point_inside() {
        // Row of two 100x40 boxes, each with a distinct id. Hits at (10,10)
        // land in the first; hits at (160,20) land in the second.
        let tree = Node::Container {
            id: "root".into(),
            props: ContainerProps {
                direction: Direction::Row,
                gap: 0.0,
                width: Sizing::Fixed(300.0),
                height: Sizing::Fixed(40.0),
                semantic: Semantic::tag("div").with_attr("data-role", "row"),
                ..Default::default()
            },
            children: vec![
                id_container("a", 100.0, 40.0, vec![("data-role", "alpha")], vec![]),
                id_container("b", 100.0, 40.0, vec![("data-role", "beta")], vec![]),
            ],
        };
        let mut surface = Surface::new(
            tree,
            Viewport {
                width: 400.0,
                height: 100.0,
            },
        );
        let hit = surface.hit_test_at(10.0, 10.0).expect("hit");
        assert_eq!(hit.id, "a");
        assert!(hit
            .attrs
            .iter()
            .any(|(k, v)| k == "data-role" && v == "alpha"));
        let hit = surface.hit_test_at(160.0, 20.0).expect("hit");
        assert_eq!(hit.id, "b");
        assert!(hit
            .attrs
            .iter()
            .any(|(k, v)| k == "data-role" && v == "beta"));
    }

    #[test]
    fn hit_test_picks_deepest_container() {
        // Outer 100x100 holding an inner 40x40 — a hit inside the inner
        // returns the inner (deepest = topmost), not the outer.
        let tree = id_container(
            "outer",
            100.0,
            100.0,
            vec![("data-role", "outer")],
            vec![id_container(
                "inner",
                40.0,
                40.0,
                vec![("data-role", "inner")],
                vec![],
            )],
        );
        let mut surface = Surface::new(
            tree,
            Viewport {
                width: 200.0,
                height: 200.0,
            },
        );
        let hit = surface.hit_test_at(10.0, 10.0).expect("hit inside inner");
        assert_eq!(hit.id, "inner");
    }

    #[test]
    fn hit_test_returns_none_outside_tree() {
        let tree = id_container("a", 50.0, 50.0, vec![], vec![]);
        let mut surface = Surface::new(
            tree,
            Viewport {
                width: 200.0,
                height: 200.0,
            },
        );
        assert!(surface.hit_test_at(80.0, 80.0).is_none());
    }

    #[test]
    fn hit_test_skips_anonymous_wrapper_containers() {
        // The outer wrapper has no id (it's anonymous like a synthesised
        // surface wrap); only the id'd child shows up in the hit cache.
        let tree = Node::Container {
            id: String::new(),
            props: ContainerProps {
                width: Sizing::Fixed(100.0),
                height: Sizing::Fixed(100.0),
                ..Default::default()
            },
            children: vec![id_container("named", 100.0, 100.0, vec![], vec![])],
        };
        let mut surface = Surface::new(
            tree,
            Viewport {
                width: 200.0,
                height: 200.0,
            },
        );
        let hit = surface.hit_test_at(10.0, 10.0).expect("hit");
        assert_eq!(hit.id, "named");
        assert_eq!(surface.hit_rects().len(), 1);
    }
}
