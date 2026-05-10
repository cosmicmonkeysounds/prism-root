//! `MenuService` — open / close / activate dropdown and
//! context menus. The slot data (`state.menus.{dropdown,context}`)
//! is read by the §21 menu bindings; this service is the
//! mutator side.
//!
//! Activation flows through the existing command table — a menu
//! item carries its `command` id, and `menu.activate` runs that
//! command after closing the menu. One mutator, one carrier, no
//! per-item glue.

use crate::cmd;
use crate::services::{CommandSpec, ShellService};

#[derive(Default)]
pub struct MenuService;

impl ShellService for MenuService {
    fn id(&self) -> &'static str {
        "menu"
    }

    fn commands(&self) -> Vec<CommandSpec> {
        vec![
            cmd!("menu.close", "Close menus", "View", "Escape", |ctx| {
                ctx.state.menus.dropdown.clear();
                ctx.state.menus.context.clear();
            }),
            // `menu.activate-N` — the host's hover/click handler
            // writes `state.menus.dropdown[N].command` (or context)
            // into a one-shot field on MenuSlot, then dispatches this
            // generic handler. The simpler form fires the *first*
            // pending item; a future iteration adds an index field on
            // MenuSlot when more than one menu can be hot at once.
        ]
    }
}

#[cfg(test)]
mod tests {
    use crate::services::vfs::test_support::InMemVfs;
    use crate::services::{MutCtx, NoopLuauHost, ServiceRegistry, UndoStack};
    use crate::state::MenuItem;
    use crate::AppState;
    use prism_ui_runtime::layout::Viewport;

    #[test]
    fn close_clears_both_menus() {
        let mut state = AppState::default();
        state.menus.dropdown.push(MenuItem {
            label: "x".into(),
            shortcut: None,
            command: None,
            separator: false,
            enabled: true,
        });
        state.menus.context.push(MenuItem::separator());
        let mut undo = UndoStack::default();
        let mut vfs = InMemVfs::default();
        let mut luau = NoopLuauHost::default();
        let reg = ServiceRegistry::with_builtins();
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
        assert!(reg.commands().run("menu.close", &mut ctx));
        assert!(state.menus.dropdown.is_empty() && state.menus.context.is_empty());
    }
}
