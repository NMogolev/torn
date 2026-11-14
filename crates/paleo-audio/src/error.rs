//! Errors reported by the Paleo audio runtime.

use std::fmt;

/// A failure while configuring PCM data or opening an output device.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Error {
    /// A sample rate or channel count is outside the supported range.
    InvalidFormat(String),
    /// A buffer does not match the format expected by a converter or device.
    FormatMismatch {
        /// The format the operation requires.
        expected: String,
        /// The format carried by the supplied buffer.
        actual: String,
    },
    /// The host reported no default output device.
    NoOutputDevice,
    /// A device identified by a persisted id is no longer present.
    UnknownDevice(String),
    /// The device does not support the requested stream configuration.
    UnsupportedConfig(String),
    /// Opening, starting, or pausing the output stream failed.
    Stream(String),
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidFormat(detail) => write!(formatter, "invalid PCM format: {detail}"),
            Self::FormatMismatch { expected, actual } => {
                write!(
                    formatter,
                    "PCM format mismatch: expected {expected}, got {actual}"
                )
            }
            Self::NoOutputDevice => formatter.write_str("no audio output device is available"),
            Self::UnknownDevice(id) => {
                write!(formatter, "audio output device {id} is not available")
            }
            Self::UnsupportedConfig(detail) => {
                write!(
                    formatter,
                    "unsupported audio stream configuration: {detail}"
                )
            }
            Self::Stream(detail) => write!(formatter, "audio output failed: {detail}"),
        }
    }
}

impl std::error::Error for Error {}

/// The result type used across `paleo-audio`.
pub type Result<T> = std::result::Result<T, Error>;
