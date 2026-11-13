use torn_core::{Color, Constraints, Point, Rect, Size};
use torn_render::{PaintContext, RasterImage};
use torn_ui::{
    Accessibility, AccessibilityRole, LayoutContext, LayoutResult, StyleTarget, TssStyle,
    UiEnvironment, Widget,
};

use crate::Alignment;

/// How an image is scaled into the space assigned by its parent.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ImageFit {
    /// Stretch both axes so the image exactly fills its bounds.
    #[default]
    Fill,
    /// Scale uniformly so the whole image stays visible.
    Contain,
    /// Scale uniformly to cover the bounds and clip the overflow.
    Cover,
    /// Keep the intrinsic pixel size without scaling.
    None,
    /// Scale like [`Self::Contain`] but never enlarge the image.
    ScaleDown,
}

/// A passive widget that paints an RGBA raster image.
///
/// By default, the image uses its intrinsic pixel dimensions as logical pixels
/// and stretches into the bounds assigned by the parent. [`Self::with_size`]
/// sets an explicit logical display size, [`Self::with_fit`] selects how the
/// pixels are mapped into those bounds, and [`Self::with_alignment`] positions
/// an image that does not fill them completely.
pub struct Image {
    source: RasterImage,
    display_size: Option<Size>,
    fit: ImageFit,
    horizontal: Alignment,
    vertical: Alignment,
    background: Option<Color>,
    label: Option<String>,
}

impl Image {
    /// Creates an image widget at its intrinsic pixel dimensions.
    #[must_use]
    pub const fn new(image: RasterImage) -> Self {
        Self {
            source: image,
            display_size: None,
            fit: ImageFit::Fill,
            horizontal: Alignment::Center,
            vertical: Alignment::Center,
            background: None,
            label: None,
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
            source: image,
            display_size: Some(size(width, height)),
            fit: ImageFit::Fill,
            horizontal: Alignment::Center,
            vertical: Alignment::Center,
            background: None,
            label: None,
        }
    }

    /// Returns the source image.
    #[must_use]
    pub const fn image(&self) -> &RasterImage {
        &self.source
    }

    /// Returns the requested display size, if explicitly set.
    #[must_use]
    pub const fn display_size(&self) -> Option<Size> {
        self.display_size
    }

    /// Returns how the image is scaled into its bounds.
    #[must_use]
    pub const fn fit(&self) -> ImageFit {
        self.fit
    }

    /// Returns the horizontal and vertical alignment used by non-filling fits.
    #[must_use]
    pub const fn alignment(&self) -> (Alignment, Alignment) {
        (self.horizontal, self.vertical)
    }

    /// Returns the optional accessible name.
    #[must_use]
    pub fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }

    /// Returns this image with an explicit scaling mode.
    #[must_use]
    pub const fn with_fit(mut self, fit: ImageFit) -> Self {
        self.fit = fit;
        self
    }

    /// Returns this image with explicit horizontal and vertical alignment.
    #[must_use]
    pub const fn with_alignment(mut self, horizontal: Alignment, vertical: Alignment) -> Self {
        self.horizontal = horizontal;
        self.vertical = vertical;
        self
    }

    /// Returns this image with a background painted behind transparent pixels.
    #[must_use]
    pub const fn with_background(mut self, color: Color) -> Self {
        self.background = Some(color);
        self
    }

    /// Returns this image with an accessible name.
    ///
    /// Assistive technologies announce this label instead of treating the
    /// widget as an unnamed decorative image.
    #[must_use]
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }
}

impl Widget for Image {
    fn style_target(&self) -> StyleTarget {
        StyleTarget::new("image")
    }

    fn layout(&mut self, _: &mut LayoutContext<'_>, constraints: Constraints) -> LayoutResult {
        let intrinsic = image_size(&self.source);
        LayoutResult::new(constraints.constrain(self.display_size.unwrap_or(intrinsic)))
    }

    fn paint(&self, context: &mut PaintContext<'_>, environment: &UiEnvironment, bounds: Rect) {
        self.paint_styled(context, environment, bounds, &TssStyle::default());
    }

    fn paint_styled(
        &self,
        context: &mut PaintContext<'_>,
        environment: &UiEnvironment,
        bounds: Rect,
        style: &TssStyle,
    ) {
        let _ = environment;
        if let Some(background) = self.background.or(style.background) {
            context.fill_rect(bounds, background);
        }
        if self.source.width() == 0 || self.source.height() == 0 || bounds.is_empty() {
            return;
        }
        let destination = fitted_rect(
            bounds,
            image_size(&self.source),
            self.fit,
            self.horizontal,
            self.vertical,
        );
        if rect_contains(bounds, destination) {
            context.draw_image(self.source.clone(), destination);
        } else {
            context.with_clip(bounds, |context| {
                context.draw_image(self.source.clone(), destination);
            });
        }
    }

    fn participates_in_hit_test(&self) -> bool {
        false
    }

    fn accessibility(&self) -> Accessibility {
        let accessibility = Accessibility::new(AccessibilityRole::Image);
        match &self.label {
            Some(label) => accessibility.with_label(label.clone()),
            None => accessibility,
        }
    }
}

/// Calculates the destination rectangle for `content` inside `bounds`.
///
/// This is shared by the passive image widgets and by the interactive image
/// viewer so every control maps pixels in exactly the same way.
pub(crate) fn fitted_rect(
    bounds: Rect,
    content: Size,
    fit: ImageFit,
    horizontal: Alignment,
    vertical: Alignment,
) -> Rect {
    if content.width() <= 0.0
        || content.height() <= 0.0
        || bounds.size.width() <= 0.0
        || bounds.size.height() <= 0.0
    {
        return bounds;
    }

    let mut width = content.width();
    let mut height = content.height();
    match fit {
        ImageFit::Fill => {
            width = bounds.size.width();
            height = bounds.size.height();
        }
        ImageFit::Contain | ImageFit::ScaleDown => {
            let mut scale = (bounds.size.width() / content.width())
                .min(bounds.size.height() / content.height());
            if fit == ImageFit::ScaleDown {
                scale = scale.min(1.0);
            }
            width = content.width() * scale;
            height = content.height() * scale;
        }
        ImageFit::Cover => {
            let scale = (bounds.size.width() / content.width())
                .max(bounds.size.height() / content.height());
            width = content.width() * scale;
            height = content.height() * scale;
        }
        ImageFit::None => {}
    }

    Rect::new(
        Point::new(
            bounds.origin.x + alignment_offset(bounds.size.width(), width, horizontal),
            bounds.origin.y + alignment_offset(bounds.size.height(), height, vertical),
        ),
        size(width, height),
    )
}

pub(crate) fn image_size(image: &RasterImage) -> Size {
    size(u32_to_f32(image.width()), u32_to_f32(image.height()))
}

fn alignment_offset(available: f32, content: f32, alignment: Alignment) -> f32 {
    match alignment {
        Alignment::Start => 0.0,
        Alignment::Center => (available - content) / 2.0,
        Alignment::End => available - content,
    }
}

fn rect_contains(outer: Rect, inner: Rect) -> bool {
    inner.origin.x >= outer.origin.x
        && inner.origin.y >= outer.origin.y
        && inner.origin.x + inner.size.width() <= outer.origin.x + outer.size.width()
        && inner.origin.y + inner.size.height() <= outer.origin.y + outer.size.height()
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

    fn solid(width: u32, height: u32) -> RasterImage {
        let pixels = vec![255; usize::try_from(width * height * 4).expect("small test image")];
        RasterImage::new(width, height, pixels).expect("valid RGBA image")
    }

    #[test]
    fn uses_intrinsic_dimensions_and_records_an_image_draw() {
        let image = solid(2, 1);
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

    #[test]
    fn contain_keeps_the_aspect_ratio_and_centers_the_image() {
        let image = solid(2, 1);
        let mut runtime = UiRuntime::new(Image::new(image).with_fit(ImageFit::Contain));
        let mut list = DisplayList::new();

        runtime
            .layout(Constraints::tight(size(10.0, 10.0)).expect("valid constraints"))
            .expect("image layout succeeds");
        runtime
            .paint(&mut PaintContext::new(&mut list))
            .expect("image paint succeeds");

        let [DisplayCommand::DrawImage { destination, .. }] = list.commands() else {
            panic!("expected one image draw");
        };
        assert_eq!(destination.size, size(10.0, 5.0));
        assert!((destination.origin.y - 2.5).abs() <= f32::EPSILON);
    }

    #[test]
    fn cover_clips_an_image_that_overflows_the_bounds() {
        let image = solid(4, 1);
        let mut runtime = UiRuntime::new(Image::new(image).with_fit(ImageFit::Cover));
        let mut list = DisplayList::new();

        runtime
            .layout(Constraints::tight(size(10.0, 10.0)).expect("valid constraints"))
            .expect("image layout succeeds");
        runtime
            .paint(&mut PaintContext::new(&mut list))
            .expect("image paint succeeds");

        assert!(matches!(
            list.commands(),
            [
                DisplayCommand::Save,
                DisplayCommand::PushClip { .. },
                DisplayCommand::DrawImage { destination, .. },
                DisplayCommand::Restore,
            ] if destination.size == size(40.0, 10.0)
        ));
    }

    #[test]
    fn scale_down_does_not_enlarge_small_images() {
        let image = solid(2, 2);
        let mut runtime = UiRuntime::new(Image::new(image).with_fit(ImageFit::ScaleDown));
        let mut list = DisplayList::new();

        runtime
            .layout(Constraints::tight(size(20.0, 20.0)).expect("valid constraints"))
            .expect("image layout succeeds");
        runtime
            .paint(&mut PaintContext::new(&mut list))
            .expect("image paint succeeds");

        let [DisplayCommand::DrawImage { destination, .. }] = list.commands() else {
            panic!("expected one image draw");
        };
        assert_eq!(destination.size, size(2.0, 2.0));
        assert_eq!(destination.origin, Point::new(9.0, 9.0));
    }
}
