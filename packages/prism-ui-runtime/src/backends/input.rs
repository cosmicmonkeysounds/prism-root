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
            // A *character-producing* key press without Ctrl / Meta /
            // Alt held becomes an `Event::Text` so it flows through
            // `FieldFocusService`'s typing arm — the same path IME
            // commits use. Without this branch, plain "abc" typing
            // generated `Event::Key { code: "a", ... }` events that
            // the focus service handled-but-ignored (only matching
            // backspace / enter / escape), so every keystroke into
            // a text field was silently dropped in production. winit's
            // `KeyEvent::text` field is precisely the text the OS
            // produced for this key chord (honouring Shift / AltGr /
            // dead-key composition) — preferring it over our own
            // `Key::Character` mapping handles uppercase, accents,
            // and IME-less input correctly.
            if pressed && !state.modifiers.ctrl && !state.modifiers.meta {
                if let Some(text) = ke.text.as_deref() {
                    if !text.is_empty() && !text.chars().all(|c| c.is_control()) {
                        return Some(Event::Text {
                            text: text.to_string(),
                        });
                    }
                }
            }
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

/// Named-key strings are emitted in lowercase so the shell's service
/// layer (`FieldFocusService`, `PaletteService`, `InputService`) can
/// match them with simple `code.as_str() == "backspace"` without
/// case-aware comparisons or maintenance of a parallel Title-Case
/// table. Existing service code (and its unit tests) was already
/// authored against the lowercase form; the previous Title-Case
/// emission silently broke every Backspace / Enter / Escape inside a
/// focused field-edit row in production.
fn named_key_code(n: NamedKey) -> &'static str {
    match n {
        NamedKey::Enter => "enter",
        NamedKey::Escape => "escape",
        NamedKey::Backspace => "backspace",
        NamedKey::Tab => "tab",
        NamedKey::Space => "space",
        NamedKey::ArrowDown => "arrowdown",
        NamedKey::ArrowLeft => "arrowleft",
        NamedKey::ArrowRight => "arrowright",
        NamedKey::ArrowUp => "arrowup",
        NamedKey::End => "end",
        NamedKey::Home => "home",
        NamedKey::PageDown => "pagedown",
        NamedKey::PageUp => "pageup",
        NamedKey::Delete => "delete",
        NamedKey::Insert => "insert",
        NamedKey::F1 => "f1",
        NamedKey::F2 => "f2",
        NamedKey::F3 => "f3",
        NamedKey::F4 => "f4",
        NamedKey::F5 => "f5",
        NamedKey::F6 => "f6",
        NamedKey::F7 => "f7",
        NamedKey::F8 => "f8",
        NamedKey::F9 => "f9",
        NamedKey::F10 => "f10",
        NamedKey::F11 => "f11",
        NamedKey::F12 => "f12",
        NamedKey::Shift => "shift",
        NamedKey::Control => "control",
        NamedKey::Alt => "alt",
        NamedKey::Super => "meta",
        _ => "",
    }
}
