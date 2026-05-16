//! `TextInputDeclaration` — declarative spec for one text-input surface.
//!
//! Each surface (command palette query, search overlay, an app's
//! bespoke filter box, the IDE-mode jump-to-symbol field, …) is one
//! declaration. The fields are all function pointers so a declaration
//! is `'static` and the registry can store them in a `&[TextInputDeclaration]`
//! slice with no allocation.
//!
//! ## What goes in a declaration
//!
//! * **Identity** — a stable id so logs / tests can name the surface.
//! * **Slot access** — `read` / `write` fn pointers projecting the
//!   `TextEditor` out of `AppState`. The dispatch borrows `&mut TextEditor`
//!   through `write` before calling the primitive.
//! * **Activation** — `is_active(&AppState) -> bool`. The service
//!   walks declarations in registration order and dispatches to the
//!   first active one. Returning `false` means "this surface isn't
//!   accepting input right now."
//! * **Hooks** — optional `on_buffer_change` / `on_display_change` /
//!   `on_commit` / `on_cancel` callbacks. The buffer-change hook is
//!   the "post-mutation work" seam (flush to prop, refilter results,
//!   mark tab dirty). Commit / cancel fire on plain Enter / Escape
//!   when the declaration's bindings list those as passthrough plain
//!   keys.
//! * **Bindings** — [`TextInputBindings`] knob struct controlling
//!   which modifier-bearing and plain keys the dispatch surfaces back
//!   to the caller vs handles itself.
//! * **Modal capture** — when set, the declaration swallows every
//!   keystroke that would otherwise reach `InputService` (Ctrl+S
//!   etc.). The four shipping modal-overlay declarations all set this.
//!
//! ## Builder ergonomics
//!
//! [`TextInputDeclaration::builder`] returns a [`TextInputDeclarationBuilder`]
//! with sensible defaults (`is_active = always`, no hooks, no modal
//! capture, plain-keys passthrough empty). Each setter consumes self
//! and returns the builder so a declaration reads as one expression.

use crate::services::MutCtx;
use crate::AppState;

use super::dispatch::TextInputBindings;

/// One text-input surface, fully described as a `'static` row. The
/// [`DeclarativeTextInputService`](super::service::DeclarativeTextInputService)
/// consumes a slice of these and dispatches events to whichever is active.
#[derive(Clone, Copy)]
pub struct TextInputDeclaration {
    pub id: &'static str,
    pub read: fn(&AppState) -> &prism_ui_runtime::editor::TextEditor,
    pub write: fn(&mut AppState) -> &mut prism_ui_runtime::editor::TextEditor,
    pub is_active: fn(&AppState) -> bool,
    pub on_buffer_change: Option<for<'a> fn(&mut MutCtx<'a>)>,
    pub on_display_change: Option<for<'a> fn(&mut MutCtx<'a>)>,
    pub on_commit: Option<for<'a> fn(&mut MutCtx<'a>)>,
    pub on_cancel: Option<for<'a> fn(&mut MutCtx<'a>)>,
    pub bindings: TextInputBindings<'static>,
    /// When true, the dispatch swallows every key while this surface
    /// is active — even Ctrl+S that the editor doesn't claim. The
    /// modal-overlay surfaces (palette, search) set this so a global
    /// shortcut doesn't fire mid-typing. Non-modal inputs (an
    /// always-rendered filter box) leave this false so Ctrl+S still
    /// saves while the field is focused.
    pub modal_capture: bool,
}

impl TextInputDeclaration {
    /// Start a declaration with safe defaults. Two getters are
    /// required to identify the slot; everything else has a default
    /// the builder lets you override. Returns the builder rather
    /// than `Self` so the call site chains methods directly —
    /// `TextInputDeclaration::builder(...).active_when(...).build()`.
    pub const fn builder(
        id: &'static str,
        read: fn(&AppState) -> &prism_ui_runtime::editor::TextEditor,
        write: fn(&mut AppState) -> &mut prism_ui_runtime::editor::TextEditor,
    ) -> TextInputDeclarationBuilder {
        TextInputDeclarationBuilder {
            decl: TextInputDeclaration {
                id,
                read,
                write,
                is_active: always_active,
                on_buffer_change: None,
                on_display_change: None,
                on_commit: None,
                on_cancel: None,
                bindings: TextInputBindings {
                    passthrough_modifier_keys: &[],
                    passthrough_plain_keys: &[],
                },
                modal_capture: false,
            },
        }
    }
}

fn always_active(_: &AppState) -> bool {
    true
}

/// Chainable builder over [`TextInputDeclaration`]. Each method
/// consumes `self` and returns the updated builder so a declaration
/// composes left-to-right.
#[derive(Clone, Copy)]
pub struct TextInputDeclarationBuilder {
    decl: TextInputDeclaration,
}

impl TextInputDeclarationBuilder {
    /// Limit when this declaration sees events. Default = always.
    pub const fn active_when(mut self, f: fn(&AppState) -> bool) -> Self {
        self.decl.is_active = f;
        self
    }

    /// Hook fired after any buffer-mutating event (`Text`, IME
    /// commit, Backspace, Ctrl+V, …). The dispatch hands you a
    /// `MutCtx` so you can flush-to-prop, mark dirty, refilter, etc.
    pub const fn on_buffer_change(mut self, f: for<'a> fn(&mut MutCtx<'a>)) -> Self {
        self.decl.on_buffer_change = Some(f);
        self
    }

    /// Hook fired after a caret/selection/IME-preedit move (display
    /// invalidates, buffer text unchanged). Skip if you don't need a
    /// per-tick redraw beyond what the dispatch already triggers.
    pub const fn on_display_change(mut self, f: for<'a> fn(&mut MutCtx<'a>)) -> Self {
        self.decl.on_display_change = Some(f);
        self
    }

    /// Plain-Enter handler. Setting this implicitly adds `"enter"` /
    /// `"return"` to the passthrough plain keys so the dispatch
    /// surfaces them as [`super::dispatch::TextInputOutcome::Ignored`]
    /// for this hook to run.
    pub const fn on_commit(mut self, f: for<'a> fn(&mut MutCtx<'a>)) -> Self {
        self.decl.on_commit = Some(f);
        self
    }

    /// Plain-Escape handler. Setting this implicitly adds `"escape"`
    /// to the passthrough plain keys.
    pub const fn on_cancel(mut self, f: for<'a> fn(&mut MutCtx<'a>)) -> Self {
        self.decl.on_cancel = Some(f);
        self
    }

    /// Modifier-bearing keys (Ctrl/Cmd + …) the dispatch should pass
    /// through so an outer service can claim them. e.g. a code editor
    /// uses this to let `Ctrl+S` reach the save command.
    pub const fn passthrough_modifier(mut self, keys: &'static [&'static str]) -> Self {
        self.decl.bindings.passthrough_modifier_keys = keys;
        self
    }

    /// Plain (non-modifier) keys the dispatch should surface as
    /// `Ignored` so the declaration's commit/cancel/etc. hooks fire.
    /// Modal overlays add `arrowup`/`arrowdown` here to route result
    /// navigation through the command table.
    pub const fn passthrough_plain(mut self, keys: &'static [&'static str]) -> Self {
        self.decl.bindings.passthrough_plain_keys = keys;
        self
    }

    /// Mark this surface as modal — Ctrl+S etc. that the editor
    /// doesn't claim are swallowed rather than passed to the global
    /// shortcut router.
    pub const fn modal(mut self) -> Self {
        self.decl.modal_capture = true;
        self
    }

    /// Finish building. Returns the `'static`-shaped declaration row.
    pub const fn build(self) -> TextInputDeclaration {
        self.decl
    }
}
