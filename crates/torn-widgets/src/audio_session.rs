//! Audio-mastered Paleo playback for [`crate::Video`].
//!
//! [`AudioPaleoSession`] is the audio-enabled counterpart of
//! [`crate::PaleoSession`]. It decodes audio through Paleo, converts it to the
//! output device format, and uses the device position as the master clock.
//! Video frames are still decoded by `paleo-h264`; they follow the audio clock
//! instead of advancing a wall-clock timer.
//!
//! The session is behind the `audio` feature of `torn-widgets`, so a normal UI
//! build does not link `cpal` or a native audio backend.

use std::collections::VecDeque;
use std::path::Path;
use std::time::Duration;

use paleo::{Event, Media, VideoCodec};
use paleo_audio::{AudioOutput, PcmBuffer, PcmConverter};
use paleo_h264::{H264Decoder, VideoFrame};
use torn_render::RasterImage;

use crate::media::{MediaClock, MediaError, MediaFrame, MediaMetadata, MediaSession, MediaUpdate};

/// Frame rate assumed when the container does not report one.
const DEFAULT_FRAME_RATE: f64 = 30.0;
/// How much audio to keep queued ahead of the output device.
const TARGET_AUDIO_BUFFER: Duration = Duration::from_millis(120);
/// How many decoded video frames to keep ahead of the playhead.
const TARGET_VIDEO_QUEUE: usize = 2;
/// Hard bound on decoded frames in memory after a seek or a sparse track.
const MAX_VIDEO_QUEUE: usize = 8;
/// Packet budget for one playback update.
const MAX_PACKETS_PER_UPDATE: usize = 64;
/// Packet budget for a seek preview, which may walk a whole GOP.
const MAX_PACKETS_PER_SEEK: usize = 2_000;

struct AudioRuntime {
    output: AudioOutput,
    converter: PcmConverter,
}

/// A [`MediaSession`] that plays audio through a device and follows it with
/// video.
///
/// Audio is the master clock whenever an audio track is present. Without an
/// audio track the session falls back to the wall-clock behavior of
/// [`crate::PaleoSession`]. A video track is decoded only when it is H.264;
/// unsupported video codecs are skipped so the audio still plays.
///
/// Playback speed is currently fixed at `1x`. Changing the speed would require
/// resampling the source at a scaled rate, so [`MediaSession::set_rate`] keeps
/// the clock at `1.0` rather than reporting a rate the device does not follow.
pub struct AudioPaleoSession {
    media: Media,
    decoder: Option<H264Decoder>,
    audio: Option<AudioRuntime>,
    clock: MediaClock,
    metadata: MediaMetadata,
    frame_duration: Duration,
    queue: VecDeque<MediaFrame>,
    ended: bool,
}

impl AudioPaleoSession {
    /// Opens `path`, prepares audio output, and decodes the first preview frame.
    ///
    /// # Errors
    ///
    /// Returns [`MediaError`] when the file cannot be opened, the selected audio
    /// codec has no decoder, no output device can be opened, or the output
    /// format cannot represent the decoded audio.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, MediaError> {
        let media = Media::open(path.as_ref())
            .map_err(|error| MediaError::new(format!("could not open media: {error}")))?;
        let info = media.info().clone();
        let duration = info.duration().unwrap_or_default();

        let audio = match info.audio() {
            Some(track) => {
                let output = AudioOutput::open().map_err(|error| {
                    MediaError::new(format!("could not open the audio output: {error}"))
                })?;
                let source = paleo_audio::PcmFormat::new(track.sample_rate(), track.channels())
                    .map_err(|error| {
                        MediaError::new(format!("invalid source audio format: {error}"))
                    })?;
                let converter = PcmConverter::new(source, output.format()).map_err(|error| {
                    MediaError::new(format!("could not adapt audio to the device: {error}"))
                })?;
                Some(AudioRuntime { output, converter })
            }
            None => None,
        };

        let decoder = match info.video() {
            Some(video) if video.codec() == VideoCodec::H264 => {
                let mut decoder = H264Decoder::new();
                decoder.configure(video.extra_data()).map_err(|error| {
                    MediaError::new(format!("could not configure H.264: {error}"))
                })?;
                Some(decoder)
            }
            // Audio playback should not fail because the video codec has no
            // backend. The video track is skipped and sound still plays.
            _ => None,
        };

        let (width, height) = info
            .video()
            .map_or((0, 0), |video| (video.width(), video.height()));
        let frame_duration = info
            .video()
            .and_then(paleo::VideoTrackInfo::frame_rate)
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
            audio,
            clock,
            metadata: MediaMetadata::new(width, height, duration),
            frame_duration,
            queue: VecDeque::new(),
            ended: false,
        };
        session.fill(Duration::ZERO, MAX_PACKETS_PER_SEEK)?;
        Ok(session)
    }

    /// Decodes until audio and video buffers are full, the stream ends, or
    /// `max_packets` packets were read.
    fn fill(&mut self, position: Duration, max_packets: usize) -> Result<(), MediaError> {
        let mut packets = 0;
        while packets < max_packets {
            let audio_needed = self
                .audio
                .as_ref()
                .is_some_and(|audio| audio.output.buffered() < TARGET_AUDIO_BUFFER);
            let video_needed = self.decoder.is_some() && self.queue.len() < TARGET_VIDEO_QUEUE;
            if self.ended || (!audio_needed && !video_needed) {
                break;
            }

            match self.media.next_event() {
                Ok(Event::Audio(buffer)) => {
                    packets += 1;
                    if let Some(audio) = self.audio.as_mut() {
                        let pcm = PcmBuffer::from_paleo(buffer).map_err(|error| {
                            MediaError::new(format!("invalid decoded audio: {error}"))
                        })?;
                        let pcm = audio.converter.push(&pcm).map_err(|error| {
                            MediaError::new(format!("could not convert audio: {error}"))
                        })?;
                        audio.output.queue(&pcm).map_err(|error| {
                            MediaError::new(format!("could not queue audio: {error}"))
                        })?;
                    }
                }
                Ok(Event::Video(packet)) => {
                    packets += 1;
                    if let Some(decoder) = self.decoder.as_mut() {
                        let frames = decoder.decode(&packet).map_err(|error| {
                            MediaError::new(format!("H.264 decoding failed: {error}"))
                        })?;
                        self.push_frames(frames)?;
                    }
                }
                Ok(Event::EndOfStream) => {
                    self.ended = true;
                    if let Some(decoder) = self.decoder.as_mut() {
                        let frames = decoder.flush();
                        self.push_frames(frames)?;
                    }
                }
                Err(error) => {
                    self.ended = true;
                    return Err(MediaError::new(format!("demuxing failed: {error}")));
                }
            }
        }
        self.drop_expired(position);
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
        while self.queue.len() > MAX_VIDEO_QUEUE {
            self.queue.pop_front();
        }
        Ok(())
    }

    fn drop_expired(&mut self, position: Duration) {
        while self
            .queue
            .front()
            .is_some_and(|frame| frame.end() <= position)
        {
            self.queue.pop_front();
        }
    }

    fn audio_buffered(&self) -> bool {
        self.audio
            .as_ref()
            .is_some_and(|audio| !audio.output.buffered().is_zero())
    }
}

impl MediaSession for AudioPaleoSession {
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
        if self.clock.is_ended() || (self.ended && self.queue.is_empty() && !self.audio_buffered())
        {
            self.seek(Duration::ZERO)?;
        }
        if let Some(audio) = self.audio.as_mut() {
            audio
                .output
                .play()
                .map_err(|error| MediaError::new(format!("could not start audio: {error}")))?;
        }
        self.clock.play();
        Ok(())
    }

    fn pause(&mut self) {
        if let Some(audio) = self.audio.as_mut() {
            audio.output.pause();
        }
        self.clock.pause();
    }

    fn seek(&mut self, position: Duration) -> Result<(), MediaError> {
        let was_playing = self.clock.is_playing();
        self.clock.seek(position);
        let target = self.clock.position();
        self.media
            .seek(target)
            .map_err(|error| MediaError::new(format!("seek failed: {error}")))?;
        if let Some(decoder) = self.decoder.as_mut() {
            decoder.reset();
        }
        self.queue.clear();
        self.ended = false;
        if let Some(audio) = self.audio.as_mut() {
            audio.converter.reset_at(target);
            audio.output.pause();
            audio.output.reset(target);
        }

        self.fill(target, MAX_PACKETS_PER_SEEK)?;

        if was_playing {
            if let Some(audio) = self.audio.as_mut() {
                audio
                    .output
                    .play()
                    .map_err(|error| MediaError::new(format!("could not resume audio: {error}")))?;
            }
            self.clock.play();
        }
        Ok(())
    }

    fn set_rate(&mut self, _rate: f64) {
        self.clock.set_rate(1.0);
    }

    fn update(&mut self, delta: Duration) -> MediaUpdate {
        let playing = self.clock.is_playing();
        let mut state_changed = false;
        let mut error = None;

        if playing {
            if let Some(audio) = self.audio.as_mut()
                && !audio.output.is_playing()
                && let Err(failure) = audio.output.play()
            {
                error = Some(MediaError::new(format!("could not start audio: {failure}")));
            }
            if error.is_none()
                && let Err(failure) = self.fill(self.clock.position(), MAX_PACKETS_PER_UPDATE)
            {
                error = Some(failure);
            }
        }

        if error.is_none() {
            if let Some(audio) = &self.audio {
                state_changed |= self.clock.synchronize(audio.output.position());
            } else if playing {
                state_changed |= self.clock.advance(delta);
            }
        }

        if let Some(failure) = error {
            self.clock.fail();
            self.ended = true;
            return MediaUpdate {
                frame: None,
                error: Some(failure),
                state_changed: true,
            };
        }

        let position = self.clock.position();
        self.drop_expired(position);
        let frame = self.queue.pop_front();

        let drained = self.ended
            && self.queue.is_empty()
            && self.audio.as_ref().is_none_or(|audio| {
                audio.output.buffered().is_zero()
                    || self.clock.position() >= self.metadata.duration()
            });
        if drained && !self.clock.is_ended() {
            state_changed |= self.clock.finish();
        }

        MediaUpdate {
            frame,
            error: None,
            state_changed,
        }
    }

    fn is_active(&self) -> bool {
        self.clock.is_playing() || !self.queue.is_empty() || self.audio_buffered()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_a_missing_file() {
        let error = AudioPaleoSession::open("torn-paleo-audio-missing-file.mp4")
            .err()
            .expect("a missing file cannot be opened");
        assert!(error.message().contains("could not open media"), "{error}");
    }
}
