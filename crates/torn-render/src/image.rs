use core::fmt;

/// An owned 8-bit sRGBA raster image in row-major order.
#[derive(Clone, Debug, PartialEq)]
pub struct RasterImage {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

impl RasterImage {
    /// Creates an image from unpremultiplied 8-bit sRGBA pixels.
    ///
    /// `pixels` must contain exactly four bytes per pixel in row-major order.
    ///
    /// # Errors
    ///
    /// Returns [`RasterImageError::InvalidPixelCount`] when the buffer length
    /// does not match the image dimensions, or [`RasterImageError::TooLarge`]
    /// when those dimensions cannot be represented in memory.
    pub fn new(width: u32, height: u32, pixels: Vec<u8>) -> Result<Self, RasterImageError> {
        let pixel_count = usize::try_from(u64::from(width) * u64::from(height))
            .map_err(|_| RasterImageError::TooLarge)?;
        let expected_length = pixel_count
            .checked_mul(4)
            .ok_or(RasterImageError::TooLarge)?;
        if pixels.len() != expected_length {
            return Err(RasterImageError::InvalidPixelCount {
                expected: expected_length,
                actual: pixels.len(),
            });
        }
        Ok(Self {
            width,
            height,
            pixels,
        })
    }

    /// Returns the image width in pixels.
    #[must_use]
    pub const fn width(&self) -> u32 {
        self.width
    }

    /// Returns the image height in pixels.
    #[must_use]
    pub const fn height(&self) -> u32 {
        self.height
    }

    /// Returns the unpremultiplied sRGBA pixels in row-major order.
    #[must_use]
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }
}

/// Why constructing a [`RasterImage`] failed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RasterImageError {
    /// The dimensions exceed the addressable pixel count.
    TooLarge,
    /// The supplied pixel buffer does not have four bytes for every pixel.
    InvalidPixelCount {
        /// Required number of bytes.
        expected: usize,
        /// Number of supplied bytes.
        actual: usize,
    },
}

impl fmt::Display for RasterImageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooLarge => formatter.write_str("raster image dimensions are too large"),
            Self::InvalidPixelCount { expected, actual } => {
                write!(
                    formatter,
                    "raster image requires {expected} pixel bytes, got {actual}"
                )
            }
        }
    }
}

impl std::error::Error for RasterImageError {}

#[cfg(test)]
mod tests {
    use super::{RasterImage, RasterImageError};

    #[test]
    fn validates_rgba_pixel_count() {
        assert_eq!(
            RasterImage::new(2, 1, vec![0; 7]),
            Err(RasterImageError::InvalidPixelCount {
                expected: 8,
                actual: 7,
            })
        );
    }
}
