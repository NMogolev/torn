use std::{
    fmt,
    sync::{Arc, OnceLock},
};

use fontdue::{Font, FontSettings};
use torn_core::{Color, Point, Size};

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

/// An immutable font resource used by shaped glyph runs.
#[derive(Clone)]
pub struct FontFace {
    inner: Arc<Font>,
}

impl FontFace {
    /// Parses a TrueType or OpenType font from its complete binary contents.
    ///
    /// # Errors
    ///
    /// Returns [`FontError`] when `bytes` do not contain a supported font.
    pub fn from_bytes(bytes: impl Into<Vec<u8>>) -> Result<Self, FontError> {
        Font::from_bytes(bytes.into(), FontSettings::default())
            .map(|inner| Self {
                inner: Arc::new(inner),
            })
            .map_err(FontError)
    }

    /// Returns Torn's bundled Ubuntu Light font.
    ///
    /// The font is licensed under the Ubuntu Font Licence 1.0; its full text is
    /// included alongside the font asset in this crate.
    ///
    /// # Panics
    ///
    /// Panics if the bundled font asset is corrupt. This is a build-time
    /// invariant maintained with the crate source.
    #[must_use]
    pub fn ubuntu_light() -> Self {
        static UBUNTU_LIGHT: OnceLock<FontFace> = OnceLock::new();

        UBUNTU_LIGHT
            .get_or_init(|| {
                Self::from_bytes(include_bytes!("../assets/Ubuntu-Light.ttf").as_slice())
                    .expect("the bundled Ubuntu Light font must be valid")
            })
            .clone()
    }

    /// Rasterizes one glyph to an 8-bit coverage bitmap.
    #[must_use]
    pub fn rasterize(&self, glyph_id: u16, font_size: f32) -> GlyphBitmap {
        let (metrics, coverage) = self.inner.rasterize_indexed(glyph_id, font_size);
        GlyphBitmap {
            width: metrics.width,
            height: metrics.height,
            coverage,
        }
    }

    fn font(&self) -> &Font {
        &self.inner
    }
}

impl fmt::Debug for FontFace {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("FontFace")
            .field("file_hash", &self.inner.file_hash())
            .field("name", &self.inner.name())
            .finish()
    }
}

impl PartialEq for FontFace {
    fn eq(&self, other: &Self) -> bool {
        self.inner.file_hash() == other.inner.file_hash()
    }
}

/// A font parsing failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FontError(&'static str);

impl fmt::Display for FontError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "could not parse font: {}", self.0)
    }
}

impl std::error::Error for FontError {}

/// A rasterized glyph's alpha coverage, arranged in row-major order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GlyphBitmap {
    width: usize,
    height: usize,
    coverage: Vec<u8>,
}

impl GlyphBitmap {
    /// Returns the bitmap width in pixels.
    #[must_use]
    pub const fn width(&self) -> usize {
        self.width
    }

    /// Returns the bitmap height in pixels.
    #[must_use]
    pub const fn height(&self) -> usize {
        self.height
    }

    /// Returns row-major 8-bit glyph coverage, from transparent to opaque.
    #[must_use]
    pub fn coverage(&self) -> &[u8] {
        &self.coverage
    }
}

/// A glyph positioned relative to the top-left layout origin.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PositionedGlyph {
    glyph_id: u16,
    position: Point,
}

impl PositionedGlyph {
    /// Returns the font-local glyph identifier.
    #[must_use]
    pub const fn glyph_id(self) -> u16 {
        self.glyph_id
    }

    /// Returns the top-left bitmap position relative to the layout origin.
    #[must_use]
    pub const fn position(self) -> Point {
        self.position
    }
}

/// A contiguous sequence of glyphs sharing a font and font size.
#[derive(Clone, Debug, PartialEq)]
pub struct GlyphRun {
    font: FontFace,
    font_size: f32,
    glyphs: Vec<PositionedGlyph>,
}

impl GlyphRun {
    /// Returns the font selected for this run.
    #[must_use]
    pub fn font(&self) -> &FontFace {
        &self.font
    }

    /// Returns the requested font size in logical pixels.
    #[must_use]
    pub const fn font_size(&self) -> f32 {
        self.font_size
    }

    /// Returns glyphs in visual drawing order.
    #[must_use]
    pub fn glyphs(&self) -> &[PositionedGlyph] {
        &self.glyphs
    }
}

/// Metrics for one laid-out text line.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LineMetrics {
    baseline: f32,
    ascent: f32,
    descent: f32,
    leading: f32,
    height: f32,
}

impl LineMetrics {
    /// Returns the baseline position relative to the layout origin.
    #[must_use]
    pub const fn baseline(self) -> f32 {
        self.baseline
    }

    /// Returns the distance above the baseline reserved by the line.
    #[must_use]
    pub const fn ascent(self) -> f32 {
        self.ascent
    }

    /// Returns the distance below the baseline reserved by the line.
    #[must_use]
    pub const fn descent(self) -> f32 {
        self.descent
    }

    /// Returns the font-recommended gap before the next line.
    #[must_use]
    pub const fn leading(self) -> f32 {
        self.leading
    }

    /// Returns this line's total height.
    #[must_use]
    pub const fn height(self) -> f32 {
        self.height
    }
}

/// Precomputed text content, metrics, and glyph placement for painting.
#[derive(Clone, Debug, PartialEq)]
pub struct TextLayout {
    text: String,
    size: Size,
    color: Color,
    glyph_runs: Vec<GlyphRun>,
    lines: Vec<LineMetrics>,
}

impl TextLayout {
    fn empty(text: &str, color: Color) -> Self {
        Self {
            text: text.to_owned(),
            size: Size::ZERO,
            color,
            glyph_runs: Vec::new(),
            lines: Vec::new(),
        }
    }

    /// Returns the original UTF-8 text that was laid out.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
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

    /// Returns positioned glyph runs in drawing order.
    #[must_use]
    pub fn glyph_runs(&self) -> &[GlyphRun] {
        &self.glyph_runs
    }

    /// Returns metrics for every visual line in the layout.
    #[must_use]
    pub fn lines(&self) -> &[LineMetrics] {
        &self.lines
    }
}

/// Shapes and measures text independently from painting.
pub trait TextShaper {
    /// Shapes `text` using `style`, optionally wrapping it to `width` logical pixels.
    fn layout(&self, text: &str, style: &TextStyle, width: Option<f32>) -> TextLayout;
}

/// A small deterministic shaper for one font face.
///
/// It performs glyph lookup, line breaking, placement, and line metric
/// calculation through `fontdue`. It intentionally does not implement kerning,
/// complex OpenType shaping, bidirectional reordering, or fallback font selection.
#[derive(Clone, Debug, PartialEq)]
pub struct FontdueTextShaper {
    font: FontFace,
}

impl FontdueTextShaper {
    /// Creates a shaper that uses `font` for every glyph.
    #[must_use]
    pub const fn new(font: FontFace) -> Self {
        Self { font }
    }

    /// Creates a shaper using Torn's bundled Ubuntu Light font.
    #[must_use]
    pub fn ubuntu_light() -> Self {
        Self::new(FontFace::ubuntu_light())
    }

    /// Shapes `text` using this shaper's font.
    #[must_use]
    pub fn layout(&self, text: &str, style: &TextStyle, width: Option<f32>) -> TextLayout {
        <Self as TextShaper>::layout(self, text, style, width)
    }
}

impl TextShaper for FontdueTextShaper {
    fn layout(&self, text: &str, style: &TextStyle, width: Option<f32>) -> TextLayout {
        if text.is_empty() || !style.font_size.is_finite() || style.font_size <= 0.0 {
            return TextLayout::empty(text, style.color);
        }

        let mut layout =
            fontdue::layout::Layout::new(fontdue::layout::CoordinateSystem::PositiveYDown);
        let max_width = width.filter(|value| value.is_finite() && *value >= 0.0);
        layout.reset(&fontdue::layout::LayoutSettings {
            max_width,
            ..fontdue::layout::LayoutSettings::default()
        });
        layout.append(
            &[self.font.font()],
            &fontdue::layout::TextStyle::new(text, style.font_size, 0),
        );

        let glyphs = layout
            .glyphs()
            .iter()
            .map(|glyph| PositionedGlyph {
                glyph_id: glyph.key.glyph_index,
                position: Point::new(glyph.x, glyph.y),
            })
            .collect();
        let lines = layout
            .lines()
            .into_iter()
            .flatten()
            .map(|line| LineMetrics {
                baseline: line.baseline_y,
                ascent: line.max_ascent,
                descent: -line.min_descent,
                leading: line.max_line_gap,
                height: line.max_new_line_size,
            })
            .collect::<Vec<_>>();
        let size = Size::new(
            measured_width(&layout, self.font.font(), style.font_size),
            layout.height(),
        )
        .expect("fontdue produces finite non-negative layout metrics");

        TextLayout {
            text: text.to_owned(),
            size,
            color: style.color,
            glyph_runs: vec![GlyphRun {
                font: self.font.clone(),
                font_size: style.font_size,
                glyphs,
            }],
            lines,
        }
    }
}

fn measured_width(layout: &fontdue::layout::Layout, font: &Font, font_size: f32) -> f32 {
    layout
        .glyphs()
        .iter()
        .map(|glyph| {
            let metrics = font.metrics_indexed(glyph.key.glyph_index, font_size);
            glyph.x - as_logical_coordinate(metrics.xmin) + metrics.advance_width
        })
        .fold(0.0, f32::max)
        .max(0.0)
}

fn as_logical_coordinate(value: i32) -> f32 {
    // Font raster bounds are practical pixel coordinates and therefore fit
    // within f32's exact integer range.
    #[allow(clippy::cast_precision_loss)]
    {
        value as f32
    }
}

#[cfg(test)]
mod tests {
    use torn_core::Color;

    use super::{FontdueTextShaper, TextStyle};

    #[test]
    fn shapes_text_into_glyphs_and_line_metrics() {
        let layout = FontdueTextShaper::ubuntu_light().layout(
            "Torn",
            &TextStyle::new(20.0, Color::BLACK),
            None,
        );

        assert_eq!(layout.text(), "Torn");
        assert_eq!(layout.glyph_runs().len(), 1);
        assert_eq!(layout.glyph_runs()[0].glyphs().len(), 4);
        assert_eq!(layout.lines().len(), 1);
        assert!(layout.size().width() > 0.0);
        assert!(layout.size().height() > 0.0);
        assert!(layout.lines()[0].baseline() > 0.0);
    }

    #[test]
    fn wraps_text_to_the_requested_width() {
        let shaper = FontdueTextShaper::ubuntu_light();
        let layout = shaper.layout(
            "one two three",
            &TextStyle::new(16.0, Color::BLACK),
            Some(30.0),
        );

        assert!(layout.lines().len() > 1);
        assert!(layout.size().height() > 16.0);
    }
}
