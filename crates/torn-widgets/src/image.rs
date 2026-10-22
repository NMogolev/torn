use torn_core::{Constraints, Rect, Size};
use torn_render::{PaintContext, RasterImage};
use torn_ui::{LayoutContext, LayoutResult, UiEnvironment, Widget};

/// A passive widget that paints an RGBA raster image.
///
/// By default, the image uses its intrinsic pixel dimensions as logical pixels.
/// [`Self::with_size`] sets an explicit logical display size; in both cases the
/// final size is constrained by the parent.
pub struct Image {
    image: RasterImage,
    display_size: Option<Size>,
}

impl Image {
    /// Creates an image widget at its intrinsic pixel dimensions.
    #[must_use]
    pub const fn new(image: RasterImage) -> Self {
        Self {
            image,
            display_size: None,
        }
    }

    /// Creates an image widget with an explicit logical display size.
    ///
    /// # Panics
    ///
    /// Panics when either dimension is negative or NaN.
    #[must_use]
    pub fn with_size(image: RasterImage, width: f32, height: f32) -> Self {
        Self {
            image,
            display_size: Some(size(width, height)),
        }
    }

    /// Returns the source image.
    #[must_use]
    pub const fn image(&self) -> &RasterImage {
        &self.image
    }

    /// Returns the requested display size, if explicitly set.
    #[must_use]
    pub const fn display_size(&self) -> Option<Size> {
        self.display_size
    }
}

impl Widget for Image {
    fn layout(&mut self, _: &mut LayoutContext<'_>, constraints: Constraints) -> LayoutResult {
        let intrinsic = size(
            u32_to_f32(self.image.width()),
            u32_to_f32(self.image.height()),
        );
        LayoutResult::new(constraints.constrain(self.display_size.unwrap_or(intrinsic)))
    }

    fn paint(&self, context: &mut PaintContext<'_>, _: &UiEnvironment, bounds: Rect) {
        context.draw_image(self.image.clone(), bounds);
    }

    fn participates_in_hit_test(&self) -> bool {
        false
    }
}

fn size(width: f32, height: f32) -> Size {
    Size::new(width, height).expect("image sizes are non-negative and not NaN")
}

fn u32_to_f32(value: u32) -> f32 {
    #[allow(clippy::cast_precision_loss)]
    {
        value as f32
    }
}

#[cfg(test)]
mod tests {
    use torn_render::{DisplayCommand, DisplayList};
    use torn_ui::UiRuntime;

    use super::*;

    #[test]
    fn uses_intrinsic_dimensions_and_records_an_image_draw() {
        let image = RasterImage::new(2, 1, vec![255; 8]).expect("valid RGBA image");
        let mut runtime = UiRuntime::new(Image::new(image.clone()));
        let mut list = DisplayList::new();

        assert_eq!(
            runtime
                .layout(Constraints::UNBOUNDED)
                .expect("image layout succeeds")
                .size(),
            size(2.0, 1.0)
        );
        runtime
            .paint(&mut PaintContext::new(&mut list))
            .expect("image paint succeeds");

        assert!(matches!(
            list.commands(),
            [DisplayCommand::DrawImage { image: recorded, .. }] if recorded == &image
        ));
    }
}
