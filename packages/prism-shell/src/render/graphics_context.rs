//! wgpu surface + device owner.
//!
//! Vendored from `clay-layout` 0.4's `examples/wgpu/graphics_context.rs`
//! with two adjustments:
//!   1. The window is held as a boxed `raw-window-handle` trait object
//!      so the same `GraphicsContext` is driven by either `tao` (the
//!      packaged Studio shell) or `winit` (the `prism-shell` dev bin).
//!   2. `resize()` takes the new pixel dimensions directly — the
//!      caller already knows them from the window event, and it
//!      lets us stop reaching back into the window for its size.

use std::sync::Arc;

use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, WindowHandle,
};
use wgpu::{Device, Queue, RenderPass, SurfaceConfiguration};

/// Erases the concrete window type behind the two raw-window-handle
/// traits that `wgpu::SurfaceTargetUnsafe::from_window` actually cares
/// about. `Arc` so we can keep the window alive alongside the surface.
pub trait WindowHandleOwner: HasWindowHandle + HasDisplayHandle + Send + Sync {}
impl<T: HasWindowHandle + HasDisplayHandle + Send + Sync> WindowHandleOwner for T {}

/// An erased, shareable window handle. Produced by [`WindowHandle::new`]
/// wrappers on the native dev bin and the Studio shell.
#[derive(Clone)]
pub struct SharedWindow {
    inner: Arc<dyn WindowHandleOwner>,
}

impl SharedWindow {
    pub fn new<W: WindowHandleOwner + 'static>(window: W) -> Self {
        Self {
            inner: Arc::new(window),
        }
    }

    /// Wrap an already-shared window without double-boxing. The
    /// native dev bin keeps its own `Arc<tao::Window>` for
    /// `request_redraw` calls and hands us the same handle here.
    pub fn from_arc<W: WindowHandleOwner + 'static>(window: Arc<W>) -> Self {
        Self { inner: window }
    }
}

impl HasWindowHandle for SharedWindow {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        self.inner.window_handle()
    }
}

impl HasDisplayHandle for SharedWindow {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        self.inner.display_handle()
    }
}

pub struct GraphicsContext {
    #[allow(dead_code)]
    instance: wgpu::Instance,
    surface: wgpu::Surface<'static>,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub config: wgpu::SurfaceConfiguration,
    depth_texture: DepthTexture,
    pub window: SharedWindow,
}

impl GraphicsContext {
    /// Build the surface + device pair from any window implementing the
    /// raw-window-handle traits (tao, winit, web-sys wrappers, ...).
    pub fn new(window: SharedWindow, size: (u32, u32)) -> Self {
        let instance = wgpu::Instance::default();

        // SAFETY: `SharedWindow` holds the underlying window inside an
        // `Arc`, so the raw handles it yields stay valid for as long as
        // `GraphicsContext` owns its clone of the `Arc`.
        let surface_target = unsafe { wgpu::SurfaceTargetUnsafe::from_window(&window) }.unwrap();
        let surface = unsafe { instance.create_surface_unsafe(surface_target) }.unwrap();

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::default(),
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }))
        .unwrap();

        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("prism-shell device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::default(),
            },
            None,
        ))
        .unwrap();

        let capabilities = surface.get_capabilities(&adapter);
        let format = capabilities
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(capabilities.formats[0]);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.0.max(1),
            height: size.1.max(1),
            present_mode: capabilities.present_modes[0],
            desired_maximum_frame_latency: 2,
            alpha_mode: capabilities.alpha_modes[0],
            view_formats: vec![],
        };

        surface.configure(&device, &config);
        let depth_texture = DepthTexture::new(&device, &config);

        Self {
            instance,
            window,
            surface,
            device,
            queue,
            config,
            depth_texture,
        }
    }

    /// Push a single frame through the pipeline. `ui` is the caller's
    /// render pass callback — it receives a live render pass plus the
    /// device/queue/config so it can allocate per-frame resources
    /// without re-borrowing `self`.
    pub fn render<F>(&mut self, ui: F) -> Result<(), wgpu::SurfaceError>
    where
        F: FnOnce(&mut RenderPass, &Device, &Queue, &SurfaceConfiguration),
    {
        let drawable = self.surface.get_current_texture()?;

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("prism-shell frame encoder"),
            });

        let view = drawable
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("prism-shell frame pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.07,
                            g: 0.07,
                            b: 0.10,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_texture.view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            ui(&mut render_pass, &self.device, &self.queue, &self.config);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        drawable.present();
        Ok(())
    }

    pub fn resize(&mut self, new_size: (u32, u32)) {
        if new_size.0 == 0 || new_size.1 == 0 {
            return;
        }
        self.config.width = new_size.0;
        self.config.height = new_size.1;
        self.surface.configure(&self.device, &self.config);
        self.depth_texture = DepthTexture::new(&self.device, &self.config);
    }
}

struct DepthTexture {
    #[allow(dead_code)]
    texture: wgpu::Texture,
    view: wgpu::TextureView,
}

impl DepthTexture {
    fn new(device: &wgpu::Device, config: &wgpu::SurfaceConfiguration) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            size: wgpu::Extent3d {
                width: config.width.max(1),
                height: config.height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            label: Some("prism-shell depth"),
            view_formats: &[],
        });

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        Self { texture, view }
    }
}
