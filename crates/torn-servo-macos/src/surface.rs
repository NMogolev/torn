//! `IOSurface` ownership, Metal texture sharing, and wgpu Metal import.

use core::fmt;

use objc2::rc::Retained;
use objc2_core_foundation::{CFDictionary, CFNumber, CFRetained, CFType};
use objc2_io_surface::{
    IOSurfaceID, IOSurfaceRef, kIOSurfaceBytesPerElement, kIOSurfaceHeight, kIOSurfacePixelFormat,
    kIOSurfaceWidth,
};
use objc2_metal::{
    MTLDevice, MTLPixelFormat, MTLSharedEvent, MTLStorageMode, MTLTexture, MTLTextureDescriptor,
    MTLTextureType, MTLTextureUsage,
};
use wgpu::{
    Extent3d, Texture, TextureDescriptor, TextureDimension, TextureFormat, TextureUsages,
    hal::metal::Api,
};

type MetalTexture = Retained<objc2::runtime::ProtocolObject<dyn MTLTexture>>;
type SharedEvent = Retained<objc2::runtime::ProtocolObject<dyn MTLSharedEvent>>;
type MetalTextureTraitObject = objc2::runtime::ProtocolObject<dyn MTLTexture>;
type SharedEventTraitObject = objc2::runtime::ProtocolObject<dyn MTLSharedEvent>;

/// `kCMPixelFormat_32BGRA`, the `IOSurface` spelling of the four byte BGRA layout
/// used by [`MTLPixelFormat::BGRA8Unorm`] and [`TextureFormat::Bgra8Unorm`].
const BGRA8_PIXEL_FORMAT: i32 = 0x4247_5241;

/// Failure while preparing or synchronizing a shared `IOSurface` texture.
#[derive(Debug)]
pub enum InteropError {
    /// The requested texture dimensions cannot describe a GPU image.
    InvalidSize,
    /// Torn's `wgpu` device is not backed by Metal.
    WgpuNotMetal,
    /// A native allocation returned no object.
    AllocationFailed(&'static str),
    /// Waiting for the other API's frame timed out.
    SynchronizationTimeout {
        /// Event that did not reach the requested value.
        event: &'static str,
        /// Value the event was expected to reach.
        value: u64,
    },
    /// The shared event counter cannot advance without overflowing.
    ValueExhausted,
    /// Polling Torn's `wgpu` device failed.
    DevicePoll(wgpu::PollError),
}

impl fmt::Display for InteropError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSize => formatter.write_str("shared texture size must be non-zero"),
            Self::WgpuNotMetal => formatter.write_str("wgpu is not using the Metal backend"),
            Self::AllocationFailed(operation) => {
                write!(formatter, "failed to allocate {operation}")
            }
            Self::SynchronizationTimeout { event, value } => {
                write!(
                    formatter,
                    "timed out waiting for the {event} event to reach value {value}"
                )
            }
            Self::ValueExhausted => formatter.write_str("shared Metal event value overflowed"),
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
            Self::InvalidSize
            | Self::WgpuNotMetal
            | Self::AllocationFailed(_)
            | Self::SynchronizationTimeout { .. }
            | Self::ValueExhausted => None,
        }
    }
}

impl From<wgpu::PollError> for InteropError {
    fn from(error: wgpu::PollError) -> Self {
        Self::DevicePoll(error)
    }
}

/// The Metal context that owns the producer's IOSurface-backed texture.
///
/// `IOSurfaces` are created through the system-wide `IOSurface` framework rather
/// than through a particular `MTLDevice`, so this context carries no state.
/// Every [`SharedTexture`] creates its own textures on Torn's Metal device.
#[derive(Debug, Clone, Copy, Default)]
pub struct Context;

impl Context {
    /// Creates an empty macOS interop context.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

/// An `IOSurface` render target shared between Servo's Metal producer and Torn's
/// `wgpu` Metal device.
///
/// The producer calls [`Self::begin_producer_frame`] before writing to
/// [`Self::metal_texture`], renders the frame, waits for its Metal command
/// buffer to complete, and then calls [`Self::signal_producer_ready`]. The Torn
/// renderer calls [`Self::wait_for_producer`] before sampling
/// [`Self::texture`], and [`Self::finish_consumer_frame`] after its final
/// submission.
pub struct SharedTexture {
    io_surface: CFRetained<IOSurfaceRef>,
    producer_texture: MetalTexture,
    producer_event: SharedEvent,
    consumer_event: SharedEvent,
    texture: Texture,
    next_producer_value: u64,
    next_consumer_value: u64,
    last_consumer_value: u64,
}

impl SharedTexture {
    /// Allocates a BGRA `IOSurface` on Torn's Metal device and imports it into
    /// `wgpu`.
    ///
    /// `context` carries no platform state on macOS. `queue` is accepted for
    /// interface parity with the other Servo backends and is not used because
    /// the consumer completion is observed through `device.poll`.
    ///
    /// # Errors
    ///
    /// Returns an error for a zero or oversized extent, a non-Metal `wgpu`
    /// device, or a failing native allocation.
    pub fn new(
        context: &Context,
        wgpu_device: &wgpu::Device,
        _queue: &wgpu::Queue,
        width: u32,
        height: u32,
    ) -> Result<Self, InteropError> {
        let _ = context;
        if width == 0 || height == 0 || width > i32::MAX as u32 || height > i32::MAX as u32 {
            return Err(InteropError::InvalidSize);
        }

        let io_surface = create_io_surface(width, height)?;

        let hal_device = unsafe {
            // SAFETY: only a short-lived borrowed guard of wgpu's Metal device
            // is taken. The retained protocol object below is reference-counted
            // independently of the `as_hal` guard.
            wgpu_device
                .as_hal::<Api>()
                .ok_or(InteropError::WgpuNotMetal)?
        };
        let raw_device = hal_device.raw_device().clone();

        let producer_texture = create_producer_texture(&raw_device, &io_surface, width, height)?;
        let consumer_texture = create_consumer_texture(&raw_device, &io_surface, width, height)?;
        let texture = import_texture(wgpu_device, consumer_texture, width, height);

        let producer_event = raw_device
            .newSharedEvent()
            .ok_or(InteropError::AllocationFailed("the producer shared event"))?;
        let consumer_event = raw_device
            .newSharedEvent()
            .ok_or(InteropError::AllocationFailed("the consumer shared event"))?;

        Ok(Self {
            io_surface,
            producer_texture,
            producer_event,
            consumer_event,
            texture,
            next_producer_value: 1,
            next_consumer_value: 1,
            last_consumer_value: 0,
        })
    }

    /// Returns the `IOSurface` that backs both textures.
    #[must_use]
    pub fn io_surface(&self) -> &IOSurfaceRef {
        &self.io_surface
    }

    /// Returns the system-wide identifier of the shared `IOSurface`.
    #[must_use]
    pub fn io_surface_id(&self) -> IOSurfaceID {
        self.io_surface.id()
    }

    /// Returns the Metal texture that Servo must render into.
    #[must_use]
    pub fn metal_texture(&self) -> &MetalTextureTraitObject {
        &self.producer_texture
    }

    /// Returns the imported texture that Torn composites on its Metal device.
    #[must_use]
    pub fn texture(&self) -> &Texture {
        &self.texture
    }

    /// Returns the shared event that the producer signals after a frame.
    ///
    /// Encode a signal on this event at the end of Servo's command buffer when
    /// GPU-side ordering is available. That is stronger than the CPU-side
    /// signal performed by [`Self::signal_producer_ready`].
    #[must_use]
    pub fn producer_event(&self) -> &SharedEventTraitObject {
        &self.producer_event
    }

    /// Returns the shared event that Torn signals after a consumer frame.
    #[must_use]
    pub fn consumer_event(&self) -> &SharedEventTraitObject {
        &self.consumer_event
    }

    /// Waits until Torn has finished the previous consumer frame.
    ///
    /// Call this before Servo starts drawing the next frame into
    /// [`Self::metal_texture`]. The first call waits for value zero and is
    /// therefore ready immediately.
    ///
    /// # Errors
    ///
    /// Returns an error if the consumer event does not reach the expected value.
    pub fn begin_producer_frame(&self) -> Result<(), InteropError> {
        if self.last_consumer_value == 0 {
            return Ok(());
        }
        if !wait(&self.consumer_event, self.last_consumer_value) {
            return Err(InteropError::SynchronizationTimeout {
                event: "consumer",
                value: self.last_consumer_value,
            });
        }
        Ok(())
    }

    /// Signals that Servo submitted its writes to the shared texture.
    ///
    /// The caller must ensure the producer's Metal command buffer has completed
    /// before calling this method. Prefer encoding a signal on
    /// [`Self::producer_event`] directly in that command buffer when possible.
    ///
    /// # Errors
    ///
    /// Returns an error when shared event values are exhausted.
    pub fn signal_producer_ready(&mut self) -> Result<u64, InteropError> {
        let value = take_next(&mut self.next_producer_value)?;
        self.producer_event.setSignaledValue(value);
        Ok(value)
    }

    /// Waits until Servo has signalled the frame that Torn is about to sample.
    ///
    /// `producer_value` must be the value returned by
    /// [`Self::signal_producer_ready`] for the frame about to be composited.
    /// `queue` is unused on macOS.
    ///
    /// # Errors
    ///
    /// Returns an error if the producer event does not reach `producer_value`.
    pub fn wait_for_producer(
        &self,
        _queue: &wgpu::Queue,
        producer_value: u64,
    ) -> Result<(), InteropError> {
        if !wait(&self.producer_event, producer_value) {
            return Err(InteropError::SynchronizationTimeout {
                event: "producer",
                value: producer_value,
            });
        }
        Ok(())
    }

    /// Waits for Torn's submissions and signals completion to the producer.
    ///
    /// Call this only after all Torn submissions that sample [`Self::texture`]
    /// for the current frame. The device is fully polled before the consumer
    /// event advances, so a later [`Self::begin_producer_frame`] observes
    /// completed rendering. The full poll is also the retirement boundary used
    /// by [`Self::wait_until_unused`].
    ///
    /// # Errors
    ///
    /// Returns an error when device polling fails or shared event values are
    /// exhausted.
    pub fn finish_consumer_frame(
        &mut self,
        device: &wgpu::Device,
        _queue: &wgpu::Queue,
    ) -> Result<(), InteropError> {
        device.poll(wgpu::PollType::wait_indefinitely())?;
        let value = take_next(&mut self.next_consumer_value)?;
        self.consumer_event.setSignaledValue(value);
        self.last_consumer_value = value;
        Ok(())
    }

    /// Blocks until Torn's GPU has finished every submission that used this
    /// texture.
    ///
    /// This is the explicit retirement contract for [`Self::texture`]. Call it
    /// before dropping a shared texture whose `IOSurface` and Metal textures may
    /// still be referenced by Torn's queue, as happens when a browser surface is
    /// resized. The method polls Torn's device until all queued GPU work
    /// completes, which makes destroying the shared `IOSurface` safe.
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
}

fn create_io_surface(width: u32, height: u32) -> Result<CFRetained<IOSurfaceRef>, InteropError> {
    let width_number =
        CFNumber::new_i32(i32::try_from(width).map_err(|_| InteropError::InvalidSize)?);
    let height_number =
        CFNumber::new_i32(i32::try_from(height).map_err(|_| InteropError::InvalidSize)?);
    let bytes_per_element = CFNumber::new_i32(4);
    let pixel_format = CFNumber::new_i32(BGRA8_PIXEL_FORMAT);

    // SAFETY: the IOSurface framework exports these constant keys.
    let (width_key, height_key, bytes_per_element_key, pixel_format_key) = unsafe {
        (
            kIOSurfaceWidth,
            kIOSurfaceHeight,
            kIOSurfaceBytesPerElement,
            kIOSurfacePixelFormat,
        )
    };
    let values: [&CFType; 4] = [
        width_number.as_ref(),
        height_number.as_ref(),
        bytes_per_element.as_ref(),
        pixel_format.as_ref(),
    ];
    let properties = CFDictionary::<CFType, CFType>::from_slices(
        &[
            width_key,
            height_key,
            bytes_per_element_key,
            pixel_format_key,
        ],
        &values,
    );
    // SAFETY: every CFDictionary can be viewed as the opaque dictionary type
    // expected by IOSurfaceCreate, which reads it through the CFType callbacks
    // used by `from_slices`.
    let properties = unsafe { CFRetained::cast_unchecked::<CFDictionary>(properties) };

    // SAFETY: the dictionary contains the four documented IOSurface properties.
    // IOSurfaceCreate copies the values it needs and returns an owned surface.
    unsafe { IOSurfaceRef::new(&properties) }
        .ok_or(InteropError::AllocationFailed("the shared IOSurface"))
}

fn create_producer_texture(
    device: &Retained<objc2::runtime::ProtocolObject<dyn MTLDevice>>,
    io_surface: &IOSurfaceRef,
    width: u32,
    height: u32,
) -> Result<MetalTexture, InteropError> {
    let descriptor = unsafe {
        // SAFETY: both dimensions are non-zero and fit in `NSUInteger`.
        MTLTextureDescriptor::texture2DDescriptorWithPixelFormat_width_height_mipmapped(
            MTLPixelFormat::BGRA8Unorm,
            width as usize,
            height as usize,
            false,
        )
    };
    descriptor.setUsage(MTLTextureUsage::RenderTarget | MTLTextureUsage::ShaderRead);
    descriptor.setStorageMode(MTLStorageMode::Shared);
    device
        .newTextureWithDescriptor_iosurface_plane(&descriptor, io_surface, 0)
        .ok_or(InteropError::AllocationFailed("the producer Metal texture"))
}

fn create_consumer_texture(
    device: &Retained<objc2::runtime::ProtocolObject<dyn MTLDevice>>,
    io_surface: &IOSurfaceRef,
    width: u32,
    height: u32,
) -> Result<MetalTexture, InteropError> {
    let descriptor = unsafe {
        // SAFETY: both dimensions are non-zero and fit in `NSUInteger`.
        MTLTextureDescriptor::texture2DDescriptorWithPixelFormat_width_height_mipmapped(
            MTLPixelFormat::BGRA8Unorm,
            width as usize,
            height as usize,
            false,
        )
    };
    descriptor.setUsage(MTLTextureUsage::ShaderRead);
    descriptor.setStorageMode(MTLStorageMode::Shared);
    device
        .newTextureWithDescriptor_iosurface_plane(&descriptor, io_surface, 0)
        .ok_or(InteropError::AllocationFailed("the consumer Metal texture"))
}

fn import_texture(
    device: &wgpu::Device,
    raw_texture: MetalTexture,
    width: u32,
    height: u32,
) -> Texture {
    let extent = Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
    };
    let descriptor = TextureDescriptor {
        label: Some("Torn Servo IOSurface shared texture"),
        size: extent,
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Bgra8Unorm,
        usage: TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    };
    unsafe {
        // SAFETY: the texture was created by this exact Metal device from the
        // IOSurface below; its extent, format, mip count, and sample count match
        // `descriptor`. Wgpu tracks the imported image from UNINITIALIZED.
        device.create_texture_from_hal::<Api>(
            wgpu::hal::metal::Device::texture_from_raw(
                raw_texture,
                TextureFormat::Bgra8Unorm,
                MTLTextureType::Type2D,
                1,
                1,
                wgpu::hal::CopyExtent {
                    width,
                    height,
                    depth: 1,
                },
            ),
            &descriptor,
        )
    }
}

fn wait(event: &SharedEventTraitObject, value: u64) -> bool {
    event.waitUntilSignaledValue_timeoutMS(value, u64::MAX)
}

fn take_next(value: &mut u64) -> Result<u64, InteropError> {
    let current = *value;
    *value = value.checked_add(1).ok_or(InteropError::ValueExhausted)?;
    Ok(current)
}

#[cfg(test)]
mod tests {
    use super::{Context, InteropError, SharedTexture};

    #[test]
    fn reports_each_local_interop_failure() {
        assert_eq!(
            InteropError::InvalidSize.to_string(),
            "shared texture size must be non-zero"
        );
        assert_eq!(
            InteropError::WgpuNotMetal.to_string(),
            "wgpu is not using the Metal backend"
        );
        assert_eq!(
            InteropError::ValueExhausted.to_string(),
            "shared Metal event value overflowed"
        );
    }

    /// Requires a Metal-capable Mac and is intentionally excluded from regular
    /// CI. Run with `cargo test -p torn-servo-macos -- --ignored`.
    #[test]
    #[ignore = "requires a Metal-capable Mac"]
    fn creates_and_synchronizes_a_shared_texture_without_cpu_readback()
    -> Result<(), Box<dyn std::error::Error>> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::METAL,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            force_fallback_adapter: false,
            ..Default::default()
        }))?;
        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
                label: Some("Torn Servo interop smoke-test device"),
                ..Default::default()
            }))?;

        let context = Context::new();
        let mut texture = SharedTexture::new(&context, &device, &queue, 64, 64)?;
        assert_ne!(texture.io_surface_id(), 0);
        assert_eq!(texture.io_surface().width(), 64);
        assert_eq!(texture.io_surface().height(), 64);

        texture.begin_producer_frame()?;
        let producer_value = texture.signal_producer_ready()?;
        texture.wait_for_producer(&queue, producer_value)?;
        texture.finish_consumer_frame(&device, &queue)?;
        texture.wait_until_unused(&device)?;
        texture.begin_producer_frame()?;
        Ok(())
    }
}
