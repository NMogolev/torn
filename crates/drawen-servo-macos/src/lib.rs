//! macOS-only `IOSurface` shared-texture interop for Drawen's Servo backend.
//!
//! This crate is intentionally a low-level implementation boundary. It owns the
//! native handles and every `unsafe` operation required to exchange a
//! Metal-backed `IOSurface` with Drawen's `wgpu` Metal device. Higher layers use
//! the facade exported by `drawen-browser`.

#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

#[cfg(target_os = "macos")]
mod surface;

#[cfg(target_os = "macos")]
pub use surface::{Context, InteropError, SharedTexture};

#[cfg(not(target_os = "macos"))]
/// Reports that the platform-specific macOS interop backend is unavailable.
#[derive(Debug)]
pub struct MacosBackendUnavailable;
