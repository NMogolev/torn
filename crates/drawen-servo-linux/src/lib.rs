//! Linux-only Vulkan dmabuf/EGL shared-texture interop for Drawen's Servo
//! backend.
//!
//! This crate is intentionally a low-level implementation boundary. It owns the
//! native handles and every `unsafe` operation required to exchange a linear
//! Vulkan image with Servo's EGL context. Higher layers use the facade exported
//! by `drawen-browser`.

#![cfg_attr(not(target_os = "linux"), allow(dead_code))]

#[cfg(target_os = "linux")]
mod surface;

#[cfg(target_os = "linux")]
pub use surface::{Context, InteropError, SharedTexture};

#[cfg(not(target_os = "linux"))]
/// Reports that the platform-specific Linux interop backend is unavailable.
#[derive(Debug)]
pub struct LinuxBackendUnavailable;
