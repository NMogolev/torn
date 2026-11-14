//! Deterministic software renderer and image-test support for Drawen.
//!
//! [`SoftwareRenderer`] executes the same [`DisplayList`](drawen_render::DisplayList)
//! as the GPU renderer and writes into a [`PixelBuffer`]. The output is
//! deterministic, so tests can assert pixels or compare stable PNG fixtures
//! without a window or a GPU. [`SoftwareRenderWorker`] moves rendering off the
//! UI thread for windowed applications.

mod pixel_buffer;
mod png;
mod renderer;
mod worker;

pub use pixel_buffer::{Pixel, PixelBuffer, PixelBufferError, PngError};
pub use renderer::{RenderError, SoftwareRenderer};
pub use worker::{
    ReceiveError, SoftwareRenderError, SoftwareRenderResult, SoftwareRenderWorker, SubmitError,
};
