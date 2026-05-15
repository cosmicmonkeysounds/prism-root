//! Per-frame `render_tree` — folds `ShellPropBindings::snapshot` into
//! the parsed `app.prism-ui` skeleton, then lowers the result through
//! `RegistryTagResolver` into a flat `Vec<Node>` ready for
//! `Surface::set_tree`.
//!
//! The fold strategy keeps duplication out: the per-frame data flow
//! is *one* AST clone + *one* recursive walk + *one* lowering pass.
//! No per-block dispatch, no parallel "render walker," no second
//! prop-routing layer. Each binding emits a `serde_json::Value` once;
//! the walker merges it into the matching element's attributes; the
//! resolver's existing `element_to_builder_node` reads it the same
//! way it reads author-supplied attributes.
//!
//! See `docs/dev/clay-migration-plan.md` §17.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use prism_core::language::prism_ui::{
    self as prism_ui_ast, AttributeName, AttributeNamespace, AttributeValue,
};
use prism_core::language::prss;
use prism_core::language::syntax::{Position, SourceRange};
use prism_ui_runtime::interpret::{
    lower_document_with_scope, ImportResolver, LowerScope, TagEmission, TagResolver,
};
use prism_ui_runtime::layout::Node as UiNode;
use serde_json::Value;

use crate::props::{PropCtx, PropEmission, ShellPropBindings};

/// Pre-parsed `app.prism-ui` skeleton. Held once at boot — every
/// frame clones the AST, merges emissions into attributes, and lowers.
#[derive(Clone, Debug)]
pub struct Skeleton {
    pub doc: prism_ui_ast::Document,
}

impl Skeleton {
    /// Parse the on-disk `ui/app.prism-ui` source. Errors surface as
    /// a single concatenated diagnostic; the source is shipped in the
    /// crate so any parse error is a build-time defect.
    pub fn load() -> Result<Self, String> {
        Self::from_source(include_str!("../ui/app.prism-ui"))
    }

    pub fn from_source(source: &str) -> Result<Self, String> {
        let (doc, errors) = prism_ui_ast::parse(source);
        if !errors.is_empty() {
            let joined = errors
                .iter()
                .map(|e| format!("{}: {}", e.code, e.message))
                .collect::<Vec<_>>()
                .join("; ");
            return Err(joined);
        }
        Ok(Self { doc })
    }

    /// Load a skeleton from an absolute filesystem path. Used by
    /// `AppLoader` to parse per-app skeletons referenced from
    /// `manifest.toml`'s `[entry] skeleton` field (ADR-009).
    pub fn load_from_path(path: impl AsRef<Path>) -> Result<Self, String> {
        let source = std::fs::read_to_string(path.as_ref())
            .map_err(|e| format!("read {}: {e}", path.as_ref().display()))?;
        Self::from_source(&source)
    }

    /// ADR-009: graft an app skeleton's top-level nodes into this
    /// host skeleton's `<shell.app-window>` body. Returns a cloned
    /// skeleton with the substitution applied; the receiver is left
    /// untouched so a single host skeleton can host every app's
    /// body without sharing mutable state.
    ///
    /// If the host skeleton has no `<shell.app-window>` element (an
    /// unusual configuration — every shipped host has one), the app
    /// body is appended verbatim to the document root so the app's
    /// content still renders rather than being silently dropped.
    pub fn with_app_body(&self, app: &Skeleton) -> Skeleton {
        let mut merged = self.doc.clone();
        let mut grafted = false;
        graft_into_app_window(&mut merged.nodes, &app.doc.nodes, &mut grafted);
        if !grafted {
            // Fallback path: no `<shell.app-window>` found. Append
            // the app's body to the document root so we don't silently
            // drop the user's app content.
            merged.nodes.extend(app.doc.nodes.iter().cloned());
        }
        Skeleton { doc: merged }
    }
}

/// Walk `nodes` looking for the first `<shell.app-window>` element.
/// When found, replace its children with `app_body` (a clone, so
/// `Skeleton::with_app_body` can be called repeatedly against the
/// same source app skeleton). Sets `grafted = true` to signal
/// success to the caller.
fn graft_into_app_window(
    nodes: &mut [prism_ui_ast::Node],
    app_body: &[prism_ui_ast::Node],
    grafted: &mut bool,
) {
    for node in nodes {
        if *grafted {
            return;
        }
        if let prism_ui_ast::Node::Element(el) = node {
            if el.tag == "shell.app-window" {
                el.children = app_body.to_vec();
                el.self_closing = false;
                *grafted = true;
                return;
            }
            graft_into_app_window(&mut el.children, app_body, grafted);
        }
    }
}

/// ADR-009 default app skeleton — what fills `<shell.app-window>`
/// when an app declares no skeleton of its own. Single
/// `<shell.dock-workspace/>` element so today's four built-in apps
/// keep rendering exactly as they did before per-app skeletons.
pub fn default_app_skeleton() -> Skeleton {
    Skeleton::from_source(r#"<shell.dock-workspace id="dock"/>"#)
        .expect("default app skeleton parses")
}

/// Pre-parsed `.prss` stylesheet — the boot-time and hot-reload
/// counterpart to [`Skeleton`] for the PRSS half of the language
/// (see `docs/dev/prss-reference.md`). Held as `Arc` so it can be
/// cheap-cloned into every per-frame [`LowerScope`] without paying
/// the deep-copy cost on the hot render path.
///
/// Most hosts construct one of these once at boot via
/// [`Stylesheet::load_from_path`] (or from an embedded source via
/// [`Stylesheet::from_source`]) and install it on the `Shell` with
/// `Shell::install_stylesheet`. The dev-loop hot-reload pipeline
/// builds a fresh `Stylesheet` from each `.prss` save and reinstalls;
/// the existing render path picks up the new tokens + classes on the
/// next frame.
#[derive(Clone, Debug)]
pub struct Stylesheet {
    sheet: Arc<prss::StyleSheet>,
}

impl Stylesheet {
    /// Empty stylesheet — equivalent to "no stylesheet loaded". Useful
    /// as a default before the host parses any `.prss` source. The
    /// runtime treats `class="..."` against an empty sheet as a
    /// data-round-trip no-op (the static class list still flows into
    /// `Semantic::class` for SSR; only the styling lookup is empty).
    pub fn empty() -> Self {
        Self {
            sheet: Arc::new(prss::StyleSheet::default()),
        }
    }

    /// Read `.prss` source from `path` and parse. Recoverable
    /// PRSS diagnostics (`missing-parent`, `cyclic-extends`, …) ride
    /// alongside the partial sheet; the host's diagnostic surface
    /// reads them through [`Stylesheet::diagnostics`]. A hard syntax
    /// error (`toml-syntax`) still parses — the sheet is empty in
    /// that case but the diagnostic is reported.
    pub fn load_from_path(path: impl AsRef<Path>) -> Result<Self, String> {
        let source = std::fs::read_to_string(path.as_ref())
            .map_err(|e| format!("{}: {e}", path.as_ref().display()))?;
        Ok(Self::from_source(&source))
    }

    /// Parse a literal `.prss` source string. Mirrors
    /// [`Skeleton::from_source`] in shape — recoverable diagnostics
    /// don't fail the construction; the caller pulls them through
    /// [`Stylesheet::diagnostics`] separately.
    pub fn from_source(source: &str) -> Self {
        let (sheet, _errors) = prss::parse(source);
        Self {
            sheet: Arc::new(sheet),
        }
    }

    /// Construct from an already-parsed [`prss::StyleSheet`]. Used by
    /// hot-reload paths that already have the parsed shape in hand
    /// (the watcher built one to compute the fingerprint anyway).
    pub fn from_sheet(sheet: prss::StyleSheet) -> Self {
        Self {
            sheet: Arc::new(sheet),
        }
    }

    /// Borrow the inner Arc so the render path can install it on the
    /// scope without a fresh allocation.
    pub fn arc(&self) -> Arc<prss::StyleSheet> {
        Arc::clone(&self.sheet)
    }

    /// Borrow the parsed stylesheet for read-only inspection (class
    /// listing, token table, descendant selectors).
    pub fn sheet(&self) -> &prss::StyleSheet {
        &self.sheet
    }

    /// Re-parse the source and surface every diagnostic. Used by the
    /// dev-loop watcher when it wants to show authors error messages
    /// alongside the cached sheet (which keeps rendering with the
    /// previous good values until the syntax issue is fixed).
    pub fn diagnostics_for(source: &str) -> Vec<prss::ParseError> {
        prss::parse(source).1
    }

    /// ADR-009 follow-on: produce a new stylesheet by layering
    /// `overlay` on top of `self`. The overlay's tokens and class
    /// definitions win on conflict — same semantics as CSS where a
    /// later rule with the same specificity overrides the earlier.
    /// Used by the render path to cascade an app stylesheet over the
    /// host's global stylesheet.
    ///
    /// Returns a fresh `Stylesheet` — both inputs are left untouched
    /// so a single host stylesheet can serve many active-app swaps.
    pub fn merge_with(&self, overlay: &Stylesheet) -> Stylesheet {
        let mut merged = (*self.sheet).clone();
        let other = overlay.sheet.as_ref();
        // Token overrides: each of the four bucket maps merges
        // entry-by-entry with the overlay winning on key conflict.
        for (k, v) in &other.tokens.colors {
            merged.tokens.colors.insert(k.clone(), v.clone());
        }
        for (k, v) in &other.tokens.spacing {
            merged.tokens.spacing.insert(k.clone(), v.clone());
        }
        for (k, v) in &other.tokens.radius {
            merged.tokens.radius.insert(k.clone(), v.clone());
        }
        for (k, v) in &other.tokens.typography {
            merged.tokens.typography.insert(k.clone(), v.clone());
        }
        // Classes: overlay's class definitions replace base's. We
        // don't field-level merge inside `ClassDef` — PRSS classes
        // are atomic by name (a class with the same name in two
        // sheets is the same class; the overlay says "use my full
        // definition").
        for (name, def) in &other.classes {
            merged.classes.insert(name.clone(), def.clone());
        }
        // Version: overlay wins if it specifies one, else keep base's.
        if other.version.is_some() {
            merged.version = other.version;
        }
        Stylesheet {
            sheet: Arc::new(merged),
        }
    }
}

impl Default for Stylesheet {
    fn default() -> Self {
        Self::empty()
    }
}

/// **PRSS hot-reload watcher** — wraps a
/// [`prism_ui_build::PrssFingerprintCache`] and a freshly-loaded
/// [`Stylesheet`] in one host-friendly bundle. Each `observe(path)`
/// call re-reads the file, classifies the change, and on success
/// rebuilds the active stylesheet. The host then installs the new
/// sheet on its `Shell` via `Shell::install_stylesheet`.
///
/// The watcher owns the cache so the patch outcomes
/// (`LiteralOnly` / `Structural`) stay deterministic across calls
/// — every `observe` against the same path classifies relative to
/// the cache's last-seen fingerprint, not the previous on-disk
/// state.
pub struct StylesheetWatcher {
    cache: prism_ui_build::PrssFingerprintCache,
    /// The most recent successfully-parsed sheet. The watcher keeps
    /// it so a `ParseError` / `ReadError` doesn't drop the host's
    /// styling — the previous good values keep rendering until the
    /// file becomes valid again.
    current: Stylesheet,
}

/// What [`StylesheetWatcher::observe`] returned: the change
/// classification plus, when the change produced a new valid
/// sheet, the sheet itself ready for `Shell::install_stylesheet`.
#[derive(Debug)]
pub struct StylesheetReload {
    pub change: prism_ui_build::PrssChange,
    /// `Some(sheet)` for `FirstSighting` / `LiteralOnly` /
    /// `Structural`. `None` for `NoChange` / `ParseError` /
    /// `ReadError` — the host keeps its previously-installed
    /// stylesheet in those cases.
    pub stylesheet: Option<Stylesheet>,
}

impl Default for StylesheetWatcher {
    fn default() -> Self {
        Self::new()
    }
}

impl StylesheetWatcher {
    pub fn new() -> Self {
        Self {
            cache: prism_ui_build::PrssFingerprintCache::new(),
            current: Stylesheet::empty(),
        }
    }

    /// The most recent successfully-parsed stylesheet. Hosts that
    /// want to re-install the cached sheet (e.g. on shell startup
    /// after a `--hot=subsecond` patch) read it through here.
    pub fn current(&self) -> Stylesheet {
        self.current.clone()
    }

    /// Observe `path` once: read + parse + classify against the
    /// cache. On a successful classification (`FirstSighting` /
    /// `LiteralOnly` / `Structural`), updates `self.current` and
    /// returns it on the [`StylesheetReload`] so the caller can
    /// hand it to `Shell::install_stylesheet`.
    pub fn observe(&mut self, path: impl AsRef<Path>) -> StylesheetReload {
        let path = path.as_ref();
        let change = self.cache.observe(path);
        let stylesheet = match &change {
            prism_ui_build::PrssChange::FirstSighting { .. }
            | prism_ui_build::PrssChange::LiteralOnly { .. }
            | prism_ui_build::PrssChange::Structural => match Stylesheet::load_from_path(path) {
                Ok(s) => {
                    self.current = s.clone();
                    Some(s)
                }
                Err(_) => None,
            },
            _ => None,
        };
        StylesheetReload { change, stylesheet }
    }

    /// Test-friendly variant of [`Self::observe`] — feed source
    /// directly without touching the filesystem. The path is the
    /// cache key only.
    pub fn observe_source(&mut self, path: impl AsRef<Path>, source: &str) -> StylesheetReload {
        let change = self
            .cache
            .observe_source(path.as_ref().to_path_buf(), source);
        let stylesheet = match &change {
            prism_ui_build::PrssChange::FirstSighting { .. }
            | prism_ui_build::PrssChange::LiteralOnly { .. }
            | prism_ui_build::PrssChange::Structural => {
                let sheet = Stylesheet::from_source(source);
                self.current = sheet.clone();
                Some(sheet)
            }
            _ => None,
        };
        StylesheetReload { change, stylesheet }
    }
}

/// Build the runtime `Node` tree for one frame.
///
/// Pipeline (each step is one function, every block flows through it):
///   `bindings.snapshot(ctx)`            — typed substate → JSON props (+ optional children)
///   → `fill_compositions(skeleton, e)`  — merge props into AST attributes
///   → `harvest_host_children(e)`        — collect emission children by tag (§43 B2)
///   → `lower_document_with_scope(...)`  — resolver dispatches per tag,
///                                         consulting both the AST and the
///                                         scope's `host_children_by_tag` map.
pub fn render_tree(
    skeleton: &Skeleton,
    bindings: &ShellPropBindings,
    resolver: Arc<dyn TagResolver>,
    ctx: &PropCtx,
) -> Vec<UiNode> {
    render_tree_with(skeleton, bindings, resolver, ctx, None, None, None)
}

/// **Wave 14.3** — same as [`render_tree`] but threads a host-owned
/// [`prism_ui_runtime::interpret::MemoCache`] through the lowering
/// scope. Elements carrying both `memo="[…]"` and a resolvable
/// `id="…"` skip re-lowering whenever their dep tuple matches the
/// previously-cached one. Callers without a cache pass `None` and
/// get the legacy behaviour.
///
/// The `stylesheet` parameter installs a PRSS [`Stylesheet`] into the
/// lowering scope so every container with a `class="..."` or
/// `class:foo="{cond}"` attribute resolves through the named-class
/// vocabulary (`prss-reference.md`). `None` skips the install — the
/// runtime treats class attributes as data-round-trip only.
pub fn render_tree_with(
    skeleton: &Skeleton,
    bindings: &ShellPropBindings,
    resolver: Arc<dyn TagResolver>,
    ctx: &PropCtx,
    memo_cache: Option<std::rc::Rc<std::cell::RefCell<prism_ui_runtime::interpret::MemoCache>>>,
    stylesheet: Option<&Stylesheet>,
    import_resolver: Option<Arc<dyn ImportResolver>>,
) -> Vec<UiNode> {
    let emissions = bindings.snapshot(ctx);
    let doc = fill_compositions(skeleton, &emissions);
    let host_children = harvest_host_children(&emissions);
    // Build the same emissions map keyed by tag for the
    // `lower_as` consultation path. Dock-panel routes by `panel-id`
    // → content tag at lower time, bypassing the resolver/AST seam
    // that `host_children_by_tag` plugs into; without this second
    // map, routed panels (`shell.builder-canvas`,
    // `shell.component-palette`, `shell.properties-panel`, …) get
    // empty props and zero children. One snapshot, two consumers.
    let tag_emissions = harvest_tag_emissions(&emissions);
    let mut scope = LowerScope::default()
        .with_resolver(resolver)
        .with_host_children_by_tag(host_children)
        .with_tag_emissions(tag_emissions)
        // **Wave 14.1** — design tokens as a global binding. Mirrors
        // the `PrismUiBlock::lower_ui` injection so every DSL surface
        // (skeleton + per-block) resolves `tokens.*` uniformly. Boot
        // tokens are the workspace default; later waves swap for a
        // user-customised palette through a settings hook.
        .with_design_tokens(&prism_core::design_tokens::DEFAULT_TOKENS);
    if let Some(cache) = memo_cache {
        scope = scope.with_memo_cache(cache);
    }
    if let Some(sheet) = stylesheet {
        // PRSS install runs *after* `with_design_tokens` so the
        // sheet's `[tokens.*]` overrides cascade over the
        // workspace default — matches the §4.6 application order
        // (sheet later → wins on conflicts).
        scope = scope.with_stylesheet(sheet.arc());
    }
    if let Some(res) = import_resolver {
        // **Wave H.1 (§5.4/§5.9)** — the active app's
        // `FsImportResolver`, so a per-app `.prui` skeleton can
        // `<import stylesheet|script [as=]/>` relative to the app
        // directory + `prism://` roots.
        scope = scope.with_import_resolver(res);
    }
    lower_document_with_scope(&doc, &scope)
}

/// Collect `emission.children` slices into a tag-keyed map for
/// injection into [`LowerScope::with_host_children_by_tag`]. Tags
/// whose emission carries no children are omitted (no point taking
/// space in the map). The values are moved out of the emissions —
/// the props field stays behind for `fill_compositions` to use.
fn harvest_host_children(
    emissions: &HashMap<&'static str, PropEmission>,
) -> HashMap<String, Vec<UiNode>> {
    let mut out: HashMap<String, Vec<UiNode>> = HashMap::new();
    for (tag, emission) in emissions {
        if emission.children.is_empty() {
            continue;
        }
        out.insert((*tag).to_string(), emission.children.clone());
    }
    out
}

/// Project every emission into a `TagEmission` snapshot for the
/// `LowerCtx::lower_as` consultation path. Unlike
/// [`harvest_host_children`], this map keeps the entry even when
/// children are empty — the props alone are valuable (toolbar
/// numbers, canvas selection-id, palette items …) and a synthesised
/// `lower_as` call still wants them merged in.
fn harvest_tag_emissions(
    emissions: &HashMap<&'static str, PropEmission>,
) -> HashMap<String, TagEmission> {
    let mut out: HashMap<String, TagEmission> = HashMap::new();
    for (tag, emission) in emissions {
        out.insert(
            (*tag).to_string(),
            TagEmission {
                props: emission.props.clone(),
                children: emission.children.clone(),
            },
        );
    }
    out
}

/// Pure recursive walk: for every `<shell.foo>` element, merge
/// `emissions["shell.foo"].props` into its attributes. Author-supplied
/// `Bare` attributes win over emissions (so the skeleton can pin
/// structural props); `Identifier` (`id`) and other namespaces are
/// untouched.
///
/// One rule, every block. Adding a new emission key is one row in
/// `register_builtin_bindings`; the walker doesn't change.
pub fn fill_compositions(
    skeleton: &Skeleton,
    emissions: &HashMap<&'static str, PropEmission>,
) -> prism_ui_ast::Document {
    let mut doc = skeleton.doc.clone();
    inject_emissions(&mut doc.nodes, emissions);
    doc
}

fn inject_emissions(
    nodes: &mut [prism_ui_ast::Node],
    emissions: &HashMap<&'static str, PropEmission>,
) {
    for node in nodes {
        if let prism_ui_ast::Node::Element(el) = node {
            if let Some(emission) = emissions.get(el.tag.as_str()) {
                merge_props_into_attributes(el, &emission.props);
            }
            inject_emissions(&mut el.children, emissions);
        }
    }
}

fn merge_props_into_attributes(element: &mut prism_ui_ast::Element, props: &Value) {
    let Value::Object(map) = props else { return };
    for (key, value) in map {
        if element
            .attributes
            .iter()
            .any(|a| a.name.namespace == AttributeNamespace::Bare && a.name.local == *key)
        {
            continue;
        }
        element.attributes.push(synthetic_attribute(key, value));
    }
}

fn synthetic_attribute(key: &str, value: &Value) -> prism_ui_ast::Attribute {
    let raw_value = match value {
        Value::String(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        // JSON arrays / nested objects round-trip as their canonical
        // serialised form — the receiving block's schema decodes the
        // string back into typed shape via `serde_json::from_str`.
        other => other.to_string(),
    };
    let zero = SourceRange {
        start: Position {
            offset: 0,
            line: 1,
            column: 0,
        },
        end: Position {
            offset: 0,
            line: 1,
            column: 0,
        },
    };
    prism_ui_ast::Attribute {
        name: AttributeName {
            raw: key.to_string(),
            local: key.to_string(),
            namespace: AttributeNamespace::Bare,
            range: zero,
        },
        value: AttributeValue::String {
            value: raw_value,
            range: zero,
        },
        range: zero,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::{
        register_shell_builtins, registry::register_full_shell_chrome, ShellComponentRegistry,
    };
    use crate::props::ShellPropBindings;
    use crate::AppState;

    fn ctx<'a>(state: &'a AppState) -> PropCtx<'a> {
        PropCtx {
            state,
            viewport_w: 1280.0,
            viewport_h: 800.0,
            canvas_zoom: 1.0,
            registry: None,
            block_invalidator: None,
            modifier_registry: None,
            dock_catalog: None,
        }
    }

    #[test]
    fn skeleton_loads_from_disk() {
        let skel = Skeleton::load().expect("parse app.prism-ui");
        assert!(!skel.doc.nodes.is_empty());
    }

    #[test]
    fn default_app_skeleton_is_a_dock_workspace() {
        // ADR-009 pin: the fallback skeleton applied when an app
        // declares no `[entry] skeleton` must be exactly one
        // `<shell.dock-workspace/>` element, preserving the
        // pre-split behaviour.
        let s = default_app_skeleton();
        assert_eq!(s.doc.nodes.len(), 1);
        match &s.doc.nodes[0] {
            prism_ui_ast::Node::Element(el) => {
                assert_eq!(el.tag, "shell.dock-workspace");
                assert!(el.children.is_empty());
            }
            other => panic!("expected element, got {other:?}"),
        }
    }

    #[test]
    fn with_app_body_grafts_into_shell_app_window() {
        // ADR-009: the host skeleton's `<shell.app-window>` body is
        // empty; `with_app_body` clones the host and replaces that
        // body with the app skeleton's top-level nodes.
        let host = Skeleton::load().expect("host parse");
        // Before the graft, the `<shell.app-window>` element should
        // have no *element* children (HTML comments are preserved by
        // the parser but they don't contribute to lowered output).
        let app_window_before = find_app_window(&host.doc.nodes).expect("app-window present");
        let element_children: Vec<_> = app_window_before
            .children
            .iter()
            .filter(|n| matches!(n, prism_ui_ast::Node::Element(_)))
            .collect();
        assert!(
            element_children.is_empty(),
            "host skeleton must declare empty `<shell.app-window>` element body"
        );

        let app =
            Skeleton::from_source(r#"<shell.dock-workspace id="my-dock"/>"#).expect("app parse");
        let merged = host.with_app_body(&app);
        let app_window_after = find_app_window(&merged.doc.nodes).expect("app-window present");
        let merged_elements: Vec<&prism_ui_ast::Node> = app_window_after
            .children
            .iter()
            .filter(|n| matches!(n, prism_ui_ast::Node::Element(_)))
            .collect();
        assert_eq!(
            merged_elements.len(),
            1,
            "graft should populate body with one app element node"
        );
        match merged_elements[0] {
            prism_ui_ast::Node::Element(el) => {
                assert_eq!(el.tag, "shell.dock-workspace");
                // Verify the id from the app skeleton flows through.
                let id_attr = el
                    .attributes
                    .iter()
                    .find(|a| a.name.raw == "id")
                    .expect("id attribute");
                match &id_attr.value {
                    prism_ui_ast::AttributeValue::String { value, .. } => {
                        assert_eq!(value, "my-dock");
                    }
                    other => panic!("expected literal id, got {other:?}"),
                }
            }
            other => panic!("expected element, got {other:?}"),
        }
    }

    #[test]
    fn with_app_body_leaves_overlay_siblings_intact() {
        // ADR-009: the host skeleton has overlay siblings
        // (workflow-page-bar, command-palette, toast-stack, …) outside
        // `<shell.app-window>`. The graft must touch only the
        // app-window's body; overlay siblings stay where they were.
        let host = Skeleton::load().expect("host parse");
        let app = Skeleton::from_source(r#"<shell.dock-workspace/>"#).expect("app parse");
        let merged = host.with_app_body(&app);
        // Same number of top-level nodes before and after.
        assert_eq!(merged.doc.nodes.len(), host.doc.nodes.len());
        // The overlay tags appear at the same positions.
        let host_tags = top_level_tags(&host.doc.nodes);
        let merged_tags = top_level_tags(&merged.doc.nodes);
        assert_eq!(host_tags, merged_tags);
    }

    #[test]
    fn with_app_body_replaces_existing_body() {
        // Idempotency: applying `with_app_body` against a host that
        // already has a body replaces it rather than appending.
        let host = Skeleton::load().expect("host parse");
        let first = host.with_app_body(
            &Skeleton::from_source(r#"<shell.dock-workspace id="first"/>"#).unwrap(),
        );
        let second = first.with_app_body(
            &Skeleton::from_source(r#"<shell.dock-workspace id="second"/>"#).unwrap(),
        );
        let body = find_app_window(&second.doc.nodes).unwrap();
        let elements: Vec<&prism_ui_ast::Node> = body
            .children
            .iter()
            .filter(|n| matches!(n, prism_ui_ast::Node::Element(_)))
            .collect();
        assert_eq!(elements.len(), 1);
        match elements[0] {
            prism_ui_ast::Node::Element(el) => {
                let id_attr = el.attributes.iter().find(|a| a.name.raw == "id").unwrap();
                match &id_attr.value {
                    prism_ui_ast::AttributeValue::String { value, .. } => {
                        assert_eq!(value, "second");
                    }
                    other => panic!("expected literal, got {other:?}"),
                }
            }
            other => panic!("expected element, got {other:?}"),
        }
    }

    fn find_app_window(nodes: &[prism_ui_ast::Node]) -> Option<&prism_ui_ast::Element> {
        for node in nodes {
            if let prism_ui_ast::Node::Element(el) = node {
                if el.tag == "shell.app-window" {
                    return Some(el);
                }
                if let Some(found) = find_app_window(&el.children) {
                    return Some(found);
                }
            }
        }
        None
    }

    fn top_level_tags(nodes: &[prism_ui_ast::Node]) -> Vec<String> {
        nodes
            .iter()
            .filter_map(|n| match n {
                prism_ui_ast::Node::Element(el) => Some(el.tag.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn render_tree_lowers_full_skeleton_through_resolver() {
        // ADR-009: compose host + default app skeletons so the
        // legacy dock-workspace body is present for this resolver
        // walk. Same as `Shell::render` does at runtime.
        let host = Skeleton::load().expect("parse");
        let skel = host.with_app_body(&default_app_skeleton());
        let bindings = ShellPropBindings::with_builtins();
        let mut reg = ShellComponentRegistry::new();
        // Wave 11.3 — the dock-workspace / dock-panel / field-editor
        // migrations moved every chrome block into the DSL registry.
        // The test runs through the resolver, so it has to register
        // the full chrome (native + DSL) instead of just the native
        // `SHELL_BUILTINS` table.
        crate::components::registry::register_full_shell_chrome(&mut reg).expect("register");
        let resolver = reg.tag_resolver();
        let state = AppState::default();

        let nodes = render_tree(&skel, &bindings, resolver, &ctx(&state));
        // Skeleton root contains the app-window plus four overlay
        // siblings (workflow-page-bar + command-palette + toast-stack
        // + help-tooltip + context-menu + menu-dropdown +
        // component-picker). Just assert non-empty and that the first
        // node lowered to a container — the structural assertions live
        // in the existing `canonical_app_prism_ui_skeleton_lowers…`
        // test in `components::registry`.
        assert!(!nodes.is_empty());
        assert!(matches!(nodes[0], UiNode::Container { .. }));
    }

    #[test]
    fn slot_data_flows_through_snapshot_into_emissions() {
        // §19: the chrome slot's `status_bar_props` is the single
        // source of the status string — bumping the slot must show
        // up in the emissions map without any other wiring.
        let bindings = ShellPropBindings::with_builtins();
        let mut state = AppState::default();
        state.chrome.status = "Saving…".into();
        let emissions = bindings.snapshot(&ctx(&state));
        let status = &emissions["shell.status-bar"].props;
        assert_eq!(status["status"], "Saving…");
        let app = &emissions["shell.app-window"].props;
        assert_eq!(app["status"], "Saving…");
    }

    #[test]
    fn workspace_page_switch_propagates_to_three_bindings() {
        // §19 cross-slot test: a single mutation on `state.workspace`
        // must show up consistently in every binding that reads it,
        // without any binding inlining its own JSON shape.
        let bindings = ShellPropBindings::with_builtins();
        let mut state = AppState::default();
        let target_idx = 2usize;
        let target_id = state.workspace.workspace.pages()[target_idx].id.clone();
        state.workspace.workspace.switch_page_by_id(&target_id);

        let emissions = bindings.snapshot(&ctx(&state));

        let pages = emissions["shell.workflow-page-bar"].props["pages"]
            .as_array()
            .expect("pages array");
        assert_eq!(pages[target_idx]["active"], true);

        let menu_tabs = emissions["shell.menu-bar-row"].props["tabs"]
            .as_array()
            .expect("menu tabs");
        assert_eq!(menu_tabs[target_idx]["active"], true);

        let win_tabs = emissions["shell.app-window"].props["tabs"]
            .as_array()
            .expect("app-window tabs");
        assert_eq!(win_tabs[target_idx]["active"], true);
    }

    #[test]
    fn nav_active_flag_propagates_to_list_and_graph_bindings() {
        // §20 cross-binding parity: a single mutation on the
        // navigation slot must show up in *both* its consumer
        // emissions (`shell.nav-page-list`, `shell.nav-graph`)
        // — the load-bearing duplication check for the
        // navigation port wave.
        use crate::state::{NavEdge, NavEdgeKind, NavPage};
        let bindings = ShellPropBindings::with_builtins();
        let mut state = AppState::default();
        state.navigation.pages = vec![
            NavPage {
                id: "home".into(),
                title: "Home".into(),
                route: "/".into(),
                x: 0.0,
                y: 0.0,
                node_count: 0,
                link_count: 0,
                is_active: false,
            },
            NavPage {
                id: "about".into(),
                title: "About".into(),
                route: "/about".into(),
                x: 200.0,
                y: 0.0,
                node_count: 0,
                link_count: 0,
                is_active: true,
            },
        ];
        state.navigation.edges.push(NavEdge {
            from: 0,
            to: 1,
            kind: NavEdgeKind::Href,
        });
        let emissions = bindings.snapshot(&ctx(&state));
        let list = &emissions["shell.nav-page-list"].props;
        let graph = &emissions["shell.nav-graph"].props;
        assert_eq!(list["pages"][1]["is-active"], true);
        assert_eq!(graph["pages"][1]["is-active"], true);
        assert_eq!(graph["edges"][0]["kind"], "href");
    }

    #[test]
    fn overlay_command_palette_open_propagates_to_emission() {
        let bindings = ShellPropBindings::with_builtins();
        let mut state = AppState::default();
        state.overlay.command_palette.open = true;
        state.overlay.command_palette.query = "save".into();
        let emissions = bindings.snapshot(&ctx(&state));
        let cp = &emissions["shell.command-palette"].props;
        assert_eq!(cp["open"], true);
        assert_eq!(cp["query"], "save");
    }

    #[test]
    fn fill_compositions_merges_props_as_attributes() {
        let skel = Skeleton::from_source(r#"<shell.status-bar id="sb"/>"#).expect("parse");
        let mut emissions: HashMap<&'static str, PropEmission> = HashMap::new();
        emissions.insert(
            "shell.status-bar",
            PropEmission::from_props(serde_json::json!({"status": "Saving…"})),
        );
        let doc = fill_compositions(&skel, &emissions);
        let prism_ui_ast::Node::Element(el) = &doc.nodes[0] else {
            panic!("expected element")
        };
        let status = el
            .attributes
            .iter()
            .find(|a| a.name.local == "status")
            .expect("status attr injected");
        match &status.value {
            AttributeValue::String { value, .. } => assert_eq!(value, "Saving…"),
            other => panic!("unexpected value shape: {other:?}"),
        }
    }

    #[test]
    fn fill_compositions_author_attribute_wins() {
        let skel =
            Skeleton::from_source(r#"<shell.status-bar id="sb" status="Pinned"/>"#).expect("parse");
        let mut emissions: HashMap<&'static str, PropEmission> = HashMap::new();
        emissions.insert(
            "shell.status-bar",
            PropEmission::from_props(serde_json::json!({"status": "Overridden"})),
        );
        let doc = fill_compositions(&skel, &emissions);
        let prism_ui_ast::Node::Element(el) = &doc.nodes[0] else {
            panic!()
        };
        let occurrences: Vec<_> = el
            .attributes
            .iter()
            .filter(|a| a.name.local == "status")
            .collect();
        assert_eq!(occurrences.len(), 1, "author attr not duplicated");
        match &occurrences[0].value {
            AttributeValue::String { value, .. } => assert_eq!(value, "Pinned"),
            _ => panic!(),
        }
    }

    #[test]
    fn dock_workspace_emission_round_trips_through_resolver_to_routed_panel() {
        // End-to-end: the binding emits the active page's DockNode,
        // the synthetic-attribute walker injects it into the
        // skeleton, the resolver decodes the JSON via `value_for`'s
        // `[`/`{` parse rule, the dock-workspace block recurses into
        // the leaf, and the leaf dispatches to the routed content tag.
        // Switching workflow pages flips the embedded panel without
        // any binding edit. This is the §16/§17 closing property
        // expressed end-to-end against the pipeline.
        // ADR-009: compose with the default app skeleton so the
        // dock-workspace is grafted into `<shell.app-window>`'s body.
        let host = Skeleton::load().expect("parse");
        let skel = host.with_app_body(&default_app_skeleton());
        let bindings = ShellPropBindings::with_builtins();
        let mut reg = ShellComponentRegistry::new();
        // Wave 11.2 batch: `shell.app-window` is DSL-authored, so the
        // skeleton's outer tag resolves via the full chrome registry.
        register_full_shell_chrome(&mut reg).expect("register");
        let resolver = reg.tag_resolver();

        let mut state = AppState::default();
        // Edit page is index 0 in builtins; switch to a different page
        // and assert the emission tracks. We use whichever non-zero
        // index exists (workspace defaults provide >=2 pages).
        if state.workspace.workspace.pages().len() > 1 {
            state.workspace.workspace.switch_page(1);
        }

        let nodes = render_tree(&skel, &bindings, resolver, &ctx(&state));
        // Walk to the dock-workspace's outer container and verify it
        // produced at least one descendant (i.e. the JSON round-tripped
        // — `value_for` parsed the dock attr back into an Object so
        // the block could decode it).
        let UiNode::Container { children, .. } = &nodes[0] else {
            panic!("root not a container")
        };
        let UiNode::Container {
            children: body_kids,
            ..
        } = &children[1]
        else {
            panic!("body row not a container")
        };
        let UiNode::Container {
            children: content_kids,
            ..
        } = &body_kids[1]
        else {
            panic!("content area not a container")
        };
        let UiNode::Container {
            children: ws_kids, ..
        } = &content_kids[0]
        else {
            panic!("dock-workspace not a container")
        };
        // Workspace wraps the recursive emission in one container —
        // proves the JSON survived the synthetic-attribute round-trip.
        assert_eq!(
            ws_kids.len(),
            1,
            "dock-workspace should wrap one recursive subtree, got {}",
            ws_kids.len()
        );
    }

    #[test]
    fn canvas_binding_emits_document_as_host_children() {
        // §43 B3 end-to-end: a non-empty `BuilderDocument` flows out of
        // `state.canvas.document`, through `lower_document_to_ui`,
        // into the canvas binding's `PropEmission::children`,
        // harvested by `harvest_host_children`, and into LowerScope's
        // tag-keyed map. The resolver then prefers those over any
        // (empty) AST pre-lowering for `shell.builder-canvas`.
        use prism_builder::{BuilderDocument, Node};

        let bindings = ShellPropBindings::with_builtins();
        let mut shell_reg = ShellComponentRegistry::new();
        register_shell_builtins(&mut shell_reg).expect("register");
        // Builder builtins must be registered too so the document's
        // `text` / `button` / etc. components resolve at lower time.
        let mut comp_reg = prism_builder::ComponentRegistry::new();
        prism_builder::starter::register_builtins(&mut comp_reg).expect("builder builtins");
        // Merge the two so the canvas binding can see both shell tags
        // and builder block ids through one registry. For this test we
        // only need the builder side — the resolver path doesn't run
        // the canvas binding through `shell_reg`.
        let live_reg = comp_reg;

        let mut state = AppState::default();
        let mut doc = BuilderDocument::page_shell();
        if let Some(root) = doc.root.as_mut() {
            root.children = vec![Node {
                id: "demo-text".into(),
                component: "text".into(),
                props: serde_json::json!({ "body": "Hi" }),
                ..Default::default()
            }];
        }
        state.canvas.document = doc;

        let ctx = PropCtx {
            state: &state,
            viewport_w: 1280.0,
            viewport_h: 800.0,
            canvas_zoom: 1.0,
            registry: Some(&live_reg),
            block_invalidator: None,
            modifier_registry: None,
            dock_catalog: None,
        };
        let emissions = bindings.snapshot(&ctx);
        let canvas = emissions
            .get("shell.builder-canvas")
            .expect("canvas emission");
        assert_eq!(
            canvas.children.len(),
            1,
            "canvas emission carries one root container child"
        );
        // Harvest folds non-empty children into the map; absent
        // emissions never appear there.
        let host_children = harvest_host_children(&emissions);
        assert!(
            host_children.contains_key("shell.builder-canvas"),
            "harvest_host_children must surface canvas emission"
        );
        assert!(
            !host_children.contains_key("shell.status-bar"),
            "harvest_host_children skips empty emissions"
        );
    }

    #[test]
    fn canvas_binding_emits_empty_children_when_registry_absent() {
        // Headless / no-DI path: same binding, no registry → empty
        // children, no panic. Pure slot-accessor bindings keep working.
        let bindings = ShellPropBindings::with_builtins();
        let state = AppState::default();
        let ctx = PropCtx {
            state: &state,
            viewport_w: 1280.0,
            viewport_h: 800.0,
            canvas_zoom: 1.0,
            registry: None,
            block_invalidator: None,
            modifier_registry: None,
            dock_catalog: None,
        };
        let emissions = bindings.snapshot(&ctx);
        let canvas = emissions
            .get("shell.builder-canvas")
            .expect("canvas emission");
        assert!(canvas.children.is_empty());
    }

    #[test]
    fn fill_compositions_serialises_arrays_as_attribute_strings() {
        let skel = Skeleton::from_source(r#"<shell.toast-stack id="t"/>"#).expect("parse");
        let mut emissions: HashMap<&'static str, PropEmission> = HashMap::new();
        emissions.insert(
            "shell.toast-stack",
            PropEmission::from_props(serde_json::json!({
                "toasts": [{"kind": "info", "title": "Hi"}]
            })),
        );
        let doc = fill_compositions(&skel, &emissions);
        let prism_ui_ast::Node::Element(el) = &doc.nodes[0] else {
            panic!()
        };
        let toasts = el
            .attributes
            .iter()
            .find(|a| a.name.local == "toasts")
            .expect("toasts attr injected");
        let AttributeValue::String { value, .. } = &toasts.value else {
            panic!()
        };
        // Round-trip: receiving block decodes via serde_json::from_str.
        let parsed: Value = serde_json::from_str(value).expect("array round-trips");
        assert!(parsed.is_array());
    }
}
