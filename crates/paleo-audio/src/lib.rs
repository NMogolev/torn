//! Audio output and PCM utilities for [Paleo](https://docs.rs/paleo).
//!
//! Paleo itself is a headless media layer: it demuxes containers, decodes
//! audio into [`paleo::AudioBuffer`] blocks, and demuxes video. This crate adds
//! the platform-dependent half of playback:
//!
//! - [`PcmFormat`] and [`PcmBuffer`] describe decoded PCM without a device;
//! - [`ChannelMap`] adapts mono, stereo, and surround layouts;
//! - [`LinearResampler`] and [`PcmConverter`] stream decoded media into the
//!   device's rate and channel layout;
//! - [`AudioOutput`] owns the device, a bounded SPSC PCM queue, volume, mute,
//!   pause, underrun diagnostics, and the playback position that can drive a
//!   [`MediaClock`](https://docs.rs/drawen-widgets/latest/drawen_widgets/struct.MediaClock.html).
//!
//! The `device` feature is enabled by default. A metadata-only, server, or test
//! build can disable default features and use the PCM utilities without linking
//! `cpal` or a native audio API:
//!
//! ```toml
//! paleo-audio = { version = "0.1", default-features = false }
//! ```
//!
//! # Playback
//!
//! ```no_run
//! use std::time::Duration;
//! use paleo::{Event, Media};
//! use paleo_audio::{AudioOptions, AudioOutput, PcmConverter};
//!
//! # fn run() -> Result<(), Box<dyn std::error::Error>> {
//! let mut media = Media::open("clip.mp4")?;
//! let mut output = AudioOutput::open_with(&AudioOptions::new())?;
//! let track = media
//!     .info()
//!     .audio()
//!     .ok_or("the file has no audio")?;
//! let source = paleo_audio::PcmFormat::new(track.sample_rate(), track.channels())?;
//! let mut converter = PcmConverter::new(source, output.format())?;
//!
//! output.play()?;
//! loop {
//!     match media.next_event()? {
//!         Event::Audio(buffer) => {
//!             let pcm = converter.push(&paleo_audio::PcmBuffer::from_paleo(buffer)?)?;
//!             output.queue(&pcm)?;
//!             if output.buffered() > Duration::from_millis(200) {
//!                 std::thread::sleep(Duration::from_millis(10));
//!             }
//!         }
//!         Event::Video(_) => {}
//!         Event::EndOfStream => break,
//!     }
//! }
//! # Ok(()) }
//! ```
//!
//! The audio device is the master clock: [`AudioOutput::position`] counts
//! frames the device actually consumed. A media session should call
//! `MediaClock::synchronize(output.position())` instead of advancing the clock
//! from wall time when audio is available.

#![forbid(unsafe_code)]

mod buffer;
mod error;
mod resample;

#[cfg(feature = "device")]
mod output;

pub use buffer::{ChannelMap, MAX_CHANNELS, MAX_SAMPLE_RATE, PcmBuffer, PcmFormat};
pub use error::{Error, Result};
pub use resample::{LinearResampler, PcmConverter};

#[cfg(feature = "device")]
pub use output::{AudioOptions, AudioOutput, DEFAULT_LATENCY, OutputDeviceInfo, QueueReport};
