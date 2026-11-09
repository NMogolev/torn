//! Font requests, face metadata, and deterministic font matching.
//!
//! The types in this module mirror the parts of Qt's font model that affect
//! layout: a font is a *request* (family preferences, size, weight, and style)
//! that a [`FontDatabase`] resolves to concrete [`FontFace`] resources. Text
//! shaping consumes the resolved faces, so measuring and painting always agree
//! on which font was selected.

use std::{
    fmt,
    sync::{Arc, OnceLock},
};

use fontdue::{Font as FontdueFont, FontSettings};

/// The logical dots per inch used when a point size is resolved without an
/// explicit DPI value.
pub const DEFAULT_DPI: f32 = 96.0;

/// A font family name.
///
/// Families are cheap to clone because the name is shared. Matching against a
/// [`FontDatabase`] is case-insensitive.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct FontFamily(Arc<str>);

impl FontFamily {
    /// Creates a family name from any string-like value.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self(Arc::from(name.into()))
    }

    /// Returns the family name as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for FontFamily {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl From<&str> for FontFamily {
    fn from(name: &str) -> Self {
        Self::new(name)
    }
}

impl From<String> for FontFamily {
    fn from(name: String) -> Self {
        Self(Arc::from(name))
    }
}

impl From<&String> for FontFamily {
    fn from(name: &String) -> Self {
        Self::new(name.as_str())
    }
}

impl AsRef<str> for FontFamily {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

/// A CSS-compatible numeric font weight.
///
/// Common values are available as constants such as [`Self::NORMAL`] and
/// [`Self::BOLD`]. Values from 100 through 900 are the interoperable range.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct FontWeight(u16);

impl FontWeight {
    /// Weight 100.
    pub const THIN: Self = Self(100);
    /// Weight 200.
    pub const EXTRA_LIGHT: Self = Self(200);
    /// Weight 300.
    pub const LIGHT: Self = Self(300);
    /// Weight 400.
    pub const NORMAL: Self = Self(400);
    /// Weight 500.
    pub const MEDIUM: Self = Self(500);
    /// Weight 600.
    pub const SEMI_BOLD: Self = Self(600);
    /// Weight 700.
    pub const BOLD: Self = Self(700);
    /// Weight 800.
    pub const EXTRA_BOLD: Self = Self(800);
    /// Weight 900.
    pub const BLACK: Self = Self(900);

    /// Creates a weight from a numeric value.
    ///
    /// Returns `None` outside the inclusive `1..=1000` range.
    #[must_use]
    pub const fn new(weight: u16) -> Option<Self> {
        if weight >= 1 && weight <= 1000 {
            Some(Self(weight))
        } else {
            None
        }
    }

    /// Returns the numeric weight.
    #[must_use]
    pub const fn value(self) -> u16 {
        self.0
    }

    /// Returns whether this weight is at least bold.
    #[must_use]
    pub const fn is_bold(self) -> bool {
        self.0 >= Self::BOLD.0
    }
}

impl Default for FontWeight {
    fn default() -> Self {
        Self::NORMAL
    }
}

/// The slant of a font face.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum FontStyle {
    /// Upright text.
    #[default]
    Normal,
    /// Cursive text with a dedicated italic face.
    Italic,
    /// Slanted text; some families provide an oblique face distinct from italic.
    Oblique,
}

impl FontStyle {
    /// Returns whether this style is slanted.
    #[must_use]
    pub const fn is_slanted(self) -> bool {
        matches!(self, Self::Italic | Self::Oblique)
    }
}

/// A requested font size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FontSize {
    /// A size in logical pixels.
    Pixels(f32),
    /// A size in typographic points. Resolved with [`FontSize::pixels_at`].
    Points(f32),
}

impl FontSize {
    /// Creates a size in logical pixels.
    #[must_use]
    pub const fn pixels(pixels: f32) -> Self {
        Self::Pixels(pixels)
    }

    /// Creates a size in typographic points.
    #[must_use]
    pub const fn points(points: f32) -> Self {
        Self::Points(points)
    }

    /// Resolves this size to logical pixels at `dpi`.
    ///
    /// Invalid or negative values resolve to zero rather than producing NaN
    /// geometry. A non-finite or non-positive `dpi` falls back to
    /// [`DEFAULT_DPI`].
    #[must_use]
    pub fn pixels_at(self, dpi: f32) -> f32 {
        let dpi = if dpi.is_finite() && dpi > 0.0 {
            dpi
        } else {
            DEFAULT_DPI
        };
        let pixels = match self {
            Self::Pixels(pixels) => pixels,
            Self::Points(points) => points * dpi / 72.0,
        };
        if pixels.is_finite() {
            pixels.max(0.0)
        } else {
            0.0
        }
    }

    /// Returns whether this size resolves to a positive pixel value at `dpi`.
    #[must_use]
    pub fn is_visible_at(self, dpi: f32) -> bool {
        self.pixels_at(dpi) > 0.0
    }
}

/// A font request, analogous to `QFont`.
///
/// A request lists family preferences in priority order. When no registered
/// face matches a requested family, the [`FontDatabase`] uses its default face
/// and then other registered faces as character fallbacks. This mirrors Qt's
/// behavior of never failing to produce a usable font when any font exists.
#[derive(Clone, Debug, PartialEq)]
pub struct Font {
    families: Vec<FontFamily>,
    size: FontSize,
    weight: FontWeight,
    style: FontStyle,
}

impl Font {
    /// Creates a font request using the default size, weight, and style.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a font request for `family` with the default size.
    #[must_use]
    pub fn from_family(family: impl Into<FontFamily>) -> Self {
        Self::new().with_family(family)
    }

    /// Creates a font request with a size in logical pixels.
    #[must_use]
    pub fn from_pixel_size(pixels: f32) -> Self {
        Self::new().with_pixel_size(pixels)
    }

    /// Creates a font request with a size in typographic points.
    #[must_use]
    pub fn from_point_size(points: f32) -> Self {
        Self::new().with_point_size(points)
    }

    /// Returns the family preferences in priority order.
    #[must_use]
    pub fn families(&self) -> &[FontFamily] {
        &self.families
    }

    /// Returns the first family preference, if any.
    #[must_use]
    pub fn family(&self) -> Option<&FontFamily> {
        self.families.first()
    }

    /// Replaces the family preferences with a single family.
    pub fn set_family(&mut self, family: impl Into<FontFamily>) {
        self.families.clear();
        self.families.push(family.into());
    }

    /// Replaces the family preferences.
    pub fn set_families(&mut self, families: impl IntoIterator<Item = FontFamily>) {
        self.families = families.into_iter().collect();
    }

    /// Appends a family preference if it is not already present.
    pub fn add_family(&mut self, family: impl Into<FontFamily>) {
        let family = family.into();
        if !self.families.contains(&family) {
            self.families.push(family);
        }
    }

    /// Returns this request with a single family preference.
    #[must_use]
    pub fn with_family(mut self, family: impl Into<FontFamily>) -> Self {
        self.set_family(family);
        self
    }

    /// Returns this request with the supplied family preferences.
    #[must_use]
    pub fn with_families(mut self, families: impl IntoIterator<Item = FontFamily>) -> Self {
        self.set_families(families);
        self
    }

    /// Returns the requested size.
    #[must_use]
    pub const fn size(&self) -> FontSize {
        self.size
    }

    /// Replaces the requested size.
    pub fn set_size(&mut self, size: FontSize) {
        self.size = size;
    }

    /// Replaces the requested size with logical pixels.
    pub fn set_pixel_size(&mut self, pixels: f32) {
        self.size = FontSize::Pixels(pixels);
    }

    /// Replaces the requested size with typographic points.
    pub fn set_point_size(&mut self, points: f32) {
        self.size = FontSize::Points(points);
    }

    /// Returns this request with the supplied size.
    #[must_use]
    pub fn with_size(mut self, size: FontSize) -> Self {
        self.size = size;
        self
    }

    /// Returns this request with a size in logical pixels.
    #[must_use]
    pub fn with_pixel_size(mut self, pixels: f32) -> Self {
        self.set_pixel_size(pixels);
        self
    }

    /// Returns this request with a size in typographic points.
    #[must_use]
    pub fn with_point_size(mut self, points: f32) -> Self {
        self.set_point_size(points);
        self
    }

    /// Resolves the requested size to logical pixels at `dpi`.
    #[must_use]
    pub fn pixel_size(&self, dpi: f32) -> f32 {
        self.size.pixels_at(dpi)
    }

    /// Returns the requested weight.
    #[must_use]
    pub const fn weight(&self) -> FontWeight {
        self.weight
    }

    /// Replaces the requested weight.
    pub fn set_weight(&mut self, weight: FontWeight) {
        self.weight = weight;
    }

    /// Returns this request with the supplied weight.
    #[must_use]
    pub fn with_weight(mut self, weight: FontWeight) -> Self {
        self.weight = weight;
        self
    }

    /// Returns the requested style.
    #[must_use]
    pub const fn style(&self) -> FontStyle {
        self.style
    }

    /// Replaces the requested style.
    pub fn set_style(&mut self, style: FontStyle) {
        self.style = style;
    }

    /// Returns this request with the supplied style.
    #[must_use]
    pub fn with_style(mut self, style: FontStyle) -> Self {
        self.style = style;
        self
    }
}

impl Default for Font {
    fn default() -> Self {
        Self {
            families: Vec::new(),
            size: FontSize::Pixels(14.0),
            weight: FontWeight::NORMAL,
            style: FontStyle::Normal,
        }
    }
}

/// Metadata used by [`FontDatabase`] to match a concrete face.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FontDescriptor {
    family: FontFamily,
    weight: FontWeight,
    style: FontStyle,
}

impl FontDescriptor {
    /// Creates a descriptor for a face.
    #[must_use]
    pub fn new(family: impl Into<FontFamily>, weight: FontWeight, style: FontStyle) -> Self {
        Self {
            family: family.into(),
            weight,
            style,
        }
    }

    /// Returns the face's family.
    #[must_use]
    pub const fn family(&self) -> &FontFamily {
        &self.family
    }

    /// Returns the face's weight.
    #[must_use]
    pub const fn weight(&self) -> FontWeight {
        self.weight
    }

    /// Returns the face's style.
    #[must_use]
    pub const fn style(&self) -> FontStyle {
        self.style
    }
}

impl Default for FontDescriptor {
    fn default() -> Self {
        Self {
            family: FontFamily::new("Sans"),
            weight: FontWeight::NORMAL,
            style: FontStyle::Normal,
        }
    }
}

/// An immutable font resource used by shaped glyph runs.
#[derive(Clone)]
pub struct FontFace {
    inner: Arc<FontdueFont>,
    descriptor: FontDescriptor,
}

impl FontFace {
    /// Parses a TrueType or OpenType font from its complete binary contents.
    ///
    /// The face family is taken from the font's full name when available, with
    /// normal weight and style. Use [`Self::from_bytes_with_descriptor`] to
    /// register a family, weight, and style explicitly.
    ///
    /// # Errors
    ///
    /// Returns [`FontError`] when `bytes` do not contain a supported font.
    pub fn from_bytes(bytes: impl Into<Vec<u8>>) -> Result<Self, FontError> {
        let font =
            FontdueFont::from_bytes(bytes.into(), FontSettings::default()).map_err(FontError)?;
        let family = font
            .name()
            .map_or_else(|| FontFamily::new("Sans"), FontFamily::new);
        let descriptor = FontDescriptor::new(family, FontWeight::NORMAL, FontStyle::Normal);
        Ok(Self {
            inner: Arc::new(font),
            descriptor,
        })
    }

    /// Parses a font and registers it under an explicit descriptor.
    ///
    /// # Errors
    ///
    /// Returns [`FontError`] when `bytes` do not contain a supported font.
    pub fn from_bytes_with_descriptor(
        bytes: impl Into<Vec<u8>>,
        descriptor: FontDescriptor,
    ) -> Result<Self, FontError> {
        FontdueFont::from_bytes(bytes.into(), FontSettings::default())
            .map(|inner| Self {
                inner: Arc::new(inner),
                descriptor,
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
                Self::from_bytes_with_descriptor(
                    include_bytes!("../assets/Ubuntu-Light.ttf").as_slice(),
                    FontDescriptor::new("Ubuntu", FontWeight::LIGHT, FontStyle::Normal),
                )
                .expect("the bundled Ubuntu Light font must be valid")
            })
            .clone()
    }

    /// Returns the face's registration metadata.
    #[must_use]
    pub const fn descriptor(&self) -> &FontDescriptor {
        &self.descriptor
    }

    /// Returns the face's family.
    #[must_use]
    pub const fn family(&self) -> &FontFamily {
        self.descriptor.family()
    }

    /// Returns the face's weight.
    #[must_use]
    pub const fn weight(&self) -> FontWeight {
        self.descriptor.weight()
    }

    /// Returns the face's style.
    #[must_use]
    pub const fn style(&self) -> FontStyle {
        self.descriptor.style()
    }

    /// Returns whether this face contains a glyph for `character`.
    #[must_use]
    pub fn has_glyph(&self, character: char) -> bool {
        self.inner.has_glyph(character)
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

    /// Returns the horizontal line metrics for `font_size` logical pixels.
    ///
    /// Returns `None` for a non-positive or non-finite size, or when the font
    /// does not provide horizontal line metrics.
    #[must_use]
    pub fn metrics(&self, font_size: f32) -> Option<FontMetrics> {
        if !font_size.is_finite() || font_size <= 0.0 {
            return None;
        }
        self.inner
            .horizontal_line_metrics(font_size)
            .map(|metrics| FontMetrics {
                ascent: metrics.ascent,
                descent: -metrics.descent,
                leading: metrics.line_gap,
                height: metrics.new_line_size,
            })
    }

    /// Returns whether both handles refer to the same font file.
    fn same_file(&self, other: &Self) -> bool {
        self.inner.file_hash() == other.inner.file_hash()
    }

    pub(crate) fn font(&self) -> &FontdueFont {
        &self.inner
    }
}

impl fmt::Debug for FontFace {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("FontFace")
            .field("descriptor", &self.descriptor)
            .field("name", &self.inner.name())
            .finish()
    }
}

impl PartialEq for FontFace {
    fn eq(&self, other: &Self) -> bool {
        self.descriptor == other.descriptor && self.same_file(other)
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

/// Horizontal metrics for one line of text.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FontMetrics {
    ascent: f32,
    descent: f32,
    leading: f32,
    height: f32,
}

impl FontMetrics {
    /// Returns the distance above the baseline reserved by the font.
    #[must_use]
    pub const fn ascent(self) -> f32 {
        self.ascent
    }

    /// Returns the distance below the baseline reserved by the font.
    #[must_use]
    pub const fn descent(self) -> f32 {
        self.descent
    }

    /// Returns the font-recommended gap between lines.
    #[must_use]
    pub const fn leading(self) -> f32 {
        self.leading
    }

    /// Returns the total line height.
    #[must_use]
    pub const fn height(self) -> f32 {
        self.height
    }

    /// Returns ascent plus descent plus leading.
    #[must_use]
    pub fn line_spacing(self) -> f32 {
        self.ascent + self.descent + self.leading
    }
}

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

/// A registry of font faces with Qt-style family and style matching.
///
/// Faces are matched case-insensitively by family. Within a family the closest
/// style and weight are selected; when no family matches, the database default
/// is used. Every registered face is also available as a per-character
/// fallback.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FontDatabase {
    faces: Vec<FontFace>,
    default_face: Option<usize>,
}

impl FontDatabase {
    /// Creates an empty database.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a database containing Torn's bundled Ubuntu Light face.
    #[must_use]
    pub fn bundled() -> Self {
        let mut database = Self::new();
        database.add_face(FontFace::ubuntu_light());
        database
    }

    /// Creates a database whose default face is `face`.
    #[must_use]
    pub fn with_default(face: FontFace) -> Self {
        let mut database = Self::new();
        database.add_face(face);
        database
    }

    /// Registers `face` and returns its index.
    ///
    /// The first registered face becomes the default.
    pub fn add_face(&mut self, face: FontFace) -> usize {
        let index = self.faces.len();
        self.faces.push(face);
        if self.default_face.is_none() {
            self.default_face = Some(index);
        }
        index
    }

    /// Parses and registers a face under `descriptor`.
    ///
    /// # Errors
    ///
    /// Returns [`FontError`] when `bytes` do not contain a supported font.
    pub fn add_font(
        &mut self,
        descriptor: FontDescriptor,
        bytes: impl Into<Vec<u8>>,
    ) -> Result<FontFace, FontError> {
        let face = FontFace::from_bytes_with_descriptor(bytes, descriptor)?;
        self.add_face(face.clone());
        Ok(face)
    }

    /// Returns the registered faces in insertion order.
    #[must_use]
    pub fn faces(&self) -> &[FontFace] {
        &self.faces
    }

    /// Returns the unique registered family names in insertion order.
    #[must_use]
    pub fn families(&self) -> Vec<&str> {
        let mut families = Vec::new();
        for face in &self.faces {
            let name = face.family().as_str();
            if !families
                .iter()
                .any(|existing: &&str| existing.eq_ignore_ascii_case(name))
            {
                families.push(name);
            }
        }
        families
    }

    /// Returns the faces registered for `family`, case-insensitively.
    pub fn faces_for_family<'a>(
        &'a self,
        family: &'a str,
    ) -> impl Iterator<Item = &'a FontFace> + 'a {
        self.faces
            .iter()
            .filter(move |face| face.family().as_str().eq_ignore_ascii_case(family))
    }

    /// Returns the default face used when no family matches.
    #[must_use]
    pub fn default_face(&self) -> Option<&FontFace> {
        self.default_face.and_then(|index| self.faces.get(index))
    }

    /// Replaces the default face by index.
    ///
    /// Returns `false` when `index` does not name a registered face.
    pub fn set_default_face(&mut self, index: usize) -> bool {
        if index < self.faces.len() {
            self.default_face = Some(index);
            true
        } else {
            false
        }
    }

    /// Returns whether no faces are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.faces.is_empty()
    }

    /// Resolves `font` to an ordered list of faces.
    ///
    /// Requested families come first, followed by the default face when no
    /// family matched, and finally every other registered face as a
    /// per-character fallback. The returned list is never empty when the
    /// database contains a face.
    #[must_use]
    pub fn resolve(&self, font: &Font) -> Vec<FontFace> {
        let mut resolved = Vec::new();
        for family in font.families() {
            if let Some(face) = self.best_face(family, font.weight(), font.style()) {
                push_unique(&mut resolved, face);
            }
        }
        if resolved.is_empty()
            && let Some(face) = self.default_face()
        {
            push_unique(&mut resolved, face.clone());
        }
        for face in &self.faces {
            push_unique(&mut resolved, face.clone());
        }
        resolved
    }

    fn best_face(
        &self,
        family: &FontFamily,
        weight: FontWeight,
        style: FontStyle,
    ) -> Option<FontFace> {
        self.faces
            .iter()
            .filter(|face| face.family().as_str().eq_ignore_ascii_case(family.as_str()))
            .min_by_key(|face| {
                (
                    style_distance(face.style(), style),
                    weight_distance(face.weight(), weight),
                )
            })
            .cloned()
    }
}

fn push_unique(faces: &mut Vec<FontFace>, face: FontFace) {
    if !faces.iter().any(|existing| existing.same_file(&face)) {
        faces.push(face);
    }
}

const fn style_distance(face: FontStyle, requested: FontStyle) -> u8 {
    match (face, requested) {
        (FontStyle::Normal, FontStyle::Normal)
        | (FontStyle::Italic, FontStyle::Italic)
        | (FontStyle::Oblique, FontStyle::Oblique) => 0,
        (FontStyle::Italic, FontStyle::Oblique) | (FontStyle::Oblique, FontStyle::Italic) => 1,
        _ => 2,
    }
}

const fn weight_distance(face: FontWeight, requested: FontWeight) -> u16 {
    face.value().abs_diff(requested.value())
}

#[cfg(test)]
mod tests {
    use super::{
        Font, FontDatabase, FontDescriptor, FontFace, FontFamily, FontSize, FontStyle, FontWeight,
    };

    fn database() -> FontDatabase {
        let mut database = FontDatabase::with_default(
            FontFace::from_bytes_with_descriptor(
                include_bytes!("../assets/Ubuntu-Light.ttf").as_slice(),
                FontDescriptor::new("Ubuntu", FontWeight::LIGHT, FontStyle::Normal),
            )
            .expect("bundled font is valid"),
        );
        database
            .add_font(
                FontDescriptor::new("Test Sans", FontWeight::NORMAL, FontStyle::Normal),
                include_bytes!("../assets/Ubuntu-Light.ttf").as_slice(),
            )
            .expect("test font is valid");
        database
    }

    #[test]
    fn resolves_family_weight_and_style_by_closest_match() {
        let database = database();
        let font = Font::from_family("test sans")
            .with_weight(FontWeight::BOLD)
            .with_style(FontStyle::Italic);

        let faces = database.resolve(&font);
        assert_eq!(faces[0].family().as_str(), "Test Sans");
        assert_eq!(faces[0].weight(), FontWeight::NORMAL);
    }

    #[test]
    fn falls_back_to_the_default_face_for_unknown_families() {
        let database = database();
        let faces = database.resolve(&Font::from_family("Missing Family"));

        assert_eq!(faces[0].family().as_str(), "Ubuntu");
        assert_eq!(database.families(), vec!["Ubuntu", "Test Sans"]);
    }

    #[test]
    fn empty_family_list_uses_the_default_face_first() {
        let database = database();
        let faces = database.resolve(&Font::new());

        assert_eq!(faces[0].family().as_str(), "Ubuntu");
        // Both registered faces use the same physical font file, so the
        // fallback list collapses them into a single face.
        assert_eq!(faces.len(), 1);
    }

    #[test]
    fn fallback_lists_do_not_repeat_a_physical_font_file() {
        let database = database();
        let faces = database.resolve(&Font::from_family("Test Sans"));

        assert_eq!(faces[0].family().as_str(), "Test Sans");
        assert_eq!(faces.len(), 1);
    }

    #[test]
    fn converts_points_using_the_supplied_dpi() {
        assert!((FontSize::points(12.0).pixels_at(96.0) - 16.0).abs() < f32::EPSILON);
        assert!((FontSize::pixels(16.0).pixels_at(144.0) - 16.0).abs() < f32::EPSILON);
        assert!((FontSize::points(12.0).pixels_at(f32::NAN) - 16.0).abs() < f32::EPSILON);
    }

    #[test]
    fn reports_line_metrics_for_a_visible_size() {
        let face = FontFace::ubuntu_light();
        let metrics = face.metrics(16.0).expect("font exposes line metrics");

        assert!(metrics.ascent() > 0.0);
        assert!(metrics.descent() > 0.0);
        assert!(metrics.height() > 0.0);
        assert!(face.metrics(0.0).is_none());
    }

    #[test]
    fn font_family_is_cheap_to_share_and_compares_by_name() {
        let family = FontFamily::new("Ubuntu");
        let copy = family.clone();

        assert_eq!(family, copy);
        assert_eq!(family.as_str(), "Ubuntu");
    }
}
