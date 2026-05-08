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
