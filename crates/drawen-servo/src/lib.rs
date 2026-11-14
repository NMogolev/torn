//! Platform-neutral selection of Drawen's Servo shared-texture interop.
//!
//! The platform crates expose the same `Context`, `SharedTexture`, and
//! `InteropError` names and the same `SharedTexture::new(context, device,
//! queue, width, height)` constructor. This crate re-exports the implementation
//! for the current target so higher layers do not need `cfg` blocks around
//! every interop call. The `drawen-browser` crate builds the producer/consumer
//! hand-off, GPU registration, and widget integration on top of it.

#![cfg_attr(
    not(any(windows, target_os = "linux", target_os = "macos")),
    allow(dead_code)
)]

#[cfg(windows)]
pub use drawen_servo_windows::{Context, InteropError, SharedTexture};

#[cfg(target_os = "linux")]
pub use drawen_servo_linux::{Context, InteropError, SharedTexture};

#[cfg(target_os = "macos")]
pub use drawen_servo_macos::{Context, InteropError, SharedTexture};

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
/// Reports that Drawen has no Servo shared-texture backend for this target.
#[derive(Debug)]
pub struct UnsupportedPlatform;
