//! wgpu + glyphon renderer that consumes Clay's `RenderCommand` stream.
//!
//! Vendored from `clay-layout` 0.4's `examples/wgpu/ui_renderer.rs` with
//! the following divergences from upstream:
//!   * `winit::dpi::PhysicalSize<u32>` is replaced with a `(u32, u32)`
//!     tuple so this module does not drag a windowing crate in.
//!   * The file is organised top-down: vertex types → pipeline builder
//!     → `UiRenderer` — that's what reads best as a reference for the
//!     rest of Prism.
//!   * Names are normalised to Prism's style (`UiRenderer`, `UiVertex`,
//!     lower camel for function args). The rendering math is identical.
//!
//! Anything labelled `// clay-upstream` was copy-pasted verbatim — when
//! we bump clay-layout we fold diffs against that marker.

use std::ops::{Add, Mul, Sub};

use clay_layout::{
    math::Dimensions,
    render_commands::{RenderCommand, RenderCommandConfig},
};
use glyphon::{
    cosmic_text, Attrs, Buffer, Cache, Color as TextColor, Family, FontSystem, Metrics, Resolution,
    Shaping, SwashCache, TextArea, TextAtlas, TextBounds, TextRenderer, Viewport,
};
use wgpu::{util::DeviceExt, MultisampleState};

// ─── vertex + geometry primitives ────────────────────────────────────

#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct UiColor {
    pub r: f32,
    pub g: f32,
    pub b: f32,
}

#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct UiPosition {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl UiPosition {
    pub const ZERO: Self = Self {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };

    pub fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    fn rotate(&mut self, mut degrees: f32) {
        degrees = -degrees * (std::f32::consts::PI / 180.0);
        let (sn, cs) = degrees.sin_cos();
        let new = UiPosition {
            x: self.x * cs - self.y * sn,
            y: self.x * sn + self.y * cs,
            z: self.z,
        };
        *self = new;
    }

    fn with_x(&self, x: f32) -> UiPosition {
        UiPosition {
            x: self.x + x,
            y: self.y,
            z: self.z,
        }
    }

    fn with_y(&self, y: f32) -> UiPosition {
        UiPosition {
            x: self.x,
            y: self.y + y,
            z: self.z,
        }
    }

    fn add_x_mut(&mut self, x: f32) -> &mut Self {
        self.x += x;
        self
    }

    fn add_y_mut(&mut self, y: f32) -> &mut Self {
        self.y += y;
        self
    }
}

impl Add for UiPosition {
    type Output = UiPosition;
    fn add(self, o: UiPosition) -> UiPosition {
        UiPosition {
            x: self.x + o.x,
            y: self.y + o.y,
            z: self.z,
        }
    }
}

impl Add<f32> for UiPosition {
    type Output = UiPosition;
    fn add(self, rhs: f32) -> UiPosition {
        UiPosition {
            x: self.x + rhs,
            y: self.y + rhs,
            z: self.z,
        }
    }
}

impl Sub<f32> for UiPosition {
    type Output = UiPosition;
    fn sub(self, rhs: f32) -> UiPosition {
        UiPosition {
            x: self.x - rhs,
            y: self.y - rhs,
            z: self.z,
        }
    }
}

impl Mul<f32> for UiPosition {
    type Output = UiPosition;
    fn mul(self, rhs: f32) -> UiPosition {
        UiPosition {
            x: self.x * rhs,
            y: self.y * rhs,
            z: self.z,
        }
    }
}

#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct UiSize {
    pub width: f32,
    pub height: f32,
}

#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct UiVertex {
    pub position: UiPosition,
    pub color: UiColor,
    pub size: UiSize,
}

impl UiVertex {
    fn new(size: (u32, u32)) -> Self {
        Self {
            position: UiPosition::ZERO,
            color: UiColor {
                r: 0.0,
                g: 0.0,
                b: 0.0,
            },
            size: UiSize {
                width: size.0 as f32,
                height: size.1 as f32,
            },
        }
    }

    fn layout() -> wgpu::VertexBufferLayout<'static> {
        const ATTR: [wgpu::VertexAttribute; 3] =
            wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2];
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<UiVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &ATTR,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct UiCornerRadii {
    pub top_left: f32,
    pub top_right: f32,
    pub bottom_left: f32,
    pub bottom_right: f32,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct UiBorderThickness {
    pub top: f32,
    pub left: f32,
    pub bottom: f32,
    pub right: f32,
}

struct TextLine {
    line: glyphon::Buffer,
    left: f32,
    top: f32,
    color: TextColor,
    bounds: Option<(UiPosition, UiPosition)>,
}

// ─── pipeline builder ────────────────────────────────────────────────

struct UiPipeline {
    pixel_format: wgpu::TextureFormat,
    vertex_layouts: Vec<wgpu::VertexBufferLayout<'static>>,
}

impl UiPipeline {
    fn new(pixel_format: wgpu::TextureFormat) -> Self {
        Self {
            pixel_format,
            vertex_layouts: Vec::new(),
        }
    }

    fn add_layout(&mut self, layout: wgpu::VertexBufferLayout<'static>) {
        self.vertex_layouts.push(layout);
    }

    fn build(&self, device: &wgpu::Device) -> wgpu::RenderPipeline {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("prism-shell ui shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("ui_shader.wgsl").into()),
        });

        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("prism-shell ui pipeline layout"),
            bind_group_layouts: &[],
            push_constant_ranges: &[],
        });

        let targets = [Some(wgpu::ColorTargetState {
            format: self.pixel_format,
            blend: Some(wgpu::BlendState::REPLACE),
            write_mask: wgpu::ColorWrites::ALL,
        })];

        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("prism-shell ui pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &self.vertex_layouts,
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &targets,
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Always,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: 1,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache: None,
        })
    }
}

fn make_ui_buffer(
    device: &wgpu::Device,
    number_of_triangles: usize,
    size: (u32, u32),
) -> (wgpu::Buffer, Vec<UiVertex>) {
    let vertices: Vec<UiVertex> = vec![UiVertex::new(size); number_of_triangles * 3];
    let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("prism-shell ui vbo"),
        contents: bytemuck::cast_slice(&vertices),
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
    });
    (buffer, vertices)
}

// ─── the renderer itself ─────────────────────────────────────────────

pub struct UiRenderer {
    vertices: Vec<UiVertex>,
    buffer: wgpu::Buffer,
    number_of_vertices: usize,
    pipeline: wgpu::RenderPipeline,

    pub font_system: FontSystem,
    swash_cache: SwashCache,
    viewport: Viewport,
    atlas: TextAtlas,
    text_renderer: TextRenderer,
    measurement_buffer: Buffer,
    lines: Vec<TextLine>,

    pub dpi_scale: f32,
}

impl UiRenderer {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pixel_format: wgpu::TextureFormat,
        size: (u32, u32),
        dpi_scale: f32,
    ) -> Self {
        let (buffer, vertices) = make_ui_buffer(device, 10_000, size);

        let mut pipeline_builder = UiPipeline::new(pixel_format);
        pipeline_builder.add_layout(UiVertex::layout());
        let pipeline = pipeline_builder.build(device);

        let mut font_system = FontSystem::new();
        let swash_cache = SwashCache::new();
        let cache = Cache::new(device);
        let viewport = Viewport::new(device, &cache);
        let mut atlas = TextAtlas::new(device, queue, &cache, pixel_format);
        let text_renderer = TextRenderer::new(
            &mut atlas,
            device,
            MultisampleState::default(),
            Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
        );
        let measurement_buffer = Buffer::new(&mut font_system, Metrics::new(30.0, 42.0));

        Self {
            vertices,
            buffer,
            number_of_vertices: 0,
            pipeline,
            font_system,
            swash_cache,
            viewport,
            atlas,
            text_renderer,
            measurement_buffer,
            lines: Vec::new(),
            dpi_scale,
        }
    }

    pub fn resize(&mut self, size: (u32, u32)) {
        for vertex in self.vertices.as_mut_slice() {
            vertex.size.width = size.0 as f32;
            vertex.size.height = size.1 as f32;
        }
    }

    /// Shape a text string through glyphon and return Clay-friendly
    /// dimensions. Called from the text-measurement callback registered
    /// on the `Clay` instance.
    pub fn measure_text(&mut self, text: &str, font_size: f32, line_height: f32) -> Dimensions {
        self.measurement_buffer.set_metrics_and_size(
            &mut self.font_system,
            Metrics {
                font_size: font_size * self.dpi_scale,
                line_height: if line_height == 0.0 {
                    (font_size * 1.5) * self.dpi_scale
                } else {
                    line_height * self.dpi_scale
                },
            },
            None,
            None,
        );
        self.measurement_buffer.set_text(
            &mut self.font_system,
            text,
            Attrs::new().family(Family::SansSerif),
            Shaping::Advanced,
        );
        self.measurement_buffer
            .shape_until_scroll(&mut self.font_system, false);

        let run = self
            .measurement_buffer
            .layout_runs()
            .next()
            .map(|r| r.line_w)
            .unwrap_or(0.0);
        (run, self.measurement_buffer.metrics().line_height).into()
    }

    /// Walk a Clay render-command stream and push the resulting
    /// geometry + text into wgpu. The stream borrows from the `Clay`
    /// arena so this must run before `Clay::begin` fires again.
    pub fn render_clay<'a, I: 'a, C: 'a>(
        &mut self,
        render_commands: impl Iterator<Item = RenderCommand<'a, I, C>>,
        render_pass: &mut wgpu::RenderPass,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        surface_config: &wgpu::SurfaceConfiguration,
    ) {
        let mut scissor_pos = UiPosition::ZERO;
        let mut scissor_bounds = UiPosition::ZERO;
        let mut scissor_active = false;
        let mut depth: f32 = 0.1;

        for command in render_commands {
            let bbox = &command.bounding_box;
            match command.config {
                RenderCommandConfig::Rectangle(r) => {
                    self.filled_rectangle(
                        UiPosition::new(bbox.x, bbox.y, depth),
                        UiPosition::new(bbox.width, bbox.height, depth),
                        UiColor {
                            r: r.color.r / 255.0,
                            g: r.color.g / 255.0,
                            b: r.color.b / 255.0,
                        },
                        UiCornerRadii {
                            top_left: r.corner_radii.top_left,
                            top_right: r.corner_radii.top_right,
                            bottom_left: r.corner_radii.bottom_left,
                            bottom_right: r.corner_radii.bottom_right,
                        },
                    );
                }
                RenderCommandConfig::Border(b) => {
                    self.rectangle(
                        UiPosition::new(bbox.x, bbox.y, depth),
                        UiPosition::new(bbox.width, bbox.height, depth),
                        UiBorderThickness {
                            top: b.width.top as f32,
                            left: b.width.left as f32,
                            bottom: b.width.bottom as f32,
                            right: b.width.right as f32,
                        },
                        UiColor {
                            r: b.color.r / 255.0,
                            g: b.color.g / 255.0,
                            b: b.color.b / 255.0,
                        },
                        UiCornerRadii {
                            top_left: b.corner_radii.top_left,
                            top_right: b.corner_radii.top_right,
                            bottom_left: b.corner_radii.bottom_left,
                            bottom_right: b.corner_radii.bottom_right,
                        },
                    );
                }
                RenderCommandConfig::Text(text) => {
                    self.push_text(
                        text.text,
                        (text.font_size as f32) * self.dpi_scale,
                        if text.line_height == 0 {
                            (text.font_size as f32) * 1.5 * self.dpi_scale
                        } else {
                            (text.line_height as f32) * self.dpi_scale
                        },
                        UiPosition::new(bbox.x, bbox.y, depth),
                        if scissor_active {
                            Some((scissor_pos, scissor_bounds))
                        } else {
                            None
                        },
                        cosmic_text::Color::rgb(
                            text.color.r as u8,
                            text.color.g as u8,
                            text.color.b as u8,
                        ),
                        depth,
                    );
                }
                RenderCommandConfig::ScissorStart() => {
                    scissor_pos = UiPosition::new(bbox.x, bbox.y, depth);
                    scissor_bounds = UiPosition::new(bbox.width, bbox.height, depth);
                    scissor_active = true;
                }
                RenderCommandConfig::ScissorEnd() => {
                    scissor_active = false;
                }
                _ => {}
            }
            depth -= 0.0001;
        }

        if self.number_of_vertices > 0 {
            self.flush_vertices(render_pass, queue);
        }
        if !self.lines.is_empty() {
            self.flush_text(device, queue, render_pass, surface_config);
        }
    }

    fn flush_vertices(&mut self, render_pass: &mut wgpu::RenderPass, queue: &wgpu::Queue) {
        render_pass.set_pipeline(&self.pipeline);
        queue.write_buffer(
            &self.buffer,
            0,
            bytemuck::cast_slice(&self.vertices[..self.number_of_vertices]),
        );
        render_pass.set_vertex_buffer(0, self.buffer.slice(..));
        render_pass.draw(0..self.number_of_vertices as u32, 0..1);
        self.number_of_vertices = 0;
    }

    fn flush_text(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        render_pass: &mut wgpu::RenderPass,
        surface_config: &wgpu::SurfaceConfiguration,
    ) {
        self.atlas.trim();
        self.viewport.update(
            queue,
            Resolution {
                width: surface_config.width,
                height: surface_config.height,
            },
        );

        let areas: Vec<TextArea> = self
            .lines
            .iter()
            .map(|text_line| TextArea {
                buffer: &text_line.line,
                left: text_line.left,
                top: text_line.top,
                scale: 1.0,
                bounds: match text_line.bounds {
                    Some((p, b)) => TextBounds {
                        left: p.x as i32,
                        top: p.y as i32,
                        right: (p.x + b.x) as i32,
                        bottom: (p.y + b.y) as i32,
                    },
                    None => TextBounds {
                        left: 0,
                        top: 0,
                        right: surface_config.width as i32,
                        bottom: surface_config.height as i32,
                    },
                },
                default_color: text_line.color,
                custom_glyphs: &[],
            })
            .collect();

        self.text_renderer
            .prepare_with_depth(
                device,
                queue,
                &mut self.font_system,
                &mut self.atlas,
                &self.viewport,
                areas,
                &mut self.swash_cache,
                |metadata| (metadata as f32) / 10_000.0,
            )
            .unwrap();

        self.text_renderer
            .render(&self.atlas, &self.viewport, render_pass)
            .unwrap();

        self.lines.clear();
    }

    // ── primitive emitters ──────────────────────────────────────────

    fn triangle(&mut self, positions: &[UiPosition; 3], color: UiColor) {
        let Some(vertices) = self
            .vertices
            .get_mut(self.number_of_vertices..self.number_of_vertices + 3)
        else {
            return;
        };
        for (vertex, position) in vertices.iter_mut().zip(positions.iter()) {
            vertex.position = *position;
            vertex.color = color;
        }
        self.number_of_vertices += 3;
    }

    fn quad(&mut self, positions: &[UiPosition; 4], color: UiColor) {
        let Some(vertices) = self
            .vertices
            .get_mut(self.number_of_vertices..self.number_of_vertices + 6)
        else {
            return;
        };
        let indices = [0, 1, 2, 0, 2, 3];
        for (slot, index) in vertices.iter_mut().zip(indices) {
            slot.position = positions[index];
            slot.color = color;
        }
        self.number_of_vertices += 6;
    }

    fn line(
        &mut self,
        position: UiPosition,
        length: f32,
        angle: f32,
        thickness: f32,
        color: UiColor,
    ) {
        let mut line: [UiPosition; 4] = [UiPosition::ZERO; 4];
        line[0].add_y_mut(-(thickness / 2.0));
        line[1].add_y_mut(thickness / 2.0);
        line[2].add_x_mut(length).add_y_mut(thickness / 2.0);
        line[3].add_x_mut(length).add_y_mut(-(thickness / 2.0));

        for point in line.iter_mut() {
            point.rotate(angle);
            *point = *point + position;
        }

        self.quad(&line, color);
    }

    fn filled_arc(
        &mut self,
        origin: UiPosition,
        radius: f32,
        degree_begin: f32,
        degree_end: f32,
        color: UiColor,
    ) {
        if radius <= 0.0 {
            return;
        }
        let arc_length = (degree_end - degree_begin).abs();
        let segments = 10.0_f32;
        let seg_length = arc_length / segments;

        let mut current = UiPosition::new(0.0, 0.0, origin.z);
        let mut next = UiPosition::new(0.0, 0.0, origin.z);

        for i in 0..segments as i32 {
            current.x = radius;
            current.y = 0.0;
            current.rotate(degree_begin + (seg_length * (i as f32 + 1.0)));

            next.x = radius;
            next.y = 0.0;
            next.rotate(degree_begin + (seg_length * (i as f32)));

            self.triangle(&[current + origin, origin, next + origin], color);
        }
    }

    fn arc(
        &mut self,
        origin: UiPosition,
        radius: f32,
        degree_begin: f32,
        degree_end: f32,
        thickness: f32,
        color: UiColor,
    ) {
        if radius <= 0.0 || thickness <= 0.0 {
            return;
        }
        let arc_length = (degree_end - degree_begin).abs();
        let segments = 10.0_f32;
        let seg_length = arc_length / segments;
        let seg_distance = (2.0 * std::f32::consts::PI * radius) * (seg_length / 360.0);

        let mut arc_point = UiPosition::new(0.0, 0.0, origin.z);

        for i in 0..segments as i32 {
            arc_point.x = radius;
            arc_point.y = 0.0;
            arc_point.rotate(degree_begin + (seg_length * (i as f32)));
            arc_point = arc_point + origin;

            self.line(
                arc_point,
                seg_distance,
                degree_begin + 90.0 + (seg_length * i as f32) + (seg_length / 2.0),
                thickness,
                color,
            );
        }
    }

    fn rectangle(
        &mut self,
        position: UiPosition,
        size: UiPosition,
        thickness: UiBorderThickness,
        color: UiColor,
        radii: UiCornerRadii,
    ) {
        self.arc(
            position + radii.top_left,
            radii.top_left,
            90.0,
            180.0,
            thickness.top,
            color,
        );
        self.arc(
            position
                .with_x(size.x - radii.top_right)
                .with_y(radii.top_right),
            radii.top_right,
            0.0,
            90.0,
            thickness.top,
            color,
        );
        self.arc(
            position
                .with_y(size.y - radii.bottom_left)
                .with_x(radii.bottom_left),
            radii.bottom_left,
            180.0,
            270.0,
            thickness.bottom,
            color,
        );
        self.arc(
            position + (size - radii.bottom_right),
            radii.bottom_right,
            270.0,
            360.0,
            thickness.bottom,
            color,
        );

        self.line(
            position.with_x(radii.top_left),
            size.x - (radii.top_left + radii.top_right),
            0.0,
            thickness.top,
            color,
        );
        self.line(
            position.with_y(radii.top_left),
            size.y - (radii.top_left + radii.bottom_left),
            270.0,
            thickness.left,
            color,
        );
        self.line(
            position.with_x(radii.bottom_left).with_y(size.y),
            size.x - (radii.bottom_left + radii.bottom_right),
            0.0,
            thickness.bottom,
            color,
        );
        self.line(
            position.with_x(size.x).with_y(radii.top_right),
            size.y - (radii.top_right + radii.bottom_right),
            270.0,
            thickness.right,
            color,
        );
    }

    fn filled_rectangle(
        &mut self,
        position: UiPosition,
        size: UiPosition,
        color: UiColor,
        radii: UiCornerRadii,
    ) {
        if radii.top_left == 0.0
            && radii.top_right == 0.0
            && radii.bottom_left == 0.0
            && radii.bottom_right == 0.0
        {
            let bl = position.with_y(size.y);
            let br = position.with_x(size.x).with_y(size.y);
            let tr = position.with_x(size.x);
            self.quad(&[position, bl, br, tr], color);
            return;
        }

        self.filled_arc(
            position + radii.top_left,
            radii.top_left,
            90.0,
            180.0,
            color,
        );
        self.filled_arc(
            position
                .with_x(size.x - radii.top_right)
                .with_y(radii.top_right),
            radii.top_right,
            0.0,
            90.0,
            color,
        );
        self.filled_arc(
            position
                .with_y(size.y - radii.bottom_left)
                .with_x(radii.bottom_left),
            radii.bottom_left,
            180.0,
            270.0,
            color,
        );
        self.filled_arc(
            position + (size - radii.top_right),
            radii.bottom_right,
            270.0,
            360.0,
            color,
        );

        self.quad(
            &[
                position.with_x(radii.top_left),
                position + radii.top_left,
                position
                    .with_x(size.x - radii.top_right)
                    .with_y(radii.top_right),
                position.with_x(size.x - radii.top_right),
            ],
            color,
        );
        self.quad(
            &[
                position
                    .with_x(radii.bottom_left)
                    .with_y(size.y - radii.bottom_left),
                position.with_x(radii.bottom_left).with_y(size.y),
                position.with_x(size.x - radii.bottom_right).with_y(size.y),
                position
                    .with_x(size.x - radii.bottom_right)
                    .with_y(size.y - radii.bottom_right),
            ],
            color,
        );
        self.quad(
            &[
                position.with_y(radii.top_left),
                position.with_y(size.y - radii.bottom_left),
                position
                    .with_x(radii.bottom_left)
                    .with_y(size.y - radii.bottom_left),
                position + radii.top_left,
            ],
            color,
        );
        self.quad(
            &[
                position.with_x(size.x - radii.top_right),
                position
                    .with_x(size.x - radii.bottom_right)
                    .with_y(size.y - radii.bottom_right),
                position.with_x(size.x).with_y(size.y - radii.bottom_right),
                position.with_x(size.x).with_y(radii.top_right),
            ],
            color,
        );
        self.quad(
            &[
                position + radii.top_left,
                position
                    .with_x(radii.bottom_left)
                    .with_y(size.y - radii.bottom_left),
                position
                    .with_x(size.x - radii.bottom_right)
                    .with_y(size.y - radii.bottom_right),
                position
                    .with_x(size.x - radii.top_right)
                    .with_y(radii.top_right),
            ],
            color,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn push_text(
        &mut self,
        text: &str,
        font_size: f32,
        line_height: f32,
        position: UiPosition,
        bounds: Option<(UiPosition, UiPosition)>,
        color: cosmic_text::Color,
        draw_order: f32,
    ) {
        let mut line = Buffer::new(&mut self.font_system, Metrics::new(font_size, line_height));
        line.set_text(
            &mut self.font_system,
            text,
            Attrs::new()
                .family(Family::SansSerif)
                .metadata((draw_order * 10_000.0) as usize),
            Shaping::Advanced,
        );
        line.shape_until_scroll(&mut self.font_system, false);

        self.lines.push(TextLine {
            line,
            left: position.x,
            top: position.y,
            color,
            bounds,
        });
    }
}
