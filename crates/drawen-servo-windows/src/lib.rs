//! Windows-only DXGI shared-texture interop for Drawen's Servo backend.
//!
//! This crate is intentionally a low-level implementation boundary. It owns the
//! native handles and every `unsafe` operation required to exchange an ANGLE
//! D3D11 texture with Drawen's D3D12 `wgpu` device. Higher layers use the facade
//! exported by `drawen-browser`.

#![cfg_attr(not(windows), allow(dead_code))]

#[cfg(windows)]
mod dx12;

#[cfg(windows)]
pub use dx12::{Context, InteropError, SharedTexture};

#[cfg(not(windows))]
/// Reports that the platform-specific Windows interop backend is unavailable.
#[derive(Debug)]
pub struct WindowsBackendUnavailable;
