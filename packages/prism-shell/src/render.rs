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
use std::sync::Arc;

use prism_core::language::prism_ui::{
    self as prism_ui_ast, AttributeName, AttributeNamespace, AttributeValue,
};
use prism_core::language::syntax::{Position, SourceRange};
use prism_ui_runtime::interpret::{
    lower_document_with_scope, LowerScope, TagEmission, TagResolver,
};
use prism_ui_runtime::layout::Node as UiNode;
use serde_json::Value;

use crate::props::{PropCtx, PropEmission, ShellPropBindings};

/// Pre-parsed `app.prism-ui` skeleton. Held once at boot — every
/// frame clones the AST, merges emissions into attributes, and lowers.
#[derive(Clone)]
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
    let scope = LowerScope::default()
        .with_resolver(resolver)
        .with_host_children_by_tag(host_children)
        .with_tag_emissions(tag_emissions);
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
    use crate::components::{register_shell_builtins, ShellComponentRegistry};
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
        }
    }

    #[test]
    fn skeleton_loads_from_disk() {
        let skel = Skeleton::load().expect("parse app.prism-ui");
        assert!(!skel.doc.nodes.is_empty());
    }

    #[test]
    fn render_tree_lowers_full_skeleton_through_resolver() {
        let skel = Skeleton::load().expect("parse");
        let bindings = ShellPropBindings::with_builtins();
        let mut reg = ShellComponentRegistry::new();
        register_shell_builtins(&mut reg).expect("register");
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
        let skel = Skeleton::load().expect("parse");
        let bindings = ShellPropBindings::with_builtins();
        let mut reg = ShellComponentRegistry::new();
        register_shell_builtins(&mut reg).expect("register");
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
