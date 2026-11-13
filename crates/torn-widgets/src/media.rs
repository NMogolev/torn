//! Backend-neutral media timing, frame scheduling, and session contracts.
//!
//! The types in this module deliberately keep decoding, presentation, and
//! playback policy separate:
//!
//! - [`MediaClock`] owns the playback timeline. A decoder-only session advances
//!   it from frame timestamps, while a session with audio can treat the audio
//!   output position as the master clock through [`MediaClock::synchronize`].
//! - [`MediaFrameScheduler`] selects the frame that should be visible for the
//!   current presentation time and discards frames that are already stale.
//! - [`MediaSession`] is the transport and decode contract. Widgets such as
//!   `Video` stay views and controls over a session.
//!
//! This is intentionally smaller than a full media runtime: buffering policy,
//! network sources, subtitle tracks, and hardware decode backends can implement
//! the same contract without changing widget code.

use std::collections::VecDeque;
use std::fmt;
use std::time::Duration;

use torn_render::RasterImage;

const MIN_RATE: f64 = 0.05;
const MAX_RATE: f64 = 16.0;
const MAX_PENDING_FRAMES: usize = 4;

/// Returns `value` reduced modulo `modulus`, or `value` when `modulus` is zero.
pub(crate) fn duration_remainder(value: Duration, modulus: Duration) -> Duration {
    if modulus.is_zero() {
        return value;
    }
    let nanos = value.as_nanos() % modulus.as_nanos();
    let seconds = u64::try_from(nanos / 1_000_000_000).unwrap_or(u64::MAX);
    let subsecond = u32::try_from(nanos % 1_000_000_000).unwrap_or(0);
    Duration::new(seconds, subsecond)
}

/// Playback state exposed by a [`MediaClock`].
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum MediaState {
    /// No media has been opened yet.
    #[default]
    Idle,
    /// The session is filling its buffer before playback can continue.
    Buffering,
    /// The clock is advancing.
    Playing,
    /// The position is held at its current value.
    Paused,
    /// The position reached the end of the media.
    Ended,
    /// The session stopped because of an error.
    Failed,
}

/// The shared playback timeline for a media session.
///
/// The clock is a value type: the runtime advances it with wall-clock deltas
/// when no audio device is available, or synchronizes it from the audio output
/// position once audio becomes the master clock. Both paths use the same
/// presentation timestamps, so video scheduling and controls stay consistent.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MediaClock {
    position: Duration,
    duration: Option<Duration>,
    rate: f64,
    state: MediaState,
}

impl Default for MediaClock {
    fn default() -> Self {
        Self {
            position: Duration::ZERO,
            duration: None,
            rate: 1.0,
            state: MediaState::Idle,
        }
    }
}

impl MediaClock {
    /// Creates an idle clock at position zero with rate `1.0`.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the current presentation position.
    #[must_use]
    pub const fn position(&self) -> Duration {
        self.position
    }

    /// Returns the media duration, or `None` when it is unknown.
    #[must_use]
    pub const fn duration(&self) -> Option<Duration> {
        self.duration
    }

    /// Returns the playback rate.
    #[must_use]
    pub const fn rate(&self) -> f64 {
        self.rate
    }

    /// Returns the current playback state.
    #[must_use]
    pub const fn state(&self) -> MediaState {
        self.state
    }

    /// Returns whether the clock is advancing.
    #[must_use]
    pub const fn is_playing(&self) -> bool {
        matches!(self.state, MediaState::Playing)
    }

    /// Returns whether playback reached the end of the media.
    #[must_use]
    pub const fn is_ended(&self) -> bool {
        matches!(self.state, MediaState::Ended)
    }

    /// Sets the media duration. Zero means an unknown duration.
    pub fn set_duration(&mut self, duration: Option<Duration>) -> bool {
        let duration = duration.filter(|value| !value.is_zero());
        let changed = self.duration != duration;
        self.duration = duration;
        if let Some(duration) = duration
            && self.position > duration
        {
            self.position = duration;
        }
        changed
    }

    /// Sets the playback rate, clamping it to a supported range.
    ///
    /// Non-finite values fall back to `1.0`.
    pub fn set_rate(&mut self, rate: f64) -> bool {
        let rate = if rate.is_finite() {
            rate.clamp(MIN_RATE, MAX_RATE)
        } else {
            1.0
        };
        if (self.rate - rate).abs() <= f64::EPSILON {
            return false;
        }
        self.rate = rate;
        true
    }

    /// Starts or pauses the clock without resetting its position.
    ///
    /// Returns whether the state changed. An ended clock must be seeked before
    /// it can play again.
    pub fn set_playing(&mut self, playing: bool) -> bool {
        if playing {
            if matches!(
                self.state,
                MediaState::Idle | MediaState::Paused | MediaState::Buffering
            ) {
                self.state = MediaState::Playing;
                return true;
            }
            return false;
        }
        if matches!(self.state, MediaState::Playing | MediaState::Buffering) {
            self.state = MediaState::Paused;
            return true;
        }
        false
    }

    /// Starts the clock without resetting its position.
    pub fn play(&mut self) -> bool {
        self.set_playing(true)
    }

    /// Pauses the clock without resetting its position.
    pub fn pause(&mut self) -> bool {
        self.set_playing(false)
    }

    /// Moves the clock to `position`, clamped to the known duration.
    ///
    /// Seeking an idle, ended, or failed clock leaves it paused at the new
    /// position. Seeking while playing keeps playing.
    pub fn seek(&mut self, position: Duration) -> bool {
        let position = self.clamp(position);
        let changed = position != self.position;
        self.position = position;
        if matches!(
            self.state,
            MediaState::Idle | MediaState::Ended | MediaState::Failed
        ) {
            self.state = MediaState::Paused;
            return true;
        }
        changed
    }

    /// Updates the position from an audio output device.
    ///
    /// Audio output is the master clock whenever it is available, so this
    /// method does not change the playback state or apply the rate twice.
    pub fn synchronize(&mut self, position: Duration) -> bool {
        let position = self.clamp(position);
        if position == self.position {
            return false;
        }
        self.position = position;
        true
    }

    /// Advances the clock by a wall-clock delta when no audio clock exists.
    ///
    /// Returns whether the position or state changed.
    pub fn advance(&mut self, delta: Duration) -> bool {
        self.advance_by(delta, false)
    }

    /// Advances and wraps the clock at the end of the media.
    ///
    /// This is useful for looping playback where the caller wants the overshoot
    /// preserved instead of restarted at zero.
    pub fn advance_looping(&mut self, delta: Duration) -> bool {
        self.advance_by(delta, true)
    }

    fn advance_by(&mut self, delta: Duration, looping: bool) -> bool {
        if self.state != MediaState::Playing || delta.is_zero() {
            return false;
        }
        let previous = self.position;
        self.position = self.position.saturating_add(delta.mul_f64(self.rate));
        if let Some(duration) = self.duration
            && !duration.is_zero()
            && self.position >= duration
        {
            if looping {
                self.position = duration_remainder(self.position, duration);
            } else {
                self.position = duration;
                self.state = MediaState::Ended;
            }
            return true;
        }
        self.position != previous
    }

    /// Marks the clock as ended at the known duration.
    pub fn finish(&mut self) -> bool {
        let position = self.duration.unwrap_or(self.position);
        let changed = self.state != MediaState::Ended || position != self.position;
        self.position = position;
        self.state = MediaState::Ended;
        changed
    }

    /// Marks the clock as failed and holds its position.
    pub fn fail(&mut self) -> bool {
        if self.state == MediaState::Failed {
            return false;
        }
        self.state = MediaState::Failed;
        true
    }

    /// Returns to an idle clock at position zero.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    fn clamp(&self, position: Duration) -> Duration {
        match self.duration {
            Some(duration) => position.min(duration),
            None => position,
        }
    }
}

/// Metadata discovered before a media session starts decoding.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MediaMetadata {
    width: u32,
    height: u32,
    duration: Duration,
}

impl MediaMetadata {
    /// Creates metadata for a video stream.
    #[must_use]
    pub const fn new(width: u32, height: u32, duration: Duration) -> Self {
        Self {
            width,
            height,
            duration,
        }
    }

    /// Returns the source video width in pixels.
    #[must_use]
    pub const fn width(&self) -> u32 {
        self.width
    }

    /// Returns the source video height in pixels.
    #[must_use]
    pub const fn height(&self) -> u32 {
        self.height
    }

    /// Returns the container duration, or zero when it is unknown.
    #[must_use]
    pub const fn duration(&self) -> Duration {
        self.duration
    }
}

/// Compatibility alias for the metadata type exposed by `Video`.
pub type VideoMetadata = MediaMetadata;

/// A decoded video frame with its presentation timestamp.
#[derive(Clone, Debug, PartialEq)]
pub struct MediaFrame {
    image: RasterImage,
    pts: Duration,
    duration: Duration,
}

impl MediaFrame {
    /// Creates a frame that becomes visible at `pts` and lasts for `duration`.
    #[must_use]
    pub fn new(image: RasterImage, pts: Duration, duration: Duration) -> Self {
        Self {
            image,
            pts,
            duration,
        }
    }

    /// Returns the decoded RGBA image.
    #[must_use]
    pub const fn image(&self) -> &RasterImage {
        &self.image
    }

    /// Returns the presentation timestamp.
    #[must_use]
    pub const fn pts(&self) -> Duration {
        self.pts
    }

    /// Returns how long the frame stays visible.
    #[must_use]
    pub const fn duration(&self) -> Duration {
        self.duration
    }

    /// Returns the first timestamp at which this frame is no longer current.
    #[must_use]
    pub fn end(&self) -> Duration {
        self.pts.saturating_add(self.duration)
    }
}

/// Selects the frame to display for the current presentation time.
///
/// The scheduler keeps a small bounded queue, drops frames that have already
/// expired, and keeps the most recent frame whose timestamp has been reached.
/// Between frames the previous image remains visible.
#[derive(Debug, Default)]
pub struct MediaFrameScheduler {
    current: Option<MediaFrame>,
    pending: VecDeque<MediaFrame>,
    position: Duration,
}

impl MediaFrameScheduler {
    /// Creates an empty scheduler at position zero.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Clears the visible and pending frames and moves to `position`.
    pub fn reset(&mut self, position: Duration) {
        self.current = None;
        self.pending.clear();
        self.position = position;
    }

    /// Clears every frame while keeping the current position.
    pub fn clear(&mut self) {
        self.current = None;
        self.pending.clear();
    }

    /// Drops pending frames and moves to `position`, keeping the visible frame.
    ///
    /// A view calls this after a seek so frames queued before the seek cannot
    /// be presented later, while the last decoded image remains on screen until
    /// a new frame arrives.
    pub fn seek(&mut self, position: Duration) {
        self.pending.clear();
        self.position = position;
    }

    /// Replaces the visible frame immediately.
    pub fn replace(&mut self, frame: MediaFrame) {
        self.position = frame.pts();
        self.pending.clear();
        self.current = Some(frame);
    }

    /// Queues a decoded frame.
    ///
    /// Returns `false` for frames that are already stale or out of order.
    pub fn push(&mut self, frame: MediaFrame) -> bool {
        if frame.end() <= self.position {
            return false;
        }
        if self
            .pending
            .back()
            .is_some_and(|last| last.pts() >= frame.pts())
        {
            return false;
        }
        self.pending.push_back(frame);
        while self.pending.len() > MAX_PENDING_FRAMES {
            self.pending.pop_front();
        }
        true
    }

    /// Selects the frame visible at `position`.
    ///
    /// Returns whether the visible frame changed. If no frame has started yet,
    /// the earliest decoded frame is shown immediately so previews appear
    /// without waiting for the first presentation timestamp.
    pub fn present(&mut self, position: Duration) -> bool {
        self.position = position;
        while self
            .pending
            .front()
            .is_some_and(|frame| frame.end() <= position)
        {
            self.pending.pop_front();
        }
        let mut chosen = None;
        while let Some(front) = self.pending.front() {
            if front.pts() <= position {
                chosen = self.pending.pop_front();
            } else {
                break;
            }
        }
        if let Some(frame) = chosen {
            let changed = self
                .current
                .as_ref()
                .is_none_or(|current| current.pts() != frame.pts());
            self.current = Some(frame);
            return changed;
        }
        if self.current.is_none() && !self.pending.is_empty() {
            self.current = self.pending.pop_front();
            return true;
        }
        false
    }

    /// Returns the frame that should currently be visible.
    #[must_use]
    pub const fn current(&self) -> Option<&MediaFrame> {
        self.current.as_ref()
    }

    /// Returns whether a decoded frame is waiting to be presented.
    #[must_use]
    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }
}

/// A runtime media failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MediaError {
    message: String,
}

impl MediaError {
    /// Creates an error with a human-readable message.
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    /// Returns the human-readable message.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for MediaError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for MediaError {}

/// One update produced by a [`MediaSession`].
#[derive(Clone, Debug, Default)]
pub struct MediaUpdate {
    /// The newest decoded frame, if one arrived.
    pub frame: Option<MediaFrame>,
    /// A fatal session error, if one occurred.
    pub error: Option<MediaError>,
    /// Whether the clock position or state changed.
    pub state_changed: bool,
}

impl MediaUpdate {
    /// Returns whether this update changes anything the view needs to paint.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.frame.is_none() && self.error.is_none() && !self.state_changed
    }
}

/// Decode and playback contract owned by a media view.
///
/// Implementations own their decoder, buffering, and clock. The view forwards
/// transport commands and calls [`Self::update`] once per frame. A session with
/// audio output should synchronize its [`MediaClock`] from the audio device in
/// `update` instead of advancing it with the wall-clock delta.
pub trait MediaSession {
    /// Returns the metadata discovered when the session was opened.
    fn metadata(&self) -> Option<&MediaMetadata>;

    /// Returns the playback timeline.
    fn clock(&self) -> &MediaClock;

    /// Returns the mutable playback timeline.
    fn clock_mut(&mut self) -> &mut MediaClock;

    /// Starts or resumes playback.
    ///
    /// # Errors
    ///
    /// Returns [`MediaError`] when the session cannot start decoding.
    fn play(&mut self) -> Result<(), MediaError>;

    /// Pauses playback while keeping the current frame and position.
    fn pause(&mut self);

    /// Seeks to `position`, preserving the current playing state.
    ///
    /// # Errors
    ///
    /// Returns [`MediaError`] when the session cannot restart at `position`.
    fn seek(&mut self, position: Duration) -> Result<(), MediaError>;

    /// Changes the playback rate.
    fn set_rate(&mut self, rate: f64);

    /// Advances playback and returns frames, errors, or state changes.
    fn update(&mut self, delta: Duration) -> MediaUpdate;

    /// Returns whether the session is decoding, buffering, or holding a frame
    /// that still needs to be presented.
    fn is_active(&self) -> bool;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(pts_ms: u64, duration_ms: u64) -> MediaFrame {
        MediaFrame::new(
            RasterImage::new(1, 1, vec![0, 0, 0, 255]).expect("valid image"),
            Duration::from_millis(pts_ms),
            Duration::from_millis(duration_ms),
        )
    }

    #[test]
    fn clock_advances_at_its_rate_and_stops_at_the_end() {
        let mut clock = MediaClock::new();
        clock.set_duration(Some(Duration::from_secs(1)));
        clock.set_rate(2.0);
        clock.play();

        assert!(clock.advance(Duration::from_millis(200)));
        assert_eq!(clock.position(), Duration::from_millis(400));
        assert!(clock.advance(Duration::from_millis(400)));
        assert_eq!(clock.position(), Duration::from_secs(1));
        assert!(clock.is_ended());
        assert!(!clock.is_playing());
    }

    #[test]
    fn clock_looping_preserves_the_overshoot() {
        let mut clock = MediaClock::new();
        clock.set_duration(Some(Duration::from_secs(1)));
        clock.play();

        clock.advance_looping(Duration::from_millis(1250));
        assert_eq!(clock.position(), Duration::from_millis(250));
        assert!(clock.is_playing());
    }

    #[test]
    fn seeking_clamps_and_leaves_ended_playback_paused() {
        let mut clock = MediaClock::new();
        clock.set_duration(Some(Duration::from_secs(2)));
        clock.play();
        clock.finish();

        assert!(clock.seek(Duration::from_secs(9)));
        assert_eq!(clock.position(), Duration::from_secs(2));
        assert_eq!(clock.state(), MediaState::Paused);
    }

    #[test]
    fn an_audio_clock_can_override_the_position() {
        let mut clock = MediaClock::new();
        clock.set_duration(Some(Duration::from_secs(3)));
        clock.play();

        assert!(clock.synchronize(Duration::from_millis(750)));
        assert_eq!(clock.position(), Duration::from_millis(750));
        assert!(clock.is_playing());
    }

    #[test]
    fn scheduler_presents_the_latest_frame_at_the_current_position() {
        let mut scheduler = MediaFrameScheduler::new();
        assert!(scheduler.push(frame(0, 100)));
        assert!(scheduler.push(frame(100, 100)));
        assert!(scheduler.push(frame(200, 100)));

        assert!(scheduler.present(Duration::ZERO));
        assert_eq!(
            scheduler.current().expect("frame zero").pts(),
            Duration::ZERO
        );
        assert!(scheduler.present(Duration::from_millis(150)));
        assert_eq!(
            scheduler.current().expect("frame one").pts(),
            Duration::from_millis(100)
        );
        assert!(scheduler.present(Duration::from_millis(250)));
        assert_eq!(
            scheduler.current().expect("frame two").pts(),
            Duration::from_millis(200)
        );
    }

    #[test]
    fn scheduler_drops_stale_frames_and_shows_early_previews() {
        let mut scheduler = MediaFrameScheduler::new();
        assert!(scheduler.push(frame(100, 50)));
        assert!(scheduler.present(Duration::ZERO));
        assert_eq!(
            scheduler.current().expect("preview frame").pts(),
            Duration::from_millis(100)
        );

        scheduler.reset(Duration::from_millis(500));
        assert!(!scheduler.push(frame(0, 100)));
        assert!(!scheduler.has_pending());
    }

    #[test]
    fn seeking_keeps_the_visible_frame_and_drops_queued_frames() {
        let mut scheduler = MediaFrameScheduler::new();
        assert!(scheduler.push(frame(0, 100)));
        assert!(scheduler.push(frame(100, 100)));
        scheduler.present(Duration::ZERO);

        scheduler.seek(Duration::from_secs(1));
        assert_eq!(
            scheduler.current().expect("visible frame").pts(),
            Duration::ZERO
        );
        assert!(!scheduler.has_pending());
        assert!(scheduler.push(frame(1000, 100)));
        assert!(scheduler.present(Duration::from_secs(1)));
        assert_eq!(
            scheduler.current().expect("seeked frame").pts(),
            Duration::from_secs(1)
        );
    }
}
