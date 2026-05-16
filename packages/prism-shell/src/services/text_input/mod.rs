//! Text-input plumbing — the shell-wide seam every `TextEditor`-backed
//! surface flows through.
//!
//! ## Three layers
//!
//! 1. **Primitive** ([`dispatch_text_input`]) — pure `Event →
//!    TextEditor` plumbing. Handles `Text`, the IME quartet, Ctrl/Cmd
//!    clipboard combos, and arbitrary editor keys. Returns a
//!    [`TextInputOutcome`] enum describing what happened; the caller
//!    runs whatever follow-up belongs to its surface (flush-to-prop,
//!    mark-dirty, rebuild-results).
//!
//! 2. **Declarative spec** ([`TextInputDeclaration`]) — one row per
//!    text-input surface. Pairs a slot getter, an `is_active`
//!    predicate, optional commit/cancel/buffer-change hooks, and a
//!    `TextInputBindings` knob struct. Built through a chainable
//!    [`TextInputDeclaration::builder`] entry so the eight or so knobs
//!    don't bloat the call site. Adding a new text-input surface to
//!    the shell is one of these declarations; the registry handles
//!    event routing, modal capture, and the dispatch fan-out.
//!
//! 3. **Service** ([`DeclarativeTextInputService`]) — iterates the
//!    declarations in priority order, dispatches events through the
//!    primitive, runs the per-surface hooks. The single service
//!    handles every declared text-input surface (palette, search,
//!    and any apps' bespoke inputs).
//!
//! `FieldFocusService` and `CodeEditorService` remain bespoke — they
//! have shape-specific side state (number-drag, scroll viewport,
//! language-aware comment toggle, tab dirty marking) that doesn't
//! collapse onto a declaration. Future bespoke surfaces (rich-text
//! formatters, multi-cursor editors) can layer on the same seam.

mod declaration;
mod dispatch;
mod service;

pub use declaration::{TextInputDeclaration, TextInputDeclarationBuilder};
pub use dispatch::{dispatch_text_input, TextInputBindings, TextInputOutcome};
pub use service::{builtin_declarations, DeclarativeTextInputService};
