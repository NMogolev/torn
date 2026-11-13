//! Paleo is Torn's pure-Rust media layer.
//!
//! Paleo does not implement containers or codecs itself. It assembles focused
//! crates behind one streaming API:
//!
//! - containers are demuxed by [Symphonia](https://docs.rs/symphonia), which
//!   reads MP4, Matroska/WebM, Ogg, WAV, AIFF, CAF, FLAC, and MPEG audio
//!   without loading the file into memory;
//! - audio is decoded by Symphonia's pure-Rust codecs and normalized to
//!   interleaved `f32`;
//! - video is demuxed into compressed packets for an external decoder, so
//!   H.264, HEVC, AV1, and VP9 backends can be added or removed per build.
//!
//! The contract is intentionally small:
//!
//! ```
//! use paleo::{Event, Media};
//! # fn run() -> Result<(), paleo::Error> {
//! let mut media = Media::open("clip.wav")?;
//! loop {
//!     match media.next_event()? {
//!         Event::Audio(audio) => { let _ = audio.samples(); }
//!         Event::Video(packet) => { let _ = packet.data(); }
//!         Event::EndOfStream => break,
//!     }
//! }
//! # Ok(()) }
//! ```
//!
//! Memory stays bounded during playback: a fixed source buffer, one decoder
//! state, and the packet or audio block returned by
//! [`Media::next_event`]. Nothing reads the whole container into memory and no
//! decoded frame queue is kept internally.

#![forbid(unsafe_code)]

mod audio;
mod codec;
mod error;
mod media;
mod video;

pub use audio::AudioBuffer;
pub use codec::{AudioCodec, VideoCodec};
pub use error::{Error, Result};
pub use media::{AudioTrackInfo, Event, Media, MediaInfo, VideoTrackInfo};
pub use video::{VideoExtraData, VideoExtraDataKind, VideoPacket};
