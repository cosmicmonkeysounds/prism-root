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

use std::collections::HashSet;

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
// The `TextInput` variant carries a fat editor-state bundle —
// caret + selection + spans + scroll + bracket-match + flags. The
// `Container` variant by contrast is small. Boxing TextInput would
// force a pointer indirection on every read in the layout / paint
// hot paths just to shave bytes off an enum stored as `Box<Node>`
// already at every aggregate site (`Node::Container.children` is a
// `Vec<Node>`, not a slice of variants). Allow the variance.
#[allow(clippy::large_enum_variant)]
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
        /// Resting background. `None` keeps the hardcoded white default
        /// the runtime has always painted; setting `style:background`
        /// from the DSL overrides it (e.g. property-row inputs paint a
        /// faint tint so the row pops against the panel).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        background: Option<Color>,
        /// Declarative hover overrides — same shape and resolution rule
        /// as `ContainerProps::hover`. Sparse: only fields the input
        /// actually wants to swap on hover land here. Resolved at
        /// command-emit time when the input's `id` matches the
        /// surface's hovered id.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        hover: Option<HoverOverrides>,
        /// When `true`, the paint pass draws a 1-px vertical caret bar
        /// after the rendered text + bumps the border colour to the
        /// accent so the user can see where their keystrokes will land.
        /// Hosts set this on the input that's currently receiving
        /// keyboard input (`state.field_focus` in the shell). Defaults
        /// to `false` so headless / SSR paths render the resting box.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        focused: bool,
        /// `true` for code editors / textareas — `\n` is laid out as
        /// a hard line break, the input grows vertically with content,
        /// and Up/Down arrows navigate lines rather than the field's
        /// caret jumping to the doc start / end. Defaults to single-
        /// line behaviour so the existing string-property rows keep
        /// their current shape with zero JSON noise.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        multiline: bool,
        /// Caret position as a *byte* offset into `value`. Carried so
        /// the paint pass can place the bar in the middle of the
        /// shaped text (arrow-key navigation, click-to-position,
        /// selection collapse) instead of always at the end. `None`
        /// preserves the legacy "caret at shaped-text end" behaviour
        /// — used by inline rows that don't manage caret state yet.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        caret_byte: Option<usize>,
        /// Active selection as `(start_byte, end_byte)` with `start
        /// < end`. The paint pass highlights the matching glyph
        /// ranges. `None` means no selection.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        selection: Option<(usize, usize)>,
        /// Syntax-highlight spans — per-byte-range colour overrides.
        /// Empty for plain text inputs; populated by the code editor's
        /// tokenizer for the active language. Glyphs outside every
        /// span paint in `props.color`.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        spans: Vec<crate::command::TextSpan>,
        /// Horizontal scroll offset in CSS pixels. Subtracted from
        /// every shaped glyph's x-position at paint time; the
        /// renderer scissor-clips to the input's bounds so glyphs
        /// outside the viewport stay hidden. `0.0` matches the
        /// non-scrolling resting case (single-line property fields).
        #[serde(default, skip_serializing_if = "f32_is_zero")]
        scroll_x: f32,
        /// Vertical scroll offset in CSS pixels. Hosts auto-scroll
        /// on caret moves through the shell's editor service so the
        /// caret stays visible.
        #[serde(default, skip_serializing_if = "f32_is_zero")]
        scroll_y: f32,
        /// Byte range to underline. Used for IME preedit decoration
        /// — the in-progress composition reads as "tentative". Lives
        /// on the input node (not just the underlying Text command)
        /// so a host can express the preedit independently of the
        /// real user selection: both can coexist, painting the
        /// preedit underline and the selection highlight together.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        underline: Option<(usize, usize)>,
        /// Byte offsets of bracket pairs the paint pass should
        /// outline. Each entry is `(open_byte, close_byte)` — the
        /// runtime draws a thin outline around the glyph at each
        /// offset so users see the matching bracket of the one
        /// adjacent to the caret. Optional; empty vector renders
        /// nothing extra. Resolved by the host via
        /// [`editor::TextEditor::matching_bracket_for`].
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        bracket_match: Vec<usize>,
        /// `true` when the editor should paint a subtle highlight
        /// behind the caret's line. Hosts flip this on whenever the
        /// editor is focused so the active line stands out from
        /// neighbours. Off by default so inline string-property
        /// fields don't gain a stray strip.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        highlight_current_line: bool,
    },
}

fn f32_is_zero(v: &f32) -> bool {
    *v == 0.0
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
    /// Per-container opacity multiplier in `[0.0, 1.0]`. `None`
    /// behaves like `Some(1.0)` (fully opaque) and skips the
    /// multiply path so existing nodes stay byte-identical.
    /// Cascades into children at command-emit time — text, images,
    /// borders, and nested container backgrounds all paint at
    /// `parent_opacity * own_opacity`. Mirrors CSS `opacity` and
    /// is the prop the animator's `animate:in` / `animate:out`
    /// substrate transitions for fade entrances and exits.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opacity: Option<f32>,
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
/// declares `props.hover` and its id matches `hovered_id` *or any of
/// its ancestors*, the overrides are folded in at the moment its
/// `NodeContext` is built — layout itself doesn't shift (hover
/// affects paint, not box model), so the only commands that change
/// are the rectangle's colour / radius. Pass `None` for the resting
/// state.
///
/// **Ancestor matching (the CSS `:hover` semantic).** The deepest
/// hit-test result names a single leaf-ish node, but UI authors
/// typically want both the leaf *and* its row / card / panel to react.
/// We pre-compute the ancestor chain of `hovered_id` here and every
/// container on the chain gets its hover overrides applied — same way
/// CSS bubbles `:hover` up the DOM. Authors who want strictly-self
/// hover put the override on the leaf only; authors who want a row
/// to darken when the user mouses a single field put a hover override
/// on the row's container and it fires for free.
pub fn compute_with_hover(
    tree: &Node,
    viewport: Viewport,
    hovered_id: Option<&str>,
) -> Vec<RenderCommand> {
    compute_full(tree, &[], viewport, hovered_id)
}

/// Walk `tree` once to collect the ids of every node along the path
/// from the root to the node whose id equals `target`. Returns `true`
/// when found so callers nesting walks (overlay sweep) can stop. The
/// `out` set receives every non-empty id from the target node up to
/// (but not past) the root, so the [`build_taffy_subtree`] hover-fold
/// can match an ancestor's id without re-walking.
fn collect_hover_path(tree: &Node, target: &str, out: &mut HashSet<String>) -> bool {
    if target.is_empty() {
        return false;
    }
    match tree {
        Node::Container { id, children, .. } => {
            if id == target {
                if !id.is_empty() {
                    out.insert(id.clone());
                }
                return true;
            }
            for child in children {
                if collect_hover_path(child, target, out) {
                    if !id.is_empty() {
                        out.insert(id.clone());
                    }
                    return true;
                }
            }
            false
        }
        Node::TextInput { id, .. } => {
            if id == target {
                if !id.is_empty() {
                    out.insert(id.clone());
                }
                true
            } else {
                false
            }
        }
        Node::Text { .. } | Node::Spacer { .. } | Node::Image { .. } => false,
    }
}

/// Build the set of ids on the hover path through `tree` plus
/// `overlays`. Overlays are independent subtrees; the walker tries
/// each separately. The result is empty when nothing is hovered or
/// the id isn't reachable — equivalent to the legacy
/// `Option<&str>::None` case.
fn hover_path_for(tree: &Node, overlays: &[Overlay], hovered_id: Option<&str>) -> HashSet<String> {
    let Some(target) = hovered_id else {
        return HashSet::new();
    };
    let mut out = HashSet::new();
    if collect_hover_path(tree, target, &mut out) {
        return out;
    }
    for overlay in overlays {
        out.clear();
        if collect_hover_path(&overlay.node, target, &mut out) {
            return out;
        }
    }
    out.clear();
    out
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
    let hover_path = hover_path_for(tree, overlays, hovered_id);
    let mut out = Vec::new();
    compute_subtree_into(tree, viewport, &hover_path, 0.0, 0.0, &mut out);
    for overlay in overlays {
        compute_overlay_into(overlay, viewport, &hover_path, &mut out);
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
    let hover_path = hover_path_for(tree, overlays, hovered_id);
    let mut commands = Vec::new();
    let mut hits = Vec::new();
    compute_subtree_into_with_hits(
        tree,
        viewport,
        &hover_path,
        0.0,
        0.0,
        &mut commands,
        &mut hits,
    );
    for overlay in overlays {
        compute_overlay_into_with_hits(overlay, viewport, &hover_path, &mut commands, &mut hits);
    }
    (commands, hits)
}

fn compute_subtree_into(
    tree: &Node,
    viewport: Viewport,
    hover_path: &HashSet<String>,
    origin_x: f32,
    origin_y: f32,
    out: &mut Vec<RenderCommand>,
) -> Option<Size<f32>> {
    let mut taffy: TaffyTree<NodeContext> = TaffyTree::new();
    let root = build_taffy_subtree(&mut taffy, tree, None, hover_path);
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
    hover_path: &HashSet<String>,
    origin_x: f32,
    origin_y: f32,
    commands: &mut Vec<RenderCommand>,
    hits: &mut Vec<HitRect>,
) -> Option<Size<f32>> {
    let mut taffy: TaffyTree<NodeContext> = TaffyTree::new();
    let root = build_taffy_subtree(&mut taffy, tree, None, hover_path);
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
    hover_path: &HashSet<String>,
    commands: &mut Vec<RenderCommand>,
    hits: &mut Vec<HitRect>,
) {
    let mut probe: TaffyTree<NodeContext> = TaffyTree::new();
    let probe_root = build_taffy_subtree(&mut probe, &overlay.node, None, hover_path);
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
    match source {
        Node::Container {
            id,
            props,
            children,
        } => {
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
        // `Node::TextInput` participates in hit-testing so the shell
        // event router can route input clicks (focus capture, the
        // `bind:value` two-way edit session). Inputs carry the same
        // `Semantic.attrs` shape containers do — `data-bind-value`,
        // `data-role`, etc. round-trip uniformly.
        Node::TextInput { id, semantic, .. } => {
            if !id.is_empty() {
                out.push(HitRect {
                    id: id.clone(),
                    bounds,
                    attrs: semantic.attrs.clone(),
                });
            }
        }
        // Text / Spacer / Image leaves still skip the cache — they
        // don't carry interaction attrs today.
        _ => {}
    }
}

fn compute_overlay_into(
    overlay: &Overlay,
    viewport: Viewport,
    hover_path: &HashSet<String>,
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
    let probe_root = build_taffy_subtree(&mut probe, &overlay.node, None, hover_path);
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
        /// Per-container opacity multiplier (1.0 = fully opaque).
        /// Cascades into children at command-emit time. See
        /// [`ContainerProps::opacity`] for the authoring shape.
        opacity: f32,
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
        /// Resolved background colour the paint pass should fill the
        /// input rectangle with — already hover-folded by
        /// `build_taffy_subtree`. `None` falls back to the legacy white
        /// fill so existing untouched inputs render identically.
        background: Option<Color>,
        focused: bool,
        multiline: bool,
        /// Caret byte offset into `text`. `None` falls back to the
        /// "caret at end" rendering — only inline rows still rely on
        /// that, and only when an editor session isn't active. Tied
        /// to the same node-context lifecycle as `text`.
        caret_byte: Option<usize>,
        selection: Option<(usize, usize)>,
        spans: Vec<crate::command::TextSpan>,
        scroll_x: f32,
        scroll_y: f32,
        underline: Option<(usize, usize)>,
        bracket_match: Vec<usize>,
        highlight_current_line: bool,
    },
}

fn build_taffy_subtree(
    taffy: &mut TaffyTree<NodeContext>,
    node: &Node,
    parent_direction: Option<Direction>,
    hover_path: &HashSet<String>,
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
                .map(|c| build_taffy_subtree(taffy, c, own_direction, hover_path))
                .collect();
            // Fold hover overrides into the resting paint state when
            // this container is on the hover path (the hovered node or
            // any of its ancestors — see [`hover_path_for`]). Layout-
            // affecting hover changes would need to live on `style`
            // instead; intentionally not supported — hover is
            // paint-only.
            let on_path = !id.is_empty() && hover_path.contains(id);
            let (background, radius) = match (props.hover.as_ref(), on_path) {
                (Some(h), true) => (
                    h.background.or(props.background),
                    h.radius.unwrap_or(props.radius),
                ),
                _ => (props.background, props.radius),
            };
            let opacity = props.opacity.unwrap_or(1.0).clamp(0.0, 1.0);
            let ctx = NodeContext::Container {
                background,
                radius,
                opacity,
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
            id,
            value,
            placeholder,
            props,
            width,
            height,
            radius,
            background,
            hover,
            focused,
            multiline,
            caret_byte,
            selection,
            spans,
            scroll_x,
            scroll_y,
            underline,
            bracket_match,
            highlight_current_line,
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
            // Same hover-fold rule as the container arm above: if the
            // input is on the hover path (its own id, or — for the rare
            // case an input wraps further interactive descendants — an
            // ancestor of the hovered node), swap its resting
            // `background` for the override's. Layout-affecting hover
            // changes aren't supported (hover is paint-only).
            let on_path = !id.is_empty() && hover_path.contains(id);
            let resolved_background = match (hover.as_ref(), on_path) {
                (Some(h), true) => h.background.or(*background),
                _ => *background,
            };
            let ctx = NodeContext::TextInput {
                text,
                is_placeholder,
                props: props.clone(),
                radius: *radius,
                background: resolved_background,
                focused: *focused,
                multiline: *multiline,
                caret_byte: *caret_byte,
                selection: *selection,
                spans: spans.clone(),
                scroll_x: *scroll_x,
                scroll_y: *scroll_y,
                underline: *underline,
                bracket_match: bracket_match.clone(),
                highlight_current_line: *highlight_current_line,
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
        Some(NodeContext::TextInput {
            text,
            props,
            multiline,
            ..
        }) => {
            // Same anti-shrink rule as Text leaves, plus the 12px / 8px
            // input padding so empty inputs still have clickable extent.
            // Multi-line inputs grow vertically with each `\n` so a
            // code editor reaches its natural height inside a flex
            // parent that lets it.
            let line_count = if *multiline {
                1 + text.bytes().filter(|b| *b == b'\n').count()
            } else {
                1
            };
            let widest_line = if *multiline {
                text.split('\n')
                    .map(|line| line.chars().count())
                    .max()
                    .unwrap_or(0)
                    .max(1)
            } else {
                text.chars().count().max(1)
            };
            let natural_w = widest_line as f32 * props.font_size * 0.55 + 12.0;
            let width = match known_dimensions.width {
                Some(w) if w >= natural_w => w,
                _ => natural_w,
            };
            let natural_h = line_count as f32 * props.font_size * 1.2 + 8.0;
            let height = known_dimensions.height.unwrap_or(natural_h);
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
    emit_commands_with_opacity(taffy, id, parent_x, parent_y, 1.0, out);
}

/// Internal recursion variant that threads a cumulative
/// `parent_opacity` multiplier into every emitted colour. A
/// container's own [`ContainerProps::opacity`] composes with its
/// parent's, so opacity cascades CSS-style through the tree. The
/// public `emit_commands` is the `parent_opacity = 1.0` entry
/// point.
fn emit_commands_with_opacity(
    taffy: &TaffyTree<NodeContext>,
    id: NodeId,
    parent_x: f32,
    parent_y: f32,
    parent_opacity: f32,
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
        Some(NodeContext::Container {
            background,
            radius,
            opacity,
        }) => {
            let effective = (parent_opacity * *opacity).clamp(0.0, 1.0);
            if let Some(bg) = background {
                out.push(RenderCommand::Rectangle {
                    bounds,
                    color: scale_color_alpha(*bg, effective),
                    radius: *radius,
                });
            }
            for child in taffy.children(id).unwrap_or_default() {
                emit_commands_with_opacity(taffy, child, bounds.x, bounds.y, effective, out);
            }
        }
        Some(NodeContext::Text { content, props }) => {
            out.push(RenderCommand::Text {
                bounds,
                content: content.clone(),
                color: scale_color_alpha(props.color, parent_opacity),
                font_size: props.font_size,
                caret: None,
                caret_byte: None,
                selection: None,
                selection_color: None,
                spans: Vec::new(),
                underline: None,
                underline_color: None,
                glyph_outlines: Vec::new(),
                glyph_outline_color: None,
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
                tint: tint.map(|c| scale_color_alpha(c, parent_opacity)),
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
            background,
            focused,
            multiline,
            caret_byte,
            selection,
            spans,
            scroll_x,
            scroll_y,
            underline,
            bracket_match,
            highlight_current_line,
        }) => {
            // Resting white fill kept as the default so every existing
            // string-property input renders byte-identically; the DSL's
            // `style:background` overrides it (and `:hovered` is already
            // folded in by `build_taffy_subtree`).
            let fill = background.unwrap_or(Color {
                r: 255,
                g: 255,
                b: 255,
                a: 255,
            });
            out.push(RenderCommand::Rectangle {
                bounds,
                color: scale_color_alpha(fill, parent_opacity),
                radius: *radius,
            });
            // Border colour bumps to the accent when the input is the
            // active focus target — same `#0060c0` family the rest of
            // the shell's "active" chrome uses. Resting border is
            // neutral grey. SSR backends ignore `focused` so the
            // semantic-HTML pass renders the resting form.
            let border = if *focused {
                Color {
                    r: 0,
                    g: 96,
                    b: 192,
                    a: 255,
                }
            } else {
                Color {
                    r: 200,
                    g: 200,
                    b: 200,
                    a: 255,
                }
            };
            let border_width = if *focused { 2.0 } else { 1.0 };
            out.push(RenderCommand::Border {
                bounds,
                color: scale_color_alpha(border, parent_opacity),
                width: border_width,
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
            let text_left = bounds.x + 6.0;
            let text_top = bounds.y + 4.0;
            let text_width = (bounds.width - 12.0).max(0.0);
            let text_height = (bounds.height - 8.0).max(0.0);
            // When focused, attach a caret colour to the Text
            // command so the paint pass can place the bar at the
            // *shaped* text end — cosmic-text knows the exact pixel
            // width of the rendered glyphs, which is what we want.
            // The previous approach (a separate Rectangle at the
            // `chars * font_size * 0.55` natural-width estimate) was
            // an *upper bound* on the actual glyph width, so the
            // caret sat a few pixels past the last glyph and
            // Backspace then deleted the next-to-last char from the
            // user's mental model.
            let caret = if *focused && !*is_placeholder {
                Some(Color {
                    r: 0,
                    g: 96,
                    b: 192,
                    a: 255,
                })
            } else {
                None
            };
            // The selection highlight has the accent's hue with a low
            // alpha so glyphs read on top. Only painted when the input
            // has a non-empty range; the renderer ignores it otherwise.
            let selection_color = Some(Color {
                r: 0,
                g: 96,
                b: 192,
                a: 64,
            });
            let selection_cmd = selection.filter(|_| !*is_placeholder).map(|(s, e)| {
                crate::command::TextSelection {
                    start_byte: s,
                    end_byte: e,
                }
            });
            // Multi-line inputs reserve vertical padding the same as
            // single-line (4 / 4); they simply have multiple shaped
            // rows inside that. The renderer reads `caret_byte` and
            // walks cosmic-text's per-glyph byte ranges, so it places
            // the bar exactly where the editor model wants it.
            let _ = multiline; // currently used only at measure time
                               // Cascade parent opacity into every span's alpha so a
                               // dimmed editor stays internally consistent — the
                               // base text + caret + selection + per-span tints all
                               // fade together.
            let cascaded_spans: Vec<crate::command::TextSpan> = spans
                .iter()
                .map(|s| crate::command::TextSpan {
                    start_byte: s.start_byte,
                    end_byte: s.end_byte,
                    color: scale_color_alpha(s.color, parent_opacity),
                })
                .collect();
            // Scroll: shift the Text command's draw origin *out* of
            // the visible box (subtracting the offset) and wrap it
            // in a scissor sized to the input's text-area. Glyphs
            // past either edge get clipped; the caret + selection
            // inherit the same shift because they're computed
            // relative to the shaped run's origin.
            let scroll_active = *scroll_x != 0.0 || *scroll_y != 0.0;
            if scroll_active {
                out.push(RenderCommand::ScissorStart {
                    bounds: Rect {
                        x: text_left,
                        y: text_top,
                        width: text_width,
                        height: text_height,
                    },
                });
            }
            // Current-line highlight — paint a faint accent strip
            // behind the caret's line. Sits under the Text command
            // so glyphs read on top. Approximated using the same
            // `font_size * 1.2` line-height heuristic the runtime
            // uses everywhere; correct for monospace, close enough
            // for proportional.
            if *highlight_current_line && !*is_placeholder {
                if let Some(byte) = caret_byte {
                    let bytes = text.as_bytes();
                    let mut row: usize = 0;
                    let limit = (*byte).min(bytes.len());
                    for (i, b) in bytes.iter().enumerate() {
                        if i >= limit {
                            break;
                        }
                        if *b == b'\n' {
                            row += 1;
                        }
                    }
                    let row_h = props.font_size * 1.2;
                    let y_top = text_top - *scroll_y + row as f32 * row_h;
                    let hl_colour = scale_color_alpha(
                        Color {
                            r: 0,
                            g: 96,
                            b: 192,
                            a: 14,
                        },
                        parent_opacity,
                    );
                    out.push(RenderCommand::Rectangle {
                        bounds: Rect {
                            x: text_left,
                            y: y_top,
                            width: text_width,
                            height: row_h,
                        },
                        color: hl_colour,
                        radius: CornerRadius::default(),
                    });
                }
            }
            out.push(RenderCommand::Text {
                bounds: Rect {
                    x: text_left - *scroll_x,
                    y: text_top - *scroll_y,
                    width: text_width,
                    height: text_height,
                },
                content: text.clone(),
                color: scale_color_alpha(text_color, parent_opacity),
                font_size: props.font_size,
                caret: caret.map(|c| scale_color_alpha(c, parent_opacity)),
                caret_byte: *caret_byte,
                selection: selection_cmd,
                selection_color: selection_color
                    .map(|c| scale_color_alpha(c, parent_opacity))
                    .filter(|_| selection_cmd.is_some()),
                spans: cascaded_spans,
                underline: underline.map(|(s, e)| crate::command::TextSelection {
                    start_byte: s,
                    end_byte: e,
                }),
                // Match the caret accent so the preedit decoration
                // reads as "this is being authored" without inventing
                // a new token in the shell's palette.
                underline_color: underline.map(|_| {
                    scale_color_alpha(
                        Color {
                            r: 0,
                            g: 96,
                            b: 192,
                            a: 255,
                        },
                        parent_opacity,
                    )
                }),
                glyph_outlines: bracket_match.clone(),
                glyph_outline_color: if bracket_match.is_empty() {
                    None
                } else {
                    Some(scale_color_alpha(
                        Color {
                            r: 0,
                            g: 96,
                            b: 192,
                            a: 96,
                        },
                        parent_opacity,
                    ))
                },
            });
            if scroll_active {
                out.push(RenderCommand::ScissorEnd);
            }
        }
        Some(NodeContext::Spacer) | None => {}
    }
}

/// Multiply a colour's alpha channel by `multiplier ∈ [0, 1]`,
/// rounding to the nearest u8. Used by `emit_commands_with_opacity`
/// to cascade container opacity into every emitted Colour.
fn scale_color_alpha(c: Color, multiplier: f32) -> Color {
    if multiplier >= 1.0 - f32::EPSILON {
        return c;
    }
    let a = ((c.a as f32) * multiplier.clamp(0.0, 1.0))
        .round()
        .clamp(0.0, 255.0) as u8;
    Color { a, ..c }
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
mod tests;
