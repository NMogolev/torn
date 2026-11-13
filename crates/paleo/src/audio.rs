//! Decoded audio buffers.

use std::time::Duration;

/// A block of decoded audio in interleaved `f32` samples.
///
/// Samples are normalized to `-1.0..=1.0` regardless of the source sample format.
/// `channels` describes the interleaving stride and `pts` is the presentation
/// timestamp of the first frame in the buffer.
#[derive(Clone, Debug)]
pub struct AudioBuffer {
    pts: Duration,
    sample_rate: u32,
    channels: usize,
    samples: Vec<f32>,
}

impl AudioBuffer {
    pub(crate) fn new(pts: Duration, sample_rate: u32, channels: usize, samples: Vec<f32>) -> Self {
        Self {
            pts,
            sample_rate,
            channels,
            samples,
        }
    }

    /// Returns the presentation timestamp of the first frame.
    #[must_use]
    pub const fn pts(&self) -> Duration {
        self.pts
    }

    /// Returns the sample rate in hertz.
    #[must_use]
    pub const fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Returns the number of interleaved channels.
    #[must_use]
    pub const fn channels(&self) -> usize {
        self.channels
    }

    /// Returns the interleaved samples.
    #[must_use]
    pub fn samples(&self) -> &[f32] {
        &self.samples
    }

    /// Returns the number of frames across all channels.
    #[must_use]
    pub fn frames(&self) -> usize {
        self.samples
            .len()
            .checked_div(self.channels)
            .unwrap_or_default()
    }

    /// Returns whether the buffer contains no frames.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.frames() == 0
    }
}
