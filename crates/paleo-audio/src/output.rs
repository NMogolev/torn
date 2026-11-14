//! Device audio output built on `cpal`.
//!
//! The output owns one SPSC ring buffer. The application pushes interleaved
//! `f32` frames from the decoding thread; the device callback owns the consumer
//! end, pops frames, and converts them to the host's sample format. The render
//! path takes no locks and allocates nothing: [`AudioOutput::reset`] publishes a
//! flush generation and an absolute sample marker, and the callback discards
//! every sample queued up to that marker on its next invocation.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{
    Device, FromSample, OutputCallbackInfo, SampleFormat, SizedSample, Stream, StreamConfig,
};
use ringbuf::traits::{Consumer, Observer, Producer, Split};
use ringbuf::{HeapCons, HeapProd, HeapRb};

use crate::buffer::{MAX_CHANNELS, PcmBuffer, PcmFormat};
use crate::error::{Error, Result};

/// Default amount of audio kept in the output ring.
pub const DEFAULT_LATENCY: Duration = Duration::from_millis(250);
/// Smallest ring capacity, in frames, even when a shorter latency is requested.
const MIN_RING_FRAMES: usize = 1_024;
/// Upper bound applied to a requested latency.
const MAX_LATENCY: Duration = Duration::from_secs(2);
/// Below this, scheduling jitter would underrun constantly; requests are raised.
const MIN_LATENCY: Duration = Duration::from_millis(20);
/// Samples converted per ring-buffer pop on the real-time path.
///
/// The callback converts into this fixed stack buffer instead of allocating a
/// scratch vector. It must hold at least one whole frame; `PcmFormat` limits
/// the channel count to [`MAX_CHANNELS`], so this always does.
const CHUNK_SAMPLES: usize = 256;
const _: () = assert!(CHUNK_SAMPLES >= MAX_CHANNELS);

/// A host output device that persisted as a stable id.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutputDeviceInfo {
    id: String,
    name: String,
}

impl OutputDeviceInfo {
    /// Returns the stable device id accepted by [`AudioOptions::with_device`].
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Returns the human-readable device name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
}

/// Options for opening an [`AudioOutput`].
#[derive(Clone, Debug)]
pub struct AudioOptions {
    device: Option<String>,
    latency: Duration,
}

impl Default for AudioOptions {
    fn default() -> Self {
        Self {
            device: None,
            latency: DEFAULT_LATENCY,
        }
    }
}

impl AudioOptions {
    /// Creates options for the default device and a 250 ms ring.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Selects a device by the id returned from [`AudioOutput::devices`].
    ///
    /// The id is stable across runs on the same host. When it is absent or the
    /// device disappeared, opening fails with [`Error::UnknownDevice`] instead
    /// of silently switching outputs.
    #[must_use]
    pub fn with_device(mut self, id: impl Into<String>) -> Self {
        self.device = Some(id.into());
        self
    }

    /// Sets the amount of audio buffered ahead of the device.
    ///
    /// The value is clamped to `20 ms..=2 s`. Larger values survive decoding
    /// stalls; smaller values reduce control latency. The actual capacity also
    /// has a floor of 1024 frames.
    #[must_use]
    pub fn with_latency(mut self, latency: Duration) -> Self {
        self.latency = latency;
        self
    }

    /// Returns the requested device id, or `None` for the default device.
    #[must_use]
    pub fn device(&self) -> Option<&str> {
        self.device.as_deref()
    }

    /// Returns the requested ring latency before clamping.
    #[must_use]
    pub const fn latency(&self) -> Duration {
        self.latency
    }
}

/// The outcome of an [`AudioOutput::queue`] call.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QueueReport {
    written_frames: usize,
    skipped_frames: usize,
}

impl QueueReport {
    /// Returns how many frames were copied into the ring.
    #[must_use]
    pub const fn written_frames(self) -> usize {
        self.written_frames
    }

    /// Returns how many frames were dropped because the ring was full.
    #[must_use]
    pub const fn skipped_frames(self) -> usize {
        self.skipped_frames
    }

    /// Returns whether the whole buffer was queued.
    #[must_use]
    pub const fn is_complete(self) -> bool {
        self.skipped_frames == 0
    }
}

/// State shared with the real-time callback.
///
/// The ring's producer stays with [`AudioOutput`]; its consumer lives inside
/// the callback. Reset is coordinated through the sample counters and the flush
/// marker below, so the callback never has to take a lock.
struct Shared {
    rendered_frames: AtomicU64,
    underrun_frames: AtomicU64,
    underruns: AtomicU64,
    volume: AtomicU32,
    muted: AtomicBool,
    paused: AtomicBool,
    error_count: AtomicU64,
    last_error: Mutex<Option<String>>,
    /// Samples successfully pushed into the ring over its lifetime.
    written_samples: AtomicU64,
    /// Samples the callback has popped over its lifetime, including discards.
    read_samples: AtomicU64,
    /// Incremented by [`AudioOutput::reset`] so the callback resets counters.
    flush_generation: AtomicU64,
    /// Absolute sample index at which the callback stops discarding.
    flush_until: AtomicU64,
}

impl Shared {
    fn new() -> Self {
        Self {
            rendered_frames: AtomicU64::new(0),
            underrun_frames: AtomicU64::new(0),
            underruns: AtomicU64::new(0),
            volume: AtomicU32::new(1.0_f32.to_bits()),
            muted: AtomicBool::new(false),
            paused: AtomicBool::new(true),
            error_count: AtomicU64::new(0),
            last_error: Mutex::new(None),
            written_samples: AtomicU64::new(0),
            read_samples: AtomicU64::new(0),
            flush_generation: AtomicU64::new(0),
            flush_until: AtomicU64::new(0),
        }
    }

    /// Returns how many samples are still waiting to be discarded after a reset.
    fn pending_flush_samples(&self) -> u64 {
        self.flush_until
            .load(Ordering::Acquire)
            .saturating_sub(self.read_samples.load(Ordering::Acquire))
    }
}

/// An open audio output device and its PCM queue.
///
/// The device format is chosen by the host's default output configuration.
/// Use [`PcmConverter`](crate::PcmConverter) to adapt decoded media to
/// [`Self::format`]. [`Self::position`] advances with the number of frames the
/// device has actually consumed, so it can drive a media clock.
pub struct AudioOutput {
    stream: Stream,
    producer: HeapProd<f32>,
    format: PcmFormat,
    capacity_frames: usize,
    shared: Arc<Shared>,
    device: OutputDeviceInfo,
    base_position: Duration,
    dropped_frames: u64,
}

impl AudioOutput {
    /// Opens the default output device with [`AudioOptions::default`].
    ///
    /// The stream starts paused; call [`Self::play`] when playback begins.
    ///
    /// # Errors
    ///
    /// Returns [`Error::NoOutputDevice`] when the host has no default output,
    /// and [`Error::Stream`] when the device cannot build an output stream.
    pub fn open() -> Result<Self> {
        Self::open_with(&AudioOptions::default())
    }

    /// Opens the selected output device with explicit options.
    ///
    /// # Errors
    ///
    /// Returns [`Error::UnknownDevice`] when a persisted device id is gone,
    /// [`Error::UnsupportedConfig`] when the device exposes no default output
    /// format, and [`Error::Stream`] when the stream cannot be built.
    pub fn open_with(options: &AudioOptions) -> Result<Self> {
        let host = cpal::default_host();
        let device = resolve_device(&host, options.device())?;
        let supported = device
            .default_output_config()
            .map_err(|error| Error::UnsupportedConfig(error.to_string()))?;
        let sample_format = supported.sample_format();
        let config = supported.config();
        let format = PcmFormat::new(config.sample_rate, usize::from(config.channels))?;

        let latency = if options.latency().is_zero() {
            DEFAULT_LATENCY
        } else {
            options.latency().clamp(MIN_LATENCY, MAX_LATENCY)
        };
        let requested_frames =
            usize::try_from(format.frames_for_duration(latency)).unwrap_or(usize::MAX);
        let capacity_frames = requested_frames.max(MIN_RING_FRAMES);
        let capacity_samples = capacity_frames
            .checked_mul(format.channels())
            .ok_or_else(|| Error::InvalidFormat("ring capacity overflows".to_owned()))?;

        let (producer, consumer) = HeapRb::<f32>::new(capacity_samples).split();
        let shared = Arc::new(Shared::new());
        let stream = build_stream(&device, &config, sample_format, &shared, consumer)?;
        let device = describe(&device)?;

        Ok(Self {
            stream,
            producer,
            format,
            capacity_frames,
            shared,
            device,
            base_position: Duration::ZERO,
            dropped_frames: 0,
        })
    }

    /// Returns every output device visible to the default host.
    ///
    /// Devices that cannot report a stable id are omitted.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Stream`] when the host cannot enumerate devices.
    pub fn devices() -> Result<Vec<OutputDeviceInfo>> {
        let host = cpal::default_host();
        let devices = host
            .output_devices()
            .map_err(|error| Error::Stream(error.to_string()))?;
        Ok(devices
            .filter_map(|device| describe(&device).ok())
            .collect())
    }

    /// Returns the host's default output device.
    ///
    /// # Errors
    ///
    /// Returns [`Error::NoOutputDevice`] when no default device exists.
    pub fn default_device() -> Result<OutputDeviceInfo> {
        let host = cpal::default_host();
        let device = host.default_output_device().ok_or(Error::NoOutputDevice)?;
        describe(&device)
    }

    /// Returns the device this output is connected to.
    #[must_use]
    pub const fn device(&self) -> &OutputDeviceInfo {
        &self.device
    }

    /// Returns the PCM format the device consumes.
    #[must_use]
    pub const fn format(&self) -> PcmFormat {
        self.format
    }

    /// Returns the ring capacity in frames.
    #[must_use]
    pub const fn capacity_frames(&self) -> usize {
        self.capacity_frames
    }

    /// Returns the ring capacity as media time.
    #[must_use]
    pub fn capacity(&self) -> Duration {
        self.format
            .duration_for_frames(u64::try_from(self.capacity_frames).unwrap_or(u64::MAX))
    }

    /// Returns how many frames are waiting in the ring.
    ///
    /// Samples that a pending [`Self::reset`] has logically discarded are not
    /// counted, even while the callback has not drained them yet.
    #[must_use]
    pub fn buffered_frames(&self) -> usize {
        let samples = u64::try_from(self.producer.occupied_len()).unwrap_or(u64::MAX);
        let buffered = samples.saturating_sub(self.shared.pending_flush_samples());
        usize::try_from(buffered).unwrap_or(usize::MAX) / self.format.channels()
    }

    /// Returns how much audio is waiting in the ring.
    #[must_use]
    pub fn buffered(&self) -> Duration {
        self.format
            .duration_for_frames(u64::try_from(self.buffered_frames()).unwrap_or(u64::MAX))
    }

    /// Starts the device clock.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Stream`] when the host cannot start the stream. The
    /// output stays paused in that case.
    pub fn play(&mut self) -> Result<()> {
        if !self.is_playing() {
            self.stream
                .play()
                .map_err(|error| Error::Stream(error.to_string()))?;
            self.shared.paused.store(false, Ordering::Release);
        }
        Ok(())
    }

    /// Stops the device clock.
    ///
    /// Some backends do not support hardware suspend. When pausing fails the
    /// stream keeps running but the callback writes silence and the reported
    /// position stops advancing, so the media timeline remains consistent.
    pub fn pause(&mut self) {
        self.shared.paused.store(true, Ordering::Release);
        let _ = self.stream.pause();
    }

    /// Returns whether the device clock is running.
    #[must_use]
    pub fn is_playing(&self) -> bool {
        !self.shared.paused.load(Ordering::Acquire)
    }

    /// Returns the position the device has played since the last reset.
    ///
    /// The position is `base + rendered_frames / sample_rate`. Underruns count
    /// as rendered time because the device still consumed the clock; inspect
    /// [`Self::underrun_frames`] to tell the two apart.
    #[must_use]
    pub fn position(&self) -> Duration {
        position_from(
            self.base_position,
            self.format,
            self.shared.rendered_frames.load(Ordering::Relaxed),
        )
    }

    /// Sets the media position that corresponds to the next device frame.
    ///
    /// Call this after a seek, together with [`Self::reset`], so the audio clock
    /// and the media clock restart at the same timestamp.
    pub fn set_base_position(&mut self, position: Duration) {
        self.base_position = position;
    }

    /// Returns the number of frames the device consumed since the last reset,
    /// including frames of silence written after an underrun.
    #[must_use]
    pub fn rendered_frames(&self) -> u64 {
        self.shared.rendered_frames.load(Ordering::Relaxed)
    }

    /// Returns the number of unterminated frames the device had to fill with
    /// silence since the last reset.
    #[must_use]
    pub fn underrun_frames(&self) -> u64 {
        self.shared.underrun_frames.load(Ordering::Relaxed)
    }

    /// Returns how many callbacks detected an underrun since the last reset.
    #[must_use]
    pub fn underruns(&self) -> u64 {
        self.shared.underruns.load(Ordering::Relaxed)
    }

    /// Returns how many frames were dropped because the ring was full.
    #[must_use]
    pub const fn dropped_frames(&self) -> u64 {
        self.dropped_frames
    }

    /// Returns how many stream errors the device reported.
    #[must_use]
    pub fn error_count(&self) -> u64 {
        self.shared.error_count.load(Ordering::Relaxed)
    }

    /// Returns the last stream error reported by the host, if any.
    #[must_use]
    pub fn last_error(&self) -> Option<String> {
        self.shared
            .last_error
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Discards queued audio, resets diagnostics, and anchors the device clock.
    ///
    /// This never touches the real-time thread's consumer. It records how many
    /// samples were queued and bumps a flush generation; the callback discards
    /// everything up to that marker before outputting again. While the stream is
    /// paused the discard is deferred until playback resumes, and
    /// [`Self::buffered_frames`] already excludes the stale samples.
    pub fn reset(&mut self, position: Duration) {
        let written = self.shared.written_samples.load(Ordering::Acquire);
        self.shared.flush_until.store(written, Ordering::Release);
        self.shared.rendered_frames.store(0, Ordering::Relaxed);
        self.shared.underrun_frames.store(0, Ordering::Relaxed);
        self.shared.underruns.store(0, Ordering::Relaxed);
        self.shared.flush_generation.fetch_add(1, Ordering::AcqRel);
        self.base_position = position;
        self.dropped_frames = 0;
    }

    /// Queues interleaved audio at the device format.
    ///
    /// The ring is bounded, so a full ring drops the remainder instead of
    /// blocking the decoding thread. Check [`QueueReport::is_complete`] or keep
    /// [`Self::buffered`] below [`Self::capacity`] to avoid drops.
    ///
    /// # Errors
    ///
    /// Returns [`Error::FormatMismatch`] when the buffer is not in
    /// [`Self::format`].
    pub fn queue(&mut self, buffer: &PcmBuffer) -> Result<QueueReport> {
        if buffer.format() != self.format {
            return Err(Error::FormatMismatch {
                expected: self.format.to_string(),
                actual: buffer.format().to_string(),
            });
        }

        let samples = buffer.samples();
        let mut written = 0;
        while written < samples.len() {
            let count = self.producer.push_slice(&samples[written..]);
            if count == 0 {
                break;
            }
            written += count;
        }

        let channels = self.format.channels();
        self.shared.written_samples.fetch_add(
            u64::try_from(written).unwrap_or(u64::MAX),
            Ordering::Relaxed,
        );
        let written_frames = written / channels;
        let skipped_frames = (samples.len() - written) / channels;
        self.dropped_frames = self
            .dropped_frames
            .saturating_add(u64::try_from(skipped_frames).unwrap_or(u64::MAX));
        Ok(QueueReport {
            written_frames,
            skipped_frames,
        })
    }

    /// Sets the linear volume applied by the callback, clamped to `0.0..=1.0`.
    pub fn set_volume(&mut self, volume: f32) {
        let volume = if volume.is_finite() {
            volume.clamp(0.0, 1.0)
        } else {
            1.0
        };
        self.shared
            .volume
            .store(volume.to_bits(), Ordering::Relaxed);
    }

    /// Returns the linear volume applied by the callback.
    #[must_use]
    pub fn volume(&self) -> f32 {
        f32::from_bits(self.shared.volume.load(Ordering::Relaxed))
    }

    /// Mutes or unmutes the output without changing the stored volume.
    pub fn set_muted(&mut self, muted: bool) {
        self.shared.muted.store(muted, Ordering::Relaxed);
    }

    /// Returns whether the output is muted.
    #[must_use]
    pub fn is_muted(&self) -> bool {
        self.shared.muted.load(Ordering::Relaxed)
    }
}

impl Drop for AudioOutput {
    fn drop(&mut self) {
        self.shared.paused.store(true, Ordering::Release);
        let _ = self.stream.pause();
    }
}

fn position_from(base: Duration, format: PcmFormat, rendered_frames: u64) -> Duration {
    base.saturating_add(format.duration_for_frames(rendered_frames))
}

fn resolve_device(host: &cpal::Host, id: Option<&str>) -> Result<Device> {
    if let Some(id) = id {
        let parsed = id
            .parse::<cpal::DeviceId>()
            .map_err(|_| Error::UnknownDevice(id.to_owned()))?;
        return host
            .device_by_id(&parsed)
            .ok_or_else(|| Error::UnknownDevice(id.to_owned()));
    }
    host.default_output_device().ok_or(Error::NoOutputDevice)
}

fn describe(device: &Device) -> Result<OutputDeviceInfo> {
    let id = device
        .id()
        .map_err(|error| Error::Stream(error.to_string()))?
        .to_string();
    let name = device
        .description()
        .map_or_else(|_| id.clone(), |description| description.name().to_owned());
    Ok(OutputDeviceInfo { id, name })
}

fn build_stream(
    device: &Device,
    config: &StreamConfig,
    sample_format: SampleFormat,
    shared: &Arc<Shared>,
    consumer: HeapCons<f32>,
) -> Result<Stream> {
    match sample_format {
        SampleFormat::I8 => build::<i8>(device, config, shared, consumer),
        SampleFormat::I16 => build::<i16>(device, config, shared, consumer),
        SampleFormat::I24 => build::<cpal::I24>(device, config, shared, consumer),
        SampleFormat::I32 => build::<i32>(device, config, shared, consumer),
        SampleFormat::I64 => build::<i64>(device, config, shared, consumer),
        SampleFormat::U8 => build::<u8>(device, config, shared, consumer),
        SampleFormat::U16 => build::<u16>(device, config, shared, consumer),
        SampleFormat::U24 => build::<cpal::U24>(device, config, shared, consumer),
        SampleFormat::U32 => build::<u32>(device, config, shared, consumer),
        SampleFormat::U64 => build::<u64>(device, config, shared, consumer),
        SampleFormat::F32 => build::<f32>(device, config, shared, consumer),
        SampleFormat::F64 => build::<f64>(device, config, shared, consumer),
        other => Err(Error::UnsupportedConfig(format!(
            "device sample format {other} is not supported"
        ))),
    }
}

fn build<T>(
    device: &Device,
    config: &StreamConfig,
    shared: &Arc<Shared>,
    consumer: HeapCons<f32>,
) -> Result<Stream>
where
    T: SizedSample + FromSample<f32>,
{
    let channels = usize::from(config.channels);
    let callback_shared = Arc::clone(shared);
    let error_shared = Arc::clone(shared);
    let mut state = CallbackState::new(consumer);
    device
        .build_output_stream(
            *config,
            move |output: &mut [T], _: &OutputCallbackInfo| {
                fill_output(output, channels, &callback_shared, &mut state);
            },
            move |error| record_error(&error_shared, &error),
            None,
        )
        .map_err(|error| Error::Stream(error.to_string()))
}

/// Real-time state owned exclusively by the device callback.
struct CallbackState {
    consumer: HeapCons<f32>,
    /// Fixed conversion buffer; populating it never allocates.
    chunk: [f32; CHUNK_SAMPLES],
    /// Absolute count of samples this callback has popped.
    read_samples: u64,
    /// Last flush generation observed from [`Shared`].
    generation: u64,
}

impl CallbackState {
    fn new(consumer: HeapCons<f32>) -> Self {
        Self {
            consumer,
            chunk: [0.0; CHUNK_SAMPLES],
            read_samples: 0,
            generation: 0,
        }
    }

    /// Drops queued samples up to the pending reset marker and clears counters.
    fn flush(&mut self, shared: &Shared) {
        let target = shared.flush_until.load(Ordering::Acquire);
        while self.read_samples < target {
            // Stop exactly at the marker: audio queued after a reset may already
            // be in the ring and must not be discarded with the stale prefix.
            let remaining = target - self.read_samples;
            let take = usize::try_from(remaining)
                .unwrap_or(usize::MAX)
                .min(CHUNK_SAMPLES);
            let count = self.consumer.pop_slice(&mut self.chunk[..take]);
            if count == 0 {
                break;
            }
            self.read_samples = self
                .read_samples
                .saturating_add(u64::try_from(count).unwrap_or(u64::MAX));
        }
        shared
            .read_samples
            .store(self.read_samples, Ordering::Relaxed);
        shared.rendered_frames.store(0, Ordering::Relaxed);
        shared.underrun_frames.store(0, Ordering::Relaxed);
        shared.underruns.store(0, Ordering::Relaxed);
    }
}

fn fill_output<T>(output: &mut [T], channels: usize, shared: &Shared, state: &mut CallbackState)
where
    T: SizedSample + FromSample<f32>,
{
    let silence = T::from_sample(0.0_f32);
    if channels == 0 {
        for sample in output.iter_mut() {
            *sample = silence;
        }
        return;
    }

    let frames = output.len() / channels;
    if shared.paused.load(Ordering::Acquire) {
        for sample in output.iter_mut() {
            *sample = silence;
        }
        return;
    }

    let volume = f32::from_bits(shared.volume.load(Ordering::Relaxed));
    let gain = if shared.muted.load(Ordering::Relaxed) {
        0.0
    } else {
        volume
    };

    let wanted = frames * channels;
    if wanted == 0 {
        return;
    }

    let generation = shared.flush_generation.load(Ordering::Acquire);
    if generation != state.generation {
        state.generation = generation;
        state.flush(shared);
    }

    // Pop and convert in whole-frame chunks that fit the fixed buffer. This
    // keeps the ring at a frame boundary even when it runs dry mid-callback.
    let chunk = (CHUNK_SAMPLES / channels) * channels;
    let mut converted = 0;
    while converted < wanted {
        let take = chunk.min(wanted - converted);
        let count = state.consumer.pop_slice(&mut state.chunk[..take]);
        if count == 0 {
            break;
        }
        for (sample, value) in output[converted..converted + count]
            .iter_mut()
            .zip(&state.chunk[..count])
        {
            *sample = T::from_sample((value * gain).clamp(-1.0, 1.0));
        }
        converted += count;
    }
    state.read_samples = state
        .read_samples
        .saturating_add(u64::try_from(converted).unwrap_or(u64::MAX));
    shared
        .read_samples
        .store(state.read_samples, Ordering::Relaxed);

    for sample in &mut output[converted..] {
        *sample = silence;
    }

    let complete = converted / channels;
    count_frames(shared, frames, frames.saturating_sub(complete));
}

fn count_frames(shared: &Shared, rendered: usize, underrun: usize) {
    shared.rendered_frames.fetch_add(
        u64::try_from(rendered).unwrap_or(u64::MAX),
        Ordering::Relaxed,
    );
    if underrun > 0 {
        shared.underrun_frames.fetch_add(
            u64::try_from(underrun).unwrap_or(u64::MAX),
            Ordering::Relaxed,
        );
        shared.underruns.fetch_add(1, Ordering::Relaxed);
    }
}

fn record_error(shared: &Shared, error: &cpal::Error) {
    shared.error_count.fetch_add(1, Ordering::Relaxed);
    if let Ok(mut slot) = shared.last_error.try_lock() {
        *slot = Some(error.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn position_is_the_base_plus_rendered_time() {
        let format = PcmFormat::new(48_000, 2).expect("valid format");
        assert_eq!(
            position_from(Duration::from_secs(3), format, 24_000),
            Duration::from_millis(3_500)
        );
        assert_eq!(
            position_from(Duration::from_secs(3), format, 48_000),
            Duration::from_secs(4)
        );
    }

    #[test]
    fn latency_is_clamped_when_the_ring_is_built() {
        let options = AudioOptions::new()
            .with_device("test-device")
            .with_latency(Duration::from_millis(1));
        assert_eq!(options.device(), Some("test-device"));
        assert_eq!(options.latency(), Duration::from_millis(1));

        let options = AudioOptions::new().with_latency(MAX_LATENCY + Duration::from_secs(1));
        assert_eq!(options.latency(), MAX_LATENCY + Duration::from_secs(1));
    }

    #[test]
    fn reset_marker_discards_only_audio_queued_before_it() {
        let (mut producer, consumer) = HeapRb::<f32>::new(64).split();
        let shared = Shared::new();
        let mut state = CallbackState::new(consumer);

        // Audio queued before the reset is stale; audio queued after it must play.
        assert_eq!(producer.push_slice(&[0.25; 32]), 32);
        shared.written_samples.store(32, Ordering::Release);
        shared.flush_until.store(32, Ordering::Release);
        shared.rendered_frames.store(7, Ordering::Relaxed);
        shared.flush_generation.fetch_add(1, Ordering::AcqRel);

        assert_eq!(producer.push_slice(&[0.5; 8]), 8);
        shared.written_samples.store(40, Ordering::Release);

        assert_eq!(shared.pending_flush_samples(), 32);

        state.flush(&shared);

        assert_eq!(state.read_samples, 32);
        assert_eq!(shared.pending_flush_samples(), 0);
        assert_eq!(shared.rendered_frames.load(Ordering::Relaxed), 0);
        assert_eq!(producer.occupied_len(), 8);

        let mut played = [0.0_f32; 8];
        assert_eq!(state.consumer.pop_slice(&mut played), 8);
        assert!(
            played
                .iter()
                .all(|sample| (*sample - 0.5).abs() <= f32::EPSILON)
        );
    }

    #[test]
    fn a_second_reset_extends_the_flush_marker() {
        let (mut producer, consumer) = HeapRb::<f32>::new(64).split();
        let shared = Shared::new();
        let mut state = CallbackState::new(consumer);

        assert_eq!(producer.push_slice(&[0.25; 16]), 16);
        shared.written_samples.store(16, Ordering::Release);
        shared.flush_until.store(16, Ordering::Release);

        assert_eq!(producer.push_slice(&[0.5; 16]), 16);
        shared.written_samples.store(32, Ordering::Release);
        shared.flush_until.store(32, Ordering::Release);

        state.flush(&shared);

        assert_eq!(state.read_samples, 32);
        assert_eq!(producer.occupied_len(), 0);
    }

    #[test]
    fn callback_streams_long_buffers_in_whole_frame_chunks() {
        let (mut producer, consumer) = HeapRb::<f32>::new(1_024).split();
        let shared = Shared::new();
        shared.paused.store(false, Ordering::Release);
        let mut state = CallbackState::new(consumer);

        let input: Vec<f32> = (0..600_u16).map(|index| f32::from(index) * 0.001).collect();
        assert_eq!(producer.push_slice(&input), input.len());

        let mut output = [0.0_f32; 600];
        fill_output(&mut output, 1, &shared, &mut state);

        for (sample, expected) in output.iter().zip(&input) {
            assert!((sample - expected).abs() <= f32::EPSILON);
        }
        assert_eq!(state.read_samples, 600);
        assert_eq!(shared.rendered_frames.load(Ordering::Relaxed), 600);
        assert_eq!(shared.underrun_frames.load(Ordering::Relaxed), 0);

        // A short ring fills the rest with silence and counts the missing frames.
        assert_eq!(producer.push_slice(&[0.5; 300]), 300);
        let mut output = [1.0_f32; 600];
        fill_output(&mut output, 1, &shared, &mut state);

        assert!((output[299] - 0.5).abs() <= f32::EPSILON);
        assert!((output[300] - 0.0).abs() <= f32::EPSILON);
        assert_eq!(shared.underrun_frames.load(Ordering::Relaxed), 300);
        assert_eq!(shared.underruns.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn real_device_smoke_test_is_opt_in() {
        if std::env::var_os("TORN_AUDIO_DEVICE_TEST").is_none() {
            return;
        }

        let mut output = AudioOutput::open().expect("a default output device");
        assert!(output.capacity_frames() >= MIN_RING_FRAMES);
        output.set_volume(0.5);
        assert!((output.volume() - 0.5).abs() < f32::EPSILON);
        output.play().expect("the stream starts");
        let silence = PcmBuffer::silent(
            Duration::ZERO,
            output.format(),
            usize::try_from(
                output
                    .format()
                    .frames_for_duration(Duration::from_millis(120)),
            )
            .expect("a small frame count"),
        )
        .expect("a silent buffer");
        let report = output
            .queue(&silence)
            .expect("the buffer matches the device");
        assert!(report.written_frames() > 0);
        std::thread::sleep(Duration::from_millis(100));
        output.pause();
        assert!(output.position() > Duration::ZERO);
    }
}
