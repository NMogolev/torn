//! Browser surface and Servo integration layer for Torn.
//!
//! `BrowserSurface` owns the platform shared texture that a Servo producer
//! renders into and that Torn composites through its `wgpu` renderer. The
//! producer-agnostic [`BrowserView`] widget positions the texture inside Torn's
//! display list.
//!
//! The actual Servo engine adapter is deliberately not part of this crate. On
//! Windows it renders into the D3D11 texture exposed by the shared texture, on
//! Linux into its OpenGL texture, and on macOS into its Metal texture. Torn only
//! consumes the `wgpu` texture exposed by `BrowserSurface::texture`.

mod view;

pub use view::BrowserView;

#[cfg(any(windows, target_os = "linux", target_os = "macos"))]
mod surface;

#[cfg(any(windows, target_os = "linux", target_os = "macos"))]
pub use surface::{BrowserError, BrowserSurface};

#[cfg(any(windows, target_os = "linux", target_os = "macos"))]
pub use torn_servo::{Context, InteropError, SharedTexture};
