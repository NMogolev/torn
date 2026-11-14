//! Streaming sample-rate conversion and PCM layout adaptation.

use std::time::Duration;

use crate::buffer::{ChannelMap, PcmBuffer, PcmFormat};
use crate::error::{Error, Result};

/// Cursor tolerance in input frames.
const EPSILON: f64 = 1e-9;

/// A streaming linear-interpolation resampler.
///
/// The resampler keeps one decoded frame of history, so consecutive input
/// buffers are interpolated without a seam. It never buffers a whole block:
/// [`Self::process`] returns at most `input_frames * output_rate / input_rate`
/// frames.
#[derive(Clone, Debug)]
pub struct LinearResampler {
    input: PcmFormat,
    output: PcmFormat,
    step: f64,
    next: u64,
    cursor: f64,
    tail: Vec<f32>,
    primed: bool,
}

impl LinearResampler {
    /// Creates a resampler from formats with the same channel count.
    ///
    /// # Errors
    ///
    /// Returns [`Error::FormatMismatch`] when the channel counts differ. Use
    /// [`ChannelMap`] first when the layouts differ.
    pub fn new(input: PcmFormat, output: PcmFormat) -> Result<Self> {
        if input.channels() != output.channels() {
            return Err(Error::FormatMismatch {
                expected: output.to_string(),
                actual: input.to_string(),
            });
        }
        Ok(Self {
            input,
            output,
            step: f64::from(input.sample_rate()) / f64::from(output.sample_rate()),
            next: 0,
            cursor: 0.0,
            tail: Vec::new(),
            primed: false,
        })
    }

    /// Returns the source format.
    #[must_use]
    pub const fn input_format(&self) -> PcmFormat {
        self.input
    }

    /// Returns the destination format.
    #[must_use]
    pub const fn output_format(&self) -> PcmFormat {
        self.output
    }

    /// Returns whether no rate conversion is performed.
    #[must_use]
    pub const fn is_identity(&self) -> bool {
        self.input.sample_rate() == self.output.sample_rate()
    }

    /// Discards interpolation history and starts a new stream at frame zero.
    pub fn reset(&mut self) {
        self.next = 0;
        self.cursor = 0.0;
        self.tail.clear();
        self.primed = false;
    }

    /// Converts one interleaved block and returns the interleaved result.
    ///
    /// A trailing partial frame is ignored. The output can be empty when the
    /// block is too short to advance the cursor.
    pub fn process(&mut self, input: &[f32]) -> Vec<f32> {
        let channels = self.input.channels();
        let frames = input.len() / channels;
        if frames == 0 {
            return Vec::new();
        }

        let start = self.next;
        let last = start.saturating_add(u64::try_from(frames - 1).unwrap_or(u64::MAX));
        let last_cursor = frame_as_f64(last);
        let mut output = Vec::with_capacity(self.capacity_for(frames));

        while self.cursor <= last_cursor + EPSILON {
            let index = cursor_frame(self.cursor);
            let fraction = cursor_fraction(self.cursor);
            for channel in 0..channels {
                let from = self.sample(index, channel, start, input);
                let to = if fraction == 0.0 {
                    from
                } else {
                    self.sample(index.saturating_add(1), channel, start, input)
                };
                output.push(from + (to - from) * fraction);
            }
            self.cursor += self.step;
        }

        self.tail.clear();
        self.tail
            .extend_from_slice(&input[input.len() - channels..]);
        self.primed = true;
        self.next = start.saturating_add(u64::try_from(frames).unwrap_or(u64::MAX));
        output
    }

    fn capacity_for(&self, frames: usize) -> usize {
        let frames = u128::from(u64::try_from(frames).unwrap_or(u64::MAX));
        let estimate = frames * u128::from(self.output.sample_rate())
            / u128::from(self.input.sample_rate())
            + 2;
        usize::try_from(estimate).unwrap_or_else(|_| {
            usize::try_from(frames)
                .unwrap_or(usize::MAX)
                .saturating_add(2)
        })
    }

    fn sample(&self, index: u64, channel: usize, start: u64, input: &[f32]) -> f32 {
        if index.checked_add(1) == Some(start) && self.primed {
            return self.tail.get(channel).copied().unwrap_or(0.0);
        }
        if index < start {
            return 0.0;
        }
        let offset = usize::try_from(index - start).unwrap_or(usize::MAX);
        offset
            .checked_mul(self.input.channels())
            .and_then(|position| position.checked_add(channel))
            .and_then(|position| input.get(position))
            .copied()
            .unwrap_or(0.0)
    }
}

/// Converts absolute input frame indices to the interpolation cursor.
///
/// A media stream can run for days, well past `f64`'s exact integer range for
/// sub-second precision, but the cursor only needs input-frame resolution and
/// the tail frame covers the fractional boundary.
#[allow(clippy::cast_precision_loss)]
fn frame_as_f64(frame: u64) -> f64 {
    frame as f64
}

/// Truncates a non-negative cursor to an input frame index.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn cursor_frame(cursor: f64) -> u64 {
    cursor as u64
}

/// Returns the fractional part of a cursor in `0.0..1.0`.
#[allow(clippy::cast_possible_truncation)]
fn cursor_fraction(cursor: f64) -> f32 {
    (cursor - cursor.trunc()) as f32
}

/// Adapts decoded Paleo buffers to an output device format.
///
/// A converter applies the channel map first and the resampler second, then
/// assigns timestamps on the output timeline. Call [`Self::reset`] after a seek
/// so interpolation history and timestamps do not leak across the discontinuity.
#[derive(Clone, Debug)]
pub struct PcmConverter {
    source: PcmFormat,
    target: PcmFormat,
    map: ChannelMap,
    resampler: LinearResampler,
    mapped: Vec<f32>,
    timeline: Option<Duration>,
    trim_from: Option<Duration>,
}

impl PcmConverter {
    /// Creates a converter from `source` to `target`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidFormat`] or [`Error::FormatMismatch`] when the
    /// formats cannot be mapped and resampled.
    pub fn new(source: PcmFormat, target: PcmFormat) -> Result<Self> {
        let map = ChannelMap::new(source.channels(), target.channels())?;
        // Channel mapping runs before resampling, so the resampler always sees
        // the target channel count at the source sample rate.
        let mapped = PcmFormat::new(source.sample_rate(), target.channels())?;
        let resampler = LinearResampler::new(mapped, target)?;
        Ok(Self {
            source,
            target,
            map,
            resampler,
            mapped: Vec::new(),
            timeline: None,
            trim_from: None,
        })
    }

    /// Returns the source format this converter accepts.
    #[must_use]
    pub const fn source_format(&self) -> PcmFormat {
        self.source
    }

    /// Returns the output format this converter produces.
    #[must_use]
    pub const fn target_format(&self) -> PcmFormat {
        self.target
    }

    /// Clears interpolation history and timestamp continuity.
    pub fn reset(&mut self) {
        self.resampler.reset();
        self.mapped.clear();
        self.timeline = None;
        self.trim_from = None;
    }

    /// Restarts the output timeline at `position` and discards decoded audio
    /// before it.
    ///
    /// Containers seek to a keyframe, so the first audio block after a seek can
    /// begin before the requested position. Trimming keeps the audio clock and
    /// the media clock aligned. Audio that ends before `position` is skipped
    /// until a buffer that overlaps it arrives.
    pub fn reset_at(&mut self, position: Duration) {
        self.reset();
        self.timeline = Some(position);
        self.trim_from = Some(position);
    }

    /// Returns the next output timestamp, or `None` before the first push.
    #[must_use]
    pub const fn position(&self) -> Option<Duration> {
        self.timeline
    }

    /// Converts one decoded buffer.
    ///
    /// # Errors
    ///
    /// Returns [`Error::FormatMismatch`] when `buffer` does not use the source
    /// format.
    pub fn push(&mut self, buffer: &PcmBuffer) -> Result<PcmBuffer> {
        if buffer.format() != self.source {
            return Err(Error::FormatMismatch {
                expected: self.source.to_string(),
                actual: buffer.format().to_string(),
            });
        }

        if self.trim_from.is_some_and(|target| buffer.end() <= target) {
            return Ok(PcmBuffer::empty(
                self.timeline.unwrap_or_default(),
                self.target,
            ));
        }

        let pts = *self.timeline.get_or_insert(buffer.pts());
        if buffer.is_empty() {
            return Ok(PcmBuffer::empty(pts, self.target));
        }

        let mut samples = buffer.samples();
        if let Some(target) = self.trim_from.take()
            && buffer.pts() < target
        {
            let frames = self
                .source
                .frames_for_duration(target.saturating_sub(buffer.pts()));
            let offset = usize::try_from(frames)
                .unwrap_or(usize::MAX)
                .saturating_mul(self.source.channels());
            samples = samples.get(offset..).unwrap_or_default();
        }

        self.map.map_into(samples, &mut self.mapped);
        let samples = if self.resampler.is_identity() {
            std::mem::take(&mut self.mapped)
        } else {
            self.resampler.process(&self.mapped)
        };
        let frames = samples.len() / self.target.channels();
        let advance = self
            .target
            .duration_for_frames(u64::try_from(frames).unwrap_or(u64::MAX));
        self.timeline = Some(pts.saturating_add(advance));
        PcmBuffer::new(pts, self.target, samples)
    }

    /// Returns an empty buffer at the current output timeline position.
    ///
    /// The linear resampler has no delayed tail to emit: its interpolation
    /// cursor always advances past the last input frame.
    #[must_use]
    pub fn flush(&self) -> PcmBuffer {
        PcmBuffer::empty(self.timeline.unwrap_or_default(), self.target)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn format(rate: u32, channels: usize) -> PcmFormat {
        PcmFormat::new(rate, channels).expect("valid test format")
    }

    fn buffer(format: PcmFormat, samples: &[f32]) -> PcmBuffer {
        PcmBuffer::new(Duration::ZERO, format, samples.to_vec()).expect("aligned buffer")
    }

    #[test]
    fn doubling_the_rate_interpolates_between_frames() {
        let mut resampler =
            LinearResampler::new(format(24_000, 1), format(48_000, 1)).expect("valid resampler");
        let output = resampler.process(&[0.0, 1.0, 0.0, -1.0]);
        assert_eq!(output.len(), 7);
        let expected = [0.0, 0.5, 1.0, 0.5, 0.0, -0.5, -1.0];
        for (actual, expected) in output.iter().zip(expected) {
            assert!((actual - expected).abs() < 1e-6, "{actual} != {expected}");
        }
    }

    #[test]
    fn halving_the_rate_drops_frames() {
        let mut resampler =
            LinearResampler::new(format(96_000, 1), format(48_000, 1)).expect("valid resampler");
        let input: Vec<f32> = (0..8)
            .map(|value| f32::from(u8::try_from(value).expect("a small index")))
            .collect();
        let output = resampler.process(&input);
        assert_eq!(output, vec![0.0, 2.0, 4.0, 6.0]);
    }

    #[test]
    fn streaming_matches_a_single_block() {
        let input: Vec<f32> = (0..1_000_u16)
            .map(|index| (f32::from(index) * 0.05).sin() * 0.5)
            .collect();

        let mut single =
            LinearResampler::new(format(44_100, 1), format(48_000, 1)).expect("valid resampler");
        let expected = single.process(&input);

        let mut split =
            LinearResampler::new(format(44_100, 1), format(48_000, 1)).expect("valid resampler");
        let mut actual = split.process(&input[..417]);
        actual.extend(split.process(&input[417..]));

        assert_eq!(actual.len(), expected.len());
        for (index, (left, right)) in actual.iter().zip(&expected).enumerate() {
            assert!(
                (left - right).abs() < 1e-5,
                "sample {index}: {left} != {right}"
            );
        }
    }

    #[test]
    fn stereo_resampling_keeps_channels_aligned() {
        let mut resampler =
            LinearResampler::new(format(24_000, 2), format(48_000, 2)).expect("valid resampler");
        let output = resampler.process(&[1.0, -1.0, 0.0, 0.0]);
        assert_eq!(output, vec![1.0, -1.0, 0.5, -0.5, 0.0, 0.0]);
    }

    #[test]
    fn converter_applies_channel_mapping_and_timestamps() {
        let source = format(48_000, 1);
        let target = format(48_000, 2);
        let mut converter = PcmConverter::new(source, target).expect("valid converter");
        let input = buffer(source, &[0.25, 0.5]);

        let first = converter.push(&input).expect("conversion succeeds");
        assert_eq!(first.pts(), Duration::ZERO);
        assert_eq!(first.samples(), &[0.25, 0.25, 0.5, 0.5]);

        // The converter ignores the timestamp of a continuous buffer and uses
        // the output timeline, so leave this buffer at zero deliberately.
        let second_input = buffer(source, &[-0.25]);
        let second = converter.push(&second_input).expect("conversion succeeds");
        assert_eq!(second.pts(), target.duration_for_frames(2));
        assert_eq!(second.samples(), &[-0.25, -0.25]);
    }

    #[test]
    fn converter_rejects_a_mismatched_source_format() {
        let mut converter =
            PcmConverter::new(format(48_000, 1), format(48_000, 2)).expect("valid converter");
        let error = converter
            .push(&buffer(format(44_100, 1), &[0.0]))
            .expect_err("the format must match");
        assert!(matches!(error, Error::FormatMismatch { .. }));
    }

    #[test]
    fn reset_at_trims_samples_before_the_target() {
        let source = format(1_000, 1);
        let mut converter = PcmConverter::new(source, source).expect("valid converter");
        converter.reset_at(Duration::from_millis(5));
        let input = PcmBuffer::new(
            Duration::ZERO,
            source,
            (0..10)
                .map(|value| f32::from(u8::try_from(value).expect("a small index")))
                .collect(),
        )
        .expect("valid buffer");

        let output = converter.push(&input).expect("conversion succeeds");
        assert_eq!(output.pts(), Duration::from_millis(5));
        assert_eq!(output.samples(), &[5.0, 6.0, 7.0, 8.0, 9.0]);
    }

    #[test]
    fn reset_at_skips_blocks_that_end_before_the_target() {
        let source = format(1_000, 1);
        let mut converter = PcmConverter::new(source, source).expect("valid converter");
        converter.reset_at(Duration::from_millis(20));

        let early = PcmBuffer::new(Duration::ZERO, source, vec![1.0; 10]).expect("valid buffer");
        let output = converter.push(&early).expect("conversion succeeds");
        assert!(output.is_empty());
        assert_eq!(output.pts(), Duration::from_millis(20));

        let late =
            PcmBuffer::new(Duration::from_millis(20), source, vec![2.0; 4]).expect("valid buffer");
        let output = converter.push(&late).expect("conversion succeeds");
        assert_eq!(output.pts(), Duration::from_millis(20));
        assert_eq!(output.samples(), &[2.0; 4]);
    }

    #[test]
    fn reset_restarts_the_output_timeline() {
        let source = format(48_000, 1);
        let target = format(48_000, 1);
        let mut converter = PcmConverter::new(source, target).expect("valid converter");
        let input = buffer(source, &[1.0, 0.0]);
        converter.push(&input).expect("conversion succeeds");
        converter.reset();
        let next = converter.push(&input).expect("conversion succeeds");
        assert_eq!(next.pts(), Duration::ZERO);
    }
}
