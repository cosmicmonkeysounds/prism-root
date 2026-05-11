//! Builder→runtime lowering: `Component::lower_ui` plumbing + the
//! shared helpers every built-in block reuses.
//!
//! This is the seam called out in the Clay/Taffy migration plan
//! (`docs/dev/clay-migration-plan.md` §3, §6). The old `ui_runtime`
//! translator dispatched on `node.component` with a hard-coded string
//! match — every new block had to teach that match about itself.
//! Now each [`crate::component::Component`] knows how to lower itself
//! to a `prism_ui_runtime::layout::Node`, and the translator is just
//! "look the component up in the registry, call `lower_ui`".
//!
//! The helpers below are the *single source of truth* for the
//! translation primitives blocks reuse:
//!
//! - [`LowerCtx`] — passed to every `lower_ui` impl. Carries the
//!   inherited style cascade and the registry, exposes
//!   [`LowerCtx::lower_children`] for recursion, and
//!   [`LowerCtx::default_container`] as the generic fallback.
//! - [`container_props_from`] — turns a node's `FlowProps` + cascaded
//!   `StyleProperties` into runtime `ContainerProps`. Containers
//!   share this; bespoke blocks (cards, columns) layer on top.
//! - [`parse_color`] — `#rgb` / `#rrggbb` / `#rrggbbaa`. The cascade
//!   resolves to strings; this is where they become runtime `Color`.
//! - [`text_node`] / [`spacer_node`] — convenience constructors so
//!   `TextBlock` / `SpacerBlock` don't reimplement the same shape.
//!
//! Blocks that don't override `lower_ui` get the default container
//! lowering automatically — same behaviour the legacy `ui_runtime`
//! translator gave for unknown component ids.

use std::collections::HashMap;
use std::sync::Arc;

use prism_ui_runtime::command::{Color, CornerRadius};
use prism_ui_runtime::interpret::TagEmission;
use prism_ui_runtime::layout::{
    ContainerProps, Direction, HoverOverrides, Node as UiNode, Padding, Semantic, Sizing, TextProps,
};

use crate::document::Node;
use crate::layout::{Dimension, FlexDirection, FlowProps, LayoutMode};
use crate::registry::ComponentRegistry;
use crate::style::{resolve_cascade, StyleProperties};

/// Context threaded through `Component::lower_ui` impls during the
/// `BuilderDocument` → `prism_ui_runtime::layout::Node` walk.
///
/// `parent_style` is *the cascade output for the node currently being
/// lowered* — i.e. for a block authoring its own children, calling
/// [`Self::lower_children`] cascades correctly without the block
/// knowing anything about the cascade.
pub struct LowerCtx<'a> {
    registry: Option<&'a ComponentRegistry>,
    parent_style: &'a StyleProperties,
    /// Pre-lowered children supplied by a host upstream of `lower_ui`
    /// — currently the [`crate::ui_resolver::RegistryTagResolver`]
    /// path, which lowers an element's AST children through the
    /// runtime before delegating to the registered block. Composition-
    /// style blocks (`shell.app-window`) consume this slice in
    /// preference to walking `node.children`. Plain blocks — the 12/13
    /// chrome primitives whose layout comes from props — never read
    /// it; the field is `None` on every other path. Single-seam DI:
    /// no new abstraction, no parallel context type, the existing
    /// `LowerCtx` simply carries a sparse extra slot.
    host_children: Option<&'a [UiNode]>,
    /// Tag-keyed binding emissions snapshot, threaded through from
    /// [`prism_ui_runtime::interpret::LowerScope::with_tag_emissions`].
    /// When [`Self::lower_as`] synthesises a routed content tag (the
    /// dock-panel `panel-id` path is the canonical caller) it merges
    /// the caller's props with this map's entry and threads the
    /// recorded children through as `host_children` — without this
    /// the synthesised tag is rendered with empty props and zero
    /// children, ignoring whatever the binding registered emit.
    /// `Arc<HashMap<...>>` so the field can outlive the originating
    /// `LowerScope` value (the resolver consumes scope by reference
    /// but stores an Arc clone here for child-scope propagation).
    tag_emissions: Option<Arc<HashMap<String, TagEmission>>>,
}

impl<'a> LowerCtx<'a> {
    /// Build a context anchored at a specific parent cascade. Callers
    /// that don't have a meaningful parent style (i.e. the root of a
    /// document) pass a borrow to a `StyleProperties::default()`.
    pub fn new(registry: Option<&'a ComponentRegistry>, parent_style: &'a StyleProperties) -> Self {
        Self {
            registry,
            parent_style,
            host_children: None,
            tag_emissions: None,
        }
    }

    /// Builder-style installer for host-supplied pre-lowered children.
    /// Composes with the existing [`Self::new`] surface — child scopes
    /// (control-flow forks, recursive `lower` calls) deliberately do
    /// **not** inherit this slot, since the children belong to the one
    /// block the resolver is delegating to.
    pub fn with_host_children(mut self, children: &'a [UiNode]) -> Self {
        self.host_children = Some(children);
        self
    }

    /// Install the tag-keyed emissions map snapshot. The resolver hands
    /// this through from
    /// [`prism_ui_runtime::interpret::LowerScope::tag_emissions_arc`];
    /// it propagates into every child `LowerCtx` constructed inside
    /// `lower_as` / `lower` so routed content several layers deep
    /// still inherits the live binding data.
    pub fn with_tag_emissions(mut self, emissions: Arc<HashMap<String, TagEmission>>) -> Self {
        self.tag_emissions = Some(emissions);
        self
    }

    /// Look up the binding emission recorded under `tag`, if any.
    /// Returns `None` either when no map was installed (headless
    /// tests, no-DI render paths) or when the tag was never registered
    /// — the caller falls through to its synthesised default.
    pub fn tag_emission(&self, tag: &str) -> Option<&TagEmission> {
        self.tag_emissions.as_ref().and_then(|m| m.get(tag))
    }

    /// Pre-lowered children, if a host upstream of `lower_ui`
    /// supplied any. Composition blocks read this in preference to
    /// walking `node.children`:
    ///
    /// ```ignore
    /// let kids = ctx
    ///     .host_children()
    ///     .map(|s| s.to_vec())
    ///     .unwrap_or_else(|| ctx.lower_children(&node.children));
    /// ```
    pub fn host_children(&self) -> Option<&[UiNode]> {
        self.host_children
    }

    /// Lower a single node. The cascade is resolved internally and a
    /// fresh child-scope `LowerCtx` is handed to whichever
    /// `Component::lower_ui` impl owns this node's component id.
    /// Unknown ids fall back to [`Self::default_container`].
    pub fn lower(&self, node: &Node) -> UiNode {
        let style = resolve_cascade(self.parent_style, &StyleProperties::default(), &node.style);
        // Note: host_children is intentionally not propagated — it
        // belongs to the block currently being resolved, not its
        // recursive sub-children. tag_emissions *is* propagated:
        // it's a snapshot keyed by tag, valid for the entire pass.
        let child = LowerCtx {
            registry: self.registry,
            parent_style: &style,
            host_children: None,
            tag_emissions: self.tag_emissions.clone(),
        };
        if let Some(reg) = self.registry {
            if let Some(comp) = reg.get(&node.component) {
                return comp.lower_ui(&child, node, &style);
            }
        }
        child.default_container(node, &style)
    }

    /// Recurse into a slice of children with this context's cascade
    /// as their parent. Blocks that wrap their children call this.
    pub fn lower_children(&self, children: &[Node]) -> Vec<UiNode> {
        children.iter().map(|c| self.lower(c)).collect()
    }

    /// Generic container lowering — what `Component::lower_ui` falls
    /// back to when a block doesn't override the method. Mirrors the
    /// pre-migration `translate_container` behaviour exactly.
    pub fn default_container(&self, node: &Node, style: &StyleProperties) -> UiNode {
        self.container_with(node, style, |_| {})
    }

    /// Declarative container lowering. Builds the same `UiNode::Container`
    /// [`Self::default_container`] would, threading cascade + flow props
    /// through [`container_props_from`], then hands the resulting
    /// `ContainerProps` to `customize` so a block can tweak the few
    /// fields it actually owns (direction, gap, padding, background…)
    /// without restating the whole construction.
    ///
    /// This is the seam every "I'm a container with one knob different"
    /// block uses — `ColumnsBlock` flips direction to `Row`,
    /// `ListBlock` overrides `gap`, `ContainerBlock` adds padding and
    /// border-derived background, etc. The cascade, sizing,
    /// colour-parsing, and child recursion live exactly once (here +
    /// in `container_props_from`); blocks contribute only their
    /// difference.
    pub fn container_with(
        &self,
        node: &Node,
        style: &StyleProperties,
        customize: impl FnOnce(&mut ContainerProps),
    ) -> UiNode {
        let flow = match &node.layout_mode {
            LayoutMode::Flow(f) | LayoutMode::Relative(f) => Some(f),
            _ => None,
        };
        let mut props = container_props_from(flow, style);
        customize(&mut props);
        UiNode::Container {
            id: node.id.clone(),
            props,
            children: self.lower_children(&node.children),
        }
    }

    /// Like [`Self::container_with`] but for blocks that synthesise
    /// children (a button rendering its own label, a code block
    /// rendering pre-formatted text) rather than walking
    /// `node.children`. Saves the per-block "build a container with
    /// these children and these prop tweaks" boilerplate.
    pub fn synthetic_container(
        &self,
        node: &Node,
        style: &StyleProperties,
        children: Vec<UiNode>,
        customize: impl FnOnce(&mut ContainerProps),
    ) -> UiNode {
        let flow = match &node.layout_mode {
            LayoutMode::Flow(f) | LayoutMode::Relative(f) => Some(f),
            _ => None,
        };
        let mut props = container_props_from(flow, style);
        customize(&mut props);
        UiNode::Container {
            id: node.id.clone(),
            props,
            children,
        }
    }

    /// Cascade output the *current scope* sees as its inherited
    /// style. Useful for blocks that need to peek at parent values
    /// without owning the cascade machinery.
    pub fn parent_style(&self) -> &StyleProperties {
        self.parent_style
    }

    /// Resolve and lower an *embedded* block by its registered component
    /// id, synthesising a derived `Node` from a JSON props value. The
    /// dispatch goes through whichever [`ComponentRegistry`] is on this
    /// `LowerCtx`, so a host that registers an alternative
    /// `shell.menu-bar-row` impl transparently overrides the default —
    /// no `lower_ui` call site has to import a concrete block type.
    ///
    /// Returns `None` when the ctx has no registry, or the id is
    /// unregistered. Composition-style blocks (`AppWindow`, future
    /// `TabPanel`, …) call this for each chrome region they host and
    /// fall back to a placeholder when None — the structural shape
    /// stays correct under headless / no-registry test contexts, and
    /// production paths (resolver-driven, registry-attached) get full
    /// rendering with zero block-type knowledge baked into the host.
    ///
    /// Smart pattern: the *only* way to embed one registered block
    /// inside another's lowering. Eliminates the "import the impl,
    /// instantiate it manually, build a fresh `LowerCtx`" duplication
    /// that AppWindow originally carried — that pattern bypassed the
    /// registry and silently ignored host-side overrides.
    pub fn lower_as(
        &self,
        component_id: &str,
        derived_id: impl Into<String>,
        props: serde_json::Value,
    ) -> Option<UiNode> {
        let reg = self.registry?;
        let comp = reg.get(component_id)?;
        // Pull the host's emission for this tag, if any. The caller's
        // own props win (so dock-panel can still pass `{ panel-id: ...
        // }` and have it stick); every key the caller didn't set falls
        // back to the binding's. Children are wholesale — no merge.
        let emission = self.tag_emission(component_id);
        let merged_props = merge_with_emission_props(props, emission.map(|e| &e.props));
        let derived = Node {
            id: derived_id.into(),
            component: component_id.into(),
            props: merged_props,
            children: Vec::new(),
            layout_mode: LayoutMode::default(),
            transform: prism_core::foundation::spatial::Transform2D::default(),
            modifiers: Vec::new(),
            style: StyleProperties::default(),
        };
        let style = resolve_cascade(
            self.parent_style,
            &StyleProperties::default(),
            &derived.style,
        );
        // Only thread an emission's children through as host_children
        // when it actually carries any. The shell registers an
        // auto-stub `{}`-props binding for every `SHELL_BUILTINS` row
        // that doesn't get a live binding (e.g. `shell.dock-panel`,
        // `shell.menu-item`, every per-row leaf). If we propagated
        // those empty children slices, callers that compose the tag
        // via `lower_as` (the dock-workspace routing path) would see
        // `ctx.host_children() == Some(&[])` and short-circuit their
        // fallback recursion. Treating "empty" as "no override"
        // preserves the original routing semantics.
        let host_children = emission
            .map(|e| e.children.as_slice())
            .filter(|s| !s.is_empty());
        let child = LowerCtx {
            registry: self.registry,
            parent_style: &style,
            host_children,
            tag_emissions: self.tag_emissions.clone(),
        };
        Some(comp.lower_ui(&child, &derived, &style))
    }
}

/// Overlay caller-provided props on top of a binding emission. The
/// caller's keys win (so explicit `panel-id="builder"` is preserved
/// even when the `shell.dock-panel` binding also emits one) and any
/// emission key absent from the caller drops in. Returns a JSON
/// `Object` even when both sides are empty so downstream code that
/// expects an object shape (every chrome block does) keeps working.
fn merge_with_emission_props(
    caller: serde_json::Value,
    emission: Option<&serde_json::Value>,
) -> serde_json::Value {
    let mut map = match caller {
        serde_json::Value::Object(m) => m,
        // Caller passed a non-object (a leaf string, an array): treat
        // it as "no props" and just hand back the emission's object.
        // Same shape every chrome block reads through `node.props.get(...)`.
        _ => serde_json::Map::new(),
    };
    if let Some(serde_json::Value::Object(em)) = emission {
        for (k, v) in em {
            if !map.contains_key(k) {
                map.insert(k.clone(), v.clone());
            }
        }
    }
    serde_json::Value::Object(map)
}

/// Build a `UiNode::Container` *without* going through a builder
/// `Node`. Used by composite blocks that synthesise nested sub-trees
/// (table headers, tab strips, accordion bars) where there's no
/// `Node` to drive cascade resolution from.
///
/// Defaults to a zero-padded, no-background, fit-sized container —
/// the closure is the *only* way fields move off the default. This
/// keeps every "build a styled box with these children" call
/// boilerplate-free at the call site.
pub fn bare_container(
    id: impl Into<String>,
    children: Vec<UiNode>,
    customize: impl FnOnce(&mut ContainerProps),
) -> UiNode {
    let mut props = ContainerProps::default();
    customize(&mut props);
    UiNode::Container {
        id: id.into(),
        props,
        children,
    }
}

/// Attach a [`Semantic`] hint to whichever variant carries one. Used
/// by blocks to declare SSR markup (`<h1>`, `<section>`, alt text)
/// alongside layout vocabulary, in the same `lower_ui` impl, with no
/// per-block walker. `Spacer` ignores the hint (no semantic field).
pub fn with_semantic(node: UiNode, semantic: Semantic) -> UiNode {
    match node {
        UiNode::Container {
            id,
            mut props,
            children,
        } => {
            props.semantic = semantic;
            UiNode::Container {
                id,
                props,
                children,
            }
        }
        UiNode::Text {
            id,
            content,
            mut props,
        } => {
            props.semantic = semantic;
            UiNode::Text { id, content, props }
        }
        UiNode::Image {
            id,
            source,
            width,
            height,
            radius,
            tint,
            ..
        } => UiNode::Image {
            id,
            source,
            width,
            height,
            radius,
            tint,
            semantic,
        },
        UiNode::TextInput {
            id,
            value,
            placeholder,
            props,
            width,
            height,
            radius,
            ..
        } => UiNode::TextInput {
            id,
            value,
            placeholder,
            props,
            width,
            height,
            radius,
            semantic,
        },
        UiNode::Spacer { .. } => node,
    }
}

/// Read a string-valued prop. Returns `""` when the key is missing
/// or the value isn't a string. Most chrome primitives reach for the
/// "give me the icon path / label / kind, defaulting to empty when
/// unset" pattern several times per `lower_ui` impl — centralising
/// the `node.props.get(k).and_then(|v| v.as_str()).unwrap_or("")`
/// dance keeps each call site to one line.
pub fn prop_str<'a>(node: &'a Node, key: &str) -> &'a str {
    node.props.get(key).and_then(|v| v.as_str()).unwrap_or("")
}

/// Owned variant of [`prop_str`] for the (very common) case where the
/// extracted prop is immediately interpolated into a child id /
/// `text_node` content / `image_node` source.
pub fn prop_string(node: &Node, key: &str) -> String {
    prop_str(node, key).to_string()
}

/// Read a bool-valued prop with a fallback. Mirrors the
/// `matches!(node.props.get(k), Some(Value::Bool(true)))` pattern
/// every chrome primitive open-coded before this helper landed.
pub fn prop_bool(node: &Node, key: &str, default: bool) -> bool {
    node.props
        .get(key)
        .and_then(|v| v.as_bool())
        .unwrap_or(default)
}

/// Build a [`UiNode::Text`] with the cascade colour overridden by an
/// explicit per-block colour string. The "clone the cascade and stamp
/// `color`" dance shows up at every chrome primitive that paints a
/// label in a non-cascade tint (section-header label/badge, toast
/// title/body, future docs/app-card text). Centralising it keeps
/// every call site to one line and frees blocks from owning a tiny
/// private helper for the same shape.
///
/// The colour string follows the same vocabulary as [`parse_color`]
/// (`#rgb` / `#rrggbb` / `#rrggbbaa`); unparseable values silently
/// fall through to the cascade (same shape `text_node` itself uses
/// when `style.color` doesn't parse).
pub fn colored_text_node(
    node_id: String,
    content: String,
    style: &StyleProperties,
    default_size: f32,
    color: &str,
) -> UiNode {
    let mut scoped = style.clone();
    scoped.color = Some(color.into());
    text_node(node_id, content, &scoped, default_size)
}

/// One-line constructor for the most common interactive-primitive
/// hover shape: "swap the background only". Returns `None` when the
/// colour string fails to parse so the caller can `props.hover = ...`
/// unconditionally without a `parse_color`/`HoverOverrides` two-liner
/// at every call site. The full `HoverOverrides` struct stays
/// available for primitives that animate radius / future fields too.
pub fn hover_bg(color: &str) -> Option<HoverOverrides> {
    parse_color(color).map(|c| HoverOverrides {
        background: Some(c),
        radius: None,
    })
}

/// Convenience: equal corner radius on all four corners. Most blocks
/// want this; the long-form struct literal is noise.
pub fn uniform_radius(r: f32) -> CornerRadius {
    CornerRadius {
        tl: r,
        tr: r,
        br: r,
        bl: r,
    }
}

/// Shared "this surface responds to a pointer" tint. Every clickable
/// chrome surface uses the same intensity so hover reads identically
/// across the whole window — palette rows, inspector rows, canvas-doc
/// nodes, field-editor rows, menu pills. Authors who need a stronger
/// or softer tint can still pass any colour to [`hover_bg`] directly.
pub const POINTER_HOVER_TINT: &str = "#1a0060c0";

/// One-liner for the "clickable chrome surface" recipe — bundles
/// the three things every routable container needs into a single
/// call: a hover-bg tint, `data-role`, and (optional) `data-target-id`.
///
/// ```ignore
/// bare_container(node.id.clone(), kids, |p| {
///     p.padding = Padding::all(8.0);
///     p.radius = uniform_radius(4.0);
///     pointer_routing(p, "my-row", target_id);
/// });
/// ```
///
/// The caller still owns the rest of the `Semantic` shape (`tag`,
/// `aria-*`, custom `data-*`) — `pointer_routing` only appends the
/// two routing attrs and the hover tint, so existing builder calls
/// compose cleanly. The function deliberately takes `&mut props` so
/// it threads naturally through `bare_container`'s closure shape.
pub fn pointer_routing(props: &mut ContainerProps, role: &'static str, target_id: &str) {
    props.hover = hover_bg(POINTER_HOVER_TINT);
    let semantic = std::mem::take(&mut props.semantic);
    let mut s = semantic.with_attr("data-role", role);
    if !target_id.is_empty() {
        s = s.with_attr("data-target-id", target_id.to_string());
    }
    props.semantic = s;
}

/// Build runtime `ContainerProps` from a node's `FlowProps` + cascade.
/// Single source of truth — every container-shaped block routes here.
pub fn container_props_from(flow: Option<&FlowProps>, style: &StyleProperties) -> ContainerProps {
    let direction = flow
        .map(|f| match f.flex_direction {
            FlexDirection::Row | FlexDirection::RowReverse => Direction::Row,
            FlexDirection::Column | FlexDirection::ColumnReverse => Direction::Column,
        })
        .unwrap_or_default();

    let gap = flow.map(|f| f.gap).unwrap_or(0.0);

    let padding = flow
        .map(|f| Padding {
            left: f.padding.left,
            right: f.padding.right,
            top: f.padding.top,
            bottom: f.padding.bottom,
        })
        .unwrap_or_default();

    let width = flow
        .map(|f| sizing_from_dimension(f.width, f.flex_grow))
        .unwrap_or_default();
    let height = flow
        .map(|f| sizing_from_dimension(f.height, f.flex_grow))
        .unwrap_or_default();

    let background = style.background.as_deref().and_then(parse_color);
    let radius = style
        .border_radius
        .map(|r| CornerRadius {
            tl: r,
            tr: r,
            br: r,
            bl: r,
        })
        .unwrap_or_default();

    ContainerProps {
        direction,
        gap,
        padding,
        width,
        height,
        background,
        radius,
        ..Default::default()
    }
}

/// Map a builder `Dimension` + `flex_grow` to a runtime `Sizing`.
pub fn sizing_from_dimension(dim: Dimension, flex_grow: f32) -> Sizing {
    match dim {
        Dimension::Px { value } => Sizing::Fixed(value),
        Dimension::Auto if flex_grow > 0.0 => Sizing::Grow,
        Dimension::Percent { value } => Sizing::Percent((value / 100.0).clamp(0.0, 1.0)),
        Dimension::Auto => Sizing::Fit,
    }
}

/// Construct a `UiNode::Text` with cascade-resolved size/color and a
/// per-block default font size (paragraph / heading / code differ).
pub fn text_node(
    node_id: String,
    content: String,
    style: &StyleProperties,
    default_size: f32,
) -> UiNode {
    let font_size = style.font_size.unwrap_or(default_size);
    let color = style
        .color
        .as_deref()
        .and_then(parse_color)
        .unwrap_or(DEFAULT_TEXT_COLOR);
    UiNode::Text {
        id: node_id,
        content,
        props: TextProps {
            font_size,
            color,
            ..Default::default()
        },
    }
}

/// Construct a `UiNode::Spacer`. Trivial wrapper, kept here so all
/// node constructors live in one place.
pub fn spacer_node(node_id: String, width: f32, height: f32) -> UiNode {
    UiNode::Spacer {
        id: node_id,
        width,
        height,
    }
}

/// Construct a `UiNode::TextInput` — the editable single-line input
/// leaf. Same builder shape as [`text_node`] / [`image_node`]: the
/// caller hands over the few fields that vary, and cascade-resolved
/// font_size / colour are pulled from `style`. `value` is what the
/// user has typed (often empty); `placeholder` paints when the value
/// is empty. Width / height drive the runtime `Sizing` policy — most
/// inputs want `Sizing::Grow` along the parent's main axis.
pub fn text_input_node(
    node_id: String,
    value: String,
    placeholder: String,
    style: &StyleProperties,
    width: Sizing,
    height: Sizing,
    default_size: f32,
) -> UiNode {
    let font_size = style.font_size.unwrap_or(default_size);
    let color = style
        .color
        .as_deref()
        .and_then(parse_color)
        .unwrap_or(DEFAULT_TEXT_COLOR);
    UiNode::TextInput {
        id: node_id,
        value,
        placeholder,
        props: TextProps {
            font_size,
            color,
            ..Default::default()
        },
        width,
        height,
        radius: style.border_radius.map(uniform_radius).unwrap_or_default(),
        semantic: Semantic::default(),
    }
}

/// Construct a `UiNode::Image`. Width/height default to `Grow` so an
/// image inside a sized container fills its slot.
pub fn image_node(
    node_id: String,
    source: String,
    style: &StyleProperties,
    width: Sizing,
    height: Sizing,
) -> UiNode {
    let radius = style.border_radius.map(uniform_radius).unwrap_or_default();
    UiNode::Image {
        id: node_id,
        source,
        width,
        height,
        radius,
        tint: None,
        semantic: prism_ui_runtime::layout::Semantic::default(),
    }
}

/// Construct a `UiNode::Image` with a colour tint applied. Tint
/// instructs the renderer to mask-paint the colour through the image
/// (canonical icon-tint pattern). Same default sizing rules as
/// [`image_node`] — `Grow`/`Grow` so an icon inside a sized container
/// fills its slot.
pub fn tinted_image_node(
    node_id: String,
    source: String,
    style: &StyleProperties,
    width: Sizing,
    height: Sizing,
    tint: prism_ui_runtime::command::Color,
) -> UiNode {
    let radius = style.border_radius.map(uniform_radius).unwrap_or_default();
    UiNode::Image {
        id: node_id,
        source,
        width,
        height,
        radius,
        tint: Some(tint),
        semantic: prism_ui_runtime::layout::Semantic::default(),
    }
}

const DEFAULT_TEXT_COLOR: Color = Color {
    r: 20,
    g: 20,
    b: 20,
    a: 255,
};

/// Tiny CSS-color parser — `#rgb`, `#rrggbb`, `#rrggbbaa`. Anything
/// else returns `None` and the caller falls back to a default. Richer
/// parsing (named colours, `rgb(...)`, `oklch(...)`) lands with the
/// design-tokens cascade wiring.
pub fn parse_color(s: &str) -> Option<Color> {
    let s = s.trim();
    let hex = s.strip_prefix('#')?;
    let bytes = match hex.len() {
        3 => {
            let r = u8::from_str_radix(&hex[0..1], 16).ok()?;
            let g = u8::from_str_radix(&hex[1..2], 16).ok()?;
            let b = u8::from_str_radix(&hex[2..3], 16).ok()?;
            [r * 17, g * 17, b * 17, 255]
        }
        6 => {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            [r, g, b, 255]
        }
        8 => {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            let a = u8::from_str_radix(&hex[6..8], 16).ok()?;
            [r, g, b, a]
        }
        _ => return None,
    };
    Some(Color {
        r: bytes[0],
        g: bytes[1],
        b: bytes[2],
        a: bytes[3],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lower_as_resolves_through_registry_when_attached() {
        use crate::block::{register_block, Block};
        use crate::registry::{ComponentRegistry, FieldSpec};
        use crate::ComponentId;
        use prism_ui_runtime::layout::{ContainerProps, Sizing};
        use std::sync::Arc;

        struct Tag {
            id: ComponentId,
        }
        impl Block for Tag {
            fn id(&self) -> &ComponentId {
                &self.id
            }
            fn schema(&self) -> Vec<FieldSpec> {
                vec![]
            }
            fn lower_ui(&self, _: &LowerCtx<'_>, node: &Node, _: &StyleProperties) -> UiNode {
                UiNode::Container {
                    id: node.id.clone(),
                    props: ContainerProps {
                        width: Sizing::Fixed(99.0),
                        ..Default::default()
                    },
                    children: vec![],
                }
            }
        }

        let mut reg = ComponentRegistry::new();
        register_block(
            &mut reg,
            Arc::new(Tag {
                id: "demo.tag".into(),
            }),
        )
        .unwrap();
        let cascade = StyleProperties::default();
        let ctx = LowerCtx::new(Some(&reg), &cascade);

        let out = ctx
            .lower_as("demo.tag", "derived", serde_json::json!({}))
            .expect("registered tag resolves");
        let UiNode::Container { id, props, .. } = out else {
            panic!()
        };
        assert_eq!(id, "derived");
        assert_eq!(props.width, Sizing::Fixed(99.0));
    }

    #[test]
    fn lower_as_returns_none_when_no_registry() {
        let cascade = StyleProperties::default();
        let ctx = LowerCtx::new(None, &cascade);
        assert!(ctx
            .lower_as("anything", "x", serde_json::json!({}))
            .is_none());
    }

    #[test]
    fn lower_as_returns_none_when_id_unregistered() {
        use crate::registry::ComponentRegistry;
        let reg = ComponentRegistry::new();
        let cascade = StyleProperties::default();
        let ctx = LowerCtx::new(Some(&reg), &cascade);
        assert!(ctx
            .lower_as("never.registered", "x", serde_json::json!({}))
            .is_none());
    }

    #[test]
    fn hover_bg_returns_some_for_valid_color() {
        let h = hover_bg("#1f000000").expect("valid color");
        assert!(h.background.is_some());
        assert!(h.radius.is_none());
    }

    #[test]
    fn hover_bg_returns_none_for_invalid_color() {
        assert!(hover_bg("not-a-color").is_none());
    }

    #[test]
    fn prop_helpers_extract_with_sensible_defaults() {
        use crate::layout::LayoutMode;
        use prism_core::foundation::spatial::Transform2D;
        use serde_json::json;
        let node = Node {
            id: "n".into(),
            component: "x".into(),
            props: json!({ "label": "Hi", "selected": true }),
            children: vec![],
            layout_mode: LayoutMode::default(),
            transform: Transform2D::default(),
            modifiers: vec![],
            style: StyleProperties::default(),
        };
        assert_eq!(prop_str(&node, "label"), "Hi");
        assert_eq!(prop_str(&node, "missing"), "");
        assert_eq!(prop_string(&node, "label"), "Hi");
        assert!(prop_bool(&node, "selected", false));
        assert!(!prop_bool(&node, "missing", false));
        assert!(prop_bool(&node, "missing", true));
    }

    #[test]
    fn colored_text_node_overrides_cascade_color() {
        let cascade = StyleProperties {
            color: Some("#000000".into()),
            ..Default::default()
        };
        let UiNode::Text { props, .. } =
            colored_text_node("t".into(), "hello".into(), &cascade, 14.0, "#ff0000")
        else {
            panic!("expected text")
        };
        assert_eq!(props.color.r, 0xff);
        assert_eq!(props.color.g, 0x00);
    }

    #[test]
    fn hover_bg_round_trips_alpha() {
        // #RRGGBBAA — last byte is alpha.
        let h = hover_bg("#0000001f").unwrap();
        let c = h.background.unwrap();
        assert_eq!(c.a, 0x1f);
    }
}
