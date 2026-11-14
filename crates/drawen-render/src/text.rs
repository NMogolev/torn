use std::{
    collections::VecDeque,
    ops::Range,
    sync::{Arc, Mutex, OnceLock},
};

use drawen_core::{Color, Point, Rect, Size};
use harfrust::{
    BufferClusterLevel, Direction, FontRef, GlyphBuffer, ShapeOptions, ShaperData, UnicodeBuffer,
};
use unicode_bidi::{Level, ParagraphBidiInfo};
use unicode_segmentation::UnicodeSegmentation;

use crate::font::{DEFAULT_DPI, Font, FontDatabase, FontDescriptor, FontError, FontFace};

const TEXT_LAYOUT_CACHE_CAPACITY: usize = 256;
const SHAPING_FRACTION_BITS: f32 = 64.0;

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
    /// is used. This preserves the original Drawen API while the richer
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

    /// Returns the top edge of the line relative to the layout origin.
    #[must_use]
    pub fn top(self) -> f32 {
        self.baseline - self.ascent
    }

    /// Returns the bottom edge of the line relative to the layout origin.
    #[must_use]
    pub fn bottom(self) -> f32 {
        self.baseline + self.descent
    }
}

/// A grapheme-sized unit of source text and its visual caret geometry.
///
/// Clusters are emitted in visual order within each line. `source` always uses
/// UTF-8 byte offsets into [`TextLayout::text`], so callers never have to
/// convert between character and byte indices.
#[derive(Clone, Debug, PartialEq)]
pub struct TextCluster {
    source: Range<usize>,
    bounds: Rect,
    line: usize,
    level: u8,
}

impl TextCluster {
    /// Returns the source bytes covered by this cluster.
    #[must_use]
    pub fn source(&self) -> Range<usize> {
        self.source.clone()
    }

    /// Returns the visual bounds of the cluster.
    #[must_use]
    pub const fn bounds(&self) -> Rect {
        self.bounds
    }

    /// Returns the visual line containing this cluster.
    #[must_use]
    pub const fn line(&self) -> usize {
        self.line
    }

    /// Returns the Unicode bidirectional embedding level.
    #[must_use]
    pub const fn level(&self) -> u8 {
        self.level
    }

    /// Returns whether the cluster belongs to a right-to-left run.
    #[must_use]
    pub const fn is_rtl(&self) -> bool {
        self.level % 2 == 1
    }
}

/// Precomputed text content, metrics, glyph placement, and caret geometry.
#[derive(Clone, Debug, PartialEq)]
pub struct TextLayout {
    text: String,
    size: Size,
    color: Color,
    glyph_runs: Vec<GlyphRun>,
    lines: Vec<LineMetrics>,
    clusters: Vec<TextCluster>,
    line_ranges: Vec<Range<usize>>,
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
            clusters: Vec::new(),
            line_ranges: Vec::new(),
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

    /// Returns visual clusters in line and visual order.
    #[must_use]
    pub fn clusters(&self) -> &[TextCluster] {
        &self.clusters
    }

    /// Returns the source byte range represented by each visual line.
    #[must_use]
    pub fn line_ranges(&self) -> &[Range<usize>] {
        &self.line_ranges
    }

    /// Returns the visual line containing `byte`.
    #[must_use]
    pub fn line_at(&self, byte: usize) -> usize {
        let byte = byte.min(self.text.len());
        for (index, range) in self.line_ranges.iter().enumerate() {
            if byte < range.end {
                return index;
            }
            if byte == range.end
                && self
                    .line_ranges
                    .get(index + 1)
                    .is_some_and(|next| next.start == byte)
            {
                return index + 1;
            }
        }
        self.lines.len().saturating_sub(1)
    }

    /// Returns the visual line at layout-local `y`.
    #[must_use]
    pub fn line_at_y(&self, y: f32) -> usize {
        self.lines
            .iter()
            .position(|line| y < line.bottom())
            .unwrap_or_else(|| self.lines.len().saturating_sub(1))
    }

    /// Returns the caret x coordinate for a UTF-8 byte offset.
    #[must_use]
    pub fn caret_x(&self, byte: usize) -> f32 {
        if self.clusters.is_empty() {
            return 0.0;
        }
        let byte = byte.min(self.text.len());
        let first = &self.clusters[0];
        if byte <= first.source.start {
            return caret_edge(first, true);
        }
        for (index, cluster) in self.clusters.iter().enumerate() {
            if byte < cluster.source.end {
                return caret_edge(cluster, true);
            }
            if byte == cluster.source.end {
                if let Some(next) = self
                    .clusters
                    .get(index + 1)
                    .filter(|next| next.source.start == byte && next.line == cluster.line)
                {
                    return caret_edge(next, true);
                }
                return caret_edge(cluster, false);
            }
        }
        self.clusters
            .last()
            .map_or(0.0, |cluster| caret_edge(cluster, false))
    }

    /// Returns the caret rectangle for a UTF-8 byte offset using `width`.
    #[must_use]
    pub fn caret_rect(&self, byte: usize, width: f32) -> Rect {
        let line_index = self.line_at(byte);
        let Some(line) = self.lines.get(line_index) else {
            return Rect::new(Point::ZERO, Size::ZERO);
        };
        Rect::new(
            Point::new(self.caret_x(byte), line.top()),
            size(width.max(0.0), line.height()),
        )
    }

    /// Returns the closest source byte for a layout-local point.
    #[must_use]
    pub fn byte_at_point(&self, point: Point) -> usize {
        let line_index = self.line_at_y(point.y);
        let mut line_clusters = self
            .clusters
            .iter()
            .filter(|cluster| cluster.line == line_index)
            .peekable();
        let Some(first) = line_clusters.peek() else {
            return self
                .line_ranges
                .get(line_index)
                .map_or(0, |range| range.start);
        };
        let mut best = first.source.start;
        for cluster in line_clusters {
            let center = cluster.bounds.origin.x + cluster.bounds.size.width() / 2.0;
            if point.x < center {
                best = if cluster.is_rtl() {
                    cluster.source.end
                } else {
                    cluster.source.start
                };
                break;
            }
            best = if cluster.is_rtl() {
                cluster.source.start
            } else {
                cluster.source.end
            };
        }
        best
    }

    /// Returns one rectangle per visual line covering `selection`.
    ///
    /// Partial graphemes are intentionally expanded to whole grapheme bounds;
    /// this keeps carets and highlights aligned with user-perceived characters.
    #[must_use]
    pub fn selection_rects(&self, selection: Range<usize>) -> Vec<Rect> {
        if selection.start >= selection.end {
            return Vec::new();
        }
        let mut rects: Vec<Rect> = Vec::new();
        for cluster in &self.clusters {
            if cluster.source.end <= selection.start || cluster.source.start >= selection.end {
                continue;
            }
            let Some(line) = self.lines.get(cluster.line) else {
                continue;
            };
            let rect = Rect::new(
                Point::new(cluster.bounds.origin.x, line.top()),
                size(cluster.bounds.size.width().max(1.0), line.height()),
            );
            if let Some(previous) = rects.last_mut()
                && (previous.right() - rect.origin.x).abs() <= f32::EPSILON
                && (previous.origin.y - rect.origin.y).abs() <= f32::EPSILON
            {
                previous.size = size(
                    previous.size.width() + rect.size.width(),
                    previous.size.height(),
                );
                continue;
            }
            rects.push(rect);
        }
        rects
    }
}

fn caret_edge(cluster: &TextCluster, start: bool) -> f32 {
    let at_start = start != cluster.is_rtl();
    if at_start {
        cluster.bounds.origin.x
    } else {
        cluster.bounds.right()
    }
}

/// Shapes and measures text independently from painting.
pub trait TextShaper {
    /// Shapes `text` using `style`, optionally wrapping it to `width` logical pixels.
    fn layout(&self, text: &str, style: &TextStyle, width: Option<f32>) -> TextLayout;

    /// Returns cumulative text-layout cache counters for profiling.
    ///
    /// Shapers without an internal cache return zeroed counters.
    fn cache_stats(&self) -> TextLayoutCacheStats {
        TextLayoutCacheStats::default()
    }
}

/// A deterministic shaper backed by `HarfRust` and a [`FontDatabase`].
///
/// The shaper performs complex OpenType shaping, kerning, ligatures, mark
/// positioning, bidirectional reordering, per-grapheme fallback, line breaking,
/// and caret-cluster generation. The historical `Fontdue` name is retained for
/// API compatibility; `fontdue` remains the rasterizer used by renderers.
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
    hits: u64,
    misses: u64,
    evictions: u64,
}

/// Cumulative text-layout cache counters exposed for frame diagnostics.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TextLayoutCacheStats {
    /// Number of cached layouts returned.
    pub hits: u64,
    /// Number of layouts shaped because no matching entry was cached.
    pub misses: u64,
    /// Number of entries evicted by the bounded FIFO cache.
    pub evictions: u64,
    /// Number of entries currently retained.
    pub entries: usize,
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

    /// Returns cumulative cache counters for profiling.
    #[must_use]
    pub fn cache_stats(&self) -> TextLayoutCacheStats {
        let cache = self
            .layouts
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        TextLayoutCacheStats {
            hits: cache.hits,
            misses: cache.misses,
            evictions: cache.evictions,
            entries: cache.entries.len(),
        }
    }

    /// Creates a shaper using Drawen's bundled Ubuntu Light font.
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
        let hit = {
            let cache = self
                .layouts
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            cache
                .entries
                .iter()
                .find(|(entry, _)| entry == &key)
                .map(|(_, layout)| Arc::clone(layout))
        };
        if let Some(layout) = hit {
            let mut cache = self
                .layouts
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            cache.hits = cache.hits.saturating_add(1);
            return layout.as_ref().clone().with_color(style.color());
        }
        let layout = Arc::new(self.shape(text, style, width));
        let mut cache = self
            .layouts
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        cache.misses = cache.misses.saturating_add(1);
        if let Some((_, existing)) = cache.entries.iter().find(|(entry, _)| entry == &key) {
            let existing = Arc::clone(existing);
            cache.hits = cache.hits.saturating_add(1);
            return existing.as_ref().clone().with_color(style.color());
        }
        if cache.entries.len() == TEXT_LAYOUT_CACHE_CAPACITY {
            cache.entries.pop_front();
            cache.evictions = cache.evictions.saturating_add(1);
        }
        cache.entries.push_back((key, Arc::clone(&layout)));
        layout.as_ref().clone().with_color(style.color())
    }

    #[allow(clippy::too_many_lines)]
    fn shape(&self, text: &str, style: &TextStyle, width: Option<f32>) -> TextLayout {
        let font_size = style.font.pixel_size(self.dpi);
        if text.is_empty() || !font_size.is_finite() || font_size <= 0.0 {
            return TextLayout::empty(text, style.color);
        }

        let faces = self.database.resolve(&style.font);
        if faces.is_empty() {
            return TextLayout::empty(text, style.color);
        }

        let max_width = width.filter(|value| value.is_finite() && *value >= 0.0);
        let default_metrics = faces[0].metrics(font_size);
        let (ascent, descent, leading) = default_metrics
            .map_or((font_size * 0.8, font_size * 0.2, 0.0), |metrics| {
                (metrics.ascent(), metrics.descent(), metrics.leading())
            });
        let line_height = (ascent + descent + leading).max(1.0);

        let mut lines = Vec::new();
        let mut glyph_runs = Vec::new();
        let mut clusters = Vec::new();
        let mut line_ranges = Vec::new();
        let mut y = 0.0;

        for paragraph in paragraphs(text) {
            let paragraph_text = &text[paragraph.clone()];
            let bidi = ParagraphBidiInfo::new(paragraph_text, None);
            let graphemes = grapheme_ranges(paragraph_text);
            let widths = max_width.map_or_else(Vec::new, |_| {
                measure_graphemes(paragraph_text, &graphemes, &bidi, &faces, font_size)
            });
            let line_spans = max_width.map_or_else(
                || std::iter::once(0..paragraph_text.len()).collect(),
                |maximum| wrap_lines(paragraph_text, &graphemes, &widths, maximum),
            );

            for span in line_spans {
                let line_index = lines.len();
                let baseline = y + ascent;
                let metrics = LineMetrics {
                    baseline,
                    ascent,
                    descent,
                    leading,
                    height: line_height,
                };
                lines.push(metrics);
                line_ranges.push(paragraph.start + span.start..paragraph.start + span.end);

                let line_glyphs = shape_line(
                    paragraph_text,
                    &span,
                    &bidi,
                    &paragraph,
                    &faces,
                    font_size,
                    baseline,
                );
                for glyph in &line_glyphs {
                    let font = faces[glyph.font_index].clone();
                    if glyph.glyph_id == 0
                        || glyph.rasterize_width == 0
                        || glyph.rasterize_height == 0
                    {
                        continue;
                    }
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

                let line_clusters = line_text_clusters(
                    &line_glyphs,
                    &graphemes,
                    span,
                    paragraph.start,
                    line_index,
                    metrics,
                );
                clusters.extend(line_clusters);
                y += line_height;
            }
        }

        if lines.is_empty() {
            return TextLayout::empty(text, style.color);
        }

        let width = clusters
            .iter()
            .map(|cluster| cluster.bounds.right())
            .fold(0.0, f32::max)
            .max(0.0);
        let size = Size::new(width, y).unwrap_or(Size::ZERO);

        TextLayout {
            text: text.to_owned(),
            size,
            color: style.color,
            glyph_runs,
            lines,
            clusters,
            line_ranges,
        }
    }
}

impl TextShaper for FontdueTextShaper {
    fn layout(&self, text: &str, style: &TextStyle, width: Option<f32>) -> TextLayout {
        self.cached_layout(text, style, width)
    }

    fn cache_stats(&self) -> TextLayoutCacheStats {
        self.cache_stats()
    }
}

fn paragraphs(text: &str) -> Vec<Range<usize>> {
    let mut result = Vec::new();
    let bytes = text.as_bytes();
    let mut start = 0;
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'\r' => {
                result.push(start..index);
                index += 1;
                if index < bytes.len() && bytes[index] == b'\n' {
                    index += 1;
                }
                start = index;
            }
            b'\n' => {
                result.push(start..index);
                index += 1;
                start = index;
            }
            _ => index += 1,
        }
    }
    result.push(start..text.len());
    result
}

fn grapheme_ranges(text: &str) -> Vec<Range<usize>> {
    text.grapheme_indices(true)
        .map(|(start, grapheme)| start..start + grapheme.len())
        .collect()
}

fn measure_graphemes(
    text: &str,
    graphemes: &[Range<usize>],
    bidi: &ParagraphBidiInfo<'_>,
    faces: &[FontFace],
    font_size: f32,
) -> Vec<f32> {
    let mut widths = vec![0.0; graphemes.len()];
    if text.is_empty() {
        return widths;
    }
    let (_, runs) = bidi.visual_runs(0..text.len());
    for run in runs {
        let level = bidi.levels.get(run.start).copied().unwrap_or(Level::ltr());
        let (glyphs, _) = shape_level_run(text, run, level, faces, font_size, 0.0, 0.0);
        for glyph in glyphs {
            let Some(index) = grapheme_index(graphemes, glyph.source.start) else {
                continue;
            };
            widths[index] += glyph.advance.abs();
        }
    }
    widths
}

fn grapheme_index(graphemes: &[Range<usize>], byte: usize) -> Option<usize> {
    graphemes
        .binary_search_by(|range| {
            if byte < range.start {
                std::cmp::Ordering::Greater
            } else if byte >= range.end {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Equal
            }
        })
        .ok()
}

fn wrap_lines(
    text: &str,
    graphemes: &[Range<usize>],
    widths: &[f32],
    maximum: f32,
) -> Vec<Range<usize>> {
    if graphemes.is_empty() {
        return std::iter::once(0..0).collect();
    }
    let maximum = maximum.max(1.0);
    let mut lines = Vec::new();
    let mut start = 0;
    while start < graphemes.len() {
        let mut end = start;
        let mut x = 0.0;
        let mut last_break = None;
        while end < graphemes.len() {
            let width = widths.get(end).copied().unwrap_or(0.0);
            if x + width > maximum && end > start {
                end = last_break.filter(|value| *value > start).unwrap_or(end);
                break;
            }
            x += width;
            if text[graphemes[end].clone()]
                .chars()
                .any(char::is_whitespace)
            {
                last_break = Some(end + 1);
            }
            end += 1;
        }
        if end <= start {
            end = start + 1;
        }
        lines.push(graphemes[start].start..graphemes[end - 1].end);
        start = end;
    }
    lines
}

struct FontRun {
    font_index: usize,
    source: Range<usize>,
    text: String,
    replacement: bool,
}

fn font_runs(text: &str, range: Range<usize>, faces: &[FontFace]) -> Vec<FontRun> {
    let mut runs = Vec::<FontRun>::new();
    for grapheme in grapheme_ranges(&text[range.clone()]) {
        let source = range.start + grapheme.start..range.start + grapheme.end;
        let fragment = &text[source.clone()];
        let font_index = fragment
            .chars()
            .filter(|character| !character.is_control())
            .find_map(|character| faces.iter().position(|face| face.has_glyph(character)));
        let (font_index, text, replacement) = font_index.map_or_else(
            || (0, "?".repeat(fragment.chars().count()), true),
            |index| (index, fragment.to_owned(), false),
        );
        if let Some(previous) = runs.last_mut()
            && !replacement
            && !previous.replacement
            && previous.font_index == font_index
            && previous.source.end == source.start
        {
            previous.source.end = source.end;
            previous.text.push_str(&text);
            continue;
        }
        runs.push(FontRun {
            font_index,
            source,
            text,
            replacement,
        });
    }
    runs
}

#[derive(Clone, Debug)]
struct ShapedGlyph {
    font_index: usize,
    glyph_id: u16,
    x: f32,
    y: f32,
    source: Range<usize>,
    advance: f32,
    pen_start: f32,
    rasterize_width: usize,
    rasterize_height: usize,
    level: u8,
}

fn shape_level_run(
    text: &str,
    range: Range<usize>,
    level: Level,
    faces: &[FontFace],
    font_size: f32,
    x: f32,
    baseline: f32,
) -> (Vec<ShapedGlyph>, f32) {
    let rtl = level.is_rtl();
    let level = level.number();
    let runs = font_runs(text, range, faces);
    let mut result = Vec::new();
    let mut cursor = x;
    let ordered: Box<dyn Iterator<Item = &FontRun>> = if rtl {
        Box::new(runs.iter().rev())
    } else {
        Box::new(runs.iter())
    };
    for run in ordered {
        let shaped = shape_font_run(
            &faces[run.font_index],
            &run.text,
            font_size,
            rtl,
            run.source.clone(),
            run.replacement,
            run.font_index,
            baseline,
        );
        let advance = shaped.1;
        let start = if rtl { cursor + advance } else { cursor };
        for glyph in shaped.0 {
            result.push(ShapedGlyph {
                x: start + glyph.x,
                y: glyph.y,
                source: glyph.source,
                advance: glyph.advance,
                pen_start: start + glyph.pen_start,
                level,
                font_index: glyph.font_index,
                glyph_id: glyph.glyph_id,
                rasterize_width: glyph.rasterize_width,
                rasterize_height: glyph.rasterize_height,
            });
        }
        cursor += advance;
    }
    (result, cursor - x)
}

struct RawShapedGlyph {
    font_index: usize,
    glyph_id: u16,
    x: f32,
    y: f32,
    source: Range<usize>,
    advance: f32,
    pen_start: f32,
    rasterize_width: usize,
    rasterize_height: usize,
}

#[allow(clippy::too_many_arguments)]
fn shape_font_run(
    face: &FontFace,
    text: &str,
    font_size: f32,
    rtl: bool,
    source: Range<usize>,
    replacement: bool,
    font_index: usize,
    baseline: f32,
) -> (Vec<RawShapedGlyph>, f32) {
    let Ok(font_ref) = FontRef::from_index(face.data(), 0) else {
        return (Vec::new(), 0.0);
    };
    let data = ShaperData::new(&font_ref);
    let shaper = data.shaper(&font_ref).build();
    let mut buffer = UnicodeBuffer::new();
    buffer.push_str(text);
    buffer.set_direction(if rtl {
        Direction::RightToLeft
    } else {
        Direction::LeftToRight
    });
    buffer.guess_segment_properties();
    buffer.set_cluster_level(BufferClusterLevel::MonotoneCharacters);
    let scale = (font_size * SHAPING_FRACTION_BITS).round().max(1.0);
    #[allow(clippy::cast_possible_truncation)]
    let scale = scale as i32;
    let glyphs = shaper.shape(
        buffer,
        ShapeOptions::new()
            .scale(Some(scale))
            .point_size(Some(font_size)),
    );
    shape_buffer(
        &glyphs,
        face,
        &source,
        replacement,
        font_index,
        font_size,
        baseline,
    )
}

#[allow(clippy::cast_precision_loss)]
fn shape_buffer(
    buffer: &GlyphBuffer,
    face: &FontFace,
    source: &Range<usize>,
    replacement: bool,
    font_index: usize,
    font_size: f32,
    baseline: f32,
) -> (Vec<RawShapedGlyph>, f32) {
    let mut pen = 0.0;
    let mut result = Vec::new();
    let infos = buffer.glyph_infos();
    let positions = buffer.glyph_positions();
    for (info, position) in infos.iter().zip(positions) {
        let advance = position.x_advance as f32 / SHAPING_FRACTION_BITS;
        let offset = position.x_offset as f32 / SHAPING_FRACTION_BITS;
        let glyph_id = u16::try_from(info.glyph_id).unwrap_or(0);
        let metrics = face.font().metrics_indexed(glyph_id, font_size);
        let x = pen + offset + metrics.bounds.xmin.floor();
        let y = baseline + (-metrics.bounds.height - metrics.bounds.ymin).floor();
        let glyph_source = if replacement {
            source.clone()
        } else {
            let start = source
                .start
                .saturating_add(usize::try_from(info.cluster).unwrap_or(0))
                .min(source.end);
            start..source.end
        };
        result.push(RawShapedGlyph {
            font_index,
            glyph_id,
            x,
            y,
            source: glyph_source,
            advance,
            pen_start: pen + offset,
            rasterize_width: metrics.width,
            rasterize_height: metrics.height,
        });
        pen += advance;
    }
    (result, pen.abs())
}

fn shape_line(
    paragraph_text: &str,
    span: &Range<usize>,
    bidi: &ParagraphBidiInfo<'_>,
    paragraph: &Range<usize>,
    faces: &[FontFace],
    font_size: f32,
    baseline: f32,
) -> Vec<ShapedGlyph> {
    let (_, runs) = bidi.visual_runs(span.clone());
    let mut result = Vec::new();
    let mut x = 0.0;
    for run in runs {
        let level = bidi.levels.get(run.start).copied().unwrap_or(Level::ltr());
        let (glyphs, width) =
            shape_level_run(paragraph_text, run, level, faces, font_size, x, baseline);
        x += width.max(0.0);
        result.extend(glyphs.into_iter().map(|glyph| ShapedGlyph {
            source: paragraph.start + glyph.source.start..paragraph.start + glyph.source.end,
            ..glyph
        }));
    }
    result
}

fn line_text_clusters(
    glyphs: &[ShapedGlyph],
    graphemes: &[Range<usize>],
    span: Range<usize>,
    paragraph_start: usize,
    line: usize,
    metrics: LineMetrics,
) -> Vec<TextCluster> {
    let mut result = Vec::new();
    let mut cursor = 0.0;
    for grapheme in graphemes {
        if grapheme.end <= span.start || grapheme.start >= span.end {
            continue;
        }
        let start = paragraph_start + grapheme.start.max(span.start);
        let end = paragraph_start + grapheme.end.min(span.end);
        let mut left = f32::INFINITY;
        let mut right = f32::NEG_INFINITY;
        let mut level = 0_u8;
        for glyph in glyphs {
            if glyph.source.start < start || glyph.source.start >= end.max(start + 1) {
                continue;
            }
            let glyph_left = glyph.pen_start.min(glyph.pen_start + glyph.advance);
            let glyph_right = glyph.pen_start.max(glyph.pen_start + glyph.advance);
            left = left.min(glyph_left);
            right = right.max(glyph_right);
            level = glyph.level;
        }
        if !left.is_finite() || !right.is_finite() {
            left = cursor;
            right = cursor;
        }
        if right < left {
            std::mem::swap(&mut left, &mut right);
        }
        cursor = right.max(cursor);
        result.push(TextCluster {
            source: start..end,
            bounds: Rect::new(
                Point::new(left, metrics.top()),
                size((right - left).max(0.0), metrics.height),
            ),
            line,
            level,
        });
    }
    result
}

fn size(width: f32, height: f32) -> Size {
    Size::new(width, height).expect("text layout sizes are finite and non-negative")
}

#[cfg(test)]
mod tests {
    use drawen_core::Color;

    use super::{FontdueTextShaper, GlyphRun, TextStyle};
    use crate::font::{Font, FontFace, FontStyle, FontWeight};

    #[test]
    fn shapes_text_into_glyphs_and_line_metrics() {
        let layout = FontdueTextShaper::ubuntu_light().layout(
            "Drawen",
            &TextStyle::new(20.0, Color::BLACK),
            None,
        );

        assert_eq!(layout.text(), "Drawen");
        assert_eq!(layout.glyph_runs().len(), 1);
        assert_eq!(layout.glyph_runs()[0].glyphs().len(), 6);
        assert_eq!(layout.lines().len(), 1);
        assert!(layout.size().width() > 0.0);
        assert!(layout.size().height() > 0.0);
        assert!(layout.lines()[0].baseline() > 0.0);
        assert_eq!(layout.clusters().len(), 6);
    }

    #[test]
    fn applies_harfr_kerning_between_adjacent_glyphs() {
        let shaper = FontdueTextShaper::ubuntu_light();
        let av = shaper
            .layout("AV", &TextStyle::new(16.0, Color::BLACK), None)
            .size()
            .width();
        let a = shaper
            .layout("A", &TextStyle::new(16.0, Color::BLACK), None)
            .size()
            .width();
        let v = shaper
            .layout("V", &TextStyle::new(16.0, Color::BLACK), None)
            .size()
            .width();

        assert!(
            av < a + v,
            "HarfRust should kern AV more tightly: {av} < {}",
            a + v
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
        let black = shaper.layout("Drawen", &TextStyle::new(16.0, Color::BLACK), None);
        let white = shaper.layout("Drawen", &TextStyle::new(16.0, Color::WHITE), None);
        let stats = shaper.cache_stats();

        assert_eq!(black.size(), white.size());
        assert_eq!(white.color(), Color::WHITE);
        assert_eq!(stats.entries, 1);
        assert_eq!(stats.hits, 1);
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
    fn assigns_global_byte_ranges_to_clusters_after_line_breaks() {
        let text = "Первая строка\nВторая строка";
        let layout = FontdueTextShaper::ubuntu_light().layout(
            text,
            &TextStyle::new(16.0, Color::BLACK),
            None,
        );
        let second_line_start = "Первая строка\n".len();

        assert_eq!(layout.lines().len(), 2);
        assert_eq!(layout.line_ranges()[1].start, second_line_start);
        assert!(
            layout
                .clusters()
                .iter()
                .any(|cluster| cluster.source().start >= second_line_start)
        );
        assert_eq!(layout.line_at(second_line_start), 1);
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
            "Drawen 🦀",
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

    #[test]
    fn caret_geometry_uses_utf8_byte_offsets_and_grapheme_clusters() {
        let layout = FontdueTextShaper::ubuntu_light().layout(
            "a🦀b",
            &TextStyle::new(16.0, Color::BLACK),
            None,
        );
        assert_eq!(layout.clusters().len(), 3);
        assert!(layout.caret_x(0) < layout.caret_x("a".len()));
        assert!(layout.caret_x("a".len()) < layout.caret_x("a🦀".len()));
        assert_eq!(layout.line_at(0), 0);
        assert_eq!(
            layout.byte_at_point(drawen_core::Point::new(-100.0, 1.0)),
            0
        );
    }

    #[test]
    fn shapes_arabic_with_joining_glyphs() {
        let layout = FontdueTextShaper::ubuntu_light().layout(
            "سلام",
            &TextStyle::new(18.0, Color::BLACK),
            None,
        );
        assert!(!layout.glyph_runs().is_empty());
        assert_eq!(layout.clusters().len(), 4);
    }
}
