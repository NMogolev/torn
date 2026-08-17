use core::fmt;

/// A single unpremultiplied 8-bit sRGBA pixel.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Pixel {
    /// Red component.
    pub red: u8,
    /// Green component.
    pub green: u8,
    /// Blue component.
    pub blue: u8,
    /// Alpha component.
    pub alpha: u8,
}

impl Pixel {
    /// A fully transparent black pixel.
    pub const TRANSPARENT: Self = Self::rgba(0, 0, 0, 0);

    /// Creates an 8-bit sRGBA pixel.
    #[must_use]
    pub const fn rgba(red: u8, green: u8, blue: u8, alpha: u8) -> Self {
        Self {
            red,
            green,
            blue,
            alpha,
        }
    }
}

/// An in-memory image buffer for deterministic rendering tests.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PixelBuffer {
    width: u32,
    height: u32,
    pixels: Vec<Pixel>,
}

impl PixelBuffer {
    /// Creates a transparent buffer with `width` by `height` pixels.
    ///
    /// # Errors
    ///
    /// Returns [`PixelBufferError::TooLarge`] when the dimensions overflow the
    /// addressable pixel count.
    pub fn new(width: u32, height: u32) -> Result<Self, PixelBufferError> {
        let pixel_count = usize::try_from(u64::from(width) * u64::from(height))
            .map_err(|_| PixelBufferError::TooLarge)?;

        Ok(Self {
            width,
            height,
            pixels: vec![Pixel::TRANSPARENT; pixel_count],
        })
    }

    /// Returns the image width in physical pixels.
    #[must_use]
    pub const fn width(&self) -> u32 {
        self.width
    }

    /// Returns the image height in physical pixels.
    #[must_use]
    pub const fn height(&self) -> u32 {
        self.height
    }

    /// Returns an immutable pixel at `(x, y)`, or `None` outside the buffer.
    #[must_use]
    pub fn get(&self, x: u32, y: u32) -> Option<Pixel> {
        self.index_of(x, y).map(|index| self.pixels[index])
    }

    /// Returns pixel data in row-major order.
    #[must_use]
    pub fn pixels(&self) -> &[Pixel] {
        &self.pixels
    }

    pub(crate) fn get_mut(&mut self, x: u32, y: u32) -> Option<&mut Pixel> {
        self.index_of(x, y).map(|index| &mut self.pixels[index])
    }

    fn index_of(&self, x: u32, y: u32) -> Option<usize> {
        (x < self.width && y < self.height)
            .then(|| usize::try_from(u64::from(y) * u64::from(self.width) + u64::from(x)).ok())
            .flatten()
    }
}

/// Why a [`PixelBuffer`] could not be allocated.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PixelBufferError {
    /// The requested dimensions exceed the addressable pixel count.
    TooLarge,
}

impl fmt::Display for PixelBufferError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("pixel buffer dimensions are too large")
    }
}

impl std::error::Error for PixelBufferError {}
