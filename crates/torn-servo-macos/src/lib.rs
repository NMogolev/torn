//! macOS-only `IOSurface` shared-texture interop for Torn's Servo backend.
//!
//! This crate is intentionally an internal implementation boundary. It owns the
//! native handles and every `unsafe` operation required to exchange a
//! Metal-backed `IOSurface` with Torn's `wgpu` Metal device. Higher layers use
//! the facade exported by `torn-browser`.

#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

#[cfg(target_os = "macos")]
mod surface;

#[cfg(target_os = "macos")]
pub use surface::{Context, InteropError, SharedTexture};

#[cfg(not(target_os = "macos"))]
/// Reports that the internal macOS interop backend is unavailable.
#[derive(Debug)]
pub struct MacosBackendUnavailable;
