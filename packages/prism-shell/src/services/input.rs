//! `InputService` — the *only* place that converts a runtime
//! `Event::Key` into a command id. Owns a stack of input schemes
//! (apps push their own on entry, pop on exit); the top-most scheme
//! that matches the incoming combo wins, and the resolved command
//! id is dispatched through [`CommandTable`] in the same call.
//!
//! The legacy `keyboard.rs` / `keybindings.rs` / `input.rs` trio
//! collapses into this single service because every consumer
//! ultimately wanted the same thing: "translate a key event into
//! a command id that the rest of the host already knows how to
//! run." The scheme-stack model (ADR-005) is preserved as the
//! mutable inner of the service.

use std::collections::HashMap;
use std::sync::Mutex;

use prism_ui_runtime::event::{Event, Modifiers};

use crate::services::{CommandTable, EventOutcome, MutCtx, ShellService};

// ── KeyCombo (focused, in-tree) ───────────────────────────────────

/// Lower-cased key + modifier flags. The legacy `keyboard::KeyCombo`
/// carried the same shape; this in-service copy keeps the input
/// surface self-contained so the legacy module can drop without
/// pulling in the rest of the keyboard model.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct KeyCombo {
    pub key: String,
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub meta: bool,
}

impl KeyCombo {
    pub fn parse(s: &str) -> Option<Self> {
        if s.is_empty() {
            return None;
        }
        let mut combo = KeyCombo {
            key: String::new(),
            ctrl: false,
            shift: false,
            alt: false,
            meta: false,
        };
        for part in s.split('+').map(str::trim) {
            match part.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => combo.ctrl = true,
                "shift" => combo.shift = true,
                "alt" | "option" => combo.alt = true,
                "meta" | "cmd" | "command" | "super" | "win" => combo.meta = true,
                key => combo.key = key.to_string(),
            }
        }
        if combo.key.is_empty() {
            None
        } else {
            Some(combo)
        }
    }

    pub fn from_event(code: &str, mods: Modifiers) -> Self {
        Self {
            key: code.to_ascii_lowercase(),
            ctrl: mods.ctrl,
            shift: mods.shift,
            alt: mods.alt,
            meta: mods.meta,
        }
    }
}

// ── input scheme (declarative) ────────────────────────────────────

/// A named layer of `combo → command-id` bindings. Apps and modal
/// overlays construct one and push it onto the service's stack;
/// resolution walks the stack top-down and returns on the first
/// match.
#[derive(Default, Clone)]
pub struct InputScheme {
    pub id: &'static str,
    bindings: HashMap<KeyCombo, &'static str>,
}

impl InputScheme {
    pub fn new(id: &'static str) -> Self {
        Self {
            id,
            bindings: HashMap::new(),
        }
    }

    /// Add one binding. Panics on duplicate combo within the same
    /// scheme — drift between two declarations of the same key is
    /// a programmer error, never a runtime branch.
    pub fn bind(mut self, combo: &str, command: &'static str) -> Self {
        let combo = KeyCombo::parse(combo).expect("invalid combo in InputScheme::bind");
        assert!(
            !self.bindings.contains_key(&combo),
            "duplicate combo `{:?}` in scheme `{}`",
            combo,
            self.id
        );
        self.bindings.insert(combo, command);
        self
    }

    pub fn resolve(&self, combo: &KeyCombo) -> Option<&'static str> {
        self.bindings.get(combo).copied()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&KeyCombo, &&'static str)> {
        self.bindings.iter()
    }
}

// ── service ──────────────────────────────────────────────────────

pub struct InputService {
    /// Top of stack is the last-pushed (most-recent) scheme. Mutex
    /// because `ShellService` is `Send + Sync` and stored as
    /// `Arc<dyn>` — push/pop is rare so contention is a non-issue.
    schemes: Mutex<Vec<InputScheme>>,
}

impl Default for InputService {
    fn default() -> Self {
        Self {
            schemes: Mutex::new(Vec::new()),
        }
    }
}

impl InputService {
    /// The shell-global scheme: every shortcut declared by every
    /// builtin service maps back to its command id here. Adding a
    /// service that contributes a new shortcut means *one* row in
    /// this method (alongside the `commands()` declaration on the
    /// service itself).
    pub fn with_defaults() -> Self {
        let svc = Self::default();
        let base = InputScheme::new("shell.base")
            .bind("ctrl+z", "edit.undo")
            .bind("ctrl+shift+z", "edit.redo")
            .bind("ctrl+shift+p", "palette.toggle")
            .bind("escape", "palette.close")
            // §25 — selection / clipboard. Esc routes to palette.close
            // first; `selection.clear` is reachable through the palette
            // and via host-pushed schemes (e.g. tablet mode).
            .bind("up", "selection.move-up")
            .bind("down", "selection.move-down")
            .bind("left", "selection.move-left")
            .bind("right", "selection.move-right")
            .bind("ctrl+c", "clipboard.copy")
            .bind("ctrl+x", "clipboard.cut")
            .bind("ctrl+v", "clipboard.paste")
            .bind("ctrl+d", "clipboard.duplicate")
            // §26 — IO services (Persistence / Project / Search).
            .bind("ctrl+n", "file.new")
            .bind("ctrl+s", "file.save")
            .bind("ctrl+shift+s", "file.save-as")
            .bind("ctrl+o", "file.open")
            .bind("ctrl+shift+o", "project.open-folder")
            .bind("ctrl+f", "search.open");
        svc.schemes.lock().expect("schemes lock").push(base);
        svc
    }

    pub fn push_scheme(&self, scheme: InputScheme) {
        self.schemes.lock().expect("schemes lock").push(scheme);
    }

    pub fn pop_scheme(&self, id: &str) -> Option<InputScheme> {
        let mut stack = self.schemes.lock().expect("schemes lock");
        let idx = stack.iter().rposition(|s| s.id == id)?;
        Some(stack.remove(idx))
    }

    pub fn resolve(&self, combo: &KeyCombo) -> Option<&'static str> {
        let stack = self.schemes.lock().expect("schemes lock");
        for scheme in stack.iter().rev() {
            if let Some(id) = scheme.resolve(combo) {
                return Some(id);
            }
        }
        None
    }
}

impl ShellService for InputService {
    fn id(&self) -> &'static str {
        "input"
    }

    fn on_event(&self, event: &Event, ctx: &mut MutCtx<'_>, cmds: &CommandTable) -> EventOutcome {
        let Event::Key {
            code,
            pressed,
            modifiers,
        } = event
        else {
            return EventOutcome::Pass;
        };
        if !*pressed {
            return EventOutcome::Pass;
        }
        let combo = KeyCombo::from_event(code, *modifiers);
        let Some(id) = self.resolve(&combo) else {
            return EventOutcome::Pass;
        };
        if cmds.run(id, ctx) {
            EventOutcome::Handled
        } else {
            EventOutcome::Pass
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::{
        vfs::test_support::InMemVfs, Clipboard, NoopLuauHost, ServiceRegistry, UndoStack,
    };
    use crate::AppState;
    use prism_ui_runtime::layout::Viewport;

    fn key(code: &str, ctrl: bool, shift: bool) -> Event {
        Event::Key {
            code: code.into(),
            pressed: true,
            modifiers: Modifiers {
                ctrl,
                shift,
                ..Default::default()
            },
        }
    }

    #[test]
    fn parse_round_trip() {
        let c = KeyCombo::parse("Ctrl+Shift+P").unwrap();
        assert_eq!(c.key, "p");
        assert!(c.ctrl && c.shift);
        assert!(!c.alt && !c.meta);
    }

    #[test]
    fn key_event_routes_through_input_service_to_command_runner() {
        // §24 keystone end-to-end test: Event::Key → fan_out →
        // InputService → CommandTable → UndoRedoService handler →
        // MutCtx → AppState mutation.
        let mut state = AppState::default();
        state.chrome.status = "v0".into();
        let mut undo = UndoStack::default();
        undo.snapshot(&state);
        state.chrome.status = "v1".into();
        let reg = ServiceRegistry::with_builtins();
        let mut vfs = InMemVfs::default();
        let mut luau = NoopLuauHost::default();
        let mut clipboard = Clipboard::default();
        let mut ctx = MutCtx {
            state: &mut state,
            viewport: Viewport {
                width: 0.0,
                height: 0.0,
            },
            undo: &mut undo,
            vfs: &mut vfs,
            luau: &mut luau,
            clipboard: &mut clipboard,
            registry: None,
        };
        assert_eq!(
            reg.fan_out(&key("z", true, false), &mut ctx),
            EventOutcome::Handled
        );
        assert_eq!(state.chrome.status, "v0");
    }

    #[test]
    fn key_release_is_pass_through() {
        let svc = InputService::with_defaults();
        let mut state = AppState::default();
        let mut undo = UndoStack::default();
        let mut vfs = InMemVfs::default();
        let mut luau = NoopLuauHost::default();
        let mut clipboard = Clipboard::default();
        let mut ctx = MutCtx {
            state: &mut state,
            viewport: Viewport {
                width: 0.0,
                height: 0.0,
            },
            undo: &mut undo,
            vfs: &mut vfs,
            luau: &mut luau,
            clipboard: &mut clipboard,
            registry: None,
        };
        let release = Event::Key {
            code: "z".into(),
            pressed: false,
            modifiers: Modifiers {
                ctrl: true,
                ..Default::default()
            },
        };
        let cmds = CommandTable::default();
        assert_eq!(svc.on_event(&release, &mut ctx, &cmds), EventOutcome::Pass);
    }

    #[test]
    fn pushed_scheme_overrides_base() {
        let svc = InputService::with_defaults();
        svc.push_scheme(InputScheme::new("test.override").bind("ctrl+z", "palette.toggle"));
        let combo = KeyCombo::parse("ctrl+z").unwrap();
        assert_eq!(svc.resolve(&combo), Some("palette.toggle"));
        svc.pop_scheme("test.override");
        assert_eq!(svc.resolve(&combo), Some("edit.undo"));
    }

    #[test]
    fn unknown_combo_passes_through() {
        let reg = ServiceRegistry::with_builtins();
        let mut state = AppState::default();
        let mut undo = UndoStack::default();
        let mut vfs = InMemVfs::default();
        let mut luau = NoopLuauHost::default();
        let mut clipboard = Clipboard::default();
        let mut ctx = MutCtx {
            state: &mut state,
            viewport: Viewport {
                width: 0.0,
                height: 0.0,
            },
            undo: &mut undo,
            vfs: &mut vfs,
            luau: &mut luau,
            clipboard: &mut clipboard,
            registry: None,
        };
        assert_eq!(
            reg.fan_out(&key("q", false, false), &mut ctx),
            EventOutcome::Pass
        );
    }

    #[test]
    fn commands_cover_every_registered_shortcut() {
        // §24.8 keystone parity. Every combo declared in the base
        // input scheme must resolve to a command id that the
        // aggregated `CommandTable` knows how to dispatch. Forgetting
        // to wire a command is a test failure here, never a silent
        // dead key in production.
        let svc = InputService::with_defaults();
        let reg = ServiceRegistry::with_builtins();
        let stack = svc.schemes.lock().expect("schemes lock");
        let base = stack.last().expect("base scheme");
        for (combo, cmd_id) in base.iter() {
            assert!(
                reg.commands().get(cmd_id).is_some(),
                "combo `{combo:?}` resolves to unknown command `{cmd_id}`"
            );
        }
    }

    #[test]
    fn palette_open_via_ctrl_shift_p() {
        let reg = ServiceRegistry::with_builtins();
        let mut state = AppState::default();
        let mut undo = UndoStack::default();
        let mut vfs = InMemVfs::default();
        let mut luau = NoopLuauHost::default();
        let mut clipboard = Clipboard::default();
        {
            let mut ctx = MutCtx {
                state: &mut state,
                viewport: Viewport {
                    width: 0.0,
                    height: 0.0,
                },
                undo: &mut undo,
                vfs: &mut vfs,
                luau: &mut luau,
                clipboard: &mut clipboard,
                registry: None,
            };
            assert_eq!(
                reg.fan_out(&key("p", true, true), &mut ctx),
                EventOutcome::Handled
            );
        }
        assert!(state.overlay.command_palette.open);
    }
}
