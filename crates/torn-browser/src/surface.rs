use core::fmt;
use core::sync::atomic::{AtomicU64, Ordering};

use torn_core::Size;
use torn_render::TextureId;
use torn_servo::{Context, InteropError, SharedTexture};
use torn_wgpu::GpuRenderer;

static NEXT_TEXTURE_ID: AtomicU64 = AtomicU64::new(1);

/// A browser frame that is either idle or owned by the producer or consumer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FrameState {
    Idle,
    Produced(u64),
    Consuming(u64),
}

/// A Servo shared texture together with Torn's producer/consumer hand-off.
///
/// The browser engine renders into the platform producer texture exposed by
/// [`Self::shared_texture`], then reports completion with
/// [`Self::signal_producer_ready`]. Torn calls [`Self::begin_consumer_frame`]
/// before rendering and [`Self::finish_consumer_frame`] after its last
/// submission. [`Self::texture`] is registered with the `wgpu` renderer under
/// [`Self::texture_id`], and [`crate::BrowserView`] paints that identifier.
pub struct BrowserSurface {
    context: Context,
    shared: SharedTexture,
    texture_id: TextureId,
    width: u32,
    height: u32,
    frame: FrameState,
}

impl BrowserSurface {
    /// Allocates a shared texture for the Servo producer.
    ///
    /// `context` must describe the producer's native device or captured EGL
    /// context. `device` and `queue` belong to Torn's [`GpuRenderer`] and can be
    /// obtained from its [`GpuRenderer::device`] and [`GpuRenderer::queue`]
    /// accessors.
    ///
    /// # Errors
    ///
    /// Returns [`BrowserError::InvalidSize`] for a zero extent or
    /// [`BrowserError::Interop`] when the platform backend cannot create or
    /// import the shared texture.
    pub fn new(
        context: Context,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        width: u32,
        height: u32,
    ) -> Result<Self, BrowserError> {
        if width == 0 || height == 0 {
            return Err(BrowserError::InvalidSize);
        }
        let shared = SharedTexture::new(&context, device, queue, width, height)?;
        Ok(Self {
            context,
            shared,
            texture_id: next_texture_id(),
            width,
            height,
            frame: FrameState::Idle,
        })
    }

    /// Returns the platform interop context supplied at construction.
    #[must_use]
    pub const fn context(&self) -> &Context {
        &self.context
    }

    /// Returns the shared texture backing the browser surface.
    ///
    /// Use the platform-specific accessor on this value to obtain the handle the
    /// Servo producer must render into.
    #[must_use]
    pub const fn shared_texture(&self) -> &SharedTexture {
        &self.shared
    }

    /// Returns the identifier to register with the GPU renderer.
    #[must_use]
    pub const fn texture_id(&self) -> TextureId {
        self.texture_id
    }

    /// Registers the current texture with `renderer`.
    ///
    /// Call this after construction. [`Self::resize`] registers the recreated
    /// texture automatically, so it does not need a second call.
    /// [`crate::BrowserView`] can then draw [`Self::texture_id`].
    pub fn register(&self, renderer: &mut GpuRenderer) {
        renderer.register_texture(self.texture_id, self.shared.texture());
    }

    /// Removes this surface from `renderer`.
    ///
    /// Returns whether the identifier was registered.
    pub fn unregister(&self, renderer: &mut GpuRenderer) -> bool {
        renderer.unregister_texture(self.texture_id)
    }

    /// Returns the imported texture that Torn composites.
    #[must_use]
    pub fn texture(&self) -> &wgpu::Texture {
        self.shared.texture()
    }

    /// Returns the current width in physical pixels.
    #[must_use]
    pub const fn width(&self) -> u32 {
        self.width
    }

    /// Returns the current height in physical pixels.
    #[must_use]
    pub const fn height(&self) -> u32 {
        self.height
    }

    /// Returns the current size in physical pixels.
    ///
    /// # Panics
    ///
    /// Panics if the stored dimensions are non-finite. Constructors reject that
    /// state, so this cannot happen for a live surface.
    #[must_use]
    pub fn size(&self) -> Size {
        Size::new(u32_to_f32(self.width), u32_to_f32(self.height))
            .expect("browser surface dimensions are nonzero and finite")
    }

    /// Replaces the shared texture with one of a new physical size.
    ///
    /// The texture identifier is stable, and the recreated texture is registered
    /// with `renderer` under that identifier before the old one is released.
    /// Resizing is rejected while a frame is in flight.
    ///
    /// The old native resource may still be referenced by GPU submissions that
    /// Torn has queued. This method therefore waits on the CPU until the GPU has
    /// retired the old texture before creating the replacement. That wait can
    /// stall until the GPU drains its queue, which is accepted because resizing
    /// is rare and the explicit lifetime boundary prevents use-after-free.
    ///
    /// # Errors
    ///
    /// Returns [`BrowserError::InvalidSize`] for a zero extent,
    /// [`BrowserError::FrameInProgress`] while a frame is in flight, or
    /// [`BrowserError::Interop`] when the backend cannot retire the old texture
    /// or create and register the replacement.
    pub fn resize(
        &mut self,
        renderer: &mut GpuRenderer,
        width: u32,
        height: u32,
    ) -> Result<(), BrowserError> {
        if width == 0 || height == 0 {
            return Err(BrowserError::InvalidSize);
        }
        if self.frame != FrameState::Idle {
            return Err(BrowserError::FrameInProgress);
        }

        // `finish_consumer_frame` signalled the fence or event after the last
        // submission that sampled the old texture. Wait for that marker so the
        // native resource can be destroyed without racing outstanding GPU work.
        self.shared.wait_until_unused(renderer.device())?;

        let shared = SharedTexture::new(
            &self.context,
            renderer.device(),
            renderer.queue(),
            width,
            height,
        )?;
        // Publish the replacement before the old texture is dropped so the
        // renderer never holds a stale identifier and the old GPU resource is
        // released only after the new one is registered.
        renderer.register_texture(self.texture_id, shared.texture());
        self.shared = shared;
        self.width = width;
        self.height = height;
        Ok(())
    }

    /// Waits until Torn has finished the previous consumer frame.
    ///
    /// Call this before the Servo producer starts writing into the shared
    /// producer texture.
    ///
    /// # Errors
    ///
    /// Returns [`BrowserError::FrameInProgress`] when the previous frame has not
    /// been consumed yet, or [`BrowserError::Interop`] when the platform wait
    /// fails.
    pub fn begin_producer_frame(&self) -> Result<(), BrowserError> {
        if self.frame != FrameState::Idle {
            return Err(BrowserError::FrameInProgress);
        }
        self.shared.begin_producer_frame()?;
        Ok(())
    }

    /// Signals that the Servo producer submitted its writes.
    ///
    /// The returned value is passed to [`Self::begin_consumer_frame`] on the
    /// consumer side automatically; callers normally only need it for logging
    /// or platform-specific ordering.
    ///
    /// # Errors
    ///
    /// Returns [`BrowserError::FrameInProgress`] when a frame is already in
    /// flight, or [`BrowserError::Interop`] when signalling fails.
    pub fn signal_producer_ready(&mut self) -> Result<u64, BrowserError> {
        if self.frame != FrameState::Idle {
            return Err(BrowserError::FrameInProgress);
        }
        let value = self.shared.signal_producer_ready()?;
        self.frame = FrameState::Produced(value);
        Ok(value)
    }

    /// Waits for the produced frame and marks it as being sampled by Torn.
    ///
    /// Call this after recording the display list and before
    /// [`GpuRenderer::render`]. The returned frame value is reserved for
    /// diagnostics.
    ///
    /// # Errors
    ///
    /// Returns [`BrowserError::NoProducedFrame`] when the producer has not
    /// signalled a frame, [`BrowserError::FrameInProgress`] when Torn is already
    /// sampling one, or [`BrowserError::Interop`] when the platform wait fails.
    pub fn begin_consumer_frame(&mut self, queue: &wgpu::Queue) -> Result<u64, BrowserError> {
        match self.frame {
            FrameState::Produced(value) => {
                self.shared.wait_for_producer(queue, value)?;
                self.frame = FrameState::Consuming(value);
                Ok(value)
            }
            FrameState::Idle => Err(BrowserError::NoProducedFrame),
            FrameState::Consuming(_) => Err(BrowserError::FrameInProgress),
        }
    }

    /// Returns the shared texture to the producer after Torn's final submission.
    ///
    /// Call this after [`GpuRenderer::render`] returns and after any later
    /// submissions that sample [`Self::texture`].
    ///
    /// # Errors
    ///
    /// Returns [`BrowserError::NoConsumerFrame`] when
    /// [`Self::begin_consumer_frame`] was not called, or
    /// [`BrowserError::Interop`] when the platform hand-off fails.
    pub fn finish_consumer_frame(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Result<(), BrowserError> {
        match self.frame {
            FrameState::Consuming(_) => {
                self.shared.finish_consumer_frame(device, queue)?;
                self.frame = FrameState::Idle;
                Ok(())
            }
            FrameState::Idle | FrameState::Produced(_) => Err(BrowserError::NoConsumerFrame),
        }
    }
}

fn next_texture_id() -> TextureId {
    TextureId::new(NEXT_TEXTURE_ID.fetch_add(1, Ordering::Relaxed))
}

fn u32_to_f32(value: u32) -> f32 {
    #[allow(clippy::cast_precision_loss)]
    {
        value as f32
    }
}

/// Why the browser surface could not complete an operation.
#[derive(Debug)]
pub enum BrowserError {
    /// The platform shared-texture backend failed.
    Interop(InteropError),
    /// A surface dimension was zero.
    InvalidSize,
    /// A frame is already waiting for or being sampled by the consumer.
    FrameInProgress,
    /// Torn tried to consume a frame that the producer has not signalled.
    NoProducedFrame,
    /// Torn tried to finish a frame that it did not start consuming.
    NoConsumerFrame,
}

impl fmt::Display for BrowserError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Interop(error) => write!(formatter, "browser shared-texture error: {error}"),
            Self::InvalidSize => formatter.write_str("browser surface dimensions must be nonzero"),
            Self::FrameInProgress => formatter.write_str("a browser frame is already in flight"),
            Self::NoProducedFrame => formatter.write_str("the browser produced no frame to sample"),
            Self::NoConsumerFrame => {
                formatter.write_str("the browser consumer frame was not started")
            }
        }
    }
}

impl std::error::Error for BrowserError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Interop(error) => Some(error),
            Self::InvalidSize
            | Self::FrameInProgress
            | Self::NoProducedFrame
            | Self::NoConsumerFrame => None,
        }
    }
}

impl From<InteropError> for BrowserError {
    fn from(error: InteropError) -> Self {
        Self::Interop(error)
    }
}
