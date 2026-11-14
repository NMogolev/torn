//! GPU execution of Drawen display lists through `wgpu`.
//!
//! This renderer owns a presentable GPU surface and executes the same
//! [`drawen_render::DisplayList`] used by Drawen's deterministic software renderer.
//! Rectangles, rounded rectangles, borders, clipping, transforms, and rasterized
//! text are composited by the GPU. Geometry with identical clipping state is
//! batched, while text shares a single atlas and bind group per renderer.

use core::{fmt, ops::Range};
use std::collections::HashMap;

use bytemuck::{Pod, Zeroable};
use drawen_core::{Affine, Color, Point, Rect};
use drawen_render::{DisplayCommand, DisplayList, RasterImage, TextLayout, TextureId};

const TEXT_ATLAS_SIZE: u32 = 2048;

const SHAPE_SHADER: &str = r"
struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) local_position: vec2<f32>,
    @location(2) size: vec2<f32>,
    @location(3) radius: f32,
    @location(4) stroke_width: f32,
    @location(5) color: vec4<f32>,
    @location(6) blur: f32,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) local_position: vec2<f32>,
    @location(1) size: vec2<f32>,
    @location(2) radius: f32,
    @location(3) stroke_width: f32,
    @location(4) color: vec4<f32>,
    @location(5) blur: f32,
};

@vertex
fn vertex(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    output.position = vec4(input.position, 0.0, 1.0);
    output.local_position = input.local_position;
    output.size = input.size;
    output.radius = input.radius;
    output.stroke_width = input.stroke_width;
    output.color = input.color;
    output.blur = input.blur;
    return output;
}

fn rounded_box_distance(point: vec2<f32>, half_size: vec2<f32>, radius: f32) -> f32 {
    let corner = max(half_size - vec2(radius), vec2(0.0));
    let offset = abs(point) - corner;
    return length(max(offset, vec2(0.0))) + min(max(offset.x, offset.y), 0.0) - radius;
}

fn srgb_to_linear(color: vec3<f32>) -> vec3<f32> {
    return select(
        color / 12.92,
        pow((color + vec3(0.055)) / 1.055, vec3(2.4)),
        color > vec3(0.04045),
    );
}

@fragment
fn fragment(input: VertexOutput) -> @location(0) vec4<f32> {
    let distance = rounded_box_distance(
        input.local_position - input.size * 0.5,
        input.size * 0.5,
        input.radius,
    );
    // Signed-distance derivatives express the edge width in screen pixels,
    // so rounded corners stay smooth at every scale factor.
    let antialias_width = max(fwidth(distance), 0.001);
    var coverage = 1.0 - smoothstep(-antialias_width, antialias_width, distance);
    if (input.blur > 0.0) {
        // Shadows stay solid inside the shape and fade over `blur` logical
        // pixels beyond the edge. The quad is expanded by the same amount in
        // the vertex stage, so the falloff is never clipped.
        coverage = 1.0 - smoothstep(0.0, input.blur, distance);
    }
    if (input.stroke_width > 0.0) {
        let inner_coverage = smoothstep(
            -antialias_width,
            antialias_width,
            distance + input.stroke_width,
        );
        coverage *= inner_coverage;
    }
    if (coverage <= 0.0) {
        discard;
    }
    let alpha = clamp(input.color.a, 0.0, 1.0) * coverage;
    return vec4(srgb_to_linear(clamp(input.color.rgb, vec3(0.0), vec3(1.0))) * alpha, alpha);
}
";

const TEXT_SHADER: &str = r"
struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vertex(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    output.position = vec4(input.position, 0.0, 1.0);
    output.uv = input.uv;
    return output;
}

@group(0) @binding(0) var glyph_texture: texture_2d<f32>;
@group(0) @binding(1) var glyph_sampler: sampler;

fn srgb_to_linear(color: vec3<f32>) -> vec3<f32> {
    return select(
        color / 12.92,
        pow((color + vec3(0.055)) / 1.055, vec3(2.4)),
        color > vec3(0.04045),
    );
}

@fragment
fn fragment(input: VertexOutput) -> @location(0) vec4<f32> {
    let sample = textureSample(glyph_texture, glyph_sampler, input.uv);
    return vec4(srgb_to_linear(sample.rgb) * sample.a, sample.a);
}
";

const TEXTURE_SHADER: &str = r"
struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vertex(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    output.position = vec4(input.position, 0.0, 1.0);
    output.uv = input.uv;
    return output;
}

@group(0) @binding(0) var source_texture: texture_2d<f32>;
@group(0) @binding(1) var source_sampler: sampler;

fn srgb_to_linear(color: vec3<f32>) -> vec3<f32> {
    return select(
        color / 12.92,
        pow((color + vec3(0.055)) / 1.055, vec3(2.4)),
        color > vec3(0.04045),
    );
}

@fragment
fn fragment(input: VertexOutput) -> @location(0) vec4<f32> {
    let sample = textureSample(source_texture, source_sampler, input.uv);
    let alpha = clamp(sample.a, 0.0, 1.0);
    if (alpha <= 0.0) {
        return vec4(0.0);
    }
    // Browser compositors produce premultiplied sRGB values. Drawen's blend
    // state expects premultiplied linear values.
    let straight = sample.rgb / alpha;
    return vec4(srgb_to_linear(straight) * alpha, alpha);
}
";

/// GPU presentation or initialization failure.
#[derive(Debug)]
pub enum GpuError {
    /// `wgpu` could not create a surface for the native window target.
    SurfaceCreation(wgpu::CreateSurfaceError),
    /// No compatible GPU adapter was available for the surface.
    Adapter(wgpu::RequestAdapterError),
    /// The selected adapter could not create a logical GPU device.
    Device(wgpu::RequestDeviceError),
    /// The surface does not support the selected adapter.
    UnsupportedSurface,
    /// A zero-sized surface was requested.
    InvalidSurfaceSize,
}

impl fmt::Display for GpuError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SurfaceCreation(error) => {
                write!(formatter, "could not create GPU surface: {error}")
            }
            Self::Adapter(error) => write!(
                formatter,
                "could not find a compatible GPU adapter: {error}"
            ),
            Self::Device(error) => write!(formatter, "could not create GPU device: {error}"),
            Self::UnsupportedSurface => {
                formatter.write_str("GPU adapter does not support the window surface")
            }
            Self::InvalidSurfaceSize => {
                formatter.write_str("GPU surface dimensions must be non-zero")
            }
        }
    }
}

impl std::error::Error for GpuError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::SurfaceCreation(error) => Some(error),
            Self::Adapter(error) => Some(error),
            Self::Device(error) => Some(error),
            Self::UnsupportedSurface | Self::InvalidSurfaceSize => None,
        }
    }
}

/// Why a [`GpuRenderer`] could not execute a display list.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GpuRendererError {
    /// A coordinate, radius, width, or transform component was not finite.
    NonFiniteGeometry,
    /// A scale factor was not finite and positive.
    InvalidScaleFactor,
    /// A restore was issued without a preceding save.
    UnmatchedRestore,
    /// A clip pop was issued without a preceding clip.
    UnmatchedClipPop,
    /// Rendering ended with a state that was never restored.
    UnmatchedSave,
    /// A display list referenced a texture that was not registered with this
    /// renderer.
    UnknownTexture,
    /// Text for a frame does not fit in the fixed-size glyph atlas.
    TextAtlasFull,
}

impl fmt::Display for GpuRendererError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::NonFiniteGeometry => "renderer received non-finite or negative geometry",
            Self::InvalidScaleFactor => "render scale factor must be finite and positive",
            Self::UnmatchedRestore => "display list restored an empty state stack",
            Self::UnmatchedClipPop => "display list popped an empty clip stack",
            Self::UnmatchedSave => "display list ended with an unbalanced save",
            Self::UnknownTexture => "display list referenced an unregistered GPU texture",
            Self::TextAtlasFull => "text for this frame exceeds the 2048 by 2048 glyph atlas",
        })
    }
}

impl std::error::Error for GpuRendererError {}

/// A `wgpu` display-list renderer attached to one native surface.
pub struct GpuRenderer {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    shape_pipeline: wgpu::RenderPipeline,
    text_pipeline: wgpu::RenderPipeline,
    texture_pipeline: wgpu::RenderPipeline,
    texture_bind_group_layout: wgpu::BindGroupLayout,
    text_atlas: wgpu::Texture,
    text_atlas_bind_group: wgpu::BindGroup,
    external_textures: HashMap<TextureId, ExternalTexture>,
}

impl GpuRenderer {
    /// Creates a renderer for `target` at the supplied physical size.
    ///
    /// Passing an `Arc<winit::window::Window>` lets `wgpu` retain the window for
    /// the full surface lifetime while this crate stays independent of winit.
    ///
    /// # Errors
    ///
    /// Returns an error when a surface, adapter, device, or initial surface
    /// configuration cannot be created.
    pub fn new(
        target: impl Into<wgpu::SurfaceTarget<'static>>,
        width: u32,
        height: u32,
    ) -> Result<Self, GpuError> {
        if width == 0 || height == 0 {
            return Err(GpuError::InvalidSurfaceSize);
        }
        let instance = wgpu::Instance::default();
        let surface = instance
            .create_surface(target)
            .map_err(GpuError::SurfaceCreation)?;
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            compatible_surface: Some(&surface),
            ..Default::default()
        }))
        .map_err(GpuError::Adapter)?;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("Drawen GPU device"),
            ..Default::default()
        }))
        .map_err(GpuError::Device)?;
        let config = surface
            .get_default_config(&adapter, width, height)
            .ok_or(GpuError::UnsupportedSurface)?;
        surface.configure(&device, &config);

        let shape_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Drawen shape shader"),
            source: wgpu::ShaderSource::Wgsl(SHAPE_SHADER.into()),
        });
        let text_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Drawen text shader"),
            source: wgpu::ShaderSource::Wgsl(TEXT_SHADER.into()),
        });
        let texture_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Drawen external texture shader"),
            source: wgpu::ShaderSource::Wgsl(TEXTURE_SHADER.into()),
        });
        let texture_bind_group_layout = create_texture_bind_group_layout(&device);
        let shape_pipeline = create_shape_pipeline(&device, &shape_shader, config.format);
        let text_pipeline = create_text_pipeline(
            &device,
            &text_shader,
            config.format,
            &texture_bind_group_layout,
        );
        let texture_pipeline = create_texture_pipeline(
            &device,
            &texture_shader,
            config.format,
            &texture_bind_group_layout,
        );
        let text_atlas = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Drawen glyph atlas"),
            size: wgpu::Extent3d {
                width: TEXT_ATLAS_SIZE,
                height: TEXT_ATLAS_SIZE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let text_atlas_view = text_atlas.create_view(&wgpu::TextureViewDescriptor::default());
        let text_atlas_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Drawen glyph atlas sampler"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let text_atlas_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Drawen glyph atlas bind group"),
            layout: &texture_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&text_atlas_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&text_atlas_sampler),
                },
            ],
        });

        Ok(Self {
            surface,
            device,
            queue,
            config,
            shape_pipeline,
            text_pipeline,
            texture_pipeline,
            texture_bind_group_layout,
            text_atlas,
            text_atlas_bind_group,
            external_textures: HashMap::new(),
        })
    }

    /// Returns the `wgpu` device used by this renderer.
    ///
    /// Browser surfaces allocate their shared textures on this device.
    #[must_use]
    pub const fn device(&self) -> &wgpu::Device {
        &self.device
    }

    /// Returns the `wgpu` queue used by this renderer.
    ///
    /// Browser surfaces submit layout transitions and synchronization to this
    /// queue.
    #[must_use]
    pub const fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    /// Registers `texture` for display-list drawing under `id`.
    ///
    /// Re-registering an identifier replaces the previous texture, which is how
    /// a browser surface publishes a texture that was recreated after a resize.
    /// The renderer keeps its own handle, so the caller may drop its reference.
    pub fn register_texture(&mut self, id: TextureId, texture: &wgpu::Texture) {
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = self.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Drawen external texture sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Drawen external texture bind group"),
            layout: &self.texture_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        self.external_textures.insert(
            id,
            ExternalTexture {
                _texture: texture.clone(),
                _view: view,
                _sampler: sampler,
                bind_group,
            },
        );
    }

    /// Removes a previously registered texture.
    ///
    /// Returns whether the identifier was registered.
    pub fn unregister_texture(&mut self, id: TextureId) -> bool {
        self.external_textures.remove(&id).is_some()
    }

    /// Reconfigures the surface to `width` by `height` physical pixels.
    ///
    /// # Errors
    ///
    /// Returns [`GpuError::InvalidSurfaceSize`] when either dimension is zero.
    pub fn resize(&mut self, width: u32, height: u32) -> Result<(), GpuError> {
        if width == 0 || height == 0 {
            return Err(GpuError::InvalidSurfaceSize);
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
        Ok(())
    }

    /// Renders `display_list` at `scale_factor` logical-to-physical scaling.
    ///
    /// Returns the recoverable surface status when presentation should be retried
    /// after reconfiguring or on a future redraw.
    ///
    /// # Errors
    ///
    /// Returns [`GpuRendererError`] when the display list has invalid geometry
    /// or unbalanced paint state.
    pub fn render(
        &self,
        display_list: &DisplayList,
        scale_factor: f32,
    ) -> Result<RenderStatus, GpuRendererError> {
        if !scale_factor.is_finite() || scale_factor <= 0.0 {
            return Err(GpuRendererError::InvalidScaleFactor);
        }
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame) => frame,
            wgpu::CurrentSurfaceTexture::Suboptimal(frame) => {
                self.render_frame(display_list, scale_factor, frame)?;
                return Ok(RenderStatus::Suboptimal);
            }
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return Ok(RenderStatus::Skipped);
            }
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                return Ok(RenderStatus::Reconfigure);
            }
            wgpu::CurrentSurfaceTexture::Validation => return Ok(RenderStatus::Skipped),
        };
        self.render_frame(display_list, scale_factor, frame)?;
        Ok(RenderStatus::Presented)
    }

    fn render_frame(
        &self,
        display_list: &DisplayList,
        scale_factor: f32,
        frame: wgpu::SurfaceTexture,
    ) -> Result<(), GpuRendererError> {
        let mut prepared = PreparedFrame::new(self.config.width, self.config.height, scale_factor);
        prepared.record(display_list)?;
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let shape_buffer = (!prepared.shape_vertices.is_empty()).then(|| {
            create_buffer(
                &self.device,
                "Drawen shape vertices",
                &prepared.shape_vertices,
            )
        });
        let text_batch = self.create_text_batch(&prepared.texts)?;
        let text_buffer = (!text_batch.vertices.is_empty())
            .then(|| create_buffer(&self.device, "Drawen text vertices", &text_batch.vertices));
        let texture_buffer = (!prepared.texture_vertices.is_empty()).then(|| {
            create_buffer(
                &self.device,
                "Drawen external texture vertices",
                &prepared.texture_vertices,
            )
        });
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Drawen GPU frame"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Drawen GPU paint pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            if let Some(buffer) = &shape_buffer {
                pass.set_pipeline(&self.shape_pipeline);
                pass.set_vertex_buffer(0, buffer.slice(..));
                for draw in &prepared.shapes {
                    set_scissor(&mut pass, draw.scissor);
                    pass.draw(draw.vertices.clone(), 0..1);
                }
            }
            if let Some(buffer) = &text_buffer {
                pass.set_pipeline(&self.text_pipeline);
                pass.set_bind_group(0, &self.text_atlas_bind_group, &[]);
                pass.set_vertex_buffer(0, buffer.slice(..));
                for draw in &text_batch.draws {
                    set_scissor(&mut pass, draw.scissor);
                    pass.draw(draw.vertices.clone(), 0..1);
                }
            }
            if let Some(buffer) = &texture_buffer {
                pass.set_pipeline(&self.texture_pipeline);
                pass.set_vertex_buffer(0, buffer.slice(..));
                for draw in &prepared.textures {
                    set_scissor(&mut pass, draw.scissor);
                    let texture = self
                        .external_textures
                        .get(&draw.texture)
                        .ok_or(GpuRendererError::UnknownTexture)?;
                    pass.set_bind_group(0, &texture.bind_group, &[]);
                    pass.draw(draw.vertices.clone(), 0..1);
                }
            }
        }
        self.queue.submit([encoder.finish()]);
        frame.present();
        Ok(())
    }

    fn create_text_batch(&self, texts: &[PreparedText]) -> Result<TextBatch, GpuRendererError> {
        let mut batch = TextBatch::default();
        let (mut cursor_x, mut cursor_y, mut row_height): (u32, u32, u32) = (0, 0, 0);
        for text in texts {
            if text.width > TEXT_ATLAS_SIZE || text.height > TEXT_ATLAS_SIZE {
                return Err(GpuRendererError::TextAtlasFull);
            }
            if cursor_x + text.width > TEXT_ATLAS_SIZE {
                cursor_x = 0;
                cursor_y = cursor_y.saturating_add(row_height);
                row_height = 0;
            }
            if cursor_y + text.height > TEXT_ATLAS_SIZE {
                return Err(GpuRendererError::TextAtlasFull);
            }
            self.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &self.text_atlas,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: cursor_x,
                        y: cursor_y,
                        z: 0,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                &text.pixels,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(text.width * 4),
                    rows_per_image: Some(text.height),
                },
                wgpu::Extent3d {
                    width: text.width,
                    height: text.height,
                    depth_or_array_layers: 1,
                },
            );
            let start = u32::try_from(batch.vertices.len())
                .map_err(|_| GpuRendererError::NonFiniteGeometry)?;
            let atlas_size = u32_to_f32(TEXT_ATLAS_SIZE);
            let u0 = u32_to_f32(cursor_x) / atlas_size;
            let v0 = u32_to_f32(cursor_y) / atlas_size;
            let u1 = u32_to_f32(cursor_x + text.width) / atlas_size;
            let v1 = u32_to_f32(cursor_y + text.height) / atlas_size;
            for (vertex, uv) in text.vertices.iter().zip([
                [u0, v0],
                [u1, v0],
                [u0, v1],
                [u0, v1],
                [u1, v0],
                [u1, v1],
            ]) {
                batch.vertices.push(TextVertex {
                    position: vertex.position,
                    uv,
                });
            }
            batch.push_draw(start..start + 6, text.scissor);
            cursor_x += text.width;
            row_height = row_height.max(text.height);
        }
        Ok(batch)
    }
}

/// Result of attempting to present a frame.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RenderStatus {
    /// The frame was queued and presented.
    Presented,
    /// The frame was presented but the surface should be reconfigured soon.
    Suboptimal,
    /// The surface needs reconfiguration before another frame can be drawn.
    Reconfigure,
    /// The OS or GPU skipped this frame without a fatal error.
    Skipped,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct ShapeVertex {
    position: [f32; 2],
    local_position: [f32; 2],
    size: [f32; 2],
    radius: f32,
    stroke_width: f32,
    color: [f32; 4],
    blur: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct TextVertex {
    position: [f32; 2],
    uv: [f32; 2],
}

#[derive(Clone)]
struct RenderState {
    transform: Affine,
    clips: Vec<Clip>,
}

impl Default for RenderState {
    fn default() -> Self {
        Self {
            transform: Affine::IDENTITY,
            clips: Vec::new(),
        }
    }
}

#[derive(Clone, Copy)]
struct Clip {
    rect: Rect,
    transform: Affine,
}

struct ShapeDraw {
    vertices: Range<u32>,
    scissor: Scissor,
}

struct PreparedText {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
    vertices: [TextVertex; 6],
    scissor: Scissor,
}

struct PreparedTexture {
    vertices: Range<u32>,
    texture: TextureId,
    scissor: Scissor,
}

#[derive(Default)]
struct TextBatch {
    vertices: Vec<TextVertex>,
    draws: Vec<ShapeDraw>,
}

impl TextBatch {
    fn push_draw(&mut self, vertices: Range<u32>, scissor: Scissor) {
        if let Some(previous) = self.draws.last_mut()
            && previous.scissor == scissor
            && previous.vertices.end == vertices.start
        {
            previous.vertices.end = vertices.end;
            return;
        }
        self.draws.push(ShapeDraw { vertices, scissor });
    }
}

struct ExternalTexture {
    _texture: wgpu::Texture,
    _view: wgpu::TextureView,
    _sampler: wgpu::Sampler,
    bind_group: wgpu::BindGroup,
}

struct PreparedFrame {
    width: u32,
    height: u32,
    scale_factor: f32,
    shape_vertices: Vec<ShapeVertex>,
    shapes: Vec<ShapeDraw>,
    texts: Vec<PreparedText>,
    texture_vertices: Vec<TextVertex>,
    textures: Vec<PreparedTexture>,
    state: RenderState,
    saved_states: Vec<RenderState>,
}

impl PreparedFrame {
    fn push_shape_draw(&mut self, vertices: Range<u32>, scissor: Scissor) {
        if let Some(previous) = self.shapes.last_mut()
            && previous.scissor == scissor
            && previous.vertices.end == vertices.start
        {
            previous.vertices.end = vertices.end;
            return;
        }
        self.shapes.push(ShapeDraw { vertices, scissor });
    }
}

impl PreparedFrame {
    fn new(width: u32, height: u32, scale_factor: f32) -> Self {
        Self {
            width,
            height,
            scale_factor,
            shape_vertices: Vec::new(),
            shapes: Vec::new(),
            texts: Vec::new(),
            texture_vertices: Vec::new(),
            textures: Vec::new(),
            state: RenderState::default(),
            saved_states: Vec::new(),
        }
    }

    fn record(&mut self, display_list: &DisplayList) -> Result<(), GpuRendererError> {
        for command in display_list.commands() {
            match command {
                DisplayCommand::Save => self.saved_states.push(self.state.clone()),
                DisplayCommand::Restore => {
                    self.state = self
                        .saved_states
                        .pop()
                        .ok_or(GpuRendererError::UnmatchedRestore)?;
                }
                DisplayCommand::PopClip => {
                    self.state
                        .clips
                        .pop()
                        .ok_or(GpuRendererError::UnmatchedClipPop)?;
                }
                DisplayCommand::Transform { transform } => {
                    if !transform.is_finite() {
                        return Err(GpuRendererError::NonFiniteGeometry);
                    }
                    self.state.transform = self.state.transform.then(*transform);
                }
                DisplayCommand::PushClip { rect } => {
                    validate_rect(*rect)?;
                    self.state.clips.push(Clip {
                        rect: *rect,
                        transform: self.state.transform,
                    });
                }
                DisplayCommand::FillRect { rect, color } => {
                    self.add_shape(*rect, 0.0, 0.0, 0.0, *color)?;
                }
                DisplayCommand::FillRoundedRect {
                    rect,
                    radius,
                    color,
                } => {
                    self.add_shape(*rect, *radius, 0.0, 0.0, *color)?;
                }
                DisplayCommand::Shadow {
                    rect,
                    radius,
                    blur,
                    color,
                } => {
                    self.add_shape(*rect, *radius, 0.0, *blur, *color)?;
                }
                DisplayCommand::StrokeRect { rect, width, color } => {
                    self.add_shape(*rect, 0.0, *width, 0.0, *color)?;
                }
                DisplayCommand::StrokeRoundedRect {
                    rect,
                    radius,
                    width,
                    color,
                } => {
                    self.add_shape(*rect, *radius, *width, 0.0, *color)?;
                }
                DisplayCommand::DrawText { layout, origin } => {
                    self.add_text(layout, *origin)?;
                }
                DisplayCommand::DrawImage { image, destination } => {
                    self.add_image(image, *destination)?;
                }
                DisplayCommand::DrawTexture {
                    texture,
                    destination,
                } => {
                    self.add_texture(*texture, *destination)?;
                }
            }
        }
        self.saved_states
            .is_empty()
            .then_some(())
            .ok_or(GpuRendererError::UnmatchedSave)
    }

    fn add_shape(
        &mut self,
        rect: Rect,
        radius: f32,
        stroke_width: f32,
        blur: f32,
        color: Color,
    ) -> Result<(), GpuRendererError> {
        validate_rect(rect)?;
        validate_radius(radius)?;
        validate_stroke_width(stroke_width)?;
        validate_blur(blur)?;
        validate_color(color)?;
        if rect.size.width() == 0.0
            || rect.size.height() == 0.0
            || stroke_width == 0.0 && color.alpha <= 0.0
        {
            return Ok(());
        }
        let (rect, radius) = if stroke_width > 0.0 {
            let half = stroke_width * 0.5;
            (
                Rect::new(
                    Point::new(rect.origin.x - half, rect.origin.y - half),
                    drawen_core::Size::new(
                        rect.size.width() + stroke_width,
                        rect.size.height() + stroke_width,
                    )
                    .expect("validated finite stroke expansion"),
                ),
                (radius + half).min(
                    (rect.size.width() + stroke_width).min(rect.size.height() + stroke_width) * 0.5,
                ),
            )
        } else {
            (
                rect,
                radius.min(rect.size.width().min(rect.size.height()) * 0.5),
            )
        };
        let start = u32::try_from(self.shape_vertices.len()).unwrap_or(u32::MAX);
        if start == u32::MAX {
            return Err(GpuRendererError::NonFiniteGeometry);
        }
        let size = [rect.size.width(), rect.size.height()];
        // The quad grows by `blur` on every side, while `local_position` stays
        // relative to the original shape so the fragment shader evaluates the
        // same rounded box and only the falloff changes.
        let quad_size = [
            rect.size.width() + blur * 2.0,
            rect.size.height() + blur * 2.0,
        ];
        for (x, y) in [
            (0.0, 0.0),
            (1.0, 0.0),
            (0.0, 1.0),
            (0.0, 1.0),
            (1.0, 0.0),
            (1.0, 1.0),
        ] {
            let local = Point::new(x * quad_size[0] - blur, y * quad_size[1] - blur);
            let point = self
                .state
                .transform
                .transform_point(Point::new(rect.origin.x + local.x, rect.origin.y + local.y));
            self.shape_vertices.push(ShapeVertex {
                position: self.ndc(point),
                local_position: [local.x, local.y],
                size,
                radius,
                stroke_width,
                color: [color.red, color.green, color.blue, color.alpha],
                blur,
            });
        }
        let end = start
            .checked_add(6)
            .ok_or(GpuRendererError::NonFiniteGeometry)?;
        self.push_shape_draw(start..end, self.scissor());
        Ok(())
    }

    fn add_text(&mut self, layout: &TextLayout, origin: Point) -> Result<(), GpuRendererError> {
        if !origin.x.is_finite() || !origin.y.is_finite() {
            return Err(GpuRendererError::NonFiniteGeometry);
        }
        validate_color(layout.color())?;
        let width = logical_to_pixel(layout.size().width(), self.scale_factor)?;
        let height = logical_to_pixel(layout.size().height(), self.scale_factor)?;
        if width == 0 || height == 0 || layout.color().alpha <= 0.0 {
            return Ok(());
        }
        let byte_count = usize::try_from(u64::from(width) * u64::from(height) * 4)
            .map_err(|_| GpuRendererError::NonFiniteGeometry)?;
        let mut pixels = vec![0; byte_count];
        for run in layout.glyph_runs() {
            let raster_size = run.font_size() * self.scale_factor;
            if !raster_size.is_finite() || raster_size <= 0.0 {
                return Err(GpuRendererError::NonFiniteGeometry);
            }
            for glyph in run.glyphs() {
                let glyph_origin = glyph.position();
                let bitmap = run.font().rasterize_cached(glyph.glyph_id(), raster_size);
                for (index, coverage) in bitmap.coverage().iter().copied().enumerate() {
                    let x =
                        glyph_origin.x * self.scale_factor + usize_to_f32(index % bitmap.width());
                    let y =
                        glyph_origin.y * self.scale_factor + usize_to_f32(index / bitmap.width());
                    let Some(x) = coordinate_to_pixel(x, width) else {
                        continue;
                    };
                    let Some(y) = coordinate_to_pixel(y, height) else {
                        continue;
                    };
                    let alpha = f32::from(coverage) / 255.0 * layout.color().alpha.clamp(0.0, 1.0);
                    let offset = (usize::try_from(y)
                        .unwrap_or(usize::MAX)
                        .saturating_mul(usize::try_from(width).unwrap_or(usize::MAX))
                        .saturating_add(usize::try_from(x).unwrap_or(usize::MAX)))
                    .saturating_mul(4);
                    if offset + 3 >= pixels.len() || alpha <= f32::from(pixels[offset + 3]) / 255.0
                    {
                        continue;
                    }
                    pixels[offset] = component_to_u8(layout.color().red);
                    pixels[offset + 1] = component_to_u8(layout.color().green);
                    pixels[offset + 2] = component_to_u8(layout.color().blue);
                    pixels[offset + 3] = component_to_u8(alpha);
                }
            }
        }
        let logical_size = Point::new(
            usize_to_f32(usize::try_from(width).unwrap_or(usize::MAX)) / self.scale_factor,
            usize_to_f32(usize::try_from(height).unwrap_or(usize::MAX)) / self.scale_factor,
        );
        let positions = [
            Point::new(origin.x, origin.y),
            Point::new(origin.x + logical_size.x, origin.y),
            Point::new(origin.x, origin.y + logical_size.y),
            Point::new(origin.x, origin.y + logical_size.y),
            Point::new(origin.x + logical_size.x, origin.y),
            Point::new(origin.x + logical_size.x, origin.y + logical_size.y),
        ];
        let uvs = [
            [0.0, 0.0],
            [1.0, 0.0],
            [0.0, 1.0],
            [0.0, 1.0],
            [1.0, 0.0],
            [1.0, 1.0],
        ];
        let mut vertices = [TextVertex::zeroed(); 6];
        for (vertex, (position, uv)) in vertices.iter_mut().zip(positions.into_iter().zip(uvs)) {
            *vertex = TextVertex {
                position: self.ndc(self.state.transform.transform_point(position)),
                uv,
            };
        }
        self.texts.push(PreparedText {
            width,
            height,
            pixels,
            vertices,
            scissor: self.scissor(),
        });
        Ok(())
    }

    fn add_image(
        &mut self,
        image: &RasterImage,
        destination: Rect,
    ) -> Result<(), GpuRendererError> {
        validate_rect(destination)?;
        if image.width() == 0
            || image.height() == 0
            || destination.size.width() == 0.0
            || destination.size.height() == 0.0
        {
            return Ok(());
        }
        let positions = [
            destination.origin,
            Point::new(destination.right(), destination.origin.y),
            Point::new(destination.origin.x, destination.bottom()),
            Point::new(destination.origin.x, destination.bottom()),
            Point::new(destination.right(), destination.origin.y),
            Point::new(destination.right(), destination.bottom()),
        ];
        let uvs = [
            [0.0, 0.0],
            [1.0, 0.0],
            [0.0, 1.0],
            [0.0, 1.0],
            [1.0, 0.0],
            [1.0, 1.0],
        ];
        let mut vertices = [TextVertex::zeroed(); 6];
        for (vertex, (position, uv)) in vertices.iter_mut().zip(positions.into_iter().zip(uvs)) {
            *vertex = TextVertex {
                position: self.ndc(self.state.transform.transform_point(position)),
                uv,
            };
        }
        self.texts.push(PreparedText {
            width: image.width(),
            height: image.height(),
            pixels: image.pixels().to_vec(),
            vertices,
            scissor: self.scissor(),
        });
        Ok(())
    }

    fn add_texture(
        &mut self,
        texture: TextureId,
        destination: Rect,
    ) -> Result<(), GpuRendererError> {
        validate_rect(destination)?;
        if destination.size.width() == 0.0 || destination.size.height() == 0.0 {
            return Ok(());
        }
        let positions = [
            destination.origin,
            Point::new(destination.right(), destination.origin.y),
            Point::new(destination.origin.x, destination.bottom()),
            Point::new(destination.origin.x, destination.bottom()),
            Point::new(destination.right(), destination.origin.y),
            Point::new(destination.right(), destination.bottom()),
        ];
        let uvs = [
            [0.0, 0.0],
            [1.0, 0.0],
            [0.0, 1.0],
            [0.0, 1.0],
            [1.0, 0.0],
            [1.0, 1.0],
        ];
        let start = u32::try_from(self.texture_vertices.len()).unwrap_or(u32::MAX);
        if start == u32::MAX {
            return Err(GpuRendererError::NonFiniteGeometry);
        }
        for (position, uv) in positions.into_iter().zip(uvs) {
            self.texture_vertices.push(TextVertex {
                position: self.ndc(self.state.transform.transform_point(position)),
                uv,
            });
        }
        let end = start
            .checked_add(6)
            .ok_or(GpuRendererError::NonFiniteGeometry)?;
        self.textures.push(PreparedTexture {
            vertices: start..end,
            texture,
            scissor: self.scissor(),
        });
        Ok(())
    }

    fn ndc(&self, point: Point) -> [f32; 2] {
        let width = u32_to_f32(self.width);
        let height = u32_to_f32(self.height);
        [
            point.x * self.scale_factor / width * 2.0 - 1.0,
            1.0 - point.y * self.scale_factor / height * 2.0,
        ]
    }

    fn scissor(&self) -> Scissor {
        self.state
            .clips
            .iter()
            .fold(Scissor::full(self.width, self.height), |current, clip| {
                current.intersect(Scissor::from_rect(
                    clip.rect,
                    clip.transform,
                    self.scale_factor,
                    self.width,
                    self.height,
                ))
            })
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
struct Scissor {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

impl Scissor {
    const fn full(width: u32, height: u32) -> Self {
        Self {
            x: 0,
            y: 0,
            width,
            height,
        }
    }

    fn from_rect(
        rect: Rect,
        transform: Affine,
        scale_factor: f32,
        width: u32,
        height: u32,
    ) -> Self {
        let points = [
            rect.origin,
            Point::new(rect.right(), rect.origin.y),
            Point::new(rect.origin.x, rect.bottom()),
            Point::new(rect.right(), rect.bottom()),
        ]
        .map(|point| transform.transform_point(point));
        let min_x = points
            .iter()
            .map(|point| point.x)
            .fold(f32::INFINITY, f32::min)
            * scale_factor;
        let min_y = points
            .iter()
            .map(|point| point.y)
            .fold(f32::INFINITY, f32::min)
            * scale_factor;
        let max_x = points
            .iter()
            .map(|point| point.x)
            .fold(f32::NEG_INFINITY, f32::max)
            * scale_factor;
        let max_y = points
            .iter()
            .map(|point| point.y)
            .fold(f32::NEG_INFINITY, f32::max)
            * scale_factor;
        let left = clamp_pixel(min_x.floor(), width);
        let top = clamp_pixel(min_y.floor(), height);
        let right = clamp_pixel(max_x.ceil(), width);
        let bottom = clamp_pixel(max_y.ceil(), height);
        Self {
            x: left,
            y: top,
            width: right.saturating_sub(left),
            height: bottom.saturating_sub(top),
        }
    }

    fn intersect(self, other: Self) -> Self {
        let left = self.x.max(other.x);
        let top = self.y.max(other.y);
        let right = self
            .x
            .saturating_add(self.width)
            .min(other.x.saturating_add(other.width));
        let bottom = self
            .y
            .saturating_add(self.height)
            .min(other.y.saturating_add(other.height));
        Self {
            x: left,
            y: top,
            width: right.saturating_sub(left),
            height: bottom.saturating_sub(top),
        }
    }
}

fn create_shape_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Drawen shape pipeline"),
        layout: None,
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vertex"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<ShapeVertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![
                    0 => Float32x2,
                    1 => Float32x2,
                    2 => Float32x2,
                    3 => Float32,
                    4 => Float32,
                    5 => Float32x4,
                    6 => Float32,
                ],
            }],
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fragment"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

fn create_texture_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Drawen textured quad bind group layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    multisampled: false,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    })
}

fn create_text_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    format: wgpu::TextureFormat,
    bind_group_layout: &wgpu::BindGroupLayout,
) -> wgpu::RenderPipeline {
    create_textured_pipeline(device, shader, format, bind_group_layout, "text")
}

fn create_texture_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    format: wgpu::TextureFormat,
    bind_group_layout: &wgpu::BindGroupLayout,
) -> wgpu::RenderPipeline {
    create_textured_pipeline(
        device,
        shader,
        format,
        bind_group_layout,
        "external texture",
    )
}

fn create_textured_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    format: wgpu::TextureFormat,
    bind_group_layout: &wgpu::BindGroupLayout,
    kind: &str,
) -> wgpu::RenderPipeline {
    let pipeline_label = format!("Drawen {kind} pipeline");
    let layout_label = format!("Drawen {kind} pipeline layout");
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some(&layout_label),
        bind_group_layouts: &[Some(bind_group_layout)],
        immediate_size: 0,
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(&pipeline_label),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vertex"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<TextVertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2],
            }],
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fragment"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

fn create_buffer<T: Pod>(device: &wgpu::Device, label: &str, values: &[T]) -> wgpu::Buffer {
    use wgpu::util::DeviceExt;

    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(label),
        contents: bytemuck::cast_slice(values),
        usage: wgpu::BufferUsages::VERTEX,
    })
}

fn set_scissor(pass: &mut wgpu::RenderPass<'_>, scissor: Scissor) {
    pass.set_scissor_rect(scissor.x, scissor.y, scissor.width, scissor.height);
}

fn validate_rect(rect: Rect) -> Result<(), GpuRendererError> {
    if rect.origin.x.is_finite()
        && rect.origin.y.is_finite()
        && rect.size.width().is_finite()
        && rect.size.height().is_finite()
    {
        Ok(())
    } else {
        Err(GpuRendererError::NonFiniteGeometry)
    }
}

fn validate_radius(radius: f32) -> Result<(), GpuRendererError> {
    (radius.is_finite() && radius >= 0.0)
        .then_some(())
        .ok_or(GpuRendererError::NonFiniteGeometry)
}

fn validate_stroke_width(width: f32) -> Result<(), GpuRendererError> {
    (width.is_finite() && width >= 0.0)
        .then_some(())
        .ok_or(GpuRendererError::NonFiniteGeometry)
}

fn validate_blur(blur: f32) -> Result<(), GpuRendererError> {
    (blur.is_finite() && blur >= 0.0)
        .then_some(())
        .ok_or(GpuRendererError::NonFiniteGeometry)
}

fn validate_color(color: Color) -> Result<(), GpuRendererError> {
    (color.red.is_finite()
        && color.green.is_finite()
        && color.blue.is_finite()
        && color.alpha.is_finite())
    .then_some(())
    .ok_or(GpuRendererError::NonFiniteGeometry)
}

fn logical_to_pixel(value: f32, scale_factor: f32) -> Result<u32, GpuRendererError> {
    if !value.is_finite() || value < 0.0 {
        return Err(GpuRendererError::NonFiniteGeometry);
    }
    let value = (value * scale_factor).ceil();
    if !value.is_finite() || value >= 4_294_967_296.0 {
        return Err(GpuRendererError::NonFiniteGeometry);
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Ok(value as u32)
}

fn coordinate_to_pixel(value: f32, limit: u32) -> Option<u32> {
    (value.is_finite() && value >= 0.0 && value < u32_to_f32(limit)).then(|| {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        {
            value.floor() as u32
        }
    })
}

fn component_to_u8(value: f32) -> u8 {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    {
        (value.clamp(0.0, 1.0) * 255.0).round() as u8
    }
}

fn usize_to_f32(value: usize) -> f32 {
    #[allow(clippy::cast_precision_loss)]
    {
        value as f32
    }
}

fn clamp_pixel(value: f32, limit: u32) -> u32 {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    {
        value.clamp(0.0, u32_to_f32(limit)) as u32
    }
}

fn u32_to_f32(value: u32) -> f32 {
    #[allow(clippy::cast_precision_loss)]
    {
        value as f32
    }
}

#[cfg(test)]
mod tests {
    use drawen_core::{Color, Point, Rect, Size};
    use drawen_render::PaintContext;

    use super::{DisplayList, PreparedFrame, TextureId};

    #[test]
    fn records_external_texture_geometry() {
        let mut list = DisplayList::new();
        PaintContext::new(&mut list).draw_texture(
            TextureId::new(1),
            Rect::new(
                Point::new(2.0, 3.0),
                Size::new(10.0, 20.0).expect("valid test size"),
            ),
        );
        let mut frame = PreparedFrame::new(100, 100, 1.0);

        frame.record(&list).expect("valid display list");

        assert_eq!(frame.textures.len(), 1);
        assert_eq!(frame.texture_vertices.len(), 6);
    }

    #[test]
    fn batches_consecutive_shapes_with_the_same_scissor() {
        let mut list = DisplayList::new();
        let mut paint = PaintContext::new(&mut list);
        paint.fill_rect(
            Rect::new(Point::ZERO, Size::new(10.0, 10.0).expect("valid size")),
            Color::BLACK,
        );
        paint.fill_rect(
            Rect::new(
                Point::new(12.0, 0.0),
                Size::new(10.0, 10.0).expect("valid size"),
            ),
            Color::WHITE,
        );
        let mut frame = PreparedFrame::new(100, 100, 1.0);

        frame.record(&list).expect("valid display list");

        assert_eq!(frame.shape_vertices.len(), 12);
        assert_eq!(frame.shapes.len(), 1);
        assert_eq!(frame.shapes[0].vertices, 0..12);
    }
}
