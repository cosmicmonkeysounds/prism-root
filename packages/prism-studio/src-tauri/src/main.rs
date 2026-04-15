//! Prism Studio — packaged desktop shell.
//!
//! Phase 0 spike #5 was retried on 2026-04-15 and Option B (Tauri 2
//! without a webview) was abandoned in favour of Option C: bare `tao`
//! for windowing, `wgpu` for rendering, `prism-shell` for the UI
//! tree, `cargo-packager` (Phase 5) for the installer layer. The
//! tauri crate is no longer in the dep tree at all — its runtime
//! unconditionally dropped every raw cursor/mouse/wheel event at
//! `tauri-runtime-wry-2.10.1/src/lib.rs:552`, which made the
//! no-webview path unworkable for a pointer-driven shell.
//!
//! This file is intentionally a near-clone of
//! `packages/prism-shell/src/bin/native.rs`. The Studio binary is
//! the "packaged" entry point and the `prism-shell` binary is the
//! "dev loop" entry point, but both drive the same
//! `GraphicsContext` + `UiRenderer` + `Clay` triple against a bare
//! `tao::Window`. The only thing Studio does that the dev bin
//! doesn't is spawn the `prism-daemond` sidecar over
//! `interprocess` and hold its handle for the lifetime of the
//! event loop.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod sidecar;

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use clay_layout::{math::Dimensions, text::TextConfig};
use prism_shell::input::{self, InputEvent, PointerButton};
use prism_shell::render::{GraphicsContext, SharedWindow, UiRenderer};
use prism_shell::{render_app, Clay, Shell};
use sidecar::DaemonSidecar;
use tao::{
    dpi::LogicalSize,
    event::{ElementState, Event, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::{Window, WindowBuilder},
};

fn main() {
    // Spawn the daemon sidecar before the event loop owns the main
    // thread. If it fails we log and continue — the renderer is still
    // useful for UI iteration when the kernel is down.
    let mut daemon: Option<DaemonSidecar> = match sidecar::spawn_dev() {
        Ok(handle) => Some(handle),
        Err(err) => {
            eprintln!("prism-studio: daemon sidecar unavailable: {err:#}");
            None
        }
    };

    let event_loop = EventLoop::new();

    let window: Window = WindowBuilder::new()
        .with_title("Prism Studio")
        .with_inner_size(LogicalSize::new(1280.0, 800.0))
        .with_min_inner_size(LogicalSize::new(640.0, 400.0))
        .with_resizable(true)
        .build(&event_loop)
        .expect("failed to build Prism Studio window");

    let physical = window.inner_size();
    let dpi_scale = window.scale_factor() as f32;
    let size = (physical.width.max(1), physical.height.max(1));

    let window = Arc::new(window);
    let shared_window = SharedWindow::from_arc(window.clone());
    let mut ctx = GraphicsContext::new(shared_window, size);

    let ui_renderer = Rc::new(RefCell::new(UiRenderer::new(
        &ctx.device,
        &ctx.queue,
        ctx.config.format,
        size,
        dpi_scale,
    )));

    let mut clay = Clay::new((size.0 as f32, size.1 as f32).into());
    clay.set_debug_mode(false);
    clay.set_measure_text_function_user_data(
        ui_renderer.clone(),
        |text: &str, cfg: &TextConfig, data: &mut Rc<RefCell<UiRenderer>>| -> Dimensions {
            data.borrow_mut()
                .measure_text(text, cfg.font_size as f32, cfg.line_height as f32)
        },
    );

    let mut shell = Shell::new();
    shell.dispatch_input(InputEvent::Resize {
        width: size.0,
        height: size.1,
    });
    let start = Instant::now();
    let mut last_frame = start;
    let mut cursor_physical = (0.0_f32, 0.0_f32);

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        match event {
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::CloseRequested => {
                    // Drop the sidecar before exit so the daemon
                    // child is killed deterministically.
                    let _ = daemon.take();
                    *control_flow = ControlFlow::Exit;
                }
                WindowEvent::Resized(new_size) => {
                    let size = (new_size.width.max(1), new_size.height.max(1));
                    ctx.resize(size);
                    ui_renderer.borrow_mut().resize(size);
                    clay.set_layout_dimensions((size.0 as f32, size.1 as f32).into());
                    shell.dispatch_input(InputEvent::Resize {
                        width: size.0,
                        height: size.1,
                    });
                    window.request_redraw();
                }
                WindowEvent::ScaleFactorChanged {
                    scale_factor,
                    new_inner_size,
                } => {
                    ui_renderer.borrow_mut().dpi_scale = scale_factor as f32;
                    let size = (new_inner_size.width.max(1), new_inner_size.height.max(1));
                    ctx.resize(size);
                    clay.set_layout_dimensions((size.0 as f32, size.1 as f32).into());
                    shell.dispatch_input(InputEvent::Resize {
                        width: size.0,
                        height: size.1,
                    });
                    window.request_redraw();
                }
                WindowEvent::CursorMoved { position, .. } => {
                    cursor_physical = (position.x as f32, position.y as f32);
                    shell.dispatch_input(InputEvent::PointerMove {
                        x: cursor_physical.0,
                        y: cursor_physical.1,
                    });
                    window.request_redraw();
                }
                WindowEvent::MouseInput {
                    state: btn_state,
                    button,
                    ..
                } => {
                    let pb = match button {
                        MouseButton::Left => Some(PointerButton::Primary),
                        MouseButton::Right => Some(PointerButton::Secondary),
                        MouseButton::Middle => Some(PointerButton::Middle),
                        _ => None,
                    };
                    if let Some(btn) = pb {
                        let event = match btn_state {
                            ElementState::Pressed => InputEvent::PointerDown {
                                x: cursor_physical.0,
                                y: cursor_physical.1,
                                button: btn,
                            },
                            _ => InputEvent::PointerUp {
                                x: cursor_physical.0,
                                y: cursor_physical.1,
                                button: btn,
                            },
                        };
                        shell.dispatch_input(event);
                        window.request_redraw();
                    }
                }
                WindowEvent::MouseWheel { delta, .. } => {
                    let (dx, dy) = match delta {
                        MouseScrollDelta::LineDelta(x, y) => (x * 18.0, y * 18.0),
                        MouseScrollDelta::PixelDelta(p) => (p.x as f32, p.y as f32),
                        _ => (0.0, 0.0),
                    };
                    shell.dispatch_input(InputEvent::Wheel { dx, dy });
                    window.request_redraw();
                }
                _ => {}
            },
            Event::MainEventsCleared => {
                window.request_redraw();
            }
            Event::RedrawRequested(_) => {
                let now = Instant::now();
                let dt = now.duration_since(last_frame).as_secs_f32();
                last_frame = now;

                shell.store_mut().mutate(|state| {
                    input::pump_clay(state, &clay, dt);
                });

                let commands = render_app(shell.state(), &mut clay);

                let ui = ui_renderer.clone();
                if let Err(err) = ctx.render(|pass, device, queue, config| {
                    ui.borrow_mut()
                        .render_clay(commands.into_iter(), pass, device, queue, config);
                }) {
                    eprintln!("prism-studio: surface render error: {err:?}");
                }
            }
            Event::LoopDestroyed => {
                // Redundant with CloseRequested, but guarantees the
                // sidecar is torn down if the loop exits another way.
                let _ = daemon.take();
            }
            _ => {}
        }
    });
}
