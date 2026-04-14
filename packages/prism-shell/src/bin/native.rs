//! `prism-shell` native dev binary.
//!
//! Standalone entry point used during the inner dev loop — skips the
//! Tauri shell entirely and pops a `tao` window straight onto the same
//! `wgpu` + Clay render pipeline the packaged app uses. The packaged
//! desktop build lives in `packages/prism-studio/src-tauri` and embeds
//! `prism-shell` as a library; both entry points drive the renderer
//! vendored in `prism_shell::render`.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use clay_layout::{math::Dimensions, text::TextConfig};
use prism_shell::render::{GraphicsContext, SharedWindow, UiRenderer};
use prism_shell::{render_app, AppState, Clay};
use tao::{
    dpi::LogicalSize,
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::{Window, WindowBuilder},
};

fn main() {
    let event_loop = EventLoop::new();

    let window: Window = WindowBuilder::new()
        .with_title("Prism Shell (dev)")
        .with_inner_size(LogicalSize::new(1280.0, 800.0))
        .build(&event_loop)
        .expect("failed to build prism-shell dev window");

    let physical = window.inner_size();
    let dpi_scale = window.scale_factor() as f32;
    let size = (physical.width.max(1), physical.height.max(1));

    // `Arc<Window>` so the render stack can hold one handle via
    // `SharedWindow` while the event loop keeps the other to drive
    // `request_redraw`.
    let window = Arc::new(window);
    let shared_window = SharedWindow::from_arc(window.clone());
    let mut ctx = GraphicsContext::new(shared_window, size);

    // The UI renderer owns the wgpu pipeline, the glyphon FontSystem,
    // and the text atlas. We hand it to Clay as the measure-text
    // callback's user data so layout can see real glyph metrics.
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

    let state = AppState::default();
    let start = Instant::now();
    let mut last_frame = start;

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        match event {
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::CloseRequested => {
                    *control_flow = ControlFlow::Exit;
                }
                WindowEvent::Resized(new_size) => {
                    let size = (new_size.width.max(1), new_size.height.max(1));
                    ctx.resize(size);
                    ui_renderer.borrow_mut().resize(size);
                    clay.set_layout_dimensions((size.0 as f32, size.1 as f32).into());
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
                    window.request_redraw();
                }
                _ => {}
            },
            Event::MainEventsCleared => {
                window.request_redraw();
            }
            Event::RedrawRequested(_) => {
                let now = Instant::now();
                let _dt = now.duration_since(last_frame).as_secs_f32();
                last_frame = now;

                // `render_app` borrows `clay` for the lifetime of the
                // returned command vec, so `ui.render_clay` must run
                // before Clay's next frame begins.
                let commands = render_app(&state, &mut clay);

                let ui = ui_renderer.clone();
                if let Err(err) = ctx.render(|pass, device, queue, config| {
                    ui.borrow_mut()
                        .render_clay(commands.into_iter(), pass, device, queue, config);
                }) {
                    eprintln!("prism-shell: surface render error: {err:?}");
                }
            }
            _ => {}
        }
    });
}
