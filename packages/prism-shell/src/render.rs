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
use prism_ui_runtime::interpret::{lower_document_with_scope, LowerScope, TagResolver};
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
///   `bindings.snapshot(ctx)`            — typed substate → JSON props
///   → `fill_compositions(skeleton, e)`  — merge into AST attributes
///   → `lower_document_with_scope(...)`  — resolver dispatches per tag
pub fn render_tree(
    skeleton: &Skeleton,
    bindings: &ShellPropBindings,
    resolver: Arc<dyn TagResolver>,
    ctx: &PropCtx,
) -> Vec<UiNode> {
    let emissions = bindings.snapshot(ctx);
    let doc = fill_compositions(skeleton, &emissions);
    let scope = LowerScope::default().with_resolver(resolver);
    lower_document_with_scope(&doc, &scope)
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
