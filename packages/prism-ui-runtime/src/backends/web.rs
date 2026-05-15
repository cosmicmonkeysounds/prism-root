//! Web (wasm) backend — winit's wasm event loop bound to an existing
//! `<canvas>` element, femtovg over the canvas's WebGL2 context.
//! Phase 4 of the Slint→Taffy migration per
//! `docs/dev/clay-migration-plan.md`.
//!
//! ## Frame loop
//!
//! Winit's wasm target schedules `RedrawRequested` via
//! `requestAnimationFrame` — calling `Window::request_redraw()` after
//! a handler dirties the surface gives us the rAF cadence the plan
//! calls for, with no hand-rolled rAF closure dance. The same
//! `ApplicationHandler` shape the native backend uses applies here;
//! the only differences are window construction (we attach to an
//! existing canvas via `WindowAttributesExtWebSys::with_canvas`) and
//! the GL context (femtovg's `OpenGl::new_from_html_canvas` over
//! WebGL2 instead of glutin).

#[cfg(target_arch = "wasm32")]
use femtovg::{renderer::OpenGl, Canvas, Color as FemtoColor};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast;
#[cfg(target_arch = "wasm32")]
use winit::application::ApplicationHandler;
#[cfg(target_arch = "wasm32")]
use winit::event::WindowEvent;
#[cfg(target_arch = "wasm32")]
use winit::event_loop::{ActiveEventLoop, EventLoop};
#[cfg(target_arch = "wasm32")]
use winit::platform::web::{EventLoopExtWebSys, WindowAttributesExtWebSys};
#[cfg(target_arch = "wasm32")]
use winit::window::{Window, WindowId};

#[cfg(target_arch = "wasm32")]
use crate::backends::input::{translate, InputState};
use crate::event::EventHandler;
use crate::images::AssetLoader;
#[cfg(target_arch = "wasm32")]
use crate::images::ImageCache;
use crate::layout::Surface;
#[cfg(target_arch = "wasm32")]
use crate::layout::Viewport;
#[cfg(target_arch = "wasm32")]
use crate::paint;
#[cfg(target_arch = "wasm32")]
use crate::text::TextSystem;

/// Mount the surface against the canvas with id `canvas_id`. On wasm,
/// builds a winit event loop, attaches it to the canvas, and runs
/// rAF-paced frames through `paint::draw`. `handler` is invoked on
/// every translated input event; mutating the supplied `Surface`
/// inside the handler triggers the next redraw. Off wasm this is a
/// stub so host code can call it from cross-target binaries without
/// a cfg dance. `loader` resolves image sources for
/// `RenderCommand::Image` — see [`crate::images::AssetLoader`].
pub fn mount(
    canvas_id: &str,
    surface: Surface,
    handler: EventHandler,
    loader: AssetLoader,
) -> Result<(), String> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = (canvas_id, surface, handler, loader);
        Err("prism-ui-runtime web backend is only available on wasm32".into())
    }

    #[cfg(target_arch = "wasm32")]
    {
        console_error_panic_hook::set_once();
        let window = web_sys::window().ok_or("no window")?;
        let document = window.document().ok_or("no document")?;
        let element = document
            .get_element_by_id(canvas_id)
            .ok_or_else(|| format!("canvas #{} not found", canvas_id))?;
        let canvas_el: web_sys::HtmlCanvasElement = element
            .dyn_into::<web_sys::HtmlCanvasElement>()
            .map_err(|_| "element is not a <canvas>".to_string())?;
        let event_loop = EventLoop::new().map_err(|e| format!("event loop: {e}"))?;
        let app = WebApp::new(surface, handler, canvas_el, loader);
        event_loop.spawn_app(app);
        Ok(())
    }
}

#[cfg(target_arch = "wasm32")]
struct WebApp {
    surface: Surface,
    text: TextSystem,
    images: ImageCache,
    handler: EventHandler,
    input: InputState,
    canvas_el: web_sys::HtmlCanvasElement,
    state: Option<RenderState>,
}

#[cfg(target_arch = "wasm32")]
struct RenderState {
    window: Window,
    canvas: Canvas<OpenGl>,
}

#[cfg(target_arch = "wasm32")]
impl WebApp {
    fn new(
        surface: Surface,
        handler: EventHandler,
        canvas_el: web_sys::HtmlCanvasElement,
        loader: AssetLoader,
    ) -> Self {
        Self {
            surface,
            text: TextSystem::new(),
            images: ImageCache::new(loader),
            handler,
            input: InputState::default(),
            canvas_el,
            state: None,
        }
    }
}

#[cfg(target_arch = "wasm32")]
impl ApplicationHandler for WebApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }
        let attrs = Window::default_attributes().with_canvas(Some(self.canvas_el.clone()));
        let window = match event_loop.create_window(attrs) {
            Ok(w) => w,
            Err(_) => {
                event_loop.exit();
                return;
            }
        };
        let renderer = match OpenGl::new_from_html_canvas(&self.canvas_el) {
            Ok(r) => r,
            Err(_) => {
                event_loop.exit();
                return;
            }
        };
        let mut canvas = match Canvas::new(renderer) {
            Ok(c) => c,
            Err(_) => {
                event_loop.exit();
                return;
            }
        };
        let width = self.canvas_el.width().max(1);
        let height = self.canvas_el.height().max(1);
        canvas.set_size(width, height, window.scale_factor() as f32);
        self.surface.set_viewport(Viewport {
            width: width as f32,
            height: height as f32,
        });
        self.state = Some(RenderState { window, canvas });
        if let Some(s) = &self.state {
            s.window.request_redraw();
        }
    }

    fn window_event(&mut self, _event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(state) = &mut self.state else {
            return;
        };
        match &event {
            WindowEvent::Resized(size) if size.width > 0 && size.height > 0 => {
                state
                    .canvas
                    .set_size(size.width, size.height, state.window.scale_factor() as f32);
                self.surface.set_viewport(Viewport {
                    width: size.width as f32,
                    height: size.height as f32,
                });
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                let size = state.window.inner_size();
                state
                    .canvas
                    .set_size(size.width, size.height, state.window.scale_factor() as f32);
            }
            WindowEvent::RedrawRequested => {
                let viewport = self.surface.viewport();
                let cmds: Vec<_> = self.surface.commands().to_vec();
                let canvas = &mut state.canvas;
                canvas.clear_rect(
                    0,
                    0,
                    canvas.width(),
                    canvas.height(),
                    FemtoColor::rgbf(1.0, 1.0, 1.0),
                );
                // Wave 14.7 — `performance.now()` is the wasm32
                // monotonic clock; the femtovg backend uses
                // `Instant::now()` against an `epoch` for the same
                // purpose. Both feed `paint::draw_at` an `u64` of
                // milliseconds-since-epoch, the unit the animated
                // image cache wraps frame indices around.
                let now_ms = web_sys::window()
                    .and_then(|w| w.performance())
                    .map(|p| p.now() as u64)
                    .unwrap_or(0);
                paint::draw_at(
                    canvas,
                    viewport,
                    &cmds,
                    &mut self.text,
                    &mut self.images,
                    now_ms,
                );
                canvas.flush_to_output(());
                if self.images.has_animations() {
                    state.window.request_redraw();
                }
                return;
            }
            _ => {}
        }
        if let Some(prism_event) = translate(&event, &mut self.input) {
            (self.handler)(&prism_event, &mut self.surface);
        }
        if self.surface.is_dirty() || self.images.has_animations() {
            state.window.request_redraw();
        }
    }
}
