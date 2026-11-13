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
    pub fn to_annex_b(
        &self,
        packet: &[u8],
        out: &mut Vec<u8>,
        prepend_parameter_sets: bool,
    ) -> Result<()> {
        out.clear();
        if prepend_parameter_sets {
            out.extend_from_slice(&self.parameter_sets);
        }

        // Some muxers keep the packets in Annex-B form even inside a
        // length-prefixed container. Pass those through untouched.
        if starts_with_start_code(packet) {
            out.extend_from_slice(packet);
            return Ok(());
        }

        let length_size = self.nal_length_size;
        let mut offset = 0;
        while offset < packet.len() {
            if offset + length_size > packet.len() {
                return Err(Error::InvalidPacket("truncated NAL length prefix"));
            }
            let mut length = 0_usize;
            for byte in &packet[offset..offset + length_size] {
                length = (length << 8) | usize::from(*byte);
            }
            offset += length_size;

            let end = offset
                .checked_add(length)
                .filter(|end| *end <= packet.len())
                .ok_or(Error::InvalidPacket("NAL length exceeds the packet"))?;
            if length == 0 {
                // Tolerate empty NAL units; they carry no data.
                continue;
            }
            out.extend_from_slice(&START_CODE);
            out.extend_from_slice(&packet[offset..end]);
            offset = end;
        }

        Ok(())
    }

    /// Reports whether the packet contains an IDR slice.
    pub fn is_idr(&self, packet: &[u8]) -> bool {
        if starts_with_start_code(packet) {
            return annex_b_has_nal_type(packet, NAL_TYPE_IDR);
        }
        self.framed_has_nal_type(packet, NAL_TYPE_IDR)
    }

    fn framed_has_nal_type(&self, packet: &[u8], nal_type: u8) -> bool {
        let length_size = self.nal_length_size;
        let Some(header_len) = length_size.checked_add(1) else {
            return false;
        };
        let mut offset = 0;
        while offset + header_len <= packet.len() {
            let mut length = 0_usize;
            for byte in &packet[offset..offset + length_size] {
                length = (length << 8) | usize::from(*byte);
            }
            offset += length_size;
            let Some(end) = offset.checked_add(length) else {
                return false;
            };
            if end > packet.len() {
                return false;
            }
            if length > 0 && packet[offset] & 0x1f == nal_type {
                return true;
            }
            offset = end;
        }
        false
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

/// Returns whether `data` starts with a three- or four-byte Annex-B start code.
pub(crate) fn starts_with_start_code(data: &[u8]) -> bool {
    data.starts_with(&[0, 0, 1]) || data.starts_with(&START_CODE)
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
