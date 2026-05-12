//! Wave 11.2 of `docs/dev/composable-builder-plan.md` — load
//! `.prism-ui` source files as registered shell `Block`s.
//!
//! ## Smart-pattern shape
//!
//! Each migrated shell component is a single declarative row in
//! [`SHELL_PRISM_UI_COMPONENTS`]:
//!
//! ```ignore
//! pub static SHELL_PRISM_UI_COMPONENTS: &[PrismUiSpec] = &[
//!     PrismUiSpec::new(
//!         "shell.toolbar-separator",
//!         include_str!("../../ui/components/toolbar-separator.prism-ui"),
//!     ),
//!     // …
//! ];
//! ```
//!
//! [`register_prism_ui_components`] parses each source at boot, wraps
//! it in a [`PrismUiBlock`] that implements [`Block`], and fans the
//! batch through the same [`register_specs`]-style path the native
//! `SHELL_BUILTINS` use. Adding a migration is **one .prism-ui file +
//! one row** — no per-component struct, no per-component test seam
//! (the cross-cutting tests in this module exercise the loader's
//! behaviour against the table).
//!
//! ## Composition seam
//!
//! Each `PrismUiBlock` holds an `Arc<OnceLock<Arc<dyn TagResolver>>>`
//! shared across the whole batch. After all native + DSL blocks
//! register, [`finalize_prism_ui_resolver`] populates the cell with a
//! fresh resolver built over the live registry. At `lower_ui` time,
//! the block reads the resolver, builds a [`LowerScope`] seeded with
//! every `node.prop` as a binding, and runs the parsed AST through
//! [`lower_document_with_scope`]. Composing primitives, hover
//! modifiers, and per-key prop interpolation (`{title}`) all flow
//! through the existing runtime — the loader adds no new vocabulary.

use std::sync::{Arc, OnceLock};

use prism_builder::{
    component::ComponentId,
    document::Node,
    registry::{FieldSpec, RegistryError},
    signal::{common_signals, SignalDef},
    style::StyleProperties,
    ui_lower::LowerCtx,
    ui_resolver::RegistryTagResolver,
    Block,
};
use prism_core::language::prism_ui::{parse, Document as AstDocument};
use prism_ui_runtime::interpret::{lower_document_with_scope, LowerScope, TagResolver};
use prism_ui_runtime::layout::Node as UiNode;

use crate::components::registry::ShellComponentRegistry;

/// Declarative spec for a `.prism-ui`-authored shell component.
/// Mirrors the shape of [`prism_builder::BlockSpec`] but trades the
/// imperative `lower: LowerFn` field for a `source` string that the
/// runtime interprets at render time.
///
/// Authored as a `pub const` in the migrated component's call site
/// or — for the bulk Tier-1 migrations — directly in the
/// [`SHELL_PRISM_UI_COMPONENTS`] table.
pub struct PrismUiSpec {
    pub id: &'static str,
    pub source: &'static str,
    pub schema: fn() -> Vec<FieldSpec>,
    pub signals: fn() -> Vec<SignalDef>,
}

impl PrismUiSpec {
    pub const fn new(id: &'static str, source: &'static str) -> Self {
        Self {
            id,
            source,
            schema: empty_schema,
            signals: common_signals,
        }
    }

    pub const fn schema(mut self, f: fn() -> Vec<FieldSpec>) -> Self {
        self.schema = f;
        self
    }

    pub const fn signals(mut self, f: fn() -> Vec<SignalDef>) -> Self {
        self.signals = f;
        self
    }
}

fn empty_schema() -> Vec<FieldSpec> {
    vec![]
}

/// Shared late-init resolver cell. Populated by
/// [`finalize_prism_ui_resolver`] after every native + DSL block
/// registers, so a DSL block's `lower_ui` can dispatch
/// composed `<shell.*>` / `<prism.*>` tags through the live
/// registry. Using `OnceLock` keeps the contract "set exactly once,
/// no interior mutability after that" — any future hot-reload of
/// the registry rebuilds the entire shell, not the cell.
pub type SharedResolver = Arc<OnceLock<Arc<dyn TagResolver>>>;

/// Runtime [`Block`] backing a `.prism-ui`-authored shell component.
/// Holds the parsed AST plus the shared resolver cell; `lower_ui`
/// snapshots `node.props` into a [`LowerScope`] and walks the AST.
pub struct PrismUiBlock {
    id: ComponentId,
    schema: fn() -> Vec<FieldSpec>,
    signals: fn() -> Vec<SignalDef>,
    parsed: AstDocument,
    resolver: SharedResolver,
}

impl PrismUiBlock {
    pub fn id(&self) -> &str {
        &self.id
    }
}

impl Block for PrismUiBlock {
    fn id(&self) -> &ComponentId {
        &self.id
    }

    fn schema(&self) -> Vec<FieldSpec> {
        (self.schema)()
    }

    fn signals(&self) -> Vec<SignalDef> {
        (self.signals)()
    }

    fn lower_ui(&self, ctx: &LowerCtx<'_>, node: &Node, style: &StyleProperties) -> UiNode {
        // Resolver — present on every production path, absent on
        // some headless test paths. Falling back to no-resolver means
        // composed `<shell.*>` tags inside the DSL drop to the
        // runtime's unknown-tag default (children surface, wrapper
        // disappears) — still a sensible fallback.
        let mut scope = LowerScope::default();
        if let Some(resolver) = self.resolver.get() {
            scope = scope.with_resolver(Arc::clone(resolver));
        } else if let Some(reg) = ctx.registry() {
            // Headless / one-shot path: build a resolver from the live
            // registry on the fly. Cheap — `ComponentRegistry::clone`
            // is an `IndexMap` clone of Arc<dyn Component> entries.
            let arc = Arc::new(reg.clone());
            scope = scope.with_resolver(Arc::new(RegistryTagResolver::new(arc)));
        }

        // Seed every prop as a scope binding so `{title}` /
        // `{enabled}` interpolations resolve verbatim. The DSL's
        // `lookup_expression` reads `scope.binding(name)`; matching
        // the prop key naming preserves Rust-side `ctx.prop_str` /
        // `ctx.prop_bool` semantics without a translation layer.
        if let Some(map) = node.props.as_object() {
            for (k, v) in map {
                scope = scope.with_binding(k.clone(), v.clone());
            }
        }

        // Wave 11.2 — pre-lowered children the caller (resolver path)
        // handed via `LowerCtx::host_children()` flow into the
        // DSL-side `<host-children/>` element. Composition wrappers
        // (toast-stack, launchpad, app-window) declare a single
        // `<host-children/>` in their body where the caller's children
        // should appear.
        if let Some(host) = ctx.host_children() {
            scope = scope.with_host_children_ui(host.to_vec());
        }

        let nodes = lower_document_with_scope(&self.parsed, &scope);
        match collapse_to_single_root(nodes, node, style, ctx) {
            UiNode::Container {
                id: _,
                props,
                children,
            } => UiNode::Container {
                // Rewrite the outer id to the calling node's id so
                // hit-testing / selection / per-NodeId reactive
                // contexts all find the right node. The DSL author
                // writes a `<container>` with whatever id, but the
                // canvas-facing id is the block instance.
                id: node.id.clone(),
                props,
                children,
            },
            other => other,
        }
    }
}

/// Collapse the AST's lowered root list into a single `UiNode`. The
/// migration convention is **single-root .prism-ui per component**;
/// when an author writes a multi-rooted source we wrap it in a flow
/// container so the shape stays predictable for the caller. Empty
/// sources fall back to the bare default container.
fn collapse_to_single_root(
    mut nodes: Vec<UiNode>,
    node: &Node,
    style: &StyleProperties,
    ctx: &LowerCtx<'_>,
) -> UiNode {
    match nodes.len() {
        0 => ctx.default_container(node, style),
        1 => nodes.remove(0),
        _ => UiNode::Container {
            id: node.id.clone(),
            props: prism_ui_runtime::layout::ContainerProps::default(),
            children: nodes,
        },
    }
}

/// Parse every spec in `specs` and register the resulting
/// [`PrismUiBlock`]s into `reg`. All blocks share the same
/// `resolver` cell — populate it post-registration with
/// [`finalize_prism_ui_resolver`].
pub fn register_prism_ui_components(
    reg: &mut ShellComponentRegistry,
    specs: &[PrismUiSpec],
    resolver: &SharedResolver,
) -> Result<(), PrismUiLoadError> {
    for spec in specs {
        let (parsed, errs) = parse(spec.source);
        if !errs.is_empty() {
            return Err(PrismUiLoadError::Parse {
                id: spec.id,
                errors: errs.into_iter().map(|e| e.message).collect(),
            });
        }
        let block = Arc::new(PrismUiBlock {
            id: spec.id.into(),
            schema: spec.schema,
            signals: spec.signals,
            parsed,
            resolver: Arc::clone(resolver),
        });
        reg.register(block).map_err(PrismUiLoadError::Register)?;
    }
    Ok(())
}

/// Populate the shared resolver cell. Call once after every
/// native + DSL block is registered so DSL `lower_ui` bodies can
/// dispatch composed tags through the live registry. Idempotent:
/// a second call after the cell is populated is a silent no-op
/// (matching `OnceLock::set`'s contract).
pub fn finalize_prism_ui_resolver(resolver: &SharedResolver, reg: &ShellComponentRegistry) {
    let _ = resolver.set(reg.tag_resolver());
}

/// Construct a fresh shared resolver cell. Returned `Arc` is cloned
/// into every `PrismUiBlock` plus the post-registration
/// [`finalize_prism_ui_resolver`] call.
pub fn make_shared_resolver() -> SharedResolver {
    Arc::new(OnceLock::new())
}

#[derive(Debug)]
pub enum PrismUiLoadError {
    Parse {
        id: &'static str,
        errors: Vec<String>,
    },
    Register(RegistryError),
}

impl std::fmt::Display for PrismUiLoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse { id, errors } => {
                write!(f, "parse error in `{id}`: {}", errors.join("; "))
            }
            Self::Register(e) => write!(f, "register error: {e:?}"),
        }
    }
}

impl std::error::Error for PrismUiLoadError {}

// ── Tier-1 migrated component sources ──
//
// Each row: an id in the `shell.*` namespace plus a `.prism-ui`
// source embedded via `include_str!`. Optional `.schema(fn)` /
// `.signals(fn)` to override the empty defaults.
//
// Author a new migration:
//   1. Write `ui/components/<id>.prism-ui` (single-root container).
//   2. Add one `PrismUiSpec::new(...)` row here.
//   3. Delete the old `components/<id>.rs` Rust file + its
//      `pub mod` row in `mod.rs` + its row in `SHELL_BUILTINS`.

fn toolbar_separator_schema() -> Vec<FieldSpec> {
    vec![]
}

fn help_tooltip_schema() -> Vec<FieldSpec> {
    vec![
        FieldSpec::text("title", "Title").required(),
        FieldSpec::text("summary", "Summary"),
    ]
}

fn docs_view_schema() -> Vec<FieldSpec> {
    vec![
        FieldSpec::text("title", "Title").required(),
        FieldSpec::text("summary", "Summary"),
        FieldSpec::text("body", "Body"),
    ]
}

fn docs_sidebar_schema() -> Vec<FieldSpec> {
    vec![
        FieldSpec::text("title", "Title").required(),
        FieldSpec::text("summary", "Summary"),
        FieldSpec::text("body", "Body"),
        FieldSpec::text("mode", "Mode (full|compact)"),
    ]
}

fn toast_stack_schema() -> Vec<FieldSpec> {
    vec![]
}

fn launchpad_schema() -> Vec<FieldSpec> {
    vec![FieldSpec::text("title", "Hero title")]
}

pub static SHELL_PRISM_UI_COMPONENTS: &[PrismUiSpec] = &[
    PrismUiSpec::new(
        "shell.toolbar-separator",
        include_str!("../../ui/components/toolbar-separator.prism-ui"),
    )
    .schema(toolbar_separator_schema),
    PrismUiSpec::new(
        "shell.help-tooltip",
        include_str!("../../ui/components/help-tooltip.prism-ui"),
    )
    .schema(help_tooltip_schema),
    PrismUiSpec::new(
        "shell.docs-view",
        include_str!("../../ui/components/docs-view.prism-ui"),
    )
    .schema(docs_view_schema),
    PrismUiSpec::new(
        "shell.docs-sidebar",
        include_str!("../../ui/components/docs-sidebar.prism-ui"),
    )
    .schema(docs_sidebar_schema),
    PrismUiSpec::new(
        "shell.toast-stack",
        include_str!("../../ui/components/toast-stack.prism-ui"),
    )
    .schema(toast_stack_schema),
    PrismUiSpec::new(
        "shell.launchpad",
        include_str!("../../ui/components/launchpad.prism-ui"),
    )
    .schema(launchpad_schema),
];

#[cfg(test)]
mod tests {
    use super::*;

    use crate::components::registry::register_shell_builtins;
    use prism_builder::document::Node as BuilderNode;
    use prism_builder::layout::LayoutMode;
    use prism_core::foundation::spatial::Transform2D;
    use serde_json::json;

    fn build_registry() -> (ShellComponentRegistry, SharedResolver) {
        let mut reg = ShellComponentRegistry::new();
        register_shell_builtins(&mut reg).expect("native shell builtins register");
        let resolver = make_shared_resolver();
        register_prism_ui_components(&mut reg, SHELL_PRISM_UI_COMPONENTS, &resolver)
            .expect("prism-ui components register");
        finalize_prism_ui_resolver(&resolver, &reg);
        (reg, resolver)
    }

    fn lower_from_registry(
        reg: &ShellComponentRegistry,
        id: &str,
        props: serde_json::Value,
    ) -> UiNode {
        let node = BuilderNode {
            id: format!("{id}-test"),
            component: id.into(),
            props,
            children: vec![],
            layout_mode: LayoutMode::default(),
            transform: Transform2D::default(),
            modifiers: vec![],
            style: StyleProperties::default(),
        };
        let cascade = StyleProperties::default();
        let ctx = LowerCtx::new(Some(reg.as_component_registry()), &cascade);
        let comp = reg
            .get(id)
            .unwrap_or_else(|| panic!("`{id}` not registered"));
        comp.lower_ui(&ctx, &node, &cascade)
    }

    #[test]
    fn every_prism_ui_spec_id_uses_shell_namespace() {
        for spec in SHELL_PRISM_UI_COMPONENTS {
            assert!(
                spec.id.starts_with("shell."),
                "id `{}` must use the shell.* namespace",
                spec.id
            );
        }
    }

    #[test]
    fn every_prism_ui_spec_parses_without_errors() {
        for spec in SHELL_PRISM_UI_COMPONENTS {
            let (_, errs) = parse(spec.source);
            assert!(
                errs.is_empty(),
                "`{}` source has parse errors: {errs:?}",
                spec.id
            );
        }
    }

    #[test]
    fn loader_populates_resolver_after_finalize() {
        let (_reg, resolver) = build_registry();
        assert!(
            resolver.get().is_some(),
            "shared resolver cell must be populated after finalize",
        );
    }

    #[test]
    fn prism_ui_specs_register_disjoint_from_native_builtins() {
        // Two id namespaces — native `SHELL_BUILTINS` and the new
        // DSL table — must stay disjoint. `register_specs` /
        // `register` rejects duplicates at runtime via
        // `RegistryError::AlreadyRegistered`, but a named pin here
        // surfaces collisions as a literal-table diff instead of a
        // boot panic.
        use crate::components::registry::SHELL_BUILTINS;
        let native: std::collections::HashSet<&str> = SHELL_BUILTINS.iter().map(|s| s.id).collect();
        for spec in SHELL_PRISM_UI_COMPONENTS {
            assert!(
                !native.contains(spec.id),
                "`{}` is in BOTH SHELL_BUILTINS (Rust) and SHELL_PRISM_UI_COMPONENTS (DSL); \
                 delete the Rust row to complete the migration",
                spec.id
            );
        }
    }

    #[test]
    fn toolbar_separator_lowers_to_1x20_translucent_stroke() {
        let (reg, _) = build_registry();
        let ui = lower_from_registry(&reg, "shell.toolbar-separator", json!({}));
        let UiNode::Container {
            id,
            props,
            children,
        } = ui
        else {
            panic!("toolbar-separator did not lower to a container")
        };
        assert_eq!(id, "shell.toolbar-separator-test");
        assert_eq!(props.width, prism_ui_runtime::layout::Sizing::Fixed(1.0));
        assert_eq!(props.height, prism_ui_runtime::layout::Sizing::Fixed(20.0));
        assert!(props.background.is_some());
        assert!(children.is_empty());
        assert_eq!(props.semantic.role.as_deref(), Some("separator"));
        let oriented = props
            .semantic
            .attrs
            .iter()
            .any(|(k, v)| k == "aria-orientation" && v == "vertical");
        assert!(oriented, "expected aria-orientation=\"vertical\" attr");
    }
}
