//! Translate `winit::event::WindowEvent`s into the backend-neutral
//! [`crate::event::Event`] vocabulary. Native + web backends share
//! this; the difference between them is window setup and the GL
//! context, not how a click maps onto a Prism event.

use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::keyboard::{Key, NamedKey};

use crate::event::{Event, Modifiers, PointerButton};

/// Per-window translation context. Tracks the latest pointer position
/// and modifier state so events that don't carry them (e.g. mouse
/// click without `position`) can still be filled in.
#[derive(Debug, Default, Clone, Copy)]
pub struct InputState {
    pub pointer: (f32, f32),
    pub modifiers: Modifiers,
}

/// Translate one winit event. Returns `None` for events that don't map
/// onto Prism's vocabulary (focus changes that aren't gain/loss,
/// IME-only events, the dozens of synthetic variants we don't act on).
pub fn translate(event: &WindowEvent, state: &mut InputState) -> Option<Event> {
    match event {
        WindowEvent::CursorMoved { position, .. } => {
            state.pointer = (position.x as f32, position.y as f32);
            Some(Event::PointerMove {
                x: state.pointer.0,
                y: state.pointer.1,
            })
        }
        WindowEvent::MouseInput {
            state: bs, button, ..
        } => {
            let pb = match button {
                MouseButton::Left => PointerButton::Primary,
                MouseButton::Right => PointerButton::Secondary,
                MouseButton::Middle => PointerButton::Middle,
                _ => return None,
            };
            Some(match bs {
                ElementState::Pressed => Event::PointerDown {
                    x: state.pointer.0,
                    y: state.pointer.1,
                    button: pb,
                },
                ElementState::Released => Event::PointerUp {
                    x: state.pointer.0,
                    y: state.pointer.1,
                    button: pb,
                },
            })
        }
        WindowEvent::MouseWheel { delta, .. } => {
            let (dx, dy) = match delta {
                MouseScrollDelta::LineDelta(x, y) => (*x * 16.0, *y * 16.0),
                MouseScrollDelta::PixelDelta(p) => (p.x as f32, p.y as f32),
            };
            Some(Event::Wheel { dx, dy })
        }
        WindowEvent::ModifiersChanged(m) => {
            let s = m.state();
            state.modifiers = Modifiers {
                shift: s.shift_key(),
                ctrl: s.control_key(),
                alt: s.alt_key(),
                meta: s.super_key(),
            };
            None
        }
        WindowEvent::KeyboardInput { event: ke, .. } => {
            let pressed = ke.state == ElementState::Pressed;
            let code = key_code(&ke.logical_key);
            Some(Event::Key {
                code,
                pressed,
                modifiers: state.modifiers,
            })
        }
        WindowEvent::Ime(winit::event::Ime::Commit(text)) => {
            Some(Event::Text { text: text.clone() })
        }
        WindowEvent::Resized(size) => Some(Event::Resize {
            width: size.width,
            height: size.height,
        }),
        WindowEvent::Focused(gained) => Some(Event::Focus { gained: *gained }),
        _ => None,
    }
}

fn key_code(k: &Key) -> String {
    match k {
        Key::Named(named) => named_key_code(*named).to_string(),
        Key::Character(s) => s.to_string(),
        Key::Unidentified(_) | Key::Dead(_) => String::new(),
    }
}

fn named_key_code(n: NamedKey) -> &'static str {
    match n {
        NamedKey::Enter => "Enter",
        NamedKey::Escape => "Escape",
        NamedKey::Backspace => "Backspace",
        NamedKey::Tab => "Tab",
        NamedKey::Space => "Space",
        NamedKey::ArrowDown => "ArrowDown",
        NamedKey::ArrowLeft => "ArrowLeft",
        NamedKey::ArrowRight => "ArrowRight",
        NamedKey::ArrowUp => "ArrowUp",
        NamedKey::End => "End",
        NamedKey::Home => "Home",
        NamedKey::PageDown => "PageDown",
        NamedKey::PageUp => "PageUp",
        NamedKey::Delete => "Delete",
        NamedKey::Insert => "Insert",
        NamedKey::F1 => "F1",
        NamedKey::F2 => "F2",
        NamedKey::F3 => "F3",
        NamedKey::F4 => "F4",
        NamedKey::F5 => "F5",
        NamedKey::F6 => "F6",
        NamedKey::F7 => "F7",
        NamedKey::F8 => "F8",
        NamedKey::F9 => "F9",
        NamedKey::F10 => "F10",
        NamedKey::F11 => "F11",
        NamedKey::F12 => "F12",
        NamedKey::Shift => "Shift",
        NamedKey::Control => "Control",
        NamedKey::Alt => "Alt",
        NamedKey::Super => "Meta",
        _ => "",
    }
}
