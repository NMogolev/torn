//! PCM formats, decoded audio buffers, and channel mapping.

use std::fmt;
use std::time::Duration;

use crate::error::{Error, Result};

/// The largest channel count accepted by [`PcmFormat`].
pub const MAX_CHANNELS: usize = 32;
/// The largest sample rate accepted by [`PcmFormat`].
pub const MAX_SAMPLE_RATE: u32 = 768_000;

/// Linear gain applied to centre and surround channels when downmixing.
const SURROUND_GAIN: f32 = std::f32::consts::FRAC_1_SQRT_2;

/// The layout of interleaved `f32` PCM samples.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PcmFormat {
    sample_rate: u32,
    channels: usize,
}

impl PcmFormat {
    /// Creates a format for `sample_rate` hertz and `channels` interleaved channels.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidFormat`] when the sample rate is zero or above
    /// 768 kHz, or when the channel count is zero or above 32.
    pub fn new(sample_rate: u32, channels: usize) -> Result<Self> {
        if sample_rate == 0 || sample_rate > MAX_SAMPLE_RATE {
            return Err(Error::InvalidFormat(format!(
                "sample rate {sample_rate} is outside 1..={MAX_SAMPLE_RATE} Hz"
            )));
        }
        if channels == 0 || channels > MAX_CHANNELS {
            return Err(Error::InvalidFormat(format!(
                "channel count {channels} is outside 1..={MAX_CHANNELS}"
            )));
        }
        Ok(Self {
            sample_rate,
            channels,
        })
    }

    /// Returns the sample rate in hertz.
    #[must_use]
    pub const fn sample_rate(self) -> u32 {
        self.sample_rate
    }

    /// Returns the number of interleaved channels.
    #[must_use]
    pub const fn channels(self) -> usize {
        self.channels
    }

    /// Returns whether the format has exactly one channel.
    #[must_use]
    pub const fn is_mono(self) -> bool {
        self.channels == 1
    }

    /// Returns whether the format has exactly two channels.
    #[must_use]
    pub const fn is_stereo(self) -> bool {
        self.channels == 2
    }

    /// Returns how long one frame occupies.
    #[must_use]
    pub fn frame_duration(self) -> Duration {
        self.duration_for_frames(1)
    }

    /// Converts a frame count to a media duration.
    ///
    /// The result is truncated to whole nanoseconds, so converting a duration
    /// back to frames may lose less than one frame.
    #[must_use]
    pub fn duration_for_frames(self, frames: u64) -> Duration {
        let nanos = u128::from(frames) * 1_000_000_000 / u128::from(self.sample_rate);
        let seconds = u64::try_from(nanos / 1_000_000_000).unwrap_or(u64::MAX);
        let subsecond = u32::try_from(nanos % 1_000_000_000).unwrap_or(0);
        Duration::new(seconds, subsecond)
    }

    /// Converts a media duration to a whole frame count, rounding down.
    #[must_use]
    pub fn frames_for_duration(self, duration: Duration) -> u64 {
        let nanos = duration.as_nanos() * u128::from(self.sample_rate) / 1_000_000_000;
        u64::try_from(nanos).unwrap_or(u64::MAX)
    }
}

impl fmt::Display for PcmFormat {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} Hz, {} channel{}",
            self.sample_rate,
            self.channels,
            if self.channels == 1 { "" } else { "s" }
        )
    }
}

/// A block of decoded interleaved `f32` PCM.
///
/// Samples are normalized to `-1.0..=1.0`. [`Self::pts`] is the presentation
/// timestamp of the first frame, matching [`paleo::AudioBuffer`].
#[derive(Clone, Debug, PartialEq)]
pub struct PcmBuffer {
    pts: Duration,
    format: PcmFormat,
    samples: Vec<f32>,
}

impl PcmBuffer {
    /// Creates a buffer from interleaved samples.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidFormat`] when the sample count is not a whole
    /// number of frames.
    pub fn new(pts: Duration, format: PcmFormat, samples: Vec<f32>) -> Result<Self> {
        if !samples.len().is_multiple_of(format.channels()) {
            return Err(Error::InvalidFormat(format!(
                "{} samples are not a whole number of {}",
                samples.len(),
                format
            )));
        }
        Ok(Self {
            pts,
            format,
            samples,
        })
    }

    /// Creates a silent buffer with `frames` frames.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidFormat`] when the sample count overflows.
    pub fn silent(pts: Duration, format: PcmFormat, frames: usize) -> Result<Self> {
        let samples = frames
            .checked_mul(format.channels())
            .ok_or_else(|| Error::InvalidFormat(format!("{frames} frames do not fit in memory")))?;
        Self::new(pts, format, vec![0.0; samples])
    }

    /// Converts a decoded Paleo buffer without copying its sample storage.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidFormat`] when the decoded buffer has no valid
    /// sample rate or channel count.
    pub fn from_paleo(buffer: paleo::AudioBuffer) -> Result<Self> {
        let pts = buffer.pts();
        let format = PcmFormat::new(buffer.sample_rate(), buffer.channels())?;
        Self::new(pts, format, buffer.into_samples())
    }

    /// Returns the presentation timestamp of the first frame.
    #[must_use]
    pub const fn pts(&self) -> Duration {
        self.pts
    }

    /// Returns the sample layout.
    #[must_use]
    pub const fn format(&self) -> PcmFormat {
        self.format
    }

    /// Returns the interleaved samples.
    #[must_use]
    pub fn samples(&self) -> &[f32] {
        &self.samples
    }

    /// Returns the number of frames across all channels.
    #[must_use]
    pub fn frames(&self) -> usize {
        self.samples.len() / self.format.channels()
    }

    /// Returns whether the buffer contains no frames.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    /// Returns the timestamp just after the last frame.
    #[must_use]
    pub fn end(&self) -> Duration {
        let frames = u64::try_from(self.frames()).unwrap_or(u64::MAX);
        self.pts
            .saturating_add(self.format.duration_for_frames(frames))
    }

    /// Returns the interleaved samples of one frame.
    ///
    /// Returns `None` when `index` is outside the buffer.
    #[must_use]
    pub fn frame(&self, index: usize) -> Option<&[f32]> {
        let start = index.checked_mul(self.format.channels())?;
        let end = start.checked_add(self.format.channels())?;
        self.samples.get(start..end)
    }

    /// Consumes the buffer and returns its interleaved samples.
    #[must_use]
    pub fn into_samples(self) -> Vec<f32> {
        self.samples
    }

    /// Creates an empty buffer without re-validating a zero sample count.
    pub(crate) fn empty(pts: Duration, format: PcmFormat) -> Self {
        Self {
            pts,
            format,
            samples: Vec::new(),
        }
    }
}

/// Maps an interleaved source layout onto another channel count.
///
/// The mapping is deliberately simple and deterministic:
///
/// - equal channel counts are copied unchanged;
/// - mono is replicated to every output channel;
/// - any layout is averaged into mono;
/// - stereo is placed in the first two output channels and the rest are silent;
/// - three to eight channels are downmixed to stereo using the standard
///   `FL FR FC LFE BL BR ...` order, with centre and surrounds at
///   `-3 dB` (`1/sqrt(2)`) and LFE omitted;
/// - other layouts copy the shared channels positionally, drop extra input
///   channels, and leave extra output channels silent.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ChannelMap {
    source: usize,
    target: usize,
}

impl ChannelMap {
    /// Creates a mapping between `source` and `target` channel counts.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidFormat`] when either count is zero or above 32.
    pub fn new(source: usize, target: usize) -> Result<Self> {
        if source == 0 || source > MAX_CHANNELS {
            return Err(Error::InvalidFormat(format!(
                "source channel count {source} is outside 1..={MAX_CHANNELS}"
            )));
        }
        if target == 0 || target > MAX_CHANNELS {
            return Err(Error::InvalidFormat(format!(
                "target channel count {target} is outside 1..={MAX_CHANNELS}"
            )));
        }
        Ok(Self { source, target })
    }

    /// Returns the number of input channels.
    #[must_use]
    pub const fn source_channels(self) -> usize {
        self.source
    }

    /// Returns the number of output channels.
    #[must_use]
    pub const fn target_channels(self) -> usize {
        self.target
    }

    /// Returns whether the mapping is a straight copy.
    #[must_use]
    pub const fn is_identity(self) -> bool {
        self.source == self.target
    }

    /// Maps a block of interleaved samples into a new allocation.
    ///
    /// A trailing partial frame is ignored.
    #[must_use]
    pub fn map(self, input: &[f32]) -> Vec<f32> {
        let mut output = Vec::with_capacity(
            input
                .len()
                .saturating_div(self.source)
                .saturating_mul(self.target),
        );
        self.map_into(input, &mut output);
        output
    }

    /// Maps a block of interleaved samples into `output`, replacing its contents.
    ///
    /// A trailing partial frame is ignored. Returns the number of frames written.
    pub fn map_into(self, input: &[f32], output: &mut Vec<f32>) -> usize {
        output.clear();
        for frame in input.chunks_exact(self.source) {
            let start = output.len();
            output.resize(start + self.target, 0.0);
            self.map_frame(frame, &mut output[start..]);
        }
        output.len() / self.target
    }

    fn map_frame(self, frame: &[f32], output: &mut [f32]) {
        match (self.source, self.target) {
            (1, _) => output.fill(frame[0]),
            (_, 1) => {
                let sum: f32 = frame.iter().copied().sum();
                let count = u8::try_from(frame.len()).unwrap_or(u8::MAX);
                output[0] = sum / f32::from(count);
            }
            (2, _) => {
                output.fill(0.0);
                output[0] = frame[0];
                output[1] = frame[1];
            }
            (_, 2) => downmix_to_stereo(frame, output),
            _ => {
                output.fill(0.0);
                let shared = self.source.min(self.target);
                output[..shared].copy_from_slice(&frame[..shared]);
            }
        }
    }
}

/// Downmixes a three-to-eight-channel frame to stereo.
///
/// The channel order follows the Wave/ITU convention. LFE is omitted because
/// it carries no directional information and downmixing it makes the result
/// boomy in small speakers.
fn downmix_to_stereo(frame: &[f32], output: &mut [f32]) {
    let mut left = frame[0];
    let mut right = frame[1];
    match frame.len() {
        3 => {
            left += frame[2] * SURROUND_GAIN;
            right += frame[2] * SURROUND_GAIN;
        }
        4 => {
            left += frame[2] * SURROUND_GAIN;
            right += frame[3] * SURROUND_GAIN;
        }
        5 => {
            left += frame[2] * SURROUND_GAIN + frame[3] * SURROUND_GAIN;
            right += frame[2] * SURROUND_GAIN + frame[4] * SURROUND_GAIN;
        }
        6 => {
            left += frame[2] * SURROUND_GAIN + frame[4] * SURROUND_GAIN;
            right += frame[2] * SURROUND_GAIN + frame[5] * SURROUND_GAIN;
        }
        7 => {
            let back = frame[4] * 0.5 * SURROUND_GAIN;
            left += frame[2] * SURROUND_GAIN + back + frame[5] * SURROUND_GAIN;
            right += frame[2] * SURROUND_GAIN + back + frame[6] * SURROUND_GAIN;
        }
        _ => {
            left += frame[2] * SURROUND_GAIN + frame[4] * SURROUND_GAIN + frame[6] * SURROUND_GAIN;
            right += frame[2] * SURROUND_GAIN + frame[5] * SURROUND_GAIN + frame[7] * SURROUND_GAIN;
        }
    }
    output[0] = left;
    output[1] = right;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn format(channels: usize) -> PcmFormat {
        PcmFormat::new(48_000, channels).expect("valid test format")
    }

    #[test]
    fn formats_convert_between_frames_and_time() {
        let format = format(2);
        assert_eq!(format.frame_duration(), Duration::from_nanos(20_833));
        assert_eq!(format.duration_for_frames(48_000), Duration::from_secs(1));
        assert_eq!(format.frames_for_duration(Duration::from_secs(1)), 48_000);
    }

    #[test]
    fn buffers_validate_frame_alignment() {
        let error = PcmBuffer::new(Duration::ZERO, format(2), vec![0.0; 3])
            .expect_err("an odd sample count is not stereo");
        assert!(matches!(error, Error::InvalidFormat(_)));

        let buffer = PcmBuffer::silent(Duration::ZERO, format(2), 4).expect("silent buffer");
        assert_eq!(buffer.frames(), 4);
        assert_eq!(buffer.samples().len(), 8);
        assert_eq!(buffer.frame(3), Some(&[0.0, 0.0][..]));
        assert_eq!(buffer.frame(4), None);
    }

    #[test]
    fn mono_is_replicated_to_every_target_channel() {
        let map = ChannelMap::new(1, 3).expect("valid map");
        assert_eq!(map.map(&[0.25, 0.5]), vec![0.25, 0.25, 0.25, 0.5, 0.5, 0.5]);
    }

    #[test]
    fn any_layout_is_averaged_into_mono() {
        let map = ChannelMap::new(2, 1).expect("valid map");
        assert_eq!(map.map(&[0.5, -0.5, 1.0, 0.0]), vec![0.0, 0.5]);
    }

    #[test]
    fn stereo_fills_only_the_front_pair_of_a_wider_layout() {
        let map = ChannelMap::new(2, 4).expect("valid map");
        assert_eq!(map.map(&[0.5, -0.5]), vec![0.5, -0.5, 0.0, 0.0]);
    }

    #[test]
    fn surround_is_downmixed_with_standard_gains() {
        let map = ChannelMap::new(6, 2).expect("valid map");
        let frame = [1.0, 0.5, 0.25, 0.75, 0.125, 0.25];
        let mapped = map.map(&frame);
        let gain = SURROUND_GAIN;
        assert!((mapped[0] - (1.0 + 0.25 * gain + 0.125 * gain)).abs() < 1e-6);
        assert!((mapped[1] - (0.5 + 0.25 * gain + 0.25 * gain)).abs() < 1e-6);
    }

    #[test]
    fn identity_maps_copy_samples() {
        let map = ChannelMap::new(2, 2).expect("valid map");
        let input = [0.0, 1.0, -1.0, 0.5];
        assert_eq!(map.map(&input), input);
        assert!(map.is_identity());
    }

    #[test]
    fn partial_frames_are_ignored() {
        let map = ChannelMap::new(2, 1).expect("valid map");
        assert_eq!(map.map(&[0.5, 0.5, 0.25]), vec![0.5]);
    }
}
