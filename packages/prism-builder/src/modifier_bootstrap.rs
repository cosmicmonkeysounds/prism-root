//! Wave 1.7 of `docs/dev/composable-builder-plan.md`: bootstrap
//! behaviour set on top of the six baseline `ModifierKind` variants.
//!
//! Six small `ModifierBehaviour` impls that immediately put the
//! composable Inspector to work without waiting for the Wave 11 DSL
//! migration:
//!
//! | id                  | what it does                                                     |
//! |---------------------|------------------------------------------------------------------|
//! | `visible`           | When `visible = false`, the node renders as a zero-sized box.    |
//! | `locked`            | Marks `aria-disabled="true"` so the §43 D pointer routes skip it.|
//! | `hover`             | Wraps the child in a hover-aware container with `:hover` tint.   |
//! | `click`             | Emits `data-on-click="<action>"` so the §43 click router fires.  |
//! | `bind-to-selection` | Reactive bind one node's prop to the current selection (Wave 8). |
//! | `run-luau-script`   | Spawns a Luau handler on the bound signal (Wave 8).              |
//!
//! Five of the six have working `wrap` bodies today; the last two
//! (`bind-to-selection`, `run-luau-script`) ship as schema-only
//! placeholders that round-trip cleanly and surface the right shape
//! in the Inspector — Wave 8's Luau parity wires their effects.
//!
//! See `register_bootstrap_modifiers` for the entry point;
//! `ModifierRegistry::with_builtins` already chains it after the six
//! baseline kinds.

use std::borrow::Cow;

use prism_ui_runtime::layout::{ContainerProps, Node as UiNode, Semantic, Sizing};
use serde_json::Value;

use crate::modifier::{Modifier, ModifierBehaviour, ModifierId};
use crate::registry::FieldSpec;
use crate::ui_lower::hover_bg;

/// `visible = false` collapses the rendered subtree to a zero-sized
/// container so it disappears from layout entirely. Headless / SSR
/// callers see the same shape — useful for live "hide this draft"
/// edits in the canvas without deleting the node.
pub struct VisibleBehaviour;
impl ModifierBehaviour for VisibleBehaviour {
    fn id(&self) -> ModifierId {
        Cow::Borrowed("visible")
    }
    fn label(&self) -> &str {
        "Visible"
    }
    fn description(&self) -> &str {
        "Hide the node without removing it from the document."
    }
    fn schema(&self) -> Vec<FieldSpec> {
        vec![FieldSpec::boolean("visible", "Visible").with_default(Value::Bool(true))]
    }
    fn wrap(&self, modifier: &Modifier, child: UiNode) -> UiNode {
        let visible = modifier
            .props
            .get("visible")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        if visible {
            return child;
        }
        // Collapse to zero-size — preserves the parent's container
        // shape but contributes nothing to layout.
        UiNode::Container {
            id: format!(
                "{}::hidden",
                ui_node_id(&child).unwrap_or("modifier-visible")
            ),
            props: ContainerProps {
                width: Sizing::Fixed(0.0),
                height: Sizing::Fixed(0.0),
                semantic: Semantic::tag("div")
                    .with_attr("data-modifier", "visible")
                    .with_attr("data-visible", "false")
                    .with_attr("aria-hidden", "true"),
                ..ContainerProps::default()
            },
            children: vec![],
        }
    }
}

/// Marks the wrapped subtree as `aria-disabled="true"`. Today's
/// `POINTER_ROUTES` table doesn't yet read this attr to suppress
/// clicks — that lands when Wave 3's canvas gestures consult it —
/// but the SSR side is already correct.
pub struct LockedBehaviour;
impl ModifierBehaviour for LockedBehaviour {
    fn id(&self) -> ModifierId {
        Cow::Borrowed("locked")
    }
    fn label(&self) -> &str {
        "Locked"
    }
    fn description(&self) -> &str {
        "Disable interaction with this node and its descendants."
    }
    fn schema(&self) -> Vec<FieldSpec> {
        vec![FieldSpec::boolean("locked", "Locked").with_default(Value::Bool(true))]
    }
    fn wrap(&self, modifier: &Modifier, child: UiNode) -> UiNode {
        let locked = modifier
            .props
            .get("locked")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        if !locked {
            return child;
        }
        wrap_with_attrs(
            child,
            "locked",
            &[("data-modifier", "locked"), ("aria-disabled", "true")],
        )
    }
}

/// Wraps the child in a hover-aware container — `tint` is the
/// background applied on hover. Reuses the existing `hover_bg`
/// helper so the runtime's hover swap fires automatically when the
/// cursor enters the container.
pub struct HoverBehaviour;
impl ModifierBehaviour for HoverBehaviour {
    fn id(&self) -> ModifierId {
        Cow::Borrowed("hover")
    }
    fn label(&self) -> &str {
        "Hover"
    }
    fn description(&self) -> &str {
        "Visual feedback while the pointer is over the node."
    }
    fn schema(&self) -> Vec<FieldSpec> {
        vec![FieldSpec::text("tint", "Tint (CSS color)")
            .with_default(Value::String("#0f000000".into()))]
    }
    fn wrap(&self, modifier: &Modifier, child: UiNode) -> UiNode {
        let tint = modifier
            .props
            .get("tint")
            .and_then(|v| v.as_str())
            .unwrap_or("#0f000000");
        let hover = hover_bg(tint);
        let id = ui_node_id(&child).unwrap_or("modifier-hover").to_string();
        UiNode::Container {
            id: format!("{}::hover", id),
            props: ContainerProps {
                width: Sizing::Fit,
                height: Sizing::Fit,
                hover,
                semantic: Semantic::tag("div").with_attr("data-modifier", "hover"),
                ..ContainerProps::default()
            },
            children: vec![child],
        }
    }
}

/// Emits a `data-on-click` attr so the §43 `route_on_click` router
/// fires the configured action when the node is clicked. The action
/// string is the same grammar `on:click="emit save"` already speaks
/// — `emit <signal>`, `set <node>.<key> = <expr>`, `bind …`, etc.
pub struct ClickBehaviour;
impl ModifierBehaviour for ClickBehaviour {
    fn id(&self) -> ModifierId {
        Cow::Borrowed("click")
    }
    fn label(&self) -> &str {
        "Click"
    }
    fn description(&self) -> &str {
        "Run an inline action when the node is clicked."
    }
    fn schema(&self) -> Vec<FieldSpec> {
        vec![FieldSpec::text("action", "Action (e.g. `emit save`)").required()]
    }
    fn wrap(&self, modifier: &Modifier, child: UiNode) -> UiNode {
        let action = modifier
            .props
            .get("action")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if action.is_empty() {
            return child;
        }
        wrap_with_attrs(
            child,
            "click",
            &[
                ("data-modifier", "click"),
                ("data-on-click", action),
                ("role", "button"),
            ],
        )
    }
}

/// Schema-only behaviour: declare that one prop of this node binds
/// reactively to the current `selection`. The actual `Effect`
/// installation lands in Wave 8 (Luau parity wave) — for now the
/// modifier round-trips through the document and shows up in the
/// inspector.
pub struct BindToSelectionBehaviour;
impl ModifierBehaviour for BindToSelectionBehaviour {
    fn id(&self) -> ModifierId {
        Cow::Borrowed("bind-to-selection")
    }
    fn label(&self) -> &str {
        "Bind to Selection"
    }
    fn description(&self) -> &str {
        "One-way reactive bind from the current canvas selection to a prop."
    }
    fn schema(&self) -> Vec<FieldSpec> {
        vec![
            FieldSpec::text("source-key", "Selection key (e.g. `id`, `label`)").required(),
            FieldSpec::text("target-key", "Prop key on this node").required(),
        ]
    }
}

/// Schema-only behaviour: run a Luau script in response to a signal
/// on this node. The runtime hookup lands in Wave 8 (`LuauModifier`
/// plus `install_effects`). Until then, the modifier round-trips
/// through the inspector + document with the right shape so the
/// edit surface is in place.
pub struct RunLuauScriptBehaviour;
impl ModifierBehaviour for RunLuauScriptBehaviour {
    fn id(&self) -> ModifierId {
        Cow::Borrowed("run-luau-script")
    }
    fn label(&self) -> &str {
        "Run Luau Script"
    }
    fn description(&self) -> &str {
        "Execute a Luau script in response to a signal on this node."
    }
    fn schema(&self) -> Vec<FieldSpec> {
        vec![
            FieldSpec::text("handler", "Signal id (e.g. `clicked`, `value-changed`)").required(),
            FieldSpec::text("script", "Luau source").required(),
        ]
    }
}

/// Register the six bootstrap behaviours into an existing registry.
/// `ModifierRegistry::with_builtins` calls this after the six
/// baseline kinds (`scroll-overflow`, `hover-effect`, …).
pub fn register_bootstrap(
    reg: &mut crate::modifier::ModifierRegistry,
) -> Result<(), crate::modifier::ModifierRegistryError> {
    reg.register(std::sync::Arc::new(VisibleBehaviour))?;
    reg.register(std::sync::Arc::new(LockedBehaviour))?;
    reg.register(std::sync::Arc::new(HoverBehaviour))?;
    reg.register(std::sync::Arc::new(ClickBehaviour))?;
    reg.register(std::sync::Arc::new(BindToSelectionBehaviour))?;
    reg.register(std::sync::Arc::new(RunLuauScriptBehaviour))?;
    Ok(())
}

// ── helpers ──────────────────────────────────────────────────────

/// Extract a `UiNode`'s id, when it has one. Used to derive
/// per-modifier wrapper-container ids so trees render with stable
/// identity across re-renders.
fn ui_node_id(node: &UiNode) -> Option<&str> {
    match node {
        UiNode::Container { id, .. } => Some(id.as_str()),
        UiNode::Text { id, .. } => Some(id.as_str()),
        UiNode::Image { id, .. } => Some(id.as_str()),
        _ => None,
    }
}

/// Wrap a `UiNode` in a transparent container that carries a set of
/// semantic attributes. Used by `LockedBehaviour` / `ClickBehaviour`
/// to layer attrs onto an existing child without mutating its props.
fn wrap_with_attrs(child: UiNode, marker: &str, attrs: &[(&str, &str)]) -> UiNode {
    let id = ui_node_id(&child).unwrap_or("modifier").to_string();
    let mut sem = Semantic::tag("div");
    for (k, v) in attrs {
        sem = sem.with_attr(*k, *v);
    }
    UiNode::Container {
        id: format!("{}::{}", id, marker),
        props: ContainerProps {
            width: Sizing::Fit,
            height: Sizing::Fit,
            semantic: sem,
            ..ContainerProps::default()
        },
        children: vec![child],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modifier::{Modifier, ModifierRegistry};
    use serde_json::json;

    fn dummy_child() -> UiNode {
        UiNode::Container {
            id: "core".into(),
            props: ContainerProps::default(),
            children: vec![],
        }
    }

    #[test]
    fn visible_off_returns_zero_size_container() {
        let beh = VisibleBehaviour;
        let m = Modifier::new("visible").with_props(json!({ "visible": false }));
        let out = beh.wrap(&m, dummy_child());
        let UiNode::Container { props, .. } = &out else {
            panic!()
        };
        assert!(matches!(props.width, Sizing::Fixed(0.0)));
        assert!(props
            .semantic
            .attrs
            .iter()
            .any(|(k, v)| k == "aria-hidden" && v == "true"));
    }

    #[test]
    fn visible_on_is_pass_through() {
        let beh = VisibleBehaviour;
        let m = Modifier::new("visible").with_props(json!({ "visible": true }));
        let out = beh.wrap(&m, dummy_child());
        let UiNode::Container { id, .. } = &out else {
            panic!()
        };
        assert_eq!(id, "core"); // unwrapped
    }

    #[test]
    fn locked_wraps_with_aria_disabled() {
        let beh = LockedBehaviour;
        let m = Modifier::new("locked").with_props(json!({ "locked": true }));
        let out = beh.wrap(&m, dummy_child());
        let UiNode::Container {
            props, children, ..
        } = &out
        else {
            panic!()
        };
        assert!(props
            .semantic
            .attrs
            .iter()
            .any(|(k, v)| k == "aria-disabled" && v == "true"));
        assert_eq!(children.len(), 1);
    }

    #[test]
    fn hover_wraps_with_hover_override() {
        let beh = HoverBehaviour;
        let m = Modifier::new("hover").with_props(json!({ "tint": "#1f000000" }));
        let out = beh.wrap(&m, dummy_child());
        let UiNode::Container { props, .. } = &out else {
            panic!()
        };
        assert!(props.hover.is_some());
    }

    #[test]
    fn click_emits_data_on_click_attr() {
        let beh = ClickBehaviour;
        let m = Modifier::new("click").with_props(json!({ "action": "emit save" }));
        let out = beh.wrap(&m, dummy_child());
        let UiNode::Container { props, .. } = &out else {
            panic!()
        };
        assert!(props
            .semantic
            .attrs
            .iter()
            .any(|(k, v)| k == "data-on-click" && v == "emit save"));
    }

    #[test]
    fn click_empty_action_is_pass_through() {
        let beh = ClickBehaviour;
        let m = Modifier::new("click");
        let out = beh.wrap(&m, dummy_child());
        let UiNode::Container { id, .. } = &out else {
            panic!()
        };
        assert_eq!(id, "core");
    }

    #[test]
    fn bootstrap_registers_six_more_behaviours_alongside_baseline() {
        // `with_builtins` already chains `register_bootstrap`, so a
        // fresh `new()` registry plus an explicit `register_bootstrap`
        // is the cleanest way to assert the six bootstrap ids land
        // without double-registering them.
        let mut reg = ModifierRegistry::new();
        register_bootstrap(&mut reg).unwrap();
        assert_eq!(reg.len(), 6);
        for id in [
            "visible",
            "locked",
            "hover",
            "click",
            "bind-to-selection",
            "run-luau-script",
        ] {
            assert!(reg.contains(id), "bootstrap missing `{id}`");
        }
        // And `with_builtins()` is the union — baseline + bootstrap.
        let full = ModifierRegistry::with_builtins();
        assert_eq!(full.len(), 12);
    }

    #[test]
    fn schema_only_behaviours_pass_through_in_wrap() {
        // `bind-to-selection` and `run-luau-script` have no wrap bodies
        // yet — the default identity impl must keep the child intact.
        let m = Modifier::new("bind-to-selection");
        let out = BindToSelectionBehaviour.wrap(&m, dummy_child());
        let UiNode::Container { id, .. } = &out else {
            panic!()
        };
        assert_eq!(id, "core");
    }
}
