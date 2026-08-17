use torn_core::{Color, Size};

/// Styling information used by a text shaper.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextStyle {
    /// Requested font size in logical pixels.
    pub font_size: f32,
    /// Color used to paint the text.
    pub color: Color,
}

impl TextStyle {
    /// Creates a text style with the supplied font size and color.
    #[must_use]
    pub const fn new(font_size: f32, color: Color) -> Self {
        Self { font_size, color }
    }
}

/// Precomputed text metrics and rendering data.
///
/// The initial contract contains only the measured bounds and paint color.
/// Future versions add shaped glyph runs without changing paint commands to
/// accept raw strings.
#[derive(Clone, Debug, PartialEq)]
pub struct TextLayout {
    size: Size,
    color: Color,
}

impl TextLayout {
    /// Creates a text layout from already measured bounds and color.
    #[must_use]
    pub const fn new(size: Size, color: Color) -> Self {
        Self { size, color }
    }

    /// Returns the measured bounds of the layout.
    #[must_use]
    pub const fn size(&self) -> Size {
        self.size
    }

    /// Returns the color selected during shaping.
    #[must_use]
    pub const fn color(&self) -> Color {
        self.color
    }
}

/// Shapes and measures text independently from painting.
pub trait TextShaper {
    /// Shapes `text` using `style`, optionally wrapping it to `width` logical pixels.
    fn layout(&self, text: &str, style: &TextStyle, width: Option<f32>) -> TextLayout;
}
