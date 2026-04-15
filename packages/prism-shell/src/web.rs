//! Browser entry point — C ABI adapter for emscripten.
//!
//! Mirrors the architecture in `prism-daemon/src/wasm.rs`: expose a
//! small, hand-rolled `extern "C"` surface that emscripten wraps
//! automatically via `ccall`/`cwrap`, and let the JS side own every
//! DOM / canvas call. The shell's job here is just to hold
//! [`AppState`] + [`Clay`] in a process-global slot, consume input
//! events the JS side pushes in, and emit render commands into a
//! shared byte buffer that the JS painter walks each frame.
//!
//! ## Why C ABI instead of wasm-bindgen?
//!
//! `clay-layout` 0.4 vendors a C core and compiles it via `cc` from
//! its build script. That source compiles cleanly under
//! `wasm32-unknown-emscripten` (emscripten ships a full libc), but
//! stock `wasm32-unknown-unknown` has no C toolchain, so clang fails
//! at `--target=wasm32-unknown-unknown`. Emscripten and wasm-bindgen
//! produce incompatible JS glue — you can't use both in the same
//! build. The daemon hit the same wall with `mlua`'s vendored Luau
//! and resolved it by going C-ABI + emscripten everywhere; prism-shell
//! inherits that choice.
//!
//! ## Build
//!
//! ```sh
//! source /path/to/emsdk/emsdk_env.sh
//! rustup target add wasm32-unknown-emscripten
//!
//! cargo build \
//!   --release \
//!   --target wasm32-unknown-emscripten \
//!   -p prism-shell \
//!   --no-default-features \
//!   --features web
//! ```
//!
//! Emscripten produces `prism_shell.wasm` + a `prism_shell.js` loader
//! alongside the main `.wasm`. The JS shim in `web/index.html` +
//! `web/loader.js` loads the module, calls `prism_shell_boot`, wires
//! DOM events to `prism_shell_pointer_*` / `prism_shell_wheel` /
//! `prism_shell_resize`, and on every `requestAnimationFrame` calls
//! `prism_shell_frame` and walks the returned command buffer.
//!
//! ## Command buffer layout
//!
//! `prism_shell_frame` writes a little-endian stream of records into
//! a static byte buffer and returns a `*const u8` pointing at it plus
//! a trailing `u32` length via `prism_shell_last_frame_len`. Each
//! record starts with a `u8` tag; the JS painter dispatches on it:
//!
//! | tag | variant       | fields                                         |
//! |-----|---------------|------------------------------------------------|
//! |  0  | Rectangle     | `f32 x, y, w, h` · `u8 r, g, b, a`            |
//! |  1  | Border        | `f32 x, y, w, h` · `u8 r, g, b, a` · `f32 w`  |
//! |  2  | Text          | `f32 x, y, w, h` · `u8 r, g, b, a` · `u16 size` · `u32 bytes` · `bytes × u8` |
//! |  3  | ScissorStart  | `f32 x, y, w, h`                              |
//! |  4  | ScissorEnd    | —                                             |
//!
//! `Image`, `Custom`, and `None` render commands are silently
//! skipped — Phase 0 has no image uploads wired up yet.

#![allow(clippy::missing_safety_doc)]

use std::cell::RefCell;
use std::os::raw::c_int;

use clay_layout::{
    math::Dimensions,
    render_commands::{RenderCommand, RenderCommandConfig},
};

use crate::input::{self, InputEvent, PointerButton};
use crate::{install_stub_text_measurer, render_app, AppState, Clay};

struct WebRuntime {
    state: AppState,
    clay: Clay,
    /// Back buffer the JS painter reads each frame. Grown on demand
    /// and never shrunk — one linear allocation per reboot is fine
    /// for Phase 0.
    frame_buffer: Vec<u8>,
}

thread_local! {
    static RUNTIME: RefCell<Option<WebRuntime>> = const { RefCell::new(None) };
}

fn with_runtime<R>(f: impl FnOnce(&mut WebRuntime) -> R) -> Option<R> {
    RUNTIME.with(|cell| cell.borrow_mut().as_mut().map(f))
}

// -----------------------------------------------------------------
// Exported C ABI
// -----------------------------------------------------------------

/// Boot the shell runtime. `width_css` / `height_css` are the canvas
/// size in CSS pixels; the JS side is responsible for tracking DPR
/// and painting into the physical backing store on top of that.
///
/// Safe to call multiple times — subsequent calls replace the
/// runtime in place.
#[no_mangle]
pub extern "C" fn prism_shell_boot(width_css: u32, height_css: u32) {
    set_panic_hook();

    let width_css = width_css.max(1);
    let height_css = height_css.max(1);

    let mut clay = Clay::new(Dimensions::new(width_css as f32, height_css as f32));
    clay.set_debug_mode(false);
    install_stub_text_measurer(&mut clay);

    let mut state = AppState::default();
    state.surface.width = width_css;
    state.surface.height = height_css;

    RUNTIME.with(|cell| {
        *cell.borrow_mut() = Some(WebRuntime {
            state,
            clay,
            frame_buffer: Vec::with_capacity(4096),
        });
    });
}

/// Tear down the runtime. Optional — pages reload just fine without
/// it since emscripten's linear memory goes away on page unload.
#[no_mangle]
pub extern "C" fn prism_shell_shutdown() {
    RUNTIME.with(|cell| *cell.borrow_mut() = None);
}

#[no_mangle]
pub extern "C" fn prism_shell_resize(width_css: u32, height_css: u32) {
    let width_css = width_css.max(1);
    let height_css = height_css.max(1);
    with_runtime(|rt| {
        rt.clay
            .set_layout_dimensions(Dimensions::new(width_css as f32, height_css as f32));
        input::dispatch(
            &mut rt.state,
            InputEvent::Resize {
                width: width_css,
                height: height_css,
            },
        );
    });
}

#[no_mangle]
pub extern "C" fn prism_shell_pointer_move(x: f32, y: f32) {
    with_runtime(|rt| {
        input::dispatch(&mut rt.state, InputEvent::PointerMove { x, y });
    });
}

#[no_mangle]
pub extern "C" fn prism_shell_pointer_button(x: f32, y: f32, button: c_int, pressed: c_int) {
    let Some(btn) = map_button(button) else {
        return;
    };
    with_runtime(|rt| {
        let event = if pressed != 0 {
            InputEvent::PointerDown { x, y, button: btn }
        } else {
            InputEvent::PointerUp { x, y, button: btn }
        };
        input::dispatch(&mut rt.state, event);
    });
}

#[no_mangle]
pub extern "C" fn prism_shell_wheel(dx: f32, dy: f32) {
    with_runtime(|rt| {
        input::dispatch(&mut rt.state, InputEvent::Wheel { dx, dy });
    });
}

#[no_mangle]
pub extern "C" fn prism_shell_key(code: u32, pressed: c_int) {
    with_runtime(|rt| {
        input::dispatch(
            &mut rt.state,
            InputEvent::Key {
                code,
                pressed: pressed != 0,
            },
        );
    });
}

/// Pump one frame: forward accumulated pointer / scroll state into
/// Clay, run the declare pass, and serialize the render commands
/// into the shared back buffer. Returns a pointer that stays valid
/// until the next `prism_shell_frame` call.
///
/// Pair with [`prism_shell_frame_len`] to read the written length.
#[no_mangle]
pub extern "C" fn prism_shell_frame(delta_seconds: f32) -> *const u8 {
    RUNTIME.with(|cell| match cell.borrow_mut().as_mut() {
        Some(rt) => {
            input::pump_clay(&mut rt.state, &rt.clay, delta_seconds.max(0.0));
            rt.frame_buffer.clear();
            let commands = render_app(&rt.state, &mut rt.clay);
            for command in commands {
                serialize_command(&mut rt.frame_buffer, &command);
            }
            rt.frame_buffer.as_ptr()
        }
        None => std::ptr::null(),
    })
}

#[no_mangle]
pub extern "C" fn prism_shell_frame_len() -> u32 {
    with_runtime(|rt| rt.frame_buffer.len() as u32).unwrap_or(0)
}

// -----------------------------------------------------------------
// Serialization — matches the table in the module docs.
// -----------------------------------------------------------------

fn serialize_command(buf: &mut Vec<u8>, command: &RenderCommand<'_, (), ()>) {
    let bb = command.bounding_box;
    match &command.config {
        RenderCommandConfig::Rectangle(rect) => {
            buf.push(0);
            push_rect(buf, bb.x, bb.y, bb.width, bb.height);
            push_color(buf, rect.color);
        }
        RenderCommandConfig::Border(border) => {
            buf.push(1);
            push_rect(buf, bb.x, bb.y, bb.width, bb.height);
            push_color(buf, border.color);
            // Phase 0 paints a single uniform stroke — future per-edge
            // work lands with the real renderer.
            let width = border.width.left.max(border.width.right) as f32;
            buf.extend_from_slice(&width.to_le_bytes());
        }
        RenderCommandConfig::Text(text) => {
            buf.push(2);
            push_rect(buf, bb.x, bb.y, bb.width, bb.height);
            push_color(buf, text.color);
            buf.extend_from_slice(&text.font_size.to_le_bytes());
            let bytes = text.text.as_bytes();
            let len = bytes.len() as u32;
            buf.extend_from_slice(&len.to_le_bytes());
            buf.extend_from_slice(bytes);
        }
        RenderCommandConfig::ScissorStart() => {
            buf.push(3);
            push_rect(buf, bb.x, bb.y, bb.width, bb.height);
        }
        RenderCommandConfig::ScissorEnd() => {
            buf.push(4);
        }
        RenderCommandConfig::Image(_)
        | RenderCommandConfig::Custom(_)
        | RenderCommandConfig::None() => {}
    }
}

fn push_rect(buf: &mut Vec<u8>, x: f32, y: f32, w: f32, h: f32) {
    buf.extend_from_slice(&x.to_le_bytes());
    buf.extend_from_slice(&y.to_le_bytes());
    buf.extend_from_slice(&w.to_le_bytes());
    buf.extend_from_slice(&h.to_le_bytes());
}

fn push_color(buf: &mut Vec<u8>, color: clay_layout::color::Color) {
    buf.push(clamp_u8(color.r));
    buf.push(clamp_u8(color.g));
    buf.push(clamp_u8(color.b));
    buf.push(clamp_u8(color.a));
}

fn clamp_u8(v: f32) -> u8 {
    v.clamp(0.0, 255.0).round() as u8
}

// -----------------------------------------------------------------
// Helpers
// -----------------------------------------------------------------

fn map_button(raw: c_int) -> Option<PointerButton> {
    match raw {
        0 => Some(PointerButton::Primary),
        1 => Some(PointerButton::Middle),
        2 => Some(PointerButton::Secondary),
        _ => None,
    }
}

fn set_panic_hook() {
    // Emscripten routes stderr to `console.error` by default, which
    // is good enough for Phase 0. A richer hook (stack traces via
    // `std::backtrace`) can land later — the hook is cheap to
    // upgrade from one module.
    std::panic::set_hook(Box::new(|info| {
        eprintln!("prism-shell panic: {info}");
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_u8_saturates() {
        assert_eq!(clamp_u8(-5.0), 0);
        assert_eq!(clamp_u8(0.0), 0);
        assert_eq!(clamp_u8(127.5), 128);
        assert_eq!(clamp_u8(254.9), 255);
        assert_eq!(clamp_u8(9999.0), 255);
    }

    #[test]
    fn map_button_covers_three_buttons() {
        assert_eq!(map_button(0), Some(PointerButton::Primary));
        assert_eq!(map_button(1), Some(PointerButton::Middle));
        assert_eq!(map_button(2), Some(PointerButton::Secondary));
        assert_eq!(map_button(42), None);
    }
}
