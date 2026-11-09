//! Windows-only DXGI shared-texture interop for Torn's Servo backend.
//!
//! This crate is intentionally an internal implementation boundary. It owns the
//! native handles and every `unsafe` operation required to exchange an ANGLE
//! D3D11 texture with Torn's D3D12 `wgpu` device. Higher layers use the facade
//! exported by `torn-browser`.

#![cfg_attr(not(windows), allow(dead_code))]

#[cfg(windows)]
mod dx12;

#[cfg(windows)]
pub use dx12::{Context, InteropError, SharedTexture};

#[cfg(not(windows))]
/// Reports that the internal Windows interop backend is unavailable.
#[derive(Debug)]
pub struct WindowsBackendUnavailable;
