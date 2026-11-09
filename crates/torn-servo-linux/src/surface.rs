//! Linear Vulkan image export, `EGLImage` import, and wgpu Vulkan ownership.

use core::{ffi::c_void, fmt, marker::PhantomData};
use std::rc::Rc;

use ash::vk;
use khronos_egl::{
    Context as EglContext, Display, DynamicInstance, EGL1_5, EGLContext as RawEglContext,
    EGLDisplay, EGLImage, NO_CONTEXT,
};
use libloading::Library;
use wgpu::{
    Extent3d, Texture, TextureDescriptor, TextureDimension, TextureFormat, TextureTransition,
    TextureUsages, TextureUses,
};

const EGL_LINUX_DMA_BUF_EXT: u32 = 0x3270;
const EGL_LINUX_DRM_FOURCC_EXT: i32 = 0x3271;
const EGL_DMA_BUF_PLANE0_FD_EXT: i32 = 0x3272;
const EGL_DMA_BUF_PLANE0_OFFSET_EXT: i32 = 0x3273;
const EGL_DMA_BUF_PLANE0_PITCH_EXT: i32 = 0x3274;
const EGL_DMA_BUF_PLANE0_MODIFIER_LO_EXT: i32 = 0x3443;
const EGL_DMA_BUF_PLANE0_MODIFIER_HI_EXT: i32 = 0x3444;
const EGL_NONE: i32 = 0x3038;
const EGL_WIDTH: i32 = 0x3057;
const EGL_HEIGHT: i32 = 0x3056;
const EGL_EXTENSIONS: i32 = 0x3055;

/// `DRM_FORMAT_ABGR8888`, whose little-endian byte order is R, G, B, A and
/// therefore matches `VK_FORMAT_R8G8B8A8_UNORM`.
const DRM_FORMAT_ABGR8888: i32 = 0x5247_4241;

const GL_TEXTURE_2D: u32 = 0x0DE1;
const GL_TEXTURE_MIN_FILTER: u32 = 0x2801;
const GL_TEXTURE_MAG_FILTER: u32 = 0x2800;
const GL_TEXTURE_WRAP_S: u32 = 0x2802;
const GL_TEXTURE_WRAP_T: u32 = 0x2803;
const GL_LINEAR: i32 = 0x2601;
const GL_CLAMP_TO_EDGE: i32 = 0x812F;

const IMAGE_DMA_BUF_EXTENSION: &str = "EGL_EXT_image_dma_buf_import";
const IMAGE_DMA_BUF_MODIFIERS_EXTENSION: &str = "EGL_EXT_image_dma_buf_import_modifiers";

type CreateImage =
    unsafe extern "system" fn(EGLDisplay, RawEglContext, u32, *mut c_void, *const i32) -> EGLImage;
type DestroyImage = unsafe extern "system" fn(EGLDisplay, EGLImage) -> u32;
type GlGenTextures = unsafe extern "system" fn(i32, *mut u32);
type GlBindTexture = unsafe extern "system" fn(u32, u32);
type GlTexParameteri = unsafe extern "system" fn(u32, u32, i32);
type GlEglImageTargetTexture = unsafe extern "system" fn(u32, EGLImage);
type GlDeleteTextures = unsafe extern "system" fn(i32, *const u32);
type GlFinish = unsafe extern "system" fn();
type GlGetError = unsafe extern "system" fn() -> u32;

/// Failure while preparing or synchronizing a shared dmabuf texture.
#[derive(Debug)]
pub enum InteropError {
    /// The requested texture dimensions cannot describe a GPU image.
    InvalidSize,
    /// The calling thread does not have the captured EGL context current.
    NotCurrentEglContext,
    /// `libEGL` could not be loaded or does not implement EGL 1.5.
    EglLibrary,
    /// No usable OpenGL ES or OpenGL library could be loaded.
    GlLibrary,
    /// The EGL display does not advertise a required extension.
    MissingEglExtension(&'static str),
    /// The Vulkan device does not advertise a required extension.
    MissingVulkanExtension(&'static str),
    /// An EGL or GL entry point could not be resolved.
    MissingFunction(&'static str),
    /// Torn's `wgpu` device is not backed by Vulkan.
    WgpuNotVulkan,
    /// A Vulkan operation failed.
    Vulkan(vk::Result),
    /// No Vulkan memory type could back the exportable image.
    ImageMemory,
    /// The image layout does not fit in an EGL attribute.
    LayoutOverflow,
    /// `eglCreateImageKHR` returned no image.
    EglImage,
    /// OpenGL reported an error while attaching the `EGLImage`.
    GlError(u32),
    /// The shared frame counter cannot advance without overflowing.
    ValueExhausted,
    /// Polling Torn's `wgpu` device failed.
    DevicePoll(wgpu::PollError),
}

impl fmt::Display for InteropError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSize => formatter.write_str("shared texture size must be non-zero"),
            Self::NotCurrentEglContext => {
                formatter.write_str("the captured EGL context is not current on this thread")
            }
            Self::EglLibrary => formatter.write_str("failed to load an EGL 1.5 library"),
            Self::GlLibrary => formatter.write_str("failed to load an OpenGL or GLES library"),
            Self::MissingEglExtension(extension) => {
                write!(formatter, "EGL display does not support {extension}")
            }
            Self::MissingVulkanExtension(extension) => {
                write!(formatter, "Vulkan device does not support {extension}")
            }
            Self::MissingFunction(function) => {
                write!(
                    formatter,
                    "could not resolve graphics entry point {function}"
                )
            }
            Self::WgpuNotVulkan => formatter.write_str("wgpu is not using the Vulkan backend"),
            Self::Vulkan(error) => write!(formatter, "Vulkan operation failed: {error:?}"),
            Self::ImageMemory => {
                formatter.write_str("no Vulkan memory type can back the exportable image")
            }
            Self::LayoutOverflow => {
                formatter.write_str("the dmabuf image layout does not fit in EGL attributes")
            }
            Self::EglImage => formatter.write_str("eglCreateImageKHR returned no image"),
            Self::GlError(error) => {
                write!(
                    formatter,
                    "OpenGL failed to attach the EGLImage: 0x{error:04X}"
                )
            }
            Self::ValueExhausted => formatter.write_str("shared frame value overflowed"),
            Self::DevicePoll(error) => {
                write!(formatter, "polling the wgpu device failed: {error}")
            }
        }
    }
}

impl std::error::Error for InteropError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::DevicePoll(error) => Some(error),
            _ => None,
        }
    }
}

impl From<vk::Result> for InteropError {
    fn from(error: vk::Result) -> Self {
        Self::Vulkan(error)
    }
}

impl From<wgpu::PollError> for InteropError {
    fn from(error: wgpu::PollError) -> Self {
        Self::DevicePoll(error)
    }
}

struct GlFunctions {
    gen_textures: GlGenTextures,
    bind_texture: GlBindTexture,
    tex_parameteri: GlTexParameteri,
    egl_image_target_texture: GlEglImageTargetTexture,
    delete_textures: GlDeleteTextures,
    finish: GlFinish,
    get_error: GlGetError,
}

struct ContextInner {
    egl: DynamicInstance<EGL1_5>,
    display: Display,
    context: EglContext,
    create_image: CreateImage,
    destroy_image: DestroyImage,
    gl: GlFunctions,
    supports_modifiers: bool,
    _gl_library: Library,
}

impl ContextInner {
    fn ensure_current(&self) -> Result<(), InteropError> {
        if self.egl.get_current_display() == Some(self.display)
            && self.egl.get_current_context() == Some(self.context)
        {
            Ok(())
        } else {
            Err(InteropError::NotCurrentEglContext)
        }
    }
}

/// The EGL display and context that own Servo's producer textures.
///
/// The context captures the EGL display and context that are current when it
/// is constructed. Every [`SharedTexture`] operation that touches OpenGL must
/// run on a thread where the same EGL context is current. GLX-only contexts are
/// not supported because a dmabuf must be imported with
/// `EGL_EXT_image_dma_buf_import`.
pub struct Context {
    inner: Rc<ContextInner>,
}

impl Context {
    /// Captures the current EGL display and context using the system `libEGL`.
    ///
    /// # Errors
    ///
    /// Returns an error when `libEGL`, an OpenGL library, or a required
    /// extension or entry point is unavailable, or when no EGL context is
    /// current on this thread.
    pub fn new() -> Result<Self, InteropError> {
        // SAFETY: loading an EGL implementation is fundamentally unsafe because
        // the library must conform to the EGL ABI. The system library path is
        // the conventional way to obtain it.
        let egl = unsafe { DynamicInstance::<EGL1_5>::load_required() }
            .map_err(|_| InteropError::EglLibrary)?;
        Self::from_egl(egl, load_gl_library(None)?)
    }

    /// Captures the current EGL display and context from explicit libraries.
    ///
    /// Use this when Servo loads EGL or OpenGL from a non-standard location,
    /// such as an ANGLE bundle. `egl_library` and `gl_library` are passed to
    /// the dynamic loader.
    ///
    /// # Errors
    ///
    /// Returns the same errors as [`Self::new`].
    pub fn with_libraries(egl_library: &str, gl_library: &str) -> Result<Self, InteropError> {
        // SAFETY: the caller names libraries that must conform to the EGL and
        // OpenGL ABIs respectively.
        let egl = unsafe { DynamicInstance::<EGL1_5>::load_required_from_filename(egl_library) }
            .map_err(|_| InteropError::EglLibrary)?;
        Self::from_egl(egl, load_gl_library(Some(gl_library))?)
    }

    fn from_egl(egl: DynamicInstance<EGL1_5>, gl_library: Library) -> Result<Self, InteropError> {
        let display = egl
            .get_current_display()
            .ok_or(InteropError::NotCurrentEglContext)?;
        let context = egl
            .get_current_context()
            .ok_or(InteropError::NotCurrentEglContext)?;

        let extensions = egl
            .query_string(Some(display), EGL_EXTENSIONS)
            .map_err(|_| InteropError::MissingEglExtension(IMAGE_DMA_BUF_EXTENSION))?
            .to_string_lossy();
        let supports_modifiers = has_extension(&extensions, IMAGE_DMA_BUF_MODIFIERS_EXTENSION);
        if !has_extension(&extensions, IMAGE_DMA_BUF_EXTENSION) {
            return Err(InteropError::MissingEglExtension(IMAGE_DMA_BUF_EXTENSION));
        }

        let create_image = load_egl_proc(&egl, "eglCreateImageKHR")?;
        let destroy_image = load_egl_proc(&egl, "eglDestroyImageKHR")?;
        let gl = GlFunctions {
            gen_textures: load_gl_proc(&egl, &gl_library, "glGenTextures")?,
            bind_texture: load_gl_proc(&egl, &gl_library, "glBindTexture")?,
            tex_parameteri: load_gl_proc(&egl, &gl_library, "glTexParameteri")?,
            egl_image_target_texture: load_gl_proc(
                &egl,
                &gl_library,
                "glEGLImageTargetTexture2DOES",
            )?,
            delete_textures: load_gl_proc(&egl, &gl_library, "glDeleteTextures")?,
            finish: load_gl_proc(&egl, &gl_library, "glFinish")?,
            get_error: load_gl_proc(&egl, &gl_library, "glGetError")?,
        };

        Ok(Self {
            inner: Rc::new(ContextInner {
                egl,
                display,
                context,
                create_image,
                destroy_image,
                gl,
                supports_modifiers,
                _gl_library: gl_library,
            }),
        })
    }
}

/// A linear Vulkan image rendered through an `EGLImage` and imported into
/// Torn's `wgpu` Vulkan device.
///
/// The producer calls [`Self::begin_producer_frame`] before rendering into
/// [`Self::gl_texture`], then calls [`Self::signal_producer_ready`] after
/// `glFinish`. The Torn renderer calls [`Self::wait_for_producer`] before
/// sampling [`Self::texture`], and [`Self::finish_consumer_frame`] after its
/// final submission. This fallback synchronizes with `glFinish` on the
/// producer and `wgpu::Device::poll` on the consumer rather than with
/// cross-API GPU semaphores.
pub struct SharedTexture {
    context: Rc<ContextInner>,
    gl_texture: u32,
    egl_image: EGLImage,
    texture: Texture,
    next_value: u64,
    /// The EGL context and the raw `EGLImage` pointer bind this value to the
    /// thread that created it.
    _not_send: PhantomData<*const ()>,
}

impl SharedTexture {
    /// Allocates a linear exportable Vulkan image and imports it into Servo's
    /// EGL context and Torn's Vulkan device.
    ///
    /// `context` must capture the EGL context in which Servo renders.
    /// `queue` is used to submit the initial layout transition and every later
    /// hand-off back to the producer.
    ///
    /// # Errors
    ///
    /// Returns an error for a zero extent, a non-Vulkan `wgpu` device, missing
    /// extensions, a failing Vulkan or EGL operation, or when the EGL context
    /// is not current.
    #[allow(clippy::too_many_lines)]
    pub fn new(
        context: &Context,
        wgpu_device: &wgpu::Device,
        queue: &wgpu::Queue,
        width: u32,
        height: u32,
    ) -> Result<Self, InteropError> {
        if width == 0 || height == 0 {
            return Err(InteropError::InvalidSize);
        }
        context.inner.ensure_current()?;

        let hal_device = unsafe {
            // SAFETY: only a short-lived borrowed view of wgpu's Vulkan device
            // is taken while this constructor creates and imports the image.
            wgpu_device
                .as_hal::<wgpu::hal::vulkan::Api>()
                .ok_or(InteropError::WgpuNotVulkan)?
        };
        require_vulkan_extensions(&hal_device)?;
        let vk = hal_device.raw_device();

        let raw_image = create_vulkan_image(vk, width, height)?;
        let image = raw_image.image;
        let layout = unsafe {
            // SAFETY: the image was created with linear tiling and one color
            // subresource, and remains alive until `raw_image` transfers
            // ownership below.
            vk.get_image_subresource_layout(
                image,
                vk::ImageSubresource::default()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .mip_level(0)
                    .array_layer(0),
            )
        };
        let fd_info = vk::MemoryGetFdInfoKHR::default()
            .memory(raw_image.memory())
            .handle_type(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT);
        let external_memory_fd = ash::khr::external_memory_fd::Device::new(
            hal_device.shared_instance().raw_instance(),
            vk,
        );
        // SAFETY: the memory was allocated with the DMA_BUF export handle type,
        // the external-memory-fd extension is enabled, and the info describes
        // that allocation.
        let fd = unsafe { external_memory_fd.get_memory_fd(&fd_info) }?;

        let egl_image = create_egl_image(
            &context.inner,
            fd,
            width,
            height,
            layout.offset,
            layout.row_pitch,
        )?;
        let gl_texture = match create_gl_texture(&context.inner, egl_image) {
            Ok(texture) => texture,
            Err(error) => {
                // SAFETY: `egl_image` was created by this EGL display and the
                // captured context is current. No GL object references it yet.
                unsafe {
                    (context.inner.destroy_image)(context.inner.display.as_ptr(), egl_image);
                }
                return Err(error);
            }
        };

        let hal_descriptor = wgpu::hal::TextureDescriptor {
            label: Some("Torn Servo dmabuf shared texture"),
            size: Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::Rgba8Unorm,
            usage: TextureUses::RESOURCE | TextureUses::STORAGE_READ_ONLY,
            memory_flags: wgpu::hal::MemoryFlags::empty(),
            view_formats: Vec::new(),
        };
        let (image, memory) = raw_image.into_parts();
        let fp = vk.fp_v1_0().clone();
        let vk_device = vk.handle();
        let drop_callback: wgpu::hal::DropCallback = Box::new(move || unsafe {
            // SAFETY: wgpu-hal invokes this callback exactly once when the
            // imported texture is destroyed. It still owns `image` and
            // `memory`, which were created by this device.
            (fp.destroy_image)(vk_device, image, core::ptr::null());
            (fp.free_memory)(vk_device, memory, core::ptr::null());
        });
        let hal_texture = unsafe {
            // SAFETY: the image is linear, was bound to `memory`, and matches
            // the hal descriptor. wgpu-hal is told the memory is external and
            // receives the only owner of both handles through `drop_callback`.
            hal_device.texture_from_raw(
                image,
                &hal_descriptor,
                Some(drop_callback),
                wgpu::hal::vulkan::TextureMemory::External,
            )
        };
        let descriptor = TextureDescriptor {
            label: Some("Torn Servo dmabuf shared texture"),
            size: Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::Rgba8Unorm,
            usage: TextureUsages::TEXTURE_BINDING | TextureUsages::STORAGE_BINDING,
            view_formats: &[],
        };
        let texture = unsafe {
            // SAFETY: the hal texture was created by this exact device and its
            // extent, format, mip count, and sample count match `descriptor`.
            wgpu_device.create_texture_from_hal::<wgpu::hal::vulkan::Api>(hal_texture, &descriptor)
        };

        let shared = Self {
            context: Rc::clone(&context.inner),
            gl_texture,
            egl_image,
            texture,
            next_value: 1,
            _not_send: PhantomData,
        };
        // Wgpu starts imported textures in UNINITIALIZED. Establish GENERAL as
        // the producer-visible layout before Servo draws anything.
        shared.transition_to_producer_layout(wgpu_device, queue)?;
        Ok(shared)
    }

    /// Returns the OpenGL texture name that Servo must render into.
    #[must_use]
    pub fn gl_texture(&self) -> u32 {
        self.gl_texture
    }

    /// Returns the `EGLImage` that backs [`Self::gl_texture`].
    #[must_use]
    pub fn egl_image(&self) -> EGLImage {
        self.egl_image
    }

    /// Returns the imported texture that Torn composites on its Vulkan device.
    #[must_use]
    pub fn texture(&self) -> &Texture {
        &self.texture
    }

    /// Confirms that the producer may start rendering the next frame.
    ///
    /// The consumer's previous hand-off already waited for Torn's GPU work in
    /// [`Self::finish_consumer_frame`], so this method only validates that the
    /// captured EGL context is current.
    ///
    /// # Errors
    ///
    /// Returns an error when the captured EGL context is not current.
    pub fn begin_producer_frame(&self) -> Result<(), InteropError> {
        self.context.ensure_current()
    }

    /// Waits for OpenGL to finish writing and returns the frame value.
    ///
    /// `glFinish` is the cross-API fallback that makes Servo's writes visible
    /// to Torn's Vulkan submissions. The returned value is retained for
    /// interface parity with the other Servo backends.
    ///
    /// # Errors
    ///
    /// Returns an error when the captured EGL context is not current or frame
    /// values are exhausted.
    pub fn signal_producer_ready(&mut self) -> Result<u64, InteropError> {
        self.context.ensure_current()?;
        // SAFETY: the captured EGL context is current, as checked above.
        unsafe {
            (self.context.gl.finish)();
        }
        let value = self.next_value;
        self.next_value = self
            .next_value
            .checked_add(1)
            .ok_or(InteropError::ValueExhausted)?;
        Ok(value)
    }

    /// Confirms that Servo's frame is ready to sample.
    ///
    /// Producer readiness is established by `glFinish` in
    /// [`Self::signal_producer_ready`], so this method performs no additional
    /// wait. `queue` and `producer_value` are accepted for interface parity.
    ///
    /// # Errors
    ///
    /// This implementation does not currently fail.
    pub fn wait_for_producer(
        &self,
        _queue: &wgpu::Queue,
        _producer_value: u64,
    ) -> Result<(), InteropError> {
        Ok(())
    }

    /// Waits for Torn's submissions and returns the image to the producer.
    ///
    /// Call this only after all Torn submissions that sample [`Self::texture`]
    /// for the current frame. The image is transitioned back to the
    /// producer-visible storage layout and the device is polled before this
    /// method returns. That poll establishes the same completion boundary used
    /// by [`Self::wait_until_unused`] for retirement.
    ///
    /// # Errors
    ///
    /// Returns an error when the layout transition or device polling fails.
    pub fn finish_consumer_frame(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Result<(), InteropError> {
        self.transition_to_producer_layout(device, queue)
    }

    /// Blocks until Torn's GPU has finished every submission that used this
    /// texture.
    ///
    /// This is the explicit retirement contract for [`Self::texture`]. Call it
    /// before dropping a shared texture whose imported Vulkan image may still be
    /// referenced by Torn's queue, as happens when a browser surface is resized.
    /// The method polls Torn's device until all queued GPU work completes, which
    /// makes destroying the `EGLImage`, the Vulkan image, and its memory safe.
    ///
    /// The wait may stall until the GPU drains all queued work. Callers should
    /// treat it as a rare, latency-insensitive boundary.
    ///
    /// # Errors
    ///
    /// Returns an error when polling Torn's `wgpu` device fails.
    pub fn wait_until_unused(&mut self, device: &wgpu::Device) -> Result<(), InteropError> {
        device.poll(wgpu::PollType::wait_indefinitely())?;
        Ok(())
    }

    fn transition_to_producer_layout(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Result<(), InteropError> {
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Torn Servo dmabuf layout transition"),
        });
        encoder.transition_resources(
            core::iter::empty(),
            core::iter::once(TextureTransition {
                texture: &self.texture,
                selector: None,
                state: TextureUses::STORAGE_READ_ONLY,
            }),
        );
        queue.submit([encoder.finish()]);
        device.poll(wgpu::PollType::wait_indefinitely())?;
        Ok(())
    }
}

impl Drop for SharedTexture {
    fn drop(&mut self) {
        // The EGL context may belong to another thread by now. Cleanup is
        // best-effort because the context cannot be made current without its
        // surface, and leaking GL objects is safer than touching foreign state.
        if self.context.ensure_current().is_ok() {
            // SAFETY: the captured EGL context is current. Deleting the GL
            // texture detaches it from the EGLImage before the image is
            // destroyed. The wgpu texture, image, and memory are released by
            // the drop callback after this method returns.
            unsafe {
                (self.context.gl.delete_textures)(1, &raw const self.gl_texture);
                (self.context.destroy_image)(self.context.display.as_ptr(), self.egl_image);
            }
        }
    }
}

fn load_gl_library(explicit: Option<&str>) -> Result<Library, InteropError> {
    if let Some(path) = explicit {
        // SAFETY: the caller asserts that the named library implements the GL
        // ABI expected by the function pointers loaded below.
        return unsafe { Library::new(path) }.map_err(|_| InteropError::GlLibrary);
    }
    for name in ["libGLESv2.so.2", "libGL.so.1"] {
        // SAFETY: these are the conventional system GLES and GL libraries.
        if let Ok(library) = unsafe { Library::new(name) } {
            return Ok(library);
        }
    }
    Err(InteropError::GlLibrary)
}

fn has_extension(extensions: &str, name: &str) -> bool {
    extensions.split_ascii_whitespace().any(|item| item == name)
}

fn require_vulkan_extensions(device: &wgpu::hal::vulkan::Device) -> Result<(), InteropError> {
    for required in [
        ash::khr::external_memory_fd::NAME,
        ash::ext::external_memory_dma_buf::NAME,
    ] {
        if !device
            .enabled_device_extensions()
            .iter()
            .any(|enabled| enabled == &required)
        {
            return Err(InteropError::MissingVulkanExtension(
                required.to_str().unwrap_or("unknown Vulkan extension"),
            ));
        }
    }
    Ok(())
}

fn create_vulkan_image(
    vk: &ash::Device,
    width: u32,
    height: u32,
) -> Result<RawImage, InteropError> {
    let mut external_memory = vk::ExternalMemoryImageCreateInfo::default()
        .handle_types(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT);
    let image_info = vk::ImageCreateInfo::default()
        .image_type(vk::ImageType::TYPE_2D)
        .format(vk::Format::R8G8B8A8_UNORM)
        .extent(vk::Extent3D {
            width,
            height,
            depth: 1,
        })
        .mip_levels(1)
        .array_layers(1)
        .samples(vk::SampleCountFlags::TYPE_1)
        .tiling(vk::ImageTiling::LINEAR)
        .usage(vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::STORAGE)
        .sharing_mode(vk::SharingMode::EXCLUSIVE)
        .initial_layout(vk::ImageLayout::UNDEFINED)
        .push_next(&mut external_memory);
    // SAFETY: `image_info` describes one non-zero linear 2D color image and
    // borrows the external-memory create info for the duration of the call.
    let image = unsafe { vk.create_image(&image_info, None) }?;
    let mut raw = RawImage {
        fp: vk.fp_v1_0().clone(),
        device: vk.handle(),
        image,
        memory: None,
    };

    // SAFETY: `image` is live and created by this device.
    let requirements = unsafe { vk.get_image_memory_requirements(image) };
    for index in 0..32 {
        if requirements.memory_type_bits & (1 << index) == 0 {
            continue;
        }
        let mut dedicated = vk::MemoryDedicatedAllocateInfo::default().image(image);
        let mut export = vk::ExportMemoryAllocateInfo::default()
            .handle_types(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT);
        let allocation = vk::MemoryAllocateInfo::default()
            .allocation_size(requirements.size)
            .memory_type_index(index)
            .push_next(&mut dedicated)
            .push_next(&mut export);
        // SAFETY: `allocation` uses a memory type accepted by the image and
        // requests a dedicated DMA_BUF export allocation.
        if let Ok(memory) = unsafe { vk.allocate_memory(&allocation, None) } {
            raw.memory = Some(memory);
            break;
        }
    }
    let memory = raw.memory.ok_or(InteropError::ImageMemory)?;
    // SAFETY: `memory` was allocated for this image and is bound at offset
    // zero, which is the only allocation the dedicated memory contains.
    unsafe {
        vk.bind_image_memory(image, memory, 0)?;
    }
    Ok(raw)
}

fn create_egl_image(
    context: &ContextInner,
    fd: i32,
    width: u32,
    height: u32,
    offset: u64,
    pitch: u64,
) -> Result<EGLImage, InteropError> {
    let result = (|| {
        let offset = i32::try_from(offset).map_err(|_| InteropError::LayoutOverflow)?;
        let pitch = i32::try_from(pitch).map_err(|_| InteropError::LayoutOverflow)?;
        let width = i32::try_from(width).map_err(|_| InteropError::LayoutOverflow)?;
        let height = i32::try_from(height).map_err(|_| InteropError::LayoutOverflow)?;

        let mut attributes = Vec::with_capacity(if context.supports_modifiers { 18 } else { 14 });
        attributes.extend_from_slice(&[
            EGL_WIDTH,
            width,
            EGL_HEIGHT,
            height,
            EGL_LINUX_DRM_FOURCC_EXT,
            DRM_FORMAT_ABGR8888,
            EGL_DMA_BUF_PLANE0_FD_EXT,
            fd,
            EGL_DMA_BUF_PLANE0_OFFSET_EXT,
            offset,
            EGL_DMA_BUF_PLANE0_PITCH_EXT,
            pitch,
        ]);
        if context.supports_modifiers {
            // A linear image has DRM_FORMAT_MOD_LINEAR (zero).
            attributes.extend_from_slice(&[
                EGL_DMA_BUF_PLANE0_MODIFIER_LO_EXT,
                0,
                EGL_DMA_BUF_PLANE0_MODIFIER_HI_EXT,
                0,
            ]);
        }
        attributes.push(EGL_NONE);

        context.ensure_current()?;
        // SAFETY: the display and context were captured from the current EGL
        // state, `attributes` is a null-terminated list of valid dmabuf
        // properties, and the caller transfers ownership of `fd` here.
        let image = unsafe {
            (context.create_image)(
                context.display.as_ptr(),
                NO_CONTEXT,
                EGL_LINUX_DMA_BUF_EXT,
                core::ptr::null_mut(),
                attributes.as_ptr(),
            )
        };
        if image.is_null() {
            Err(InteropError::EglImage)
        } else {
            Ok(image)
        }
    })();
    // SAFETY: this function is the only owner of the fd returned by
    // `vkGetMemoryFdKHR`; EGLImage creation does not consume it.
    unsafe {
        libc::close(fd);
    }
    result
}

fn create_gl_texture(context: &ContextInner, egl_image: EGLImage) -> Result<u32, InteropError> {
    context.ensure_current()?;
    let mut texture = 0_u32;
    // SAFETY: the captured EGL context is current and `egl_image` was created
    // by this display. The texture name remains owned by this function until it
    // is returned.
    unsafe {
        (context.gl.gen_textures)(1, &raw mut texture);
        if texture == 0 {
            return Err(InteropError::GlError(0));
        }
        (context.gl.bind_texture)(GL_TEXTURE_2D, texture);
        (context.gl.tex_parameteri)(GL_TEXTURE_2D, GL_TEXTURE_MIN_FILTER, GL_LINEAR);
        (context.gl.tex_parameteri)(GL_TEXTURE_2D, GL_TEXTURE_MAG_FILTER, GL_LINEAR);
        (context.gl.tex_parameteri)(GL_TEXTURE_2D, GL_TEXTURE_WRAP_S, GL_CLAMP_TO_EDGE);
        (context.gl.tex_parameteri)(GL_TEXTURE_2D, GL_TEXTURE_WRAP_T, GL_CLAMP_TO_EDGE);
        (context.gl.egl_image_target_texture)(GL_TEXTURE_2D, egl_image);
        let error = (context.gl.get_error)();
        if error != 0 {
            (context.gl.delete_textures)(1, &raw const texture);
            return Err(InteropError::GlError(error));
        }
    }
    Ok(texture)
}

struct RawImage {
    fp: ash::DeviceFnV1_0,
    device: vk::Device,
    image: vk::Image,
    memory: Option<vk::DeviceMemory>,
}

impl RawImage {
    fn memory(&self) -> vk::DeviceMemory {
        self.memory.expect("raw image memory was allocated")
    }

    fn into_parts(mut self) -> (vk::Image, vk::DeviceMemory) {
        let memory = self.memory.take().expect("raw image memory was allocated");
        let image = self.image;
        // The wgpu drop callback now owns both handles.
        core::mem::forget(self);
        (image, memory)
    }
}

impl Drop for RawImage {
    fn drop(&mut self) {
        if let Some(memory) = self.memory.take() {
            // SAFETY: the memory was allocated by this device and is not bound
            // to any other resource after construction fails.
            unsafe {
                (self.fp.free_memory)(self.device, memory, core::ptr::null());
            }
        }
        // SAFETY: the image was created by this device and is destroyed exactly
        // once because `into_parts` forgets the owner.
        unsafe {
            (self.fp.destroy_image)(self.device, self.image, core::ptr::null());
        }
    }
}

#[allow(clippy::missing_transmute_annotations)]
fn load_egl_proc<T: Copy>(
    egl: &DynamicInstance<EGL1_5>,
    name: &'static str,
) -> Result<T, InteropError> {
    let function = egl
        .get_proc_address(name)
        .ok_or(InteropError::MissingFunction(name))?;
    transmute_proc(function, name)
}

#[allow(clippy::missing_transmute_annotations)]
fn load_gl_proc<T: Copy>(
    egl: &DynamicInstance<EGL1_5>,
    library: &Library,
    name: &'static str,
) -> Result<T, InteropError> {
    if let Some(function) = egl.get_proc_address(name) {
        return transmute_proc(function, name);
    }
    // SAFETY: the caller selected a library that implements the GL ABI and the
    // symbol is interpreted as a function pointer of the requested signature.
    let function = unsafe { library.get::<extern "system" fn()>(name.as_bytes()) }
        .map_err(|_| InteropError::MissingFunction(name))?;
    transmute_proc(*function, name)
}

#[allow(clippy::missing_transmute_annotations)]
fn transmute_proc<T: Copy>(
    function: extern "system" fn(),
    name: &'static str,
) -> Result<T, InteropError> {
    if core::mem::size_of::<T>() != core::mem::size_of_val(&function) {
        return Err(InteropError::MissingFunction(name));
    }
    // SAFETY: all supported platforms represent function pointers with one
    // pointer-sized value. The caller supplies the signature that matches the
    // resolved entry point.
    Ok(unsafe { core::mem::transmute_copy(&function) })
}

#[cfg(test)]
mod tests {
    use super::InteropError;

    #[test]
    fn reports_each_local_interop_failure() {
        assert_eq!(
            InteropError::InvalidSize.to_string(),
            "shared texture size must be non-zero"
        );
        assert_eq!(
            InteropError::WgpuNotVulkan.to_string(),
            "wgpu is not using the Vulkan backend"
        );
        assert_eq!(
            InteropError::ValueExhausted.to_string(),
            "shared frame value overflowed"
        );
    }
}
