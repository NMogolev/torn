//! Streaming container access, audio decoding, and video demuxing.
//!
//! [`Media`] owns a [`FormatReader`] and at most one audio decoder. It never
//! reads the whole container into memory: the format reader pulls from a fixed
//! 64 KiB source buffer and every packet is borrowed from the container only
//! long enough to decode or hand to the caller.

use std::fs::File;
use std::io::ErrorKind;
use std::path::Path;
use std::time::Duration;

use symphonia::core::codecs::CodecParameters;
use symphonia::core::codecs::audio::{AudioCodecParameters, AudioDecoder, AudioDecoderOptions};
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{
    FormatOptions, FormatReader, SeekMode, SeekTo, Track, TrackFlags, TrackType,
};
use symphonia::core::io::{MediaSourceStream, MediaSourceStreamOptions};
use symphonia::core::meta::MetadataOptions;
use symphonia::core::units::{Time, TimeBase, Timestamp};

use crate::audio::AudioBuffer;
use crate::codec::{AudioCodec, VideoCodec};
use crate::error::{Error, Result};
use crate::video::VideoPacket;

/// Consecutive undecodable audio packets tolerated before playback fails.
const MAX_DECODE_FAILURES: u32 = 64;

/// Everything a caller needs to configure playback without decoding.
#[derive(Clone, Debug)]
pub struct MediaInfo {
    format: String,
    duration: Option<Duration>,
    audio: Option<AudioTrackInfo>,
    video: Option<VideoTrackInfo>,
}

impl MediaInfo {
    /// Returns the container short name, for example `mp4` or `matroska`.
    #[must_use]
    pub fn format(&self) -> &str {
        &self.format
    }

    /// Returns the media duration when the container reports one.
    #[must_use]
    pub const fn duration(&self) -> Option<Duration> {
        self.duration
    }

    /// Returns the selected audio track.
    #[must_use]
    pub const fn audio(&self) -> Option<&AudioTrackInfo> {
        self.audio.as_ref()
    }

    /// Returns the selected video track.
    #[must_use]
    pub const fn video(&self) -> Option<&VideoTrackInfo> {
        self.video.as_ref()
    }
}

/// Metadata for the selected audio track.
#[derive(Clone, Copy, Debug)]
pub struct AudioTrackInfo {
    codec: AudioCodec,
    sample_rate: u32,
    channels: usize,
}

impl AudioTrackInfo {
    /// Returns the audio codec.
    #[must_use]
    pub const fn codec(&self) -> AudioCodec {
        self.codec
    }

    /// Returns the sample rate in hertz.
    #[must_use]
    pub const fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Returns the number of channels.
    #[must_use]
    pub const fn channels(&self) -> usize {
        self.channels
    }
}

/// Metadata for the selected video track.
#[derive(Clone, Copy, Debug)]
pub struct VideoTrackInfo {
    codec: VideoCodec,
    width: u32,
    height: u32,
    frame_rate: Option<f64>,
}

impl VideoTrackInfo {
    /// Returns the video codec.
    #[must_use]
    pub const fn codec(&self) -> VideoCodec {
        self.codec
    }

    /// Returns the coded width in pixels.
    #[must_use]
    pub const fn width(&self) -> u32 {
        self.width
    }

    /// Returns the coded height in pixels.
    #[must_use]
    pub const fn height(&self) -> u32 {
        self.height
    }

    /// Returns the average frame rate when it can be derived from the container.
    #[must_use]
    pub const fn frame_rate(&self) -> Option<f64> {
        self.frame_rate
    }
}

/// One item produced by [`Media::next_event`].
#[derive(Clone, Debug)]
pub enum Event {
    /// A decoded block of audio.
    Audio(AudioBuffer),
    /// A compressed video packet.
    Video(VideoPacket),
    /// No more packets will be produced until the next seek.
    EndOfStream,
}

/// An open media file.
///
/// The reader is streaming and bounded: memory use is the source buffer, the
/// decoder state, and one returned [`AudioBuffer`] or [`VideoPacket`] per call.
pub struct Media {
    reader: Box<dyn FormatReader>,
    info: MediaInfo,
    audio_track: Option<u32>,
    video_track: Option<u32>,
    seek_track: Option<u32>,
    audio_time_base: Option<TimeBase>,
    video_time_base: Option<TimeBase>,
    decoder: Option<Box<dyn AudioDecoder>>,
    decode_failures: u32,
    ended: bool,
}

impl Media {
    /// Opens `path`, probes the container, and prepares the default tracks.
    ///
    /// Video tracks are demuxed but not decoded. The selected audio track must
    /// have a decoder in this build.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Io`] when the file cannot be read, [`Error::Format`]
    /// when the container cannot be probed or parsed, and
    /// [`Error::UnsupportedCodec`] when the selected audio codec has no decoder.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let file = File::open(path)?;
        let source = MediaSourceStream::new(Box::new(file), MediaSourceStreamOptions::default());

        // Most containers are recognized by their leading bytes, but a few
        // codec-only streams (raw MP3, ADTS AAC) are only distinguishable by
        // extension. The hint is advisory and never overrides content probing.
        let mut hint = Hint::new();
        if let Some(extension) = path.extension().and_then(|extension| extension.to_str()) {
            hint.with_extension(extension);
        }

        let mut reader = symphonia::default::get_probe().probe(
            &hint,
            source,
            FormatOptions::default(),
            MetadataOptions::default(),
        )?;

        let format = reader.format_info().short_name.to_owned();
        let duration = media_duration(reader.as_ref());
        let tracks = reader.tracks().to_vec();

        let audio_track = select_track(&tracks, TrackType::Audio);
        let video_track = select_track(&tracks, TrackType::Video);

        let (audio, decoder) = match audio_track
            .and_then(|track| audio_parameters(track))
            .map(|params| (AudioCodec::from_id(params.codec), params))
        {
            Some((codec, params)) => {
                let decoder = symphonia::default::get_codecs()
                    .make_audio_decoder(params, &AudioDecoderOptions::default())
                    .map_err(|error| Error::UnsupportedCodec(format!("{codec} audio ({error})")))?;
                let resolved = decoder.codec_params();
                let info = AudioTrackInfo {
                    codec,
                    sample_rate: resolved
                        .sample_rate
                        .or(params.sample_rate)
                        .unwrap_or_default(),
                    channels: resolved
                        .channels
                        .as_ref()
                        .or(params.channels.as_ref())
                        .map_or(0, symphonia::core::audio::Channels::count),
                };
                (Some(info), Some(decoder))
            }
            None => (None, None),
        };

        let video = video_track.and_then(video_info);

        let seek_track = video_track
            .map(|track| track.id)
            .or_else(|| audio_track.map(|track| track.id));

        let info = MediaInfo {
            format,
            duration,
            audio,
            video,
        };

        // Metadata revisions are only needed by tag viewers; do not retain them.
        let _ = reader.metadata();

        Ok(Self {
            reader,
            info,
            audio_track: audio_track.map(|track| track.id),
            video_track: video_track.map(|track| track.id),
            seek_track,
            audio_time_base: audio_track.and_then(|track| track.time_base),
            video_time_base: video_track.and_then(|track| track.time_base),
            decoder,
            decode_failures: 0,
            ended: false,
        })
    }

    /// Returns the stream metadata.
    #[must_use]
    pub const fn info(&self) -> &MediaInfo {
        &self.info
    }

    /// Returns the next decoded audio block, video packet, or end marker.
    ///
    /// Packets from unselected tracks are skipped. A packet that the audio
    /// decoder rejects is dropped; [`MAX_DECODE_FAILURES`] consecutive failures
    /// abort playback with [`Error::Decode`].
    ///
    /// # Errors
    ///
    /// Returns [`Error::Io`] when the source fails, [`Error::Decode`] after
    /// repeated undecodable audio packets, or [`Error::Format`] for containers
    /// that become inconsistent while reading.
    pub fn next_event(&mut self) -> Result<Event> {
        if self.ended {
            return Ok(Event::EndOfStream);
        }

        loop {
            let packet = match self.reader.next_packet() {
                Ok(Some(packet)) => packet,
                Ok(None) => return Ok(self.finish()),
                Err(symphonia::core::errors::Error::IoError(error))
                    if error.kind() == ErrorKind::UnexpectedEof =>
                {
                    return Ok(self.finish());
                }
                Err(symphonia::core::errors::Error::ResetRequired) => {
                    self.reset_decoder();
                    continue;
                }
                Err(error) => return Err(error.into()),
            };

            if self.video_track == Some(packet.track_id) {
                let pts = timestamp_duration(self.video_time_base, packet.pts).unwrap_or_default();
                let dts = timestamp_duration(self.video_time_base, packet.dts).unwrap_or(pts);
                return Ok(Event::Video(VideoPacket::new(pts, dts, packet.data)));
            }

            if self.audio_track == Some(packet.track_id) {
                let Some(decoder) = self.decoder.as_mut() else {
                    continue;
                };
                match decoder.decode(&packet) {
                    Ok(buffer) => {
                        let pts = timestamp_duration(self.audio_time_base, packet.pts)
                            .unwrap_or_default();
                        let mut samples = Vec::with_capacity(buffer.samples_interleaved());
                        buffer.copy_to_vec_interleaved(&mut samples);
                        let spec = buffer.spec();
                        let audio =
                            AudioBuffer::new(pts, spec.rate(), spec.channels().count(), samples);
                        self.decode_failures = 0;
                        return Ok(Event::Audio(audio));
                    }
                    Err(symphonia::core::errors::Error::DecodeError(detail)) => {
                        self.decode_failures += 1;
                        if self.decode_failures >= MAX_DECODE_FAILURES {
                            return Err(Error::Decode(format!(
                                "{MAX_DECODE_FAILURES} consecutive packets: {detail}"
                            )));
                        }
                    }
                    Err(symphonia::core::errors::Error::ResetRequired) => {
                        self.reset_decoder();
                    }
                    Err(error) => return Err(error.into()),
                }
            }
        }
    }

    /// Seeks to `position` and resets asynchronous decoder state.
    ///
    /// The next event starts at or before `position`; containers with coarse
    /// indexes may start earlier. The audio decoder is reset, so any buffered
    /// samples from before the seek are discarded.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Seek`] when the position cannot be represented or the
    /// container refuses the seek, and [`Error::Io`] when the source fails.
    pub fn seek(&mut self, position: Duration) -> Result<()> {
        let Some(track_id) = self.seek_track else {
            return Ok(());
        };
        let time = Time::try_from_secs_f64(position.as_secs_f64())
            .ok_or_else(|| Error::Seek(format!("{position:?} is out of range")))?;
        self.reader
            .seek(
                SeekMode::Accurate,
                SeekTo::Time {
                    time,
                    track_id: Some(track_id),
                },
            )
            .map_err(|error| Error::Seek(error.to_string()))?;
        self.reset_decoder();
        self.decode_failures = 0;
        self.ended = false;
        Ok(())
    }

    fn finish(&mut self) -> Event {
        self.ended = true;
        Event::EndOfStream
    }

    fn reset_decoder(&mut self) {
        if let Some(decoder) = self.decoder.as_mut() {
            decoder.reset();
        }
    }
}

fn select_track(tracks: &[Track], track_type: TrackType) -> Option<&Track> {
    tracks
        .iter()
        .filter(|track| track.track_type() == Some(track_type))
        .max_by_key(|track| track.flags.contains(TrackFlags::DEFAULT))
}

fn audio_parameters(track: &Track) -> Option<&AudioCodecParameters> {
    match track.codec_params.as_ref()? {
        CodecParameters::Audio(params) => Some(params),
        _ => None,
    }
}

fn video_info(track: &Track) -> Option<VideoTrackInfo> {
    let CodecParameters::Video(params) = track.codec_params.as_ref()? else {
        return None;
    };
    let width = u32::from(params.width?);
    let height = u32::from(params.height?);
    if width == 0 || height == 0 {
        return None;
    }
    let frame_rate = track
        .num_frames
        .zip(track.duration)
        .and_then(|(frames, duration)| {
            let frames = u32::try_from(frames).ok()?;
            let time = track.time_base?.calc_duration(duration)?;
            Some((frames, time))
        })
        .map(|(frames, time)| f64::from(frames) * time.as_secs_f64().recip())
        .filter(|rate| rate.is_finite() && *rate > 0.0);

    Some(VideoTrackInfo {
        codec: VideoCodec::from_id(params.codec),
        width,
        height,
        frame_rate,
    })
}

fn media_duration(reader: &dyn FormatReader) -> Option<Duration> {
    let media = reader.media_info();
    let time_base = media.time_base?;
    let ticks = media.duration?;
    duration_from_time(time_base.calc_duration(ticks)?)
}

fn timestamp_duration(time_base: Option<TimeBase>, timestamp: Timestamp) -> Option<Duration> {
    let time = time_base?.calc_time(timestamp)?;
    duration_from_time(time)
}

fn duration_from_time(time: Time) -> Option<Duration> {
    let seconds = time.as_secs_f64();
    if !seconds.is_finite() || seconds < 0.0 {
        return None;
    }
    Duration::try_from_secs_f64(seconds).ok()
}
