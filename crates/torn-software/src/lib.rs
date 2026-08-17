//! Deterministic reference renderer and test support for Torn.

mod pixel_buffer;
mod renderer;

pub use pixel_buffer::{Pixel, PixelBuffer, PixelBufferError};
pub use renderer::{RenderError, SoftwareRenderer};
