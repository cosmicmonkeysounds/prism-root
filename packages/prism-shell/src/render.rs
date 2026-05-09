//! Per-frame `render_tree` — folds `ShellPropBindings::snapshot` into
//! the parsed `app.prism-ui` skeleton, then lowers the result through
//! `RegistryTagResolver` into a single runtime `Node` tree ready for
//! `Surface::set_tree`.
//!
//! See `docs/dev/clay-migration-plan.md` §17.

use std::collections::HashMap;
use std::sync::Arc;

use prism_builder::{document::BuilderDocument, ComponentRegistry};
use prism_ui_runtime::layout::Node;

use crate::props::{PropCtx, PropEmission, ShellPropBindings};

/// The pre-parsed `app.prism-ui` skeleton. Held as a `BuilderDocument`
/// so the per-frame `fill_compositions` step can clone-and-mutate
/// without re-parsing.
#[derive(Clone)]
pub struct Skeleton {
    pub doc: BuilderDocument,
}

impl Skeleton {
    /// Parse the on-disk `ui/app.prism-ui` skeleton once at boot.
    pub fn load() -> Result<Self, String> {
        let _src = include_str!("../ui/app.prism-ui");
        // TODO(§17): wire `prism_builder::source_parse::parse_prism_ui_document`
        // (the same call site `RegistryTagResolver::resolve` uses) and
        // construct the `BuilderDocument` from the parsed AST. The
        // parser already exists — see §15's
        // `canonical_app_prism_ui_skeleton_lowers_end_to_end`.
        Ok(Self {
            doc: BuilderDocument::default(),
        })
    }
}

/// Build the runtime `Node` tree for one frame.
///
/// Pipeline: `bindings.snapshot(ctx)` → `fill_compositions(skeleton, emissions)`
/// → `lower_document(doc, registry, resolver)` → `Node`.
pub fn render_tree(
    skeleton: &Skeleton,
    bindings: &ShellPropBindings,
    registry: Arc<ComponentRegistry>,
    ctx: &PropCtx,
) -> Node {
    let emissions = bindings.snapshot(ctx);
    let _doc = fill_compositions(skeleton, &emissions);
    let _ = registry;
    // TODO(§17): once `Skeleton::load` returns a real BuilderDocument,
    // call `prism_builder::ui_runtime::document_to_ui_tree_with_registry`
    // (with a `RegistryTagResolver` wrapping `registry`) and return the
    // resulting `Node`. Stubbed to an empty container until the parser
    // wiring lands.
    Node::Container {
        id: "root".into(),
        props: Default::default(),
        children: Vec::new(),
    }
}

/// Pure recursive walk: for every `<shell.foo>` element, merge
/// `emissions["shell.foo"].props` into its attributes and (if the
/// emission carries `children`) replace the element's children with
/// the host_children slot. One rule, every block.
pub fn fill_compositions(
    skeleton: &Skeleton,
    _emissions: &HashMap<&'static str, PropEmission>,
) -> BuilderDocument {
    // TODO(§17): walk skeleton.doc, on each registered tag look up the
    // emission, merge `props` into the node's prop bag, and (when
    // emission.children is non-empty) splice them into the node's
    // children slot via the same `host_children` mechanism the
    // resolver uses.
    skeleton.doc.clone()
}
