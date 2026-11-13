//! Pure-Rust H.264 video playback for [`crate::Video`].
//!
//! [`PaleoSession`] adapts a streaming [`paleo::Media`] and a
//! [`paleo_h264::H264Decoder`] to the backend-neutral [`MediaSession`]
//! contract. It decodes on the calling thread in bounded steps: each
//! [`MediaSession::update`] advances the clock and decodes only enough packets
//! to keep a couple of pictures ahead of the playhead, so the session never
//! builds an unbounded frame queue.

use std::collections::VecDeque;
use std::path::Path;
use std::time::Duration;

use paleo::{Event, Media, MediaSelection, VideoCodec};
use paleo_h264::{H264Decoder, VideoFrame};
use torn_render::RasterImage;

use crate::media::{MediaClock, MediaError, MediaFrame, MediaMetadata, MediaSession, MediaUpdate};

/// Frame rate assumed when the container does not report one.
const DEFAULT_FRAME_RATE: f64 = 30.0;
/// How many decoded pictures to keep buffered ahead of the playhead.
const TARGET_QUEUE: usize = 2;
/// Packet budget for one playback update.
const MAX_PACKETS_PER_UPDATE: usize = 16;
/// Packet budget for a seek preview, which may walk a whole GOP.
const MAX_PACKETS_PER_SEEK: usize = 2_000;

/// A [`MediaSession`] backed by Paleo's demuxer and the pure-Rust H.264
/// decoder.
///
/// Only H.264 in a container understood by Symphonia is supported for now.
/// Files are opened video-only, so an unsupported audio track does not prevent
/// playback and no audio is decoded before an output device exists.
pub struct PaleoSession {
    media: Media,
    decoder: H264Decoder,
    clock: MediaClock,
    metadata: MediaMetadata,
    frame_duration: Duration,
    queue: VecDeque<MediaFrame>,
    ended: bool,
}

impl PaleoSession {
    /// Opens `path` and decodes the first picture.
    ///
    /// # Errors
    ///
    /// Returns [`MediaError`] when the file cannot be opened, has no video
    /// track, uses a codec other than H.264, carries no usable `avcC` record,
    /// or fails to decode.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, MediaError> {
        let media = Media::open_with(path.as_ref(), MediaSelection::VideoOnly)
            .map_err(|error| MediaError::new(format!("could not open media: {error}")))?;
        let video = media
            .info()
            .video()
            .cloned()
            .ok_or_else(|| MediaError::new("the file has no video track"))?;
        if video.codec() != VideoCodec::H264 {
            return Err(MediaError::new(format!(
                "unsupported video codec {}; the Paleo backend decodes H.264 only",
                video.codec()
            )));
        }

        let mut decoder = H264Decoder::new();
        decoder
            .configure(video.extra_data())
            .map_err(|error| MediaError::new(format!("could not configure H.264: {error}")))?;

        let duration = media.info().duration().unwrap_or_default();
        let frame_duration = video
            .frame_rate()
            .filter(|rate| rate.is_finite() && *rate > 0.0)
            .map_or_else(
                || Duration::from_secs_f64(1.0 / DEFAULT_FRAME_RATE),
                |rate| Duration::from_secs_f64(1.0 / rate),
            );

        let mut clock = MediaClock::new();
        clock.set_duration(Some(duration));
        clock.seek(Duration::ZERO);

        let mut session = Self {
            media,
            decoder,
            clock,
            metadata: MediaMetadata::new(video.width(), video.height(), duration),
            frame_duration,
            queue: VecDeque::new(),
            ended: false,
        };
        session.fill(Duration::ZERO, MAX_PACKETS_PER_SEEK)?;
        Ok(session)
    }

    /// Decodes until the queue holds [`TARGET_QUEUE`] pictures at or after
    /// `position`, the stream ends, or `max_packets` packets were read.
    fn fill(&mut self, position: Duration, max_packets: usize) -> Result<(), MediaError> {
        let mut packets = 0;
        while !self.ended && self.queue.len() < TARGET_QUEUE && packets < max_packets {
            match self.media.next_event() {
                Ok(Event::Video(packet)) => {
                    packets += 1;
                    let frames = self.decoder.decode(&packet).map_err(|error| {
                        MediaError::new(format!("H.264 decoding failed: {error}"))
                    })?;
                    self.push_frames(frames)?;
                    self.drop_expired(position);
                }
                // Video-only sessions request no audio track; tolerate the
                // event anyway so the session stays usable if that changes.
                Ok(Event::Audio(_)) => {}
                Ok(Event::EndOfStream) => {
                    self.ended = true;
                    let frames = self.decoder.flush();
                    self.push_frames(frames)?;
                    self.drop_expired(position);
                }
                Err(error) => {
                    self.ended = true;
                    return Err(MediaError::new(format!("demuxing failed: {error}")));
                }
            }
        }
        Ok(())
    }

    fn push_frames(&mut self, frames: Vec<VideoFrame>) -> Result<(), MediaError> {
        for frame in frames {
            let pts = frame.pts();
            let width = frame.width();
            let height = frame.height();
            let image = RasterImage::new(width, height, frame.into_pixels())
                .map_err(|error| MediaError::new(format!("decoded frame is invalid: {error}")))?;
            self.queue
                .push_back(MediaFrame::new(image, pts, self.frame_duration));
        }
        Ok(())
    }

    /// Drops pictures that can no longer be visible at `position`.
    fn drop_expired(&mut self, position: Duration) {
        while self
            .queue
            .front()
            .is_some_and(|frame| frame.end() <= position)
        {
            self.queue.pop_front();
        }
    }
}

impl MediaSession for PaleoSession {
    fn metadata(&self) -> Option<&MediaMetadata> {
        Some(&self.metadata)
    }

    fn clock(&self) -> &MediaClock {
        &self.clock
    }

    fn clock_mut(&mut self) -> &mut MediaClock {
        &mut self.clock
    }

    fn play(&mut self) -> Result<(), MediaError> {
        if self.clock.is_ended() || (self.ended && self.queue.is_empty()) {
            self.seek(Duration::ZERO)?;
        }
        self.clock.play();
        Ok(())
    }

    fn pause(&mut self) {
        self.clock.pause();
    }

    fn seek(&mut self, position: Duration) -> Result<(), MediaError> {
        self.clock.seek(position);
        let target = self.clock.position();
        self.media
            .seek(target)
            .map_err(|error| MediaError::new(format!("seek failed: {error}")))?;
        self.decoder.reset();
        self.queue.clear();
        self.ended = false;
        // Decode a preview even while paused, so the view can repaint before
        // playback resumes.
        self.fill(target, MAX_PACKETS_PER_SEEK)
    }

    fn set_rate(&mut self, rate: f64) {
        self.clock.set_rate(rate);
    }

    fn update(&mut self, delta: Duration) -> MediaUpdate {
        let mut state_changed = self.clock.advance(delta);

        let mut error = None;
        if self.clock.is_playing()
            && !self.ended
            && let Err(failure) = self.fill(self.clock.position(), MAX_PACKETS_PER_UPDATE)
        {
            error = Some(failure);
        }

        if self.ended && self.queue.is_empty() && !self.clock.is_ended() {
            state_changed |= self.clock.finish();
        }
        if error.is_some() {
            self.clock.fail();
            self.ended = true;
            state_changed = true;
        }

        let frame = if error.is_some() {
            None
        } else {
            self.queue.pop_front()
        };
        MediaUpdate {
            frame,
            error,
            state_changed,
        }
    }

    fn is_active(&self) -> bool {
        self.clock.is_playing() || !self.queue.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_a_missing_file() {
        let error = PaleoSession::open("torn-paleo-missing-file.mp4")
            .err()
            .expect("a missing file cannot be opened");
        assert!(error.message().contains("could not open media"), "{error}");
    }
}
