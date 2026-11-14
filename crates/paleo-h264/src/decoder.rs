//! Streaming H.264 decoding over Paleo's demuxed packets.

use std::time::Duration;

use paleo::{VideoExtraData, VideoExtraDataKind, VideoPacket};
use rusty_h264_common::YuvFrame;
use rusty_h264_decoder::Decoder;

use crate::avc::AvcConfig;
use crate::error::{Error, Result};
use crate::rgba::{ColorMatrix, convert};

/// How many decoded pictures may wait for their display order to be known.
///
/// H.264 B-slices make decode order differ from presentation order, so the
/// backend holds a few decoded pictures keyed by `PicOrderCnt` and emits the
/// earliest one once the window is full. The depth must be at least the
/// stream's `max_num_reorder_frames`; four covers the common two- and
/// three-B-pyramid patterns. Larger windows cost one YUV frame of memory each.
pub const DEFAULT_REORDER_DEPTH: usize = 4;

/// One decoded picture, converted to RGBA.
#[derive(Clone, Debug)]
pub struct VideoFrame {
    pts: Duration,
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

impl VideoFrame {
    /// Returns the presentation timestamp of the picture.
    #[must_use]
    pub const fn pts(&self) -> Duration {
        self.pts
    }

    /// Returns the width in pixels.
    #[must_use]
    pub const fn width(&self) -> u32 {
        self.width
    }

    /// Returns the height in pixels.
    #[must_use]
    pub const fn height(&self) -> u32 {
        self.height
    }

    /// Returns the row stride in bytes. Pixels are tightly packed RGBA.
    #[must_use]
    pub fn stride(&self) -> usize {
        usize::try_from(self.width).unwrap_or_default() * 4
    }

    /// Returns the tightly packed RGBA pixels.
    #[must_use]
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    /// Consumes the frame and returns its RGBA pixels.
    #[must_use]
    pub fn into_pixels(self) -> Vec<u8> {
        self.pixels
    }
}

/// A streaming H.264 decoder driven by [`paleo::VideoPacket`] values.
///
/// The decoder is single-threaded and owns no packet queue. It keeps the
/// decoder reference state, one reusable Annex-B scratch buffer, and at most
/// [`DEFAULT_REORDER_DEPTH`] decoded YUV pictures.
pub struct H264Decoder {
    decoder: Decoder,
    config: Option<AvcConfig>,
    matrix: ColorMatrix,
    reorder_depth: usize,
    /// Decoded pictures in decode order, keyed by `PicOrderCnt`.
    pending: Vec<(i32, YuvFrame)>,
    /// Packet timestamps whose picture has not been assigned a presentation
    /// timestamp yet. Container timestamps arrive in decode order, so the
    /// smallest one is always the next display time within a closed GOP.
    slots: Vec<Duration>,
    scratch: Vec<u8>,
    awaiting_keyframe: bool,
}

impl Default for H264Decoder {
    fn default() -> Self {
        Self::new()
    }
}

impl H264Decoder {
    /// Creates a decoder that will wait for an IDR picture.
    #[must_use]
    pub fn new() -> Self {
        Self {
            decoder: Decoder::new(),
            config: None,
            matrix: ColorMatrix::Auto,
            reorder_depth: DEFAULT_REORDER_DEPTH,
            pending: Vec::new(),
            slots: Vec::new(),
            scratch: Vec::new(),
            awaiting_keyframe: true,
        }
    }

    /// Reads the H.264 configuration record from track metadata.
    ///
    /// # Errors
    ///
    /// Returns [`Error::MissingConfig`] when the track carries no `avcC`
    /// record, and [`Error::InvalidConfig`] when the record is malformed.
    pub fn configure(&mut self, extra_data: &[VideoExtraData]) -> Result<()> {
        let data = extra_data
            .iter()
            .find(|extra| extra.kind() == VideoExtraDataKind::AvcDecoderConfig)
            .map(VideoExtraData::data)
            .ok_or(Error::MissingConfig)?;
        self.configure_avc(data)
    }

    /// Reads an `AVCDecoderConfigurationRecord` directly.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidConfig`] when the record is malformed.
    pub fn configure_avc(&mut self, record: &[u8]) -> Result<()> {
        self.config = Some(AvcConfig::parse(record)?);
        self.reset();
        Ok(())
    }

    /// Sets the YUV to RGB matrix. The default derives it from the frame height.
    pub const fn set_color_matrix(&mut self, matrix: ColorMatrix) {
        self.matrix = matrix;
    }

    /// Sets how many decoded pictures may wait for presentation order.
    ///
    /// Set this to at least the stream's `max_num_reorder_frames` from the SPS
    /// VUI. A depth of one is only correct for streams without B-slices.
    pub fn set_reorder_depth(&mut self, depth: usize) {
        self.reorder_depth = depth.max(1);
    }

    /// Drops decoder state, buffered pictures, and the current keyframe.
    ///
    /// Call this after seeking. The decoder skips packets until the next IDR
    /// picture, because inter pictures cannot be decoded from an arbitrary
    /// point in the stream.
    pub fn reset(&mut self) {
        self.decoder = Decoder::new();
        self.pending.clear();
        self.slots.clear();
        self.awaiting_keyframe = true;
    }

    /// Reports whether the packet is a random-access point.
    ///
    /// Returns `false` before [`Self::configure`] has seen an `avcC` record.
    #[must_use]
    pub fn is_keyframe(&self, packet: &VideoPacket) -> bool {
        self.is_keyframe_data(packet.data())
    }

    /// Reports whether raw length-prefixed packet data contains an IDR slice.
    ///
    /// This is the byte-level form of [`Self::is_keyframe`], used while scanning
    /// the demuxer for the keyframe that precedes a seek target.
    #[must_use]
    pub fn is_keyframe_data(&self, data: &[u8]) -> bool {
        self.config
            .as_ref()
            .is_some_and(|config| config.is_idr(data))
    }

    /// Reports whether decoding is waiting for the next random-access packet.
    ///
    /// A reset decoder drops inter pictures until the next IDR. A session can
    /// use this to avoid demuxing far ahead of its clock while video is
    /// resynchronizing after a seek or a hidden window.
    #[must_use]
    pub const fn is_awaiting_keyframe(&self) -> bool {
        self.awaiting_keyframe
    }

    /// Decodes one packet and returns the pictures that are ready to present.
    ///
    /// Most packets produce at most one picture. The result is empty while the
    /// reorder window fills, while waiting for an IDR picture after a reset,
    /// or when a packet carries only parameter sets.
    ///
    /// # Errors
    ///
    /// Returns [`Error::MissingConfig`] before configuration,
    /// [`Error::InvalidPacket`] for malformed NAL framing, and
    /// [`Error::Decode`] when the bitstream uses an unsupported tool.
    pub fn decode(&mut self, packet: &VideoPacket) -> Result<Vec<VideoFrame>> {
        let keyframe = match self.config.as_ref() {
            Some(config) => config.is_idr(packet.data()),
            None => return Err(Error::MissingConfig),
        };

        if self.awaiting_keyframe {
            if !keyframe {
                return Ok(Vec::new());
            }
            self.awaiting_keyframe = false;
        }

        // An IDR starts a new presentation sequence: its `PicOrderCnt` is zero,
        // so buffered pictures from the previous GOP must be emitted first or
        // they would sort after it.
        let mut ready = Vec::new();
        if keyframe {
            self.drain_pending_into(&mut ready);
        }

        let Some(config) = self.config.as_ref() else {
            return Err(Error::MissingConfig);
        };
        config.to_annex_b(packet.data(), &mut self.scratch, keyframe)?;
        let decoded = self.decoder.decode(&self.scratch)?;

        if let Some(frame) = decoded {
            // `last_poc` is the display-order key of the picture just decoded.
            // Container timestamps arrive in decode order; since a reordered
            // GOP is a permutation of the same display times, the smallest
            // unassigned timestamp is the correct one for the next picture in
            // display order.
            self.pending.push((self.decoder.last_poc(), frame));
            self.slots.push(packet.pts());
            while self.pending.len() > self.reorder_depth {
                ready.push(self.pop_earliest());
            }
        }
        Ok(self.convert_all(ready))
    }

    /// Emits every buffered picture in presentation order.
    ///
    /// Call this at end of stream; [`Self::decode`] already flushes a GOP when
    /// the next keyframe arrives.
    #[must_use]
    pub fn flush(&mut self) -> Vec<VideoFrame> {
        let mut ready = Vec::new();
        self.drain_pending_into(&mut ready);
        self.convert_all(ready)
    }

    /// Removes every buffered picture, ordered by `PicOrderCnt`.
    fn drain_pending_into(&mut self, out: &mut Vec<(Duration, YuvFrame)>) {
        self.pending.sort_by_key(|(poc, _)| *poc);
        let mut pending = std::mem::take(&mut self.pending);
        out.extend(
            pending
                .drain(..)
                .map(|(_, frame)| (self.take_slot(), frame)),
        );
        // Reuse the allocation for the next GOP.
        self.pending = pending;
    }

    /// Removes the picture with the smallest `PicOrderCnt`.
    fn pop_earliest(&mut self) -> (Duration, YuvFrame) {
        let mut earliest = 0;
        for (index, (poc, _)) in self.pending.iter().enumerate() {
            if *poc < self.pending[earliest].0 {
                earliest = index;
            }
        }
        let (_, frame) = self.pending.swap_remove(earliest);
        (self.take_slot(), frame)
    }

    /// Takes the smallest unassigned timestamp.
    fn take_slot(&mut self) -> Duration {
        let mut earliest = 0;
        for (index, slot) in self.slots.iter().enumerate() {
            if *slot < self.slots[earliest] {
                earliest = index;
            }
        }
        self.slots.swap_remove(earliest)
    }

    fn convert_all(&mut self, frames: Vec<(Duration, YuvFrame)>) -> Vec<VideoFrame> {
        let mut output = Vec::with_capacity(frames.len());
        for (pts, frame) in frames {
            let width = u32::try_from(frame.width).unwrap_or_default();
            let height = u32::try_from(frame.height).unwrap_or_default();
            let mut pixels = Vec::new();
            convert(&frame, self.matrix, &mut pixels);
            self.decoder.recycle_frame(frame);
            output.push(VideoFrame {
                pts,
                width,
                height,
                pixels,
            });
        }
        output
    }
}
