//! Demuxed video packets and codec configuration.

use std::time::Duration;

/// The container-level configuration record for a video codec.
///
/// Most compressed video formats need out-of-band data before the first frame
/// can be decoded: H.264 needs SPS/PPS from the `avcC` record, HEVC needs
/// `hvcC`, and AV1 needs `av1C`. Containers such as ISO/MP4 and Matroska carry
/// this record in the track header, not in the packet stream.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum VideoExtraDataKind {
    /// `AVCDecoderConfigurationRecord` from an `avcC` box.
    AvcDecoderConfig,
    /// `HEVCDecoderConfigurationRecord` from an `hvcC` box.
    HevcDecoderConfig,
    /// VP9 codec configuration.
    Vp9DecoderConfig,
    /// AV1 codec configuration from an `av1C` box.
    Av1DecoderConfig,
    /// Dolby Vision configuration.
    DolbyVisionConfig,
    /// A record this build does not recognize.
    Other,
}

impl VideoExtraDataKind {
    /// Returns the stable human-readable name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AvcDecoderConfig => "avcC",
            Self::HevcDecoderConfig => "hvcC",
            Self::Vp9DecoderConfig => "vp9",
            Self::Av1DecoderConfig => "av1C",
            Self::DolbyVisionConfig => "dovi",
            Self::Other => "unknown",
        }
    }
}

impl std::fmt::Display for VideoExtraDataKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One codec configuration record attached to a video track.
#[derive(Clone, Debug)]
pub struct VideoExtraData {
    kind: VideoExtraDataKind,
    data: Box<[u8]>,
}

impl VideoExtraData {
    pub(crate) fn new(kind: VideoExtraDataKind, data: Box<[u8]>) -> Self {
        Self { kind, data }
    }

    /// Returns the record kind.
    #[must_use]
    pub const fn kind(&self) -> VideoExtraDataKind {
        self.kind
    }

    /// Returns the raw record bytes, exactly as stored by the container.
    #[must_use]
    pub fn data(&self) -> &[u8] {
        &self.data
    }
}

/// One compressed video packet as stored in the container.
///
/// Paleo does not decode video itself. A backend takes these packets, together
/// with [`VideoTrackInfo`](crate::VideoTrackInfo), and produces images.
#[derive(Clone, Debug)]
pub struct VideoPacket {
    pts: Duration,
    dts: Duration,
    data: Box<[u8]>,
}

impl VideoPacket {
    pub(crate) fn new(pts: Duration, dts: Duration, data: Box<[u8]>) -> Self {
        Self { pts, dts, data }
    }

    /// Returns the presentation timestamp.
    #[must_use]
    pub const fn pts(&self) -> Duration {
        self.pts
    }

    /// Returns the decode timestamp.
    #[must_use]
    pub const fn dts(&self) -> Duration {
        self.dts
    }

    /// Returns the compressed packet payload.
    #[must_use]
    pub fn data(&self) -> &[u8] {
        &self.data
    }
}
