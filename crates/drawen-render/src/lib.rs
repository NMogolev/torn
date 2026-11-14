//! Backend-neutral display-list, paint, image, and text contracts for Drawen.
//!
//! Widgets paint into a [`PaintContext`], which records a [`DisplayList`] of
//! filled and rounded rectangles, borders, clips, transforms, images, and
//! pre-shaped text. Renderers execute that list without depending on the widget
//! tree.
//!
//! Text measurement and painting share one [`TextLayout`]: [`FontdueTextShaper`]
//! resolves [`Font`] requests through a [`FontDatabase`], shapes text with
//! `HarfRust`, applies bidirectional reordering and grapheme fallback, and exposes
//! line, caret, hit-testing, and selection geometry. The crate has no platform
//! or window dependency, so the same contracts work in headless tests.

mod display_list;
mod font;
mod image;
mod paint;
mod text;

pub use display_list::{DisplayCommand, DisplayList, DisplayListStats, TextureId};
pub use font::{
    DEFAULT_DPI, Font, FontDatabase, FontDescriptor, FontError, FontFace, FontFamily, FontMetrics,
    FontSize, FontStyle, FontWeight, GlyphBitmap,
};
pub use image::{RasterImage, RasterImageError};
pub use paint::{PaintContext, PaintTransition};
pub use text::{
    FontdueTextShaper, GlyphRun, LineMetrics, PositionedGlyph, TextLayout, TextLayoutCacheStats,
    TextShaper, TextStyle,
};
