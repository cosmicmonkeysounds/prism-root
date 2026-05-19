//! Native femtovg backend — winit window + glutin GL context driving
//! a `femtovg::Canvas<OpenGl>`. Phase 1 of the Slint→Taffy migration
//! per `docs/dev/clay-migration-plan.md` §5.3.
//!
//! `run(surface, handler)` opens a window sized to the surface's
//! viewport and enters winit's event loop. Each frame:
//!
//! 1. Translate winit input → `event::Event` and hand both the event
//!    and `&mut Surface` to the host's [`EventHandler`] (this is the
//!    Phase-4 input dispatch hook — the shell wires it through to
//!    `prism_builder::signal::dispatch_signal`).
//! 2. After the handler runs, `Surface::commands()` lazily recomputes
//!    layout iff the handler dirtied the tree (the retained-mode
//!    contract in §5.2).
//! 3. Walk the stream through `paint::draw` and swap GL buffers.

use std::num::NonZeroU32;
use std::time::Instant;

use femtovg::{renderer::OpenGl, Canvas, Color as FemtoColor};
use glutin::config::{ConfigTemplateBuilder, GlConfig};
use glutin::context::{ContextApi, ContextAttributesBuilder, PossiblyCurrentContext};
use glutin::display::{GetGlDisplay, GlDisplay};
use glutin::prelude::*;
use glutin::surface::{Surface as GlutinSurface, WindowSurface};
use glutin_winit::{DisplayBuilder, GlWindow};
use raw_window_handle::HasWindowHandle;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowId};

use crate::backends::input::{translate, InputState};
use crate::event::EventHandler;
use crate::images::{noop_loader, AssetLoader, ImageCache};
use crate::layout::{Surface, Viewport};
use crate::paint;
use crate::text::TextSystem;

/// Open a native window and render `surface` into it. `handler` is
/// invoked on every translated input event before each redraw.
/// Blocks until the window closes. `loader` resolves the source
/// strings carried by `RenderCommand::Image` to raw bytes; hosts
/// that have no images can pass [`crate::images::noop_loader`].
pub fn run(
    surface: Surface,
    handler: EventHandler,
    loader: AssetLoader,
) -> Result<(), Box<dyn std::error::Error>> {
    let event_loop = EventLoop::new()?;
    let mut app = App::new(surface, handler, loader);
    event_loop.run_app(&mut app)?;
    Ok(())
}

/// Type alias for the per-event "tick" callback. The host gets a
/// chance to mutate the `Surface` (typically by polling a
/// hot-reload channel and applying skeleton swaps) before the
/// event-loop decides whether the surface is dirty enough to
/// re-render. Called *after* every translated input event and once
/// at startup so a pending reload landed before the first frame
/// still applies.
pub type TickHook = Box<dyn FnMut(&mut Surface)>;

/// Same as [`run`] but with a per-event tick hook that runs before
/// the dirty check. Used by `prism-shell::Shell::run_with_hot_reload`
/// to poll a file-watcher channel each tick — closes C3 of
/// `docs/dev/ui-migration-followups.md` from the backend side.
pub fn run_with_tick(
    surface: Surface,
    handler: EventHandler,
    loader: AssetLoader,
    tick: TickHook,
) -> Result<(), Box<dyn std::error::Error>> {
    let event_loop = EventLoop::new()?;
    let mut app = App::new(surface, handler, loader);
    app.tick = Some(tick);
    event_loop.run_app(&mut app)?;
    Ok(())
}

/// Back-compat helper for hosts that don't yet plug in an
/// [`AssetLoader`]. Equivalent to `run(surface, handler,
/// noop_loader())` — every `Image` render command renders as
/// nothing.
pub fn run_without_images(
    surface: Surface,
    handler: EventHandler,
) -> Result<(), Box<dyn std::error::Error>> {
    run(surface, handler, noop_loader())
}

struct App {
    surface: Surface,
    text: TextSystem,
    images: ImageCache,
    handler: EventHandler,
    input: InputState,
    state: Option<RenderState>,
    /// Wave 14.7 — monotonic epoch the per-frame `now_ms` is
    /// expressed against. Constructed once at `App::new` so every
    /// `draw_at` call shares the same origin and animated-image
    /// frame indices stay stable across redraws.
    epoch: Instant,
    /// C3 — optional per-event tick hook. The host installs one to
    /// poll a file-watcher channel and call `Surface::set_tree`
    /// when fresh skeleton source arrives. `None` for the default
    /// `run` path; `Some(...)` for `run_with_tick`.
    tick: Option<TickHook>,
}

struct RenderState {
    window: Window,
    gl_surface: GlutinSurface<WindowSurface>,
    gl_ctx: PossiblyCurrentContext,
    canvas: Canvas<OpenGl>,
}

impl App {
    fn new(surface: Surface, handler: EventHandler, loader: AssetLoader) -> Self {
        Self {
            surface,
            text: TextSystem::new(),
            images: ImageCache::new(loader),
            handler,
            input: InputState::default(),
            state: None,
            epoch: Instant::now(),
            tick: None,
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }
        let viewport = self.surface.viewport();
        let initial = PhysicalSize::new(
            viewport.width.max(1.0) as u32,
            viewport.height.max(1.0) as u32,
        );
        let window_attrs = Window::default_attributes()
            .with_title("Prism")
            .with_inner_size(initial);
        let template = ConfigTemplateBuilder::new().with_alpha_size(8);
        let display_builder = DisplayBuilder::new().with_window_attributes(Some(window_attrs));
        let (window, gl_config) = match display_builder.build(event_loop, template, |configs| {
            configs
                .reduce(|acc, cfg| {
                    if cfg.num_samples() > acc.num_samples() {
                        cfg
                    } else {
                        acc
                    }
                })
                .expect("glutin: no GL configs returned")
        }) {
            Ok((Some(window), config)) => (window, config),
            Ok((None, _)) | Err(_) => {
                eprintln!("prism-ui-runtime: failed to create GL window");
                event_loop.exit();
                return;
            }
        };
        let raw_window_handle = window
            .window_handle()
            .ok()
            .map(|h| h.as_raw())
            .expect("winit: no window handle");
        let gl_display = gl_config.display();
        let context_attrs = ContextAttributesBuilder::new()
            .with_context_api(ContextApi::OpenGl(None))
            .build(Some(raw_window_handle));
        let not_current = unsafe {
            gl_display
                .create_context(&gl_config, &context_attrs)
                .expect("glutin: create_context")
        };
        let surface_attrs = window
            .build_surface_attributes(<_>::default())
            .expect("glutin: surface attrs");
        let gl_surface = unsafe {
            gl_display
                .create_window_surface(&gl_config, &surface_attrs)
                .expect("glutin: window surface")
        };
        let gl_ctx = not_current
            .make_current(&gl_surface)
            .expect("glutin: make current");
        let renderer = unsafe {
            OpenGl::new_from_function_cstr(|s| gl_display.get_proc_address(s) as *const _)
                .expect("femtovg: OpenGl renderer")
        };
        let mut canvas = Canvas::new(renderer).expect("femtovg: canvas");
        let size = window.inner_size();
        canvas.set_size(
            size.width.max(1),
            size.height.max(1),
            window.scale_factor() as f32,
        );
        self.surface.set_viewport(Viewport {
            width: size.width as f32,
            height: size.height as f32,
        });
        // Enable IME composition so the OS reports `Ime::Preedit` /
        // `Ime::Commit` events whenever a CJK / dead-key composition
        // is in progress. Without this winit never delivers IME
        // events; the editor's preedit machinery has nothing to
        // route. macOS and Wayland honour this immediately; X11
        // composition uses XIM under the hood.
        window.set_ime_allowed(true);
        self.state = Some(RenderState {
            window,
            gl_surface,
            gl_ctx,
            canvas,
        });
        if let Some(s) = &self.state {
            s.window.request_redraw();
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(state) = &mut self.state else {
            return;
        };
        match &event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
                return;
            }
            WindowEvent::Resized(size) if size.width > 0 && size.height > 0 => {
                state.gl_surface.resize(
                    &state.gl_ctx,
                    NonZeroU32::new(size.width).unwrap(),
                    NonZeroU32::new(size.height).unwrap(),
                );
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
                let now_ms = self.epoch.elapsed().as_millis() as u64;
                redraw(
                    state,
                    &mut self.surface,
                    &mut self.text,
                    &mut self.images,
                    now_ms,
                );
                // Wave 14.7 — animated image sources keep the
                // surface "logically dirty" between authoring
                // changes so GIF / animated-WebP / APNG frames
                // advance. We schedule the next redraw here when
                // any animation is in-flight; clean frames (no
                // animation, no event) stay clean.
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

        // C3 hot-reload tick: drain any pending skeleton swaps
        // before deciding whether the surface needs a redraw. The
        // host's hook may dirty the surface (via `set_tree`); the
        // standard dirty check below picks that up uniformly.
        if let Some(tick) = self.tick.as_mut() {
            tick(&mut self.surface);
        }

        // The retained-mode contract: only request a repaint when the
        // surface is actually dirty (handler mutation, viewport
        // resize, etc.). Clean frames stay clean.
        if self.surface.is_dirty() || self.images.has_animations() {
            state.window.request_redraw();
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        // C3 — also drain on the idle-wait edge so a file change
        // applied while no input events are flowing still wakes the
        // next redraw. Surface mutation inside the hook trips the
        // `is_dirty` branch below, which requests a redraw.
        let Some(state) = &mut self.state else { return };
        if let Some(tick) = self.tick.as_mut() {
            tick(&mut self.surface);
        }
        if self.surface.is_dirty() || self.images.has_animations() {
            state.window.request_redraw();
        }
    }
}

fn redraw(
    state: &mut RenderState,
    surface: &mut Surface,
    text: &mut TextSystem,
    images: &mut ImageCache,
    now_ms: u64,
) {
    let viewport = surface.viewport();
    let cmds: Vec<_> = surface.commands().to_vec();
    // TEMP DIAGNOSTIC (blank-canvas regression). Remove after fix.
    {
        use crate::command::RenderCommand;
        let rects = cmds
            .iter()
            .filter(|c| matches!(c, RenderCommand::Rectangle { .. }))
            .count();
        let texts: Vec<&str> = cmds
            .iter()
            .filter_map(|c| match c {
                RenderCommand::Text { content, .. } => Some(content.as_str()),
                _ => None,
            })
            .collect();
        eprintln!(
            "[PRISM_FRAME] vp={:.0}x{:.0} cmds={} rects={} texts={} :: {:?}",
            viewport.width,
            viewport.height,
            cmds.len(),
            rects,
            texts.len(),
            texts.iter().take(12).collect::<Vec<_>>()
        );
    }
    let canvas = &mut state.canvas;
    canvas.clear_rect(
        0,
        0,
        canvas.width(),
        canvas.height(),
        FemtoColor::rgbf(1.0, 1.0, 1.0),
    );
    paint::draw_at(canvas, viewport, &cmds, text, images, now_ms);
    canvas.flush_to_output(());
    let _ = state.gl_surface.swap_buffers(&state.gl_ctx);
}
