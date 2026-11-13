//! `AVCDecoderConfigurationRecord` parsing and NAL framing conversion.
//!
//! ISO/MP4 and Matroska store H.264 access units with length-prefixed NAL
//! units ("AVCC" framing), while the decoder consumes Annex-B start codes. The
//! `avcC` record in the track header provides both the NAL length size and the
//! SPS/PPS parameter sets that the first keyframe needs.

use crate::error::{Error, Result};

/// A four-byte Annex-B start code. Three-byte start codes are accepted on
/// input, but every NAL this crate writes uses the four-byte form.
pub(crate) const START_CODE: [u8; 4] = [0, 0, 0, 1];

/// The parts of an `AVCDecoderConfigurationRecord` that a decoder needs.
#[derive(Clone, Debug)]
pub(crate) struct AvcConfig {
    nal_length_size: usize,
    parameter_sets: Box<[u8]>,
}

impl AvcConfig {
    /// Parses an `AVCDecoderConfigurationRecord`, as stored in an `avcC` box
    /// or in Matroska `CodecPrivate`.
    ///
    /// Trailing optional fields (SPS extensions, high-profile PPS extensions)
    /// are ignored; the record is self-delimiting before them.
    pub fn parse(data: &[u8]) -> Result<Self> {
        if data.len() < 7 {
            return Err(Error::InvalidConfig("record is shorter than 7 bytes"));
        }
        if data[0] != 1 {
            return Err(Error::InvalidConfig("unsupported configuration version"));
        }

        // Byte 4: 6 reserved bits, then `lengthSizeMinusOne`.
        let nal_length_size = usize::from(data[4] & 0x03) + 1;
        // Byte 5: 3 reserved bits, then the 5-bit SPS count.
        let sps_count = usize::from(data[5] & 0x1f);

        let mut parameter_sets = Vec::new();
        let mut offset = 6;
        offset = copy_parameter_sets(data, offset, sps_count, &mut parameter_sets)?;
        let pps_count = *data
            .get(offset)
            .ok_or(Error::InvalidConfig("missing PPS count"))?;
        offset += 1;
        let _ = copy_parameter_sets(data, offset, usize::from(pps_count), &mut parameter_sets)?;

        if parameter_sets.is_empty() {
            return Err(Error::InvalidConfig("record declares no parameter sets"));
        }

        Ok(Self {
            nal_length_size,
            parameter_sets: parameter_sets.into_boxed_slice(),
        })
    }

    /// Rewrites one packet into Annex-B framing in `out`.
    ///
    /// When `prepend_parameter_sets` is set, the SPS/PPS from the record are
    /// emitted first, which is required at every random-access point because
    /// length-prefixed packets do not repeat them in-band.
    ///
    /// Packets are length-prefixed by contract. A packet that does not form a
    /// consistent sequence of length-prefixed NAL units is accepted as
    /// Annex-B instead, because some muxers keep the stream in that form even
    /// when the track declares an `avcC` record.
    pub fn to_annex_b(
        &self,
        packet: &[u8],
        out: &mut Vec<u8>,
        prepend_parameter_sets: bool,
    ) -> Result<()> {
        out.clear();
        let parameters_len = if prepend_parameter_sets {
            out.extend_from_slice(&self.parameter_sets);
            self.parameter_sets.len()
        } else {
            0
        };

        if self.append_framed(packet, out) {
            return Ok(());
        }

        out.truncate(parameters_len);
        if annex_b_nal_header(packet).is_some() {
            out.extend_from_slice(packet);
            return Ok(());
        }

        Err(Error::InvalidPacket("NAL length exceeds the packet"))
    }

    /// Reports whether the packet contains an IDR slice.
    pub fn is_idr(&self, packet: &[u8]) -> bool {
        if let Some(idr) = self.framed_is_idr(packet) {
            return idr;
        }
        annex_b_has_nal_type(packet, NAL_TYPE_IDR)
    }

    /// Appends the packet's length-prefixed NAL units as Annex-B.
    ///
    /// Returns `false` unless the prefixes consume the packet exactly and at
    /// least one NAL is non-empty. This disambiguates a four-byte length prefix
    /// such as `00 00 01 b0` (432) from a three-byte Annex-B start code followed
    /// by a NAL header, which otherwise look identical at the start of a
    /// packet. Nothing is rolled back here; the caller truncates `out`.
    fn append_framed(&self, packet: &[u8], out: &mut Vec<u8>) -> bool {
        let mut offset = 0;
        let mut count = 0;
        while offset < packet.len() {
            let Some((start, length)) = self.next_nal(packet, &mut offset) else {
                return false;
            };
            if length > 0 {
                count += 1;
                out.extend_from_slice(&START_CODE);
                out.extend_from_slice(&packet[start..start + length]);
            }
        }
        count > 0
    }

    /// Returns whether the length-prefixed packet contains an IDR slice, or
    /// `None` when its framing is not a consistent sequence of NAL units.
    fn framed_is_idr(&self, packet: &[u8]) -> Option<bool> {
        let mut offset = 0;
        let mut count = 0;
        let mut idr = false;
        while offset < packet.len() {
            let (start, length) = self.next_nal(packet, &mut offset)?;
            if length > 0 {
                count += 1;
                idr |= packet[start] & 0x1f == NAL_TYPE_IDR;
            }
        }
        (count > 0).then_some(idr)
    }

    /// Reads one NAL length prefix and returns the payload offset and length.
    fn next_nal(&self, packet: &[u8], offset: &mut usize) -> Option<(usize, usize)> {
        let length_size = self.nal_length_size;
        let header_end = offset.checked_add(length_size)?;
        if header_end > packet.len() {
            return None;
        }
        let mut length = 0_usize;
        for byte in &packet[*offset..header_end] {
            length = (length << 8) | usize::from(*byte);
        }
        let end = header_end.checked_add(length)?;
        if end > packet.len() {
            return None;
        }
        *offset = end;
        Some((header_end, length))
    }
}

/// H.264 NAL unit type for a coded slice of an IDR picture.
const NAL_TYPE_IDR: u8 = 5;

fn copy_parameter_sets(
    data: &[u8],
    mut offset: usize,
    count: usize,
    out: &mut Vec<u8>,
) -> Result<usize> {
    for _ in 0..count {
        let length_bytes = data
            .get(offset..offset + 2)
            .ok_or(Error::InvalidConfig("truncated parameter set length"))?;
        offset += 2;
        let length = usize::from(u16::from_be_bytes([length_bytes[0], length_bytes[1]]));
        let end = offset
            .checked_add(length)
            .filter(|end| *end <= data.len())
            .ok_or(Error::InvalidConfig("parameter set exceeds the record"))?;
        out.extend_from_slice(&START_CODE);
        out.extend_from_slice(&data[offset..end]);
        offset = end;
    }
    Ok(offset)
}

/// Returns the Annex-B NAL header byte when `data` begins with a start code
/// and a valid NAL header.
///
/// The forbidden zero bit must be clear (H.264 §7.3.1). This check keeps a
/// four-byte length prefix such as `00 00 01 b0` from being mistaken for a
/// three-byte start code followed by a NAL header.
fn annex_b_nal_header(data: &[u8]) -> Option<u8> {
    let offset = if data.starts_with(&START_CODE) {
        4
    } else if data.starts_with(&[0, 0, 1]) {
        3
    } else {
        return None;
    };
    let header = *data.get(offset)?;
    if header & 0x80 == 0 {
        Some(header)
    } else {
        None
    }
}

fn annex_b_has_nal_type(data: &[u8], nal_type: u8) -> bool {
    let mut offset = 0;
    while offset + 3 <= data.len() {
        if data[offset] == 0 && data[offset + 1] == 0 && data[offset + 2] == 1 {
            let header = offset + 3;
            if header < data.len() && data[header] & 0x1f == nal_type {
                return true;
            }
            offset = header + 1;
        } else {
            offset += 1;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a minimal `avcC` with one SPS and one PPS. The payloads are not
    /// real bitstreams; the parser does not inspect them.
    fn record(nal_length_size_minus_one: u8, sps: &[u8], pps: &[u8]) -> Vec<u8> {
        let mut data = vec![1, 0x64, 0x00, 0x1f, 0xe0 | nal_length_size_minus_one, 0xe1];
        data.extend_from_slice(&u16::try_from(sps.len()).unwrap().to_be_bytes());
        data.extend_from_slice(sps);
        data.push(1);
        data.extend_from_slice(&u16::try_from(pps.len()).unwrap().to_be_bytes());
        data.extend_from_slice(pps);
        data
    }

    #[test]
    fn parses_parameter_sets() {
        let config = AvcConfig::parse(&record(3, &[0x67, 1, 2], &[0x68, 3])).unwrap();
        assert_eq!(config.nal_length_size, 4);
        let expected = [
            START_CODE.as_slice(),
            &[0x67, 1, 2],
            &START_CODE,
            &[0x68, 3],
        ]
        .concat();
        assert_eq!(config.parameter_sets.as_ref(), expected);
    }

    #[test]
    fn rejects_short_and_mismatched_records() {
        assert!(matches!(
            AvcConfig::parse(&[1, 2, 3]),
            Err(Error::InvalidConfig(_))
        ));
        let mut data = record(3, &[0x67], &[0x68]);
        data[0] = 2;
        assert!(matches!(
            AvcConfig::parse(&data),
            Err(Error::InvalidConfig(_))
        ));
        let mut data = record(3, &[0x67], &[0x68]);
        data.truncate(data.len() - 2);
        assert!(matches!(
            AvcConfig::parse(&data),
            Err(Error::InvalidConfig(_))
        ));
    }

    #[test]
    fn converts_length_prefixed_nals() {
        let config = AvcConfig::parse(&record(3, &[0x67], &[0x68])).unwrap();
        // Two NAL units, four-byte lengths: an IDR slice (type 5) and SEI.
        let packet = [0, 0, 0, 2, 0x65, 0xaa, 0, 0, 0, 1, 0x06];
        let mut out = Vec::new();
        config.to_annex_b(&packet, &mut out, false).unwrap();
        assert_eq!(out, [0, 0, 0, 1, 0x65, 0xaa, 0, 0, 0, 1, 0x06].to_vec());
        assert!(config.is_idr(&packet));

        config.to_annex_b(&packet, &mut out, true).unwrap();
        assert_eq!(
            out,
            [
                0, 0, 0, 1, 0x67, 0, 0, 0, 1, 0x68, 0, 0, 0, 1, 0x65, 0xaa, 0, 0, 0, 1, 0x06
            ]
            .to_vec()
        );
    }

    #[test]
    fn does_not_confuse_a_nal_length_with_a_start_code() {
        let config = AvcConfig::parse(&record(3, &[0x67], &[0x68])).unwrap();
        // A four-byte NAL length of 432 is `00 00 01 b0`, which begins with
        // the same three bytes as a three-byte Annex-B start code. A stream of
        // 256-511-byte NALs is common for small frames, so this ambiguity must
        // resolve in favour of the declared length-prefixed framing.
        let mut packet = vec![0, 0, 1, 0xb0];
        packet.resize(4 + 432, 0xaa);
        let mut out = Vec::new();
        config.to_annex_b(&packet, &mut out, false).unwrap();
        assert_eq!(out.len(), 4 + 432);
        assert_eq!(&out[..4], &START_CODE);
        assert!(out[4..].iter().all(|byte| *byte == 0xaa));
        assert!(!config.is_idr(&packet));
    }

    #[test]
    fn passes_annex_b_through() {
        let config = AvcConfig::parse(&record(3, &[0x67], &[0x68])).unwrap();
        let packet = [0, 0, 1, 0x65, 0x11];
        let mut out = Vec::new();
        config.to_annex_b(&packet, &mut out, false).unwrap();
        assert_eq!(out, packet.to_vec());
        assert!(config.is_idr(&packet));
        assert!(!config.is_idr(&[0, 0, 1, 0x41, 0x11]));
    }

    #[test]
    fn detects_truncated_packets() {
        let config = AvcConfig::parse(&record(3, &[0x67], &[0x68])).unwrap();
        let mut out = Vec::new();
        assert!(matches!(
            config.to_annex_b(&[0, 0, 0, 8, 0x65], &mut out, false),
            Err(Error::InvalidPacket(_))
        ));
        assert!(matches!(
            config.to_annex_b(&[0, 0], &mut out, false),
            Err(Error::InvalidPacket(_))
        ));
    }
}
