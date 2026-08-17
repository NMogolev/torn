use core::fmt;

use torn_core::{Color, Point, Rect, Size};
use torn_render::{DisplayCommand, DisplayList};

use crate::{Pixel, PixelBuffer};

/// Deterministic reference renderer for rectangle-based display lists.
#[derive(Debug, Default)]
pub struct SoftwareRenderer;

impl SoftwareRenderer {
    /// Renders `display_list` into `target` using source-over composition.
    ///
    /// Text commands are intentionally ignored until a deterministic glyph
    /// rasterizer is available; all other M1 commands are executed.
    ///
    /// # Errors
    ///
    /// Returns [`RenderError`] for non-finite geometry or an unmatched clip pop.
    pub fn render(
        &self,
        display_list: &DisplayList,
        target: &mut PixelBuffer,
    ) -> Result<(), RenderError> {
        let full_clip = Rect::new(
            Point::ZERO,
            Size::new(
                as_logical_coordinate(target.width()),
                as_logical_coordinate(target.height()),
            )
            .map_err(|_| RenderError::NonFiniteGeometry)?,
        );
        let mut clips = vec![full_clip];

        for command in display_list.commands() {
            match command {
                DisplayCommand::FillRect { rect, color } => {
                    Self::validate_rect(*rect)?;
                    let clip = clips.last().copied().ok_or(RenderError::UnmatchedClipPop)?;
                    Self::fill_rect(target, intersect(*rect, clip)?, *color);
                }
                DisplayCommand::PushClip { rect } => {
                    Self::validate_rect(*rect)?;
                    let clip = clips.last().copied().ok_or(RenderError::UnmatchedClipPop)?;
                    clips.push(intersect(*rect, clip)?);
                }
                DisplayCommand::PopClip => {
                    if clips.len() == 1 {
                        return Err(RenderError::UnmatchedClipPop);
                    }
                    clips.pop();
                }
                DisplayCommand::DrawText { .. } => {}
            }
        }

        Ok(())
    }

    fn validate_rect(rect: Rect) -> Result<(), RenderError> {
        if !rect.origin.x.is_finite()
            || !rect.origin.y.is_finite()
            || !rect.size.width().is_finite()
            || !rect.size.height().is_finite()
        {
            return Err(RenderError::NonFiniteGeometry);
        }

        Ok(())
    }

    fn fill_rect(target: &mut PixelBuffer, rect: Rect, color: Color) {
        let start_x = floor_to_pixel(rect.origin.x, target.width());
        let start_y = floor_to_pixel(rect.origin.y, target.height());
        let end_x = ceil_to_pixel(rect.right(), target.width());
        let end_y = ceil_to_pixel(rect.bottom(), target.height());

        for y in start_y..end_y {
            for x in start_x..end_x {
                if let Some(pixel) = target.get_mut(x, y) {
                    *pixel = blend(*pixel, color);
                }
            }
        }
    }
}

/// Why a [`SoftwareRenderer`] could not execute a display list.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RenderError {
    /// A rectangle has a non-finite origin or extent.
    NonFiniteGeometry,
    /// A display list popped a clip that was never pushed.
    UnmatchedClipPop,
}

impl fmt::Display for RenderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::NonFiniteGeometry => "renderer received non-finite geometry",
            Self::UnmatchedClipPop => "display list popped an empty clip stack",
        })
    }
}

impl std::error::Error for RenderError {}

fn intersect(left: Rect, right: Rect) -> Result<Rect, RenderError> {
    let x = left.origin.x.max(right.origin.x);
    let y = left.origin.y.max(right.origin.y);
    let width = (left.right().min(right.right()) - x).max(0.0);
    let height = (left.bottom().min(right.bottom()) - y).max(0.0);

    Ok(Rect::new(
        Point::new(x, y),
        Size::new(width, height).map_err(|_| RenderError::NonFiniteGeometry)?,
    ))
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
    let value = (component(value) * 255.0).round();

    // `component` restricts `value` to the inclusive 0..=255 range.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    {
        value as u8
    }
}

fn as_logical_coordinate(value: u32) -> f32 {
    // Pixel buffers larger than f32's exact integer range are neither practical
    // for this reference renderer nor representable exactly in logical pixels.
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
    let coordinate = coordinate.clamp(0.0, as_logical_coordinate(limit));

    // The clamp establishes an inclusive 0..=u32::MAX range before conversion.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    {
        coordinate as u32
    }
}

#[cfg(test)]
mod tests {
    use torn_core::{Color, Point, Rect, Size};
    use torn_render::{DisplayList, PaintContext};

    use super::{Pixel, PixelBuffer, RenderError, SoftwareRenderer};

    fn rect(x: f32, y: f32, width: f32, height: f32) -> Rect {
        Rect::new(
            Point::new(x, y),
            Size::new(width, height).expect("valid test size"),
        )
    }

    #[test]
    fn renders_filled_rectangles_with_nested_clips() {
        let mut list = DisplayList::new();
        let mut paint = PaintContext::new(&mut list);
        paint.fill_rect(rect(0.0, 0.0, 4.0, 4.0), Color::rgba8(255, 0, 0, 255));
        paint.push_clip(rect(1.0, 1.0, 2.0, 2.0));
        paint.fill_rect(rect(0.0, 0.0, 4.0, 4.0), Color::rgba8(0, 0, 255, 255));
        paint.pop_clip();

        let mut pixels = PixelBuffer::new(4, 4).expect("small test buffer");
        SoftwareRenderer
            .render(&list, &mut pixels)
            .expect("valid display list");

        assert_eq!(pixels.get(0, 0), Some(Pixel::rgba(255, 0, 0, 255)));
        assert_eq!(pixels.get(1, 1), Some(Pixel::rgba(0, 0, 255, 255)));
        assert_eq!(pixels.get(2, 2), Some(Pixel::rgba(0, 0, 255, 255)));
        assert_eq!(pixels.get(3, 3), Some(Pixel::rgba(255, 0, 0, 255)));
    }

    #[test]
    fn composites_unpremultiplied_colors_source_over() {
        let mut list = DisplayList::new();
        let mut paint = PaintContext::new(&mut list);
        paint.fill_rect(rect(0.0, 0.0, 1.0, 1.0), Color::BLACK);
        paint.fill_rect(rect(0.0, 0.0, 1.0, 1.0), Color::rgba(1.0, 0.0, 0.0, 0.5));

        let mut pixels = PixelBuffer::new(1, 1).expect("small test buffer");
        SoftwareRenderer
            .render(&list, &mut pixels)
            .expect("valid display list");

        assert_eq!(pixels.get(0, 0), Some(Pixel::rgba(128, 0, 0, 255)));
    }

    #[test]
    fn rejects_unmatched_clip_pop() {
        let mut list = DisplayList::new();
        PaintContext::new(&mut list).pop_clip();
        let mut pixels = PixelBuffer::new(1, 1).expect("small test buffer");

        assert_eq!(
            SoftwareRenderer.render(&list, &mut pixels),
            Err(RenderError::UnmatchedClipPop)
        );
    }
}
