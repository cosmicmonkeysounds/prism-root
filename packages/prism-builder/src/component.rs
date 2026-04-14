//! The `Component` trait — every renderable block implements this.
//!
//! The builder doesn't know how a component paints; it only knows how
//! to serialize it, how to list its props for the property panel, and
//! how to hand it a render context when it's time to emit Clay draw
//! commands. The `render` function returns a placeholder for now; once
//! the Clay binding lands it returns `ClayElement`.

use serde_json::Value;

/// Stable identifier for a component *type* (e.g. `"card"`, `"button"`).
///
/// This is the key the [`crate::registry::ComponentRegistry`] uses when
/// looking up a renderer for a given node.
pub type ComponentId = String;

/// What a component needs to paint itself. Placeholder for the real
/// render context that will carry the Clay arena, design tokens,
/// selection state, etc. once the renderer is wired.
pub struct RenderContext<'a> {
    pub tokens: &'a prism_core::design_tokens::DesignTokens,
}

/// The core contract. Trait-objects of this type live in the
/// registry; each node in the builder document is dispatched through
/// whichever impl the registry hands back for its `ComponentId`.
pub trait Component: Send + Sync {
    fn id(&self) -> &ComponentId;

    /// Schema for the property panel. JSON today so the shape can
    /// evolve without churning every call site; a typed field-factory
    /// layer will land on top of this in Phase 3.
    fn schema(&self) -> Value;

    /// Paint the component. Stub until the Clay binding is wired — the
    /// renderer just returns the serialized props so tests can assert
    /// round-trips.
    fn render(&self, ctx: &RenderContext<'_>, props: &Value) -> Value {
        let _ = ctx;
        props.clone()
    }
}
