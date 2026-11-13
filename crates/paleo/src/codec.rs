//! Codec identification shared by track metadata and future decoder selection.

use symphonia::core::codecs::audio::{AudioCodecId, well_known as audio};
use symphonia::core::codecs::video::{VideoCodecId, well_known as video};

/// An audio codec recognized by the container layer.
///
/// The set is intentionally open: a container may carry a codec that this
/// build cannot decode, so [`AudioTrackInfo::codec`](crate::AudioTrackInfo::codec)
/// can report a name even when opening a decoder fails.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum AudioCodec {
    /// Advanced Audio Coding, low complexity profile.
    Aac,
    /// Apple Lossless.
    Alac,
    /// Free Lossless Audio Codec.
    Flac,
    /// MPEG-1 Audio Layer I.
    Mp1,
    /// MPEG-1 Audio Layer II.
    Mp2,
    /// MPEG-1 Audio Layer III.
    Mp3,
    /// Ogg Vorbis.
    Vorbis,
    /// Opus, as carried in Ogg or Matroska.
    Opus,
    /// Linear PCM in any integer or floating-point encoding.
    Pcm,
    /// Adaptive Differential PCM.
    Adpcm,
    /// Any codec not listed above.
    Other,
}

impl AudioCodec {
    /// Returns the stable human-readable name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Aac => "aac",
            Self::Alac => "alac",
            Self::Flac => "flac",
            Self::Mp1 => "mp1",
            Self::Mp2 => "mp2",
            Self::Mp3 => "mp3",
            Self::Vorbis => "vorbis",
            Self::Opus => "opus",
            Self::Pcm => "pcm",
            Self::Adpcm => "adpcm",
            Self::Other => "unknown",
        }
    }

    pub(crate) fn from_id(id: AudioCodecId) -> Self {
        match id {
            audio::CODEC_ID_AAC => Self::Aac,
            audio::CODEC_ID_ALAC => Self::Alac,
            audio::CODEC_ID_FLAC => Self::Flac,
            audio::CODEC_ID_MP1 => Self::Mp1,
            audio::CODEC_ID_MP2 => Self::Mp2,
            audio::CODEC_ID_MP3 => Self::Mp3,
            audio::CODEC_ID_VORBIS => Self::Vorbis,
            audio::CODEC_ID_OPUS => Self::Opus,
            audio::CODEC_ID_ADPCM_MS
            | audio::CODEC_ID_ADPCM_IMA_WAV
            | audio::CODEC_ID_ADPCM_IMA_QT => Self::Adpcm,
            id if is_pcm(id) => Self::Pcm,
            _ => Self::Other,
        }
    }
}

impl std::fmt::Display for AudioCodec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

fn is_pcm(id: AudioCodecId) -> bool {
    // `AudioCodecId` exposes no numeric accessor, so compare against the
    // well-known PCM identifiers. Listed in the order Symphonia defines them.
    const PCM: &[AudioCodecId] = &[
        audio::CODEC_ID_PCM_S32LE,
        audio::CODEC_ID_PCM_S32LE_PLANAR,
        audio::CODEC_ID_PCM_S32BE,
        audio::CODEC_ID_PCM_S32BE_PLANAR,
        audio::CODEC_ID_PCM_S24LE,
        audio::CODEC_ID_PCM_S24LE_PLANAR,
        audio::CODEC_ID_PCM_S24BE,
        audio::CODEC_ID_PCM_S24BE_PLANAR,
        audio::CODEC_ID_PCM_S16LE,
        audio::CODEC_ID_PCM_S16LE_PLANAR,
        audio::CODEC_ID_PCM_S16BE,
        audio::CODEC_ID_PCM_S16BE_PLANAR,
        audio::CODEC_ID_PCM_S8,
        audio::CODEC_ID_PCM_S8_PLANAR,
        audio::CODEC_ID_PCM_U32LE,
        audio::CODEC_ID_PCM_U32LE_PLANAR,
        audio::CODEC_ID_PCM_U32BE,
        audio::CODEC_ID_PCM_U32BE_PLANAR,
        audio::CODEC_ID_PCM_U24LE,
        audio::CODEC_ID_PCM_U24LE_PLANAR,
        audio::CODEC_ID_PCM_U24BE,
        audio::CODEC_ID_PCM_U24BE_PLANAR,
        audio::CODEC_ID_PCM_U16LE,
        audio::CODEC_ID_PCM_U16LE_PLANAR,
        audio::CODEC_ID_PCM_U16BE,
        audio::CODEC_ID_PCM_U16BE_PLANAR,
        audio::CODEC_ID_PCM_U8,
        audio::CODEC_ID_PCM_U8_PLANAR,
        audio::CODEC_ID_PCM_F32LE,
        audio::CODEC_ID_PCM_F32LE_PLANAR,
        audio::CODEC_ID_PCM_F32BE,
        audio::CODEC_ID_PCM_F32BE_PLANAR,
        audio::CODEC_ID_PCM_F64LE,
        audio::CODEC_ID_PCM_F64LE_PLANAR,
        audio::CODEC_ID_PCM_F64BE,
        audio::CODEC_ID_PCM_F64BE_PLANAR,
        audio::CODEC_ID_PCM_ALAW,
        audio::CODEC_ID_PCM_MULAW,
    ];

    PCM.contains(&id)
}

/// A video codec recognized by the container layer.
///
/// Paleo only demuxes video packets today; decoding is provided by optional
/// backends. The variant is reported regardless of whether a decoder exists.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum VideoCodec {
    /// H.264 / MPEG-4 AVC.
    H264,
    /// H.265 / HEVC.
    Hevc,
    /// `AOMedia` Video 1.
    Av1,
    /// Google VP8.
    Vp8,
    /// Google VP9.
    Vp9,
    /// MPEG-4 Part 2.
    Mpeg4,
    /// Motion JPEG.
    Mjpeg,
    /// Any codec not listed above.
    Other,
}

impl VideoCodec {
    /// Returns the stable human-readable name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::H264 => "h264",
            Self::Hevc => "hevc",
            Self::Av1 => "av1",
            Self::Vp8 => "vp8",
            Self::Vp9 => "vp9",
            Self::Mpeg4 => "mpeg4",
            Self::Mjpeg => "mjpeg",
            Self::Other => "unknown",
        }
    }

    pub(crate) fn from_id(id: VideoCodecId) -> Self {
        match id {
            video::CODEC_ID_H264 => Self::H264,
            video::CODEC_ID_HEVC => Self::Hevc,
            video::CODEC_ID_AV1 => Self::Av1,
            video::CODEC_ID_VP8 => Self::Vp8,
            video::CODEC_ID_VP9 => Self::Vp9,
            video::CODEC_ID_MPEG4 => Self::Mpeg4,
            video::CODEC_ID_MJPEG => Self::Mjpeg,
            _ => Self::Other,
        }
    }
}

impl std::fmt::Display for VideoCodec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}
