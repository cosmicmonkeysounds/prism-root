//! Backend-neutral input events. Winit produces native events on
//! desktop and web; this module normalises them into Prism's
//! `Event` value, which is then dispatched into the existing
//! `prism_builder::signal::dispatch_signal` machinery.

use serde::{Deserialize, Serialize};

use crate::layout::Surface;

/// Boxed callback the runtime invokes on every translated input
/// event. Hosts use this to mutate the [`Surface`] (which marks it
/// dirty and triggers a re-layout on the next frame) and to forward
/// events into `prism_builder::signal::dispatch_signal`. `'static`
/// because the wasm runtime requires `App: 'static` for
/// `spawn_app`; native respects the same bound for parity.
pub type EventHandler = Box<dyn FnMut(&Event, &mut Surface) + 'static>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Event {
    PointerMove {
        x: f32,
        y: f32,
    },
    PointerDown {
        x: f32,
        y: f32,
        button: PointerButton,
    },
    PointerUp {
        x: f32,
        y: f32,
        button: PointerButton,
    },
    Wheel {
        dx: f32,
        dy: f32,
    },
    Key {
        code: String,
        pressed: bool,
        modifiers: Modifiers,
    },
    Text {
        text: String,
    },
    Resize {
        width: u32,
        height: u32,
    },
    Focus {
        gained: bool,
    },
    /// IME composition is open — usually triggers a UI hint (the OS
    /// composition window will be positioned over the active caret).
    /// Hosts use this to switch from "raw key handling" to "preedit
    /// display" mode.
    ImeEnabled,
    /// IME composition closed — drop any preedit state.
    ImeDisabled,
    /// Preedit text — the partial composition the user is in the
    /// middle of typing (Japanese / Chinese / Korean kana, dead-key
    /// accent stacks, etc.). The host displays this in-line at the
    /// caret, usually with an underline. Replacing a previous
    /// preedit; an empty `text` clears the preedit. `cursor_byte` is
    /// the OS-reported caret position *within* the preedit string,
    /// in byte offsets; `None` means "no cursor inside preedit".
    ImePreedit {
        text: String,
        cursor_byte: Option<usize>,
    },
    /// Commit — the IME has finalised a composition. The host
    /// inserts `text` at the caret as a normal edit and clears any
    /// active preedit. This is the *normal* path most printable
    /// keystrokes flow through on platforms where winit reports
    /// IME (macOS + recent Linux) — the `Event::Text` arm covers
    /// dead-key-free typing on Windows.
    ImeCommit {
        text: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PointerButton {
    Primary,
    Secondary,
    Middle,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Modifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub meta: bool,
}
