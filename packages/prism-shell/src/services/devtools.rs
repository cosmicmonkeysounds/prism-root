//! `DevToolsService` — IDE-mode Phase 4 / cross-cutting §4.3.
//!
//! Owns the command surface for the unified Inspector / DevTools
//! panel (`shell.devtools`) — four lenses (Document, Presence,
//! Probes, Bindings) in one tabbed surface. Event handling for the
//! filter field is declarative: a single [`TextInputDeclaration`]
//! in [`text_input::builtin_declarations`](super::text_input) drives
//! it through the shared dispatch primitive. Tab clicks are routed
//! by the pointer-route table (see `events.rs::handle_devtools_tab_click`).
//!
//! Future wiring this service will own:
//! * `data-probe-*` pointer-hit → `state.devtools.record_probe(...)`
//!   (cross-cutting §4.3 / fusion G.2).
//! * `PresenceManager::on_change` → `state.devtools.presence` refresh
//!   (parallel to `network::presence`).
//!
//! For now, the buffers populate via tests + scenes; the service
//! contributes only the commands that operate on the slot.

use crate::cmd;
use crate::services::{CommandSpec, ShellService};
use crate::state::DevToolsLens;

#[derive(Default)]
pub struct DevToolsService;

impl ShellService for DevToolsService {
    fn id(&self) -> &'static str {
        "devtools"
    }

    fn commands(&self) -> Vec<CommandSpec> {
        vec![
            cmd!(
                "devtools.show-document",
                "DevTools: Document Lens",
                "View",
                |ctx| {
                    ctx.state.devtools.switch_lens(DevToolsLens::Document);
                }
            ),
            cmd!(
                "devtools.show-presence",
                "DevTools: Presence Lens",
                "View",
                |ctx| {
                    ctx.state.devtools.switch_lens(DevToolsLens::Presence);
                }
            ),
            cmd!(
                "devtools.show-probes",
                "DevTools: Probes Lens",
                "View",
                |ctx| {
                    ctx.state.devtools.switch_lens(DevToolsLens::Probes);
                }
            ),
            cmd!(
                "devtools.show-bindings",
                "DevTools: Bindings Lens",
                "View",
                |ctx| {
                    ctx.state.devtools.switch_lens(DevToolsLens::Bindings);
                }
            ),
            cmd!(
                "devtools.clear-probes",
                "DevTools: Clear Probe Buffer",
                "View",
                |ctx| {
                    ctx.state.devtools.clear_probes();
                }
            ),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::vfs::test_support::InMemVfs;
    use crate::services::{Clipboard, MutCtx, NoopLuauHost, ServiceRegistry, UndoStack};
    use crate::AppState;
    use prism_ui_runtime::layout::Viewport;

    fn run(state: &mut AppState, id: &str) -> bool {
        let reg = ServiceRegistry::with_builtins();
        let mut undo = UndoStack::default();
        let mut vfs = InMemVfs::default();
        let mut luau = NoopLuauHost::default();
        let mut clipboard = Clipboard::default();
        let mut ctx = MutCtx {
            state,
            viewport: Viewport {
                width: 0.0,
                height: 0.0,
            },
            undo: &mut undo,
            vfs: &mut vfs,
            luau: &mut luau,
            clipboard: &mut clipboard,
            registry: None,
            modifier_registry: None,
        };
        reg.commands().run(id, &mut ctx)
    }

    #[test]
    fn switch_lens_commands_change_active_lens() {
        let mut state = AppState::default();
        assert_eq!(state.devtools.active_lens, DevToolsLens::Document);
        run(&mut state, "devtools.show-probes");
        assert_eq!(state.devtools.active_lens, DevToolsLens::Probes);
        run(&mut state, "devtools.show-bindings");
        assert_eq!(state.devtools.active_lens, DevToolsLens::Bindings);
        run(&mut state, "devtools.show-presence");
        assert_eq!(state.devtools.active_lens, DevToolsLens::Presence);
        run(&mut state, "devtools.show-document");
        assert_eq!(state.devtools.active_lens, DevToolsLens::Document);
    }

    #[test]
    fn clear_probes_empties_the_buffer() {
        use crate::state::ProbeEvent;
        let mut state = AppState::default();
        state.devtools.record_probe(ProbeEvent {
            name: "click".into(),
            payload: serde_json::Value::Null,
            timestamp_ms: 0,
            source_node_id: None,
        });
        assert_eq!(state.devtools.probes.len(), 1);
        run(&mut state, "devtools.clear-probes");
        assert!(state.devtools.probes.is_empty());
    }
}
