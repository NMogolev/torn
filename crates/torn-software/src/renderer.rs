use core::fmt;

use torn_core::{Affine, Color, Diagnostic, DiagnosticReporter, Point, Rect, Size};
use torn_render::{DisplayCommand, DisplayList, TextLayout};

use crate::{Pixel, PixelBuffer};

/// Deterministic reference renderer for Torn display lists.
#[derive(Debug, Default)]
pub struct SoftwareRenderer;

impl SoftwareRenderer {
    /// Renders `display_list` into `target` at one physical pixel per logical pixel.
    ///
    /// For a high-DPI target, use [`Self::render_with_scale_factor`].
    ///
    /// # Errors
    ///
    /// Returns [`RenderError`] for invalid geometry or unbalanced state commands.
    pub fn render(
        &self,
        display_list: &DisplayList,
        target: &mut PixelBuffer,
    ) -> Result<(), RenderError> {
        self.render_with_scale_factor(display_list, target, 1.0)
    }

    /// Renders logical-pixel display commands into a physical-pixel `target`.
    ///
    /// All display-list geometry, including text sizes and transforms, remains
    /// in device-independent logical pixels. `scale_factor` is applied exactly
    /// once while sampling the physical target.
    ///
    /// # Errors
    ///
    /// Returns [`RenderError::InvalidScaleFactor`] for a non-finite or
    /// non-positive scale factor, and other errors for invalid commands.
    pub fn render_with_scale_factor(
        &self,
        display_list: &DisplayList,
        target: &mut PixelBuffer,
        scale_factor: f32,
    ) -> Result<(), RenderError> {
        if !scale_factor.is_finite() || scale_factor <= 0.0 {
            return Err(RenderError::InvalidScaleFactor);
        }
        let mut state = RenderState::default();
        let mut saved_states = Vec::new();

        for command in display_list.commands() {
            match command {
                DisplayCommand::Save => saved_states.push(state.clone()),
                DisplayCommand::Restore => {
                    state = saved_states.pop().ok_or(RenderError::UnmatchedRestore)?;
                }
                DisplayCommand::PopClip => {
                    state.clips.pop().ok_or(RenderError::UnmatchedClipPop)?;
                }
                DisplayCommand::Transform { transform } => {
                    if !transform.is_finite() {
                        return Err(RenderError::NonFiniteGeometry);
                    }
                    state.transform = state.transform.then(*transform);
                }
                DisplayCommand::PushClip { rect } => {
                    validate_rect(*rect)?;
                    state.clips.push(Clip {
                        rect: *rect,
                        inverse: state
                            .transform
                            .inverse()
                            .ok_or(RenderError::SingularTransform)?,
                    });
                }
                DisplayCommand::FillRect { rect, color } => {
                    Self::fill_shape(
                        target,
                        &state,
                        scale_factor,
                        *color,
                        Shape::fill(*rect, 0.0),
                    )?;
                }
                DisplayCommand::FillRoundedRect {
                    rect,
                    radius,
                    color,
                } => {
                    Self::fill_shape(
                        target,
                        &state,
                        scale_factor,
                        *color,
                        Shape::fill(*rect, *radius),
                    )?;
                }
                DisplayCommand::StrokeRect { rect, width, color } => {
                    Self::fill_shape(
                        target,
                        &state,
                        scale_factor,
                        *color,
                        Shape::stroke(*rect, 0.0, *width),
                    )?;
                }
                DisplayCommand::StrokeRoundedRect {
                    rect,
                    radius,
                    width,
                    color,
                } => {
                    Self::fill_shape(
                        target,
                        &state,
                        scale_factor,
                        *color,
                        Shape::stroke(*rect, *radius, *width),
                    )?;
                }
                DisplayCommand::DrawText { layout, origin } => {
                    Self::draw_text(target, layout, *origin, &state, scale_factor)?;
                }
            }
        }

        if saved_states.is_empty() {
            Ok(())
        } else {
            Err(RenderError::UnmatchedSave)
        }
    }

    /// Renders a display list and reports a diagnostic if rendering fails.
    ///
    /// # Errors
    ///
    /// Returns the same [`RenderError`] as [`Self::render`] after reporting an
    /// error diagnostic.
    pub fn render_with_diagnostics(
        &self,
        display_list: &DisplayList,
        target: &mut PixelBuffer,
        reporter: &mut dyn DiagnosticReporter,
    ) -> Result<(), RenderError> {
        let result = self.render(display_list, target);
        if let Err(error) = result {
            reporter.report(&Diagnostic::error("torn-software", error.to_string()));
        }
        result
    }

    fn fill_shape(
        target: &mut PixelBuffer,
        state: &RenderState,
        scale_factor: f32,
        color: Color,
        shape: Shape,
    ) -> Result<(), RenderError> {
        validate_rect(shape.rect)?;
        validate_radius(shape.radius)?;
        validate_stroke_width(shape.stroke_width)?;
        let inverse = state
            .transform
            .inverse()
            .ok_or(RenderError::SingularTransform)?;
        let bounds = shape
            .stroke_width
            .map_or(shape.rect, |width| expand(shape.rect, width * 0.5));
        let (start_x, start_y, end_x, end_y) =
            physical_bounds(bounds, state.transform, target, scale_factor);

        for y in start_y..end_y {
            for x in start_x..end_x {
                let point = logical_pixel_center(x, y, scale_factor);
                if !is_visible(point, state) {
                    continue;
                }
                let local = inverse.transform_point(point);
                let inside = match shape.stroke_width {
                    None => contains_rounded_rect(shape.rect, shape.radius, local),
                    Some(width) => contains_stroke(shape.rect, shape.radius, width, local),
                };
                if inside {
                    blend_pixel(target, x, y, color);
                }
            }
        }
        Ok(())
    }

    fn draw_text(
        target: &mut PixelBuffer,
        layout: &TextLayout,
        origin: Point,
        state: &RenderState,
        scale_factor: f32,
    ) -> Result<(), RenderError> {
        if !origin.x.is_finite() || !origin.y.is_finite() {
            return Err(RenderError::NonFiniteGeometry);
        }
        let inverse = state
            .transform
            .inverse()
            .ok_or(RenderError::SingularTransform)?;

        for run in layout.glyph_runs() {
            if !run.font_size().is_finite() || run.font_size() <= 0.0 {
                continue;
            }
            let raster_size = run.font_size() * scale_factor;
            if !raster_size.is_finite() || raster_size <= 0.0 {
                return Err(RenderError::NonFiniteGeometry);
            }
            for glyph in run.glyphs() {
                let glyph_origin =
                    Point::new(origin.x + glyph.position().x, origin.y + glyph.position().y);
                if !glyph_origin.x.is_finite() || !glyph_origin.y.is_finite() {
                    return Err(RenderError::NonFiniteGeometry);
                }
                let bitmap = run.font().rasterize(glyph.glyph_id(), raster_size);
                let glyph_rect = Rect::new(
                    glyph_origin,
                    Size::new(
                        as_logical_coordinate_usize(bitmap.width()) / scale_factor,
                        as_logical_coordinate_usize(bitmap.height()) / scale_factor,
                    )
                    .map_err(|_| RenderError::NonFiniteGeometry)?,
                );
                let (start_x, start_y, end_x, end_y) =
                    physical_bounds(glyph_rect, state.transform, target, scale_factor);
                for y in start_y..end_y {
                    for x in start_x..end_x {
                        let point = logical_pixel_center(x, y, scale_factor);
                        if !is_visible(point, state) {
                            continue;
                        }
                        let local = inverse.transform_point(point);
                        let bitmap_x =
                            coordinate_to_usize((local.x - glyph_origin.x) * scale_factor);
                        let bitmap_y =
                            coordinate_to_usize((local.y - glyph_origin.y) * scale_factor);
                        let Some(&coverage) = bitmap.coverage().get(
                            bitmap_y
                                .saturating_mul(bitmap.width())
                                .saturating_add(bitmap_x),
                        ) else {
                            continue;
                        };
                        let alpha = f32::from(coverage) / 255.0;
                        blend_pixel(
                            target,
                            x,
                            y,
                            Color::rgba(
                                layout.color().red,
                                layout.color().green,
                                layout.color().blue,
                                layout.color().alpha * alpha,
                            ),
                        );
                    }
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone, Default)]
struct RenderState {
    transform: Affine,
    clips: Vec<Clip>,
}

#[derive(Clone)]
struct Clip {
    rect: Rect,
    inverse: Affine,
}

#[derive(Clone, Copy)]
struct Shape {
    rect: Rect,
    radius: f32,
    stroke_width: Option<f32>,
}

impl Shape {
    const fn fill(rect: Rect, radius: f32) -> Self {
        Self {
            rect,
            radius,
            stroke_width: None,
        }
    }

    const fn stroke(rect: Rect, radius: f32, width: f32) -> Self {
        Self {
            rect,
            radius,
            stroke_width: Some(width),
        }
    }
}

/// Why a [`SoftwareRenderer`] could not execute a display list.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RenderError {
    /// A coordinate, radius, width, or transform component was not finite.
    NonFiniteGeometry,
    /// A scale factor was not finite and positive.
    InvalidScaleFactor,
    /// A transformed operation required an inverse of a singular transform.
    SingularTransform,
    /// A restore was issued without a preceding save.
    UnmatchedRestore,
    /// A clip pop was issued without a preceding clip.
    UnmatchedClipPop,
    /// Rendering ended with a state that was never restored.
    UnmatchedSave,
}

impl fmt::Display for RenderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::NonFiniteGeometry => "renderer received non-finite or negative geometry",
            Self::InvalidScaleFactor => "render scale factor must be finite and positive",
            Self::SingularTransform => "renderer received a singular transform",
            Self::UnmatchedRestore => "display list restored an empty state stack",
            Self::UnmatchedClipPop => "display list popped an empty clip stack",
            Self::UnmatchedSave => "display list ended with an unbalanced save",
        })
    }
}

impl std::error::Error for RenderError {}

fn validate_rect(rect: Rect) -> Result<(), RenderError> {
    if !rect.origin.x.is_finite()
        || !rect.origin.y.is_finite()
        || !rect.size.width().is_finite()
        || !rect.size.height().is_finite()
    {
        Err(RenderError::NonFiniteGeometry)
    } else {
        Ok(())
    }
}

fn validate_radius(radius: f32) -> Result<(), RenderError> {
    if radius.is_finite() && radius >= 0.0 {
        Ok(())
    } else {
        Err(RenderError::NonFiniteGeometry)
    }
}

fn validate_stroke_width(width: Option<f32>) -> Result<(), RenderError> {
    if width.is_none_or(|value| value.is_finite() && value >= 0.0) {
        Ok(())
    } else {
        Err(RenderError::NonFiniteGeometry)
    }
}

fn expand(rect: Rect, amount: f32) -> Rect {
    Rect::new(
        Point::new(rect.origin.x - amount, rect.origin.y - amount),
        Size::new(
            rect.size.width() + amount * 2.0,
            rect.size.height() + amount * 2.0,
        )
        .expect("expanding a finite non-negative rectangle stays valid"),
    )
}

fn contains_stroke(rect: Rect, radius: f32, width: f32, point: Point) -> bool {
    if width == 0.0 {
        return false;
    }
    let outer = expand(rect, width * 0.5);
    let inner = Rect::new(
        Point::new(rect.origin.x + width * 0.5, rect.origin.y + width * 0.5),
        Size::new(
            (rect.size.width() - width).max(0.0),
            (rect.size.height() - width).max(0.0),
        )
        .expect("clamped stroke interior is valid"),
    );
    let outer_radius =
        (radius + width * 0.5).min(outer.size.width().min(outer.size.height()) * 0.5);
    let inner_radius = (radius - width * 0.5)
        .max(0.0)
        .min(inner.size.width().min(inner.size.height()) * 0.5);
    contains_rounded_rect(outer, outer_radius, point)
        && !contains_rounded_rect(inner, inner_radius, point)
}

fn contains_rounded_rect(rect: Rect, radius: f32, point: Point) -> bool {
    if rect.size.width() <= 0.0 || rect.size.height() <= 0.0 {
        return false;
    }
    if !rect.contains(point) {
        return false;
    }
    let radius = radius.min(rect.size.width().min(rect.size.height()) * 0.5);
    if radius == 0.0 {
        return true;
    }
    let closest_x = point.x.clamp(rect.origin.x + radius, rect.right() - radius);
    let closest_y = point
        .y
        .clamp(rect.origin.y + radius, rect.bottom() - radius);
    let dx = point.x - closest_x;
    let dy = point.y - closest_y;
    dx * dx + dy * dy <= radius * radius
}

fn is_visible(point: Point, state: &RenderState) -> bool {
    state
        .clips
        .iter()
        .all(|clip| clip.rect.contains(clip.inverse.transform_point(point)))
}

fn physical_bounds(
    rect: Rect,
    transform: Affine,
    target: &PixelBuffer,
    scale_factor: f32,
) -> (u32, u32, u32, u32) {
    let corners = [
        rect.origin,
        Point::new(rect.right(), rect.origin.y),
        Point::new(rect.origin.x, rect.bottom()),
        Point::new(rect.right(), rect.bottom()),
    ];
    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    for corner in corners {
        let point = transform.transform_point(corner);
        min_x = min_x.min(point.x);
        min_y = min_y.min(point.y);
        max_x = max_x.max(point.x);
        max_y = max_y.max(point.y);
    }
    (
        floor_to_pixel(min_x * scale_factor, target.width()),
        floor_to_pixel(min_y * scale_factor, target.height()),
        ceil_to_pixel(max_x * scale_factor, target.width()),
        ceil_to_pixel(max_y * scale_factor, target.height()),
    )
}

fn logical_pixel_center(x: u32, y: u32, scale_factor: f32) -> Point {
    Point::new(
        (as_logical_coordinate(x) + 0.5) / scale_factor,
        (as_logical_coordinate(y) + 0.5) / scale_factor,
    )
}

fn blend_pixel(target: &mut PixelBuffer, x: u32, y: u32, color: Color) {
    if let Some(pixel) = target.get_mut(x, y) {
        *pixel = blend(*pixel, color);
    }
}

fn blend(destination: Pixel, source: Color) -> Pixel {
    let source_alpha = component(source.alpha);
    let destination_alpha = f32::from(destination.alpha) / 255.0;
    let output_alpha = source_alpha + destination_alpha * (1.0 - source_alpha);

    let source_red = component(source.red) * source_alpha;
    let source_green = component(source.green) * source_alpha;
    let source_blue = component(source.blue) * source_alpha;
    let destination_red = f32::from(destination.red) / 255.0 * destination_alpha;
    let destination_green = f32::from(destination.green) / 255.0 * destination_alpha;
    let destination_blue = f32::from(destination.blue) / 255.0 * destination_alpha;

    let (red, green, blue) = if output_alpha == 0.0 {
        (0.0, 0.0, 0.0)
    } else {
        (
            (source_red + destination_red * (1.0 - source_alpha)) / output_alpha,
            (source_green + destination_green * (1.0 - source_alpha)) / output_alpha,
            (source_blue + destination_blue * (1.0 - source_alpha)) / output_alpha,
        )
    };
    Pixel::rgba(to_u8(red), to_u8(green), to_u8(blue), to_u8(output_alpha))
}

fn component(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

fn to_u8(value: f32) -> u8 {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    {
        (component(value) * 255.0).round() as u8
    }
}

fn as_logical_coordinate(value: u32) -> f32 {
    #[allow(clippy::cast_precision_loss)]
    {
        value as f32
    }
}

fn as_logical_coordinate_usize(value: usize) -> f32 {
    #[allow(clippy::cast_precision_loss)]
    {
        value as f32
    }
}

fn floor_to_pixel(coordinate: f32, limit: u32) -> u32 {
    coordinate_to_pixel(coordinate.floor(), limit)
}

fn ceil_to_pixel(coordinate: f32, limit: u32) -> u32 {
    coordinate_to_pixel(coordinate.ceil(), limit)
}

fn coordinate_to_pixel(coordinate: f32, limit: u32) -> u32 {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    {
        coordinate.clamp(0.0, as_logical_coordinate(limit)) as u32
    }
}

fn coordinate_to_usize(coordinate: f32) -> usize {
    if coordinate.is_sign_negative() || !coordinate.is_finite() {
        return usize::MAX;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    {
        coordinate.floor() as usize
    }
}

#[cfg(test)]
mod tests {
    use torn_core::{Affine, Color, DiagnosticSeverity, Point, Rect, Size};
    use torn_render::{DisplayList, FontdueTextShaper, PaintContext, TextStyle};

    use super::{Pixel, PixelBuffer, RenderError, SoftwareRenderer};

    fn rect(x: f32, y: f32, width: f32, height: f32) -> Rect {
        Rect::new(
            Point::new(x, y),
            Size::new(width, height).expect("valid test size"),
        )
    }

    #[test]
    fn renders_filled_rectangles_with_scoped_clips() {
        let mut list = DisplayList::new();
        let mut paint = PaintContext::new(&mut list);
        paint.fill_rect(rect(0.0, 0.0, 4.0, 4.0), Color::rgba8(255, 0, 0, 255));
        paint.with_clip(rect(1.0, 1.0, 2.0, 2.0), |context| {
            context.fill_rect(rect(0.0, 0.0, 4.0, 4.0), Color::rgba8(0, 0, 255, 255));
        });
        let mut pixels = PixelBuffer::new(4, 4).expect("small test buffer");

        SoftwareRenderer
            .render(&list, &mut pixels)
            .expect("valid display list");

        assert_eq!(pixels.get(0, 0), Some(Pixel::rgba(255, 0, 0, 255)));
        assert_eq!(pixels.get(1, 1), Some(Pixel::rgba(0, 0, 255, 255)));
        assert_eq!(pixels.get(3, 3), Some(Pixel::rgba(255, 0, 0, 255)));
    }

    #[test]
    fn transforms_are_isolated_by_save_restore() {
        let mut list = DisplayList::new();
        let mut paint = PaintContext::new(&mut list);
        paint.with_transform(Affine::translate(2.0, 0.0), |context| {
            context.fill_rect(rect(0.0, 0.0, 1.0, 1.0), Color::BLACK);
        });
        paint.fill_rect(rect(0.0, 0.0, 1.0, 1.0), Color::WHITE);
        let mut pixels = PixelBuffer::new(3, 1).expect("small test buffer");

        SoftwareRenderer
            .render(&list, &mut pixels)
            .expect("valid display list");

        assert_eq!(pixels.get(0, 0), Some(Pixel::rgba(255, 255, 255, 255)));
        assert_eq!(pixels.get(1, 0), Some(Pixel::TRANSPARENT));
        assert_eq!(pixels.get(2, 0), Some(Pixel::rgba(0, 0, 0, 255)));
    }

    #[test]
    fn rasterizes_rounded_rectangles_and_borders() {
        let mut list = DisplayList::new();
        let mut paint = PaintContext::new(&mut list);
        paint.fill_rounded_rect(rect(0.0, 0.0, 6.0, 6.0), 2.0, Color::WHITE);
        paint.stroke_rect(rect(1.5, 1.5, 3.0, 3.0), 1.0, Color::BLACK);
        let mut pixels = PixelBuffer::new(6, 6).expect("small test buffer");

        SoftwareRenderer
            .render(&list, &mut pixels)
            .expect("valid display list");

        assert_eq!(pixels.get(0, 0), Some(Pixel::TRANSPARENT));
        assert_eq!(pixels.get(1, 1), Some(Pixel::rgba(0, 0, 0, 255)));
        assert_eq!(pixels.get(3, 3), Some(Pixel::rgba(255, 255, 255, 255)));
    }

    #[test]
    fn maps_logical_geometry_to_a_high_dpi_target_once() {
        let mut list = DisplayList::new();
        PaintContext::new(&mut list).fill_rect(rect(0.0, 0.0, 1.0, 1.0), Color::BLACK);
        let mut pixels = PixelBuffer::new(2, 2).expect("small test buffer");

        SoftwareRenderer
            .render_with_scale_factor(&list, &mut pixels, 2.0)
            .expect("valid display list");

        assert!(
            pixels
                .pixels()
                .iter()
                .all(|pixel| *pixel == Pixel::rgba(0, 0, 0, 255))
        );
    }

    #[test]
    fn rasterizes_text_under_the_active_transform_and_clip() {
        let layout = FontdueTextShaper::ubuntu_light().layout(
            "T",
            &TextStyle::new(20.0, Color::BLACK),
            None,
        );
        let mut list = DisplayList::new();
        let mut paint = PaintContext::new(&mut list);
        paint.with_clip(rect(5.0, 0.0, 5.0, 24.0), |context| {
            context.translate(Point::new(4.0, 0.0));
            context.draw_text(layout, Point::new(1.0, 1.0));
        });
        let mut pixels = PixelBuffer::new(24, 24).expect("small test buffer");

        SoftwareRenderer
            .render(&list, &mut pixels)
            .expect("valid text display list");

        assert!(pixels.pixels().iter().any(|pixel| pixel.alpha > 0));
        assert!(pixels.pixels().chunks_exact(24).all(|row| {
            row[..5].iter().all(|pixel| pixel.alpha == 0)
                && row[10..].iter().all(|pixel| pixel.alpha == 0)
        }));
    }

    #[test]
    fn reports_unmatched_restore_as_a_diagnostic() {
        let mut list = DisplayList::new();
        PaintContext::new(&mut list).restore();
        let mut pixels = PixelBuffer::new(1, 1).expect("small test buffer");
        let mut diagnostics = Vec::new();

        assert_eq!(
            SoftwareRenderer.render_with_diagnostics(&list, &mut pixels, &mut diagnostics),
            Err(RenderError::UnmatchedRestore)
        );
        assert_eq!(diagnostics[0].severity(), DiagnosticSeverity::Error);
        assert_eq!(diagnostics[0].component(), "torn-software");
    }
}
