//! Errors reported by the H.264 backend.

use std::fmt;

/// A failure while configuring or decoding H.264 video.
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    /// The track has no `avcC` configuration record.
    MissingConfig,
    /// The `avcC` record is malformed.
    InvalidConfig(&'static str),
    /// A packet does not match the NAL framing declared by the configuration.
    InvalidPacket(&'static str),
    /// The bitstream uses a feature the decoder does not implement.
    Decode(rusty_h264_decoder::DecodeError),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingConfig => f.write_str("the H.264 track has no avcC record"),
            Self::InvalidConfig(detail) => write!(f, "invalid avcC record: {detail}"),
            Self::InvalidPacket(detail) => write!(f, "invalid H.264 packet: {detail}"),
            Self::Decode(error) => write!(f, "H.264 decode failed: {error}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Decode(error) => Some(error),
            _ => None,
        }
    }
}

impl From<rusty_h264_decoder::DecodeError> for Error {
    fn from(error: rusty_h264_decoder::DecodeError) -> Self {
        Self::Decode(error)
    }
}

/// The result type used across the backend.
pub type Result<T> = std::result::Result<T, Error>;
