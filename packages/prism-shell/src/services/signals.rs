//! `SignalsService` — runtime dispatch of the connections authored
//! in the builder. Reaches Luau through `MutCtx::luau` (resource
//! seam) for `Custom` action handlers; everything else mutates
//! `state.canvas.document` directly.
//!
//! `EmitSignal` cascades through this same dispatcher; the depth
//! cap is local to one call so re-entrancy stays bounded.

use serde_json::Value;

use crate::cmd;
use crate::services::{CommandSpec, MutCtx, ShellService};

const MAX_CASCADE_DEPTH: u32 = 8;

#[derive(Default)]
pub struct SignalsService;

impl ShellService for SignalsService {
    fn id(&self) -> &'static str {
        "signals"
    }

    fn commands(&self) -> Vec<CommandSpec> {
        vec![
            // `signals.fire` is a meta-command — the host invokes it
            // with `(signal, source-node, payload)` packed into the
            // pointer/click site. The simple form is exposed for the
            // command palette so authors can manually fire by id.
            cmd!(
                "signals.fire-mounted",
                "Fire mounted on selection",
                "Tools",
                |ctx| {
                    let Some(id) = ctx.state.canvas.selection.clone() else {
                        return;
                    };
                    fire_signal(ctx, &id, "mounted", &Value::Null, 0);
                }
            ),
        ]
    }
}

/// The single dispatch entry. `pointer-down`, `clicked`, etc. all
/// route through here. Returns the number of connections fired —
/// callers ignore unless they specifically care.
pub fn fire_signal(
    ctx: &mut MutCtx<'_>,
    source_node: &str,
    signal: &str,
    payload: &Value,
    depth: u32,
) -> usize {
    if depth >= MAX_CASCADE_DEPTH {
        return 0;
    }
    let connections: Vec<_> = ctx
        .state
        .canvas
        .document
        .connections
        .iter()
        .filter(|c| c.source_node == source_node && c.signal == signal)
        .cloned()
        .collect();
    let n = connections.len();
    for c in connections {
        apply_action(ctx, &c, payload, depth + 1);
    }
    n
}

fn apply_action(
    ctx: &mut MutCtx<'_>,
    conn: &prism_builder::Connection,
    payload: &Value,
    depth: u32,
) {
    use prism_builder::ActionKind;
    match &conn.action {
        ActionKind::SetProperty { key, value } => {
            let property = key;
            if let Some(target) = ctx
                .state
                .canvas
                .document
                .root
                .as_mut()
                .and_then(|r| r.find_mut(&conn.target_node))
            {
                if let Value::Object(map) = &mut target.props {
                    map.insert(property.clone(), value.clone());
                } else {
                    target.props =
                        Value::Object([(property.clone(), value.clone())].into_iter().collect());
                }
            }
        }
        ActionKind::ToggleVisibility => {
            if let Some(target) = ctx
                .state
                .canvas
                .document
                .root
                .as_mut()
                .and_then(|r| r.find_mut(&conn.target_node))
            {
                let cur = target
                    .props
                    .get("visible")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true);
                if let Value::Object(map) = &mut target.props {
                    map.insert("visible".into(), Value::Bool(!cur));
                }
            }
        }
        ActionKind::EmitSignal { signal } => {
            let target = conn.target_node.clone();
            let signal = signal.clone();
            fire_signal(ctx, &target, &signal, payload, depth);
        }
        ActionKind::NavigateTo { .. } | ActionKind::PlayAnimation { .. } => {
            // Page navigation flows through the workspace slot —
            // no-op here until that mutator lands.
        }
        ActionKind::Custom { handler } => {
            // Cross-resource reach: Luau via `MutCtx::luau`.
            let _ = ctx.luau.exec(handler, payload);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::vfs::test_support::InMemVfs;
    use crate::services::{NoopLuauHost, ServiceRegistry, UndoStack};
    use crate::AppState;
    use prism_ui_runtime::layout::Viewport;

    #[test]
    fn fire_with_no_connections_is_zero() {
        let mut state = AppState::default();
        let mut undo = UndoStack::default();
        let mut vfs = InMemVfs::default();
        let mut luau = NoopLuauHost::default();
        let _reg = ServiceRegistry::with_builtins();
        let mut ctx = MutCtx {
            state: &mut state,
            viewport: Viewport {
                width: 0.0,
                height: 0.0,
            },
            undo: &mut undo,
            vfs: &mut vfs,
            luau: &mut luau,
        };
        assert_eq!(fire_signal(&mut ctx, "n", "clicked", &Value::Null, 0), 0);
    }
}
