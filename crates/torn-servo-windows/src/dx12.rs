//! Direct3D 11/12 shared-texture ownership and synchronization.

use core::fmt;

use wgpu::{
    Extent3d, Texture, TextureDescriptor, TextureDimension, TextureFormat, TextureTransition,
    TextureUsages, TextureUses, hal::dx12::Api,
};
use windows::{
    Win32::{
        Foundation::{CloseHandle, GENERIC_ALL, LUID, WAIT_OBJECT_0},
        Graphics::{
            Direct3D11::{
                D3D11_BIND_RENDER_TARGET, D3D11_BIND_SHADER_RESOURCE, D3D11_FENCE_FLAG_SHARED,
                D3D11_RESOURCE_MISC_SHARED, D3D11_RESOURCE_MISC_SHARED_NTHANDLE,
                D3D11_TEXTURE2D_DESC, D3D11_USAGE_DEFAULT, ID3D11Device, ID3D11Device5,
                ID3D11DeviceContext4, ID3D11Fence, ID3D11Texture2D,
            },
            Direct3D12::{ID3D12Device, ID3D12Fence, ID3D12Resource},
            Dxgi::{
                Common::{DXGI_FORMAT_R8G8B8A8_UNORM, DXGI_SAMPLE_DESC},
                IDXGIAdapter, IDXGIDevice, IDXGIResource1,
            },
        },
        System::Threading::{CreateEventW, INFINITE, WaitForSingleObject},
    },
    core::Interface,
};

/// Failure while preparing or synchronizing a shared D3D11/D3D12 texture.
#[derive(Debug)]
pub enum InteropError {
    /// The requested texture dimensions cannot describe a GPU image.
    InvalidSize,
    /// Torn's `wgpu` device is not backed by Direct3D 12.
    WgpuNotDx12,
    /// ANGLE's D3D11 device and Torn's D3D12 device use different adapters.
    AdapterMismatch,
    /// The GPU fence counter cannot advance without overflowing.
    FenceValueExhausted,
    /// A named Windows graphics operation failed.
    NativeOperation {
        /// Native operation that returned the error.
        operation: &'static str,
        /// Native error returned by Windows.
        error: windows::core::Error,
    },
    /// A Windows graphics API call failed.
    Windows(windows::core::Error),
}

impl fmt::Display for InteropError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSize => formatter.write_str("shared texture size must be non-zero"),
            Self::WgpuNotDx12 => formatter.write_str("wgpu is not using the Direct3D 12 backend"),
            Self::AdapterMismatch => {
                formatter.write_str("the D3D11 and D3D12 devices use different GPU adapters")
            }
            Self::FenceValueExhausted => formatter.write_str("shared GPU fence value overflowed"),
            Self::NativeOperation { operation, error } => {
                write!(formatter, "{operation} failed: {error}")
            }
            Self::Windows(error) => write!(formatter, "Windows graphics API failed: {error}"),
        }
    }
}

impl std::error::Error for InteropError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::NativeOperation { error, .. } | Self::Windows(error) => Some(error),
            Self::InvalidSize
            | Self::WgpuNotDx12
            | Self::AdapterMismatch
            | Self::FenceValueExhausted => None,
        }
    }
}

impl From<windows::core::Error> for InteropError {
    fn from(error: windows::core::Error) -> Self {
        Self::Windows(error)
    }
}

impl InteropError {
    fn with_operation(self, operation: &'static str) -> Self {
        match self {
            Self::Windows(error) => Self::NativeOperation { operation, error },
            error => error,
        }
    }
}

/// The D3D11 device and immediate context that own the producer's shared
/// texture.
///
/// ANGLE normally supplies both objects through Surfman. They must belong to
/// the same D3D11 device and select the same adapter as Torn's `wgpu` device.
pub struct Context {
    device: ID3D11Device,
    context: ID3D11DeviceContext4,
}

impl Context {
    /// Bundles the D3D11 device and immediate context used by the producer.
    #[must_use]
    pub const fn new(device: ID3D11Device, context: ID3D11DeviceContext4) -> Self {
        Self { device, context }
    }

    /// Returns the D3D11 device that created ANGLE's resources.
    #[must_use]
    pub const fn device(&self) -> &ID3D11Device {
        &self.device
    }

    /// Returns the immediate context that ANGLE records commands on.
    #[must_use]
    pub const fn context(&self) -> &ID3D11DeviceContext4 {
        &self.context
    }
}

/// A D3D11 render target imported as a `wgpu` D3D12 texture.
///
/// The producer must call [`Self::begin_producer_frame`] before ANGLE writes
/// the D3D11 texture, then call [`Self::signal_producer_ready`] after a
/// `glFlush`. The Torn renderer calls [`Self::wait_for_producer`] before using
/// [`Self::texture`], and [`Self::finish_consumer_frame`] after its final
/// submission. This creates only GPU-side waits; it never waits on the CPU.
pub struct SharedTexture {
    d3d11_texture: ID3D11Texture2D,
    d3d11_context: ID3D11DeviceContext4,
    d3d11_fence: ID3D11Fence,
    d3d12_fence: ID3D12Fence,
    texture: Texture,
    next_fence_value: u64,
    last_consumer_value: u64,
}

impl SharedTexture {
    /// Allocates a D3D11 shared texture and imports it into Torn's DX12 device.
    ///
    /// `context` must describe the same ANGLE/Surfman device that produces the
    /// browser frames. The constructor validates that this D3D11 device selects
    /// the same adapter as `wgpu_device` before creating any shared resources.
    ///
    /// `queue` is accepted for interface parity with the other Servo backends
    /// and is not used on Windows.
    ///
    /// # Errors
    ///
    /// Returns an error for a zero extent, a non-DX12 `wgpu` device, mismatched
    /// adapters, or a failing native graphics call.
    pub fn new(
        context: &Context,
        wgpu_device: &wgpu::Device,
        _queue: &wgpu::Queue,
        width: u32,
        height: u32,
    ) -> Result<Self, InteropError> {
        if width == 0 || height == 0 {
            return Err(InteropError::InvalidSize);
        }

        let d3d12_device = d3d12_device(wgpu_device)?;
        if d3d11_adapter_luid(context.device())? != d3d12_adapter_luid(&d3d12_device) {
            return Err(InteropError::AdapterMismatch);
        }

        let d3d11_texture = create_d3d11_texture(context.device(), width, height)
            .map_err(|error| error.with_operation("creating the D3D11 shared texture"))?;
        let d3d12_resource = open_texture_on_d3d12(&d3d11_texture, &d3d12_device)
            .map_err(|error| error.with_operation("opening the shared texture on D3D12"))?;
        let texture = import_texture(wgpu_device, d3d12_resource, width, height);
        let (d3d11_fence, d3d12_fence) = create_shared_fence(context.device(), &d3d12_device)
            .map_err(|error| error.with_operation("creating the shared D3D11/D3D12 fence"))?;

        Ok(Self {
            d3d11_texture,
            d3d11_context: context.context().clone(),
            d3d11_fence,
            d3d12_fence,
            texture,
            next_fence_value: 1,
            last_consumer_value: 0,
        })
    }

    /// Returns the D3D11 render target that ANGLE must attach to its pbuffer.
    #[must_use]
    pub fn d3d11_texture(&self) -> &ID3D11Texture2D {
        &self.d3d11_texture
    }

    /// Returns the imported texture that Torn composites in its DX12 render pass.
    #[must_use]
    pub fn texture(&self) -> &Texture {
        &self.texture
    }

    /// Queues a GPU-side D3D11 wait until Torn has finished the prior frame.
    ///
    /// Call this before ANGLE starts drawing the next frame into the texture.
    /// The first call only waits for fence value zero and is therefore ready
    /// immediately.
    ///
    /// # Errors
    ///
    /// Returns an error if Direct3D rejects the GPU-side wait.
    pub fn begin_producer_frame(&self) -> Result<(), InteropError> {
        // SAFETY: `d3d11_fence` was created by the same D3D11 device as this
        // immediate context. The value was last signalled by the D3D12 queue on
        // the same shared fence and the operation only queues a GPU-side wait.
        unsafe {
            self.d3d11_context
                .Wait(&self.d3d11_fence, self.last_consumer_value)?;
        }
        Ok(())
    }

    /// Signals that ANGLE submitted its writes to the shared texture.
    ///
    /// The caller must issue `glFlush` first so ANGLE's OpenGL commands are
    /// translated and queued on the D3D11 immediate context before this signal.
    ///
    /// # Errors
    ///
    /// Returns an error when the D3D11 signal fails or fence values are
    /// exhausted.
    pub fn signal_producer_ready(&mut self) -> Result<u64, InteropError> {
        let value = self.take_next_fence_value()?;
        // SAFETY: the D3D11 immediate context and fence have a common owning
        // device. The fence value is strictly monotonic and follows the ANGLE
        // flush required by this method's contract.
        unsafe {
            self.d3d11_context.Signal(&self.d3d11_fence, value)?;
        }
        Ok(value)
    }

    /// Queues a GPU-side DX12 wait before Torn samples the imported texture.
    ///
    /// `producer_value` must be the value returned by
    /// [`Self::signal_producer_ready`] for the frame about to be composited.
    ///
    /// # Errors
    ///
    /// Returns an error when `queue` is not a DX12 queue or Direct3D rejects
    /// the queued wait.
    pub fn wait_for_producer(
        &self,
        queue: &wgpu::Queue,
        producer_value: u64,
    ) -> Result<(), InteropError> {
        let hal_queue = unsafe {
            // SAFETY: the caller passes Torn's queue, paired with the DX12
            // device that imported this texture. The guard remains alive until
            // the raw command-queue call has completed.
            queue.as_hal::<Api>().ok_or(InteropError::WgpuNotDx12)?
        };
        // SAFETY: `d3d12_fence` was opened by the same DX12 device behind this
        // queue. This only queues a GPU-side wait, ordered before the following
        // `wgpu::Queue::submit` calls on this command queue.
        unsafe {
            hal_queue.as_raw().Wait(&self.d3d12_fence, producer_value)?;
        }
        Ok(())
    }

    /// Returns the texture to `COMMON` and signals completion to the D3D11 producer.
    ///
    /// Call this only after all Torn submissions that sample [`Self::texture`]
    /// for the current frame. The transition is submitted before the signal, so
    /// a later [`Self::begin_producer_frame`] sees both completion and the
    /// cross-API-compatible `COMMON` resource state. The signal is also the
    /// retirement marker observed by [`Self::wait_until_unused`].
    ///
    /// # Errors
    ///
    /// Returns an error when fence values are exhausted, `queue` is not DX12,
    /// or Direct3D rejects the final signal.
    pub fn finish_consumer_frame(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Result<(), InteropError> {
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Torn Servo shared-texture release"),
        });
        encoder.transition_resources(
            core::iter::empty(),
            core::iter::once(TextureTransition {
                texture: &self.texture,
                selector: None,
                state: TextureUses::PRESENT,
            }),
        );
        queue.submit([encoder.finish()]);

        let value = self.take_next_fence_value()?;
        let hal_queue = unsafe {
            // SAFETY: the caller passes Torn's DX12 queue. The guard keeps the
            // native queue alive while it receives the post-submission signal.
            queue.as_hal::<Api>().ok_or(InteropError::WgpuNotDx12)?
        };
        // SAFETY: this fence was opened by the DX12 device behind `queue`.
        // Queue submission is ordered, therefore this GPU-side signal executes
        // after the just-submitted transition and every prior Torn submission.
        unsafe {
            hal_queue.as_raw().Signal(&self.d3d12_fence, value)?;
        }
        self.last_consumer_value = value;
        Ok(())
    }

    /// Blocks until Torn's GPU has finished every submission that used this
    /// texture.
    ///
    /// This is the explicit retirement contract for [`Self::texture`]. Call it
    /// before dropping a shared texture whose imported resource may still be
    /// referenced by Torn's command queue, as happens when a browser surface is
    /// resized. The method waits on the CPU for the fence value signalled by
    /// [`Self::finish_consumer_frame`], which the queue executes after every
    /// submission that sampled the texture. When `finish_consumer_frame` was
    /// called after the final submission, as its contract requires, the native
    /// D3D11 texture and the imported D3D12 resource are no longer in use when
    /// this method returns.
    ///
    /// The wait may stall until the GPU drains all queued work. Callers should
    /// treat it as a rare, latency-insensitive boundary.
    ///
    /// `device` is accepted for interface parity with the other Servo backends
    /// and is not used because the completion fence is signalled directly on
    /// Torn's DX12 queue.
    ///
    /// # Errors
    ///
    /// Returns an error when the completion event cannot be created, the fence
    /// wait cannot be registered, or the CPU wait fails.
    pub fn wait_until_unused(&mut self, _device: &wgpu::Device) -> Result<(), InteropError> {
        if self.last_consumer_value == 0 {
            return Ok(());
        }

        // SAFETY: creates an unnamed, initially unsignalled manual-reset event.
        // The returned handle is owned by this call and closed below.
        let event = unsafe { CreateEventW(None, true, false, None) }.map_err(|error| {
            InteropError::from(error).with_operation("creating the consumer fence event")
        })?;

        let result = (|| {
            // SAFETY: `d3d12_fence` was opened by the DX12 device behind Torn's
            // queue and `event` is a live handle owned by this function. The
            // value was signalled after every submission that sampled this
            // texture, so observing it means the GPU has retired the texture.
            unsafe {
                self.d3d12_fence
                    .SetEventOnCompletion(self.last_consumer_value, event)?;
            }
            // SAFETY: `event` is a live handle and INFINITE waits until the
            // fence reaches the requested value.
            let wait = unsafe { WaitForSingleObject(event, INFINITE) };
            if wait != WAIT_OBJECT_0 {
                return Err(InteropError::Windows(windows::core::Error::from_thread())
                    .with_operation("waiting for the shared texture to retire"));
            }
            Ok(())
        })();

        // SAFETY: this function owns `event` and closes it exactly once.
        let close_result = unsafe { CloseHandle(event) };
        match (result, close_result) {
            (Err(error), _) => Err(error),
            (Ok(()), Err(error)) => {
                Err(InteropError::from(error).with_operation("closing the consumer fence event"))
            }
            (Ok(()), Ok(())) => Ok(()),
        }
    }

    fn take_next_fence_value(&mut self) -> Result<u64, InteropError> {
        let value = self.next_fence_value;
        self.next_fence_value = self
            .next_fence_value
            .checked_add(1)
            .ok_or(InteropError::FenceValueExhausted)?;
        Ok(value)
    }
}

fn d3d12_device(wgpu_device: &wgpu::Device) -> Result<ID3D12Device, InteropError> {
    let hal_device = unsafe {
        // SAFETY: only a short-lived clone of wgpu's DX12 device interface is
        // taken. The returned COM pointer is reference-counted independently of
        // the `as_hal` guard.
        wgpu_device
            .as_hal::<Api>()
            .ok_or(InteropError::WgpuNotDx12)?
    };
    Ok(hal_device.raw_device().clone())
}

fn d3d11_adapter_luid(device: &ID3D11Device) -> Result<LUID, InteropError> {
    let dxgi_device: IDXGIDevice = device.cast()?;
    let adapter: IDXGIAdapter = unsafe {
        // SAFETY: `dxgi_device` is a valid interface obtained from the D3D11
        // device and receives its owning adapter as a COM value.
        dxgi_device.GetAdapter()?
    };
    let description = unsafe {
        // SAFETY: `adapter` is valid for this call and the method writes a plain
        // value into an initialized return slot managed by windows-rs.
        adapter.GetDesc()?
    };
    Ok(description.AdapterLuid)
}

fn d3d12_adapter_luid(device: &ID3D12Device) -> LUID {
    // SAFETY: `device` is the live DX12 device that owns the imported resource.
    // The method only returns immutable adapter identification data.
    unsafe { device.GetAdapterLuid() }
}

fn create_d3d11_texture(
    device: &ID3D11Device,
    width: u32,
    height: u32,
) -> Result<ID3D11Texture2D, InteropError> {
    let description = D3D11_TEXTURE2D_DESC {
        Width: width,
        Height: height,
        MipLevels: 1,
        ArraySize: 1,
        Format: DXGI_FORMAT_R8G8B8A8_UNORM,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        Usage: D3D11_USAGE_DEFAULT,
        BindFlags: (D3D11_BIND_RENDER_TARGET.0 | D3D11_BIND_SHADER_RESOURCE.0) as u32,
        CPUAccessFlags: 0,
        MiscFlags: (D3D11_RESOURCE_MISC_SHARED.0 | D3D11_RESOURCE_MISC_SHARED_NTHANDLE.0) as u32,
    };
    let mut texture = None;
    // SAFETY: `description` fully describes a non-zero 2D texture. `texture`
    // is valid storage for the returned COM object and has no initial data.
    unsafe {
        device.CreateTexture2D(&raw const description, None, Some(&raw mut texture))?;
    }
    texture.ok_or(InteropError::Windows(windows::core::Error::from_thread()))
}

fn open_texture_on_d3d12(
    d3d11_texture: &ID3D11Texture2D,
    d3d12_device: &ID3D12Device,
) -> Result<ID3D12Resource, InteropError> {
    let resource: IDXGIResource1 = d3d11_texture.cast()?;
    let handle = unsafe {
        // SAFETY: the D3D11 texture was allocated with both SHARED and
        // SHARED_NTHANDLE flags. The unnamed NT handle is closed below exactly
        // once after D3D12 imports it.
        resource.CreateSharedHandle(None, GENERIC_ALL.0, None)?
    };
    let mut d3d12_resource = None;
    let open_result = unsafe {
        // SAFETY: `handle` is a live shared-resource NT handle. The DX12 device
        // has the same adapter LUID as the D3D11 producer and writes a COM value
        // into `d3d12_resource`.
        d3d12_device.OpenSharedHandle(handle, &raw mut d3d12_resource)
    };
    let close_result = unsafe {
        // SAFETY: this function is the exclusive owner of the NT handle returned
        // by CreateSharedHandle, and it is no longer needed after OpenSharedHandle.
        CloseHandle(handle)
    };
    open_result?;
    close_result?;
    d3d12_resource.ok_or(InteropError::Windows(windows::core::Error::from_thread()))
}

fn import_texture(
    device: &wgpu::Device,
    resource: ID3D12Resource,
    width: u32,
    height: u32,
) -> Texture {
    let extent = Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
    };
    let descriptor = TextureDescriptor {
        label: Some("Torn Servo D3D11 shared texture"),
        size: extent,
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Rgba8Unorm,
        usage: TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    };
    unsafe {
        // SAFETY: the resource was opened by this exact DX12 device; its extent,
        // format, mip count, and sample count match `descriptor`; D3D11 creates
        // the texture in a defined state. Wgpu tracks it from COMMON
        // (UNINITIALIZED) and this module returns it to COMMON after every use.
        device.create_texture_from_hal::<Api>(
            wgpu::hal::dx12::Device::texture_from_raw(
                resource,
                TextureFormat::Rgba8Unorm,
                TextureDimension::D2,
                extent,
                1,
                1,
            ),
            &descriptor,
        )
    }
}

fn create_shared_fence(
    d3d11_device: &ID3D11Device,
    d3d12_device: &ID3D12Device,
) -> Result<(ID3D11Fence, ID3D12Fence), InteropError> {
    let d3d11_device5: ID3D11Device5 = d3d11_device.cast()?;
    let mut d3d11_fence: Option<ID3D11Fence> = None;
    // SAFETY: `d3d11_device5` is the producer's live device and Windows writes
    // the requested fence interface into the owned `Option` storage.
    unsafe {
        d3d11_device5.CreateFence(0, D3D11_FENCE_FLAG_SHARED, &raw mut d3d11_fence)?;
    }
    let d3d11_fence =
        d3d11_fence.ok_or(InteropError::Windows(windows::core::Error::from_thread()))?;
    let handle = unsafe {
        // SAFETY: this shared D3D11 fence owns the returned NT handle. This
        // function closes that handle after its single D3D12 import.
        d3d11_fence.CreateSharedHandle(None, GENERIC_ALL.0, None)?
    };
    let mut d3d12_fence: Option<ID3D12Fence> = None;
    let open_result = unsafe {
        // SAFETY: `handle` names the live shared D3D11 fence and `d3d12_device`
        // is on the validated common adapter. The type parameter requests the
        // D3D12 fence interface into valid `Option` storage.
        d3d12_device.OpenSharedHandle(handle, &raw mut d3d12_fence)
    };
    let close_result = unsafe {
        // SAFETY: this function exclusively owns this CreateSharedHandle result.
        CloseHandle(handle)
    };
    open_result?;
    close_result?;
    let d3d12_fence =
        d3d12_fence.ok_or(InteropError::Windows(windows::core::Error::from_thread()))?;
    Ok((d3d11_fence, d3d12_fence))
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use super::{Context, InteropError, SharedTexture, d3d12_adapter_luid};
    use windows::{
        Win32::{
            Foundation::HMODULE,
            Graphics::{
                Direct3D::{D3D_DRIVER_TYPE_UNKNOWN, D3D_FEATURE_LEVEL_11_0},
                Direct3D11::{
                    D3D11_CREATE_DEVICE_FLAG, D3D11_SDK_VERSION, D3D11CreateDevice, ID3D11Device,
                    ID3D11DeviceContext, ID3D11DeviceContext4, ID3D11RenderTargetView,
                },
                Dxgi::{CreateDXGIFactory1, IDXGIAdapter, IDXGIFactory1},
            },
        },
        core::Interface,
    };

    #[test]
    fn reports_each_local_interop_failure() {
        assert_eq!(
            InteropError::InvalidSize.to_string(),
            "shared texture size must be non-zero"
        );
        assert_eq!(
            InteropError::AdapterMismatch.to_string(),
            "the D3D11 and D3D12 devices use different GPU adapters"
        );
        assert_eq!(
            InteropError::FenceValueExhausted.to_string(),
            "shared GPU fence value overflowed"
        );
    }

    /// Requires a Windows DX12 adapter and is intentionally excluded from regular CI.
    ///
    /// Run this with `cargo test -p torn-servo-windows -- --ignored` on a Windows
    /// machine before enabling the Servo producer path for users.
    #[test]
    #[ignore = "requires a Windows DX12-capable GPU"]
    fn creates_and_synchronizes_a_shared_texture_without_cpu_readback()
    -> Result<(), Box<dyn std::error::Error>> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::DX12,
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
        let d3d12_device = unsafe {
            // SAFETY: the test requires DX12 explicitly and clones the COM
            // pointer before dropping wgpu's temporary guard.
            device
                .as_hal::<wgpu::hal::dx12::Api>()
                .ok_or(InteropError::WgpuNotDx12)?
                .raw_device()
                .clone()
        };
        let adapter_luid = d3d12_adapter_luid(&d3d12_device);
        let d3d11_adapter = find_adapter(adapter_luid)?;
        let (d3d11_device, d3d11_context) = create_d3d11_device(&d3d11_adapter)?;

        let context = Context::new(d3d11_device, d3d11_context.clone());
        let mut texture = SharedTexture::new(&context, &device, &queue, 64, 64)?;
        texture.begin_producer_frame()?;
        clear_producer_texture(context.device(), &d3d11_context, &texture)?;
        let producer_value = texture.signal_producer_ready()?;
        unsafe {
            // SAFETY: the immediate context owns the clear and fence-signal
            // commands queued above. Flushing sends both to D3D11 without
            // waiting for GPU completion on the CPU.
            d3d11_context.Flush();
        }
        texture.wait_for_producer(&queue, producer_value)?;
        composite_shared_texture(&device, &queue, &texture)?;
        texture.finish_consumer_frame(&device, &queue)?;
        texture.wait_until_unused(&device)?;
        device.poll(wgpu::PollType::wait_indefinitely())?;
        Ok(())
    }

    fn clear_producer_texture(
        device: &ID3D11Device,
        context: &ID3D11DeviceContext4,
        texture: &SharedTexture,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut render_target_view: Option<ID3D11RenderTargetView> = None;
        unsafe {
            // SAFETY: the shared texture was created by this D3D11 device and
            // the output slot is owned storage for the returned view.
            device.CreateRenderTargetView(
                texture.d3d11_texture(),
                None,
                Some(&raw mut render_target_view),
            )?;
        }
        let render_target_view = render_target_view
            .ok_or("CreateRenderTargetView returned no view for the shared texture")?;
        unsafe {
            // SAFETY: the render-target view belongs to the immediate context's
            // D3D11 device. The color slice remains valid for the call.
            context.ClearRenderTargetView(&render_target_view, &[0.25, 0.5, 0.75, 1.0]);
        }
        Ok(())
    }

    #[allow(clippy::too_many_lines)]
    fn composite_shared_texture(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        texture: &SharedTexture,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let validation_scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let source_view = texture
            .texture()
            .create_view(&wgpu::TextureViewDescriptor::default());
        let destination = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Torn Servo interop smoke-test composite target"),
            size: wgpu::Extent3d {
                width: 64,
                height: 64,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let destination_view = destination.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Torn Servo interop smoke-test sampler"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Torn Servo interop smoke-test bind group layout"),
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
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Torn Servo interop smoke-test bind group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&source_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Torn Servo interop smoke-test shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(
                r"
                    struct VertexOutput {
                        @builtin(position) position: vec4<f32>,
                        @location(0) uv: vec2<f32>,
                    }

                    @group(0) @binding(0) var source_texture: texture_2d<f32>;
                    @group(0) @binding(1) var source_sampler: sampler;

                    @vertex
                    fn vertex(@builtin(vertex_index) index: u32) -> VertexOutput {
                        let positions = array<vec2<f32>, 3>(
                            vec2<f32>(-1.0, -1.0),
                            vec2<f32>(3.0, -1.0),
                            vec2<f32>(-1.0, 3.0),
                        );
                        let uvs = array<vec2<f32>, 3>(
                            vec2<f32>(0.0, 0.0),
                            vec2<f32>(2.0, 0.0),
                            vec2<f32>(0.0, 2.0),
                        );
                        var output: VertexOutput;
                        output.position = vec4<f32>(positions[index], 0.0, 1.0);
                        output.uv = uvs[index];
                        return output;
                    }

                    @fragment
                    fn fragment(input: VertexOutput) -> @location(0) vec4<f32> {
                        return textureSample(source_texture, source_sampler, input.uv);
                    }
                ",
            )),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Torn Servo interop smoke-test pipeline layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Torn Servo interop smoke-test pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vertex"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fragment"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Torn Servo interop smoke-test composite encoder"),
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Torn Servo interop smoke-test composite pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &destination_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.draw(0..3, 0..1);
        }
        queue.submit([encoder.finish()]);

        if let Some(error) = pollster::block_on(validation_scope.pop()) {
            return Err(error.into());
        }
        Ok(())
    }

    fn find_adapter(
        luid: windows::Win32::Foundation::LUID,
    ) -> Result<IDXGIAdapter, Box<dyn std::error::Error>> {
        let factory: IDXGIFactory1 = unsafe {
            // SAFETY: DXGI returns a reference-counted factory interface.
            CreateDXGIFactory1()?
        };
        for index in 0.. {
            let adapter = unsafe {
                // SAFETY: `factory` remains alive for the duration of adapter
                // enumeration and `index` is a monotonically increasing value.
                factory.EnumAdapters1(index)
            };
            let Ok(adapter) = adapter else {
                break;
            };
            let description = unsafe {
                // SAFETY: `adapter` is a live interface obtained from the factory.
                adapter.GetDesc1()?
            };
            if description.AdapterLuid == luid {
                return Ok(adapter.cast()?);
            }
        }
        Err("no DXGI adapter matches the wgpu DX12 device LUID".into())
    }

    fn create_d3d11_device(
        adapter: &IDXGIAdapter,
    ) -> Result<(ID3D11Device, ID3D11DeviceContext4), Box<dyn std::error::Error>> {
        let mut device = None;
        let mut context: Option<ID3D11DeviceContext> = None;
        let feature_levels = [D3D_FEATURE_LEVEL_11_0];
        unsafe {
            // SAFETY: `adapter` comes from DXGI and is passed with the required
            // UNKNOWN driver type. Both output slots are valid owned storage.
            D3D11CreateDevice(
                adapter,
                D3D_DRIVER_TYPE_UNKNOWN,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_FLAG::default(),
                Some(&feature_levels),
                D3D11_SDK_VERSION,
                Some(&raw mut device),
                None,
                Some(&raw mut context),
            )?;
        }
        let device = device.ok_or("D3D11CreateDevice returned no device")?;
        let context: ID3D11DeviceContext4 = context
            .ok_or("D3D11CreateDevice returned no immediate context")?
            .cast()?;
        Ok((device, context))
    }
}
