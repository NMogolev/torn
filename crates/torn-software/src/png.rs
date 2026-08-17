use crate::{Pixel, PixelBuffer, PngError};

const PNG_SIGNATURE: [u8; 8] = [137, 80, 78, 71, 13, 10, 26, 10];
const IHDR: [u8; 4] = *b"IHDR";
const IDAT: [u8; 4] = *b"IDAT";
const IEND: [u8; 4] = *b"IEND";

pub(crate) fn encode(buffer: &PixelBuffer) -> Result<Vec<u8>, PngError> {
    if buffer.width() == 0 || buffer.height() == 0 {
        return Err(PngError::InvalidDimensions);
    }

    let scanlines = scanlines(buffer)?;
    let compressed = zlib_store(&scanlines);

    let mut png = Vec::new();
    png.extend_from_slice(&PNG_SIGNATURE);

    let mut header = [0_u8; 13];
    header[..4].copy_from_slice(&buffer.width().to_be_bytes());
    header[4..8].copy_from_slice(&buffer.height().to_be_bytes());
    header[8] = 8;
    header[9] = 6;
    write_chunk(&mut png, IHDR, &header);

    for chunk in compressed.chunks(u32::MAX as usize) {
        write_chunk(&mut png, IDAT, chunk);
    }
    write_chunk(&mut png, IEND, &[]);

    Ok(png)
}

fn scanlines(buffer: &PixelBuffer) -> Result<Vec<u8>, PngError> {
    let width = usize::try_from(buffer.width()).map_err(|_| PngError::DataTooLarge)?;
    let height = usize::try_from(buffer.height()).map_err(|_| PngError::DataTooLarge)?;
    let row_length = width.checked_mul(4).ok_or(PngError::DataTooLarge)?;
    let length = row_length
        .checked_add(1)
        .and_then(|row_length| row_length.checked_mul(height))
        .ok_or(PngError::DataTooLarge)?;

    let mut scanlines = Vec::with_capacity(length);
    for row in buffer.pixels().chunks_exact(width) {
        scanlines.push(0);
        for pixel in row {
            scanlines.extend_from_slice(&rgba(*pixel));
        }
    }

    Ok(scanlines)
}

fn rgba(pixel: Pixel) -> [u8; 4] {
    [pixel.red, pixel.green, pixel.blue, pixel.alpha]
}

fn zlib_store(data: &[u8]) -> Vec<u8> {
    let mut zlib = Vec::with_capacity(data.len().saturating_add(data.len() / 65_535 * 5 + 11));
    zlib.extend_from_slice(&[0x78, 0x01]);

    for (index, block) in data.chunks(65_535).enumerate() {
        let is_final = index == (data.len() - 1) / 65_535;
        zlib.push(u8::from(is_final));

        let length = u16::try_from(block.len()).expect("DEFLATE stored block length is bounded");
        zlib.extend_from_slice(&length.to_le_bytes());
        zlib.extend_from_slice(&(!length).to_le_bytes());
        zlib.extend_from_slice(block);
    }

    zlib.extend_from_slice(&adler32(data).to_be_bytes());
    zlib
}

fn write_chunk(png: &mut Vec<u8>, kind: [u8; 4], data: &[u8]) {
    let length = u32::try_from(data.len()).expect("PNG chunk length is bounded by chunking");
    png.extend_from_slice(&length.to_be_bytes());
    png.extend_from_slice(&kind);
    png.extend_from_slice(data);

    let checksum_start = png.len() - data.len() - kind.len();
    png.extend_from_slice(&crc32(&png[checksum_start..]).to_be_bytes());
}

fn adler32(data: &[u8]) -> u32 {
    const MODULUS: u32 = 65_521;

    let mut a = 1_u32;
    let mut b = 0_u32;
    for byte in data {
        a = (a + u32::from(*byte)) % MODULUS;
        b = (b + a) % MODULUS;
    }

    (b << 16) | a
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = u32::MAX;
    for byte in data {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & (0_u32.wrapping_sub(crc & 1)));
        }
    }

    !crc
}

#[cfg(test)]
mod tests {
    use crate::{PixelBuffer, PngError};

    use super::{adler32, crc32};

    #[test]
    fn encodes_a_valid_rgba_png() {
        let buffer = PixelBuffer::new(1, 1).expect("small test buffer");

        let png = buffer.encode_png().expect("valid PNG");
        assert_eq!(&png[..8], &[137, 80, 78, 71, 13, 10, 26, 10]);
        assert_eq!(&png[8..12], &[0, 0, 0, 13]);
        assert_eq!(&png[12..16], b"IHDR");
        assert_eq!(&png[16..29], &[0, 0, 0, 1, 0, 0, 0, 1, 8, 6, 0, 0, 0]);
        assert_eq!(&png[29..33], &[31, 21, 196, 137]);

        let (idat, end) = chunk(&png, 33);
        assert_eq!(&png[37..41], b"IDAT");
        assert_eq!(crc32(&png[37..end - 4]).to_be_bytes(), png[end - 4..end]);
        assert_eq!(inflate_stored(idat), &[0, 0, 0, 0, 0]);

        let iend_start = end;
        let (_, end) = chunk(&png, iend_start);
        assert_eq!(&png[iend_start + 4..iend_start + 8], b"IEND");
        assert_eq!(end, png.len());
    }

    #[test]
    fn rejects_zero_sized_images() {
        let buffer = PixelBuffer::new(0, 1).expect("empty test buffer");

        assert!(matches!(
            buffer.encode_png(),
            Err(PngError::InvalidDimensions)
        ));
    }

    fn chunk(png: &[u8], offset: usize) -> (&[u8], usize) {
        let length = u32::from_be_bytes(png[offset..offset + 4].try_into().expect("chunk length"));
        let length = usize::try_from(length).expect("chunk length fits usize");
        let data_start = offset + 8;
        let end = data_start + length + 4;
        (&png[data_start..data_start + length], end)
    }

    fn inflate_stored(zlib: &[u8]) -> Vec<u8> {
        assert_eq!(&zlib[..2], &[0x78, 0x01]);

        let mut offset = 2;
        let mut data = Vec::new();
        loop {
            let final_block = zlib[offset] == 1;
            offset += 1;
            let length = u16::from_le_bytes(zlib[offset..offset + 2].try_into().expect("length"));
            offset += 2;
            let complement = u16::from_le_bytes(
                zlib[offset..offset + 2]
                    .try_into()
                    .expect("length complement"),
            );
            offset += 2;
            assert_eq!(complement, !length);

            let length = usize::from(length);
            data.extend_from_slice(&zlib[offset..offset + length]);
            offset += length;
            if final_block {
                break;
            }
        }

        assert_eq!(adler32(&data).to_be_bytes(), zlib[offset..offset + 4]);
        assert_eq!(offset + 4, zlib.len());
        data
    }
}
