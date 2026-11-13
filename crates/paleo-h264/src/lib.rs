//! H.264 video decoding backend for [Paleo](https://docs.rs/paleo).
//!
//! Paleo demuxes H.264 into compressed packets and exposes the track's `avcC`
//! record. This crate bridges the two to the pure-Rust
//! [`rusty_h264-decoder`](https://docs.rs/rusty_h264-decoder):
//!
//! - it converts length-prefixed (AVCC) packets to Annex-B access units and
//!   prepends SPS/PPS at every keyframe;
//! - it decodes with a single streaming decoder, without a packet queue;
//! - it orders B-pictures by the decoded `PicOrderCnt` with a bounded window
//!   of four pictures, because Symphonia 0.6 does not expose ISO/MP4
//!   composition offsets and reports presentation time equal to decode time;
//! - it converts the decoder's YUV 4:2:0 output to tightly packed RGBA.
//!
//! ```no_run
//! use std::time::Duration;
//! use paleo::{Event, Media};
//! use paleo_h264::H264Decoder;
//!
//! # fn run() -> Result<(), Box<dyn std::error::Error>> {
//! let mut media = Media::open("clip.mp4")?;
//! let mut decoder = H264Decoder::new();
//! if let Some(video) = media.info().video() {
//!     decoder.configure(video.extra_data())?;
//! }
//!
//! loop {
//!     match media.next_event()? {
//!         Event::Video(packet) => {
//!             for frame in decoder.decode(&packet)? {
//!                 let _rgba: &[u8] = frame.pixels();
//!             }
//!         }
//!         Event::Audio(_) => {}
//!         Event::EndOfStream => break,
//!     }
//! }
//! for frame in decoder.flush() {
//!     let _ = frame.pts();
//! }
//! # Ok(()) }
//! ```
//!
//! The decoder is feature-gated at the call site: a build that does not need
//! video never links it. Seeking is handled by [`H264Decoder::reset`], which
//! makes the decoder skip inter pictures until the next IDR.

#![forbid(unsafe_code)]

mod avc;
mod decoder;
mod error;
mod rgba;

pub use decoder::{DEFAULT_REORDER_DEPTH, H264Decoder, VideoFrame};
pub use error::{Error, Result};
pub use rgba::ColorMatrix;
