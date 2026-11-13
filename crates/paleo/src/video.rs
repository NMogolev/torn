//! Demuxed video packets.

use std::time::Duration;

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
