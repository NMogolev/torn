use torn_core::{Constraints, Rect, Size};
use torn_render::{PaintContext, TextureId};
use torn_ui::{LayoutContext, LayoutResult, UiEnvironment, Widget};

/// A widget that paints a browser texture at a fixed logical size.
///
/// The texture is allocated and synchronized by
/// [`crate::BrowserSurface`]; this widget only records a display-list draw for
/// [`TextureId`]. The application must adapt [`Self::size`] to the environment's
/// scale factor and resize the browser surface accordingly.
pub struct BrowserView {
    texture: TextureId,
    size: Size,
}

impl BrowserView {
    /// Creates a view that paints `texture` at `size` logical pixels.
    #[must_use]
    pub const fn new(texture: TextureId, size: Size) -> Self {
        Self { texture, size }
    }

    /// Returns the texture drawn by this view.
    #[must_use]
    pub const fn texture(&self) -> TextureId {
        self.texture
    }

    /// Replaces the texture drawn by this view.
    ///
    /// Use this after a browser surface was recreated with a new identifier.
    pub const fn set_texture(&mut self, texture: TextureId) {
        self.texture = texture;
    }

    /// Returns the view's logical display size.
    #[must_use]
    pub const fn size(&self) -> Size {
        self.size
    }

    /// Replaces the view's logical display size.
    ///
    /// The browser surface must be resized to the matching physical extent.
    pub const fn set_size(&mut self, size: Size) {
        self.size = size;
    }
}

impl Widget for BrowserView {
    fn layout(&mut self, _: &mut LayoutContext<'_>, constraints: Constraints) -> LayoutResult {
        LayoutResult::new(constraints.constrain(self.size))
    }

    fn paint(&self, context: &mut PaintContext<'_>, _: &UiEnvironment, bounds: Rect) {
        context.draw_texture(self.texture, bounds);
    }
}

#[cfg(test)]
mod tests {
    use torn_core::{Point, Size};
    use torn_render::{DisplayCommand, DisplayList, TextureId};
    use torn_ui::UiRuntime;

    use super::BrowserView;

    #[test]
    fn lays_out_and_records_a_texture_draw() {
        let texture = TextureId::new(7);
        let mut runtime = UiRuntime::new(BrowserView::new(
            texture,
            Size::new(320.0, 200.0).expect("valid size"),
        ));
        let mut list = DisplayList::new();

        let layout = runtime
            .layout(torn_core::Constraints::UNBOUNDED)
            .expect("browser layout succeeds");
        let size = layout.size();
        runtime
            .paint(&mut torn_render::PaintContext::new(&mut list))
            .expect("browser paint succeeds");

        assert_eq!(size, Size::new(320.0, 200.0).expect("valid size"));
        assert!(matches!(
            list.commands(),
            [DisplayCommand::DrawTexture {
                texture: recorded,
                destination,
            }] if recorded == &texture && destination.origin == Point::ZERO
        ));
    }
}
