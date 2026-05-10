//! The `Component` trait — every renderable block implements this.
//!
//! [`Component::lower_ui`] is the single render path: it emits a
//! `prism_ui_runtime::layout::Node` consumed by both the shell renderer
//! and the relay's semantic-HTML SSR walker. The Slint source emission
//! path was deleted in the Phase 5 cutover follow-up.
//!
//! The trait is deliberately object-safe so `Arc<dyn Component>`s can
//! live in the [`crate::registry::ComponentRegistry`] and be dispatched
//! by `ComponentId` at walk time.

use prism_core::help::HelpEntry;
use thiserror::Error;

use crate::document::Node;
use crate::registry::FieldSpec;
use crate::signal::SignalDef;
use crate::style::StyleProperties;
use crate::variant::VariantAxis;
use prism_core::widget::ToolbarAction;

/// Stable identifier for a component *type* (e.g. `"card"`, `"button"`).
///
/// This is the key the [`crate::registry::ComponentRegistry`] uses when
/// looking up a renderer for a given node.
pub type ComponentId = String;

/// Errors a component can hit while rendering into a target backend.
/// Surfaces as 500s in `prism-relay` and as a developer-visible red
/// banner in Studio's builder pane.
#[derive(Debug, Error)]
pub enum RenderError {
    /// A child node references a `ComponentId` that isn't registered.
    #[error("unknown component: {0}")]
    UnknownComponent(ComponentId),

    /// Props failed validation or a component chose to bail.
    #[error("render failed: {0}")]
    Failed(String),
}

/// Render context for ad-hoc host-side callers that need typed access
/// to the shared design tokens without going through the document
/// walker. Currently a thin token carrier; expanded as needs arise.
pub struct RenderContext<'a> {
    pub tokens: &'a prism_core::design_tokens::DesignTokens,
}

/// The core component contract. Trait-objects of this type live in the
/// registry; each node in the builder document is dispatched through
/// whichever impl the registry hands back for its `ComponentId`.
pub trait Component: Send + Sync {
    fn id(&self) -> &ComponentId;

    /// Typed schema for the Studio property panel.
    fn schema(&self) -> Vec<FieldSpec>;

    fn help_entry(&self) -> Option<HelpEntry> {
        None
    }

    fn signals(&self) -> Vec<SignalDef> {
        crate::signal::common_signals()
    }

    fn variants(&self) -> Vec<VariantAxis> {
        vec![]
    }

    fn toolbar_actions(&self) -> Vec<ToolbarAction> {
        vec![]
    }

    /// Lower this component into a `prism_ui_runtime::layout::Node`.
    /// Default: a generic container — same shape `ui_runtime` always
    /// gave to unknown component ids. Built-ins that need bespoke
    /// runtime nodes (text, headings, spacers, …) override this; the
    /// shared helpers in [`crate::ui_lower`] keep the impls trivial.
    fn lower_ui(
        &self,
        ctx: &crate::ui_lower::LowerCtx<'_>,
        node: &Node,
        style: &StyleProperties,
    ) -> prism_ui_runtime::layout::Node {
        ctx.default_container(node, style)
    }
}
