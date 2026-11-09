use std::{
    collections::VecDeque,
    sync::{Arc, Mutex, OnceLock},
};

use fontdue::layout::LayoutSettings;
use torn_core::{Color, Point, Size};

use crate::font::{DEFAULT_DPI, Font, FontDatabase, FontDescriptor, FontError, FontFace};

const TEXT_LAYOUT_CACHE_CAPACITY: usize = 256;

/// Styling information used by a text shaper.
///
/// A text style combines a [`Font`] request with a paint color. The shaper
/// resolves the font request through its [`FontDatabase`], so family fallback,
/// weight, and style selection are identical during measurement and painting.
#[derive(Clone, Debug, PartialEq)]
pub struct TextStyle {
    font: Font,
    color: Color,
}

impl TextStyle {
    /// Creates a text style with the supplied pixel size and color.
    ///
    /// The font request has no family preference, so the shaper's default face
    /// is used. This preserves the original Torn API while the richer
    /// [`Self::new_with_font`] constructor is available for explicit fonts.
    #[must_use]
    pub fn new(font_size: f32, color: Color) -> Self {
        Self {
            font: Font::from_pixel_size(font_size),
            color,
        }
    }

    /// Creates a text style from an explicit font request and color.
    #[must_use]
    pub fn new_with_font(font: Font, color: Color) -> Self {
        Self { font, color }
    }

    /// Returns the color used to paint the text.
    #[must_use]
    pub const fn color(&self) -> Color {
        self.color
    }

    /// Returns the font request.
    #[must_use]
    pub const fn font(&self) -> &Font {
        &self.font
    }

    /// Returns the requested size in logical pixels at [`DEFAULT_DPI`].
    #[must_use]
    pub fn font_size(&self) -> f32 {
        self.font.size().pixels_at(DEFAULT_DPI)
    }

    /// Returns this style with a different paint color.
    #[must_use]
    pub fn with_color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    /// Returns this style with a different font request.
    #[must_use]
    pub fn with_font(mut self, font: Font) -> Self {
        self.font = font;
        self
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
    /// Creates an empty layout that remembers `text` and `color`.
    ///
    /// This is useful for widgets that defer shaping until they can query the
    /// runtime environment's font database.
    #[must_use]
    pub fn empty(text: &str, color: Color) -> Self {
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

    /// Returns the same layout painted with `color`.
    ///
    /// Glyph positions and metrics are unchanged, so this is cheap compared to
    /// shaping the text again.
    #[must_use]
    pub fn with_color(mut self, color: Color) -> Self {
        self.color = color;
        self
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

/// A small deterministic shaper backed by a [`FontDatabase`].
///
/// It performs family matching, glyph lookup, line breaking, placement, GPOS
/// pair kerning, and line metric calculation through `fontdue`. It does not
/// implement complex OpenType shaping or bidirectional reordering. All
/// registered faces are available as per-character fallbacks.
#[derive(Clone, Debug)]
pub struct FontdueTextShaper {
    database: FontDatabase,
    dpi: f32,
    layout_generation: u64,
    layouts: Arc<Mutex<TextLayoutCache>>,
}

#[derive(Debug, PartialEq)]
struct TextLayoutKey {
    text: String,
    font: Font,
    width: Option<u32>,
    dpi: u32,
    generation: u64,
}

#[derive(Debug, Default)]
struct TextLayoutCache {
    entries: VecDeque<(TextLayoutKey, Arc<TextLayout>)>,
}

impl PartialEq for FontdueTextShaper {
    fn eq(&self, other: &Self) -> bool {
        self.database == other.database && self.dpi == other.dpi
    }
}

impl FontdueTextShaper {
    /// Creates a shaper that uses `font` as its default face.
    #[must_use]
    pub fn new(font: FontFace) -> Self {
        Self {
            database: FontDatabase::with_default(font),
            dpi: DEFAULT_DPI,
            layout_generation: 0,
            layouts: Arc::new(Mutex::new(TextLayoutCache::default())),
        }
    }

    /// Creates a shaper from an existing font database.
    ///
    /// The first face becomes the default when the database does not already
    /// select one.
    #[must_use]
    pub fn from_database(mut database: FontDatabase) -> Self {
        if database.default_face().is_none() && !database.is_empty() {
            database.set_default_face(0);
        }
        Self {
            database,
            dpi: DEFAULT_DPI,
            layout_generation: 0,
            layouts: Arc::new(Mutex::new(TextLayoutCache::default())),
        }
    }

    /// Adds fallback faces used when the requested families lack a character.
    ///
    /// Faces are consulted in insertion order after the requested family.
    #[must_use]
    pub fn with_fallbacks(
        font: FontFace,
        fallback_fonts: impl IntoIterator<Item = FontFace>,
    ) -> Self {
        let mut database = FontDatabase::with_default(font);
        for fallback in fallback_fonts {
            database.add_face(fallback);
        }
        Self {
            database,
            dpi: DEFAULT_DPI,
            layout_generation: 0,
            layouts: Arc::new(Mutex::new(TextLayoutCache::default())),
        }
    }

    /// Returns the shaper's font database.
    #[must_use]
    pub const fn database(&self) -> &FontDatabase {
        &self.database
    }

    /// Returns mutable access to the shaper's font database.
    pub fn database_mut(&mut self) -> &mut FontDatabase {
        self.layout_generation = self.layout_generation.saturating_add(1);
        &mut self.database
    }

    /// Parses and registers a face in the shaper's database.
    ///
    /// # Errors
    ///
    /// Returns [`FontError`] when `bytes` do not contain a supported font.
    pub fn add_font(
        &mut self,
        descriptor: FontDescriptor,
        bytes: impl Into<Vec<u8>>,
    ) -> Result<FontFace, FontError> {
        let face = self.database.add_font(descriptor, bytes)?;
        self.layout_generation = self.layout_generation.saturating_add(1);
        Ok(face)
    }

    /// Returns the default face used when no family matches.
    #[must_use]
    pub fn primary_font(&self) -> Option<&FontFace> {
        self.database.default_face()
    }

    /// Returns the logical DPI used to resolve point sizes.
    #[must_use]
    pub const fn dpi(&self) -> f32 {
        self.dpi
    }

    /// Updates the logical DPI used to resolve point sizes.
    ///
    /// # Panics
    ///
    /// Panics when `dpi` is not finite or is not positive.
    pub fn set_dpi(&mut self, dpi: f32) {
        assert!(
            dpi.is_finite() && dpi > 0.0,
            "font DPI must be finite and positive"
        );
        self.dpi = dpi;
    }

    /// Creates a shaper using Torn's bundled Ubuntu Light font.
    #[must_use]
    pub fn ubuntu_light() -> Self {
        static UBUNTU_LIGHT: OnceLock<FontdueTextShaper> = OnceLock::new();

        UBUNTU_LIGHT
            .get_or_init(|| Self::from_database(FontDatabase::bundled()))
            .clone()
    }

    /// Shapes `text` using this shaper's font database.
    #[must_use]
    pub fn layout(&self, text: &str, style: &TextStyle, width: Option<f32>) -> TextLayout {
        <Self as TextShaper>::layout(self, text, style, width)
    }
}

impl FontdueTextShaper {
    fn cached_layout(&self, text: &str, style: &TextStyle, width: Option<f32>) -> TextLayout {
        let key = TextLayoutKey {
            text: text.to_owned(),
            font: style.font().clone(),
            width: width.map(f32::to_bits),
            dpi: self.dpi.to_bits(),
            generation: self.layout_generation,
        };
        let mut cache = self
            .layouts
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some((_, layout)) = cache.entries.iter().find(|(entry, _)| entry == &key) {
            return layout.as_ref().clone().with_color(style.color());
        }
        let layout = self.shape(text, style, width);
        let layout = Arc::new(layout);
        if cache.entries.len() == TEXT_LAYOUT_CACHE_CAPACITY {
            cache.entries.pop_front();
        }
        cache.entries.push_back((key, Arc::clone(&layout)));
        layout.as_ref().clone().with_color(style.color())
    }

    fn shape(&self, text: &str, style: &TextStyle, width: Option<f32>) -> TextLayout {
        let font_size = style.font.pixel_size(self.dpi);
        if text.is_empty() || !font_size.is_finite() || font_size <= 0.0 {
            return TextLayout::empty(text, style.color);
        }

        let faces = self.database.resolve(&style.font);
        if faces.is_empty() {
            return TextLayout::empty(text, style.color);
        }

        let normalized_text = text.replace("\r\n", "\n").replace('\r', "\n");
        let prepared_text = prepare_text(&faces, &normalized_text);
        let font_refs = faces.iter().map(FontFace::font).collect::<Vec<_>>();
        let mut layout =
            fontdue::layout::Layout::new(fontdue::layout::CoordinateSystem::PositiveYDown);
        let max_width = width.filter(|value| value.is_finite() && *value >= 0.0);
        layout.reset(&LayoutSettings {
            max_width,
            ..LayoutSettings::default()
        });
        for (font_index, segment) in prepared_text {
            layout.append(
                &font_refs,
                &fontdue::layout::TextStyle::new(&segment, font_size, font_index),
            );
        }

        let parsed_faces = faces
            .iter()
            .map(|face| {
                if face.has_gpos_kerning() {
                    face.parsed()
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        let mut placed = layout
            .glyphs()
            .iter()
            .map(|glyph| PlacedGlyph {
                font_index: glyph.font_index,
                glyph_id: glyph.key.glyph_index,
                x: glyph.x,
                y: glyph.y,
                rasterize: glyph.char_data.rasterize(),
            })
            .collect::<Vec<_>>();
        apply_kerning(
            &mut placed,
            layout.lines().map(Vec::as_slice),
            &faces,
            &parsed_faces,
            font_size,
        );

        let mut glyph_runs = Vec::new();
        for glyph in placed.iter().filter(|glyph| glyph.rasterize) {
            let font = faces[glyph.font_index].clone();
            let positioned = PositionedGlyph {
                glyph_id: glyph.glyph_id,
                position: Point::new(glyph.x, glyph.y),
            };
            if let Some(run) = glyph_runs
                .last_mut()
                .filter(|run: &&mut GlyphRun| run.font == font)
            {
                run.glyphs.push(positioned);
            } else {
                glyph_runs.push(GlyphRun {
                    font,
                    font_size,
                    glyphs: vec![positioned],
                });
            }
        }
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
        let size = Size::new(measured_width(&placed, &faces, font_size), layout.height())
            .expect("fontdue produces finite non-negative layout metrics");

        TextLayout {
            text: text.to_owned(),
            size,
            color: style.color,
            glyph_runs,
            lines,
        }
    }
}

impl TextShaper for FontdueTextShaper {
    fn layout(&self, text: &str, style: &TextStyle, width: Option<f32>) -> TextLayout {
        self.cached_layout(text, style, width)
    }
}

fn prepare_text(faces: &[FontFace], text: &str) -> Vec<(usize, String)> {
    let mut segments = Vec::<(usize, String)>::new();
    for character in text.chars() {
        let (font_index, character) = if character.is_control() {
            (0, character)
        } else if let Some(font_index) = faces.iter().position(|font| font.has_glyph(character)) {
            (font_index, character)
        } else {
            (0, '?')
        };
        if let Some((previous_index, segment)) = segments.last_mut()
            && *previous_index == font_index
        {
            segment.push(character);
        } else {
            segments.push((font_index, character.to_string()));
        }
    }
    segments
}

/// A glyph emitted by fontdue with mutable horizontal placement.
#[derive(Clone, Copy, Debug)]
struct PlacedGlyph {
    font_index: usize,
    glyph_id: u16,
    x: f32,
    y: f32,
    rasterize: bool,
}

/// Shifts glyphs horizontally to apply pair kerning within each visual line.
///
/// Each pair's advance adjustment accumulates into a running offset for the
/// remainder of the line, while its placement adjustment moves only the second
/// glyph. Kerning is only applied between glyphs that share a font; fontdue's
/// line ranges keep it from crossing a line break.
fn apply_kerning(
    glyphs: &mut [PlacedGlyph],
    lines: Option<&[fontdue::layout::LinePosition]>,
    faces: &[FontFace],
    parsed_faces: &[Option<ttf_parser::Face<'_>>],
    font_size: f32,
) {
    let Some(lines) = lines else {
        return;
    };
    let Some(last) = glyphs.len().checked_sub(1) else {
        return;
    };
    for line in lines {
        let start = line.glyph_start.min(glyphs.len());
        let end = line.glyph_end.min(last);
        if start >= end {
            continue;
        }
        let mut shift = 0.0;
        for index in start..end {
            let left = glyphs[index];
            let right = glyphs[index + 1];
            if left.font_index == right.font_index
                && let Some(adjustment) = faces[left.font_index].horizontal_kerning(
                    parsed_faces[left.font_index].as_ref(),
                    left.glyph_id,
                    right.glyph_id,
                    font_size,
                )
            {
                shift += adjustment.advance;
                glyphs[index + 1].x += adjustment.placement;
            }
            glyphs[index + 1].x += shift;
        }
    }
}

fn measured_width(glyphs: &[PlacedGlyph], faces: &[FontFace], font_size: f32) -> f32 {
    glyphs
        .iter()
        .filter(|glyph| glyph.rasterize)
        .map(|glyph| {
            let metrics = faces[glyph.font_index]
                .font()
                .metrics_indexed(glyph.glyph_id, font_size);
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

    use super::{FontdueTextShaper, GlyphRun, TextStyle};
    use crate::font::{Font, FontFace, FontStyle, FontWeight};

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
    fn applies_gpos_kerning_between_adjacent_glyphs() {
        let layout = FontdueTextShaper::ubuntu_light().layout(
            "AV",
            &TextStyle::new(16.0, Color::BLACK),
            None,
        );
        let glyphs = layout.glyph_runs()[0].glyphs();
        assert_eq!(glyphs.len(), 2);

        let face = layout.glyph_runs()[0].font();
        let left = face.font().metrics_indexed(glyphs[0].glyph_id(), 16.0);
        let right = face.font().metrics_indexed(glyphs[1].glyph_id(), 16.0);
        let unkerned =
            (left.advance_width.ceil() + right.bounds.xmin).floor() - left.bounds.xmin.floor();
        let actual = glyphs[1].position().x - glyphs[0].position().x;

        assert!(
            actual < unkerned,
            "kerning should tighten AV: {actual} < {unkerned}"
        );
    }

    #[test]
    fn does_not_apply_kerning_across_line_breaks() {
        let shaper = FontdueTextShaper::ubuntu_light();
        let style = TextStyle::new(16.0, Color::BLACK);
        let with_a = shaper.layout("A\nV", &style, None);
        let without_a = shaper.layout("V\nV", &style, None);

        let second_line_v = |layout: &super::TextLayout| {
            layout
                .glyph_runs()
                .iter()
                .flat_map(GlyphRun::glyphs)
                .nth(1)
                .expect("the second line contains V")
                .position()
                .x
        };

        let with_a_x = second_line_v(&with_a);
        let without_a_x = second_line_v(&without_a);
        assert!(
            (with_a_x - without_a_x).abs() < f32::EPSILON,
            "the line break must reset kerning: {with_a_x} != {without_a_x}"
        );
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

    #[test]
    fn reuses_cached_geometry_when_only_the_text_color_changes() {
        let shaper = FontdueTextShaper::new(FontFace::ubuntu_light());
        let black = shaper.layout("Torn", &TextStyle::new(16.0, Color::BLACK), None);
        let white = shaper.layout("Torn", &TextStyle::new(16.0, Color::WHITE), None);
        let cache = shaper
            .layouts
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        assert_eq!(black.size(), white.size());
        assert_eq!(white.color(), Color::WHITE);
        assert_eq!(cache.entries.len(), 1);
    }

    #[test]
    fn handles_explicit_line_breaks_without_emitting_notdef_glyphs() {
        let layout = FontdueTextShaper::ubuntu_light().layout(
            "first\nsecond",
            &TextStyle::new(16.0, Color::BLACK),
            None,
        );

        assert_eq!(layout.lines().len(), 2);
        assert!(
            layout
                .glyph_runs()
                .iter()
                .flat_map(GlyphRun::glyphs)
                .all(|glyph| glyph.glyph_id() != 0)
        );
    }

    #[test]
    fn treats_windows_line_endings_as_single_line_breaks() {
        let layout = FontdueTextShaper::ubuntu_light().layout(
            "first\r\nsecond\rthird",
            &TextStyle::new(16.0, Color::BLACK),
            None,
        );

        assert_eq!(layout.lines().len(), 3);
    }

    #[test]
    fn replaces_unsupported_characters_instead_of_rendering_notdef_glyphs() {
        let layout = FontdueTextShaper::ubuntu_light().layout(
            "Torn 🦀",
            &TextStyle::new(16.0, Color::BLACK),
            None,
        );

        assert!(
            layout
                .glyph_runs()
                .iter()
                .flat_map(GlyphRun::glyphs)
                .all(|glyph| glyph.glyph_id() != 0)
        );
    }

    #[test]
    fn uses_the_requested_font_weight_and_style_when_available() {
        let style = TextStyle::new_with_font(
            Font::from_family("Ubuntu")
                .with_weight(FontWeight::BOLD)
                .with_style(FontStyle::Italic),
            Color::BLACK,
        );
        let layout = FontdueTextShaper::ubuntu_light().layout("Font", &style, None);

        assert_eq!(layout.glyph_runs()[0].font().family().as_str(), "Ubuntu");
        assert_eq!(layout.glyph_runs()[0].font().weight(), FontWeight::LIGHT);
    }
}
