//! Prism Studio — Tauri 2 desktop shell (no-webview configuration).
//!
//! Per the Clay migration plan §4.5 Option B (resolved 2026-04-14 by
//! Phase 0 spike #5), Studio uses Tauri 2 for packaging, signing,
//! auto-update, and sidecar lifecycle management — but never creates
//! a webview. The window is built via `tauri::window::WindowBuilder`
//! (gated behind the `unstable` feature), which produces a pure
//! `tao::Window` with no wry surface attached. `prism-shell` then
//! drives a `wgpu` render loop directly against that window's raw
//! handle.
//!
//! The event loop is *Tauri's*: `App::run` delivers `RunEvent`s and
//! we dispatch them into the render state on `Ready`,
//! `MainEventsCleared`, and window resize events. This mirrors the
//! structure of `packages/prism-shell/src/bin/native.rs` — which
//! uses the same `GraphicsContext` + `UiRenderer` + `Clay` triple
//! against a bare tao `EventLoop` — so the two shells stay in lock
//! step on the render path.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod sidecar;

use std::cell::RefCell;
use std::rc::Rc;

use clay_layout::{math::Dimensions, text::TextConfig};
use prism_shell::render::{GraphicsContext, SharedWindow, UiRenderer};
use prism_shell::{render_app, AppState, Clay};
use tauri::{RunEvent, WindowEvent};

/// Everything the Studio host owns for a given window. Non-`Send` on
/// purpose — Clay and `Rc<RefCell<UiRenderer>>` are not thread-safe,
/// but the `tauri::App::run` callback is only `'static + FnMut`, so
/// we can freely own these inside it.
struct RenderState {
    window: tauri::Window<tauri::Wry>,
    ctx: GraphicsContext,
    ui: Rc<RefCell<UiRenderer>>,
    clay: Clay,
    app_state: AppState,
}

fn main() {
    let app = tauri::Builder::default()
        .setup(|_app| {
            sidecar::spawn_dev();
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("failed to build Prism Studio app");

    let mut state: Option<RenderState> = None;

    app.run(move |handle, event| match event {
        RunEvent::Ready => {
            if state.is_some() {
                return;
            }
            match build_render_state(handle) {
                Ok(s) => state = Some(s),
                Err(err) => eprintln!("prism-studio: failed to build render state: {err}"),
            }
        }
        RunEvent::WindowEvent { event: we, .. } => {
            let Some(s) = state.as_mut() else {
                return;
            };
            match we {
                WindowEvent::Resized(size) => {
                    let new_size = (size.width.max(1), size.height.max(1));
                    s.ctx.resize(new_size);
                    s.ui.borrow_mut().resize(new_size);
                    s.clay
                        .set_layout_dimensions((new_size.0 as f32, new_size.1 as f32).into());
                    draw_frame(s);
                }
                WindowEvent::ScaleFactorChanged {
                    scale_factor,
                    new_inner_size,
                    ..
                } => {
                    s.ui.borrow_mut().dpi_scale = scale_factor as f32;
                    let new_size = (new_inner_size.width.max(1), new_inner_size.height.max(1));
                    s.ctx.resize(new_size);
                    s.clay
                        .set_layout_dimensions((new_size.0 as f32, new_size.1 as f32).into());
                    draw_frame(s);
                }
                WindowEvent::CloseRequested { .. } => {
                    // Tauri will exit on its own; drop the render
                    // state so we release the wgpu surface before
                    // the window handle dies.
                    state = None;
                }
                _ => {}
            }
        }
        RunEvent::MainEventsCleared => {
            if let Some(s) = state.as_mut() {
                draw_frame(s);
            }
        }
        _ => {}
    });
}

fn build_render_state(handle: &tauri::AppHandle<tauri::Wry>) -> tauri::Result<RenderState> {
    let window = tauri::window::WindowBuilder::new(handle, "main")
        .title("Prism Studio")
        .inner_size(1280.0, 800.0)
        .min_inner_size(640.0, 400.0)
        .resizable(true)
        .build()?;

    let physical = window.inner_size()?;
    let dpi_scale = window.scale_factor()? as f32;
    let size = (physical.width.max(1), physical.height.max(1));

    let shared = SharedWindow::new(window.clone());
    let mut ctx = GraphicsContext::new(shared, size);

    let ui = Rc::new(RefCell::new(UiRenderer::new(
        &ctx.device,
        &ctx.queue,
        ctx.config.format,
        size,
        dpi_scale,
    )));

    let mut clay = Clay::new((size.0 as f32, size.1 as f32).into());
    clay.set_debug_mode(false);
    clay.set_measure_text_function_user_data(
        ui.clone(),
        |text: &str, cfg: &TextConfig, data: &mut Rc<RefCell<UiRenderer>>| -> Dimensions {
            data.borrow_mut()
                .measure_text(text, cfg.font_size as f32, cfg.line_height as f32)
        },
    );

    // Silence the unused-ctx warning during the first frame — we
    // actually use it inside draw_frame once the RenderState exists.
    let _ = &mut ctx;

    Ok(RenderState {
        window,
        ctx,
        ui,
        clay,
        app_state: AppState::default(),
    })
}

fn draw_frame(state: &mut RenderState) {
    let commands = render_app(&state.app_state, &mut state.clay);
    let ui = state.ui.clone();
    if let Err(err) = state.ctx.render(|pass, device, queue, config| {
        ui.borrow_mut()
            .render_clay(commands.into_iter(), pass, device, queue, config);
    }) {
        eprintln!("prism-studio: surface render error: {err:?}");
    }
    // Touch the window field so rustc doesn't warn on unused — it
    // still owns the tao handle that `ctx` is rendering into.
    let _ = &state.window;
}
