//! Errors reported by the Paleo media layer.

use std::fmt;

/// A failure while opening, demuxing, decoding, or seeking media.
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    /// The media source could not be read.
    Io(std::io::Error),
    /// The container could not be probed or parsed.
    Format(String),
    /// The selected codec has no decoder in this build.
    UnsupportedCodec(String),
    /// A compressed packet could not be decoded.
    Decode(String),
    /// The requested seek is not possible.
    Seek(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "media I/O failed: {error}"),
            Self::Format(detail) => write!(f, "unsupported or invalid container: {detail}"),
            Self::UnsupportedCodec(detail) => write!(f, "no decoder for {detail}"),
            Self::Decode(detail) => write!(f, "decode failed: {detail}"),
            Self::Seek(detail) => write!(f, "seek failed: {detail}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<symphonia::core::errors::Error> for Error {
    fn from(error: symphonia::core::errors::Error) -> Self {
        use symphonia::core::errors::Error as Source;

        match error {
            Source::IoError(error) => Self::Io(error),
            Source::Unsupported(detail) => Self::Format(detail.to_owned()),
            other => Self::Format(other.to_string()),
        }
    }
}

/// The result type used across Paleo.
pub type Result<T> = std::result::Result<T, Error>;
